import { ref, watch, type Ref } from "vue"
import { useGameStateStore } from "../stores/gameStateStore"
import { useGameDataStore } from "../stores/gameDataStore"
import { useMarketStore } from "../stores/marketStore"
import { useAggregateStore } from "../stores/aggregateStore"
import type { AppView } from "../components/MenuBar.vue"
import type { ItemInfo } from "../types/gameData/items"
import type { RecipeInfo } from "../types/gameData/recipes"
import type { NpcInfo } from "../types/gameData/npcs"
import type { QuestInfo } from "../types/gameData/quests"

// ── Result types ──────────────────────────────────────────────────────────────

export interface SearchNavigation {
  view: AppView
  subTab?: string
  entityType?: string
  entityId?: string | number
}

export interface SearchResult {
  category: string
  label: string
  detail: string
  navigation: SearchNavigation
}

export interface SearchCategory {
  name: string
  results: SearchResult[]
}

// ── Composable ────────────────────────────────────────────────────────────────

export function useQuickSearch(query: Ref<string>, options?: { cap?: number }) {
  const cap = options?.cap ?? 5
  const categories = ref<SearchCategory[]>([])
  const loading = ref(false)

  let searchVersion = 0

  watch(query, async (q) => {
    const trimmed = q.trim().toLowerCase()
    if (!trimmed || trimmed.length < 2) {
      categories.value = []
      loading.value = false
      return
    }

    const version = ++searchVersion
    loading.value = true

    // Player data searches are synchronous from store refs
    const playerResults = searchPlayerData(trimmed, cap)

    // Game data searches are async (Tauri invocations)
    const gameResults = await searchGameData(trimmed, cap)

    // Only apply if this is still the latest search
    if (version !== searchVersion) return

    categories.value = [...playerResults, ...gameResults].filter(c => c.results.length > 0)
    loading.value = false
  }, { immediate: true })

  return { categories, loading }
}

// ── Player data (synchronous) ─────────────────────────────────────────────────

function searchPlayerData(query: string, cap: number): SearchCategory[] {
  const gameState = useGameStateStore()
  const market = useMarketStore()
  const aggregate = useAggregateStore()

  const yourItems: SearchResult[] = []
  const yourSkills: SearchResult[] = []
  const seen = new Set<string>()

  // Helper: build market value suffix
  function marketSuffix(name: string): string {
    const mv = market.valuesByName[name]
    return mv ? ` · ${mv.market_value.toLocaleString()}g` : ""
  }

  // Helper: resolve vault key to a friendly name
  function vaultName(vaultKey: string): string {
    const meta = gameState.storageVaultsByKey[vaultKey]
    return meta?.npc_friendly_name ?? meta?.area_name ?? vaultKey
  }

  // 1. Backpack / live inventory (current character)
  for (const [name, count] of Object.entries(gameState.inventoryItemCounts)) {
    if (yourItems.length >= cap) break
    if (name.toLowerCase().includes(query)) {
      seen.add(name)
      yourItems.push({
        category: "Your Items",
        label: name,
        detail: `x${count} · Backpack${marketSuffix(name)}`,
        navigation: { view: "inventory", subTab: "storage" },
      })
    }
  }

  // 2. Vault storage (current character) — items not already matched from backpack
  if (yourItems.length < cap) {
    // Aggregate vault stacks per item for a cleaner result
    const vaultItems = new Map<string, { total: number; locations: string[] }>()
    for (const [vaultKey, items] of Object.entries(gameState.storageByVault)) {
      for (const item of items) {
        if (!item.item_name.toLowerCase().includes(query)) continue
        if (seen.has(item.item_name)) continue
        const existing = vaultItems.get(item.item_name)
        const loc = vaultName(vaultKey)
        if (existing) {
          existing.total += item.stack_size
          if (!existing.locations.includes(loc)) existing.locations.push(loc)
        } else {
          vaultItems.set(item.item_name, { total: item.stack_size, locations: [loc] })
        }
      }
    }
    for (const [name, info] of vaultItems) {
      if (yourItems.length >= cap) break
      seen.add(name)
      const locs = info.locations.length <= 2
        ? info.locations.join(", ")
        : `${info.locations[0]} +${info.locations.length - 1} more`
      yourItems.push({
        category: "Your Items",
        label: name,
        detail: `x${info.total} · ${locs}${marketSuffix(name)}`,
        navigation: { view: "inventory", subTab: "vaults" },
      })
    }
  }

  // 3. Aggregate inventory (other characters on same server)
  if (yourItems.length < cap) {
    for (const item of aggregate.inventory) {
      if (yourItems.length >= cap) break
      if (seen.has(item.item_name)) continue
      if (!item.item_name.toLowerCase().includes(query)) continue
      seen.add(item.item_name)
      const chars = item.characters.map(c => c.character_name).join(", ")
      yourItems.push({
        category: "Your Items",
        label: item.item_name,
        detail: `x${item.total_stack_size} · ${chars}${marketSuffix(item.item_name)}`,
        navigation: { view: "inventory", subTab: "vaults" },
      })
    }
  }

  // Your Skills
  for (const skill of gameState.skills) {
    if (yourSkills.length >= cap) break
    if (skill.skill_name.toLowerCase().includes(query)) {
      yourSkills.push({
        category: "Your Skills",
        label: skill.skill_name,
        detail: `Lv ${skill.level} · ${skill.xp.toLocaleString()} XP`,
        navigation: { view: "character", subTab: "skills" },
      })
    }
  }

  return [
    { name: "Your Items", results: yourItems },
    { name: "Your Skills", results: yourSkills },
  ]
}

// ── Game data (async, Tauri invocations) ──────────────────────────────────────

async function searchGameData(query: string, cap: number): Promise<SearchCategory[]> {
  const gameData = useGameDataStore()
  const market = useMarketStore()

  // Fire all searches in parallel
  const [items, recipes, npcs, quests] = await Promise.all([
    gameData.searchItems(query).catch(() => [] as ItemInfo[]),
    gameData.searchRecipes(query, cap).catch(() => [] as RecipeInfo[]),
    gameData.searchNpcs(query).catch(() => [] as NpcInfo[]),
    gameData.searchQuests(query).catch(() => [] as QuestInfo[]),
  ])

  const gameItems: SearchResult[] = items.slice(0, cap).map(item => ({
    category: "Game Items",
    label: item.name,
    detail: item.keywords?.join(", ") ?? "",
    navigation: {
      view: "data-browser" as AppView,
      subTab: "items",
      entityType: "item",
      entityId: item.name,
    },
  }))

  const gameRecipes: SearchResult[] = recipes.slice(0, cap).map(recipe => ({
    category: "Game Recipes",
    label: recipe.name,
    detail: [recipe.skill, recipe.skill_level_req ? `Lv ${recipe.skill_level_req}` : null].filter(Boolean).join(" · "),
    navigation: {
      view: "data-browser" as AppView,
      subTab: "recipes",
      entityType: "recipe",
      entityId: recipe.name,
    },
  }))

  const gameNpcs: SearchResult[] = npcs.slice(0, cap).map(npc => ({
    category: "NPCs",
    label: npc.name,
    detail: npc.area_name ?? "",
    navigation: {
      view: "data-browser" as AppView,
      subTab: "npcs",
      entityType: "npc",
      entityId: npc.key,
    },
  }))

  const gameQuests: SearchResult[] = quests.slice(0, cap).map(quest => ({
    category: "Quests",
    label: quest.raw.Name ?? quest.internal_name,
    detail: quest.raw.DisplayedLocation ?? "",
    navigation: {
      view: "data-browser" as AppView,
      subTab: "quests",
      entityType: "quest",
      entityId: quest.internal_name,
    },
  }))

  // Market Values
  const marketResults: SearchResult[] = []
  for (const mv of market.values) {
    if (marketResults.length >= cap) break
    if (mv.item_name.toLowerCase().includes(query)) {
      marketResults.push({
        category: "Market Values",
        label: mv.item_name,
        detail: `${mv.market_value.toLocaleString()}g`,
        navigation: { view: "economics", subTab: "market-prices" },
      })
    }
  }

  return [
    { name: "Game Items", results: gameItems },
    { name: "Game Recipes", results: gameRecipes },
    { name: "NPCs", results: gameNpcs },
    { name: "Quests", results: gameQuests },
    { name: "Market Values", results: marketResults },
  ]
}
