import { computed, ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { useGameStateStore } from '../stores/gameStateStore'
import { useSettingsStore } from '../stores/settingsStore'
import { useStatehelmTracker, MAX_GIFTS_PER_WEEK } from './useStatehelmTracker'
import { CASINO_PORTAL_BY_PHASE, type MoonPhaseName } from './useMoonPhase'

/** One vault→NPC gift candidate from the backend matcher. */
export interface StatehelmGiftMatch {
  item_name: string
  item_type_id: number | null
  vault_key: string
  stack_size: number
  npc_key: string
  npc_name: string
  desire: string
  pref: number
  match_type: string
}

/** A match claimed by the greedy assignment for one NPC. */
export interface GiftAssignment {
  match: StatehelmGiftMatch
  /** CDN area key of the vault (for routing); null when unknown. */
  vaultArea: string | null
  /** Human-readable vault label. */
  vaultLabel: string
}

/** Assignments grouped under one vault for the match table. */
export interface VaultGiftGroup {
  vaultKey: string
  vaultLabel: string
  vaultArea: string | null
  assignments: GiftAssignment[]
}

export interface PlannedRouteStep {
  zone: string
  action: string
  details: string
}

export interface PlannedRoute {
  steps: PlannedRouteStep[]
  total_hops: number
}

interface RouteStop {
  zone: string
  purpose: string
  details: string
}

// Friendly area names (from live game state) to CDN zone keys — mirrors the
// TripPlannerWidget table so pickup stops resolve the same way.
const FRIENDLY_TO_KEY: Record<string, string> = {
  'Anagoge Island': 'AreaNewbieIsland',
  Serbule: 'AreaSerbule',
  'Serbule Hills': 'AreaSerbule2',
  Eltibule: 'AreaEltibule',
  'Sun Vale': 'AreaSunVale',
  'Kur Mountains': 'AreaKurMountains',
  'Red Wing Casino': 'AreaCasino',
  Ilmari: 'AreaDesert1',
  'Ilmari Desert': 'AreaDesert1',
  Rahu: 'AreaRahu',
  Gazluk: 'AreaGazluk',
  'Gazluk Plateau': 'AreaGazluk',
  'Fae Realm': 'AreaFaeRealm1',
  Povus: 'AreaPovus',
  Vidaria: 'AreaVidaria',
  Statehelm: 'AreaStatehelm',
  'Winter Nexus': 'AreaPlanes',
  'Existential Planes': 'AreaPlanes',
}

interface TripPlannerWidgetConfig {
  primaryBind: string | null
  secondaryBind: string | null
  mushroomCircle: string | null
}

function loadTravelConfig(): Record<string, string | boolean | null> {
  let saved: Partial<TripPlannerWidgetConfig> = {}
  try {
    const raw = localStorage.getItem('tripPlannerWidget.config')
    if (raw) saved = { ...saved, ...JSON.parse(raw) }
  } catch { /* ignore */ }
  return {
    primaryBind: saved.primaryBind ?? null,
    secondaryBind: saved.secondaryBind ?? null,
    mushroomCircle1: saved.mushroomCircle ?? null,
    mushroomCircle2: null,
    useTpMachine: false,
    casinoPortal: null,
  }
}

export function useStatehelmPlanner() {
  const gameState = useGameStateStore()
  const settingsStore = useSettingsStore()
  const tracker = useStatehelmTracker()

  const excludedNpcKeys = ref<Set<string>>(new Set())
  const matches = ref<StatehelmGiftMatch[]>([])
  const loading = ref(false)
  const error = ref('')
  const route = ref<PlannedRoute | null>(null)
  const planning = ref(false)
  const routeError = ref('')

  function toggleExcluded(npcKey: string) {
    const next = new Set(excludedNpcKeys.value)
    if (next.has(npcKey)) next.delete(npcKey)
    else next.add(npcKey)
    excludedNpcKeys.value = next
    // Matches were fetched for a different NPC set — drop the stale plan.
    matches.value = []
    route.value = null
  }

  /** NPCs that still want gifts this week and aren't excluded. */
  const needingGifts = computed(() =>
    tracker.npcStatuses.value.filter(
      (s) => s.giftTracking && s.giftsThisWeek < s.maxGifts && !excludedNpcKeys.value.has(s.npc.key),
    ),
  )

  const remainingByNpc = computed(() => {
    const out: Record<string, number> = {}
    for (const s of tracker.npcStatuses.value) {
      out[s.npc.key] = Math.max(0, s.maxGifts - s.giftsThisWeek)
    }
    return out
  })

  function vaultMeta(vaultKey: string): { area: string | null; label: string } {
    const detail = gameState.storageVaultsByKey[vaultKey]
    if (!detail) return { area: null, label: vaultKey }
    return {
      area: detail.area ?? null,
      label: detail.npc_friendly_name ?? detail.area_name ?? vaultKey,
    }
  }

  /**
   * Greedy assignment over the backend-ordered matches (Love first, higher
   * pref first): each (vault, item) stack goes to the highest-benefit NPC
   * that still has weekly gifts remaining. Never exceeds the remaining
   * weekly counts from the tracker (manual entries included).
   */
  const assignments = computed<GiftAssignment[]>(() => {
    const remaining = { ...remainingByNpc.value }
    const claimedItems = new Set<string>()
    const out: GiftAssignment[] = []
    for (const m of matches.value) {
      if (excludedNpcKeys.value.has(m.npc_key)) continue
      if ((remaining[m.npc_key] ?? 0) <= 0) continue
      const itemKey = JSON.stringify([m.vault_key, m.item_name])
      if (claimedItems.has(itemKey)) continue
      claimedItems.add(itemKey)
      remaining[m.npc_key] = (remaining[m.npc_key] ?? 0) - 1
      const meta = vaultMeta(m.vault_key)
      out.push({ match: m, vaultArea: meta.area, vaultLabel: meta.label })
    }
    return out
  })

  /** Assignments grouped by vault for the match table. */
  const vaultGroups = computed<VaultGiftGroup[]>(() => {
    const groups = new Map<string, GiftAssignment[]>()
    for (const a of assignments.value) {
      const list = groups.get(a.match.vault_key) ?? []
      list.push(a)
      groups.set(a.match.vault_key, list)
    }
    return [...groups.entries()]
      .map(([vaultKey, list]) => ({
        vaultKey,
        vaultLabel: list[0].vaultLabel,
        vaultArea: list[0].vaultArea,
        assignments: list,
      }))
      .sort((a, b) => a.vaultLabel.localeCompare(b.vaultLabel))
  })

  const assignedNpcKeys = computed(() => [...new Set(assignments.value.map((a) => a.match.npc_key))])

  async function loadMatches() {
    const character = settingsStore.settings.activeCharacterName
    const server = settingsStore.settings.activeServerName
    const keys = needingGifts.value.map((s) => s.npc.key)
    matches.value = []
    route.value = null
    routeError.value = ''
    if (!character || !server || keys.length === 0) return
    loading.value = true
    error.value = ''
    try {
      matches.value = await invoke<StatehelmGiftMatch[]>('plan_statehelm_gifts', {
        characterName: character,
        serverName: server,
        npcKeys: keys,
      })
    } catch (e) {
      error.value = String(e)
    } finally {
      loading.value = false
    }
  }

  async function planRoute() {
    route.value = null
    routeError.value = ''
    if (assignments.value.length === 0) return
    planning.value = true
    try {
      const stops: RouteStop[] = []
      const seenAreas = new Set<string>()
      for (const g of vaultGroups.value) {
        if (!g.vaultArea || seenAreas.has(g.vaultArea)) continue
        seenAreas.add(g.vaultArea)
        const items = g.assignments.map((a) => a.match.item_name).join(', ')
        stops.push({
          zone: g.vaultArea,
          purpose: 'pickup',
          details: `Pick up ${items} from ${g.vaultLabel}`,
        })
      }
      const recipients = assignedNpcKeys.value
        .map((k) => tracker.npcStatuses.value.find((s) => s.npc.key === k)?.npc.name ?? k)
        .join(', ')
      stops.push({
        zone: 'AreaStatehelm',
        purpose: 'turn_in',
        details: `Deliver gifts to ${recipients}`,
      })

      const areaName = gameState.world.area?.area_name
      const startZone = (areaName && FRIENDLY_TO_KEY[areaName]) || 'AreaSerbule'
      const travelConfig = loadTravelConfig()
      try {
        const phase = await invoke<{ game_phase: string }>('get_current_moon_phase')
        const dest = CASINO_PORTAL_BY_PHASE[phase.game_phase as MoonPhaseName]
        travelConfig.casinoPortal = dest ? dest.toLowerCase() : null
      } catch {
        // Moon phase unavailable — solver uses both casino edges.
      }
      route.value = await invoke<PlannedRoute>('plan_trip', {
        startZone,
        stops,
        travelConfig,
      })
    } catch (e) {
      routeError.value = String(e)
    } finally {
      planning.value = false
    }
  }

  return {
    excludedNpcKeys,
    toggleExcluded,
    needingGifts,
    remainingByNpc,
    maxGifts: MAX_GIFTS_PER_WEEK,
    matches,
    assignments,
    vaultGroups,
    assignedNpcKeys,
    loading,
    error,
    loadMatches,
    route,
    planning,
    routeError,
    planRoute,
  }
}
