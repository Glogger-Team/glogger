/**
 * Display-timezone day bucketing for chat browsing.
 *
 * Chat timestamps are stored as UTC (the parser shifts the log's local clock
 * by the configured offset). Every clock time the UI renders is shifted into
 * the viewer's display timezone instead — see the display modes in
 * `useTimestamp`. Day buckets (the day picker, the day windows it opens, and
 * the day headers inside the list) must use that same shift, otherwise a
 * non-UTC viewer's day list disagrees with the times inside it: the top or
 * bottom of "2026-08-24" shows clock times stamped 2026-08-23.
 *
 * Offsets are seconds EAST of UTC (UTC-7 → -25200), matching
 * `timezone_offset_seconds` / `manual_timezone_override` and
 * `chat_local_to_utc`.
 *
 * Everything here is derived from ONE offset — the one in effect now — so the
 * picker, the windows it opens and the row headers can never disagree with
 * each other. The only consequence is that an hour around a DST switch buckets
 * like the current offset rather than that date's true local offset; the
 * rendered clock time follows real DST rules, so on those two days a year the
 * hour before/after local midnight can land in the neighbouring bucket.
 */

const DAY_MS = 86_400_000

/** Milliseconds since the epoch for a stored `YYYY-MM-DD HH:MM:SS` UTC stamp. */
function utcMs(timestamp: string): number {
  if (/[Zz]$/.test(timestamp) || /[+-]\d{2}:\d{2}$/.test(timestamp)) return Date.parse(timestamp)
  return Date.parse(`${timestamp.replace(' ', 'T')}Z`)
}

/** `YYYY-MM-DD HH:MM:SS` (UTC) for an epoch-millisecond value. */
function utcStamp(ms: number): string {
  const pad = (n: number) => String(n).padStart(2, '0')
  const d = new Date(ms)
  return `${d.getUTCFullYear()}-${pad(d.getUTCMonth() + 1)}-${pad(d.getUTCDate())} ` +
    `${pad(d.getUTCHours())}:${pad(d.getUTCMinutes())}:${pad(d.getUTCSeconds())}`
}

/** `YYYY-MM-DD` of a stored UTC timestamp, in the display timezone. */
export function displayDayOf(timestamp: string, offsetSeconds: number): string {
  const ms = utcMs(timestamp)
  if (isNaN(ms)) return timestamp.slice(0, 10)
  return utcStamp(ms + offsetSeconds * 1000).slice(0, 10)
}

/**
 * UTC bounds (`startTime`/`endTime` inclusive, as stored) covering exactly one
 * display-timezone day. These are the filters every day-window query uses.
 */
export function displayDayBounds(
  day: string,
  offsetSeconds: number,
): { startTime: string, endTime: string } {
  const start = Date.parse(`${day}T00:00:00Z`) - offsetSeconds * 1000
  return { startTime: utcStamp(start), endTime: utcStamp(start + DAY_MS - 1000) }
}

/**
 * Seconds east of UTC for a display mode. `serverOffsetSeconds` is the
 * configured override / auto-detected offset (east-positive, null when unset).
 */
export function displayOffsetSeconds(
  mode: 'local' | 'server' | 'utc',
  serverOffsetSeconds: number | null,
): number {
  if (mode === 'utc') return 0
  if (mode === 'server') return serverOffsetSeconds ?? 0
  return -new Date().getTimezoneOffset() * 60
}
