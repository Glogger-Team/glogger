import { defineStore, acceptHMRUpdate } from 'pinia'
import { ref, computed } from 'vue'

export interface UserTimer {
  id: string
  label: string
  /** Duration in seconds */
  durationSecs: number
  /** Unix timestamp (ms) when timer ends — null if paused */
  endsAt: number | null
  /** Remaining seconds when paused — null if running */
  pausedRemaining: number | null
  /** Whether the timer was created from a preset */
  presetId?: string
}

export interface TimerPreset {
  id: string
  label: string
  /** Duration in seconds */
  durationSecs: number
}

const STORAGE_KEY = 'glogger-user-timers'
const PRESETS_KEY = 'glogger-timer-presets'

const DEFAULT_PRESETS: TimerPreset[] = [
  { id: 'pudding', label: 'Pudding', durationSecs: 3 * 60 * 60 },
]

function loadTimers(): UserTimer[] {
  try {
    const raw = localStorage.getItem(STORAGE_KEY)
    return raw ? JSON.parse(raw) : []
  } catch {
    return []
  }
}

function loadPresets(): TimerPreset[] {
  try {
    const raw = localStorage.getItem(PRESETS_KEY)
    return raw ? JSON.parse(raw) : [...DEFAULT_PRESETS]
  } catch {
    return [...DEFAULT_PRESETS]
  }
}

function saveTimers(timers: UserTimer[]) {
  localStorage.setItem(STORAGE_KEY, JSON.stringify(timers))
}

function savePresets(presets: TimerPreset[]) {
  localStorage.setItem(PRESETS_KEY, JSON.stringify(presets))
}

function generateId(): string {
  return Date.now().toString(36) + Math.random().toString(36).slice(2, 6)
}

export const useTimerStore = defineStore('user-timers', () => {
  const timers = ref<UserTimer[]>(loadTimers())
  const presets = ref<TimerPreset[]>(loadPresets())
  const now = ref(Date.now())

  let tickInterval: ReturnType<typeof setInterval> | null = null

  function startTicking() {
    if (tickInterval) return
    tickInterval = setInterval(() => {
      now.value = Date.now()
    }, 1000)
  }

  function stopTicking() {
    if (tickInterval) {
      clearInterval(tickInterval)
      tickInterval = null
    }
  }

  /** Get remaining seconds for a timer (negative = expired) */
  function getRemaining(timer: UserTimer): number {
    if (timer.pausedRemaining != null) return timer.pausedRemaining
    if (timer.endsAt == null) return 0
    return Math.ceil((timer.endsAt - now.value) / 1000)
  }

  /** All timers with computed remaining time, sorted: running first, then expired */
  const timersWithRemaining = computed(() => {
    return timers.value.map(t => ({
      ...t,
      remaining: getRemaining(t),
      isRunning: t.endsAt != null && t.pausedRemaining == null,
      isPaused: t.pausedRemaining != null,
      isExpired: t.endsAt != null && t.pausedRemaining == null && getRemaining(t) <= 0,
    })).sort((a, b) => {
      // Expired first, then running (nearest expiry first), then paused
      if (a.isExpired && !b.isExpired) return -1
      if (!a.isExpired && b.isExpired) return 1
      if (a.isRunning && !b.isRunning) return -1
      if (!a.isRunning && b.isRunning) return 1
      return a.remaining - b.remaining
    })
  })

  const hasActiveTimers = computed(() =>
    timers.value.some(t => t.endsAt != null && t.pausedRemaining == null)
  )

  function addTimer(label: string, durationSecs: number, presetId?: string) {
    const timer: UserTimer = {
      id: generateId(),
      label,
      durationSecs,
      endsAt: Date.now() + durationSecs * 1000,
      pausedRemaining: null,
      presetId,
    }
    timers.value.push(timer)
    saveTimers(timers.value)
    startTicking()
  }

  function pauseTimer(id: string) {
    const timer = timers.value.find(t => t.id === id)
    if (!timer || timer.endsAt == null || timer.pausedRemaining != null) return
    timer.pausedRemaining = Math.max(0, Math.ceil((timer.endsAt - Date.now()) / 1000))
    timer.endsAt = null
    saveTimers(timers.value)
  }

  function resumeTimer(id: string) {
    const timer = timers.value.find(t => t.id === id)
    if (!timer || timer.pausedRemaining == null) return
    timer.endsAt = Date.now() + timer.pausedRemaining * 1000
    timer.pausedRemaining = null
    saveTimers(timers.value)
    startTicking()
  }

  function restartTimer(id: string) {
    const timer = timers.value.find(t => t.id === id)
    if (!timer) return
    timer.endsAt = Date.now() + timer.durationSecs * 1000
    timer.pausedRemaining = null
    saveTimers(timers.value)
    startTicking()
  }

  function removeTimer(id: string) {
    timers.value = timers.value.filter(t => t.id !== id)
    saveTimers(timers.value)
    if (!hasActiveTimers.value) stopTicking()
  }

  function addPreset(label: string, durationSecs: number) {
    presets.value.push({ id: generateId(), label, durationSecs })
    savePresets(presets.value)
  }

  function removePreset(id: string) {
    presets.value = presets.value.filter(p => p.id !== id)
    savePresets(presets.value)
  }

  function resetPresets() {
    presets.value = [...DEFAULT_PRESETS]
    savePresets(presets.value)
  }

  // Start ticking if there are active timers on load
  if (hasActiveTimers.value) startTicking()

  return {
    timers,
    presets,
    now,
    timersWithRemaining,
    hasActiveTimers,
    addTimer,
    pauseTimer,
    resumeTimer,
    restartTimer,
    removeTimer,
    addPreset,
    removePreset,
    resetPresets,
    startTicking,
    stopTicking,
  }
})

if (import.meta.hot) {
  import.meta.hot.accept(acceptHMRUpdate(useTimerStore, import.meta.hot))
}
