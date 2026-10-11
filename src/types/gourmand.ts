export type FoodCategory = 'Meal' | 'Snack' | 'Instant-Snack'

/**
 * The categories the Gourmand screen has a section for. Ingestion canonicalises
 * these tokens (`canonical_food_category` in `cdn_persistence.rs`), so anything
 * else is either a brand-new game category or a spelling drift that needs that
 * function updated — the view buckets such foods under "Unknown Categories"
 * rather than dropping them.
 */
export const KNOWN_FOOD_CATEGORIES: readonly FoodCategory[] = ['Meal', 'Snack', 'Instant-Snack']

export function isKnownFoodCategory(category: string): category is FoodCategory {
  return (KNOWN_FOOD_CATEGORIES as readonly string[]).includes(category)
}

/**
 * Where a food comes from. Derived during CDN ingestion by
 * `game_data::food_sources` — keep in sync with the `KIND_*` constants there.
 */
export type FoodSourceKind =
  | 'unobtainable'
  | 'crafted'
  | 'event'
  | 'vendor'
  | 'quest'
  | 'npc-gift'
  | 'hangout'
  | 'barter'
  | 'container'
  | 'other'

export interface FoodItem {
  item_id: number
  name: string
  icon_id: number | null
  /**
   * Category token stored at ingest: `Meal`, `Snack`, or `Instant-Snack` when
   * recognised, otherwise the raw CDN token (see `isKnownFoodCategory`).
   */
  food_category: string
  food_level: number
  gourmand_req: number | null
  effect_descs: string[]
  keywords: string[]
  value: number | null
  /** All applicable source kinds, most-significant first. */
  source_kinds: FoodSourceKind[]
  /** Display names of the skills that can craft this food. */
  craft_skills: string[]
}

export interface GourmandFoodEntry {
  name: string
  count: number
  manually_marked: boolean
}

export interface GourmandImportResult {
  foods_imported: number
}

// ── Source kind presentation ─────────────────────────────────────────────────

/**
 * Ordering matches the backend's priority, so this doubles as the sort key for
 * the "Source" sort mode.
 */
export const FOOD_SOURCE_ORDER: FoodSourceKind[] = [
  'crafted',
  'vendor',
  'quest',
  'npc-gift',
  'hangout',
  'barter',
  'container',
  'event',
  'other',
  'unobtainable',
]

const SOURCE_LABELS: Record<FoodSourceKind, string> = {
  unobtainable: 'Unobtainable',
  crafted: 'Crafted',
  event: 'Event',
  vendor: 'Vendor',
  quest: 'Quest',
  'npc-gift': 'NPC Gift',
  hangout: 'Hangout',
  barter: 'Barter',
  container: 'Container',
  other: 'Drop / Gather',
}

const SOURCE_DESCRIPTIONS: Record<FoodSourceKind, string> = {
  unobtainable: 'Flagged as no longer obtainable in the game data',
  crafted: 'Made from a recipe',
  event: 'Handed out during a seasonal event or as a commemorative gift',
  vendor: 'Sold by a vendor',
  quest: 'Quest reward',
  'npc-gift': 'Given as an NPC favor gift',
  hangout: 'Earned from an NPC hangout',
  barter: 'Bartered from an NPC',
  container: 'Found inside another item',
  other: 'Dropped, foraged, butchered, or otherwise ungated by the game data',
}

/** The kind a food is filed under — its badge, and its "Source" sort key. */
export function primaryFoodSource(food: FoodItem): FoodSourceKind {
  return food.source_kinds[0] ?? 'other'
}

/** Position in `FOOD_SOURCE_ORDER`; drives the "Source" sort mode. */
export function foodSourceRank(food: FoodItem): number {
  const rank = FOOD_SOURCE_ORDER.indexOf(primaryFoodSource(food))
  return rank === -1 ? FOOD_SOURCE_ORDER.length : rank
}

/** Badge text: the craft skill when known, otherwise the kind's label. */
export function foodSourceLabel(food: FoodItem): string {
  const kind = primaryFoodSource(food)
  if (kind === 'crafted' && food.craft_skills.length > 0) {
    return food.craft_skills.length === 1 ? food.craft_skills[0] : 'Crafted'
  }
  return SOURCE_LABELS[kind]
}

/** Tooltip text spelling out every source the food has. */
export function foodSourceTitle(food: FoodItem): string {
  if (food.source_kinds.length === 0) return SOURCE_DESCRIPTIONS.other
  return food.source_kinds
    .map(kind =>
      kind === 'crafted' && food.craft_skills.length > 0
        ? `Crafted (${food.craft_skills.join(', ')})`
        : SOURCE_DESCRIPTIONS[kind],
    )
    .join(' · ')
}

export function foodSourceKindLabel(kind: FoodSourceKind): string {
  return SOURCE_LABELS[kind]
}

/**
 * Tailwind classes for the badge. The badge is only allowed to shout for the
 * foods you can't just go and make — everything else stays quiet so it doesn't
 * compete with the eaten/manual/unusable colouring the rows already carry.
 * (Green and blue are deliberately avoided here: they mean eaten and manually
 * marked on this screen.)
 */
export function foodSourceClasses(food: FoodItem): string {
  switch (primaryFoodSource(food)) {
    case 'event':
      return 'bg-accent-warning/10 text-accent-warning border-accent-warning/30'
    case 'unobtainable':
      return 'bg-surface-dark text-text-dim border-border-default line-through'
    default:
      return 'bg-surface-elevated text-text-muted border-border-default'
  }
}

/**
 * Foods you can still realistically go and get. Excludes the ones the game
 * data flags as unobtainable and the event-only ones you can't work toward
 * outside their event window.
 */
export function isAttainable(food: FoodItem): boolean {
  const kind = primaryFoodSource(food)
  return kind !== 'unobtainable' && kind !== 'event'
}
