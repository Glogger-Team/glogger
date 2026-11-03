import { defineStore } from 'pinia'
import { ref, computed, watch } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { useSettingsStore } from './settingsStore'
import type {
  StallStats,
  StallFilterOptions,
  StallEventsFilters,
} from '../types/stallTracker'

/**
 * Shared Stall Tracker state.
 *
 * The store deliberately holds **only** the cross-tab data (stats header
 * numbers + filter dropdown options) and a `dataVersion` counter that tabs
 * watch to know when to refetch their own row data. Per-tab queries
 * (sales list, revenue pivot, inventory snapshot) are owned by the tab
 * components, not here, so the store stays small and the heavy queries
 * fire only when the user actually opens the relevant tab.
 *
 * `dataVersion` is a monotonic counter (not a boolean) so two back-to-back
 * mutations always produce a new value tabs can react to.
 */
export const useStallTrackerStore = defineStore('stallTracker', () => {
  const settingsStore = useSettingsStore()

  const stats = ref<StallStats | null>(null)
  const filterOptions = ref<StallFilterOptions>({
    buyers: [],
    players: [],
    items: [],
    dates: [],
    actions: [],
  })
  const dataVersion = ref(0)
  /** Epoch ms of the last successful manual `rescan_stall_books` from the
   * parent action row. Tabs read this for the "Synced <label>" display —
   * it lives in the store (not the action row) so it survives tab switches
   * and HMR remounts of the parent view. */
  const lastSyncedAt = ref<number | null>(null)
  // Reactive clock so the relative label ages ("just now" → "2m ago")
  // without waiting for the next rescan. 30s cadence matches the Books
  // watcher; minute-granular display never needs per-second wakeups.
  // No cleanup: Pinia stores are session singletons (same as gameStateStore's
  // _clockInterval and updateStore's poller).
  const nowMs = ref(Date.now())
  if (typeof window !== 'undefined') {
    setInterval(() => {
      nowMs.value = Date.now()
    }, 30_000)
  }
  const lastSyncedLabel = computed(() => {
    if (lastSyncedAt.value == null) return ''
    void nowMs.value
    const secs = Math.max(0, Math.round((Date.now() - lastSyncedAt.value) / 1000))
    if (secs < 60) return 'just now'
    if (secs < 3600) return `${Math.floor(secs / 60)}m ago`
    return `${Math.floor(secs / 3600)}h ago`
  })
  const lastSyncedFull = computed(() =>
    lastSyncedAt.value == null ? '' : new Date(lastSyncedAt.value).toLocaleString(),
  )
  // Per-method request tokens. Every loadStats / loadFilterOptions call
  // captures the current token and the owner at issue time; on resume after
  // await, the response is discarded if either the token was superseded or
  // the active character changed. This prevents a slow query for char A
  // from clobbering a fresh query for char B during a rapid character
  // switch — see Phase 11 audit.
  let statsToken = 0
  let filterOptionsToken = 0

  /** Active character, normalized — empty/whitespace becomes `null`. */
  const currentOwner = computed<string | null>(() => {
    const name = settingsStore.settings.activeCharacterName
    return name && name.trim().length > 0 ? name : null
  })

  /** True when the current character has any persisted stall data. */
  const hasData = computed<boolean>(() => (stats.value?.total_sales ?? 0) > 0)

  async function loadStats(filters?: StallEventsFilters): Promise<void> {
    if (!currentOwner.value) {
      stats.value = null
      return
    }
    const token = ++statsToken
    const ownerAtCall = currentOwner.value
    try {
      const merged: StallEventsFilters = {
        ...(filters ?? {}),
        owner: ownerAtCall,
      }
      const result = await invoke<StallStats>('get_stall_stats', { filters: merged })
      if (token !== statsToken || ownerAtCall !== currentOwner.value) return
      stats.value = result
    } catch (e) {
      console.error('[stallTrackerStore] Failed to load stats:', e)
    }
  }

  async function loadFilterOptions(): Promise<void> {
    if (!currentOwner.value) {
      filterOptions.value = { buyers: [], players: [], items: [], dates: [], actions: [] }
      return
    }
    const token = ++filterOptionsToken
    const ownerAtCall = currentOwner.value
    try {
      const result = await invoke<StallFilterOptions>('get_stall_filter_options', {
        owner: ownerAtCall,
      })
      if (token !== filterOptionsToken || ownerAtCall !== currentOwner.value) return
      filterOptions.value = result
    } catch (e) {
      console.error('[stallTrackerStore] Failed to load filter options:', e)
    }
  }

  /** Toggle a single event's `ignored` flag. Bumps `dataVersion` on success.
   *
   * Does NOT refresh `stats` here: stats are recomputed by the active tab's
   * reload (triggered via the dataVersion watcher), with that tab's local
   * filter set threaded through. Refreshing stats here would briefly show
   * unfiltered numbers between this call and the tab's reload. */
  async function toggleIgnored(id: number, ignored: boolean): Promise<void> {
    if (!currentOwner.value) {
      throw new Error('No active character — cannot toggle ignored flag')
    }
    await invoke('toggle_stall_event_ignored', {
      id,
      ignored,
      owner: currentOwner.value,
    })
    dataVersion.value++
  }

  /** Delete every stall event for the active character. Returns row count. */
  async function clearAll(): Promise<number> {
    if (!currentOwner.value) {
      throw new Error('No active character — cannot clear stall data')
    }
    const deleted = await invoke<number>('clear_stall_events', {
      owner: currentOwner.value,
    })
    dataVersion.value++
    await Promise.all([loadStats(), loadFilterOptions()])
    return deleted
  }

  /** Public refresh — call this from the import command after a successful import. */
  async function refresh(): Promise<void> {
    dataVersion.value++
    await Promise.all([loadStats(), loadFilterOptions()])
  }

  // Character switch: invalidate stats, reload, and bump dataVersion so all
  // tabs refetch with the new owner. Each tab also watches currentOwner
  // directly to reset its own filter state — see plan §11.
  watch(currentOwner, () => {
    dataVersion.value++
    void loadStats()
    void loadFilterOptions()
  })

  // Coordinator side-channel: live-ingest fires `stall-events-updated` after
  // every successful batch insert. Player.log catch-up can replay many books
  // in a burst, so we debounce 500ms — a single trailing refresh is enough
  // and avoids spamming the backend with identical stats queries.
  //
  // The `listen()` returns an unlisten function that we deliberately discard.
  // Pinia stores are singletons in production and live for the full app
  // session, so there's nothing to clean up. This matches `coordinatorStore`
  // and the rest of the codebase's listener-in-store convention. During Vite
  // HMR the store may re-initialize and accumulate duplicate listeners — a
  // dev-only annoyance, never a production concern.
  let coordTimer: ReturnType<typeof setTimeout> | null = null
  void listen<number>('stall-events-updated', () => {
    if (coordTimer) clearTimeout(coordTimer)
    coordTimer = setTimeout(() => {
      dataVersion.value++
      void loadStats()
      void loadFilterOptions()
    }, 500)
  })

  // Dev-only: expose a bench helper on `window.stallBench` for DevTools-driven
  // scale testing per plan §14.5. `seed(100000)` should complete in a few
  // seconds on a modern machine; tab opens after that should remain sub-second.
  if (typeof window !== 'undefined' && import.meta.env.DEV) {
    ;(window as unknown as { stallBench: unknown }).stallBench = {
      seed: async (count: number) => {
        if (!currentOwner.value) {
          console.error('[stallBench] No active character — seed requires an owner.')
          return 0
        }
        const inserted = await invoke<number>('seed_stall_events_dev', {
          count,
          owner: currentOwner.value,
        })
        await refresh()
        console.log(`[stallBench] seeded ${inserted} events for ${currentOwner.value}`)
        return inserted
      },
      clear: async () => {
        const deleted = await clearAll()
        console.log(`[stallBench] cleared ${deleted} events`)
        return deleted
      },
      bump: () => {
        dataVersion.value++
      },
    }
  }

  return {
    stats,
    filterOptions,
    dataVersion,
    currentOwner,
    hasData,
    loadStats,
    loadFilterOptions,
    toggleIgnored,
    clearAll,
    refresh,
    lastSyncedAt,
    lastSyncedLabel,
    lastSyncedFull,
  }
})
