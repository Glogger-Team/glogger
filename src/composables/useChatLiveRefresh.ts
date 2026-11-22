import { onMounted, onUnmounted, ref, type Ref } from 'vue'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'

/**
 * Live-refresh hook for the chat message tabs.
 *
 * Listens for the backend `chat-messages-inserted` event (emitted per drain
 * batch with the inserted row count). Each batch first refreshes the day list
 * (a new-day arrival postdates the cached list), then reloads the sidebar
 * list via `onInsert` when the view provides one. When the view is parked at
 * the live head (descending order, no older pages loaded, active day
 * latest-or-unset) the hook calls `reset()` to refetch through the view's own
 * loader — which stays under the existing `useChatRequestGuard` generation
 * guard. Otherwise the batch count accumulates in `pendingCount` for the
 * view's N-new-messages pill, whose click calls `showNewest()`: plain
 * `reset()` would repeat the stale window (oldest page under asc order, or
 * the selected past day) and clear the count without showing the messages.
 *
 * `hasOlderPages` is view-owned pagination state (set in the load-more path,
 * cleared in `reset()`/selection changes): the raw `offset` cannot detect the
 * newest page because loaders advance it on every successful load, including
 * the first one. The listener is removed on unmount; a registration that
 * resolves after unmount is released immediately.
 */
export function useChatLiveRefresh(options: {
  sortOrder: Ref<'asc' | 'desc'>
  hasOlderPages: Ref<boolean>
  dateNav: {
    activeDay: Ref<string | null>
    days: Ref<Array<{ day: string }>>
    loadDays: () => Promise<void>
    jumpToDay: (day: string | null) => void
  }
  reset: () => void
  onInsert?: () => void
}) {
  const pendingCount = ref(0)
  let unlisten: UnlistenFn | null = null
  let mounted = true

  /** True when viewing the newest day (or no day filter at all). */
  function isLatestDay(): boolean {
    const active = options.dateNav.activeDay.value
    if (!active) return true
    const days = options.dateNav.days.value
    if (days.length === 0) return true
    return days[0].day === active
  }

  function isLiveHead(): boolean {
    return options.sortOrder.value === 'desc'
      && !options.hasOlderPages.value
      && isLatestDay()
  }

  /** Pending-pill action: jump to the newest window and reload it. */
  function showNewest(): void {
    pendingCount.value = 0
    options.sortOrder.value = 'desc'
    if (options.dateNav.activeDay.value !== null) {
      // The day watcher restarts the window for the unfiltered live view.
      options.dateNav.jumpToDay(null)
    } else {
      options.reset()
    }
  }

  async function handleInsert(count: number): Promise<void> {
    try {
      await options.dateNav.loadDays()
    } catch {
      // Fall through with the cached day list.
    }
    options.onInsert?.()
    if (!mounted) return
    if (isLiveHead()) {
      options.reset()
    } else {
      pendingCount.value += count
    }
  }

  onMounted(async () => {
    const stop = await listen<number>('chat-messages-inserted', (event) => {
      void handleInsert(typeof event.payload === 'number' ? event.payload : 1)
    })
    if (mounted) {
      unlisten = stop
    } else {
      stop()
    }
  })

  onUnmounted(() => {
    mounted = false
    if (unlisten) unlisten()
  })

  return { pendingCount, showNewest }
}
