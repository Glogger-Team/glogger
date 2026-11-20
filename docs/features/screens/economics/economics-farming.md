# Economics — Farming Sessions (frozen, historic-only)

## Overview

Frozen since the 10/21 game client: the client stopped writing the verbose `LocalPlayer:` Player.log lines, so loot drops, extracts, item deltas, vendor gold, favor, skill XP, and log backfill accrue no new data. The Farming tab is now a historic-only reader (Session History, Database, Sources). New kills, deaths, XP, and Combat Wisdom from Chat.log live in the Combat view.

## Architecture

### Files (surviving surface — live-session files deleted)

**Backend (Rust):**

- `src-tauri/src/db/migrations.rs` — migration_v5_farming (4 tables)
- `src-tauri/src/db/farming_commands.rs` — save/get/update/delete commands (no new sessions; edits/deletes of historic rows still work)

**Frontend (Vue/TS):**

- `src/types/farming.ts` — frozen aliases kept so historic readers compile; live-session shape intentionally not rebuilt
- `src/stores/farmingStore.ts` — trimmed to `fetchEnemyStats` + cache (currently unreferenced; readers invoke `get_enemy_kill_stats` directly)
- `src/components/Economics/EconomicsFarmingView.vue` — 2-tab shell (Session History / Database)
- `src/components/Farming/HistoricalTab.vue` — past sessions from DB
- `src/components/Farming/DatabaseTab.vue` — historic + imported drop-rate database (export/import, no scan)
- Kill-only ledger: `sweep_unlinked_gains` (`db/unknown_source_commands.rs`) purges gains with no source of any kind after 24h (startup + every scan/tail dispatch). The unattributed-gains tab and its command are deleted.
- `src/components/Farming/ItemDropBreakdownTable.vue`, `EnemyDropTable.vue`, `ExtractDetailTable.vue` — table variants reading backend commands directly

Deleted with the freeze: `FarmingView.vue` (legacy wrapper), `FarmingSessionCard.vue` (live session display), `FarmingLog.vue` (live activity feed), `ItemDropBreakdown.vue` (session-hover popover). The Combat view (`src/components/Combat/CombatView.vue` + `src/stores/combatStore.ts`) covers live kills/deaths/XP/Wisdom.

## How It Worked (pre-10/21 ingestion, now frozen)

> Everything in this section is historical: the deleted live-session
> components (`FarmingView`, `FarmingSessionCard`, `FarmingLog`,
> `ItemDropBreakdown`) and the trimmed store arms (`handlePlayerEvent`,
> `handleSkillUpdate`, `handleEnemyKilled`, `startSession`/`endSession`,
> session computeds/timers, `buildSessionInput`) no longer exist.

### Event Pipeline (no longer produces new data)

The farming calculator used to subscribe to multiple event channels:

```text
Player.log → PlayerEventParser → player-events-batch channel ─┐
                                skill-update channel ─────────┤
Chat.log  → ChatCombatParser  → enemy-killed channel ─────────┤
                                                              ▼
                                                    farmingStore (if session active)
                                                              │
                                                    accumulate deltas + push log
                                                              │
                                                    on endSession() → invoke("save_farming_session")
```

New rows stopped with the 10/21 client (verbose `LocalPlayer:` lines gone). Chat-log kills/deaths/XP/wisdom still flow — see the Combat view.

### What Used To Be Tracked (historic rows only)

| Category | Source Event (frozen) | Details |
|----------|----------------------|---------|
| Skill XP | `skill-update` | Baseline delta per skill, level-ups detected, XP/hr computed |
| Items Gained | `ItemStackChanged` (positive delta) | Net quantity per item, items/hr |
| Items Lost | `ItemStackChanged` (negative delta), `ItemDeleted` (Consumed/Unknown) | Storage transfers and vendor sales excluded |
| NPC Favor | `FavorChanged` | Accumulated per NPC |
| Vendor Gold | `VendorSold` | Total gold from vendor sales |
| Enemy Kills | `enemy-killed` (chat combat) | Kill count per enemy type, kills/hr, loot attributed via CorpseSearch provenance |

### Item Tracking Design (historic rows only)

- `ItemStackChanged.delta` is the primary quantity source (every inventory change manifests as a stack change)
- `ItemDeleted` with `StorageTransfer` or `VendorSale` context is **excluded** from net loss — those are intentional player actions
- `ItemDeleted` with `Consumed` or `Unknown` context counts as a farming loss

### Active Session (removed — Combat view covers live kills/XP)

Historic sessions stay readable under Farming → Session History. Live combat (kills, deaths, XP, Combat Wisdom, event log, rates, Reset) lives in the Combat view.

### Session Lifecycle (no longer runs)

1. User used to click **Start Session** (optionally naming it)
2. Store intercepted `player-event` and `skill-update` events
3. Live metrics updated as events arrived (XP/hr, items/hr, etc.)
4. User could **Pause/Resume** — paused time excluded from rate calculations
5. User could edit session **name** and **notes** inline
6. User clicked **End Session** — summary saved to SQLite
7. Session appears in **History** tab with expandable detail view

The Active Session tab, its store arms, and the live-session components are deleted; the readers (History/Database/Sources) remain.

### Historical Sessions

- Browse past sessions with full detail (skills, items, favors, kills)
- Edit session name and notes after the fact
- Delete sessions

## Kill Tracking (historic + imported only)

Kill tracking operates at two levels, both frozen for new data. Chat kills/deaths/XP/Wisdom flow only into the Combat view's live accumulators, never into farming tables:

### Standalone Kill Database (historic + imported only)

Each searched lootable corpse used to persist to the `enemy_kills` table. When the player looted a corpse, items with `CorpseSearch` provenance were linked to the kill via entity_id matching and stored in `enemy_kill_loot`. No new rows accrue (client writes no loot lines); historic + imported rows remain queryable.

The kill-to-loot link used a short-lived in-memory map (`recent_kills`) mapping `enemy_entity_id -> kill_row_id`. When items arrived with `CorpseSearch { entity_id }` provenance, the coordinator looked up the matching kill and inserted into `enemy_kill_loot`. Entries expired after 120 seconds.

### Farming Session Integration (removed)

Live session kill attribution is deleted (Combat view covers live kills; the Active Session tab is gone). Persisted `farming_session_kills` rows remain readable and feed the Combat view's lifetime baselines.

## Database Tables

- **`farming_sessions`** — session summary (name, notes, timing, vendor gold)
- **`farming_session_skills`** — XP gained per skill per session
- **`farming_session_items`** — net item quantity changes per session
- **`farming_session_favors`** — favor changes per NPC per session
- **`farming_session_kills`** — kill counts per enemy type per session
- **`enemy_kills`** — standalone record of every enemy kill (not session-scoped)
- **`enemy_kill_loot`** — items attributed to specific kills via entity_id matching

`farming_session_*` tables cascade on delete from `farming_sessions`. `enemy_kill_loot` cascades on delete from `enemy_kills`.

## Time Handling (historical — live-session clock deleted)

Farming follows the app-wide time standards defined in [time.md](../../../architecture/time.md).

### Active Sessions (deleted; frozen design below)

- The deleted in-memory `FarmingSession` stored `startTime`, `endTime`, and `pauseStartTime` as `HH:MM:SS` strings generated by `getCurrentTimestamp()` (`formatTimeFull()` from `useTimestamp.ts`).
- **Elapsed calculation** converted these strings to seconds via `tsToSeconds()` for pause-aware duration math. The computed `elapsed` display used `formatDuration()` with `alwaysShowSeconds: true` for the live timer.
- **Per-hour rates** (XP/hr, items/hr) divided accumulated totals by `getActiveSeconds() / 3600`.
- **Activity log** entries carried the raw timestamp from the Player.log event and were displayed via `formatAnyTimestamp()`.

### Persistence (unchanged — historic rows still read this way)

- When a session ends, `start_time` and `end_time` strings and the computed `elapsed_seconds` integer are saved to the database.
- Historical sessions display timestamps via `formatDateTimeShort()` and durations via `formatDuration()` (without seconds for completed sessions).

## Tauri Commands

- `save_farming_session(input) → session_id`
- `get_farming_sessions(limit?) → Vec<FarmingSession>`
- `update_farming_session(session_id, name, notes)`
- `delete_farming_session(session_id)`
