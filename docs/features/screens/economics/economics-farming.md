# Economics — Farming Sessions (frozen, historic-only)

## Overview

Frozen since the 10/21 game client: the client stopped writing the verbose `LocalPlayer:` Player.log lines, so loot drops, extracts, item deltas, vendor gold, favor, skill XP, and log backfill accrue no new data. The Farming tab is now a historic-only reader (Time-boxed, Database). New kills, deaths, XP, and Combat Wisdom from Chat.log live in the Combat view.

## Architecture

### Files (surviving surface — live-session files deleted)

**Backend (Rust):**

- `src-tauri/src/db/migrations.rs` — migration_v5_farming (4 tables)
- `src-tauri/src/db/farming_commands.rs` — save/get/update/delete commands (no new sessions; edits/deletes of historic rows still work)

**Frontend (Vue/TS):**

- `src/types/farming.ts` — live DB-read types only (`EnemyKillStats`, drop-rate search/result rows, `ExtractDetail`, `HistoricalFarmingSession`); the frozen live-session aliases are deleted
- `src/stores/farmingStore.ts` — empty shell (HMR registration only); loot readers invoke `get_enemy_kill_stats` directly
- `src/components/Economics/EconomicsFarmingView.vue` — 2-tab shell (Time-boxed / Database)
- `src/components/Farming/HistoricalTab.vue` — past sessions from DB (unmounted from the Farming view; kept on disk pending a deletion decision)
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
Player.log → item/XP parsers → player event stream ──────────┐
                              skill channel ─────────────────┤
Chat.log  → chat combat parser → enemy-killed channel ───────┤
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
| Skill XP | frozen skill-channel deltas (pre-10/21) | Baseline delta per skill, level-ups detected, XP/hr computed |
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

The Session History tab was removed (Time-boxed is the default view). Live combat (kills, deaths, XP, Combat Wisdom, event log, rates, Reset) lives in the Combat view.

### Session Lifecycle (no longer runs)

1. User used to click **Start Session** (optionally naming it)
2. Store intercepted player-event and skill-channel events
3. Live metrics updated as events arrived (XP/hr, items/hr, etc.)
4. User could **Pause/Resume** — paused time excluded from rate calculations
5. User could edit session **name** and **notes** inline
6. User clicked **End Session** — summary saved to SQLite
7. Session appears in **History** tab with expandable detail view

The Active Session tab, its store arms, and the live-session components are deleted; the readers (History/Database) remain.

### Historical Sessions

- Browse past sessions with full detail (skills, items, favors, kills)
- Edit session name and notes after the fact
- Delete sessions

## Kill Tracking (historic + imported only)

Kill tracking operates at two levels, both frozen for new data. Chat kills/deaths/XP/Wisdom flow only into the Combat view's live accumulators, never into farming tables:

### Standalone Kill Database (historic + imported only)

Each searched lootable corpse used to persist to the `enemy_kills` table. When the player looted a corpse, items with `CorpseSearch` provenance were attributed to the kill via a read-time join: ledger rows (`item_transactions` with `from_entity_id`) matched to `enemy_kills` on identity plus the inclusive 30-minute window, newest kill winning overlaps. No link table exists (the `enemy_kill_loot` link table was dropped in v99). No new rows accrue (client writes no loot lines); historic + imported rows remain queryable.

The pre-join kill-to-loot link used a short-lived in-memory map (`recent_kills`) mapping `enemy_entity_id -> kill_row_id`. When items arrived with `CorpseSearch { entity_id }` provenance, the coordinator looked up the matching kill and inserted into the link table. Entries expired after 120 seconds.

### Farming Session Integration (removed)

Live session kill attribution is deleted (Combat view covers live kills; the Active Session tab is gone). Persisted `farming_session_kills` rows remain readable and feed the Combat view's lifetime baselines.

## Database Tables

- **`farming_sessions`** — session summary (name, notes, timing, vendor gold, attributing character_name/server_name, NULL on legacy rows)
- **`farming_session_skills`** — XP gained per skill per session
- **`farming_session_items`** — net item quantity changes per session
- **`farming_session_favors`** — favor changes per NPC per session
- **`farming_session_kills`** — kill counts per enemy type per session
- **`enemy_kills`** — standalone record of every enemy kill (not session-scoped)

`farming_session_*` tables cascade on delete from `farming_sessions`. Mine-scope loot is a read-time join over `item_transactions` (scoped by character/server like its parent kills), so it follows character deletes/renames with the ledger — no link-table cascade remains.

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

- `save_farming_session(input) → session_id` — `input` may carry `character_name`/`server_name`; absent = legacy NULL row
- `get_farming_sessions(limit?, character_name?, server_name?) → Vec<HistoricalFarmingSession>` — a `Some` filter matches that value OR NULL, so legacy unattributed rows stay visible to every character
- `update_farming_session(session_id, name, notes)`
- `delete_farming_session(session_id)`
