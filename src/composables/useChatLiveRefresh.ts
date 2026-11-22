import { onMounted, onUnmounted, ref, type Ref } from 'vue'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'

/**
 * Live-refresh hook for the chat message tabs.
 *
 * Listens for the backend `chat-messages-inserted` event (emitted per drain
 * batch with the inserted row count). On each event, when the view is parked
 * at the live head (`sortOrder` desc, `offset` 0, active day latest-or-unset)
 * it calls `reset()` to refetch through the view's own loader — which stays
 * under the existing `useChatRequestGuard` generation guard. Otherwise the
 * batch count accumulates in `pendingCount` for the view's N-new-messages
 * pill, whose click calls the same `reset()`.
 *
 * The listener is removed on unmount.
 */
export function useChatLiveRefresh(options: {
  sortOrder: Ref<'asc' | 'desc'>
  offset: Ref<number>
  dateNav: { activeDay: Ref<string | null>, days: Ref<Array<{ day: string }>> }
  reset: () => void
}) {
  const pendingCount = ref(0)
  let unlisten: UnlistenFn | null = null

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
      && options.offset.value === 0
      && isLatestDay()
  }

  onMounted(async () => {
    unlisten = await listen<number>('chat-messages-inserted', (event) => {
      const count = typeof event.payload === 'number' ? event.payload : 1
      if (isLiveHead()) {
        options.reset()
      } else {
        pendingCount.value += count
      }
    })
  })

  onUnmounted(() => {
    if (unlisten) unlisten()
  })

  return { pendingCount }
}
