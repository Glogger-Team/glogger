// TypeScript types matching Rust game state query response structs

export interface GameStateSkill {
  skill_id: number
  skill_name: string
  level: number
  base_level: number
  bonus_levels: number
  xp: number
  tnl: number
  max_level: number
  last_confirmed_at: string
  source: 'log' | 'snapshot'
}

/** Total effective level (base + bonus) — this is what the game displays and what `level` stores */
export function skillTotalLevel(skill: Pick<GameStateSkill, 'level'>): number {
  return skill.level
}

/** Base level without bonuses — used for XP table indexing */
export function skillBaseLevel(skill: Pick<GameStateSkill, 'base_level'>): number {
  return skill.base_level
}

export interface GameStateAttribute {
  attribute_name: string
  value: number
  last_confirmed_at: string
}

export interface GameStateActiveSkills {
  skill1_id: number
  skill1_name: string
  skill2_id: number
  skill2_name: string
  last_confirmed_at: string
}

export interface GameStateWeather {
  weather_name: string
  is_active: boolean
  last_confirmed_at: string
}

export interface GameStateCombat {
  in_combat: boolean
  last_confirmed_at: string
}

export interface GameStateMount {
  is_mounted: boolean
  last_confirmed_at: string
}

export interface GameStateArea {
  area_name: string
  last_confirmed_at: string
}

export interface GameStateWorld {
  weather: GameStateWeather | null
  combat: GameStateCombat | null
  mount: GameStateMount | null
  area: GameStateArea | null
}

export interface GameStateInventoryItem {
  instance_id: number
  item_name: string
  item_type_id: number | null
  stack_size: number
  slot_index: number
  last_confirmed_at: string
  source: 'log' | 'snapshot'
}

export interface GameStateRecipe {
  recipe_id: number
  completion_count: number
  last_confirmed_at: string
  source: 'log' | 'snapshot'
}

export interface GameStateEquipmentSlot {
  slot: string
  appearance_key: string
  last_confirmed_at: string
}

export interface GameStateFavor {
  npc_key: string
  npc_name: string
  session_delta: number
  favor_tier: string | null
  last_confirmed_at: string
  source: 'log' | 'snapshot'
}

export interface GameStateCurrency {
  currency_name: string
  amount: number
  last_confirmed_at: string
  source: 'log' | 'snapshot'
}

export interface GameStateEffect {
  effect_instance_id: number
  effect_name: string | null
  source_entity_id: number
  last_confirmed_at: string
}

export interface GameStateStorageItem {
  vault_key: string
  instance_id: number
  item_name: string
  item_type_id: number | null
  stack_size: number
  slot_index: number
  last_confirmed_at: string
  source: 'log' | 'snapshot'
}

export interface GameStateVendor {
  npc_key: string
  vendor_gold_available: number | null
  vendor_gold_max: number | null
  vendor_gold_timer_start: string | null
  last_interaction_at: string | null
  last_sell_at: string | null
  last_confirmed_at: string
  councils_earned_current: number
  councils_earned_lifetime: number
}

export interface StorageVaultDetail {
  key: string
  id: number
  npc_friendly_name: string | null
  area: string | null
  area_name: string | null
  grouping: string | null
  grouping_name: string | null
  num_slots: number | null
  levels: Record<string, number> | null
  slot_attribute: string | null
  required_item_keywords: string[] | null
  requirement_description: string | null
  num_slots_script_atomic_max: number | null
}
