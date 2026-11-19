# Glogger Pipeline Structure

> **Historical note (2026-11):** this doc is early design musing, not current architecture. The Player.log item parser (`PlayerEventParser`, `feed_chat_gain`, `player-events-batch`) was removed — Player.log now yields session signals only (login, chat-log path, area). Chat.log parsing and report snapshot imports are the live producers; see `live-event-streams.md` and `game-state.md`.

## CDN Data

Does this need to be complicated? I feel like this is all we need, structurally. Pretty sure this is basically what we have

```text
CDN JSONs
  |
  V
CDN Ingester
    |
    V
  CDN Table Builders
    |
    V
CDN SQLite Database
    ^
    |
  CDN-related Query requests
```

> **Claude's take:** Yeah, this is basically what you have and it's fine. The CDN pipeline is clean and self-contained: download JSONs on startup, deserialize into the in-memory `GameData` struct (with raw JSON preservation so nothing is lost), build indices, and persist select tables for query support. The one interesting design choice already baked in is the "raw-JSON-first" pattern — deserialize as `HashMap<String, Value>`, extract typed fields, keep the original `Value`. That's a good call since CDN schema changes won't break you. The only thing missing from this diagram is the in-memory index layer (`item_name_index`, `recipes_by_skill`, etc.) that sits between the DB and query requests, but structurally the flow is right.

## Player Data

- events triggered off ingestion
- gamestate comprised of 4 buckets: server-scoped data, account-scoped data, character-scoped data, session-scoped data
  - server-scoped: stuff that transcends a single session, but isn't tied to the account. garden growing seasons are an example of this.
  - account-scoped: things that are account wide (duh) like transfer chest storage contents, and information about what characters are on the account. maybe other stuff?
  - character-scoped: stuff specific to the character. the majority of our data. skill levels, xp, most inventories, etc.
  - session-scoped: stuff that is agnostic to the player, but we're detecting via logs like current zone, current weather, etc
- sources of data:
  - player.log
  - chat logs
  - character jsons
  - inventory jsons
  - saved reports

- need to ingest and deconflict data
  - know timestamps of data to know when to overwrite
  - know what triggers resetting data (ex: even though the character json doesn't give us favor numbers, we can at least know to reset the progress towards the next rank if we detect a rank increase via the json. inversely, if we don't detect a rank increase, we know not to touch it.)
  - sometimes getting accurate data is best served via correlating across sources, _especially_ when it comes to our player and chat logs
  - different sources will use different timestamps. the very first thing we do for every piece of data is converting to utc.
  - handle aligning times approprirately when different sources may have different ganularity.
  - be performant and responsive even when we're getting hit with tons of log messages every second
  - resuming previous sessions, etc. even though files may have changed, been replaced by newer versions, date rollovers, etc.
  - probably rely as little on system time as we can? the files have timestamps and those will help us the most probably?
- features should be able to consume events to update the gamestate.
  - this could be single line update triggers, or starting/closing event collection windows, etc. survey tracker is a good example of this kind of thing.
  - features should try their damnedest to use rust to manage their interactions with the gamestate and db writes/reads. this is just more performant, I assume, and lets us build more unified ways of doing this. also i assume. maybe i'm wrong.
  - does this mean we have a handle of pinia stores that manage all the coodination between the rust layer and the frontend layer? does that make sense? could certainly help standardize how we interact with the gamestate.
  - typescript/vue should be used to leverage how flexible, modular, sharable, and easy its interface building is.
- how do we preserve historical? gamestate should always reflect the "what is now" - we will want was to see "what was" for some features. like a set of historical tables.
- how do we not overwhelm the windows message queue? how do we handle playback of existing log data? is this method testable and scalable? how future proof is it?

> **Claude's thoughts on the above:**
>
> **The 4-bucket scoping is solid and already implemented.** Looking at what's actually built, the scoping works like this in practice:
>
> - **Server-scoped:** `market_values`, vendor gold tracking, survey data — keyed by `(server_name)`. Garden seasons would fit here too.
> - **Account-scoped:** Currently implicit — everything lives in one SQLite file per user. There's no explicit multi-account table structure yet, but the single-DB-per-user approach means account scope is "the whole database." Transfer chest contents would be interesting here — right now storage is character-scoped via `(character_name, server_name)` composite keys, so if transfer chests are truly account-wide, that's a gap.
> - **Character-scoped:** This is the bulk of it. All `game_state_*` tables use `(character_name, server_name)` composite PKs. Skills, inventory, equipment, favor, recipes, effects, attributes, currencies — all character-scoped.
> - **Session-scoped:** In-memory only on the frontend (`sessionSkills`, `liveItemMap`, `liveEventLog` in gameStateStore). Resets on login. Weather/moon/combat state are technically persisted in singleton DB tables but get cleared/refreshed each session.
>
> **On deconflicting data across sources:** The `last_confirmed_at` timestamp on game state rows is the right approach and it's what we have. Snapshot imports timestamp their writes, log events timestamp theirs, and "most recent wins" is the conflict resolution. The chat-to-player-log correlation is the most complex piece — `feed_chat_gain()` on the PlayerEventParser seeds stack sizes by matching chat `"Item x5 added to inventory"` messages within a ±2 second window of the corresponding `ProcessUpdateItemCount` line. That's clever but fragile — if timing drifts or the game changes its message format, this breaks silently.
>
> **On timestamps:** You're right to distrust system time. The watchers poll every 1.5 seconds and the parsed timestamps come from the log files themselves (`[HH:MM:SS]` for Player.log, `YY-MM-DD HH:MM:SS` for chat). The Player.log timestamps don't include dates though, so date rollover detection (midnight crossing) is something the watcher has to handle. System time really only matters for "when did we last poll" and even that is just a scheduling concern.
>
> **On Rust owning state:** You're not wrong. The current architecture already does this well. Rust owns the lifecycle: ingestion → parsing → persistence. Vue/Pinia stores are thin read layers that invoke queries and listen for events. The Pinia stores (`gameStateStore`, `coordinatorStore`, `craftingStore`, etc.) do act as the coordination layer between Rust and the frontend — they listen for `game-state-updated` and `player-events-batch` events and refresh their local state from Tauri commands. This pattern works. The main thing to watch is that stores don't accumulate business logic that should live in Rust — they should stay as dumb query+display adapters.
>
> **On historical data:** This is a real gap. Right now the game state tables are "last known value" only — they get overwritten. The `item_transactions` table is the one exception: it's an append-only audit trail of every item event with full provenance context. If you want "what was my inventory at 3pm yesterday" or "how did my skill XP change over time," you'd need similar audit/history tables for other domains. The `character_snapshots` table preserves point-in-time snapshots from JSON imports, but those are manual and infrequent. A general pattern of "current state table + history/audit table" per domain would solve this, but it's a lot of write amplification for domains where history doesn't matter.
>
> **On not overwhelming the message queue:** The 1.5s polling interval is the main throttle. Each poll cycle processes a batch of lines, produces a `Vec<PlayerEvent>`, and emits a single `player-events-batch` Tauri event to the frontend. That batching is key — you're not firing an event per log line. For playback of existing log data (catching up on a large log file), the same batch processing applies, it just runs more batches. The risk would be if a huge log file produces thousands of events in rapid succession — the frontend stores would be hammering reactive updates. If that becomes a problem, a debounce/coalesce layer on the frontend side (e.g., "only refresh the UI at most every 200ms during catch-up") would help without changing the backend architecture.

```text
DataIngestCoordinator
    ├── PlayerLogWatcher (1.5s poll) ──→ session signals (login/path/area) ──→ GameStateManager ──→ SQLite
    ├── ChatLogWatcher (1.5s poll) ──→ ChatStatusParser ──→ item_transactions/skills
    ├── CharacterJSON import ──→ seed_game_state_from_snapshot ──→ game_state_*
    └── InventoryJSON import ──→ game_state_inventory/storage
                                        │
                        Feature consumers (SurveyAggregator, DeathTracker, StallTracker, etc.)
                                        │
                                    SQLite DB
                                        │
                                Tauri Event Bridge
                                (game-state-updated, player-events-batch, coordinator-status)
                                        │
                                Vue/Pinia Stores ──→ UI Components
```

## Ideal way to build this

[something smart goes here]

> **Claude's take:** Honestly? What you've built is pretty close to the "ideal" for this problem space. The core pattern — a central coordinator dispatching to watchers, a parser that produces structured events with identity resolution and provenance, a game state manager that persists "last known value" with timestamp-based conflict resolution, and a thin reactive frontend — is the right architecture. If I were designing this from scratch with full hindsight, I'd probably want:
>
> 1. **A formal event bus** between the parser and consumers, rather than the coordinator manually dispatching to each feature manager. Right now adding a new consumer means touching the coordinator. An event bus (or trait-based subscriber pattern) would let features register themselves. But this is a "nice to have" refactor, not a structural problem.
>
> 2. **History tables from day one** for the domains where history matters (skills/XP over time, favor over time). Retrofitting these is annoying because you lose all the historical data from before the table existed. But you can't predict which domains will want history until you build features around them, so this is a classic "you can't know until you know" situation.
>
> 3. **A clearer contract between "what the parser produces" and "what the game state manager expects."** Right now the PlayerEvent enum is that contract, and it works, but it's grown organically to 30+ variants. Some events are "state snapshots" (SkillsLoaded replaces everything), some are "deltas" (ItemStackChanged), and some are "signals" (CharacterLogin triggers state clearing). Making that taxonomy explicit (snapshot vs delta vs signal) could help new contributors understand which pattern to follow.
>
> 4. **The chat↔player.log correlation** would ideally be more robust. The ±2 second timestamp matching for stack size seeding works, but it's the most fragile part of the pipeline. A sequence-number or session-ID based correlation would be more reliable, but you don't control the log format, so you're working with what the game gives you.
>
> But none of these are "you built it wrong." They're "if you could go back in time" refinements. The actual architecture is sound.

## How we actually built it

> **Claude's take:** See the flowchart above in the Player Data section — that's what you actually built. The good news is the gap between "ideal" and "actual" is small. The main divergences from a textbook architecture are:
>
> - **The coordinator is a manual dispatch hub** rather than a pub/sub bus. Feature managers (survey, death, stall) are wired in explicitly. This works fine at the current scale but will get unwieldy if you add 10 more feature managers.
> - **Session-scoped state lives in two places:** some in Rust (survey aggregator state, death tracker state) and some in Vue (sessionSkills, liveItemMap in gameStateStore). This split is a natural consequence of some features being Rust-first (survey) and some being frontend-first (live XP display). Not a problem yet, but worth being intentional about which side "owns" new session-scoped state.
> - **No formal schema for the Tauri event payloads.** The events (`game-state-updated`, `player-events-batch`) carry data, but the shape of that data is implicitly agreed upon between Rust emitters and TypeScript listeners. If these drift apart, you get runtime errors, not compile errors. TypeScript types help on the frontend side, but there's no shared schema generation.
>
> Overall: you built it well. The architecture doc you're writing here is mostly going to confirm "yes, this is what we have, and it's fine" rather than revealing structural problems that need fixing.
