// ── Backend types (match Rust structs) ───────────────────────────────────────

export interface CraftingProjectSummary {
  id: number
  name: string
  notes: string
  group_name: string | null
  created_at: string
  updated_at: string
  entry_count: number
}

export interface CraftingProjectEntry {
  id: number
  project_id: number
  recipe_id: number
  recipe_name: string
  quantity: number
  sort_order: number
  expanded_ingredient_ids: number[]
  target_stock: number | null
  /** Item IDs pinned to the recipe's variable slots, in slot order. Empty = generic. */
  slot_item_ids: number[]
}

export interface CraftingProject {
  id: number
  name: string
  notes: string
  group_name: string | null
  fee_config: FeeConfig
  customer_provides: Record<string, number>
  created_at: string
  updated_at: string
  entries: CraftingProjectEntry[]
}

// ── Dependency resolver types ────────────────────────────────────────────────

export interface ResolvedIngredient {
  /** CDN item ID (null for dynamic/keyword ingredients) */
  item_id: number | null
  /** Display name */
  item_name: string
  /** Quantity required per single craft */
  per_craft: number
  /** Total quantity needed (per_craft × craft_count) */
  quantity_needed: number
  /** Chance each unit is consumed (1.0 = always) */
  chance_to_consume: number
  /** Expected consumption accounting for chance */
  expected_quantity: number
  /** Whether this ingredient can itself be crafted */
  is_craftable: boolean
  /** If expanded, the recipe used to produce this ingredient */
  source_recipe_id: number | null
  source_recipe_name: string | null
  /** Child ingredients (if this intermediate is expanded) */
  children: ResolvedIngredient[]
  /** Number of crafts needed to produce this ingredient */
  crafts_needed: number
  /** Keyword keys for dynamic ingredients (e.g., ["Crystal", "ToxicFrogSkin"]) */
  item_keys: string[]
  /** True when this ingredient is keyword-based (no specific item_id) */
  is_dynamic: boolean
}

/** A non-item currency cost (Combat Wisdom, Fae Energy, …) scaled to the craft count */
export interface ResolvedCurrencyCost {
  currency: string
  /** Cost of a single craft */
  per_craft: number
  /** Total across every craft, including expanded intermediates */
  total: number
}

export interface ResolvedRecipe {
  recipe_id: number
  recipe_name: string
  /** How many times to craft this recipe */
  craft_count: number
  /** Desired output quantity */
  desired_quantity: number
  /** Actual output per craft (batch size) */
  output_per_craft: number
  /** XP per craft */
  xp_per_craft: number
  /** First-time bonus XP (0 if already crafted) */
  xp_first_time: number
  /** Total XP for all crafts */
  total_xp: number
  /** Skill that receives XP */
  reward_skill: string | null
  /** Flat ingredient list (leaf nodes only) */
  ingredients: ResolvedIngredient[]
  /** Estimated vendor cost for all ingredients */
  estimated_cost: number
  /** Currency costs (Combat Wisdom, Fae Energy, …), incl. expanded intermediates */
  currency_costs: ResolvedCurrencyCost[]
}

// ── Flattened material types ─────────────────────────────────────────────────

export interface FlattenedMaterial {
  /** Unique key: item ID as string, or "kw:KeywordName" for dynamic */
  key: string
  /** CDN item ID (null for dynamic) */
  item_id: number | null
  /** Display name */
  item_name: string
  /** Raw quantity needed (before chance-to-consume) */
  quantity: number
  /** Chance each unit is consumed (1.0 = always) */
  chance_to_consume: number
  /** Expected consumption accounting for chance */
  expected_quantity: number
  /** Whether this is a keyword-based ingredient */
  is_dynamic: boolean
  /** Keywords for dynamic ingredients */
  item_keys: string[]
  /** Whether this ingredient can be crafted via a known recipe */
  is_craftable: boolean
}

export interface IntermediateCraft {
  recipe_name: string
  recipe_id: number
  item_name: string
  item_id: number
  crafts_needed: number
  quantity_produced: number
}

/** How much of one item a crafting project needs (Data Browser cross-reference) */
export interface ProjectItemNeed {
  project_id: number
  project_name: string
  group_name: string | null
  /** Expected quantity needed as a raw material (post-buffer) */
  quantity: number
  /** Quantity needed as an expanded intermediate (crafted within the project) */
  intermediate_quantity: number
  /** Dynamic keyword slots this item can fill, with quantities */
  keyword_needs: { keyword: string; quantity: number }[]
}

// ── Inventory integration types ──────────────────────────────────────────────

export interface VaultStock {
  vault_name: string
  quantity: number
  /** For dynamic material vault entries: the concrete item ID */
  item_id?: number
  /** For dynamic material vault entries: the concrete item name */
  item_name?: string
}

export interface MaterialAvailability {
  item_type_id: number
  item_name: string
  inventory_quantity: number
  storage_quantity: number
  vault_breakdown: VaultStock[]
  total_available: number
}

export interface MaterialNeed {
  item_id: number
  item_name: string
  quantity_needed: number
  /** From persisted + live inventory */
  inventory_have: number
  /** From storage vaults (snapshot) */
  storage_have: number
  /** Per-vault breakdown */
  vault_breakdown: VaultStock[]
  /** How many more the player needs to acquire */
  shortfall: number
  /** Estimated NPC vendor price per unit (value * 2), null if not vendor-purchasable */
  vendor_price: number | null
  /** Best known acquisition price: market price if available, else value * 2 fallback */
  unit_price: number | null
  /** True if this item can be crafted via a known recipe (intermediate ingredient) */
  is_craftable: boolean
  /** True if this is a wildcard/keyword slot (e.g. "Any Crystal") */
  is_dynamic?: boolean
  /** Keywords for dynamic slots (e.g. ["Crystal"]) */
  item_keys?: string[]
  /** For dynamic materials: per-item breakdown of what the player has */
  dynamic_breakdown?: DynamicItemBreakdown[]
}

export interface DynamicItemBreakdown {
  item_id: number
  item_name: string
  inventory_qty: number
  storage_qty: number
}

// ── Leveling helper types ───────────────────────────────────────────────────

export interface LevelingPlanEntry {
  recipe_id: number
  recipe_name: string
  craft_count: number
  /** Base XP per craft (before buff) */
  xp_per_craft: number
  /** First-craft bonus: the XP added on top of one repeat craft so the first-craft
   *  total equals base × 4 (unbuffed, no drop-off). 0 if already crafted. */
  xp_first_time: number
  /** XP drop-off multiplier (0..1) for repeat crafts at the level this entry was planned for */
  xp_drop_off_mult?: number
  /** Effective total XP for all crafts (with buff + drop-off applied) */
  total_xp: number
  /** Total estimated ingredient cost */
  estimated_cost: number
}

export interface LevelingPlanLevel {
  from_level: number
  to_level: number
  /** XP needed to complete this level */
  xp_needed: number
  /** XP assigned so far from entries */
  xp_accumulated: number
  entries: LevelingPlanEntry[]
}

export interface RecipeCompletionEntry {
  recipe_key: string
  completions: number
}

// ── Crafting history types ──────────────────────────────────────────────────

export interface CraftingHistoryRecipe {
  recipe_key: string
  recipe_name: string
  completions: number
  skill: string | null
  reward_skill: string | null
  skill_level_req: number | null
}

export interface SkillCraftingStats {
  skill_name: string
  total_recipes: number
  crafted_recipes: number
  total_completions: number
  uncrafted_count: number
  completion_percent: number
}

// ── Live crafting detection types ───────────────────────────────────────────

export interface CraftingTracker {
  /** Which project is being tracked (null for quick-calc) */
  project_id: number | null
  /** Recipe entries being tracked */
  entries: TrackedRecipeEntry[]
  /** When tracking started */
  started_at: string
  /** Whether tracking is active */
  active: boolean
}

export interface TrackedRecipeEntry {
  recipe_id: number
  recipe_name: string
  /** Item name of the primary output */
  output_item_name: string
  /** Item type ID of the primary output (for matching) */
  output_item_type_id: number | null
  /** Output per craft (batch size) */
  output_per_craft: number
  /** Target quantity (from project entry) */
  target_quantity: number
  /** How many output items detected so far */
  detected_output: number
  /** Estimated crafts completed */
  crafts_completed: number
  /** Baseline completion_count from RecipeUpdated at tracking start */
  baseline_completion_count: number | null
  /** Manual +/- adjustment (user overrides for missed/extra detections) */
  manual_adjustment: number
}

export interface CraftDetectionEvent {
  timestamp: string
  recipe_name: string
  item_name: string
  quantity: number
}

// ── Price Helper types ─────────────────────────────────────────────────────

export type MaterialPctBasis = "yours" | "theirs" | "total";

export interface FeeConfig {
  per_craft_fee: number;
  material_pct: number;
  material_pct_basis: MaterialPctBasis;
  flat_fee: number;
}

export const DEFAULT_FEE_CONFIG: FeeConfig = {
  per_craft_fee: 0,
  material_pct: 0,
  material_pct_basis: "total",
  flat_fee: 0,
};

export interface PriceHelperQuoteSummary {
  id: number;
  name: string;
  notes: string;
  created_at: string;
  updated_at: string;
  entry_count: number;
}

export interface PriceHelperQuoteEntry {
  id: number;
  quote_id: number;
  recipe_id: number;
  recipe_name: string;
  quantity: number;
  sort_order: number;
}

export interface PriceHelperQuote {
  id: number;
  name: string;
  notes: string;
  fee_config: FeeConfig;
  customer_provides: Record<string, number>;
  created_at: string;
  updated_at: string;
  entries: PriceHelperQuoteEntry[];
}

// ── Work order types ────────────────────────────────────────────────────────

/** Raw work order data from character snapshot */
export interface WorkOrderSnapshotData {
  active: string[]
  completed: string[]
  inventory_item_ids: number[]
}

/**
 * Cross-character work-order presence for a single quest key, read from each
 * character's latest snapshot. `category` is `work_order` or
 * `completed_work_order`.
 */
export interface WorkOrderPresence {
  character_name: string
  server_name: string
  quest_key: string
  category: string
}

/** Enriched work order for display */
export interface EnrichedWorkOrder {
  /** Quest internal name (e.g., "Carpentry_QualityMeleeStaves") */
  quest_key: string
  /** Display name from quest data */
  name: string
  /** Crafting skill this work order is for */
  craft_skill: string | null
  /** Item internal name to deliver */
  item_internal_name: string | null
  /** Item display name */
  item_name: string | null
  /** Quantity of items to deliver */
  quantity: number
  /** Industry XP reward */
  industry_xp: number
  /** Gold reward */
  gold_reward: number
  /** Industry level requirement */
  industry_level_req: number | null
  /** Whether the player has memorized/accepted this work order */
  is_active: boolean
  /** Whether the player has completed this work order before */
  is_completed: boolean
  /** Whether the scroll item is in inventory/storage (not yet memorized) */
  is_in_inventory: boolean
  /** Recipe that produces the required item (if found) */
  recipe_id: number | null
  recipe_name: string | null
}

/**
 * Lean, un-enriched work order for the "uncompleted work orders" backlog view.
 * Recipe resolution is deferred to project-creation time (the backlog can be
 * ~1,000 rows, so we never resolve recipes for the whole set up front).
 */
export interface WorkOrderTodo {
  quest_key: string
  name: string
  craft_skill: string | null
  item_internal_name: string | null
  quantity: number
  industry_xp: number
  gold_reward: number
  /** Player has this work order accepted right now */
  is_active: boolean
  /** Turn-in board NPC, e.g. "Fitz the Boatman" (parsed from quest text) */
  board_npc: string | null
  /** Turn-in board location, e.g. "Serbule docks" (parsed from quest text) */
  board_location: string | null
}
