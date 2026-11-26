// ── Build Planner Types ─────────────────────────────────────────────────────
//
// Byte-identical shapes are re-exported from the generated Specta bindings
// (`buildPlanner.specta.ts`, `cdn.specta.ts`). The hand definitions that remain
// differ from the bindings: `BuildPresetAbility.bar` and
// `CpRecipeOption.effect_type` are narrow string-literal unions here but plain
// `string` in the bindings. Everything else below has no generated counterpart.

export type {
  BuildPreset,
  BuildPresetMod,
  BuildPresetSlotItem,
  BuildPresetAbilityInput,
  BuildPresetModInput,
  BuildPresetCpRecipe,
  EquippedGearMod,
  EquippedCandidate,
  EquippedSlotCandidates,
} from './buildPlanner.specta'
import type { TsysTierSummary } from './cdn.specta'
export type { TsysTierSummary }
import type {
  BuildPresetSlotItem,
  EquippedCandidate,
  EquippedSlotCandidates,
} from './buildPlanner.specta'

/** A saved ability assignment in a build bar */
export interface BuildPresetAbility {
  id: number
  preset_id: number
  bar: 'primary' | 'secondary' | 'sidebar'
  slot_position: number
  ability_id: number
  ability_name: string | null
}

/** A CP-consuming recipe option returned from CDN query */
export interface CpRecipeOption {
  recipe_id: number
  recipe_name: string
  icon_id: number | null
  skill: string | null
  skill_level_req: number | null
  cp_cost: number
  effect_type: 'shamanic_infusion' | 'crafting_enhancement'
  effect_key: string
  effect_description: string
}

/** Result of fetching equipped-gear candidates for the loadout picker */
export interface EquippedCandidatesResult {
  reason: 'no-preset' | 'no-character' | 'no-gear' | null
  slots: EquippedSlotCandidates[]
}

/** A user-confirmed slot→item choice to apply from the loadout picker */
export interface EquippedSelection {
  equip_slot: string
  item: EquippedCandidate
}

export interface SlotTsysPower {
  key: string
  internal_name: string | null
  skill: string | null
  prefix: string | null
  suffix: string | null
  tier_id: string | null
  effects: string[]
  raw_effects: string[]
  min_rarity: string | null
  skill_level_prereq: number | null
  icon_id: number | null
  available_tiers: TsysTierSummary[]
  /** Equipment slots this power can appear on */
  slots: string[]
}

/** Equipment slot definition for the build planner */
export interface EquipSlotDef {
  id: string
  label: string
  group: 'armor' | 'weapon' | 'jewelry' | 'extra'
  /** Index into RARITY_DEFS for the highest allowed rarity. Undefined = no cap. */
  maxRarityIndex?: number
  /** Default rarity for this slot (overrides the preset's target rarity). */
  defaultRarity?: string
  /** Slot holds fixed-effect items only — no TSys mods, no rarity/level/CP
   *  (e.g. Racial jewelry: the item's innate EffectDescs are its whole stat
   *  contribution). Drives the simplified slot detail UI. */
  noMods?: boolean
}

/** Rarity levels with their mod slot distributions.
 *  primarySlots/secondarySlots represent the default distribution of skill-specific
 *  mod slots. Any skill slot can be replaced by generic/endurance. */
export interface RarityDef {
  id: string
  label: string
  totalMods: number
  /** Number of mod slots defaulting to primary skill */
  primarySlots: number
  /** Number of mod slots defaulting to secondary skill */
  secondarySlots: number
}

// ── Constants ───────────────────────────────────────────────────────────────

export const EQUIPMENT_SLOTS: EquipSlotDef[] = [
  { id: 'Head', label: 'Head', group: 'armor' },
  { id: 'Chest', label: 'Chest', group: 'armor' },
  { id: 'Legs', label: 'Legs', group: 'armor' },
  { id: 'Hands', label: 'Hands', group: 'armor' },
  { id: 'Feet', label: 'Feet', group: 'armor' },
  { id: 'MainHand', label: 'Main Hand', group: 'weapon' },
  { id: 'OffHand', label: 'Off Hand', group: 'weapon' },
  { id: 'Ring', label: 'Ring', group: 'jewelry' },
  { id: 'Necklace', label: 'Necklace', group: 'jewelry' },
  { id: 'Belt', label: 'Belt', group: 'extra', maxRarityIndex: 1, defaultRarity: 'Uncommon' },
  // Racial jewelry (tail/nose/navel rings, earrings): fixed-effect items with no
  // TSys mod rolls — zero powers list the Racial slot — so it contributes only
  // its innate EffectDescs.
  { id: 'Racial', label: 'Racial', group: 'extra', maxRarityIndex: 0, defaultRarity: 'Common', noMods: true },
]

export const RARITY_DEFS: RarityDef[] = [
  { id: 'Common', label: 'Common', totalMods: 0, primarySlots: 0, secondarySlots: 0 },
  { id: 'Uncommon', label: 'Uncommon', totalMods: 3, primarySlots: 1, secondarySlots: 0 },
  { id: 'Rare', label: 'Rare', totalMods: 3, primarySlots: 1, secondarySlots: 1 },
  { id: 'Exceptional', label: 'Exceptional', totalMods: 3, primarySlots: 2, secondarySlots: 1 },
  { id: 'Epic', label: 'Epic', totalMods: 4, primarySlots: 2, secondarySlots: 2 },
  { id: 'Legendary', label: 'Legendary', totalMods: 5, primarySlots: 3, secondarySlots: 2 },
]

/** Valid mod configurations per rarity.
 *  Each config is [mainSkillCount, auxSkillCount, genericCount].
 *  "main" is whichever combat skill ends up with more mods.
 *  "aux" is the other combat skill. "generic" includes endurance. */
export type ModConfig = [main: number, aux: number, generic: number]

/** Any one combat skill can claim at most 3 of an item's mod slots — the same cap
 *  for either skill on the item, at every rarity. Generic mods (including
 *  endurance) are unconstrained filler and can take any slot a skill mod doesn't,
 *  which is why crafted items routinely come out as e.g. 3 shield + 2 generic or
 *  2 shield + 3 generic. So rarity only decides how many slots there are; the
 *  distribution rules are this cap, at most two combat skills per item, and
 *  `aux <= main` (main is by definition the skill with more mods). */
export const MAX_MODS_PER_SKILL = 3

/** Enumerate every [main, aux, generic] split a rarity allows, skill-heavy first. */
function buildRarityConfigs(totalMods: number): ModConfig[] {
  if (totalMods === 0) return []

  const configs: ModConfig[] = []
  for (let main = Math.min(MAX_MODS_PER_SKILL, totalMods); main >= 0; main--) {
    for (let aux = Math.min(MAX_MODS_PER_SKILL, main, totalMods - main); aux >= 0; aux--) {
      configs.push([main, aux, totalMods - main - aux])
    }
  }
  return configs
}

export const RARITY_CONFIGS: Record<string, ModConfig[]> = Object.fromEntries(
  RARITY_DEFS.map(r => [r.id, buildRarityConfigs(r.totalMods)]),
)

/** Given the current mod counts on an item, determine what types of mods can still be added.
 *  Returns { canAddSkillMod: boolean, canAddGeneric: boolean, mustBeSkill: boolean }
 *  where mustBeSkill means the remaining slots MUST be skill mods (no generic allowed). */
export interface SlotConstraints {
  /** Can a mod from an existing combat skill be added? (any existing skill) */
  canAddSkillMod: boolean
  /** Can a generic/endurance mod be added? */
  canAddGeneric: boolean
  /** Can a mod from a NEW (third) combat skill be added? */
  canAddNewSkill: boolean
  /** Which existing skills can still accept more mods */
  growableSkills: Set<string>
  /** Remaining empty slot count */
  emptySlots: number
  /** Which configs are still reachable */
  validConfigs: ModConfig[]
}

/**
 * Compute what mods can be placed given the current state.
 * @param rarity - item rarity ID
 * @param skillCounts - map of combat skill name → count of mods from that skill
 * @param genericCount - count of generic/endurance mods
 */
export function computeSlotConstraints(
  rarity: string,
  skillCounts: Map<string, number>,
  genericCount: number,
): SlotConstraints {
  const configs = RARITY_CONFIGS[rarity] ?? []
  const totalMods = getRarityDef(rarity).totalMods
  const usedSlots = [...skillCounts.values()].reduce((a, b) => a + b, 0) + genericCount
  const emptySlots = totalMods - usedSlots

  // Get sorted skill counts (highest first)
  const sorted = [...skillCounts.entries()].sort((a, b) => b[1] - a[1])
  const skill1Name = sorted[0]?.[0] ?? null
  const skill1Count = sorted[0]?.[1] ?? 0
  const skill2Name = sorted[1]?.[0] ?? null
  const skill2Count = sorted[1]?.[1] ?? 0
  const numSkills = sorted.filter(([, c]) => c > 0).length

  const fits = (m: number, a: number, g: number) =>
    skill1Count <= m && skill2Count <= a && genericCount <= g &&
    m + a + g === totalMods

  if (emptySlots <= 0) {
    // All slots filled — check if current state matches any valid config
    const isValid = configs.some(([main, aux, gen]) =>
      fits(main, aux, gen) || fits(aux, main, gen)
    )
    return {
      canAddSkillMod: false, canAddGeneric: false, canAddNewSkill: false,
      growableSkills: new Set(), emptySlots: 0,
      validConfigs: isValid ? configs.filter(([main, aux, gen]) => fits(main, aux, gen) || fits(aux, main, gen)) : [],
    }
  }

  // Find which configs are still reachable.
  // For each config [main, aux, gen], try assigning skill1 as main or aux.

  const validConfigs = configs.filter(([main, aux, gen]) =>
    fits(main, aux, gen) || fits(aux, main, gen)
  )

  let canAddSkillMod = false
  let canAddGeneric = false
  let canAddNewSkill = false
  const growableSkills = new Set<string>()

  for (const [main, aux, gen] of validConfigs) {
    const tryAssignment = (m: number, a: number, g: number) => {
      if (skill1Count <= m && skill2Count <= a && genericCount <= g) {
        if (skill1Count < m && skill1Name) { canAddSkillMod = true; growableSkills.add(skill1Name) }
        if (skill2Count < a && skill2Name) { canAddSkillMod = true; growableSkills.add(skill2Name) }
        if (genericCount < g) canAddGeneric = true
        if (numSkills < 2 && a > 0 && skill2Count === 0) canAddNewSkill = true
        if (numSkills === 0 && m > 0) canAddNewSkill = true
      }
    }
    tryAssignment(main, aux, gen)
    tryAssignment(aux, main, gen)
  }

  return { canAddSkillMod, canAddGeneric, canAddNewSkill, growableSkills, emptySlots, validConfigs }
}

export const ABILITY_BARS = [
  { id: 'primary' as const, label: 'Primary', slots: 6 },
  { id: 'secondary' as const, label: 'Secondary', slots: 6 },
  { id: 'sidebar' as const, label: 'Sidebar', slots: 6 },
]

/** Maximum number of sidebar slots a user can configure */
export const MAX_SIDEBAR_SLOTS = 12

/**
 * Combat sub-skills whose abilities and gear mods fold into a parent combat
 * skill in the build planner. Fairy Magic is a Mentalism sub-skill flagged
 * `AuxCombat`/`Combat: false` in the CDN, so it never appears in the standalone
 * combat-skill list — but it has its own abilities and mods that Mentalism
 * builds use. Surfacing them under the parent's selection keeps them reachable
 * without adding a skill the game doesn't let you pick as a bar skill.
 *
 * Keys and values are display names (matching preset skill_primary/secondary
 * and the resolved `skill` on abilities and mods).
 */
export const CHILD_COMBAT_SKILLS: Record<string, string[]> = {
  Mentalism: ['Fairy Magic'],
}

/** Expand a build-planner combat skill into itself plus any folded child skills. */
export function expandBuildSkill(skill: string | null | undefined): string[] {
  if (!skill) return []
  const children = CHILD_COMBAT_SKILLS[skill]
  return children ? [skill, ...children] : [skill]
}

export function getRarityDef(rarity: string): RarityDef {
  return RARITY_DEFS.find(r => r.id === rarity) ?? RARITY_DEFS[4] // default to Epic
}

/** Get the allowed rarities for a specific slot (belt is limited to Common/Uncommon) */
export function getAllowedRarities(slot: EquipSlotDef): RarityDef[] {
  if (slot.maxRarityIndex != null) {
    return RARITY_DEFS.slice(0, slot.maxRarityIndex + 1)
  }
  // Non-belt slots: skip Common (index 0) since regular equipment can't be Common in the planner
  return RARITY_DEFS.slice(1)
}

/** Get the default rarity for a slot (belt defaults to Uncommon, others use preset target) */
export function getDefaultRarityForSlot(slot: EquipSlotDef): string {
  return slot.defaultRarity ?? 'Epic'
}

/** Rarity text color classes (for dropdowns, labels, etc.) */
export function getRarityTextColor(rarity: string): string {
  switch (rarity) {
    case 'Legendary': return 'text-yellow-400'
    case 'Epic': return 'text-purple-400'
    case 'Exceptional': return 'text-blue-400'
    case 'Rare': return 'text-emerald-400'
    case 'Uncommon': return 'text-text-primary'
    case 'Common': return 'text-text-dim'
    default: return 'text-text-muted'
  }
}

/** Rarity border color classes (for slot indicators, cards, etc.) */
export function getRarityBorderColor(rarity: string): string {
  switch (rarity) {
    case 'Legendary': return 'border-yellow-500/60'
    case 'Epic': return 'border-purple-500/60'
    case 'Exceptional': return 'border-blue-500/60'
    case 'Rare': return 'border-emerald-500/60'
    case 'Uncommon': return 'border-border-default'
    case 'Common': return 'border-border-default/50'
    default: return 'border-border-default'
  }
}

/** Cost in crafting points to apply an augment */
export const AUGMENT_CP_COST = 100

/** Calculate total crafting points budget for a slot based on its properties.
 *  Belts and fixed-effect slots (Racial) get 0 CP regardless of status.
 *  Mastercrafted/foretold legendaries are a flat 160 CP.
 *  Otherwise: crafted = 120, dropped = 100. */
export function getSlotCraftingPoints(slotItem: BuildPresetSlotItem | undefined): number {
  if (!slotItem) return 0
  if (slotItem.equip_slot === 'Belt') return 0
  if (EQUIPMENT_SLOTS.find(s => s.id === slotItem.equip_slot)?.noMods) return 0
  if (slotItem.is_masterwork) return 160
  return slotItem.is_crafted ? 120 : 100
}

/** Armor type keywords used to detect material type */
export const ARMOR_TYPE_KEYWORDS = ['ClothArmor', 'LeatherArmor', 'MetalArmor', 'OrganicArmor'] as const
export type ArmorType = 'Cloth' | 'Leather' | 'Metal' | 'Organic'

/** Map keyword to display name */
export function getArmorTypeFromKeywords(keywords: string[]): ArmorType | null {
  if (keywords.includes('ClothArmor')) return 'Cloth'
  if (keywords.includes('LeatherArmor')) return 'Leather'
  if (keywords.includes('MetalArmor')) return 'Metal'
  if (keywords.includes('OrganicArmor')) return 'Organic'
  return null
}

/** Armor slots that count toward 3-piece set bonus */
export const ARMOR_SET_SLOTS = ['Head', 'Chest', 'Legs', 'Hands', 'Feet']

/** Get max mods for a specific rarity */
export function getMaxModsForRarity(rarity: string): number {
  return getRarityDef(rarity).totalMods
}

/** Get a display name for a TSys power */
export function getPowerDisplayName(power: SlotTsysPower): string {
  return power.prefix ?? power.suffix ?? power.internal_name ?? power.key
}
