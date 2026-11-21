# Widget: Milking Timers

**Size:** Small (1 column)

## Purpose

Tracks cow milking activity across two categories: player-to-player milking leaderboards and self-milking (cow players milking themselves). The NPC-cow cooldown Timers tab was removed 2026-11-21 (only the self row had a live writer; every other row was frozen since the log went dark) — `milking_timers` rows are still written by the Collect-Milk arm but no surface reads them.

## Features

### Timers Tab (removed 2026-11-21)

Previously showed 1-hour countdown timers per cow (5-minute for the self-named row), grouped by zone with the current zone floated to top, rendering `≤ Xm` upper bounds for `cooldown_unknown` rows. Removed with the per-cow timer command: the only live writer was the self-milk row, every other cow's row was frozen, and the tab had been hidden behind `milking-dark` since the log went dark. The `record_cow_milk` write path stays (it gates the `milking` domain emit); the read surface is gone.

### Players Milked Tab

- **Leaderboard:** Ranked list of other players milked by this character, sorted by count descending.

### Milked By Tab

- **Leaderboard:** Ranked list of players who have milked this character, sorted by count descending.

### Self-Milked Counter

- **Inline counter:** A single line above the tabs showing the total self-milk count. Only visible when count > 0.

## Detection & Data Flow

### NPC Cow Milking

Live producer: the Combat-channel Collect-Milk arm in `dispatch_chat_message`
(`src-tauri/src/chat_status_dispatch.rs`). A `[Combat] Sender: Collect Milk on Target!`
line issued by the active character parses via
`chat_combat_parser::parse_self_ability_line(…, "Collect Milk")` — the sender must be
the active character, and the match runs on the sender-split `message` (never the raw
line, same gotcha as the self-milk counter below). Each firing writes one `milking_timers`
row keyed `(character_name, server_name, cow_name)` = (active character, active server,
the ability target): `zone` is the current area (`''` when unknown), `last_milked_at` is
the chat line's UTC stamp, `cooldown_unknown` is `0`, the stamp is newer-wins, and a known
zone is never overwritten with a blank one. Since the Timers tab removal, no surface reads
these rows — the write path stays because `record_cow_milk`'s row-advanced return gates the
`milking` domain emit. When a cow-form player milks themselves, the target IS the player's
own name, so the arm writes a self-named timer row alongside the `self_milked` log row —
that is what the code does today.

Cooldown lengths (for the record, no longer surfaced): every cow runs the 1-hour cooldown — the 14 named NPC cows ([wiki roster](https://wiki.projectgorgon.com/wiki/Cow_(Domesticated_Animal))) and player cows milked by another player alike. The sole exception is the self-named row, which runs the 5-minute self-milk cooldown.

### NPC Cow Cooldown Backfill

No live producer. The `(attempt - 1h, attempt]` bound previously came from
`ProcessScreenText` cooldown errors, which no longer exist. `cooldown_unknown`
rows in the table are legacy; nothing renders them since the Timers tab removal.

### Player-to-Player Milking (Milking Another Player)

No live producer. The `"You obtain fresh milk from …"` line is not parsed, so no
`milked`-direction rows are written (`v38` still allows the direction for legacy rows).

### Player-to-Player Milking (Being Milked)

1. Another player milks this character
2. Chat `[Status] PlayerName has milked you.` is detected via `chat_status_parser::try_milked_by` — this is the live producer (28 events observed in Chat logs Jun–Oct 2026; e.g. `26-10-24 22:07:13 [Status] Azyima has milked you.`)
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
| cooldown_unknown | INTEGER | 1 = exact milk time unknown; the removed Timers tab rendered an upper bound (`≤ Xm`) instead of a countdown (migration v73) |

Like `player_milking_log.milked_at`, this is the milk event's own time (the chat line's
UTC stamp via `chat_status_dispatch::utc_naive_to_rfc3339`), not ingest time. The
Collect-Milk arm above is the live writer, so the `MAX(excluded, stored)` merge rule is
live: replaying an older line never moves the stamp backwards, while a replay that names
a previously-unknown area still learns it (pinned by `collect_milk_arm_records_named_cow`).

**Identity is per-cow, not per-cow-per-zone.** A cow's 1-hour cooldown belongs to
the NPC itself (the game's "You've already milked Homer in the past hour." message
carries no zone). `zone` was originally part of the primary key, so a cow recorded
once with a known area and once while `current_area` was still unknown (`zone = ''`)
produced two rows and the widget showed the cow twice. Migration v63 drops `zone`
from the PK (keeping the most recent milk time and most recent non-empty zone when
merging existing duplicates); the write path now upserts on
`(character_name, server_name, cow_name)` and never overwrites a known zone with a
blank one (the removed widget additionally de-duplicated by `cow_name` as a safeguard).

### Table: `player_milking_log` (migration v36, updated v38, deduped v72)

| Column | Type | Description |
|--------|------|-------------|
| id | INTEGER PK | Auto-increment |
| character_name | TEXT | Character who performed/received the milking |
| server_name | TEXT | Server name |
| other_player | TEXT | The other player involved (or self for self_milked) |
| direction | TEXT | `milked`, `milked_by`, or `self_milked` |
| milked_at | TEXT | RFC3339 UTC **event time** (not ingest time) |

`milked_at` is derived from the event itself by `chat_status_dispatch::utc_naive_to_rfc3339`:

| Source | Time handed in |
|---|---|
| Chat `[Status] … has milked you.` (`milked_by`) | the Status line's UTC timestamp via `chat_status_parser::try_milked_by` |
| Chat `[Combat] … Collect Milk …` (`self_milked`) | the chat line timestamp converted with **that log file's own login-line offset** (falling back to the settings offset) — see below |
| `milked` (milking another player) | — no producer; direction unwritten (legacy rows only) |

A missing/unparseable event time falls back to `Utc::now()` so a milking is never dropped.

Fixed 2026-10: `record_player_milk` previously ignored the event timestamp and wrote `Utc::now()` (ingest time), so `milked`/`milked_by` rows read during a catch-up replay were stamped with the read time instead of the log's — a backfill would have dated an entire archive "today". (The `self_milked` arm additionally could not fire at all before the sender-split matcher fix.) The timestamp *format* is pinned by `utc_naive_to_rfc3339_converts_and_falls_back`; the call-site wiring was verified live against record 2026-10-13T01:14:16+00:00 for a 20:14:16 local self-milk.

The backfill converts with each log's **own** offset rather than today's setting, because the instant is half of the dedup key: re-running after a DST flip or a changed `manual_timezone_override` must re-derive the same string, not insert a twin row.

`v72` adds `UNIQUE(character_name, server_name, direction, other_player, milked_at)` (after collapsing any pre-existing exact duplicates), and every write goes through `game_state_commands::record_milking_event` as `INSERT … ON CONFLICT(<that key>) DO NOTHING` — the conflict target is explicit so a CHECK/NOT NULL violation still surfaces instead of looking like a duplicate. Without the key, the live tail, a re-read catch-up and the archive backfill each added their own row for one milk and the counter inflated.

## Files

- `src/components/Dashboard/widgets/MilkingTimersWidget.vue` — widget component (Players Milked + Milked By tabs + self-milked counter; Timers tab removed 2026-11-21)
- `src-tauri/src/chat_status_dispatch.rs` — live detection (`MilkedBy` status arm, self-milk combat arm); `src-tauri/src/chat_commands.rs` — `backfill_ability_history` archive scan
- `src-tauri/src/db/migrations.rs` — migrations v35 (milking_timers), v36 (player_milking_log), v38 (self_milked direction), v72 (per-event dedup key), v73 (`cooldown_unknown` bound column)
- `src-tauri/src/db/game_state_commands.rs` — `get_player_milking_leaderboard` Tauri command (per-cow timer command removed 2026-11-21; `record_cow_milk` write path stays for the `milking` domain emit)
