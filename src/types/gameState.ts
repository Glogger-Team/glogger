// TypeScript types matching Rust game state query response structs
//
// Byte-identical shapes are re-exported from the generated Specta bindings
// (`gameState.specta.ts`, `cdn.specta.ts`). The hand definitions that remain
// differ from the bindings: `source` is a narrow `'log' | 'snapshot'` union
// here but a plain `string` in the bindings, and several numerics are
// non-nullable here but `| null` in the bindings (`GameStateAttribute.value`,
// `GameStateFavor.session_delta`, `GameStateCurrency.amount`,
// `GameStateVendor.councils_earned_*`).

export type {
  GameStateActiveSkills,
  GameStateWeather,
  GameStateCombat,
  GameStateMount,
  GameStateArea,
  GameStateWorld,
  GameStateEquipmentSlot,
  GameStateEffect,
} from './gameState.specta'
export type { StorageVaultDetail } from './cdn.specta'

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
