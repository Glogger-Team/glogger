# Time & Timestamp Standards

This is the single source of truth for how glogger handles time. All features must follow these conventions.

## Core Rule: UTC Internally, Convert at Display

All timestamps are stored and transmitted as **UTC**. Conversion to the user's preferred display timezone happens only at the final rendering step in the frontend.

## Storage Format

Database columns store timestamps as `TEXT` in the format:

```
YYYY-MM-DD HH:MM:SS
```

No timezone suffix — it is always UTC by convention. Column names follow the pattern `last_confirmed_at`, `timestamp`, `last_login_at`, etc.

## Timestamp Sources

The app ingests timestamps from two sources with different raw formats:

| Source | Raw Format | Conversion |
|---|---|---|
| Player.log | `[HH:MM:SS]` — UTC, no date | Dated via [`parsers::LogDateResolver`](../../src-tauri/src/parsers.rs): an explicit base date (replay / old-log reparse), else the session date on the log's own login line, else the clock with a one-day rollback — no offset needed |
| Chat.log | `YY-MM-DD HH:MM:SS` — player's local time | Converted to UTC via [`chat_local_to_utc()`](../../src-tauri/src/parsers.rs) using the detected timezone offset |

### Player.log base date

Because Player.log doesn't carry a date, the caller supplies one:

| Mode | Base date |
|---|---|
| Live tailing | `None` → today's UTC date (`Utc::now().date_naive()`) |
| Dual-log replay | Derived from the chat log filename (`Chat-YY-MM-DD.log`), falling back to the first chat timestamp, falling back to today |
| Solo `parse_log` (old-log reparse in Advanced Settings) | The Player.log file's modification time converted to UTC (`SystemTime` → `DateTime<Utc>`) |

`GameStateManager` and `SurveySessionAggregator` both expose `set_base_date()` for the replay and reparse paths; live tailing leaves the override unset.

### Timezone Offset Detection

The game's chat login line contains `Timezone Offset -07:00:00` (or similar). This is parsed by [`parse_timezone_offset()`](../../src-tauri/src/chat_parser.rs) into total seconds from UTC and stored in settings as `timezone_offset_seconds`. This offset is used to convert Chat.log local timestamps to UTC at ingestion time (in the coordinator, replay, and scan commands).

Users can also set `manual_timezone_override` in settings, which takes precedence over auto-detection.

## Backend (Rust)

- Use [`parsers::LogDateResolver`](../../src-tauri/src/parsers.rs) to convert Player.log `HH:MM:SS` times to full UTC datetime strings: `set_base_date(date)` when the caller knows the capture date (replay / old-log reparse, which wins over everything), `anchor_session(utc)` on the log's own login line (`PlayerEvent::LogSessionAnchored`), and `resolve(ts)` to stamp each line — it rolls the session date forward when the time of day goes backwards (a UTC-midnight crossing). Use `peek(ts)` instead of `resolve(ts)` from any side channel that reads a line before its batch is replayed: it reports the same date without advancing the rollover baseline. A live path with no anchor falls back to the clock, rolled back a day when the line's time of day is still ahead of it (`to_utc_datetime_with_base(ts, None)` now applies that rollback; only pass `Some(date)` when you have a real base date).
- Use `chat_local_to_utc(dt, tz_offset_seconds)` from [`parsers.rs`](../../src-tauri/src/parsers.rs) to convert Chat.log `NaiveDateTime` from local time to UTC.
- Use `chrono::Utc::now()` when generating timestamps from the system clock.
- Never store local time in the database.
- The `chrono` crate (with `serde` feature) is the only date/time dependency.

## Frontend (TypeScript/Vue)

### Display Modes

Users choose how timestamps appear via the `timestampDisplayMode` setting:

| Mode | Behavior |
|---|---|
| `"local"` | Browser's local timezone (default) |
| `"server"` | Game server's timezone (uses detected or manual offset) |
| `"utc"` | Raw UTC |

All formatting functions respect this setting automatically.

### Displaying Timestamps

**For standalone timestamp display**, use the [`<Timestamp>`](../../src/components/Shared/Timestamp.vue) component:

```vue
<Timestamp value="2026-03-26 14:30:00" />
<Timestamp value="2026-03-26 14:30:00" granularity="datetime-short" />
```

**For timestamps embedded in strings** (interpolated text, `<option>` tags, computed values), use the formatter functions from [`useTimestamp.ts`](../../src/composables/useTimestamp.ts):

```typescript
import { formatTimeShort, formatDateTimeShort } from '@/composables/useTimestamp'

const label = `Started at ${formatTimeShort(session.started_at)}`
```

### Granularity Reference

| Granularity | Example | Function | Component prop |
|---|---|---|---|
| Time, short | `14:30` | `formatTimeShort()` | `"time-short"` |
| Time, full | `14:30:00` | `formatTimeFull()` | `"time-full"` |
| Date, short | `Mar 26` | `formatDateShort()` | `"date-short"` |
| Date, full | `2026-03-26` | `formatDate()` | `"date-full"` |
| DateTime, short | `Mar 26, 14:30` | `formatDateTimeShort()` | `"datetime-short"` |
| DateTime, full | `2026-03-26 14:30:00` | `formatDateTimeFull()` | `"datetime-full"` |
| Relative | `2m ago`, `Yesterday` | `formatRelative()` | `"relative"` |
| Smart | Time if today, datetime-short otherwise | `formatSmart()` | `"smart"` (default) |

### Duration Formatting

Use `formatDuration(seconds, opts?)` for elapsed time and session durations:

```typescript
import { formatDuration } from '@/composables/useTimestamp'

formatDuration(45)       // "45s"
formatDuration(125)      // "2m 5s"
formatDuration(3725)     // "1h 2m"

// For live timers where seconds matter:
formatDuration(3725, { alwaysShowSeconds: true })  // "1h 2m 5s"
```

Returns `"—"` for zero/negative/falsy values. Use `alwaysShowSeconds: true` for live session displays, omit it for historical summaries.

### Staleness Labels

Use `formatStaleness(timestamp)` for "last updated" displays:

```typescript
import { formatStaleness } from '@/composables/useTimestamp'

formatStaleness("2026-04-02T10:00:00Z")  // "today"
formatStaleness("2026-03-31T10:00:00Z")  // "2 days ago"
formatStaleness("2026-01-15T10:00:00Z")  // "2 months ago"
```

### Other Utilities

- `parseUtc(timestamp)` — Parse a UTC string to a `Date` object. Appends `Z` if no timezone indicator is present.
- `getTimezoneSuffix()` — Returns `""`, `" UTC"`, or `" Server"` based on display mode. Use for tooltip/label clarity.
- `formatAnyTimestamp(timestamp)` — Handles both full datetime strings (from DB) and bare `HH:MM:SS` strings (from live Player.log events). Full datetimes go through timezone-aware formatting; bare times pass through as-is.

## Deconfliction

When a report import and a log event conflict on the same row, **the most recent `last_confirmed_at` wins**. See [game-state.md](game-state.md) for details.

## What NOT to Do

- Do not store local time in the database.
- Do not use `new Date()` or `Date.now()` to format timestamps for display — use the composable functions so the display mode is respected.
- Do not write manual duration formatting (`Math.floor(seconds / 3600)` etc.) — use `formatDuration()`.
- Do not write manual relative date logic (`diffDays`, `diffMonths`) — use `formatStaleness()` or `formatRelative()`.
- Do not use `toLocaleTimeString()` or similar browser APIs — they bypass the display mode setting.
- Do not add external date libraries (dayjs, date-fns, moment). The app uses native `Date` plus the custom composable intentionally.
- Do not parse Player.log timestamps directly in feature code — let the backend convert them to UTC before they reach the database.
- Do not stamp session `started_at` / `ended_at` from `Utc::now()` and leave them — survey sessions recompute both from the first/last attributed event timestamp at end time (see [`recompute_session_bounds_and_end`](../../src-tauri/src/survey/persistence.rs)), which makes replayed and live bounds equally accurate.
