<template>
  <div class="flex flex-col gap-2 overflow-y-auto pr-1">
    <!-- Regular mod slots -->
    <div v-if="maxMods > 0">
      <div class="flex items-center gap-2 mb-1.5">
        <h4 class="micro-label">
          Mod Slots ({{ assignedRegularMods.length }}/{{ maxMods }})
        </h4>
        <!-- Distribution info -->
        <div class="group relative">
          <span class="text-[10px] leading-none text-text-dim cursor-help border-b border-dotted border-text-dim/40">
            Slot Rules
          </span>
          <!-- Tooltip -->
          <div class="absolute left-0 top-full mt-1 z-20 hidden group-hover:block bg-surface-base border border-border-default rounded-md shadow-lg p-3 w-72 text-xs">
            <div class="font-semibold text-text-primary mb-1.5">{{ rarityLabel }} — {{ maxMods }} Mod Slots</div>

            <!-- Current skill counts -->
            <div v-if="skillModCounts.size > 0 || genericCount > 0" class="mb-2">
              <div class="text-[10px] text-text-muted uppercase tracking-wider mb-1">Currently Assigned</div>
              <div v-for="[skill, count] in skillModCounts" :key="skill" class="flex justify-between text-[10px] py-0.5">
                <span class="text-blue-400">{{ skill }}</span>
                <span class="text-text-dim">{{ count }} mod{{ count !== 1 ? 's' : '' }}</span>
              </div>
              <div v-if="genericCount > 0" class="flex justify-between text-[10px] py-0.5">
                <span class="text-text-secondary">Generic / Endurance</span>
                <span class="text-text-dim">{{ genericCount }} mod{{ genericCount !== 1 ? 's' : '' }}</span>
              </div>
            </div>

            <!-- What can be added -->
            <div v-if="constraints.emptySlots > 0" class="mb-2">
              <div class="text-[10px] text-text-muted uppercase tracking-wider mb-1">{{ constraints.emptySlots }} Slot{{ constraints.emptySlots !== 1 ? 's' : '' }} Remaining</div>
              <div class="text-[10px] text-text-dim leading-relaxed">
                <template v-if="constraints.canAddSkillMod && constraints.canAddGeneric">
                  Can add skill-specific or generic mods.
                </template>
                <template v-else-if="constraints.canAddSkillMod">
                  Remaining slots must be skill-specific (no generic).
                </template>
                <template v-else-if="constraints.canAddGeneric">
                  Remaining slots must be generic/endurance.
                </template>
              </div>
            </div>

            <!-- Valid configurations -->
            <div class="border-t border-border-default/50 pt-1.5">
              <div class="text-[10px] text-text-muted uppercase tracking-wider mb-1">Valid Configurations</div>
              <div class="text-[10px] text-text-dim leading-relaxed">
                <div v-for="(cfg, i) in constraints.validConfigs" :key="i">
                  <template v-if="cfg[0] > 0">{{ cfg[0] }} main</template>
                  <template v-if="cfg[1] > 0">{{ cfg[0] > 0 ? ' + ' : '' }}{{ cfg[1] }} aux</template>
                  <template v-if="cfg[2] > 0">{{ (cfg[0] > 0 || cfg[1] > 0) ? ' + ' : '' }}{{ cfg[2] }} generic</template>
                  <template v-if="cfg[0] === 0 && cfg[1] === 0 && cfg[2] > 0">{{ cfg[2] }} generic</template>
                </div>
              </div>
            </div>
          </div>
        </div>
      </div>
      <div class="flex flex-col gap-1.5">
        <ModSlotBar
          v-for="(mod, i) in modSlots"
          :key="i"
          :mod="mod"
          :accent-class="accentClassForMod(mod)"
          :label="mod ? '' : emptySlotLabel"
          @remove="removeMod(mod!)"
          @drop="(key) => onDropToSlot(key, false, i)" />
      </div>
      <!-- Over-limit warning -->
      <div v-if="overLimit" class="text-[10px] text-red-400 mt-1 px-1">
        Too many skill-specific mods. Check the slot rules for this rarity.
      </div>
    </div>
    <div v-else class="text-xs text-text-dim py-2 text-center">
      No mod slots at this rarity.
    </div>

    <!-- Augment slot (not available on belts) -->
    <div v-if="!isBelt" class="border-t border-border-default/50 pt-2">
      <h4 class="text-[10px] font-semibold text-purple-400 uppercase tracking-wider mb-1.5">
        Augment ({{ AUGMENT_CP_COST }} CP)
      </h4>
      <ModSlotBar
        :mod="currentAugment"
        label="No augment — use '+ Aug' in the mod browser"
        :is-augment="true"
        @remove="removeAugment"
        @drop="(key) => onDropToSlot(key, true)" />
    </div>

    <!-- Craft Points section -->
    <div v-if="cpBudget > 0" class="border-t border-border-default/50 pt-2">
      <div class="flex items-center justify-between mb-1.5">
        <h4 class="text-[10px] font-semibold text-amber-400 uppercase tracking-wider">
          Craft Points
        </h4>
        <span class="text-[10px] text-text-dim">
          {{ cpRemaining }}/{{ cpBudget }} CP remaining
        </span>
      </div>

      <CpProgressBar :used="cpUsed" :budget="cpBudget" class="mb-2" />

      <!-- Assigned CP recipes (grouped with counters) -->
      <div v-if="groupedAssignedRecipes.length > 0" class="flex flex-col gap-1 mb-2">
        <div
          v-for="group in groupedAssignedRecipes"
          :key="group.recipe_id"
          class="flex items-center gap-2 px-2 py-1 rounded text-xs bg-surface-elevated border border-border-default group">
          <span class="text-[10px] font-semibold text-amber-400 uppercase">{{ group.typeLabel }}</span>
          <span class="text-text-primary truncate flex-1">{{ group.recipe_name }}</span>
          <span class="text-[10px] text-text-dim shrink-0">{{ group.cp_cost }} CP</span>
          <span v-if="group.count > 1" class="text-[10px] text-accent-gold font-semibold shrink-0">
            x{{ group.count }}
          </span>
          <button
            class="text-red-400/60 hover:text-red-400 text-[10px] opacity-0 group-hover:opacity-100 transition-opacity cursor-pointer shrink-0"
            title="Remove one"
            @click="removeOneRecipe(group.recipe_id)">
            &minus;
          </button>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import { useBuildPlannerStore } from '../../../stores/buildPlannerStore'
import { AUGMENT_CP_COST, getSlotCraftingPoints, getRarityDef, computeSlotConstraints } from '../../../types/buildPlanner'
import ModSlotBar from './ModSlotBar.vue'
import CpProgressBar from './CpProgressBar.vue'

const store = useBuildPlannerStore()

const maxMods = computed(() => store.maxModsPerSlot)

const assignedRegularMods = computed(() =>
  store.selectedSlotMods.filter(m => !m.is_augment)
)

const slotPrimarySkill = computed(() =>
  store.selectedSlot ? store.getSlotSkillPrimary(store.selectedSlot) : null
)
const slotSecondarySkill = computed(() =>
  store.selectedSlot ? store.getSlotSkillSecondary(store.selectedSlot) : null
)

/**
 * Group rank for ordering regular mods: primary-skill mods first, then
 * secondary-skill mods, then everything else (other skills / generic).
 * The augment lives in its own section and is unaffected.
 */
function skillGroupRank(mod: typeof assignedRegularMods.value[number]): number {
  const skill = getModSkill(mod.power_name)
  if (skill && slotPrimarySkill.value && skill === slotPrimarySkill.value) return 0
  if (skill && slotSecondarySkill.value && skill === slotSecondarySkill.value) return 1
  return 2
}

/**
 * Left-edge accent color per skill group, matching the skill dropdown colors
 * in the slot header (primary = blue, secondary = emerald, other/generic = slate).
 */
function accentClassForMod(mod: typeof assignedRegularMods.value[number] | null): string {
  if (!mod) return ''
  switch (skillGroupRank(mod)) {
    case 0: return 'border-l-4 border-l-blue-400/70'
    case 1: return 'border-l-4 border-l-emerald-400/70'
    default: return 'border-l-4 border-l-slate-500/50'
  }
}

/** Regular mods ordered by skill group, then by sort_order within each group */
const orderedRegularMods = computed(() =>
  [...assignedRegularMods.value].sort((a, b) => {
    const ra = skillGroupRank(a)
    const rb = skillGroupRank(b)
    if (ra !== rb) return ra - rb
    return a.sort_order - b.sort_order
  })
)

const modSlots = computed(() => {
  const slots: (typeof assignedRegularMods.value[number] | null)[] = []
  const mods = orderedRegularMods.value
  for (let i = 0; i < maxMods.value; i++) {
    slots.push(mods[i] ?? null)
  }
  return slots
})

const currentAugment = computed(() =>
  store.selectedSlotMods.find(m => m.is_augment) ?? null
)

const isBelt = computed(() => store.selectedSlot === 'Belt')

const rarityDef = computed(() => {
  if (!store.selectedSlot) return getRarityDef('Epic')
  return getRarityDef(store.getSlotRarity(store.selectedSlot))
})

const rarityLabel = computed(() => rarityDef.value.label)

/** Resolve which skill a mod belongs to */
function getModSkill(powerName: string): string | null {
  const power = store.slotPowers.find(p => (p.internal_name ?? p.key) === powerName)
  return power?.skill ?? null
}

function isGenericSkill(skill: string | null): boolean {
  return !skill || skill === 'AnySkill' || skill === 'Endurance'
}

/** Count mods per combat skill (excluding generic/endurance) */
const skillModCounts = computed(() => {
  const counts = new Map<string, number>()
  for (const m of assignedRegularMods.value) {
    const skill = getModSkill(m.power_name)
    if (isGenericSkill(skill)) continue
    counts.set(skill!, (counts.get(skill!) ?? 0) + 1)
  }
  return counts
})

const genericCount = computed(() =>
  assignedRegularMods.value.filter(m => isGenericSkill(getModSkill(m.power_name))).length
)

/** Constraint solver — determines what can be added based on reachable configs */
const constraints = computed(() => {
  if (!store.selectedSlot) return computeSlotConstraints('Epic', new Map(), 0)
  const rarity = store.getSlotRarity(store.selectedSlot)
  return computeSlotConstraints(rarity, skillModCounts.value, genericCount.value)
})

const overLimit = computed(() => constraints.value.validConfigs.length === 0 && assignedRegularMods.value.length > 0)

/** Compute a descriptive label for empty slots based on current constraints */
const emptySlotLabel = computed(() => {
  if (isBelt.value) return 'Generic only (no skill mods on belts)'

  const c = constraints.value
  if (c.emptySlots === 0) return 'Full'

  const parts: string[] = []
  const growable = [...c.growableSkills]

  // List specific growable skills
  if (growable.length > 0) {
    parts.push(...growable)
  }

  // Can a new (not yet used) skill be added?
  if (c.canAddNewSkill) {
    parts.push('other skill')
  }

  if (c.canAddGeneric) {
    parts.push('Generic')
  }

  if (parts.length === 0) return 'No valid mods'
  if (parts.length === 1 && parts[0] === 'Generic') return 'Generic / Endurance only'
  if (!c.canAddGeneric && !c.canAddNewSkill && growable.length === 1) return `${growable[0]} only`

  // Check if this is effectively "any mod"
  if (c.canAddGeneric && c.canAddNewSkill && skillModCounts.value.size === 0) return 'Any mod'

  return parts.join(', ')
})

/** Check if adding a mod with the given skill is allowed by the constraint solver */
function canAddModWithSkill(modSkill: string | null): boolean {
  if (assignedRegularMods.value.length >= maxMods.value) return false
  // Belts only accept generic mods (no skill-specific, not even endurance)
  if (isBelt.value) return !modSkill || modSkill === 'AnySkill'
  if (isGenericSkill(modSkill)) return constraints.value.canAddGeneric
  // Existing skill — check if that specific skill can grow
  if (skillModCounts.value.has(modSkill!)) return constraints.value.growableSkills.has(modSkill!)
  // New skill — check if we can introduce a new combat skill
  return constraints.value.canAddNewSkill
}

const cpBudget = computed(() => {
  if (!store.selectedSlot) return 0
  return getSlotCraftingPoints(store.getSlotItem(store.selectedSlot))
})

const cpUsed = computed(() => {
  if (!store.selectedSlot) return 0
  return store.getSlotCpUsed(store.selectedSlot)
})

const cpRemaining = computed(() => cpBudget.value - cpUsed.value)

interface GroupedRecipe {
  recipe_id: number
  recipe_name: string
  cp_cost: number
  typeLabel: string
  count: number
}

const groupedAssignedRecipes = computed((): GroupedRecipe[] => {
  const groups = new Map<number, GroupedRecipe>()
  for (const r of store.selectedSlotCpRecipes) {
    const existing = groups.get(r.recipe_id)
    if (existing) {
      existing.count++
    } else {
      groups.set(r.recipe_id, {
        recipe_id: r.recipe_id,
        recipe_name: r.recipe_name ?? 'Unknown',
        cp_cost: r.cp_cost,
        typeLabel: r.effect_type === 'shamanic_infusion' ? 'Infusion'
          : r.effect_type === 'crafting_enhancement' ? 'Enhance' : 'CP',
        count: 1,
      })
    }
  }
  return Array.from(groups.values())
})

function removeOneRecipe(recipeId: number) {
  const recipes = [...store.selectedSlotCpRecipes]
  for (let i = recipes.length - 1; i >= 0; i--) {
    if (recipes[i].recipe_id === recipeId) {
      store.removeCpRecipe(recipes[i])
      return
    }
  }
}

function removeMod(mod: typeof assignedRegularMods.value[number]) {
  store.removeMod(mod)
}

function removeAugment() {
  if (currentAugment.value) {
    store.removeMod(currentAugment.value)
  }
}

function onDropToSlot(powerKey: string, isAugment: boolean, slotIndex?: number) {
  const power = store.slotPowers.find(p => p.key === powerKey)
  if (!power) return

  const powerName = power.internal_name ?? power.key
  const alreadyAssigned = store.selectedSlotMods.some(m =>
    m.power_name === powerName && m.is_augment === isAugment
  )
  if (alreadyAssigned) return

  if (isAugment) {
    if (currentAugment.value) store.removeMod(currentAugment.value)
  } else {
    // Check skill count limits
    if (!canAddModWithSkill(power.skill)) return

    if (assignedRegularMods.value.length >= maxMods.value) {
      // Slots full — replace the mod at the dropped position
      if (slotIndex == null) return
      const target = orderedRegularMods.value[slotIndex]
      if (target) store.removeMod(target)
      else return
    }
  }

  const tierId = power.tier_id ?? undefined
  store.addMod(power, isAugment, tierId)
}
</script>
