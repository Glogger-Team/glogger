import { defineStore, acceptHMRUpdate } from 'pinia'
import { ref, computed } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { open } from '@tauri-apps/plugin-dialog'
import { useCraftingStore } from './craftingStore'
import { useGameStateStore } from './gameStateStore'
import { useSettingsStore } from './settingsStore'
import type { RecipeInfo } from '../types/gameData/recipes'
import type { FoodItem, GourmandFoodEntry, GourmandImportResult } from '../types/gourmand'
import type { MaterialNeed } from '../types/crafting'

export interface HelpfulRecipe {
  recipe: RecipeInfo
  food: FoodItem
}

export const useCooksHelperStore = defineStore('cooksHelper', () => {
  const crafting = useCraftingStore()

  // ── State ──────────────────────────────────────────────────────────────────

  const importedEatenNames = ref<Set<string>>(new Set())
  const allFoods = ref<FoodItem[]>([])
  const foodRecipes = ref<RecipeInfo[]>([])
  const gameStateStore = useGameStateStore()

  const selectedRecipeIds = ref<Set<number>>(new Set())
  const materialNeedsMap = ref<Map<number, MaterialNeed[]>>(new Map())

  const filterSkill = ref<string>('all')
  const filterAvailability = ref<'all' | 'can-craft' | 'missing-materials'>('all')
  const searchQuery = ref('')
  const sortMode = ref<'name' | 'skill-level' | 'food-level'>('name')

  const loading = ref(false)
  const checkingMaterials = ref(false)
  const error = ref<string | null>(null)

  // ── Computed ───────────────────────────────────────────────────────────────

  const blankMode = ref(false)
  const isImported = computed(() => blankMode.value || importedEatenNames.value.size > 0)

  const uneatenFoods = computed(() =>
    allFoods.value.filter(f => !importedEatenNames.value.has(f.name))
  )

  /** Distinct skill names present in loaded food recipes, sorted alphabetically */
  const availableSkills = computed<string[]>(() => {
    const skills = new Set<string>()
    for (const r of foodRecipes.value) {
      if (r.skill) skills.add(r.skill)
    }
    return [...skills].sort()
  })

  /** Recipes the cook knows that produce at least one uneaten food */
  const helpfulRecipes = computed<HelpfulRecipe[]>(() => {
    if (!isImported.value) return []

    // Build item_id → food lookup from uneaten foods
    const foodById = new Map<number, FoodItem>()
    for (const food of uneatenFoods.value) {
      foodById.set(food.item_id, food)
    }

    const learned = gameStateStore.learnedRecipeKeys
    const results: HelpfulRecipe[] = []
    for (const recipe of foodRecipes.value) {
      // Only include recipes the cook has learned
      if (learned.size > 0 && !learned.has(`Recipe_${recipe.id}`)) continue

      for (const resultId of recipe.result_item_ids) {
        const food = foodById.get(resultId)
        if (food) {
          results.push({ recipe, food })
          break // one match per recipe is enough
        }
      }
    }

    return results
  })

  /** Apply filters and search */
  const filteredRecipes = computed<HelpfulRecipe[]>(() => {
    let list = helpfulRecipes.value

    // Skill filter
    if (filterSkill.value !== 'all') {
      list = list.filter(h => h.recipe.skill === filterSkill.value)
    }

    // Availability filter
    if (filterAvailability.value !== 'all') {
      list = list.filter(h => {
        const needs = materialNeedsMap.value.get(h.recipe.id)
        if (!needs) return filterAvailability.value === 'missing-materials'
        const canCraft = needs.every(n => n.shortfall === 0 || n.vendor_price !== null)
        return filterAvailability.value === 'can-craft' ? canCraft : !canCraft
      })
    }

    // Search
    if (searchQuery.value.trim()) {
      const q = searchQuery.value.toLowerCase()
      list = list.filter(h =>
        h.recipe.name.toLowerCase().includes(q) ||
        h.food.name.toLowerCase().includes(q)
      )
    }

    // Sort
    list = [...list]
    switch (sortMode.value) {
      case 'name':
        list.sort((a, b) => a.food.name.localeCompare(b.food.name))
        break
      case 'skill-level':
        list.sort((a, b) => (a.recipe.skill_level_req ?? 0) - (b.recipe.skill_level_req ?? 0))
        break
      case 'food-level':
        list.sort((a, b) => a.food.food_level - b.food.food_level)
        break
    }

    return list
  })

  const selectionCount = computed(() => selectedRecipeIds.value.size)

  /** Check how many of a food the player currently owns (inventory + storage) — total across all locations */
  function ownedCount(foodName: string): number {
    return gameStateStore.ownedItemCounts[foodName] ?? 0
  }

  const stats = computed(() => ({
    totalFoods: allFoods.value.length,
    eaten: importedEatenNames.value.size,
    uneaten: uneatenFoods.value.length,
    craftable: helpfulRecipes.value.length,
  }))

  // ── Actions ────────────────────────────────────────────────────────────────

  /** Reload the eaten-food set from the shared gourmand table */
  async function loadEatenFromDb() {
    const entries = await invoke<GourmandFoodEntry[]>('get_gourmand_eaten_foods')
    importedEatenNames.value = new Set(entries.map(e => e.name))
  }

  /** Auto-import the newest report in Books/ on mount, then hydrate from the DB */
  async function tryAutoImport() {
    if (blankMode.value) return
    loading.value = true
    try {
      await invoke<GourmandImportResult | null>('import_latest_gourmand_report')
      await loadEatenFromDb()
      await loadFoodsAndRecipes()
    } catch (e) {
      console.warn("Cook's Helper auto-import:", e)
    } finally {
      loading.value = false
    }
  }

  async function importFile() {
    error.value = null

    const settingsStore = useSettingsStore()
    const filePath = await open({
      defaultPath: (settingsStore.settings.gameDataPath ? settingsStore.settings.gameDataPath + '/Books' : undefined),
      filters: [{ name: 'Gourmand Skill Report', extensions: ['txt'] }],
    })

    if (!filePath) return

    try {
      loading.value = true
      await invoke<GourmandImportResult>('import_gourmand_report', {
        filePath: filePath as string,
      })
      await loadEatenFromDb()

      // Load food + recipe data
      await loadFoodsAndRecipes()

      // Reset selection and materials
      selectedRecipeIds.value = new Set()
      materialNeedsMap.value = new Map()
    } catch (e) {
      error.value = String(e)
    } finally {
      loading.value = false
    }
  }

  async function loadFoodsAndRecipes() {
    const foods = await invoke<FoodItem[]>('get_all_foods')
    allFoods.value = foods

    // Food-first approach: get all recipes that produce any food item
    const foodItemIds = foods.map(f => f.item_id)
    const recipes = await invoke<RecipeInfo[]>('get_recipes_producing_items', { itemIds: foodItemIds })
    foodRecipes.value = recipes
  }

  async function checkAllMaterials() {
    checkingMaterials.value = true
    error.value = null

    try {
      const newMap = new Map<number, MaterialNeed[]>()

      for (const { recipe } of helpfulRecipes.value) {
        const resolved = await crafting.resolveRecipeIngredients(recipe, 1)
        const flat = crafting.flattenIngredients(resolved.ingredients)
        const needs = await crafting.checkMaterialAvailability(flat)
        newMap.set(recipe.id, needs)
      }

      materialNeedsMap.value = newMap
    } catch (e) {
      error.value = `Material check failed: ${e}`
    } finally {
      checkingMaterials.value = false
    }
  }

  function toggleSelection(recipeId: number) {
    const next = new Set(selectedRecipeIds.value)
    if (next.has(recipeId)) {
      next.delete(recipeId)
    } else {
      next.add(recipeId)
    }
    selectedRecipeIds.value = next
  }

  function selectAll() {
    selectedRecipeIds.value = new Set(filteredRecipes.value.map(h => h.recipe.id))
  }

  function deselectAll() {
    selectedRecipeIds.value = new Set()
  }

  async function addToProject(projectId: number) {
    for (const { recipe } of helpfulRecipes.value) {
      if (selectedRecipeIds.value.has(recipe.id)) {
        await crafting.addEntry(projectId, recipe.id, recipe.name, 1)
      }
    }
    selectedRecipeIds.value = new Set()
  }

  async function createProjectFromSelection(name?: string) {
    const projectName = name || `Cook's Helper — ${new Date().toLocaleDateString()}`
    const projectId = await crafting.createProject(projectName)
    await addToProject(projectId)
    return projectId
  }

  /** Start with a blank slate — treat all foods as uneaten, no import needed */
  async function startFresh() {
    error.value = null
    loading.value = true
    try {
      importedEatenNames.value = new Set()
      blankMode.value = true
      await loadFoodsAndRecipes()
      selectedRecipeIds.value = new Set()
      materialNeedsMap.value = new Map()
    } catch (e) {
      error.value = String(e)
    } finally {
      loading.value = false
    }
  }

  function clear() {
    importedEatenNames.value = new Set()
    blankMode.value = false
    allFoods.value = []
    foodRecipes.value = []
    selectedRecipeIds.value = new Set()
    materialNeedsMap.value = new Map()
    error.value = null
  }

  // Keep eaten state fresh when the backend imports a report or the log records a live food-eaten
  listen<number>('gourmand-updated', () => {
    if (!blankMode.value) void loadEatenFromDb()
  })

  return {
    // State
    importedEatenNames,
    selectedRecipeIds,
    materialNeedsMap,
    filterSkill,
    filterAvailability,
    searchQuery,
    sortMode,
    loading,
    checkingMaterials,
    error,
    // Computed
    isImported,
    uneatenFoods,
    availableSkills,
    helpfulRecipes,
    filteredRecipes,
    selectionCount,
    stats,
    // Actions
    importFile,
    tryAutoImport,
    startFresh,
    ownedCount,
    checkAllMaterials,
    toggleSelection,
    selectAll,
    deselectAll,
    addToProject,
    createProjectFromSelection,
    clear,
  }
})

if (import.meta.hot) {
  import.meta.hot.accept(acceptHMRUpdate(useCooksHelperStore, import.meta.hot))
}
