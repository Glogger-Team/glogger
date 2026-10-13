import { ref, computed, watch, type Ref, type ComputedRef } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import type { ChatDay } from '../types/database'
import { getDisplayOffsetSeconds } from './useTimestamp'
import { displayDayBounds, displayDayOf } from '../utils/displayDay'

/**
 * Shared chat date navigation: play-day list, active day filter and the day
 * windows opened from it. Used by every Chat view that renders a
 * ChatMessageList backed by `get_chat_messages`.
 *
 * Days and their windows are bucketed in the viewer's DISPLAY timezone (see
 * `utils/displayDay`), not UTC, so a selected day matches the clock times
 * rendered inside it. A day window narrows `startTime`/`endTime` to that
 * day's UTC bounds, so ordinary offset pagination keeps working inside it.
 */

export interface ChatDateNav {
  /** Play days (display-timezone `YYYY-MM-DD`) with message counts, newest first */
  days: Ref<ChatDay[]>
  /** Currently selected day, or null for live/unfiltered browsing */
  activeDay: Ref<string | null>
  /** True when a day filter is active (viewing the past) */
  isPastView: ComputedRef<boolean>
  /** `startTime`/`endTime` derived from activeDay */
  dayFilter: ComputedRef<{ startTime: string, endTime: string } | null>
  /** Display-timezone offset (seconds east of UTC) the buckets are built from */
  offsetSeconds: ComputedRef<number>
  loadDays: () => Promise<void>
  jumpToDay: (day: string | null) => void
  /** Display-timezone `YYYY-MM-DD` of a message timestamp */
  dayOf: (timestamp: string) => string
  /** UTC bounds covering a display day, selected or not */
  boundsForDay: (day: string) => { startTime: string, endTime: string }
  /** dir 1 = older day, dir -1 = newer day */
  stepDay: (dir: 1 | -1) => void
  canStepDay: (dir: 1 | -1) => boolean
  /** Returns dayFilter fields to merge into a ChatFilter */
  filterParams: () => { startTime?: string, endTime?: string }
}

export function useChatDateNav(): ChatDateNav {
  const days = ref<ChatDay[]>([])
  const activeDay = ref<string | null>(null)
  // Display shift for the current Local/Server/UTC mode. Switching the mode
  // or the timezone override re-fetches the day list (already-open windows
  // refresh on the view's next load).
  const offsetSeconds = computed(() => getDisplayOffsetSeconds())

  async function loadDays() {
    try {
      days.value = await invoke<ChatDay[]>('get_chat_days', { offsetSeconds: offsetSeconds.value })
      // Drop the active day if it vanished (e.g. admin purge)
      if (activeDay.value && !days.value.some(d => d.day === activeDay.value)) {
        activeDay.value = null
      }
    } catch (e) {
      console.error('Failed to load chat days:', e)
    }
  }

  watch(offsetSeconds, () => { void loadDays() })

  const isPastView = computed(() => activeDay.value !== null)

  const dayFilter = computed(() =>
    activeDay.value ? displayDayBounds(activeDay.value, offsetSeconds.value) : null
  )

  function filterParams(): { startTime?: string, endTime?: string } {
    return dayFilter.value ?? {}
  }

  function boundsForDay(day: string) {
    return displayDayBounds(day, offsetSeconds.value)
  }

  function stepDay(dir: 1 | -1) {
    if (!activeDay.value || days.value.length === 0) return
    const idx = days.value.findIndex(d => d.day === activeDay.value)
    if (idx === -1) return
    const next = days.value[idx + dir] // days are newest-first
    if (next) activeDay.value = next.day
  }

  function canStepDay(dir: 1 | -1): boolean {
    if (!activeDay.value) return false
    const idx = days.value.findIndex(d => d.day === activeDay.value)
    if (idx === -1) return false
    // dir -1 = newer day (earlier in the newest-first list)
    return dir === -1 ? idx > 0 : idx < days.value.length - 1
  }

  function jumpToDay(day: string | null) {
    activeDay.value = day
  }

  return {
    days,
    activeDay,
    isPastView,
    dayFilter,
    offsetSeconds,
    loadDays,
    jumpToDay,
    dayOf: (timestamp: string) => displayDayOf(timestamp, offsetSeconds.value),
    boundsForDay,
    stepDay,
    canStepDay,
    filterParams,
  }
}
