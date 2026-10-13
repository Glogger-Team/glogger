# Widget: Milking Timers

**Size:** Small (1 column)

## Purpose

Tracks cow milking activity across three categories: NPC cow cooldown timers, player-to-player milking leaderboards, and self-milking (cow players milking themselves).

## Features

### Timers Tab (NPC Cows)
- **Cooldown tracking:** Shows 1-hour countdown timers for each NPC cow milked, with "Ready!" when cooldown expires. When the milk time is genuinely unknown (see the backfill below) the timer says `≤ 45m` instead — an upper bound that is certain at zero, rather than a fabricated countdown.
- **Grouped by zone:** Cows are organized under their zone, sorted alphabetically.
- **Current zone first:** The player's current zone is highlighted and floated to the top.
- **Cooldown backfill:** If the player tries to milk a cow that's still on cooldown, the error is detected and a *bound* is recorded (see "NPC Cow Cooldown Backfill").

### Players Milked Tab
- **Leaderboard:** Ranked list of other players milked by this character, sorted by count descending.

### Milked By Tab
- **Leaderboard:** Ranked list of players who have milked this character, sorted by count descending.

### Self-Milked Counter
- **Inline counter:** A single line above the tabs showing the total self-milk count. Only visible when count > 0.

## Detection & Data Flow

### NPC Cow Milking
1. Player interacts with an NPC cow (e.g., `Cow_Homer`)
2. `PlayerEventParser` emits `InteractionStarted` with an `npc_name` starting with `Cow_`
3. Coordinator stores `pending_cow_interaction`
4. Chat status `[Status] Bottle of Milk added to inventory.` fires
5. Coordinator matches the milk gain to the pending cow interaction
6. Cow name and zone are persisted to `milking_timers` table (UPSERT)

### NPC Cow Cooldown Backfill
1. Player tries to milk a cow still on cooldown
2. `ProcessScreenText(ErrorMessage, "You've already milked Homer in the past hour.")` is detected
3. Coordinator records a **bound**, not a milk time: `cooldown_unknown = 1` with `last_milked_at` = the error line's own timestamp (the earliest attempt seen wins — each accepted attempt tightens the bound)

The error only says the milk happened somewhere in `(attempt - 1h, attempt]`, so the widget's remaining time is `expiry ≤ attempt + 1h` and it renders `≤ Xm`; at zero the cow is certainly ready. A real milk (`record_cow_milk`) clears the flag and stores the true event time, and a bound can never overwrite a known time (only a previously-unknown row's bound is tightened). Rows that predate migration v73 hold the old "59 minutes ago" fabrication and are treated as known — they age out at that cow's next milk.

### Player-to-Player Milking (Milking Another Player)
1. Player milks another player in cow form
2. `ProcessScreenText(GeneralInfo, "You obtain fresh milk from PlayerName.")` is detected
3. Recorded as direction `milked` in `player_milking_log`

### Player-to-Player Milking (Being Milked)
1. Another player milks this character
2. `ProcessScreenText(ImportantInfo, "PlayerName has milked you.")` is detected
3. Recorded as direction `milked_by` in `player_milking_log`

### Self-Milking (Cow Player Milks Themselves)
1. A player in cow form uses Collect Milk on themselves
2. Combat chat `[Combat] PlayerName: Collect Milk on PlayerName!` is detected (no entity ID, matching active character as both source and target) via `chat_combat_parser::parse_self_ability_line(…, "Collect Milk")`, matched against the **sender-split** `message` — never against the raw line, which still carries `"PlayerName: "` (see the gotcha in [chat-parser.md](../../chat-parser.md))
3. Recorded as direction `self_milked` in `player_milking_log` (the v38 CHECK constraint allows the direction)
4. Note: self-milking does NOT produce the `ProcessScreenText(GeneralInfo, "You obtain fresh milk from ...")` message that milking other players does
5. Note: the counter's history comes from the same `backfill_ability_history` scan the Character → Stats card uses (one scan per session, shared via `useAbilityHistoryBackfill`); the widget requests it on mount, so archived `Chat-*.log` self-milks land without waiting for a new one

> Fixed 2026-10: step 2 previously compared `message` against `"PlayerName: Collect Milk on PlayerName!"`, which can never match a sender-split message — the counter sat at zero no matter how often you milked yourself. Guarded by `test_self_milk_line_matches_self_but_not_raw_line_comparison`.

## Database

### Table: `milking_timers` (migration v34, revised v63)

| Column | Type | Description |
|--------|------|-------------|
| character_name | TEXT | PK part — character who milked |
| server_name | TEXT | PK part — server |
| cow_name | TEXT | PK part — display name (e.g., "Homer") |
| zone | TEXT | Area where the cow was last milked (grouping label only, not identity) |
| last_milked_at | TEXT | RFC3339 UTC **event time** of last milk (or, when `cooldown_unknown`, the earliest failed attempt that bounds it) |
| cooldown_unknown | INTEGER | 1 = exact milk time unknown; the widget renders an upper bound (`≤ Xm`) instead of a countdown (migration v73) |

Like `player_milking_log.milked_at`, this is the milk event's own time (from the
chat "Bottle of Milk added to inventory" line — the `[Status]` path, not
Player.log), not ingest time: a catch-up replay of an old chat log used to hand
out a fresh cooldown hour every time it was re-read. The upsert keeps
`MAX(excluded, stored)` for the stamp, so a replayed line can't push a cooldown
back while the zone merge still learns a previously-unknown area. When
`cooldown_unknown = 1` the stamp is instead the earliest failed attempt (see the
backfill section) and is tightened with `MIN`.

**Identity is per-cow, not per-cow-per-zone.** A cow's 1-hour cooldown belongs to
the NPC itself (the game's "You've already milked Homer in the past hour." message
carries no zone). `zone` was originally part of the primary key, so a cow recorded
once with a known area and once while `current_area` was still unknown (`zone = ''`)
produced two rows and the widget showed the cow twice. Migration v63 drops `zone`
from the PK (keeping the most recent milk time and most recent non-empty zone when
merging existing duplicates); the write path now upserts on
`(character_name, server_name, cow_name)` and never overwrites a known zone with a
blank one. The widget additionally de-duplicates by `cow_name` as a safeguard.

### Table: `player_milking_log` (migration v36, updated v38, deduped v72)

| Column | Type | Description |
|--------|------|-------------|
| id | INTEGER PK | Auto-increment |
| character_name | TEXT | Character who performed/received the milking |
| server_name | TEXT | Server name |
| other_player | TEXT | The other player involved (or self for self_milked) |
| direction | TEXT | `milked`, `milked_by`, or `self_milked` |
| milked_at | TEXT | RFC3339 UTC **event time** (not ingest time) |

`milked_at` is derived from the event itself by `coordinator::milking_event_timestamp`:

| Source | Time handed in |
|---|---|
| Player.log `ScreenText` (`milked` / `milked_by`) | `GameStateManager::to_utc("HH:MM:SS")` — bare log clock time + date, honouring the replay `base_date_override` |
| Chat `[Combat] … Collect Milk …` (`self_milked`) | the chat line timestamp converted with **that log file's own login-line offset** (falling back to the settings offset) — see below |

A missing/unparseable event time falls back to `Utc::now()` so a milking is never dropped.

Fixed 2026-10: `record_player_milk` previously ignored the event timestamp and wrote `Utc::now()` (ingest time), so `milked`/`milked_by` rows read during a catch-up replay were stamped with the read time instead of the log's — a backfill would have dated an entire archive "today". (The `self_milked` arm additionally could not fire at all before the sender-split matcher fix.) The timestamp *format* is pinned by `coordinator::milking_timestamp_tests`; the call-site wiring was verified live against record 2026-10-13T01:14:16+00:00 for a 20:14:16 local self-milk.

The backfill converts with each log's **own** offset rather than today's setting, because the instant is half of the dedup key: re-running after a DST flip or a changed `manual_timezone_override` must re-derive the same string, not insert a twin row.

`v72` adds `UNIQUE(character_name, server_name, direction, other_player, milked_at)` (after collapsing any pre-existing exact duplicates), and every write goes through `game_state_commands::record_milking_event` as `INSERT … ON CONFLICT(<that key>) DO NOTHING` — the conflict target is explicit so a CHECK/NOT NULL violation still surfaces instead of looking like a duplicate. Without the key, the live tail, a re-read catch-up and the archive backfill each added their own row for one milk and the counter inflated.

## Files

- `src/components/Dashboard/widgets/MilkingTimersWidget.vue` — widget component (3 tabs + self-milked counter)
- `src-tauri/src/coordinator.rs` — detection logic for all milking types
- `src-tauri/src/db/migrations.rs` — migrations v35 (milking_timers), v36 (player_milking_log), v38 (self_milked direction), v72 (per-event dedup key), v73 (`cooldown_unknown` bound column)
- `src-tauri/src/db/game_state_commands.rs` — `get_milking_timers` and `get_player_milking_leaderboard` Tauri commands
