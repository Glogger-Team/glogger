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

- **At startup** — after the CDN data is persisted (`lib.rs`'s background
  `init_game_data` task, right after `game-data-ready`), *not* in the Step 5d
  backfill chain: the reconcile resolves NPC display names against the persisted
  `npcs` table, and the chain is a separately spawned task, so from there it
  silently no-opped on a fresh DB (empty `npcs`) with no retry. It logs
  `Statehelm gift backfill skipped: npcs table is empty` in that case.
  **On demand**: `backfill_statehelm_gifts`, the "Rescan logs" button on
  Character → Statehelm. The command is `async` + `spawn_blocking`
  (`replay::ingest_player_log`'s pattern) because the scan can read the whole
  archive — 512 MiB by default — and a non-async Tauri command would do that on
  the main thread.
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
- **A `ProcessDeltaFavor` line is not a gift just because the game flags it
  `is_gift`.** Three signals decide, all verified against the user's own logs
  (2026-10-16):
  1. **`npc_id == 0`** — the game attributed the favor to no entity. Every such
     line is a scripted payout: `ProcessCompleteQuest(18236949, 27909)` →
     `ProcessDeltaFavor(0, "NPC_Cheb", 25, True)` → `ProcessAddItem(Recipe_…)`,
     one repeatable quest that put **116** "gifts" in Cheb's log.
  2. **No gift data in the CDN** (`game_data::npcs::has_gift_tastes`, i.e.
     `Preferences` and `ItemGifts` both empty) — without tastes there is no
     "appropriate gift", so gifting cannot move the NPC's reputation: Gossamyr
     the Great's *accepted* gift emitted **no** `ProcessDeltaFavor` line at all,
     and his rows are his card reading. Every NPC in the DB whose deltas are
     preference-scaled (a real gift: Selaxi 5.90, Corinth 58.97, Merriana
     797.92) has non-empty gift data; the flat-only ones have none.
  3. **A gift-capped area** (`is_gift_capped_area` = `AreaStatehelm` +
     `AreaStatehelmCaves`, the twin of the tracker's `statehelmNpcs` filter) —
     the cap note is that region's authoritative count and prints even at 0, so
     the note owns the rows.
  The `FavorChanged` arm skips the `game_state_gift_log` row for all three and
  still accumulates the favor (it *is* real favor, just not a gift the player
  spent); an NPC the CDN does not know keeps the old behavior. The note arm
  applies rule 2 as well (a tasteless NPC's cap note is ignored), and the
  backfill applies it to the stored `npcs` row so both paths agree. Uncapped,
  tasting NPCs have no note, so there the delta stays the only source. Pinned by
  `game_state::tests::gift_flagged_favor_is_only_logged_when_a_gift_was_spent`
  and `game_data::npcs::tests::gift_data_is_what_marks_an_npc_as_worth_tracking`.
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
- **Only this feature's own files are touched:** prune claims only its own naming
  (`Player-<16 hex digits>.log`, and the `.partial` write-then-rename leftovers)
  and only ever runs in glogger's own data directory (never the game's log
  folder) — a `Player.log` / `Player-prev.log` a user drops in there is read by
  the backfill but never deleted. Values are clamped to ≥ 1 on both sides, and
  the count/byte eviction **never removes the newest copy**, so even a
  hand-edited `max_mb` of 1 (below one 30–45 MB session) cannot make the archive
  delete what it just wrote.

- **The byte budget cannot empty the archive.** Eviction is oldest-first but
  stops at the newest copy, so a budget smaller than one session keeps that one
  session and the backfill still reaches back one rotation.

## Frontend

- `useStatehelmTracker`'s week boundary is the DB format
  (`YYYY-MM-DD HH:MM:SS`, `weekStartDb`), not `toISOString()`: an ISO cutoff
  reads `…T04:00:00.000Z`, whose index 10 is `'T'` (0x54) against a row's `' '`
  (0x20), so it sorts *after* every same-date row — the pre-fix
  `row >= cutoff` test was therefore false for Monday-dated rows and dropped
  them. It is also
  what `remove_last_gift`'s `gifted_at >= ?4` compares against. Rows written by
  `add_manual_gift` (`'T' > ' '` on the boundary day) stay included, so no data
  migration was needed.
- `statehelmNpcs` covers the whole Statehelm region — `AreaStatehelm` **and**
  `AreaStatehelmCaves` (Statehelm Sewers: Hamilton, Smoke Tail, Fuzzlebun, all
  of which show the cap note) — instead of the city alone, so sewers NPCs are
  tracked and reconciled like city ones.
- **`StatehelmNpcStatus.giftTracking`** mirrors `has_gift_tastes`: false for the
  **28 of 68** Statehelm NPCs whose CDN entry lists neither `Preferences` nor
  `ItemGifts` (Gossamyr the Great, Captain Gregory, Slim Ostren, Fenna, Grim
  Jim…). Their cards still render — name, area, favor tier, services — but
  `NpcCard`'s `showGiftTracking` is off, so no counter, no dots, no ± buttons
  (the `+` was also the path that planted Gossamyr's phantom row). They are
  excluded from the widget's skill-driven targets and from the header's
  `totalGiftsGiven` / `totalGiftsMax`, and the "Hide maxed gifts" filter skips
  them. The ingest drops their rows too (see the invariants), so the number
  cannot come back through the DB. An opt-in **"Only NPCs whose gifts matter"**
  checkbox (next to "Hide maxed gifts") narrows the cards to the tracked set;
  the 28 tasteless cards stay visible by default, since the tab is also how the
  user finds NPCs like Gossamyr at all.
- **Both headers read `N of M gift slots used`** with a tooltip ("Each Statehelm
  NPC accepts 5 gifts per week — 40 tracked NPCs × 5. Every NPC's cap resets
  Monday 00:00 America/New_York.") and a **traceable caption** —
  `Gifted this week: Hamilton 5/5 · Sourgum 2/5`. Reason: the header aggregates
  *every* tracked NPC while the dashboard widget's rows are a skill-driven
  "who's next" slice, so `7 / 200` sat above seven `0 / 5` rows and read as
  broken — the 7 came from Hamilton (maxed, so off the row list) and Sourgum
  (his skill ranks below the two shown). The caption comes from
  `giftedThisWeek` (tracked NPCs with `giftsThisWeek > 0`, most first) and
  `trackedNpcCount`; `MAX_GIFTS_PER_WEEK` is exported so the label can't drift
  from the per-NPC caps.
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
- **The note prints at *zero* usage, and its numbers are 5/week + Monday-EDT**
  (verified in game 2026-10-16 while standing at `NPC_Gossamyr`, who has no
  `Preferences`): `"… Gossamyr the Great will accept up to <b>5</b> gifts per
  calendar week and has received <b>0</b> so far this week. (The week resets on
  Monday EDT.)"` — so a note proves the NPC *is* giftable rather than marking a
  used-up cap, and both `MAX_GIFTS_PER_WEEK = 5` and the America/New_York week
  boundary the frontend computes are the game's own numbers. The post-gift
  `ProcessTalkScreen` then repeats it with `<b>1</b>`.
- **An accepted gift can emit no `ProcessDeltaFavor` line at all.** Gossamyr's
  gift that day produced two pre-gift prompt notes, the post-gift note, and *no*
  favor line — so the delta route alone never sees it; the count is note-derived
  (`reconcile_gift_count`, `favor_delta = 0`).
- **A `+` (manual) row is not distinguishable from a note row by shape.** The
  button stamps `chrono::Utc::now()` in the same `YYYY-MM-DD HH:MM:SS` format,
  and it can disagree with the game: the tracker held `NPC_Gossamyr` at
  `2026-10-16 04:10:30` (Thu 23:10 local, `favor_delta = 0`) while the game's own
  note reported **0** that week, and no scanned log (`Player.log`,
  `Player-prev.log` — session login 10/16 01:02 — and the two archived sessions
  10/13, 10/14) carries a Gossamyr note anywhere but in the 10/17 session. Since
  the only writers of a space-format zero-delta row are `reconcile_gift_count`
  and `add_manual_gift`, that row is a manual mark; the next cap note trims it
  (delete-newest-extras), i.e. the game stays authoritative.
- **The cap itself is not in the CDN.** No NPC field marks the weekly gift cap
  (`ItemGifts` is a service tier list, not a limit), so the region is used as
  the signal: Statehelm city + sewers — `area_name` only. `preferences` is a
  favourites list, not a giftability flag, and gating the **roster** on it hid
  NPCs the tab is supposed to list (Gossamyr the Great, Captain Gregory, Slim
  Ostren, Fenna) — removed in Session 43 and staying removed. Session 43 also
  justified that removal with "those NPCs report gift favor"; the corrected
  reading (Session 44) is that their flat 25/100 rows were *scripted* payouts,
  not gifts — Gossamyr's card reading and a repeatable quest flagged
  `ProcessDeltaFavor(0, …)`. What `preferences` (with `ItemGifts`) *is* good for
  is the separate question of whether gifts can move an NPC's reputation at all:
  `giftTracking` / `has_gift_tastes`. A newly capped NPC outside those two areas
  would need the filter widened (and one that shares an area but isn't capped
  would need it narrowed).
- **The CDN cannot mark giftability either (checked, closed).** The tool-facing
  CDN is the file list documented at `https://cdn.projectgorgon.com/v{ver}/data/`
  (mirrored in `docs/CDN-full-examples/`): there is no per-entity attribute
  table, `npcs.json` carries exactly `AreaFriendlyName, AreaName, Desc, Name,
  Pos, Services, Preferences, ItemGifts`, `strings_all.json` contains none of
  the cap-note fragments, and `Translation/strings_npcs.json` (fetched, 1715
  entries) is only `npcinfo_<key>_{Name,Desc,AreaFriendlyName}`. Elder Game
  *does* model it — `attributes.json` defines `NPC_MOD_FAVORFROMGIFTS`
  ("Favor Earned From Gifts %", default 1) — but no published file carries the
  per-NPC values, and the CDN's own note on `npcs.json` says the `Pref` field is
  "not sufficient to exactly calculate favor earned from a gift". So
  giftability/cap can only be observed in the logs (a `Give Gift` prompt, a cap
  note) or asserted by the user; do not re-chase this.
- **The dialog action ids in `ProcessTalkScreen` are decodable** (useful, and the
  reason the array must not be read as a gift flag): the id after the menu array
  on a `ProcessPromptForItem`/`PromptForItem`/`TrainingScreen`/`VendorScreen`
  line names the action that produced it — `-1301` Give Gift (every one of the
  51 gift prompts), `-1801` Training, `-1601` Shop, `-1701` Storage, `-1501`
  Barter, `-2001` Consignment. They match the CDN service lists, which is how
  the earlier "-1801 marks gifting" reading was falsified: the gifted NPCs in
  the sample were all trainers, and `-1301` is present in *every* NPC action
  menu (26/27 captured; the exception is a non-NPC object).
- **`Player.log` itself is never archived** — only rotated copies; the live log
  is always read directly.
