import { defineStore, acceptHMRUpdate } from 'pinia'
import { computed } from 'vue'
import { useCoordinatorStore } from './coordinatorStore'

/**
 * Chat store - now a thin wrapper around the coordinator store
 *
 * This maintains backward compatibility with existing components while
 * delegating actual tailing logic to the coordinator-managed architecture.
 */
export const useChatStore = defineStore('chat', () => {
  const coordinator = useCoordinatorStore()

  // ── Computed State (delegates to coordinator) ─────────────────────────────
  const tailing = computed(() => coordinator.isChatLogTailing)
  const newMessageCount = computed(() => coordinator.newChatMessageCount)
  const currentLogFile = computed(() => {
    if (coordinator.status.current_chat_log) {
      // Extract just the filename from the full path
      const parts = coordinator.status.current_chat_log.split(/[/\\]/)
      return parts[parts.length - 1]
    }
    return ''
  })

  // ── Actions (delegates to coordinator) ────────────────────────────────────

  async function startTailing() {
    await coordinator.startChatTailing()
  }

  function stopTailing() {
    coordinator.stopChatTailing()
  }

  function resetMessageCount() {
    coordinator.resetMessageCount()
  }

  return {
    tailing,
    currentLogFile,
    newMessageCount,
    startTailing,
    stopTailing,
    resetMessageCount,
  }
})

if (import.meta.hot) {
  import.meta.hot.accept(acceptHMRUpdate(useChatStore, import.meta.hot))
}
