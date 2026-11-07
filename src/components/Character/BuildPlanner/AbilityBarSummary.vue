<template>
  <div class="flex flex-col gap-2">
    <div v-for="bar in bars" :key="bar.id">
      <!-- Bar card -->
      <div
        class="w-full flex flex-col rounded border transition-all"
        :class="barClasses(bar.id)">

        <!-- Header row: skill selector (or label) + fill count -->
        <div class="flex items-center justify-between px-2.5 py-1.5">
          <StyledSelect
            v-if="bar.id !== 'sidebar'"
            :model-value="getBarSkill(bar.id) ?? ''"
            :options="skillOptions(bar.id)"
            :placeholder="bar.id === 'primary' ? 'Primary...' : 'Secondary...'"
            size="xs"
            full-width
            :color-class="bar.id === 'primary' ? 'text-blue-400' : 'text-emerald-400'"
            @update:model-value="(val: string) => onSkillChange(bar.id as BarId, val)" />
          <span v-else class="text-xs font-semibold">Sidebar</span>
          <span class="text-[10px]" :class="fillColor(bar.id)">
            {{ store.barAbilityCounts[bar.id] }}/{{ getBarMaxSlots(bar.id) }}
          </span>
        </div>

        <!-- Ability grid: 3 per row, each cell is a fixed slot position -->
        <div class="grid grid-cols-3 gap-1.5 px-2.5 pb-2 -mt-0.5 justify-items-center">
          <template v-for="slotIdx in getBarMaxSlots(bar.id)" :key="slotIdx">
            <!-- Filled slot -->
            <EntityTooltipWrapper
              v-if="getAbilityAtSlot(bar.id, slotIdx - 1)"
              border-class="border-entity-ability/50"
              entity-type="ability"
              :entity-reference="getAbilityAtSlot(bar.id, slotIdx - 1)!.ability_name ?? ''"
              :entity-label="getAbilityAtSlot(bar.id, slotIdx - 1)!.ability_name ?? ''">
              <button
                class="flex flex-col items-center cursor-pointer"
                @click="openPicker(bar.id as BarId, slotIdx - 1)">
                <GameIcon
                  v-if="resolvedAbilities[getAbilityAtSlot(bar.id, slotIdx - 1)!.ability_id]"
                  :icon-id="resolvedAbilities[getAbilityAtSlot(bar.id, slotIdx - 1)!.ability_id]!.icon_id"
                  :alt="getAbilityAtSlot(bar.id, slotIdx - 1)!.ability_name ?? ''"
                  size="lg" />
                <div
                  v-else
                  class="w-12 h-12 rounded-sm bg-surface-hover border border-border-default/50 flex items-center justify-center text-text-dim text-xs">
                  ?
                </div>
              </button>
              <template #tooltip>
                <AbilityTooltip
                  :ability="resolvedAbilities[getAbilityAtSlot(bar.id, slotIdx - 1)!.ability_id]!"
                  :icon-src="null"
                  :damage-type-override="statsForAbility(getAbilityAtSlot(bar.id, slotIdx - 1)!.ability_id)?.damage_type"
                  v-if="resolvedAbilities[getAbilityAtSlot(bar.id, slotIdx - 1)!.ability_id]" />
                <AbilityEffectiveStats
                  :stats="statsForAbility(getAbilityAtSlot(bar.id, slotIdx - 1)!.ability_id)" />
              </template>
            </EntityTooltipWrapper>

            <!-- Empty slot -->
            <button
              v-else
              class="flex flex-col items-center cursor-pointer group"
              :title="`Set ability for slot ${slotIdx}`"
              @click="openPicker(bar.id as BarId, slotIdx - 1)">
              <div class="w-12 h-12 shrink-0 rounded-sm border border-dashed border-border-default/40 group-hover:border-accent-gold/50 transition-colors flex items-center justify-center">
                <span class="text-text-dim/40 group-hover:text-accent-gold/60 text-lg transition-colors">+</span>
              </div>
            </button>
          </template>
        </div>

        <!-- Sidebar slot config -->
        <div v-if="bar.id === 'sidebar'" class="flex items-center gap-2 px-2.5 pb-2 border-t border-border-default/30 pt-1.5">
          <label class="text-[10px] leading-none text-text-dim">Slots:</label>
          <input
            type="number"
            :value="store.sidebarSlotCount"
            :min="Math.max(6, store.barAbilityCounts.sidebar)"
            :max="MAX_SIDEBAR_SLOTS"
            class="bg-surface-elevated border border-border-default rounded px-1 py-0.5 text-[10px] text-text-primary w-10 text-center"
            @change="onSidebarSlotsChange" />
          <span class="text-[10px] leading-none text-text-dim">(max {{ MAX_SIDEBAR_SLOTS }})</span>
        </div>
      </div>
    </div>

    <!-- Ability Picker Dialog -->
    <AbilityPickerDialog
      v-if="pickerBar != null"
      :show="showPicker"
      :bar="pickerBar"
      :target-slot="pickerTargetSlot"
      @update:show="showPicker = $event"
      @added="onAbilityAdded" />
  </div>
</template>

<script setup lang="ts">
import { ref, watch, onMounted } from 'vue'
import { useBuildPlannerStore } from '../../../stores/buildPlannerStore'
import { useGameDataStore } from '../../../stores/gameDataStore'
import { ABILITY_BARS, MAX_SIDEBAR_SLOTS, expandBuildSkill } from '../../../types/buildPlanner'
import type { AbilityInfo } from '../../../types/gameData'
import type { BuildPresetAbility } from '../../../types/buildPlanner'
import GameIcon from '../../Shared/GameIcon.vue'
import StyledSelect from '../../Shared/StyledSelect.vue'
import EntityTooltipWrapper from '../../Shared/EntityTooltipWrapper.vue'
import AbilityTooltip from '../../Shared/Ability/AbilityTooltip.vue'
import AbilityEffectiveStats from './AbilityEffectiveStats.vue'
import AbilityPickerDialog from './AbilityPickerDialog.vue'
import { useAbilityBuildStats } from '../../../composables/useAbilityBuildStats'

type BarId = 'primary' | 'secondary' | 'sidebar'

const store = useBuildPlannerStore()
const gameData = useGameDataStore()
const { statsForAbility, prefetch } = useAbilityBuildStats()
const bars = ABILITY_BARS

const resolvedAbilities = ref<Record<number, AbilityInfo>>({})

// Picker state
const showPicker = ref(false)
const pickerBar = ref<BarId | null>(null)
const pickerTargetSlot = ref(0)

function getBarMaxSlots(barId: string): number {
  if (barId === 'sidebar') return store.sidebarSlotCount
  return 6
}

function getBarSkill(barId: string): string | null {
  if (barId === 'primary') return store.activePreset?.skill_primary ?? null
  if (barId === 'secondary') return store.activePreset?.skill_secondary ?? null
  return null
}

function skillOptions(barId: string) {
  const placeholder = barId === 'primary' ? 'Primary...' : 'Secondary...'
  const realSkills = store.combatSkills.filter(s =>
    !(s.raw_json as Record<string, unknown>)?.IsFakeCombatSkill
  )
  return [
    { value: '', label: placeholder },
    ...realSkills.map(s => ({ value: s.name, label: s.name })),
  ]
}

async function onSkillChange(barId: BarId, val: string) {
  const key = barId === 'primary' ? 'skill_primary' : 'skill_secondary'
  await store.updatePreset({ [key]: val || null })
  await store.clearBar(barId)
  await store.onBuildParamsChanged()
  resolveAbilityIcons()
}

function getAbilityAtSlot(barId: string, slotIdx: number): BuildPresetAbility | null {
  return store.getBarAbilities(barId).find(a => a.slot_position === slotIdx) ?? null
}

function barClasses(barId: string): string {
  const count = store.barAbilityCounts[barId as keyof typeof store.barAbilityCounts] ?? 0
  const max = getBarMaxSlots(barId)
  if (count >= max) {
    return 'bg-surface-elevated border-border-default text-text-primary border-l-2 border-l-green-500/50'
  }
  if (count > 0) {
    return 'bg-surface-elevated border-border-default text-text-primary border-l-2 border-l-yellow-500/40'
  }
  return 'bg-surface-elevated border-border-default text-text-secondary'
}

function fillColor(barId: string): string {
  const count = store.barAbilityCounts[barId as keyof typeof store.barAbilityCounts] ?? 0
  const max = getBarMaxSlots(barId)
  if (count >= max) return 'text-value-positive'
  if (count > 0) return 'text-yellow-400'
  return 'text-text-dim'
}

function openPicker(barId: BarId, slotIndex: number) {
  pickerBar.value = barId
  pickerTargetSlot.value = slotIndex
  store.selectBar(barId)
  showPicker.value = true
}

function onAbilityAdded(_ability: AbilityInfo) {
  resolveAbilityIcons()
}

function onSidebarSlotsChange(e: Event) {
  const val = Number((e.target as HTMLInputElement).value)
  const min = Math.max(6, store.barAbilityCounts.sidebar)
  if (val >= min && val <= MAX_SIDEBAR_SLOTS) {
    store.sidebarSlotCount = val
  }
}

async function resolveAbilityIcons() {
  const abilityIds = store.presetAbilities.map(a => a.ability_id)
  if (abilityIds.length === 0) {
    resolvedAbilities.value = {}
    return
  }

  const skills = new Set<string>()
  // Include folded child skills (e.g. Fairy Magic under Mentalism) so their
  // abilities resolve to a name/icon/tooltip instead of a bare "?" placeholder.
  for (const s of expandBuildSkill(store.activePreset?.skill_primary)) skills.add(s)
  for (const s of expandBuildSkill(store.activePreset?.skill_secondary)) skills.add(s)
  for (const s of ['FirstAid', 'ArmorPatching', 'SurvivalInstincts']) {
    skills.add(s)
  }

  const map: Record<number, AbilityInfo> = {}
  for (const skill of skills) {
    try {
      const abilities = await gameData.getAbilitiesForSkill(skill)
      for (const a of abilities) {
        if (abilityIds.includes(a.id)) {
          map[a.id] = a
        }
      }
    } catch {
      // Skill might not exist
    }
  }
  resolvedAbilities.value = map

  // Warm the effective-stats cache so hovers render with build data immediately.
  prefetch(abilityIds)
}

// Re-resolve when abilities change or build switches
// Watch the abilities array itself (reference changes on preset load)
watch(() => store.presetAbilities, () => {
  resolveAbilityIcons()
})

watch(() => [store.activePreset?.skill_primary, store.activePreset?.skill_secondary], () => {
  resolveAbilityIcons()
})

onMounted(() => {
  if (store.presetAbilities.length > 0) {
    resolveAbilityIcons()
  }
})
</script>
