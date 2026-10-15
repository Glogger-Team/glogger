# Statehelm gift-status backfill from the game's logs

Gifts given while glogger was closed used to be lost: the weekly gift tracker
(`game_state_gift_log` → `useStatehelmTracker` → Character → Statehelm +
the `statehelm-summary` dashboard widget) only reflected gifts glogger was
*running* for. The game keeps only `Player.log` + `Player-prev.log`, and only
the live parser ever read the gift-cap notes out of them.

This feature re-reads the game's own authoritative weekly counts from the logs
glogger already watches and reconciles each gift-capped NPC's current-week row
count to the game's number.

## What it does

- **At startup** (Step 5d chain in `lib.rs`, after the arena-bet backfill) and
  **on demand** (`backfill_statehelm_gifts`, the "Rescan logs" button on
  Character → Statehelm).
- Sources, in order: archived rotated copies in
  `<app data dir>/player-logs` (recent ones only), `Player-prev.log`, then
  `Player.log`. Path list is built by `backfill_from_player_logs`.
- For every gift-capped NPC named in those files, the current week's
  `game_state_gift_log` rows are set to the game's count — rows added at the
  note's timestamp with `favor_delta = 0` when short, newest extras trimmed
  when long.
- Idempotent by construction (absolute count per NPC per week): re-running
  changes nothing, so no content-hash table and no migration were needed.
- Emits `game-state-updated ["favor"]` only when a count actually changed.

## Contract / invariants

- **One reconcile implementation.** `db::game_state_commands::reconcile_gift_count`
  is shared by the live `PlayerEvent::GiftCountObserved` arm
  (`game_state.rs`) and the backfill, so the two cannot drift.
- **One parser.** `player_event_parser::parse_gift_count_line` (free function,
  no parser state) is called by both the live dispatch and the backfill, and
  both gate on the same literal
  `"gifts per calendar week and has received"`.
- **Dating rule.** `Player.log` lines carry `[HH:MM:SS]` only, and PG writes
  those in UTC (verified: the bracket time equals the login line's
  `Time UTC=`). So the log date is the file's login-line UTC date, advanced one
  day whenever the time of day goes backwards (midnight rollover). A file with
  no login line is **skipped** rather than guessed — a wrong date would
  silently pollute the week. The parser is now shared
  (`parsers::parse_player_log_login_line`).
- **The live ingest path uses the same anchor.** Everything the live path
  persists is dated by `parsers::LogDateResolver`: an explicit base date (replay
  / old-log reparse) → the session date from the log's own login line
  (`PlayerEvent::LogSessionAnchored`, rolled at UTC midnight) → the clock with a
  one-day rollback when the line's time of day is ahead of it. Before that, the
  live path had no anchor at all and stamped lines with the *ingest* date, so a
  lagging catch-up over UTC midnight wrote timestamps up to ~24 h in the future
  (a 23:31 note logged 2026-10-14 came out as `2026-10-15 23:31:26`, which lands
  in the *next* ET week and skews that week's counts). Verified live: a catch-up
  five days behind the clock wrote `2026-10-10 23:31:15` / `2026-10-11 00:10:00`
  (anchored + rolled) instead of the clock's date.
- **Week filter.** Notes are kept only when `at >= current_week_start_utc()`
  (both stamps are the same DB format, so the string compare is chronological);
  a stale last-week note must never be reconciled against this week's rows.
  **Both paths** apply it: the backfill while scanning, and the live
  `GiftCountObserved` arm before it calls `reconcile_gift_count`. Without the
  live guard, a note the game printed last week — replayed by a catch-up that
  spans the weekly reset, or by a manual replay of an old log — was still
  reconciled against this week: measured, it wrote 5 rows into *last* week, or,
  when this week's count was already higher, trimmed this week's real gifts
  down to last week's number (5 → 2).
- **The week boundary is Eastern, and both sides own one definition.** The
  game's cap note says "The week resets on Monday EDT.", so the week starts at
  Monday 00:00 **America/New_York** — 04:00 UTC in DST, 05:00 UTC in EST — in
  `game_state::current_week_start_utc` and in
  `useStatehelmTracker.getCurrentWeekStart`. Rust derives the offset from the US
  DST dates (second Sunday in March / first Sunday in November, no tz database
  dependency); the frontend reads the offset from `Intl` (`America/New_York`).
  Both are pinned to the same seven-case table (two of them DST edges) in
  `game_state::tests::week_boundary_is_monday_midnight_eastern` and by the
  boundary check recorded in `HANDOFF.md` session 40. Change one, change the
  other *in the same commit* — a row that is written but filtered out (or vice
  versa) shows wrong counts.
- **No `ProcessDeltaFavor` replay.** `FavorChanged` handling *adds* to
  `game_state_favor.cumulative_delta`, so replaying those lines would
  double-count favor. The absolute note count is the only signal needed.
- **No new game interaction.** Log text only, from files the client wrote to
  the user's own disk — see `docs/research/glogger-game-interaction-limits.md`
  §3. No packet, memory, or client-binary anything.
- **Server attribution** comes from `active_server_name`; the log's own
  character (login line) supplies `character_name`. With no active server set,
  the backfill logs and returns 0 (rows must match what the widget queries).
- **NPC keys** resolve against the already-persisted `npcs` table (exact
  display-name match mirroring `GameDataState::npc_name_index`, `ORDER BY key`
  because names repeat), falling back to the raw name. An empty `npcs` table
  (fresh DB, no CDN persisted yet) makes the backfill a no-op.

## Rotated-log archive

The game keeps only two Player logs, so the backfill can only reach back to the
previous session unless rotated copies are kept. `spawn_player_prev_watcher`
(the existing 30 s rotation watcher) now copies each newly-rotated
`Player-prev.log` into `<app data dir>/player-logs/` as
`Player-<fnv1a content hash>.log`.

Limits are **settings** (`AppSettings.player_log_archive_*`, Settings → General,
next to the Player-prev.log toggle; defaults mirrored by the
`PLAYER_LOG_ARCHIVE_DEFAULT_*` consts in `replay.rs`), enforced oldest-first by
`prune_player_log_archive`:

| Setting | Default | Notes |
|---|---|---|
| `player_log_archive_retention_days` | 14 | age-based eviction |
| `player_log_archive_max_files` | 20 | count-based eviction |
| `player_log_archive_max_mb` | 512 | byte budget — the usual binding limit (≈11 sessions at 45 MB) |

Why these three, and why in that shape:

- **A file count alone does not bound disk use.** A session's `Player.log` is
  30–45 MB here, so 20 copies is ~900 MB. Every retention system that survives
  contact with variable-size records gates on bytes — `journald`
  (`SystemMaxUse=`/`SystemMaxFileSize=`), Docker (`log-opt max-size`,
  `max-file`), `logrotate` (`size`/`maxsize` + `rotate`). The byte budget is
  the real ceiling; count and age are secondary.
- **Write-then-rename, always.** The archive name is *content-addressed*, so a
  copy interrupted mid-write would exist under a name certifying the wrong
  bytes — and its `exists()` check would suppress the correct copy forever.
  `archive_player_log` therefore writes `Player-<hash>.log.partial` and renames
  (atomic) into place; prune also sweeps stray `.partial` files.
- **Enforced every watcher tick, not just on a new session.** Pruning runs
  after a successful copy *and* on each 30 s tick, so a limit lowered in
  Settings takes effect within one tick instead of waiting for the game to
  rotate (or for a restart). A `readdir` over a handful of entries is far
  cheaper than the log scans beside it.
- **Only this feature's own files are touched:** prune ignores anything not
  matching `Player-*.log` and only ever runs in glogger's own data directory
  (never the game's log folder). Values are clamped to ≥ 1 on both sides, so a
  hand-edited `0` cannot make the archive delete what it just wrote.

## Frontend

- `useStatehelmTracker`'s week boundary is the DB format
  (`YYYY-MM-DD HH:MM:SS`, `weekStartDb`), not `toISOString()`: an ISO cutoff
  sorts before every Monday-dated row and silently dropped them. It is also
  what `remove_last_gift`'s `gifted_at >= ?4` compares against. Rows written by
  `add_manual_gift` (`'T' > ' '` on the boundary day) stay included, so no data
  migration was needed.
- `statehelmNpcs` covers the whole Statehelm region — `AreaStatehelm` **and**
  `AreaStatehelmCaves` (Statehelm Sewers: Hamilton, Smoke Tail, Fuzzlebun, all
  of which show the cap note) — instead of the city alone, so sewers NPCs are
  tracked and reconciled like city ones.
- The **Rescan logs** button (Character → Statehelm, next to the existing stats
  block) invokes `backfill_statehelm_gifts`, reloads the gift log, and reports
  "Reconciled N NPC(s) from the logs." / "No new gift info found in the logs."
  The dashboard widget is untouched (it is size-budgeted per
  `docs/architecture/widget-sizing-audit.md`).
- Settings → General carries the rotated-log archive limits (see above), next to
  the Player-prev.log toggle they belong to.

## Known limits

- **Capped NPCs only.** Only NPCs with an in-week cap note are reconciled.
  Uncapped NPCs (flat 25-favor gifts, no note) keep whatever the live path
  recorded.
- **The cap itself is not in the CDN.** No NPC field marks the weekly gift cap
  (`ItemGifts` is a service tier list, not a limit), so the region is used as
  the signal: Statehelm city + sewers, with gift preferences. A newly capped NPC
  outside those two areas would need the filter widened (and one that shares an
  area but isn't capped would need it narrowed).
- **`Player.log` itself is never archived** — only rotated copies; the live log
  is always read directly.
