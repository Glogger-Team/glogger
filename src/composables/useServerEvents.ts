import { ref, computed } from 'vue'
import { useSettingsStore } from '../stores/settingsStore'

/**
 * Server-events schedule — same list `/events` shows in-game.
 *
 * Why curated, not parsed: the popup is server-fed over the game protocol —
 * it never lands in Chat.log or Player.log (verified 2026-10-24: "Dreva
 * Events" appears in neither). Per the TOS hard lines
 * (`docs/research/glogger-game-interaction-limits.md`) we never touch game
 * traffic, so the schedule is a hand-maintained JSON file
 * (`public/data/server-events.json`, shipped with the app).
 *
 * Updating: edit that JSON (or use the widget's gear menu), bump the app.
 * User edits persist in localStorage; shipped JSON is the fallback/reset.
 *
 * All times are US Eastern (game server time), resolved through the
 * platform tz database via `Intl` — never re-implemented (same pattern as
 * `useStatehelmTracker.easternOffsetMinutes`).
 */

export interface RecurringServerEvent {
  id: string
  name: string
  /** 0 = Sunday … 6 = Saturday (Eastern wall-clock weekday) */
  weekday: number
  /** Eastern wall-clock hour/minute */
  hour: number
  minute: number
  location: string
  servers: string[]
  note?: string
}

export interface CustomServerEvent {
  id: string
  name: string
  /** Real instant (ISO string) of the start */
  startsAt: string
  /** Real instant (ISO string) of the end, if known */
  endsAt: string | null
  location: string
  note: string
  servers: string[]
}

export interface UpcomingEvent {
  id: string
  name: string
  location: string
  note: string
  kind: 'recurring' | 'custom'
  /** Next (or current) start as a real instant */
  nextStart: Date
  /** True when the start already passed but the event is likely still live */
  inProgress: boolean
  /** Seconds until start (negative when in progress) */
  secondsUntil: number
  /** Optional end instant (custom events only) */
  endsAt: Date | null
  servers: string[]
}

/** Shipped schedule (fallback + "restore defaults"). Loaded from the JSON file. */
const shippedRecurring = ref<RecurringServerEvent[]>([])
const shippedCustom = ref<CustomServerEvent[]>([])
let loaded = false
let loadPromise: Promise<void> | null = null

function ensureLoaded(): Promise<void> {
  if (loaded) return Promise.resolve()
  if (!loadPromise) {
    loadPromise = fetch('data/server-events.json')
      .then((r) => {
        if (!r.ok) throw new Error(`HTTP ${r.status}`)
        return r.json()
      })
      .then((j) => {
        shippedRecurring.value = Array.isArray(j.recurring) ? j.recurring : []
        shippedCustom.value = Array.isArray(j.custom) ? j.custom : []
        loaded = true
      })
      .catch((e) => {
        console.error('[server-events] Failed to load schedule JSON:', e)
        loaded = true // don't retry forever; user overrides still work
      })
  }
  return loadPromise
}

/** How long after its start a jam still counts as "likely live" (no durations published). */
const LIVE_WINDOW_SECS = 2 * 3600

const RECURRING_OVERRIDES_KEY = 'glogger-server-events-recurring'
const CUSTOM_KEY = 'glogger-server-events-custom'

function loadOverrides<T>(key: string): T[] {
  try {
    const raw = localStorage.getItem(key)
    if (raw) return JSON.parse(raw) as T[]
  } catch { /* fall through to shipped */ }
  return []
}

function saveOverrides(key: string, events: unknown[]) {
  localStorage.setItem(key, JSON.stringify(events))
}

const tickGlobal = globalThis as typeof globalThis & { __gloggerServerEventsTick?: number }
const now = ref(Date.now())
if (!tickGlobal.__gloggerServerEventsTick) {
  tickGlobal.__gloggerServerEventsTick = window.setInterval(() => { now.value = Date.now() }, 30_000)
}

/** User overrides (empty = use shipped). Custom events: full user-owned list. */
const recurringOverrides = ref<RecurringServerEvent[]>(loadOverrides(RECURRING_OVERRIDES_KEY))
const customEvents = ref<CustomServerEvent[]>(loadOverrides<CustomServerEvent>(CUSTOM_KEY))

/** UTC offset of America/New_York at `instant`, in minutes (-240 EDT, -300 EST). */
function easternOffsetMinutes(instant: Date): number {
  const parts = new Intl.DateTimeFormat('en-US', {
    timeZone: 'America/New_York',
    year: 'numeric', month: '2-digit', day: '2-digit',
    hour: '2-digit', minute: '2-digit', second: '2-digit',
    hour12: false,
  }).formatToParts(instant)
  const wall: Record<string, string> = {}
  for (const { type, value } of parts) wall[type] = value
  const asUtc = Date.UTC(
    Number(wall.year), Number(wall.month) - 1, Number(wall.day),
    Number(wall.hour) % 24, Number(wall.minute), Number(wall.second),
  )
  return Math.round((asUtc - instant.getTime()) / 60000)
}

/** Eastern wall-clock Y/M/D of `instant`. */
function easternYmd(instant: Date): { y: number; m: number; d: number } {
  const shifted = new Date(instant.getTime() + easternOffsetMinutes(instant) * 60000)
  return { y: shifted.getUTCFullYear(), m: shifted.getUTCMonth(), d: shifted.getUTCDate() }
}

/** Turn an Eastern wall time into a real instant (two-pass offset resolution). */
function easternWallToInstant(y: number, m: number, d: number, hour: number, minute: number): Date {
  const wallUtc = Date.UTC(y, m, d, hour, minute)
  const off1 = easternOffsetMinutes(new Date(wallUtc))
  const inst1 = new Date(wallUtc - off1 * 60000)
  const off2 = easternOffsetMinutes(inst1)
  return new Date(wallUtc - off2 * 60000)
}

/** Next real instant at/after `now` matching the Eastern weekly slot. */
function nextWeeklyOccurrence(weekday: number, hour: number, minute: number, now: Date): Date {
  const { y, m, d } = easternYmd(now)
  const base = Date.UTC(y, m, d)
  for (let delta = 0; delta <= 7; delta++) {
    const day = new Date(base + delta * 86400000)
    if (day.getUTCDay() !== weekday) continue
    const inst = easternWallToInstant(
      day.getUTCFullYear(), day.getUTCMonth(), day.getUTCDate(), hour, minute,
    )
    if (inst.getTime() > now.getTime()) return inst
  }
  // Unreachable: delta === 7 always matches the weekday a week out.
  return easternWallToInstant(y, m, d + 7, hour, minute)
}

/** "Sun 2:00 PM EDT" for a real instant, in Eastern terms. */
export function formatEastern(instant: Date): string {
  const date = instant.toLocaleDateString('en-US', { timeZone: 'America/New_York', weekday: 'short' })
  const time = instant.toLocaleTimeString('en-US', {
    timeZone: 'America/New_York', hour: 'numeric', minute: '2-digit',
  })
  const tz = instant.toLocaleTimeString('en-US', { timeZone: 'America/New_York', timeZoneName: 'short' }).split(' ').pop()
  return `${date} ${time} ${tz}`
}

export function formatCountdown(secondsUntil: number): string {
  const s = Math.abs(Math.floor(secondsUntil))
  const d = Math.floor(s / 86400)
  const h = Math.floor((s % 86400) / 3600)
  const m = Math.ceil((s % 3600) / 60)
  if (d > 0) return h > 0 ? `${d}d ${h}h` : `${d}d`
  if (h > 0) return m > 0 ? `${h}h ${m}m` : `${h}m`
  if (m <= 0) return 'now'
  return `${m}m`
}

/** Effective recurring list: overrides win when present, else shipped. */
const effectiveRecurring = computed<RecurringServerEvent[]>(() =>
  recurringOverrides.value.length > 0 ? recurringOverrides.value : shippedRecurring.value,
)

/**
 * Custom events for `server`: user list when the user has saved one,
 * else shipped entries applying to this server (or all servers).
 */
function customForServer(server: string | null): CustomServerEvent[] {
  if (customEvents.value.length > 0) {
    return customEvents.value.filter(
      (e) => !e.servers || e.servers.length === 0 || (server && e.servers.includes(server)),
    )
  }
  return shippedCustom.value.filter(
    (e) => !e.servers || e.servers.length === 0 || (server && e.servers.includes(server)),
  )
}

const upcoming = computed<UpcomingEvent[]>(() => {
  void ensureLoaded() // kick off the fetch; reactivity lands via shipped* refs
  const t = now.value
  // Active server scopes the list; fall back to showing everything.
  let server: string | null = null
  try {
    server = useSettingsStore().settings.activeServerName ?? null
  } catch { /* store not ready (HMR) — show all */ }
  const out: UpcomingEvent[] = []

  for (const e of effectiveRecurring.value) {
    if (e.servers && e.servers.length > 0 && server && !e.servers.includes(server)) continue
    const start = nextWeeklyOccurrence(e.weekday, e.hour, e.minute, new Date(t))
    const secs = Math.floor((start.getTime() - t) / 1000)
    // If last week's slot started within the live window, it's still on.
    const prev = new Date(start.getTime() - 7 * 86400000)
    const sincePrev = Math.floor((t - prev.getTime()) / 1000)
    if (sincePrev >= 0 && sincePrev < LIVE_WINDOW_SECS) {
      out.push({
        id: e.id, name: e.name, location: e.location, note: e.note ?? '',
        kind: 'recurring', nextStart: prev, inProgress: true,
        secondsUntil: -sincePrev, endsAt: null, servers: e.servers ?? [],
      })
    } else {
      out.push({
        id: e.id, name: e.name, location: e.location, note: e.note ?? '',
        kind: 'recurring', nextStart: start, inProgress: false,
        secondsUntil: secs, endsAt: null, servers: e.servers ?? [],
      })
    }
  }

  for (const e of customForServer(server)) {
    const start = new Date(e.startsAt).getTime()
    if (Number.isNaN(start)) continue
    const end = e.endsAt ? new Date(e.endsAt).getTime() : NaN
    if (!Number.isNaN(end) && t >= end) continue // season over — drop it
    if (t < start) {
      out.push({
        id: e.id, name: e.name, location: e.location, note: e.note,
        kind: 'custom', nextStart: new Date(start), inProgress: false,
        secondsUntil: Math.floor((start - t) / 1000),
        endsAt: Number.isNaN(end) ? null : new Date(end),
        servers: e.servers ?? [],
      })
    } else if (!Number.isNaN(end) || t - start < LIVE_WINDOW_SECS * 1000) {
      out.push({
        id: e.id, name: e.name, location: e.location, note: e.note,
        kind: 'custom', nextStart: new Date(start), inProgress: true,
        secondsUntil: Math.floor((start - t) / 1000),
        endsAt: Number.isNaN(end) ? null : new Date(end),
        servers: e.servers ?? [],
      })
    }
  }

  // Live first, then nearest start.
  return out.sort((a, b) => {
    if (a.inProgress !== b.inProgress) return a.inProgress ? -1 : 1
    return a.nextStart.getTime() - b.nextStart.getTime()
  })
})

function generateId(): string {
  return Date.now().toString(36) + Math.random().toString(36).slice(2, 6)
}

/** `upcoming` ticks on its own 30s clock — no manual refresh needed. */
export function useServerEvents() {
  /** Full recurring list for the config editor (shipped merged over nothing — overrides replace wholesale). */
  const allRecurring = computed<RecurringServerEvent[]>(() => effectiveRecurring.value)

  function saveRecurring(events: RecurringServerEvent[]) {
    recurringOverrides.value = events
    saveOverrides(RECURRING_OVERRIDES_KEY, events)
  }

  function addRecurring(e: Omit<RecurringServerEvent, 'id'>) {
    saveRecurring([...effectiveRecurring.value, { ...e, id: generateId() }])
  }

  function removeRecurring(id: string) {
    saveRecurring(effectiveRecurring.value.filter((e) => e.id !== id))
  }

  function resetRecurring() {
    recurringOverrides.value = []
    localStorage.removeItem(RECURRING_OVERRIDES_KEY)
  }

  /** All custom events the user owns (unfiltered — the editor shows everything). */
  const allCustom = computed<CustomServerEvent[]>(() =>
    customEvents.value.length > 0 ? customEvents.value : shippedCustom.value,
  )

  function addCustom(e: Omit<CustomServerEvent, 'id'>) {
    const base = customEvents.value.length > 0 ? customEvents.value : shippedCustom.value
    customEvents.value = [...base, { ...e, id: generateId() }]
    saveOverrides(CUSTOM_KEY, customEvents.value)
  }

  function removeCustom(id: string) {
    const base = customEvents.value.length > 0 ? customEvents.value : shippedCustom.value
    customEvents.value = base.filter((e) => e.id !== id)
    saveOverrides(CUSTOM_KEY, customEvents.value)
  }

  function resetCustom() {
    customEvents.value = []
    localStorage.removeItem(CUSTOM_KEY)
  }

  /** Known server names: active server plus every server named in the schedule. */
  const knownServers = computed<string[]>(() => {
    const set = new Set<string>()
    for (const e of [...shippedRecurring.value, ...shippedCustom.value]) {
      for (const s of e.servers ?? []) set.add(s)
    }
    try {
      const active = useSettingsStore().settings.activeServerName
      if (active) set.add(active)
    } catch { /* store not ready */ }
    return [...set].sort()
  })

  return {
    upcoming,
    allRecurring,
    allCustom,
    knownServers,
    shippedLoaded: computed(() => loaded),
    ensureLoaded,
    addRecurring,
    removeRecurring,
    saveRecurring,
    resetRecurring,
    addCustom,
    removeCustom,
    resetCustom,
  }
}
