import { ref, computed, watch } from 'vue'
import { useBuildPlannerStore } from '../stores/buildPlannerStore'
import { useGameDataStore } from '../stores/gameDataStore'

/** Strip trailing rank number from ability name (e.g. "Pound To Slag 9" -> "Pound To Slag") */
function baseAbilityName(name: string): string {
  return name.replace(/\s+\d+$/, '')
}

/**
 * Provides cross-referencing between mods and abilities in the build planner.
 * Uses a precomputed backend index (built once at CDN load time) for O(1) lookups.
 * Makes a single batch call when slot powers change, then all queries are in-memory.
 */
export function useBuildCrossRef() {
  const store = useBuildPlannerStore()
  const gameData = useGameDataStore()

  /** Deduplicated base ability names from all assigned abilities */
  const abilityBaseNames = computed(() => {
    const names = new Map<string, string>()
    for (const a of store.presetAbilities) {
      if (!a.ability_name) continue
      const base = baseAbilityName(a.ability_name)
      if (!names.has(base) || a.ability_name > names.get(base)!) {
        names.set(base, a.ability_name)
      }
    }
    return names
  })

  /** Set of assigned ability IDs for fast lookup */
  const assignedAbilityIds = computed(() =>
    new Set(store.presetAbilities.map(a => a.ability_id))
  )

  /**
   * Cache: power key → list of assigned ability names that this power affects.
   * Built from one batch backend call, then filtered client-side.
   */
  const powerAbilities = ref(new Map<string, string[]>())

  /** Raw mapping from backend: tsys_key → all ability IDs it affects */
  let tsysAbilityCache: Record<string, number[]> = {}

  /** Rebuild the cross-reference cache using the precomputed backend index */
  async function refreshCrossRef() {
    if (store.slotPowers.length === 0 || store.presetAbilities.length === 0) {
      powerAbilities.value = new Map()
      tsysAbilityCache = {}
      return
    }

    // Single batch call to the backend — uses precomputed O(1) lookup per key
    const tsysKeys = store.slotPowers.map(p => p.key)
    try {
      tsysAbilityCache = await gameData.getTsysAbilityMap(tsysKeys)
    } catch {
      tsysAbilityCache = {}
    }

    // Build the power → ability names mapping by filtering to assigned abilities
    const abilityNameById = new Map<number, string>()
    for (const a of store.presetAbilities) {
      if (a.ability_name) abilityNameById.set(a.ability_id, a.ability_name)
    }

    const result = new Map<string, string[]>()
    for (const power of store.slotPowers) {
      const key = power.internal_name ?? power.key
      const abilityIds = tsysAbilityCache[power.key]
      if (!abilityIds) continue

      const matched = abilityIds
        .filter(id => assignedAbilityIds.value.has(id))
        .map(id => abilityNameById.get(id))
        .filter((name): name is string => name != null)

      if (matched.length > 0) {
        result.set(key, matched)
      }
    }
    powerAbilities.value = result
  }

  // Refresh when slot powers or abilities change
  watch(
    () => [store.slotPowers.length, store.presetAbilities.length],
    () => { refreshCrossRef() },
    { immediate: true },
  )

  watch(() => store.selectedSlot, () => {
    refreshCrossRef()
  })

  /**
   * Count of mods (from slotPowers) that reference each assigned ability.
   */
  const abilityModCounts = computed(() => {
    const counts = new Map<string, number>()
    for (const abilities of powerAbilities.value.values()) {
      for (const name of abilities) {
        const base = baseAbilityName(name)
        counts.set(base, (counts.get(base) ?? 0) + 1)
      }
    }
    return counts
  })

  function isAbilityRelated(powerKey: string): boolean {
    return powerAbilities.value.has(powerKey)
  }

  function getAbilitiesForPower(powerKey: string): string[] {
    return powerAbilities.value.get(powerKey) ?? []
  }

  /**
   * Get the raw tsys→ability ID mapping for use by other components (e.g. BuildSummary).
   * Returns the cached result from the last batch call.
   */
  function getTsysAbilityCache(): Record<string, number[]> {
    return tsysAbilityCache
  }

  /**
   * Snapshot known-ability check. `knownRows` are bare CDN InternalNames
   * (e.g. "DebilitatingBlow1") stored per skill at import time. A family
   * counts as known when a stored key matches any tier's InternalName or
   * the family's base key; empty known set = pre-v74 snapshot, fall back
   * to the level proxy at the call site (never filter to zero on missing).
   */
  function familyHasKnownAbility(
    tiers: { raw_json?: unknown; name?: string }[],
    baseInternalName: string,
    knownKeys: Set<string>,
  ): boolean {
    if (knownKeys.size === 0) return false
    if (knownKeys.has(baseInternalName)) return true
    for (const t of tiers) {
      const internal = (t.raw_json as Record<string, unknown> | undefined)?.InternalName
      if (typeof internal === 'string' && knownKeys.has(internal)) return true
    }
    return false
  }

  return {
    abilityBaseNames,
    powerAbilities,
    abilityModCounts,
    isAbilityRelated,
    getAbilitiesForPower,
    getTsysAbilityCache,
    familyHasKnownAbility,
  }
}
