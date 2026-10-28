import { defineStore, acceptHMRUpdate } from 'pinia'
import { ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { open } from '@tauri-apps/plugin-dialog'
import { useSettingsStore } from './settingsStore'
import { useGameStateStore } from './gameStateStore'
import type {
  CharacterInfo,
  CharacterSnapshotSummary,
  SnapshotSkillLevel,
  SnapshotSkillAbility,
  SnapshotNpcFavor,
  SnapshotRecipeCompletion,
  SnapshotStat,
  SnapshotCurrency,
  SnapshotActiveQuest,
  ImportResult,
  SkillDiff,
  InventorySnapshotSummary,
  SnapshotItem,
  InventoryImportResult,
  InventorySummary,
  CrossCharacterItem,
} from '../types/database'
import type { WorkOrderPresence } from '../types/crafting'

export const useCharacterStore = defineStore('character', () => {
  const characters = ref<CharacterInfo[]>([])
  const snapshots = ref<CharacterSnapshotSummary[]>([])
  const selectedCharacter = ref<CharacterInfo | null>(null)
  const selectedSnapshot = ref<CharacterSnapshotSummary | null>(null)
  const skills = ref<SnapshotSkillLevel[]>([])
  const snapshotAbilities = ref<SnapshotSkillAbility[]>([])
  const npcFavor = ref<SnapshotNpcFavor[]>([])
  const recipes = ref<SnapshotRecipeCompletion[]>([])
  const stats = ref<SnapshotStat[]>([])
  const currencies = ref<SnapshotCurrency[]>([])
  const activeQuests = ref<SnapshotActiveQuest[]>([])
  const skillDiffs = ref<SkillDiff[]>([])
  const lastImport = ref<ImportResult | null>(null)
  const error = ref<string | null>(null)
  const loading = ref(false)
  let reportWatchInterval: ReturnType<typeof setInterval> | null = null

  // Inventory state
  const inventorySnapshots = ref<InventorySnapshotSummary[]>([])
  const selectedInventorySnapshot = ref<InventorySnapshotSummary | null>(null)
  const inventoryItems = ref<SnapshotItem[]>([])
  const inventorySummary = ref<InventorySummary | null>(null)
  const lastInventoryImport = ref<InventoryImportResult | null>(null)

  // Cross-character storage search state (latest snapshot per character)
  const allCharacterItems = ref<CrossCharacterItem[]>([])
  // Invalidation generation: bumped on every cache clear so an in-flight
  // load started before the invalidation drops its stale result instead of
  // committing it (server switch / import / delete mid-await).
  const allCharacterItemsGeneration = ref(0)
  const allCharacterItemsLoading = ref(false)
  const allCharacterItemsServer = ref<string | undefined>(undefined)
  const allCharacterItemsLoaded = ref(false)

  // Cross-character work-order presence (latest snapshot per character)
  const allCharacterWorkOrders = ref<WorkOrderPresence[]>([])
  const allCharacterWorkOrdersLoading = ref(false)
  const allCharacterWorkOrdersServer = ref<string | undefined>(undefined)
  // Same generation guard as the items cache above.
  const allCharacterWorkOrdersGeneration = ref(0)
  const allCharacterWorkOrdersLoaded = ref(false)

  async function importCharacterReport(): Promise<ImportResult | null> {
    const settingsStore = useSettingsStore()
    error.value = null

    const filePath = await open({
      filters: [{ name: 'Character Report', extensions: ['json'] }],
      defaultPath: settingsStore.settings.gameDataPath || undefined,
    })

    if (!filePath) return null

    try {
      loading.value = true
      const result = await invoke<ImportResult>('import_character_report', {
        filePath: filePath as string,
      })
      lastImport.value = result
      // WO presence derives from the latest report — invalidate the cache.
      allCharacterWorkOrdersLoaded.value = false
      allCharacterWorkOrdersGeneration.value++


      // Refresh character list after import
      await loadCharacters()

      // Refresh game state domains seeded by character import
      const gameState = useGameStateStore()
      await gameState.refreshDomain('storage')
      await gameState.refreshDomain('favor')

      return result
    } catch (e) {
      error.value = String(e)
      return null
    } finally {
      loading.value = false
    }
  }

  async function loadCharacters() {
    try {
      characters.value = await invoke<CharacterInfo[]>('get_characters')
    } catch (e) {
      error.value = String(e)
    }
  }

  async function initForActiveCharacter() {
    const settingsStore = useSettingsStore()
    const characterName = settingsStore.settings.activeCharacterName
    const serverName = settingsStore.settings.activeServerName

    if (!characterName) return

    error.value = null
    loading.value = true

    try {
      // Try to auto-import the latest report from the Reports folder
      await invoke('import_latest_report_for_character', { characterName, serverName: serverName || undefined })
      // Report import seeds game state — refresh domains so the UI has the data
      const gameState = useGameStateStore()
      await Promise.all([
        gameState.refreshDomain('favor'),
        gameState.refreshDomain('storage'),
      ])
      // WO presence derives from the latest report — invalidate the cache.
      allCharacterWorkOrdersLoaded.value = false
      allCharacterWorkOrdersGeneration.value++
    } catch (e) {
      // Non-fatal — the report may not exist on disk
      console.warn('Auto-import latest report:', e)
    }

    try {
      // Load all characters and find the active one
      await loadCharacters()
      const activeChar = characters.value.find(
        c => c.character_name === characterName && (!serverName || c.server_name === serverName)
      )

      if (activeChar) {
        selectedCharacter.value = activeChar
        await loadSnapshots(activeChar.character_name, activeChar.server_name)

        // Auto-select the most recent snapshot
        if (snapshots.value.length > 0) {
          await selectSnapshot(snapshots.value[0])
        }
      }
    } catch (e) {
      error.value = String(e)
    } finally {
      loading.value = false
    }

    // Load inventory data and import reports for all other characters on
    // this server in parallel — both must complete before the app goes interactive
    // so the aggregate view and inventory views have data.
    const parallelTasks: Promise<void>[] = [initInventoryForActiveCharacter()]

    if (serverName) {
      parallelTasks.push(
        invoke<number>('import_reports_for_server', { serverName }).then(async (count) => {
          if (count > 0) {
            // Imported reports are the WO source data — invalidate so
            // workOrderStateFor recomputes instead of serving stale lists.
            allCharacterWorkOrdersLoaded.value = false
            allCharacterWorkOrdersGeneration.value++
            await loadCharacters()
          }
        }).catch((e) => {
          console.warn('Server-wide report import:', e)
        })
      )
    }

    await Promise.all(parallelTasks)
  }

  async function selectCharacter(character: CharacterInfo) {
    selectedCharacter.value = character
    selectedSnapshot.value = null
    skills.value = []
    snapshotAbilities.value = []
    npcFavor.value = []
    recipes.value = []
    stats.value = []
    currencies.value = []
    activeQuests.value = []
    skillDiffs.value = []
    await loadSnapshots(character.character_name, character.server_name)
  }

  async function loadSnapshots(characterName: string, serverName?: string) {
    try {
      snapshots.value = await invoke<CharacterSnapshotSummary[]>(
        'get_character_snapshots',
        { characterName, serverName },
      )
    } catch (e) {
      error.value = String(e)
    }
  }

  async function selectSnapshot(snapshot: CharacterSnapshotSummary) {
    selectedSnapshot.value = snapshot
    skillDiffs.value = []
    snapshotAbilities.value = []
    await loadSnapshotDetails(snapshot.id)
  }

  async function loadSnapshotDetails(snapshotId: number) {
    try {
      loading.value = true
      const [skillsResult, abilitiesResult, favorResult, recipesResult, statsResult, currenciesResult, questsResult] =
        await Promise.all([
          invoke<SnapshotSkillLevel[]>('get_snapshot_skills', { snapshotId }),
          invoke<SnapshotSkillAbility[]>('get_snapshot_skill_abilities', { snapshotId }),
          invoke<SnapshotNpcFavor[]>('get_snapshot_npc_favor', { snapshotId }),
          invoke<SnapshotRecipeCompletion[]>('get_snapshot_recipes', { snapshotId }),
          invoke<SnapshotStat[]>('get_snapshot_stats', { snapshotId }),
          invoke<SnapshotCurrency[]>('get_snapshot_currencies', { snapshotId }),
          invoke<SnapshotActiveQuest[]>('get_snapshot_active_quests', { snapshotId }),
        ])
      skills.value = skillsResult
      snapshotAbilities.value = abilitiesResult
      npcFavor.value = favorResult
      recipes.value = recipesResult
      stats.value = statsResult
      currencies.value = currenciesResult
      activeQuests.value = questsResult
    } catch (e) {
      error.value = String(e)
    } finally {
      loading.value = false
    }
  }

  async function compareSnapshots(oldId: number, newId: number) {
    try {
      skillDiffs.value = await invoke<SkillDiff[]>('compare_snapshots', {
        snapshotIdOld: oldId,
        snapshotIdNew: newId,
      })
    } catch (e) {
      error.value = String(e)
    }
  }

  async function pollForNewReports() {
    const settingsStore = useSettingsStore()
    const characterName = settingsStore.settings.activeCharacterName
    if (!characterName) return

    try {
      const serverName = settingsStore.settings.activeServerName
      const result = await invoke<ImportResult | null>(
        'import_latest_report_for_character',
        { characterName, serverName: serverName || undefined },
      )

      if (result) {
        // New report was imported — refresh snapshot list and auto-select newest
        lastImport.value = result
        // WOs come from the latest report — invalidate so tooltips refresh.
        allCharacterWorkOrdersLoaded.value = false
      allCharacterWorkOrdersGeneration.value++
        // A fresh export re-anchors the council-wallet estimate (delta reset) —
        // pull the new anchor so the dashboard figure reflects it immediately.
        import('./gameStateStore').then(({ useGameStateStore }) => {
          useGameStateStore().fetchCurrencyEstimate()
        })
        await loadCharacters()
        const activeChar = characters.value.find(
          c => c.character_name === characterName && (!serverName || c.server_name === serverName)
        )
        if (activeChar) {
          selectedCharacter.value = activeChar
          await loadSnapshots(activeChar.character_name, activeChar.server_name)
          if (snapshots.value.length > 0) {
            await selectSnapshot(snapshots.value[0])
          }
        }
      }
    } catch (e) {
      // Silent — polling errors shouldn't disrupt the UI
      console.warn('Report watch poll error:', e)
    }

    // Also poll for inventory reports
    try {
      const serverName = settingsStore.settings.activeServerName
      const invResult = await invoke<InventoryImportResult | null>(
        'import_latest_inventory_for_character',
        { characterName, serverName: serverName || undefined },
      )

      if (invResult) {
        lastInventoryImport.value = invResult
        allCharacterItemsLoaded.value = false
      allCharacterItemsGeneration.value++
        await loadInventorySnapshots(characterName, serverName || undefined)
        if (inventorySnapshots.value.length > 0) {
          await selectInventorySnapshot(inventorySnapshots.value[0])
        }
        // Auto-scan for new brewing discoveries (non-blocking)
        if (!invResult.was_duplicate) {
          import('./breweryStore').then(({ useBreweryStore }) => {
            useBreweryStore().onInventoryImported(invResult.character_name)
          })
        }
      }
    } catch (e) {
      console.warn('Inventory watch poll error:', e)
    }
  }

  function startReportWatching() {
    stopReportWatching()
    const settingsStore = useSettingsStore()
    if (!settingsStore.settings.autoWatchReports) return

    const intervalMs = settingsStore.settings.reportWatchIntervalSeconds * 1000
    reportWatchInterval = setInterval(pollForNewReports, intervalMs)
  }

  function stopReportWatching() {
    if (reportWatchInterval !== null) {
      clearInterval(reportWatchInterval)
      reportWatchInterval = null
    }
  }

  // Listen for chat-detected report saves — triggers an immediate import
  // instead of waiting for the next polling cycle.
  listen<string>('report-saved', (event) => {
    console.log('[characterStore] Report saved detected via chat:', event.payload)
    pollForNewReports()
  })

  // ── Inventory Actions ──────────────────────────────────────────────────────

  async function importInventoryReport(): Promise<InventoryImportResult | null> {
    const settingsStore = useSettingsStore()
    error.value = null

    const filePath = await open({
      filters: [{ name: 'Inventory Report', extensions: ['json'] }],
      defaultPath: settingsStore.settings.gameDataPath || undefined,
    })

    if (!filePath) return null

    try {
      loading.value = true
      const result = await invoke<InventoryImportResult>('import_inventory_report', {
        filePath: filePath as string,
      })
      // Items come from the latest reports — invalidate the cross-character cache.
      lastInventoryImport.value = result
      allCharacterItemsLoaded.value = false
      allCharacterItemsGeneration.value++

      // Refresh inventory snapshots after import
      if (selectedCharacter.value) {
        await loadInventorySnapshots(
          selectedCharacter.value.character_name,
          selectedCharacter.value.server_name,
        )
      }

      // Refresh game state domains (inventory import seeds both game_state_storage and game_state_inventory)
      if (!result.was_duplicate) {
        const gameState = useGameStateStore()
        await Promise.all([
          gameState.refreshDomain('storage'),
          gameState.refreshDomain('inventory'),
        ])

        // Auto-scan for new brewing discoveries (non-blocking)
        const { useBreweryStore } = await import('./breweryStore')
        useBreweryStore().onInventoryImported(result.character_name)
      }

      return result
    } catch (e) {
      error.value = String(e)
      return null
    } finally {
      loading.value = false
    }
  }

  async function loadInventorySnapshots(characterName: string, serverName?: string) {
    try {
      inventorySnapshots.value = await invoke<InventorySnapshotSummary[]>(
        'get_inventory_snapshots',
        { characterName, serverName },
      )
    } catch (e) {
      error.value = String(e)
    }
  }

  async function selectInventorySnapshot(snapshot: InventorySnapshotSummary) {
    selectedInventorySnapshot.value = snapshot
    try {
      loading.value = true
      const [items, summary] = await Promise.all([
        invoke<SnapshotItem[]>('get_snapshot_items', { snapshotId: snapshot.id }),
        invoke<InventorySummary>('get_inventory_summary', { snapshotId: snapshot.id }),
      ])
      inventoryItems.value = items
      inventorySummary.value = summary
    } catch (e) {
      error.value = String(e)
    } finally {
      loading.value = false
    }
  }

  async function initInventoryForActiveCharacter() {
    const settingsStore = useSettingsStore()
    const characterName = settingsStore.settings.activeCharacterName
    const serverName = settingsStore.settings.activeServerName
    if (!characterName) return

    // Try auto-import
    try {
      const invResult = await invoke<InventoryImportResult | null>('import_latest_inventory_for_character', { characterName, serverName: serverName || undefined })
      if (invResult) {
        allCharacterItemsLoaded.value = false
        allCharacterItemsGeneration.value++
      }
      // Auto-scan for new brewing discoveries (non-blocking)
      if (invResult && !invResult.was_duplicate) {
        import('./breweryStore').then(({ useBreweryStore }) => {
          useBreweryStore().onInventoryImported(invResult.character_name)
        })
      }
    } catch (e) {
      console.warn('Auto-import inventory:', e)
    }

    // Load snapshots for the active character
    await loadInventorySnapshots(characterName, serverName || undefined)

    // Auto-select most recent snapshot
    if (inventorySnapshots.value.length > 0) {
      await selectInventorySnapshot(inventorySnapshots.value[0])
    }
  }

  /**
   * Load the latest inventory snapshot of every character on the active server
   * for the cross-character storage search. First backfills inventory reports
   * for characters that haven't been loaded recently (best-effort), then pulls
   * the aggregated item set. Caches results; pass `force` to refresh.
   */
  async function loadAllCharacterItems(force = false) {
    const settingsStore = useSettingsStore()
    const serverName = settingsStore.settings.activeServerName || undefined
    // No server selected: never fetch unfiltered — clear the cache so the
    // search shows nothing instead of all servers' rows.
    if (!serverName) {
      allCharacterItems.value = []
      allCharacterItemsLoaded.value = false
      allCharacterItemsGeneration.value++
      allCharacterItemsServer.value = undefined
      return
    }
    const cacheMatchesServer = allCharacterItemsServer.value === serverName
    if (allCharacterItemsLoaded.value && cacheMatchesServer && !force) return

    allCharacterItemsLoading.value = true
    // Clear stale rows before the async fetch/backfill so consumers never
    // render the previous server's rows while the new query is in flight.
    allCharacterItems.value = []
    const generation = allCharacterItemsGeneration.value
    try {
      // Backfill inventory reports for every character on this server so the
      // search sees characters the user hasn't opened recently. Best-effort —
      // a missing Reports folder or file just yields fewer results.
      // Not repeated on retry (fetchAllCharacterItems below re-reads only):
      // re-scanning report files there could restore a character deleted
      // mid-flight (delete clears DB rows only; report files stay on disk).
      try {
        await invoke<number>('import_all_inventory_for_server', { serverName })
      } catch (e) {
        console.warn('Cross-character inventory backfill:', e)
      }

      await fetchAllCharacterItems(serverName, generation)
    } catch (e) {
      error.value = String(e)
      // Failed fetch must not poison the cache: allow the next call through.
      allCharacterItemsLoaded.value = false
      allCharacterItemsGeneration.value++
    } finally {
      allCharacterItemsLoading.value = false
    }
  }

  /**
   * Read-only half of the cross-character load: query + guard, no backfill.
   * Split out so the invalidation retry re-reads without re-scanning report
   * files (see the delete-restores-files note above).
   */
  async function fetchAllCharacterItems(serverName: string, generation: number) {
    const settingsStore = useSettingsStore()
    const rows = await invoke<CrossCharacterItem[]>('get_all_character_items', {
      serverName,
    })
    // Drop the result if the server changed or the cache was invalidated
    // mid-flight — committing it would poison the new scope with old rows.
    if (settingsStore.settings.activeServerName !== serverName) return
    if (generation !== allCharacterItemsGeneration.value) {
      // Same server, invalidated mid-flight: re-read without backfill.
      // Converges — each retry re-resolves generation; imports are bounded.
      // Awaited so the caller's finally holds the loading flag.
      await fetchAllCharacterItems(serverName, allCharacterItemsGeneration.value)
      return
    }
    allCharacterItems.value = rows
    allCharacterItemsLoaded.value = true
    allCharacterItemsServer.value = serverName
  }

  /**
   * Load cross-character work-order presence from every character's latest
   * snapshot on the active server. Caches results; pass `force` to refresh.
   */
  async function loadAllCharacterWorkOrders(force = false) {
    const settingsStore = useSettingsStore()
    const serverName = settingsStore.settings.activeServerName || undefined
    // No server selected: never fetch unfiltered (backend treats an omitted
    // filter as all servers) — clear the cache so tooltips show nothing.
    if (!serverName) {
      allCharacterWorkOrders.value = []
      allCharacterWorkOrdersLoaded.value = false
      allCharacterWorkOrdersGeneration.value++
      allCharacterWorkOrdersServer.value = undefined
      return
    }
    const cacheMatchesServer = allCharacterWorkOrdersServer.value === serverName
    if (allCharacterWorkOrdersLoaded.value && cacheMatchesServer && !force) return

    allCharacterWorkOrdersLoading.value = true
    // Clear stale rows before the async fetch so tooltips never render the
    // previous server's rows while the new query is in flight.
    allCharacterWorkOrders.value = []
    const generation = allCharacterWorkOrdersGeneration.value
    try {
      await fetchAllCharacterWorkOrders(serverName, generation)
    } catch (e) {
      error.value = String(e)
      // Failed fetch leaves rows empty (pre-cleared above): allow the next
      // call through so the retry repopulates. Tooltips show nothing until then.
      allCharacterWorkOrdersLoaded.value = false
      allCharacterWorkOrdersGeneration.value++
    } finally {
      allCharacterWorkOrdersLoading.value = false
    }
  }

  /**
   * Read-only half of the cross-character work-order load: query + guard.
   * Split out so a same-server mid-flight invalidation retries the read
   * without leaving the cache empty (mirrors fetchAllCharacterItems).
   */
  async function fetchAllCharacterWorkOrders(serverName: string, generation: number) {
    const settingsStore = useSettingsStore()
    const rows = await invoke<WorkOrderPresence[]>('get_all_character_work_orders', {
      serverName,
    })
    // Drop the result if the server changed during the await — committing it
    // would poison the new scope with old rows.
    if (settingsStore.settings.activeServerName !== serverName) return
    if (generation !== allCharacterWorkOrdersGeneration.value) {
      // Same server, invalidated mid-flight: re-read without backfill.
      // Converges — each retry re-resolves generation; imports are bounded.
      // Awaited so the caller's finally holds the loading flag.
      await fetchAllCharacterWorkOrders(serverName, allCharacterWorkOrdersGeneration.value)
      return
    }
    allCharacterWorkOrders.value = rows
    allCharacterWorkOrdersLoaded.value = true
    allCharacterWorkOrdersServer.value = serverName
  }

  /**
   * Cross-character work-order state for a quest key, or null when no data
   * exists. Callers render the `Done/Active` line only on non-null.
   */
  function workOrderStateFor(questKey: string | null | undefined): { completedBy: string[]; activeBy: string[] } | null {
    if (!questKey) return null
    // Cache is server-scoped; without a selection there is nothing valid to show.
    // Rows carry their own server; skip any that do not match, so a stale
    // cache from another server can never render under this one.
    const serverName = useSettingsStore().settings.activeServerName
    if (!serverName) return null
    if (!allCharacterWorkOrdersLoaded.value || allCharacterWorkOrdersServer.value !== serverName)
      return null
    const completedBy: string[] = []
    const activeBy: string[] = []
    for (const row of allCharacterWorkOrders.value) {
      if (row.quest_key !== questKey) continue
      if (row.server_name !== serverName) continue
      if (row.category === 'completed_work_order') completedBy.push(row.character_name)
      else if (row.category === 'work_order') activeBy.push(row.character_name)
    }
    if (completedBy.length === 0 && activeBy.length === 0) return null
    return { completedBy, activeBy }
  }

  return {
    characters,
    snapshots,
    selectedCharacter,
    selectedSnapshot,
    skills,
    snapshotAbilities,
    npcFavor,
    recipes,
    stats,
    currencies,
    activeQuests,
    skillDiffs,
    lastImport,
    error,
    loading,
    importCharacterReport,
    loadCharacters,
    initForActiveCharacter,
    selectCharacter,
    loadSnapshots,
    selectSnapshot,
    loadSnapshotDetails,
    compareSnapshots,
    startReportWatching,
    stopReportWatching,
    // Inventory
    inventorySnapshots,
    selectedInventorySnapshot,
    inventoryItems,
    inventorySummary,
    lastInventoryImport,
    importInventoryReport,
    loadInventorySnapshots,
    selectInventorySnapshot,
    initInventoryForActiveCharacter,
    // Cross-character storage search
    allCharacterItems,
    allCharacterItemsLoading,
    allCharacterItemsGeneration,
    allCharacterItemsServer,
    allCharacterItemsLoaded,
    loadAllCharacterItems,
    // Cross-character work-order presence
    allCharacterWorkOrders,
    allCharacterWorkOrdersLoading,
    allCharacterWorkOrdersGeneration,
    allCharacterWorkOrdersServer,
    allCharacterWorkOrdersLoaded,
    loadAllCharacterWorkOrders,
    workOrderStateFor,
  }
})

if (import.meta.hot) {
  import.meta.hot.accept(acceptHMRUpdate(useCharacterStore, import.meta.hot))
}
