# Time Handling Audit

Audit date: 2026-04-23. Checked against [time.md](time.md).

## Overall Assessment

The codebase is **largely consistent** with the documented time standards. The backend (Rust) correctly stores UTC everywhere, the shared `useTimestamp.ts` composable is well-designed and widely adopted, and most screens use it properly. The issues found are minor — mostly in peripheral UI components that bypass the composable for display formatting.

## Backend (Rust): Clean

- All DB timestamps stored as `YYYY-MM-DD HH:MM:SS` UTC text — confirmed across `game_state.rs`, `chat_commands.rs`, `stall_aggregations.rs`, etc.
- `chrono::Utc::now()` used consistently for generating timestamps.
- `chrono::Local` usage is appropriate:
  - `Local::now()` in debug `eprintln!` macros (`game_state.rs:16`, `cdn_commands.rs:23`, `lib.rs:212`) — debug logging only, not stored.
  - `Local::now()` in `stall_year_resolver.rs:84` — used to determine the current calendar year for year-less stall timestamps, not stored as a timestamp.
- [`parsers::LogDateResolver`](../../src-tauri/src/parsers.rs) owns the Player.log date for both consumers (`GameStateManager`, `SurveySessionAggregator`), and `chat_local_to_utc()` handles Chat.log (local-to-UTC) — both ingestion paths are correct; the resolver's clock fallback also rolls back a day when a line's time of day is still ahead of it (a lagging read over UTC midnight).
- No issues found.

## Frontend: useTimestamp.ts Composable — Correct

The composable (`src/composables/useTimestamp.ts`) is well-structured:
- `parseUtc()` correctly appends `Z` to bare `YYYY-MM-DD HH:MM:SS` strings.
- `toDisplayTime()` correctly handles all three display modes (local, server, UTC).
- All granularity formatters delegate through `toDisplayTime()`.
- Duration formatting via `formatDuration()` is clean.

## Violations Found

### Category 1: `toLocaleDateString()` / `toLocaleTimeString()` bypassing display mode

These use browser APIs directly instead of the `useTimestamp.ts` composable, meaning they always display in browser-local time regardless of the user's `timestampDisplayMode` setting.

| File | Line | What it does | Severity |
|---|---|---|---|
| ~~`src/components/Dashboard/StatusWidget.vue`~~ (deleted — unregistered copy; the live Status widget is `ContextBar.vue`, which renders no dates) | 60 | `toLocaleDateString()` for report import dates | Low — these are import metadata, not game event timestamps |
| `src/components/Dashboard/widgets/HoplologyWidget.vue` | 104 | `toLocaleDateString()` for study dates | Low — dates only (no time component), so TZ shift rarely matters |
| `src/components/Dashboard/widgets/GardenAlmanacWidget.vue` | 120 | `toLocaleString()` for almanac event dates | **Medium** — these are game-world event times that should respect display mode |
| `src/components/Help/ChangelogTab.vue` | 202 | `toLocaleDateString()` for GitHub release dates | None — external metadata, not game timestamps |
| `src/components/StallTracker/StallInventoryTab.vue` | 364 | `toLocaleDateString()` for stall event dates | Low — date-only display |
| `src/components/Shared/DatePicker.vue` | 163, 167 | `toLocaleDateString()` for calendar labels | None — calendar UI widget, inherently local |
| `src/dev-panel/tabs/DebugCaptureTab.vue` | 181 | `toLocaleTimeString()` for capture start time | None — dev tooling |

### Category 2: `new Date().toLocaleDateString()` for generating display strings from wall clock

| File | Line | What it does | Severity |
|---|---|---|---|
| `src/stores/cooksHelperStore.ts` | 234 | Default project name includes `new Date().toLocaleDateString()` | None — cosmetic label, not a timestamp |

### Category 3: `new Date().toISOString()` for storing/sending timestamps

| File | Line | What it does | Concern |
|---|---|---|---|
| `src/stores/marketStore.ts` | 78 | `updated_at: new Date().toISOString()` for optimistic UI | **OK** — this is a local optimistic update; the real value comes from the backend. ISO format with `Z` suffix is valid UTC. |
| `src/stores/craftingStore.ts` | 1017 | `started_at: new Date().toISOString()` for craft tracker | **OK** — ephemeral session tracking, not DB-persisted directly from frontend. |

### Category 4: `Date.now()` for ephemeral/timer use

| File | Usage | Concern |
|---|---|---|
| `src/stores/timerStore.ts` | Timer countdown math | None — wall-clock timers, correct use |
| `src/stores/toastStore.ts` | Toast creation timestamp | None — ephemeral UI |
| `src/stores/deathStore.ts` | Temporary ID generation | None — just a unique number |
| `src/stores/buildPlannerStore.ts` | Temporary negative IDs | None — just a unique number |
| `src/stores/dataBrowserStore.ts` | Recent history timestamp | None — ephemeral browsing history |
| `src/composables/useTimestamp.ts` | `formatRelative()`, `formatStaleness()` | None — correctly uses wall clock for "how long ago" calculations |
| Various survey/farming components | `liveNow` for live duration display | None — correct wall-clock usage for elapsed time |

### Category 5: Manual duration formatting instead of `formatDuration()`

| File | Line | Status |
|---|---|---|
| `src/components/Surveying/ItemCostCalculator.vue` | 328 | **Fixed** — replaced with `formatDuration()` |

### Category 6: Manual relative-time logic instead of `formatRelative()`

| File | Line | What it does | Severity |
|---|---|---|---|
| `src/components/DataBrowser/DataBrowserSidebar.vue` | 277-286 | Custom `relativeTime()` from epoch millis | Low — operates on `Date.now()` epoch values (not UTC strings), so the composable doesn't directly apply. The logic is equivalent. |

## What Was Fixed

1. **`src/components/Surveying/ItemCostCalculator.vue`** — Replaced manual `formatTime()` duration formatter with `formatDuration()` from `useTimestamp.ts`. Added the import.

## Recommendations (Not Fixed)

1. **GardenAlmanacWidget `formatCapturedAt()`** — Should use `formatDateTimeShort()` from `useTimestamp.ts` instead of `toLocaleString()`. This displays game-world event times and should respect the display mode setting. Straightforward fix.

2. ~~**StatusWidget `formatDate()`** — Should use `formatDateTimeShort()` from `useTimestamp.ts`. Low priority since it's just import metadata dates.~~ (Moot — file deleted.)

3. **HoplologyWidget `formatStudyDate()`** — Should use `formatDateShort()` from `useTimestamp.ts`. Low priority since it's date-only.

4. **StallInventoryTab `formatShortDate()`** — Should use `formatDateShort()` from `useTimestamp.ts`. Low priority since it's date-only.

5. **farmingStore `getCurrentTimestamp()`** (line 432) — Uses `formatTimeFull(new Date().toISOString())` which is correct functionally (creates UTC ISO, then formats it), but the function name suggests it returns a raw timestamp rather than a display-formatted string. Consider renaming for clarity.

6. **ContextBar local clock** (line 214) — Uses `toLocaleTimeString()` but this is intentionally the user's local wall clock, not a game timestamp. The doc says "do not use `toLocaleTimeString()`" but this is a legitimate exception (showing the actual wall clock time). Consider adding a note in `time.md` that wall-clock displays are exempt.

## Non-Issues Confirmed

- **DatePicker.vue** — Uses `toLocaleDateString()` for calendar month/day labels. This is a date-selection widget where the local calendar is correct. Not a violation.
- **ChangelogTab.vue** — Uses `toLocaleDateString()` for GitHub release dates. External metadata, not game timestamps. Not a violation.
- **gameStateStore `_tickClocks()`** — Uses `toLocaleString('en-US', { timeZone: 'America/New_York' })` to calculate PG server time / game time. This is intentional — it's computing real-world Eastern time for the game clock display, not formatting a stored timestamp. Correct.
- **useStatehelmTracker** — Computes the weekly boundary from the platform's own tz database (`Intl`, `America/New_York`) and compares it against DB-format (`YYYY-MM-DD HH:MM:SS`, UTC) stamps. Correct — the Statehelm weekly reset is Monday 00:00 *Eastern* (04:00/05:00 UTC; the game's note says "The week resets on Monday EDT."), not UTC.
  Mechanics that must move together: the `weekStartDb` formatter (`useStatehelmTracker.ts`) and the `replace(gifted_at,'T',' ')` comparison (SQL + TS) are one comparison — change one side, change the other. Legacy ISO-with-`T` stamps are tolerated input, never canonical output.
