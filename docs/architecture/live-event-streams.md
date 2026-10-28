# Live Event Streams

This document describes how live game data flows from log files through parsers to features. It is the single reference for any feature that consumes real-time game events — whether from Player.log, Chat.log, or both.

## Overview

The game produces two log files with complementary data about the same player actions:

| Log File | Format | Timezone | What it contains |
|---|---|---|---|
| **Player.log** | `[HH:MM:SS] LocalPlayer: ProcessXxx(...)` | Local time, no date | Structured game engine events — items, skills, NPCs, vendors, storage, combat state, UI actions |
| **Chat.log** | `YY-MM-DD HH:MM:SS\t[Channel] Sender: Message` | UTC | Chat messages across channels — including `[Status]` which carries item quantities, XP deltas, economy events, and more |

Each log has its own watcher and parser. The **DataIngestCoordinator** polls both, routes events through their respective parsers, and emits structured events to the frontend. Features subscribe to whichever events they need.

## Data Flow

```
Player.log                                  Chat.log (Chat-YY-MM-DD.log)
    |                                            |
    v                                            v
PlayerLogWatcher::poll()                    ChatLogWatcher::poll()
    |                                            |
    +-- PlayerEventParser::process_line()         +-- parse_chat_lines()
    |       |                                    |       |
    |       v                                    |       v
    |   Vec<PlayerEvent>                         |   Vec<LogEvent::ChatMessage>
    |                                            |
    v                                            v
DataIngestCoordinator                       DataIngestCoordinator
  ::process_player_events()                   ::process_chat_events()
    |                                            |
    +-- GameStateManager                         +-- parse_status_message()
    |     .process_event()                       |       |
    |       |                                    |       v
    |       +-- accumulate domains               |   Option<ChatStatusEvent>
    |                                            |       |
    +-- SurveySessionAggregator                  |       +-- emit "chat-status-event"
    |     .handle_event()                        |
    |       |                                    +-- insert_chat_messages()
    |       +-- stitches SurveyMapUse → Mining   |     (excluded_channels applied here)
    |       +-- emit survey-tracker-* events     |
    |                                            +-- evaluate_rules()
    +-- accumulate PlayerEvents                        |
    +-- flush batches ──┐                              +-- emit "watch-rule-triggered"
    |                   +-- emit "player-events-batch" +-- emit "chat-messages-inserted"
    |                   +-- emit "game-state-updated"
    +-- emit "skill-update"                    +-- GameStateManager
    +-- emit "character-login"                 |     .correct_stack_from_chat()
    +-- emit "area-transition"                 |     (fixes stack_size=1 rows)
                                               |     +-- emit "game-state-updated"
                                               +-- item_transactions ledger
                                                     (records chat-sourced gains)
```

### Poll cycle ordering

Both watchers are polled on the same coordinator tick (every 500ms — `spawn_polling_thread(coordinator, Duration::from_millis(500))`, `lib.rs`). Within a single tick:

1. **Player events processed first** — `process_player_events()`
2. **Chat events processed second** — `process_chat_events()`
3. **Game state switches to live mode** after the first poll (catch-up replay complete) — i.e. after both drains: the chat login marker's `set_active_character` clears transient state when live, and that must not land on rows the replay just built
4. **Watcher positions saved** to database

This ordering means a `PlayerEvent::ItemAdded` always arrives before the corresponding `ChatStatusEvent::ItemGained` for the same game action. Features that correlate across streams can rely on this.

### Batching strategy

Tauri's `emit()` sends events through the Windows webview via `PostMessage`. Each call is one message in the OS message queue, which has a finite capacity (~10,000). During startup catch-up, the coordinator can process thousands of `PlayerEvent`s in a single poll tick — emitting each one individually would overflow the queue and produce `PostMessage failed; Error code 0x80070718` errors.

To prevent this, high-volume event types are **batched** before emission:

| Constant | Value | Purpose |
|---|---|---|
| `BATCH_MAX_SIZE` | 50 | Flush when the batch reaches this many events |
| `BATCH_MAX_AGE` | 20ms | Flush when this much time has passed since the batch started |

The coordinator accumulates `PlayerEvent`s and domain update strings as it processes `LogEvent::PlayerEventParsed` variants. When either threshold is hit, it flushes:
- `"player-events-batch"` — `Vec<PlayerEvent>` (1–50 events per emission)
- `"game-state-updated"` — deduplicated `Vec<&str>` of domain names

Low-volume events (`character-login`, `area-transition`, `skill-update`, `survey-*`) are still emitted individually since they don't contribute meaningfully to queue pressure.

A `character-login` event forces an immediate flush of any pending batch, ensuring the frontend processes all events for the previous character before the identity changes.

Chat events don't need this treatment — `ChatMessage`s are already bulk-inserted into the DB with a single `chat-messages-inserted` count emit, and the per-message `chat-status-event` / `survey-loot-correction` emits are low-volume (only Status channel messages generate them).

## Event Streams Reference

### Stream: `"player-events-batch"`

**Source:** Player.log via [`PlayerEventParser`](../../src-tauri/src/player_event_parser.rs)

Raw structured events from the game engine, **emitted in batches**. Every `ProcessXxx(...)` line becomes a typed `PlayerEvent`. The parser is **stateful** — it tracks item instances, stack sizes, and interaction context to resolve identities and compute deltas.

The backend accumulates `PlayerEvent`s and flushes them as a `Vec<PlayerEvent>` array when the batch reaches **50 events** or **20ms** has elapsed — whichever comes first. This reduces the number of IPC messages through the Windows webview PostMessage layer, preventing message queue overflow during large catch-up replays. See [Batching Strategy](#batching-strategy) for details.

Full event catalog: [`player-event-parser.md`](player-event-parser.md)

Key event categories:
- **Items:** `ItemAdded`, `ItemStackChanged`, `ItemDeleted` (with context: storage transfer, vendor sale, unknown)
- **Skills:** `SkillsLoaded`
- **NPCs:** `InteractionStarted`, `InteractionEnded`, `FavorChanged`
- **Vendors:** `VendorSold`, `VendorStackUpdated`, `VendorGoldChanged`
- **Storage:** `StorageDeposit`, `StorageWithdrawal`
- **Actions:** `DelayLoopStarted`, `ScreenText`, `BookOpened`
- **State:** `ActiveSkillsChanged`, `MountStateChanged`, `WeatherChanged`, `CombatStateChanged`
- **Recipes:** `RecipeUpdated`
- **Login snapshots:** `AbilitiesLoaded`, `RecipesLoaded`, `EquipmentChanged`, `AttributesChanged`
- **Effects:** `EffectsAdded`, `EffectsRemoved`, `EffectNameUpdated`

### Stream: `"chat-status-event"`

**Source:** Chat.log `[Status]` channel via [`ChatStatusParser`](../../src-tauri/src/chat_status_parser.rs)

Structured events parsed from `[Status]` channel messages. The parser is **stateless** — each message maps to 0 or 1 events. Accumulation and correlation are left to subscribing features.

| Event | Status Message Pattern | Key Fields |
|---|---|---|
| `ItemGained` | `X added to inventory.` / `X xN added to inventory.` | `item_name`, `quantity` |
| `XpGained` | `You earned N XP in Skill.` | `skill`, `amount` |
| `LevelUp` | `You earned N XP and reached level L in Skill!` | `skill`, `level`, `xp` |
| `CoinsLooted` | `You searched the corpse and found N coins.` | `amount` |
| `CouncilsChanged` | `You received N Councils.` / `You used N councils.` | `amount` (negative for spending) |
| `TreasureDistance` | `The treasure is N meters from here.` | `meters` |
| `AnatomyResult` | `You bury the corpse.` / `You botch the autopsy!` | `success` (bool) |
| `Summoned` | `Summoned X xN` | `item_name`, `quantity` |
| `ItemStudied` | `You carefully study the X.` | `item_name` |
| `ReportSaved` | `Saved report to <path>` | `file_path` |

### Stream: `"report-saved"`

**Source:** Chat Status channel via `chat_status_parser.rs` → coordinator

Emitted when the game writes a character or inventory report to disk (triggered by Export Character as JSON or Export Storage as JSON in Settings → VIP). The coordinator re-emits this as a dedicated `"report-saved"` event (payload: file path string) so the frontend can trigger an immediate report import instead of waiting for the next polling cycle.

### Stream: `"game-state-updated"`

**Source:** Player.log via [`GameStateManager`](../../src-tauri/src/game_state.rs)

Not a raw event stream — this is a **domain notification**. When `GameStateManager` processes a `PlayerEvent` and persists changes to the database, it accumulates which domains were updated. At batch flush time, the coordinator deduplicates the domain list and emits it as a single array (e.g., `["skills", "inventory"]`). Frontend stores use this to know when to refresh their data from the DB.

Full details: [`game-state.md`](game-state.md)

### Stream: `"survey-tracker-*"`

**Source:** [`SurveySessionAggregator`](../../src-tauri/src/survey/aggregator.rs) consuming `PlayerEvent`s in the coordinator.

Downstream consumer of `PlayerEvent`s. The aggregator is **stateful** — it tracks survey sessions, stitches `SurveyMapUse` → `Mining` attribution, and manages open multihit nodes (persisted to DB).

Events emitted: `survey-tracker-session-started`, `survey-tracker-session-ended`, `survey-tracker-use-recorded`, `survey-tracker-use-completed`, `survey-tracker-multihit-opened`, `survey-tracker-multihit-closed`.

Full details: [`economics-surveying.md`](../features/screens/economics/economics-surveying.md)

### Other signals

| Event | Source | Purpose |
|---|---|---|
| `"skill-update"` | Player.log `ProcessUpdateSkill` | Legacy per-skill XP/level update (predates PlayerEventParser) |
| `"character-login"` | Both logs | Character name detected |
| `"server-detected"` | Chat.log login line | Server name + timezone offset |
| `"area-transition"` | Player.log `LOADING LEVEL` | Area change |
| `"chat-messages-inserted"` | Chat.log | New messages persisted to DB |
| `"watch-rule-triggered"` | Chat.log | User-defined watch rule matched |
| `"coordinator-status"` | Coordinator | Watcher started/stopped |

## Subscribing to Events (Frontend)

All events use Tauri's event system. Both `PlayerEvent` and `ChatStatusEvent` use `#[serde(tag = "kind")]`, so every payload includes a `kind` field for discriminating variants.

```typescript
import { listen } from '@tauri-apps/api/event'
import type { UnlistenFn } from '@tauri-apps/api/event'

// Listen to player events (arrives as batches of 1-50 events)
const unlistenPlayer: UnlistenFn = await listen<PlayerEvent[]>('player-events-batch', (event) => {
  for (const e of event.payload) {
    switch (e.kind) {
      case 'ItemAdded':
        console.log(`Item: ${e.item_name} (new: ${e.is_new})`)
        break
      case 'FavorChanged':
        console.log(`Favor: ${e.npc_name} ${e.delta > 0 ? '+' : ''}${e.delta}`)
        break
    }
  }
})

// Listen to chat status events
const unlistenStatus: UnlistenFn = await listen<ChatStatusEvent>('chat-status-event', (event) => {
  const e = event.payload
  switch (e.kind) {
    case 'ItemGained':
      console.log(`Got ${e.quantity}x ${e.item_name}`)
      break
    case 'XpGained':
      console.log(`+${e.amount} XP in ${e.skill}`)
      break
  }
})

// Listen to domain refresh notifications
const unlistenState: UnlistenFn = await listen<string[]>('game-state-updated', (event) => {
  const domains = event.payload
  if (domains.includes('inventory')) {
    // Reload inventory from DB
    await refreshInventory()
  }
})
```

### Choosing which stream to use

| You need... | Subscribe to... | Why |
|---|---|---|
| Item identity, instance IDs, slot positions | `"player-events-batch"` (`ItemAdded`, `ItemDeleted`) | Player.log has the game engine data |
| Item quantities for new stacks | `"chat-status-event"` (`ItemGained`) | Chat Status has the actual stack size |
| Cumulative skill XP/level | `"player-events-batch"` (`SkillsLoaded`) or `"game-state-updated"` | Player.log has the full snapshot |
| Per-action XP deltas | `"chat-status-event"` (`XpGained`) | Only available in Status channel |
| Vendor transactions | `"player-events-batch"` (`VendorSold`, `VendorGoldChanged`) | Only in Player.log |
| Coin/council economy | `"chat-status-event"` (`CoinsLooted`, `CouncilsChanged`) | Only in Status channel |
| Storage vault changes | `"player-events-batch"` (`StorageDeposit`, `StorageWithdrawal`) | Player.log has vault keys |
| NPC favor changes | `"player-events-batch"` (`FavorChanged`) | Player.log has numeric delta |
| Survey triangulation | `"chat-status-event"` (`TreasureDistance`) | Only in Status channel |
| Level-up notifications | `"chat-status-event"` (`LevelUp`) | Only in Status channel |
| Refreshing a view after DB changes | `"game-state-updated"` | Signals which domains changed |

### Combining streams

Some data appears in both streams with different levels of detail. The pattern is: **Player.log is primary** (identity, structure), **Chat Status is supplementary** (quantities, deltas, human-readable names). Features don't deduplicate — they use different fields from each.

#### Backend cross-stream correlation

The coordinator performs **automatic inventory stack correction** by correlating chat status events with Player.log data:

1. Player.log `ProcessAddItem` records items with `stack_size = 1` in `game_state_inventory` / `game_state_storage`
2. Chat.log `[Status] Item x5 added to inventory.` provides the real quantity
3. The coordinator calls `GameStateManager::correct_stack_from_chat()` which resolves the chat display name to a Player.log internal name via CDN data, finds recent `stack_size=1` rows, and updates them
4. A `"game-state-updated"` event is emitted so the frontend refreshes

Both sources also write to the `item_transactions` ledger table (v15 migration) — Player.log events with `source = 'player_log'` and chat events with `source = 'chat_status'`.

#### Frontend cross-stream example

```typescript
// Example: track items with correct quantities
listen<PlayerEvent[]>('player-events-batch', (event) => {
  for (const e of event.payload) {
    if (e.kind === 'ItemAdded' && e.is_new) {
      // Record the item with instance ID — quantity starts at 1
      addItem(e.instance_id, e.item_name, 1)
    }
  }
})

listen<ChatStatusEvent>('chat-status-event', (event) => {
  if (event.payload.kind === 'ItemGained' && event.payload.quantity > 1) {
    // Correct the quantity using the name-matched item
    updateItemQuantity(event.payload.item_name, event.payload.quantity)
  }
})
```

## Adding Persistence (Rust-side)

When a feature needs to persist events to the database, add a handler in the coordinator. Follow the existing patterns:

### For PlayerEvent persistence

Follow the `GameStateManager` or `SurveySessionTracker` pattern:

1. Create a persistence module (e.g., `src-tauri/src/my_feature_persistence.rs`)
2. Add a tracker struct to `DataIngestCoordinator`
3. Match on specific `PlayerEvent` variants in `process_player_events`

```rust
// In coordinator.rs, inside the PlayerEventParsed match arm:
LogEvent::PlayerEventParsed(player_event) => {
    self.my_tracker.process_event(&player_event, &self.db_pool);
    // ...existing game_state processing, batch accumulation...
}
```

### For ChatStatusEvent persistence

Same pattern, in the chat side:

```rust
// In coordinator.rs, inside the ChatMessage match arm:
LogEvent::ChatMessage(msg) => {
    if let Some(status_event) = parse_status_message(&msg) {
        self.economy_tracker.process_event(&status_event, &self.db_pool);
        self.app_handle.emit("chat-status-event", &status_event).ok();
    }
    messages.push(msg);
}
```

## Timestamps

All timestamps are **UTC internally**. The frontend converts to local time for display.

| Source | Raw Format | How it becomes UTC |
|---|---|---|
| Player.log | `[HH:MM:SS]` (already UTC, no date) | Dated by [`parsers::LogDateResolver`](../../src-tauri/src/parsers.rs) in [`parsers.rs`](../../src-tauri/src/parsers.rs) — an explicit base date for replay / solo reparse, else the session date from the log's own login line (`PlayerEvent::LogSessionAnchored`), else the clock with a one-day rollback when the line's time of day is ahead of it |
| Chat.log | `YY-MM-DD HH:MM:SS` (player's local time) | Converted to UTC via `chat_local_to_utc()` in [`parsers.rs`](../../src-tauri/src/parsers.rs) using the detected timezone offset |
| Timezone offset | `Timezone Offset -07:00:00` in chat login line | Parsed by `parse_timezone_offset()` in [`chat_parser.rs`](../../src-tauri/src/chat_parser.rs), stored in settings, used for Chat.log timestamp conversion |

### Frontend display

Use the shared [`useTimestamp.ts`](../../src/composables/useTimestamp.ts) composable to convert UTC timestamps to local time:

```typescript
import { formatTimeShort, formatDateTimeShort } from '@/composables/useTimestamp'

// "3:12 PM"
formatTimeShort('2026-03-27 22:12:48')

// "Mar 27, 3:12 PM"
formatDateTimeShort('2026-03-27 22:12:48')
```

## Extending the Parsers

### Adding a new PlayerEvent variant

1. Add a variant to `PlayerEvent` in [`player_event_parser.rs`](../../src-tauri/src/player_event_parser.rs)
2. Add a `parse_xxx` method on `PlayerEventParser`
3. Add a dispatch branch in `process_line` (check for `"ProcessXxx("`)
4. Add tests
5. Update [`player-event-parser.md`](player-event-parser.md)

### Adding a new ChatStatusEvent variant

1. Add a variant to `ChatStatusEvent` in [`chat_status_parser.rs`](../../src-tauri/src/chat_status_parser.rs)
2. Add a `try_xxx` function with string matching logic
3. Chain it into `parse_status_message()` via `.or_else()`
4. Add tests using the `status_msg()` helper
5. Update this doc's event table

Both parsers use manual string operations (`starts_with`, `ends_with`, `find`, `parse`) — no regex. Follow existing parse functions as templates.

### Known Status patterns not yet parsed

Add these as features need them:

| Pattern | Proposed Variant |
|---|---|
| `X collected! Also found Y xN (speed bonus!)` | `SurveyCollected` |
| `X was distilled into Y xN` | `Distilled` |
| `X was decomposed into Y xN and Z xN` | `Decomposed` |
| `Your friend X is now online/offline` | `FriendPresence` |
| `X joined/left the hunting party` | `PartyChange` |
| `Your actions have caused you to gain Favor with NPC!` | `FavorGained` |
| `You lost N.N favor with NPC` | `FavorLost` |
| `Ratkin Mugger stole N Councils!` | `CouncilsStolen` |
| `Stowed N items across N storages` | `BulkStowed` |

### Future: `[Combat]` channel parser

The `[Combat]` channel contains damage, healing, and kill events. A future `CombatParser` would follow the same pattern as `ChatStatusParser` — a new module that receives `ChatMessage`s where `channel == "Combat"`, parses them into `CombatEvent` variants, and emits them via the coordinator. The `excluded_channels` setting would not gate this parsing, same as with Status.

## Channel Exclusion

The `excluded_channels` setting in `AppSettings` controls what the **chat log feature** persists to the `chat_messages` database table. It does **not** affect parsing or event routing.

```
Chat.log line arrives
    |
    v
ChatLogWatcher parses ALL channels (no exclusions)
    |
    v
Coordinator receives LogEvent::ChatMessage
    |
    +-- ChatStatusParser sees it (if Status channel)     <-- always runs
    +-- Watch rules evaluate it                          <-- always runs
    +-- insert_chat_messages() filters by excluded_channels  <-- only place exclusions apply
```

This means features like the ChatStatusParser and future CombatParser always receive their data, even if the user has excluded those channels from their chat log display.

## Testing

```bash
cd src-tauri

# All tests
cargo test

# Player event parser tests
cargo test player_event_parser

# Chat status parser tests
cargo test chat_status_parser

# Survey tracker tests (aggregator, persistence, commands)
cargo test --lib survey::

# Chat parser tests (line parsing, login detection)
cargo test chat_parser

# Log watcher tests (watcher integration)
cargo test log_watchers
```
