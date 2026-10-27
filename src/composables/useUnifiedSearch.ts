import { ref, watch, type Ref } from "vue"
import { invoke } from "@tauri-apps/api/core"
import { useGameStateStore } from "../stores/gameStateStore"
import { useMarketStore } from "../stores/marketStore"
import { useAggregateStore } from "../stores/aggregateStore"
import type { AppView } from "../components/MenuBar.vue"

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
  score: number
  iconId?: number
}

export interface SearchCategory {
  name: string
  results: SearchResult[]
}

// Backend result shape from unified_search command
interface UnifiedSearchResult {
  entity_type: string
  name: string
  detail: string
  entity_id: string
  score: number
  icon_id: number | null
}

// ── Entity type → display category + navigation mapping ─────────────────────

interface CategoryConfig {
  displayName: string
  navigation: (entityId: string) => SearchNavigation
}

const CATEGORY_MAP: Record<string, CategoryConfig> = {
  item: {
    displayName: "Game Items",
    navigation: (id) => ({ view: "data-browser", subTab: "items", entityType: "item", entityId: id }),
  },
  recipe: {
    displayName: "Game Recipes",
    navigation: (id) => ({ view: "data-browser", subTab: "recipes", entityType: "recipe", entityId: id }),
  },
  npc: {
    displayName: "NPCs",
    navigation: (id) => ({ view: "data-browser", subTab: "npcs", entityType: "npc", entityId: id }),
  },
  quest: {
    displayName: "Quests",
    navigation: (id) => ({ view: "data-browser", subTab: "quests", entityType: "quest", entityId: id }),
  },
  skill: {
    displayName: "Skills",
    navigation: (id) => ({ view: "data-browser", subTab: "skills", entityType: "skill", entityId: id }),
  },
  ability: {
    displayName: "Abilities",
    navigation: (id) => ({ view: "data-browser", subTab: "abilities", entityType: "ability", entityId: id }),
  },
  effect: {
    displayName: "Effects",
    navigation: (id) => ({ view: "data-browser", subTab: "effects", entityType: "effect", entityId: id }),
  },
  enemy: {
    displayName: "Enemies",
    navigation: (id) => ({ view: "data-browser", subTab: "enemies", entityType: "enemy", entityId: id }),
  },
  area: {
    displayName: "Areas",
    navigation: (id) => ({ view: "data-browser", subTab: "areas", entityType: "area", entityId: id }),
  },
  title: {
    displayName: "Titles",
    navigation: (id) => ({ view: "data-browser", subTab: "titles", entityType: "title", entityId: id }),
  },
  lorebook: {
    displayName: "Lorebooks",
    navigation: (id) => ({ view: "data-browser", subTab: "lorebooks", entityType: "lorebook", entityId: id }),
  },
}

// Category display order
const CATEGORY_ORDER = [
  "Your Items", "Your Skills", "Game Items", "Game Recipes",
  "NPCs", "Quests", "Skills", "Abilities", "Effects",
  "Enemies", "Areas", "Titles", "Lorebooks", "Market Values",
]

// ── Composable ────────────────────────────────────────────────────────────────

export function useUnifiedSearch(query: Ref<string>, options?: { cap?: number }) {
  const cap = options?.cap ?? 5
  const categories = ref<SearchCategory[]>([])
  const loading = ref(false)

  let searchVersion = 0

  watch(query, async (q) => {
    const trimmed = q.trim()
    if (!trimmed || (trimmed.length < 2 && !trimmed.includes(":"))) {
      categories.value = []
      loading.value = false
      return
    }

    const version = ++searchVersion
    loading.value = true

    const lowerTrimmed = trimmed.toLowerCase()

    // Run player data (sync) and game data (async) in parallel
    const playerResults = searchPlayerData(lowerTrimmed, cap)
    const gameResults = await searchGameData(trimmed, cap)

    if (version !== searchVersion) return

    // Merge and order categories
    const allCategories = [...playerResults, ...gameResults].filter(c => c.results.length > 0)
    allCategories.sort((a, b) => {
      const ai = CATEGORY_ORDER.indexOf(a.name)
      const bi = CATEGORY_ORDER.indexOf(b.name)
      return (ai === -1 ? 999 : ai) - (bi === -1 ? 999 : bi)
    })

    categories.value = allCategories
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

  function marketSuffix(name: string): string {
    const mv = market.valuesByName[name]
    return mv ? ` · ${mv.market_value.toLocaleString()}g` : ""
  }

  function vaultName(vaultKey: string): string {
    const meta = gameState.storageVaultsByKey[vaultKey]
    return meta?.npc_friendly_name ?? meta?.area_name ?? vaultKey
  }

  // Backpack
  for (const [name, count] of Object.entries(gameState.inventoryItemCounts)) {
    if (yourItems.length >= cap) break
    if (name.toLowerCase().includes(query)) {
      seen.add(name)
      yourItems.push({
        category: "Your Items",
        label: name,
        detail: `x${count} · Backpack${marketSuffix(name)}`,
        navigation: { view: "inventory", subTab: "storage" },
        score: 90,
      })
    }
  }

  // Vault storage
  if (yourItems.length < cap) {
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
        score: 85,
      })
    }
  }

  // Aggregate inventory
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
        score: 80,
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
        score: 85,
      })
    }
  }

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
        score: 70,
      })
    }
  }

  return [
    { name: "Your Items", results: yourItems },
    { name: "Your Skills", results: yourSkills },
    { name: "Market Values", results: marketResults },
  ]
}

// ── Game data (async, single Tauri command) ──────────────────────────────────

async function searchGameData(query: string, cap: number): Promise<SearchCategory[]> {
  try {
    const results = await invoke<UnifiedSearchResult[]>("unified_search", {
      query,
      limit: cap,
    })

    // Group results by entity type → display category
    const grouped = new Map<string, SearchResult[]>()

    for (const r of results) {
      const config = CATEGORY_MAP[r.entity_type]
      if (!config) continue

      const categoryName = config.displayName
      if (!grouped.has(categoryName)) {
        grouped.set(categoryName, [])
      }

      const list = grouped.get(categoryName)!
      if (list.length >= cap) continue

      list.push({
        category: categoryName,
        label: r.name,
        detail: r.detail,
        navigation: config.navigation(r.entity_id),
        score: r.score,
        iconId: r.icon_id ?? undefined,
      })
    }

    return Array.from(grouped.entries()).map(([name, results]) => ({ name, results }))
  } catch {
    return []
  }
}
