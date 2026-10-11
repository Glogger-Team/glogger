import { defineStore } from 'pinia'
import { ref, computed } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { open, save } from '@tauri-apps/plugin-dialog'
import { useGameStateStore } from './gameStateStore'
import { useCharacterStore } from './characterStore'
import { useSettingsStore } from './settingsStore'
import type {
  FoodItem,
  FoodSourceKind,
  GourmandFoodEntry,
  GourmandImportResult,
} from '../types/gourmand'
import { foodSourceLabel, foodSourceRank, isAttainable, isKnownFoodCategory } from '../types/gourmand'

/**
 * `all` shows everything; `not-crafted` is the complement of `crafted` (handy
 * for "what's left that I can't just cook?"); anything else matches foods that
 * list that kind among their sources — not just as their primary one, so a
 * quest-given event candy shows up under both Quest and Event.
 */
export type FoodSourceFilter = 'all' | 'not-crafted' | FoodSourceKind

export function matchesSourceFilter(food: FoodItem, filter: FoodSourceFilter): boolean {
  if (filter === 'all') return true
  if (filter === 'not-crafted') return !food.source_kinds.includes('crafted')
  return food.source_kinds.includes(filter)
}

export const useGourmandStore = defineStore('gourmand', () => {
  // ── State ──────────────────────────────────────────────────────────────────

  const allFoods = ref<FoodItem[]>([])
  const eatenFoods = ref<Map<string, number>>(new Map())
  const manuallyMarkedFoods = ref<Set<string>>(new Set())
  const reportLoaded = ref(false)
  const loading = ref(false)
  const error = ref<string | null>(null)

  // Gourmand skill level
  const manualGourmandLevel = ref<number | null>(null)

  // UI state
  const selectedMeal = ref<FoodItem | null>(null)
  const selectedSnack = ref<FoodItem | null>(null)
  const hideEaten = ref(false)
  const hideUnusable = ref(false)
  const sourceFilter = ref<FoodSourceFilter>('all')
  /** Count progress against only the foods you can still realistically get. */
  const attainableOnly = ref(false)
  const sortMode = ref<'level' | 'alpha' | 'food-level' | 'source'>('level')
  const sortAsc = ref(true)
  const viewMode = ref<'card' | 'list'>('card')

  // ── Computed: Gourmand level resolution ────────────────────────────────────

  const gourmandLevel = computed<number | null>(() => {
    // Priority: manual override > session > game state (persisted) > snapshot
    if (manualGourmandLevel.value !== null) return manualGourmandLevel.value

    const gameState = useGameStateStore()
    const sessionSkill = gameState.sessionSkills['Gourmand']
    if (sessionSkill) return sessionSkill.currentLevel

    const persisted = gameState.skillsByName['Gourmand']
    if (persisted) return persisted.level

    const characterStore = useCharacterStore()
    const snapshotSkill = characterStore.skills.find(s => s.skill_name === 'Gourmand')
    if (snapshotSkill) return snapshotSkill.level

    return null
  })

  const gourmandLevelSource = computed<'manual' | 'session' | 'game_state' | 'snapshot' | null>(() => {
    if (manualGourmandLevel.value !== null) return 'manual'

    const gameState = useGameStateStore()
    if (gameState.sessionSkills['Gourmand']) return 'session'

    if (gameState.skillsByName['Gourmand']) return 'game_state'

    const characterStore = useCharacterStore()
    if (characterStore.skills.find(s => s.skill_name === 'Gourmand')) return 'snapshot'

    return null
  })

  // ── Computed: Category splits ──────────────────────────────────────────────

  const meals = computed(() => allFoods.value.filter(f => f.food_category === 'Meal'))
  const snacks = computed(() => allFoods.value.filter(f => f.food_category === 'Snack'))
  const instantSnacks = computed(() => allFoods.value.filter(f => f.food_category === 'Instant-Snack'))

  /**
   * Foods filed under a category with no section on this screen — a CDN
   * spelling drift or a new game category. Surfaced as their own bucket so they
   * get noticed and fixed, never silently dropped (see the v70 Instant Snack
   * regression).
   */
  const unknownCategoryFoods = computed(() =>
    allFoods.value.filter(f => !isKnownFoodCategory(f.food_category)),
  )

  // ── Computed: Progress stats ───────────────────────────────────────────────

  // Progress counts against their own set rather than the category lists above,
  // so the "attainable only" toggle can drop event and unobtainable foods from
  // the denominators without hiding them from the lists.
  const progressFoods = computed(() =>
    attainableOnly.value ? allFoods.value.filter(isAttainable) : allFoods.value,
  )
  const progressMeals = computed(() => progressFoods.value.filter(f => f.food_category === 'Meal'))
  const progressSnacks = computed(() => progressFoods.value.filter(f => f.food_category === 'Snack'))
  const progressInstantSnacks = computed(() =>
    progressFoods.value.filter(f => f.food_category === 'Instant-Snack'),
  )

  function countEaten(foods: FoodItem[]): number {
    return foods.filter(f => eatenFoods.value.has(f.name)).length
  }

  const mealsEaten = computed(() => countEaten(progressMeals.value))
  const snacksEaten = computed(() => countEaten(progressSnacks.value))
  const instantSnacksEaten = computed(() => countEaten(progressInstantSnacks.value))
  const totalEaten = computed(() => countEaten(progressFoods.value))

  const mealProgress = computed(() => progressMeals.value.length ? (mealsEaten.value / progressMeals.value.length) * 100 : 0)
  const snackProgress = computed(() => progressSnacks.value.length ? (snacksEaten.value / progressSnacks.value.length) * 100 : 0)
  const instantSnackProgress = computed(() => progressInstantSnacks.value.length ? (instantSnacksEaten.value / progressInstantSnacks.value.length) * 100 : 0)
  const overallProgress = computed(() => progressFoods.value.length ? (totalEaten.value / progressFoods.value.length) * 100 : 0)

  // ── Computed: Favorites (top 3 most consumed per category) ─────────────────

  function topEaten(foods: FoodItem[], n = 3): { name: string; count: number }[] {
    return foods
      .filter(f => eatenFoods.value.has(f.name))
      .map(f => ({ name: f.name, count: eatenFoods.value.get(f.name)! }))
      .sort((a, b) => b.count - a.count)
      .slice(0, n)
  }

  const favoriteMeals = computed(() => topEaten(meals.value))
  const favoriteSnacks = computed(() => topEaten(snacks.value))
  const favoriteInstantSnacks = computed(() => topEaten(instantSnacks.value))

  // ── Computed: Uneaten foods ────────────────────────────────────────────────

  const uneatenFoods = computed(() => allFoods.value.filter(f => !eatenFoods.value.has(f.name)))

  /** What "Export Uneaten" writes: uneaten, narrowed by the active filters. */
  const exportableUneatenFoods = computed(() =>
    uneatenFoods.value
      .filter(f => matchesSourceFilter(f, sourceFilter.value))
      .filter(f => !attainableOnly.value || isAttainable(f)),
  )

  // ── Computed: Combined effects for comparison panel ────────────────────────

  const combinedEffects = computed<string[]>(() => {
    const effects: string[] = []
    if (selectedMeal.value) {
      effects.push(...selectedMeal.value.effect_descs)
    }
    if (selectedSnack.value) {
      effects.push(...selectedSnack.value.effect_descs)
    }
    return effects
  })

  // ── Actions ────────────────────────────────────────────────────────────────

  async function loadAllFoods() {
    try {
      loading.value = true
      allFoods.value = await invoke<FoodItem[]>('get_all_foods')
    } catch (e) {
      error.value = String(e)
    } finally {
      loading.value = false
    }
  }

  async function loadEatenFoods() {
    try {
      const entries = await invoke<GourmandFoodEntry[]>('get_gourmand_eaten_foods')
      if (entries.length > 0) {
        const map = new Map<string, number>()
        const manualSet = new Set<string>()
        for (const entry of entries) {
          map.set(entry.name, entry.count)
          if (entry.manually_marked) {
            manualSet.add(entry.name)
          }
        }
        eatenFoods.value = map
        manuallyMarkedFoods.value = manualSet
        reportLoaded.value = true
      }
    } catch (e) {
      error.value = String(e)
    }
  }

  async function importReport() {
    error.value = null

    const settingsStore = useSettingsStore()
    const filePath = await open({
      defaultPath: (settingsStore.settings.gameDataPath ? settingsStore.settings.gameDataPath + '/Books' : undefined),
      filters: [{ name: 'Gourmand Report', extensions: ['txt'] }],
    })

    if (!filePath) return null

    try {
      loading.value = true
      const result = await invoke<GourmandImportResult>('import_gourmand_report', {
        filePath: filePath as string,
      })

      // Reload eaten foods from DB after import
      await loadEatenFoods()

      return result
    } catch (e) {
      error.value = String(e)
      return null
    } finally {
      loading.value = false
    }
  }

  function setManualLevel(level: number | null) {
    manualGourmandLevel.value = level
  }

  function selectMeal(food: FoodItem | null) {
    selectedMeal.value = food
  }

  function selectSnack(food: FoodItem | null) {
    selectedSnack.value = food
  }

  function clearSelection() {
    selectedMeal.value = null
    selectedSnack.value = null
  }

  async function exportUneaten() {
    const filePath = await save({
      filters: [{ name: 'Text File', extensions: ['txt'] }],
      defaultPath: 'uneaten-foods.txt',
    })

    if (!filePath) return

    const foods = exportableUneatenFoods.value
    const lines = foods.map(f => {
      const req = f.gourmand_req !== null ? ` (Gourmand ${f.gourmand_req})` : ''
      return `${f.name} — ${f.food_category} Level ${f.food_level}${req} [${foodSourceLabel(f)}]`
    })

    // Spell out which filters shaped the list, so an export narrowed to e.g.
    // Cooking-only foods isn't mistaken for the full remaining set.
    const scope: string[] = []
    if (sourceFilter.value !== 'all') scope.push(`source: ${sourceFilter.value}`)
    if (attainableOnly.value) scope.push('attainable only')
    const header = scope.length
      ? `Uneaten Foods (${lines.length} remaining — ${scope.join(', ')})`
      : `Uneaten Foods (${lines.length} remaining)`

    const content = `${header}\n\n${lines.join('\n')}\n`

    try {
      await invoke('export_text_file', { filePath, content })
    } catch (e) {
      error.value = `Failed to write export file: ${e}`
    }
  }

  async function toggleFoodEaten(foodName: string) {
    try {
      const isNowEaten = await invoke<boolean>('toggle_food_eaten_status', { foodName })
      // Update local state immediately
      if (isNowEaten) {
        eatenFoods.value.set(foodName, 1)
        manuallyMarkedFoods.value.add(foodName)
      } else {
        eatenFoods.value.delete(foodName)
        manuallyMarkedFoods.value.delete(foodName)
      }
      // Trigger reactivity
      eatenFoods.value = new Map(eatenFoods.value)
      manuallyMarkedFoods.value = new Set(manuallyMarkedFoods.value)
      reportLoaded.value = eatenFoods.value.size > 0
    } catch (e) {
      error.value = String(e)
    }
  }

  async function tryAutoImport() {
    try {
      const result = await invoke<GourmandImportResult | null>('import_latest_gourmand_report')
      if (result) {
        await loadEatenFoods()
      }
    } catch (e) {
      console.warn('Gourmand auto-import:', e)
    }
  }

  // ── Sort helper ────────────────────────────────────────────────────────────

  function sortedFoods(foods: FoodItem[]): FoodItem[] {
    const sorted = [...foods]
    switch (sortMode.value) {
      case 'level':
        sorted.sort((a, b) => (a.gourmand_req ?? 0) - (b.gourmand_req ?? 0) || a.name.localeCompare(b.name))
        break
      case 'food-level':
        sorted.sort((a, b) => a.food_level - b.food_level || a.name.localeCompare(b.name))
        break
      case 'alpha':
        sorted.sort((a, b) => a.name.localeCompare(b.name))
        break
      case 'source':
        sorted.sort((a, b) => foodSourceRank(a) - foodSourceRank(b) || a.name.localeCompare(b.name))
        break
    }
    return sorted
  }

  // Auto-reload eaten foods when the backend imports a gourmand report
  // from a ProcessBook event (user ran /report gourmand in-game)
  listen<number>('gourmand-updated', () => {
    loadEatenFoods()
  })

  return {
    // State
    allFoods,
    eatenFoods,
    manuallyMarkedFoods,
    reportLoaded,
    loading,
    error,
    manualGourmandLevel,
    selectedMeal,
    selectedSnack,
    hideEaten,
    hideUnusable,
    sourceFilter,
    attainableOnly,
    sortMode,
    sortAsc,
    viewMode,
    // Computed
    gourmandLevel,
    gourmandLevelSource,
    meals,
    snacks,
    instantSnacks,
    unknownCategoryFoods,
    progressFoods,
    progressMeals,
    progressSnacks,
    progressInstantSnacks,
    mealsEaten,
    snacksEaten,
    instantSnacksEaten,
    totalEaten,
    mealProgress,
    snackProgress,
    instantSnackProgress,
    overallProgress,
    favoriteMeals,
    favoriteSnacks,
    favoriteInstantSnacks,
    uneatenFoods,
    exportableUneatenFoods,
    combinedEffects,
    // Actions
    loadAllFoods,
    loadEatenFoods,
    importReport,
    setManualLevel,
    selectMeal,
    selectSnack,
    clearSelection,
    exportUneaten,
    toggleFoodEaten,
    tryAutoImport,
    sortedFoods,
  }
})
