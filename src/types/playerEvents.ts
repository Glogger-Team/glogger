// TypeScript types matching Rust PlayerEvent enum (#[serde(tag = "kind")])

// === Item Events ===

// Source attribution for inventory gains. See Rust ItemProvenance in
// src-tauri/src/player_event_parser.rs for full semantics.
export type ActivitySource =
  | { kind: 'Mining'; node_entity_id: number | null; node_name: string | null }
  | { kind: 'SurveyMapUse'; survey_map_internal_name: string | null }
  | { kind: 'SurveyMapCraft' }
  | { kind: 'GeneralCraft'; action_type: string; label: string }
  | { kind: 'CorpseSearch'; entity_id: number; corpse_name: string }
  | { kind: 'VendorBrowsing'; npc_entity_id: number; npc_name: string | null }
  | { kind: 'StorageBrowsing'; vault_owner_entity_id: number; vault_name: string }

export type AttributionConfidence = 'Confident' | 'Probable' | 'Weak'

export type ItemProvenance =
  | {
      kind: 'Attributed'
      source: ActivitySource
      confidence: AttributionConfidence
      // A3 stitching link: when set, this gain belongs to the named survey_uses row.
      // Only ever populated by feature aggregators (survey tracker today); the
      // backend parser leaves it absent. Field is omitted from the JSON entirely
      // when None on the Rust side, hence optional here.
      survey_use_id?: number
    }
  | { kind: 'Uncertain'; candidates: ActivitySource[] }
  | { kind: 'UnknownSource' }
  | { kind: 'NotApplicable' }

export interface ItemAddedEvent {
  kind: 'ItemAdded'
  timestamp: string
  item_name: string
  instance_id: number
  slot_index: number
  is_new: boolean
  provenance: ItemProvenance
}

export interface ItemStackChangedEvent {
  kind: 'ItemStackChanged'
  timestamp: string
  instance_id: number
  item_name: string | null
  item_type_id: number
  old_stack_size: number
  new_stack_size: number
  delta: number
  from_server: boolean
  provenance: ItemProvenance
}

export type DeleteContext = 'StorageTransfer' | 'VendorSale' | 'Consumed' | 'Unknown'

export interface ItemDeletedEvent {
  kind: 'ItemDeleted'
  timestamp: string
  instance_id: number
  item_name: string | null
  context: DeleteContext
}

// === Skill Events ===

export interface SkillSnapshot {
  skill_type: string
  raw: number
  bonus: number
  xp: number
  tnl: number
  max: number
}

export interface SkillsLoadedEvent {
  kind: 'SkillsLoaded'
  timestamp: string
  skills: SkillSnapshot[]
}

// === NPC Events ===

export interface InteractionStartedEvent {
  kind: 'InteractionStarted'
  timestamp: string
  entity_id: number
  interaction_type: number
  npc_name: string
}

export interface FavorChangedEvent {
  kind: 'FavorChanged'
  timestamp: string
  npc_id: number
  npc_name: string
  delta: number
  is_gift: boolean
}

// === Vendor Events ===

export interface VendorSoldEvent {
  kind: 'VendorSold'
  timestamp: string
  price: number
  item_name: string
  instance_id: number
  is_buyback: boolean
}

export interface VendorStackUpdatedEvent {
  kind: 'VendorStackUpdated'
  timestamp: string
  instance_id: number
  item_type_id: number
  new_stack_size: number
  price: number
}

// === Storage Events ===

export interface StorageDepositEvent {
  kind: 'StorageDeposit'
  timestamp: string
  npc_id: number
  vault_key: string | null
  slot: number
  item_name: string
  instance_id: number
}

export interface StorageWithdrawalEvent {
  kind: 'StorageWithdrawal'
  timestamp: string
  npc_id: number
  vault_key: string | null
  instance_id: number
  quantity: number
  provenance: ItemProvenance
}

// === Action Events ===

export interface DelayLoopStartedEvent {
  kind: 'DelayLoopStarted'
  timestamp: string
  duration: number
  action_type: string
  label: string
  entity_id: number
  abort_condition: string
}

// === Screen/Book Events ===

export interface ScreenTextEvent {
  kind: 'ScreenText'
  timestamp: string
  category: string
  message: string
}

export interface BookOpenedEvent {
  kind: 'BookOpened'
  timestamp: string
  title: string
  content: string
  book_type: string
}

// === Interaction Events ===

export interface InteractionEndedEvent {
  kind: 'InteractionEnded'
  timestamp: string
  entity_id: number // i32 — can be negative
}

// === Skill Bar Events ===

export interface ActiveSkillsChangedEvent {
  kind: 'ActiveSkillsChanged'
  timestamp: string
  skill1: string
  skill2: string
}

// === Mount Events ===

export interface MountStateChangedEvent {
  kind: 'MountStateChanged'
  timestamp: string
  entity_id: number
  is_mounting: boolean
}

// === Weather Events ===

export interface WeatherChangedEvent {
  kind: 'WeatherChanged'
  timestamp: string
  weather_name: string
  is_active: boolean
}

// === Recipe Events ===

export interface RecipeUpdatedEvent {
  kind: 'RecipeUpdated'
  timestamp: string
  recipe_id: number
  completion_count: number
}

// === Combat Events ===

export interface CombatStateChangedEvent {
  kind: 'CombatStateChanged'
  timestamp: string
  in_combat: boolean
}

// === Vendor Gold Events ===

export interface VendorGoldChangedEvent {
  kind: 'VendorGoldChanged'
  timestamp: string
  current_gold: number
  server_id: number
  max_gold: number
}

// === Attribute Events ===

export interface AttributeValue {
  name: string
  value: number
}

export interface AttributesChangedEvent {
  kind: 'AttributesChanged'
  timestamp: string
  entity_id: number
  attributes: AttributeValue[]
}

// === Login Snapshot Events ===

export interface AbilitiesLoadedEvent {
  kind: 'AbilitiesLoaded'
  timestamp: string
  skill1: string
  skill2: string
}

export interface RecipesLoadedEvent {
  kind: 'RecipesLoaded'
  timestamp: string
}

export interface EquipmentSlot {
  slot: string
  appearance_key: string
}

export interface EquipmentChangedEvent {
  kind: 'EquipmentChanged'
  timestamp: string
  entity_id: number
  appearance: string
  equipment: EquipmentSlot[]
}

// === Effect Events ===

export interface EffectsAddedEvent {
  kind: 'EffectsAdded'
  timestamp: string
  entity_id: number
  source_entity_id: number
  effect_ids: number[]
  is_login_batch: boolean
}

export interface EffectsRemovedEvent {
  kind: 'EffectsRemoved'
  timestamp: string
  entity_id: number
}

export interface EffectNameUpdatedEvent {
  kind: 'EffectNameUpdated'
  timestamp: string
  entity_id: number
  effect_instance_id: number
  display_name: string
}

// === Loot Events ===

export interface LootPickedUpEvent {
  kind: 'LootPickedUp'
  timestamp: string
  instance_id: number
  corpse_entity_id: number | null
  corpse_name: string | null
  item_name: string | null
  item_type_id: number | null
  quantity: number
}

// Item extracted from a corpse via Butchering/Skinning (no loot-table roll).
export interface CorpseExtractEvent {
  kind: 'CorpseExtract'
  timestamp: string
  item_name: string
  item_type_id: number | null
  quantity: number
  skill: string // "Butchering" | "Skinning"
  corpse_name: string | null
  skill_level: number | null
  equipment_bonus: number | null
  anatomy_family: string | null
  anatomy_level: number | null
}

// === Union Type ===

/** The game's own declared UTC instant from a `Player.log` login line
 *  ("Logged in as character X. Time UTC=MM/DD/YYYY HH:MM:SS."). Anchors the date
 *  for every `[HH:MM:SS]` line after it; nothing in the UI consumes it. */
export interface LogSessionAnchoredEvent {
  kind: 'LogSessionAnchored'
  utc: string
}

/** A gift-capped (Statehelm) NPC's authoritative weekly gift count, from the
 *  "<Npc> will accept up to <b>N</b> gifts … has received <b>M</b>" note. */
export interface GiftCountObservedEvent {
  kind: 'GiftCountObserved'
  timestamp: string
  npc_name: string
  received: number
  max: number
}

export interface CorpseSearchedEvent {
  kind: 'CorpseSearched'
  timestamp: string
  corpse_entity_id: number
  corpse_name: string
  has_permission: boolean
}

export type PlayerEvent =
  | ItemAddedEvent
  | ItemStackChangedEvent
  | ItemDeletedEvent
  | SkillsLoadedEvent
  | InteractionStartedEvent
  | FavorChangedEvent
  | VendorSoldEvent
  | VendorStackUpdatedEvent
  | StorageDepositEvent
  | StorageWithdrawalEvent
  | DelayLoopStartedEvent
  | ScreenTextEvent
  | BookOpenedEvent
  | InteractionEndedEvent
  | ActiveSkillsChangedEvent
  | MountStateChangedEvent
  | WeatherChangedEvent
  | RecipeUpdatedEvent
  | CombatStateChangedEvent
  | VendorGoldChangedEvent
  | AttributesChangedEvent
  | AbilitiesLoadedEvent
  | RecipesLoadedEvent
  | EquipmentChangedEvent
  | EffectsAddedEvent
  | EffectsRemovedEvent
  | EffectNameUpdatedEvent
  | LootPickedUpEvent
  | CorpseExtractEvent
  | CorpseSearchedEvent
  | GiftCountObservedEvent
  | LogSessionAnchoredEvent
