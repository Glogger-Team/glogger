// Re-export all game data types from their modules
export type { ItemInfo } from './items'
export type { SkillInfo } from './skills'
export type { AbilityInfo, AbilityFamily, CombatStats } from './abilities'
export type { RecipeIngredient, RecipeCost, RecipeResultItem, RecipeInfo } from './recipes'
export type {
  QuestInfo,
  QuestData,
  QuestObjective,
  QuestReward,
  QuestRewardItem,
  QuestRequirement
} from './quests'
export type { NpcPreference, NpcInfo } from './npcs'
export type { PlayerTitleInfo } from './playerTitles'
export type { AchievementInfo } from './achievements'
export type { EffectInfo } from './effects'
export type { EnemyInfo, MonsterEntry } from './enemies'
export type { LorebookEntry, LorebookCategoryInfo } from './lorebooks'
export type { TsysBrowserEntry, TsysTierInfo } from './tsys'
export type { SourceEntry, QuestSummary, EntitySources } from './sources'

// Cross-reference types
export interface NpcFavorEntry {
  npc_key: string
  npc_name: string
  desire: string
  pref: number
  match_type: string
}

export interface TsysAbilityXref {
  key: string
  internal_name: string | null
  skill: string | null
  prefix: string | null
  suffix: string | null
  slots: string[]
  tier_count: number
  top_tier_effects: string[]
}

export interface AbilityTsysXref {
  id: number
  name: string
  icon_id: number | null
  skill: string | null
  level: number | null
  internal_name: string | null
}

// Gardening product chain types
export interface GardeningChainItem {
  id: number
  name: string
  icon_id: number | null
  role: string // "seedling" | "plant" | "product"
}

export interface GardeningChainRecipe {
  id: number
  name: string
  skill: string | null
  skill_level_req: number | null
}

export interface GardeningChainProduct {
  recipe: GardeningChainRecipe
  items: GardeningChainItem[]
}

export interface GardeningProductChain {
  seedling: GardeningChainItem | null
  plant: GardeningChainItem | null
  products: GardeningChainProduct[]
  gardening_level: number | null
}

// CacheStatus lives here since it's not really a game data type
export interface CacheStatus {
  cached_version: number | null
  remote_version: number | null
  up_to_date: boolean
  item_count: number
  skill_count: number
}
