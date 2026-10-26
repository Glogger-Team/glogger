import { defineStore, acceptHMRUpdate } from 'pinia'
import { ref, computed } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'

export interface CoordinatorStatus {
  player_log_active: boolean
  chat_log_active: boolean
  active_character: string | null
  current_chat_log: string | null
  operation: string
}

export const useCoordinatorStore = defineStore('coordinator', () => {
  // ── State ──────────────────────────────────────────────────────────────────
  const status = ref<CoordinatorStatus>({
    player_log_active: false,
    chat_log_active: false,
    active_character: null,
    current_chat_log: null,
    operation: 'idle',
  })

  const newChatMessageCount = ref(0)
  const activeCharacter = computed(() => status.value.active_character)
  const isPlayerLogTailing = computed(() => status.value.player_log_active)
  const isChatLogTailing = computed(() => status.value.chat_log_active)
  const currentOperation = computed(() => status.value.operation)

  // ── Event Listeners ────────────────────────────────────────────────────────

  // Listen for coordinator status changes
  listen<CoordinatorStatus>('coordinator-status', (event) => {
    status.value = event.payload
  })

  // Listen for character login events
  listen<string>('character-login', (event) => {
    console.log('Character logged in:', event.payload)
  })

  // Listen for area transitions
  listen<string>('area-transition', (event) => {
    console.log('Area transition:', event.payload)
  })

  // Listen for chat message insertions
  listen<number>('chat-messages-inserted', (event) => {
    newChatMessageCount.value += event.payload
  })

  // ── Actions ────────────────────────────────────────────────────────────────

  /**
   * Start player log tailing (monitors Player.log for character/area changes)
   */
  async function startPlayerTailing(): Promise<void> {
    try {
      await invoke('start_player_tailing')
      await refreshStatus()
    } catch (e) {
      console.error('Failed to start player log tailing:', e)
      throw e
    }
  }

  /**
   * Stop player log tailing
   */
  async function stopPlayerTailing(): Promise<void> {
    try {
      await invoke('stop_player_tailing')
      await refreshStatus()
    } catch (e) {
      console.error('Failed to stop player log tailing:', e)
      throw e
    }
  }

  /**
   * Start chat log tailing (monitors daily chat logs)
   */
  async function startChatTailing(): Promise<void> {
    try {
      await invoke('start_chat_tailing')
      await refreshStatus()
    } catch (e) {
      console.error('Failed to start chat log tailing:', e)
      throw e
    }
  }

  /**
   * Stop chat log tailing
   */
  async function stopChatTailing(): Promise<void> {
    try {
      await invoke('stop_chat_tailing')
      await refreshStatus()
    } catch (e) {
      console.error('Failed to stop chat log tailing:', e)
      throw e
    }
  }

  /**
   * Force a one-shot catch-up poll of all active watchers. Drains everything
   * on disk from each watcher's saved position to end-of-file immediately,
   * instead of waiting for the next background poll cycle (or an app restart).
   */
  async function pollWatchers(): Promise<void> {
    try {
      await invoke('poll_watchers')
    } catch (e) {
      console.error('Failed to poll watchers:', e)
      throw e
    }
  }

  /**
   * Refresh coordinator status from backend
   */
  async function refreshStatus(): Promise<void> {
    try {
      const newStatus = await invoke<CoordinatorStatus>('get_coordinator_status')
      status.value = newStatus
    } catch (e) {
      console.error('Failed to get coordinator status:', e)
    }
  }

  /**
   * Start background polling on the Rust side (immune to WebView timer throttling)
   */
  async function startPolling(): Promise<void> {
    try {
      await invoke('start_background_polling')
    } catch (e) {
      console.error('Failed to start background polling:', e)
    }
  }

  /**
   * Stop background polling
   */
  async function stopPolling(): Promise<void> {
    try {
      await invoke('stop_background_polling')
    } catch (e) {
      console.error('Failed to stop background polling:', e)
    }
  }

  /**
   * Reset the new message count
   */
  function resetMessageCount(): void {
    newChatMessageCount.value = 0
  }

  // ── Initialization ─────────────────────────────────────────────────────────

  // Fetch initial status
  refreshStatus()

  return {
    // State
    status,
    newChatMessageCount,
    activeCharacter,
    isPlayerLogTailing,
    isChatLogTailing,
    currentOperation,
    // Actions
    startPlayerTailing,
    stopPlayerTailing,
    startChatTailing,
    stopChatTailing,
    refreshStatus,
    startPolling,
    stopPolling,
    pollWatchers,
    resetMessageCount,
  }
})

if (import.meta.hot) {
  import.meta.hot.accept(acceptHMRUpdate(useCoordinatorStore, import.meta.hot))
}
