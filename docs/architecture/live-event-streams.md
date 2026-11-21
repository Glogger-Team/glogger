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

```text
Player.log                                  Chat.log (Chat-YY-MM-DD.log)
    |                                            |
    v                                            v
PlayerLogWatcher::poll()                    ChatLogWatcher::poll()
    |                                            |
    +-- process_player_events()                  +-- parse_chat_lines()
    |   (CharacterLogin/ChatLogPath/             |       |
    |    AreaTransition only)                    |       v
    |                                            |   Vec<LogEvent::ChatMessage>
    v                                            v
DataIngestCoordinator                       DataIngestCoordinator
  ::process_player_events()                   ::process_chat_events()
    |                                            |
    +-- area-transition emit                     +-- parse_status_message()
    +-- game-state-updated (area)                |       |
                                                |       v
                                                |   Option<ChatStatusEvent>
                                                |       |
                                                +-- emit "chat-status-event"
                                                +-- insert_chat_messages()
                                                +-- evaluate_rules()
                                                +-- emit "watch-rule-triggered"
                                                +-- emit "chat-messages-inserted"
                                                +-- GameStateManager
                                                |     .correct_stack_for_session()
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

This ordering means a Player.log login/area line always lands before the corresponding chat messages for the same game action. Features that correlate across streams can rely on this.

## Event Streams Reference

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

**Source:** chat dispatch + `GameStateManager` (active character/session) — see [`game-state.md`](game-state.md)

Not a raw event stream — this is a **domain notification**. When chat dispatch persists changes to the database, it accumulates which domains were updated and emits them as a single array (e.g., `["skills", "inventory"]`). Frontend stores use this to know when to refresh their data from the DB.

Full details: [`game-state.md`](game-state.md)

### Stream: `"survey-tracker-*"`

**Source:** [`SurveySessionAggregator`](../../src-tauri/src/survey/aggregator.rs) driven by chat loot gains (`attribute_chat_gain`) and manual sessions.

Downstream consumer of chat loot gains. The aggregator is **stateful** — it tracks survey sessions, stitches survey attribution, and manages open multihit nodes (persisted to DB).

Events emitted: `survey-tracker-session-started`, `survey-tracker-session-ended`, `survey-tracker-use-recorded`, `survey-tracker-use-completed`, `survey-tracker-multihit-opened`, `survey-tracker-multihit-closed`.

Full details: [`economics-surveying.md`](../features/screens/economics/economics-surveying.md)

### Other signals

| Event | Source | Purpose |
|---|---|---|
| `"character-login"` | Both logs | Character name detected |
| `"server-detected"` | Chat.log login line | Server name + timezone offset |
| `"area-transition"` | Player.log `LOADING LEVEL` | Area change |
| `"chat-messages-inserted"` | Chat.log | New messages persisted to DB |
| `"watch-rule-triggered"` | Chat.log | User-defined watch rule matched |
| `"coordinator-status"` | Coordinator | Watcher started/stopped |

## Subscribing to Events (Frontend)

All events use Tauri's event system. `ChatStatusEvent` uses `#[serde(tag = "kind")]`, so its payload includes a `kind` field for discriminating variants; `game-state-updated` carries a plain string array of changed domain names.

```typescript
import { listen } from '@tauri-apps/api/event'
import type { UnlistenFn } from '@tauri-apps/api/event'

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
| Item quantities for new stacks | `"chat-status-event"` (`ItemGained`) | Chat Status has the actual stack size |
| Per-action XP deltas | `"chat-status-event"` (`XpGained`) | Only available in Status channel |
| Coin/council economy | `"chat-status-event"` (`CoinsLooted`, `CouncilsChanged`) | Only in Status channel |
| Survey triangulation | `"chat-status-event"` (`TreasureDistance`) | Only in Status channel |
| Level-up notifications | `"chat-status-event"` (`LevelUp`) | Only in Status channel |
| Refreshing a view after DB changes | `"game-state-updated"` | Signals which domains changed |

### Combining streams

Some data appears in both streams with different levels of detail. The pattern is: **Player.log is primary** (identity, structure), **Chat Status is supplementary** (quantities, deltas, human-readable names). Features don't deduplicate — they use different fields from each.

#### Backend cross-stream correlation

The coordinator performs **automatic inventory stack correction** by correlating chat status events with Player.log data:

1. Player.log `ProcessAddItem` records items with `stack_size = 1` in `game_state_inventory` / `game_state_storage`
2. Chat.log `[Status] Item x5 added to inventory.` provides the real quantity
3. The coordinator calls `GameStateManager::correct_stack_for_session()` with the gain's session identity (tag-preferred, active fallback) which resolves the chat display name to a Player.log internal name via CDN data, finds recent `stack_size=1` rows, and updates them
4. A `"game-state-updated"` event is emitted so the frontend refreshes

Both sources also write to the `item_transactions` ledger table (v15 migration) — Player.log events with `source = 'player_log'` and chat events with `source = 'chat_status'`.

#### Frontend cross-stream example

```typescript
// Example: track items with correct quantities
listen<string[]>('game-state-updated', async (event) => {
  if (event.payload.includes('inventory')) {
    // Reload item rows from the DB — quantities already corrected
    await refreshInventory()
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

### For LogEvent persistence

`process_player_events` handles only `CharacterLogin`, `ChatLogPath`, and
`AreaTransition` (coordinator.rs:818-920); there is no generic `PlayerEvent`
dispatch arm. Player.log is not a persistence surface for new features — add
chat-status or report-based persistence instead.

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
| Player.log | `[HH:MM:SS]` (already UTC, no date) | Dated by [`to_utc_datetime_with_base`](../../src-tauri/src/parsers.rs) in [`parsers.rs`](../../src-tauri/src/parsers.rs) — an explicit base date for replay / solo reparse, else the session date from the log's own login line ([`parse_player_log_login_line`](../../src-tauri/src/parsers.rs)), else the clock with a one-day rollback when the line's time of day is ahead of it |
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

```text
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
