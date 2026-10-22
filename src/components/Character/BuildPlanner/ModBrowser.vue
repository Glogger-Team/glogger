<template>
  <div class="flex flex-col gap-2 min-h-0 h-full">
    <!-- Filter controls -->
    <div class="flex flex-col gap-1.5 shrink-0">
      <div class="flex items-center gap-2">
        <StyledSelect
          v-model="skillFilter"
          :options="skillFilterOptions"
          size="xs"
          class="flex-1 min-w-0" />
        <label
          v-if="store.presetAbilities.length > 0"
          class="flex items-center gap-1 text-[10px] text-text-muted cursor-pointer shrink-0"
          title="Show only mods that affect your assigned abilities">
          <input
            v-model="myAbilitiesFilter"
            type="checkbox"
            class="w-2.5 h-2.5 cursor-pointer" />
          <span>My abilities</span>
        </label>
        <label
          class="flex items-center gap-1 text-[10px] text-text-muted cursor-pointer shrink-0"
          :title="knownAbilitiesTitle">
          <input
            v-model="knownAbilitiesFilter"
            type="checkbox"
            :disabled="!snapshotHasAbilityRows"
            class="w-2.5 h-2.5 cursor-pointer disabled:opacity-40" />
          <span>Known abilities</span>
        </label>
      </div>
      <div class="flex items-center gap-2">
        <input
          v-model="store.modFilter"
          type="text"
          placeholder="Filter mods..."
          class="bg-surface-elevated border border-border-default rounded px-2 py-0.5 text-xs text-text-primary flex-1 min-w-0" />
        <label
          class="flex items-center gap-1 text-[10px] text-text-muted cursor-pointer shrink-0"
          title="Hide effect details">
          <input
            v-model="compactMode"
            type="checkbox"
            class="w-2.5 h-2.5 cursor-pointer" />
          <span>Compact</span>
        </label>
      </div>
      <div class="text-[10px] text-text-dim">
        {{ filteredPowers.length }} mods available
      </div>
    </div>

    <!-- Loading -->
    <div v-if="store.loadingPowers" class="text-xs text-text-muted py-4 text-center">
      Loading mods...
    </div>

    <!-- Mod list -->
    <div v-else class="flex-1 overflow-y-auto flex flex-col gap-1">
      <ModBrowserItem
        v-for="power in filteredPowers"
        :key="power.key"
        :power="power"
        :compact="compactMode"
        @add="(tierId) => addMod(power, tierId)"
        @add-augment="(tierId) => addAugment(power, tierId)" />

      <div v-if="filteredPowers.length === 0" class="text-xs text-text-dim text-center py-4">
        No mods match your filters.
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, watch } from 'vue'
import { useBuildPlannerStore } from '../../../stores/buildPlannerStore'
import { useCharacterStore } from '../../../stores/characterStore'
import type { SlotTsysPower } from '../../../types/buildPlanner'
import { computeSlotConstraints, expandBuildSkill } from '../../../types/buildPlanner'
import { useBuildCrossRef } from '../../../composables/useBuildCrossRef'
import StyledSelect from '../../Shared/StyledSelect.vue'
import ModBrowserItem from './ModBrowserItem.vue'

const store = useBuildPlannerStore()
const characterStore = useCharacterStore()
const { abilityBaseNames, isAbilityRelated } = useBuildCrossRef()

const skillFilter = ref('__build__')
const myAbilitiesFilter = ref(false)
const knownAbilitiesFilter = ref(false)
const compactMode = ref(false)

/** Post-v74 snapshot rows present → the toggle means something. */
const snapshotHasAbilityRows = computed(() => characterStore.snapshotAbilities.length > 0)
const knownAbilitiesTitle = computed(() =>
  snapshotHasAbilityRows.value
    ? 'Show only mods that affect snapshot-known abilities'
    : 'Select a post-import snapshot to enable',
)

const skillFilterOptions = computed(() => {
  const primary = store.selectedSlot ? store.getSlotSkillPrimary(store.selectedSlot) : store.activePreset?.skill_primary
  const secondary = store.selectedSlot ? store.getSlotSkillSecondary(store.selectedSlot) : store.activePreset?.skill_secondary
  const buildLabel = [primary, secondary].filter(Boolean).join(' + ') || 'Build Skills'

  const options: { value: string; label: string }[] = [
    { value: '__build__', label: `${buildLabel} + Generic` },
    { value: '', label: 'All Skills' },
    { value: '__generic__', label: 'Generic Only' },
    { value: 'Endurance', label: 'Endurance Only' },
  ]
  const seen = new Set<string>()
  for (const skill of [primary, secondary]) {
    // Include each build skill plus any folded child skills (e.g. Fairy Magic
    // under Mentalism) as their own "X Only" filter.
    for (const s of expandBuildSkill(skill)) {
      if (seen.has(s)) continue
      seen.add(s)
      options.push({ value: s, label: `${s} Only` })
    }
  }
  return options
})

// Reset filter when slot changes
watch(() => store.selectedSlot, () => {
  skillFilter.value = '__build__'
})

function isGenericPower(power: { skill: string | null }): boolean {
  return !power.skill || power.skill === 'AnySkill'
}

const filteredPowers = computed(() => {
  let powers = store.filteredPowers

  // Apply skill filter
  if (skillFilter.value === '__build__') {
    const primary = store.selectedSlot ? store.getSlotSkillPrimary(store.selectedSlot) : store.activePreset?.skill_primary
    const secondary = store.selectedSlot ? store.getSlotSkillSecondary(store.selectedSlot) : store.activePreset?.skill_secondary
    // Expand each build skill to include folded child skills (e.g. Fairy Magic
    // mods surface under a Mentalism build).
    const buildSkills = new Set([...expandBuildSkill(primary), ...expandBuildSkill(secondary)])
    powers = powers.filter(p =>
      isGenericPower(p) ||
      p.skill === 'Endurance' ||
      (p.skill != null && buildSkills.has(p.skill))
    )
  } else if (skillFilter.value === '__generic__') {
    powers = powers.filter(p => isGenericPower(p))
  } else if (skillFilter.value) {
    powers = powers.filter(p => p.skill === skillFilter.value)
  }

  // Apply ability filter
  if (myAbilitiesFilter.value && abilityBaseNames.value.size > 0) {
    powers = powers.filter(p => isAbilityRelated(p.internal_name ?? p.key))
  }

  // Known-abilities filter: keep mods whose skill is one of the snapshot's
  // known skills. Scoped to the build's skills already applied above — never
  // filters to zero on missing data (toggle disabled when no rows).
  if (knownAbilitiesFilter.value && snapshotHasAbilityRows.value) {
    const knownSkills = new Set(characterStore.snapshotAbilities.map(a => a.skill_name))
    powers = powers.filter(p => p.skill == null || knownSkills.has(p.skill) || isGenericPower(p))
  }

  return powers
})

function addMod(power: SlotTsysPower, tierId?: string) {
  if (!store.selectedSlot) return

  const regularMods = store.selectedSlotMods.filter(m => !m.is_augment)
  if (regularMods.length >= store.maxModsPerSlot) return

  const powerName = power.internal_name ?? power.key
  const alreadyAssigned = store.selectedSlotMods.some(m => m.power_name === powerName)
  if (alreadyAssigned) return

  // Check skill constraints using the constraint solver
  const isGeneric = !power.skill || power.skill === 'AnySkill' || power.skill === 'Endurance'
  const rarity = store.getSlotRarity(store.selectedSlot)

  const skillCounts = new Map<string, number>()
  let genCount = 0
  for (const m of regularMods) {
    const p = store.slotPowers.find(sp => (sp.internal_name ?? sp.key) === m.power_name)
    const s = p?.skill
    if (!s || s === 'AnySkill' || s === 'Endurance') {
      genCount++
    } else {
      skillCounts.set(s, (skillCounts.get(s) ?? 0) + 1)
    }
  }

  const constraints = computeSlotConstraints(rarity, skillCounts, genCount)
  if (isGeneric) {
    if (!constraints.canAddGeneric) return
  } else if (skillCounts.has(power.skill!)) {
    if (!constraints.canAddSkillMod) return
  } else {
    if (!constraints.canAddNewSkill) return
  }

  store.addMod(power, false, tierId)
}

function addAugment(power: SlotTsysPower, tierId?: string) {
  if (!store.selectedSlot) return

  const powerName = power.internal_name ?? power.key
  const alreadyAssigned = store.selectedSlotMods.some(m => m.power_name === powerName)
  if (alreadyAssigned) return

  // Replace existing augment if present
  const existingAug = store.selectedSlotMods.find(m => m.is_augment)
  if (existingAug) store.removeMod(existingAug)

  store.addMod(power, true, tierId)
}
</script>
