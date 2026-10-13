/**
 * Timestamp formatting utilities.
 *
 * All timestamps stored in the database are UTC. These helpers convert
 * UTC strings to the user's preferred display timezone (local, server, or UTC)
 * based on the `timestampDisplayMode` setting.
 *
 * Granularity levels (use the appropriate one for your context):
 *   - timeShort:      "14:30"
 *   - timeFull:       "14:30:00"
 *   - dateShort:      "Mar 26"
 *   - dateFull:       "2026-03-26"
 *   - dateTimeShort:  "Mar 26, 14:30"
 *   - dateTimeFull:   "2026-03-26 14:30:00"
 *   - relative:       "2m ago", "3h ago", "Yesterday"
 */

import { useSettingsStore } from '../stores/settingsStore'
import { displayOffsetSeconds } from '../utils/displayDay'

export type TimestampDisplayMode = 'local' | 'server' | 'utc'

/**
 * Parse a UTC timestamp string ("YYYY-MM-DD HH:MM:SS" or ISO) and return
 * a Date object. Appends "Z" if no timezone indicator is present so that
 * the browser interprets it as UTC rather than local.
 */
function parseUtc(timestamp: string): Date {
  // If already has timezone info (Z or +/-offset), use as-is
  if (/[Zz]$/.test(timestamp) || /[+-]\d{2}:\d{2}$/.test(timestamp)) {
    return new Date(timestamp)
  }
  // Replace space with T and append Z for proper UTC parsing
  const iso = timestamp.replace(' ', 'T') + 'Z'
  return new Date(iso)
}

/**
 * Get the effective server timezone offset in seconds.
 * Returns manual override if set, otherwise auto-detected, otherwise 0.
 */
function getServerOffsetSeconds(): number {
  const store = useSettingsStore()
  return store.settings.manualTimezoneOverride ?? store.settings.timezoneOffsetSeconds ?? 0
}

/**
 * Get the current display mode from settings.
 */
function getDisplayMode(): TimestampDisplayMode {
  const store = useSettingsStore()
  return store.settings.timestampDisplayMode
}

/**
 * Whether to render times in 24-hour format. Defaults to true.
 */
function use24HourTime(): boolean {
  const store = useSettingsStore()
  return store.settings.use24HourTime
}

/** Pad a number to 2 digits */
function pad2(n: number): string {
  return String(n).padStart(2, '0')
}

/**
 * Format "HH:MM" honoring the 12/24-hour setting.
 * 24h → "14:30"; 12h → "2:30 PM".
 */
function formatHM(hours: number, minutes: number): string {
  if (use24HourTime()) return `${pad2(hours)}:${pad2(minutes)}`
  const period = hours >= 12 ? 'PM' : 'AM'
  const h12 = hours === 0 ? 12 : hours > 12 ? hours - 12 : hours
  return `${h12}:${pad2(minutes)} ${period}`
}

/**
 * Format "HH:MM:SS" honoring the 12/24-hour setting.
 * 24h → "14:30:00"; 12h → "2:30:00 PM".
 */
function formatHMS(hours: number, minutes: number, seconds: number): string {
  if (use24HourTime()) return `${pad2(hours)}:${pad2(minutes)}:${pad2(seconds)}`
  const period = hours >= 12 ? 'PM' : 'AM'
  const h12 = hours === 0 ? 12 : hours > 12 ? hours - 12 : hours
  return `${h12}:${pad2(minutes)}:${pad2(seconds)} ${period}`
}

/**
 * Core conversion: takes a UTC Date and returns a Date-like object
 * adjusted for the current display mode.
 *
 * For "local" mode, we return the Date as-is (JS Date methods use local time).
 * For "utc" mode, we use getUTC* methods.
 * For "server" mode, we shift the UTC time by the server offset.
 */
interface DisplayTime {
  year: number
  month: number   // 0-indexed
  day: number
  hours: number
  minutes: number
  seconds: number
  /** The original Date object (UTC epoch) for relative time calculations */
  utcDate: Date
}

function toDisplayTime(d: Date): DisplayTime {
  const mode = getDisplayMode()

  if (mode === 'utc') {
    return {
      year: d.getUTCFullYear(),
      month: d.getUTCMonth(),
      day: d.getUTCDate(),
      hours: d.getUTCHours(),
      minutes: d.getUTCMinutes(),
      seconds: d.getUTCSeconds(),
      utcDate: d,
    }
  }

  if (mode === 'server') {
    // Server offset is seconds east of UTC (e.g., -25200 for UTC-7)
    // To get server local time: UTC + offset
    const offsetMs = getServerOffsetSeconds() * 1000
    const shifted = new Date(d.getTime() + offsetMs)
    return {
      year: shifted.getUTCFullYear(),
      month: shifted.getUTCMonth(),
      day: shifted.getUTCDate(),
      hours: shifted.getUTCHours(),
      minutes: shifted.getUTCMinutes(),
      seconds: shifted.getUTCSeconds(),
      utcDate: d,
    }
  }

  // "local" — use browser local time (default)
  return {
    year: d.getFullYear(),
    month: d.getMonth(),
    day: d.getDate(),
    hours: d.getHours(),
    minutes: d.getMinutes(),
    seconds: d.getSeconds(),
    utcDate: d,
  }
}

/** Short month names */
const MONTHS_SHORT = ['Jan', 'Feb', 'Mar', 'Apr', 'May', 'Jun', 'Jul', 'Aug', 'Sep', 'Oct', 'Nov', 'Dec']

/**
 * Get a suffix label for the current display mode (for tooltip/display clarity).
 * Returns "" for local, "UTC" for utc, "Server" for server.
 */
export function getTimezoneSuffix(): string {
  const mode = getDisplayMode()
  if (mode === 'utc') return ' UTC'
  if (mode === 'server') return ' Server'
  return ''
}

/**
 * Seconds east of UTC for the current display mode — the shift applied to
 * stored UTC timestamps when rendering. Day bucketing (day picker, day
 * windows, day headers) uses the same value so a day matches the clock times
 * inside it.
 */
export function getDisplayOffsetSeconds(): number {
  return displayOffsetSeconds(getDisplayMode(), getServerOffsetSeconds())
}

// ─── Granularity Formatters ──────────────────────────────────────

/** Format a UTC timestamp as time: "14:30" or "2:30 PM" */
export function formatTimeShort(timestamp: string): string {
  const d = parseUtc(timestamp)
  if (isNaN(d.getTime())) return timestamp
  const t = toDisplayTime(d)
  return formatHM(t.hours, t.minutes)
}

/** Format a UTC timestamp as time: "14:30:00" or "2:30:00 PM" */
export function formatTimeFull(timestamp: string): string {
  const d = parseUtc(timestamp)
  if (isNaN(d.getTime())) return timestamp
  const t = toDisplayTime(d)
  return formatHMS(t.hours, t.minutes, t.seconds)
}

/** Format a UTC timestamp as short date: "Mar 26" */
export function formatDateShort(timestamp: string): string {
  const d = parseUtc(timestamp)
  if (isNaN(d.getTime())) return timestamp
  const t = toDisplayTime(d)
  return `${MONTHS_SHORT[t.month]} ${t.day}`
}

/** Format a UTC timestamp as full date: "2026-03-26" */
export function formatDate(timestamp: string): string {
  const d = parseUtc(timestamp)
  if (isNaN(d.getTime())) return timestamp
  const t = toDisplayTime(d)
  return `${t.year}-${pad2(t.month + 1)}-${pad2(t.day)}`
}

/** Format a UTC timestamp as short date+time: "Mar 26, 14:30" or "Mar 26, 2:30 PM" */
export function formatDateTimeShort(timestamp: string): string {
  const d = parseUtc(timestamp)
  if (isNaN(d.getTime())) return timestamp
  const t = toDisplayTime(d)
  return `${MONTHS_SHORT[t.month]} ${t.day}, ${formatHM(t.hours, t.minutes)}`
}

/** Format a UTC timestamp as full date+time: "2026-03-26 14:30:00" or "2026-03-26 2:30:00 PM" */
export function formatDateTimeFull(timestamp: string): string {
  const d = parseUtc(timestamp)
  if (isNaN(d.getTime())) return timestamp
  const t = toDisplayTime(d)
  return `${t.year}-${pad2(t.month + 1)}-${pad2(t.day)} ${formatHMS(t.hours, t.minutes, t.seconds)}`
}

/** Format a UTC timestamp as relative time: "2m ago", "3h ago", "Yesterday", "Mar 26" */
export function formatRelative(timestamp: string): string {
  const d = parseUtc(timestamp)
  if (isNaN(d.getTime())) return timestamp

  const now = Date.now()
  const diffMs = now - d.getTime()

  if (diffMs < 0) return formatDateTimeShort(timestamp) // future dates
  if (diffMs < 60_000) return 'Just now'
  if (diffMs < 3_600_000) return `${Math.floor(diffMs / 60_000)}m ago`
  if (diffMs < 86_400_000) return `${Math.floor(diffMs / 3_600_000)}h ago`
  if (diffMs < 172_800_000) return 'Yesterday'

  return formatDateShort(timestamp)
}

/**
 * Smart time display: if today show time only, if older show date+time.
 * Uses the display timezone for the "today" check.
 */
export function formatSmart(timestamp: string): string {
  const d = parseUtc(timestamp)
  if (isNaN(d.getTime())) return timestamp

  const t = toDisplayTime(d)
  const nowT = toDisplayTime(new Date())

  const isToday = t.year === nowT.year && t.month === nowT.month && t.day === nowT.day

  return isToday ? formatTimeShort(timestamp) : formatDateTimeShort(timestamp)
}

/**
 * Format a timestamp that may be either a full datetime string (from DB)
 * or a bare "HH:MM:SS" string (from live Player.log events).
 * Full datetimes go through timezone-aware formatting; bare times pass through.
 */
export function formatAnyTimestamp(timestamp: string): string {
  if (!timestamp) return ''
  // Full datetime: contains a date separator
  if (timestamp.includes('-') || timestamp.includes('T')) {
    return formatTimeFull(timestamp)
  }
  // Bare HH:MM:SS from live session events — pass through as-is
  return timestamp
}

// ─── Duration Formatting ─────────────────────────────────────────

/**
 * Format a duration in seconds as a human-readable string.
 *
 * By default shows seconds for short durations (< 1 hour) and omits them
 * for longer durations:
 *   - 45       → "45s"
 *   - 125      → "2m 5s"
 *   - 3725     → "1h 2m"
 *
 * Pass `alwaysShowSeconds: true` to always include seconds:
 *   - 3725     → "1h 2m 5s"
 *
 * Returns "—" for zero/negative/falsy values.
 */
export function formatDuration(
  seconds: number,
  opts?: { alwaysShowSeconds?: boolean },
): string {
  if (!seconds || seconds <= 0) return '—'

  const h = Math.floor(seconds / 3600)
  const m = Math.floor((seconds % 3600) / 60)
  const s = seconds % 60

  if (h > 0) {
    return opts?.alwaysShowSeconds ? `${h}h ${m}m ${s}s` : `${h}h ${m}m`
  }
  if (m > 0) return `${m}m ${s}s`
  return `${s}s`
}

/**
 * Format a UTC timestamp (or ISO string) as a relative staleness label.
 * Designed for "last updated" displays — always shows full units:
 *   - same day   → "today"
 *   - 1 day ago  → "1 day ago"
 *   - < 30 days  → "X days ago"
 *   - ≥ 30 days  → "X months ago"
 */
export function formatStaleness(timestamp: string): string {
  const d = parseUtc(timestamp)
  if (isNaN(d.getTime())) return ''

  const now = new Date()
  const diffMs = now.getTime() - d.getTime()
  const diffDays = Math.floor(diffMs / (1000 * 60 * 60 * 24))

  if (diffDays <= 0) return 'today'
  if (diffDays === 1) return '1 day ago'
  if (diffDays < 30) return `${diffDays} days ago`
  const diffMonths = Math.floor(diffDays / 30)
  return diffMonths === 1 ? '1 month ago' : `${diffMonths} months ago`
}

// ─── Raw Utilities (for non-component use) ───────────────────────

/** Parse a UTC timestamp string to a Date. Exported for edge cases. */
export { parseUtc }
