<template>
  <div
    class="flex items-start gap-2 px-2 py-1.5 rounded text-sm transition-all border"
    :class="rowClass"
    :title="disabledReason"
    @click="handleClick">
    <GameIcon v-if="power.icon_id" :icon-id="power.icon_id" size="xs" class="shrink-0 mt-0.5" />
    <div class="flex-1 min-w-0">
      <div class="flex items-center gap-1.5">
        <span
          v-if="power.skill && power.skill !== 'AnySkill'"
          class="text-[10px] font-semibold px-1 py-0.5 rounded"
          :class="skillBadgeClass">
          {{ power.skill }}
        </span>
        <span v-else class="text-[10px] font-semibold text-text-dim px-1 py-0.5 rounded bg-surface-hover">
          Generic
        </span>
        <span class="font-medium text-text-primary truncate">{{ displayName }}</span>
        <TierSelector
          v-if="hasTiers && !isAssigned && !isAssignedAsAugment"
          :tiers="power.available_tiers"
          :model-value="selectedTierId"
          class="ml-auto"
          @update:model-value="selectedTierId = $event" />
      </div>
      <template v-if="!compact">
        <div v-if="activeEffects.length > 0" class="mt-0.5">
          <EffectLine v-for="(effect, i) in activeEffects" :key="i" :text="effect" />
        </div>
        <div v-if="activePrereq" class="text-[10px] text-text-dim mt-0.5">
          Requires {{ power.skill ?? 'skill' }} level {{ activePrereq }}
        </div>
        <div v-if="boostedAbilities.length > 0" class="text-[10px] text-accent-gold/70 mt-0.5">
          boosts: {{ boostedAbilities.join(', ') }}
        </div>
        <!-- Equip slot chips -->
        <div v-if="power.slots.length > 0" class="flex flex-wrap gap-0.5 mt-0.5">
          <span
            v-for="slot in power.slots"
            :key="slot"
            class="text-[10px] leading-tight px-1.5 py-0.5 rounded border border-border-default bg-surface-dark text-text-secondary">
            {{ slot }}
          </span>
        </div>
        <div v-if="disabledReason" class="text-[10px] text-amber-400/80 mt-0.5">
          {{ disabledReason }}
        </div>
      </template>
    </div>

    <!-- Action buttons -->
    <div v-if="!isAssigned && !isAssignedAsAugment && !disabled" class="flex flex-col gap-0.5 shrink-0 mt-0.5">
      <button
        class="text-accent-gold/70 hover:text-accent-gold text-xs cursor-pointer px-1 py-0.5 rounded hover:bg-accent-gold/10 transition-colors"
        title="Add as mod"
        @click.stop="emitAdd">
        + Mod
      </button>
      <button
        v-if="!hasAugment && !isBeltSlot"
        class="text-mod-augment/70 hover:text-mod-augment text-[10px] cursor-pointer px-1 py-0.5 rounded hover:bg-purple-900/20 transition-colors"
        title="Add as augment (100 CP)"
        @click.stop="emitAddAugment">
        + Aug
      </button>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, watch } from 'vue'
import { useBuildPlannerStore } from '../../../stores/buildPlannerStore'
import type { SlotTsysPower } from '../../../types/buildPlanner'
import { getPowerDisplayName } from '../../../types/buildPlanner'
import GameIcon from '../../Shared/GameIcon.vue'
import TierSelector from './TierSelector.vue'
import EffectLine from './EffectLine.vue'
import { useBuildCrossRef } from '../../../composables/useBuildCrossRef'

const props = defineProps<{
  power: SlotTsysPower
  disabled?: boolean
  disabledReason?: string
  compact?: boolean
}>()

const emit = defineEmits<{
  add: [tierId?: string]
  addAugment: [tierId?: string]
}>()

const store = useBuildPlannerStore()
const { getAbilitiesForPower } = useBuildCrossRef()

const boostedAbilities = computed(() => {
  const powerKey = props.power.internal_name ?? props.power.key
  return getAbilitiesForPower(powerKey)
})

const displayName = computed(() => getPowerDisplayName(props.power))
const hasTiers = computed(() => props.power.available_tiers.length > 1)
const selectedTierId = ref(props.power.tier_id ?? '')

watch(() => props.power.tier_id, (newTier) => {
  selectedTierId.value = newTier ?? ''
})

const activeEffects = computed(() => {
  if (!hasTiers.value) return props.power.effects
  const tier = props.power.available_tiers.find(t => t.tier_id === selectedTierId.value)
  return tier?.effects ?? props.power.effects
})

const activePrereq = computed(() => {
  if (!hasTiers.value) return props.power.skill_level_prereq
  const tier = props.power.available_tiers.find(t => t.tier_id === selectedTierId.value)
  return tier?.skill_level_prereq ?? props.power.skill_level_prereq
})

function emitAdd() {
  const tierId = hasTiers.value ? selectedTierId.value : undefined
  emit('add', tierId)
}

function emitAddAugment() {
  const tierId = hasTiers.value ? selectedTierId.value : undefined
  emit('addAugment', tierId)
}

function handleClick() {
  if (props.disabled || isAssigned.value || isAssignedAsAugment.value) return
  emitAdd()
}

const isAssigned = computed(() => {
  const powerName = props.power.internal_name ?? props.power.key
  return store.selectedSlotMods.some(m => m.power_name === powerName && !m.is_augment)
})

const isAssignedAsAugment = computed(() => {
  const powerName = props.power.internal_name ?? props.power.key
  return store.selectedSlotMods.some(m => m.power_name === powerName && m.is_augment)
})

const hasAugment = computed(() =>
  store.selectedSlotMods.some(m => m.is_augment)
)

const isBeltSlot = computed(() => store.selectedSlot === 'Belt')

const rowClass = computed(() => {
  if (isAssigned.value || isAssignedAsAugment.value) {
    return 'bg-surface-elevated border-border-default opacity-40 cursor-default'
  }
  if (props.disabled) {
    return 'bg-surface-elevated border-border-default opacity-50 cursor-not-allowed'
  }
  return 'bg-surface-elevated border-border-default hover:bg-surface-hover hover:border-accent-gold/30 cursor-pointer'
})

const skillBadgeClass = computed(() => {
  if (!props.power.skill || !store.selectedSlot) return 'text-text-dim bg-surface-hover'
  const primary = store.getSlotSkillPrimary(store.selectedSlot)
  const secondary = store.getSlotSkillSecondary(store.selectedSlot)
  if (props.power.skill === primary) {
    return 'text-blue-300 bg-blue-900/30'
  }
  if (props.power.skill === secondary) {
    return 'text-emerald-300 bg-emerald-900/30'
  }
  return 'text-text-dim bg-surface-hover'
})
</script>
