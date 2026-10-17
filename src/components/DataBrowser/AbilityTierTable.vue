<template>
  <div class="flex flex-col gap-1.5">
    <div class="text-[0.65rem] uppercase tracking-widest text-text-dim border-b border-surface-card pb-0.5">
      Tier Progression
      <span class="text-text-muted">({{ tiers.length }} tiers)</span>
    </div>

    <!-- Compact table -->
    <div class="overflow-x-auto">
      <DataTable
        :columns="columns"
        :rows="(tiers as unknown as Record<string, unknown>[])"
        :row-class="tierRowClass"
        compact
        row-clickable
        @row-click="onTierRowClick">
        <template #cell-tier_index="{ row }">
          <span class="text-text-primary">{{ tierIndexById.get(asTier(row).id) }}</span>
        </template>
        <template #cell-level="{ row }">
          <span class="text-text-secondary font-mono">{{ asTier(row).level ?? 0 }}</span>
        </template>
        <template #cell-damage="{ row }">
          <span
            v-if="asTier(row).pve?.damage != null"
            class="text-text-secondary font-mono">{{ asTier(row).pve?.damage }}</span>
        </template>
        <template #cell-power_cost="{ row }">
          <span
            v-if="asTier(row).power_cost != null"
            class="font-mono text-[#c0a040]">{{ asTier(row).power_cost }}</span>
          <span
            v-else-if="asTier(row).pve?.power_cost != null"
            class="font-mono text-[#c0a040]">{{ asTier(row).pve?.power_cost }}</span>
        </template>
        <template #cell-mana_cost="{ row }">
          <span
            v-if="asTier(row).mana_cost != null"
            class="font-mono text-[#6090c0]">{{ asTier(row).mana_cost }}</span>
        </template>
        <template #cell-armor_cost="{ row }">
          <span
            v-if="asTier(row).armor_cost != null"
            class="font-mono text-text-muted">{{ asTier(row).armor_cost }}</span>
        </template>
        <template #cell-health_cost="{ row }">
          <span
            v-if="asTier(row).health_cost != null"
            class="font-mono text-accent-red">{{ asTier(row).health_cost }}</span>
        </template>
        <template #cell-range="{ row }">
          <span
            v-if="asTier(row).range != null"
            class="text-text-secondary font-mono">{{ asTier(row).range }}m</span>
        </template>
        <template #cell-reset_time="{ row }">
          <span
            v-if="asTier(row).reset_time != null"
            class="text-text-secondary font-mono">{{ asTier(row).reset_time }}s</span>
        </template>
        <template #cell-rage_cost="{ row }">
          <span
            v-if="asTier(row).pve?.rage_cost != null"
            class="font-mono text-[#c06060]">{{ asTier(row).pve?.rage_cost }}</span>
        </template>
      </DataTable>
    </div>

    <!-- Expanded tier detail -->
    <div v-if="expandedTier != null && expandedAbility" class="bg-surface-dark border border-surface-card p-3 flex flex-col gap-2">
      <div class="flex items-center justify-between">
        <span class="text-text-primary text-xs font-medium">{{ expandedAbility.name }}</span>
        <span class="text-text-dim text-[0.65rem]">ID: {{ expandedAbility.id }}</span>
      </div>

      <div v-if="expandedAbility.description" class="text-xs text-text-secondary italic">
        {{ expandedAbility.description }}
      </div>

      <div v-if="expandedAbility.prerequisite" class="text-xs text-[#e08060] px-2 py-1 bg-surface-inset border-l-2 border-l-[#4a2a2a]">
        Requires: {{ expandedAbility.prerequisite }}
      </div>

      <div v-if="expandedAbility.special_info" class="text-xs text-text-secondary italic">
        {{ expandedAbility.special_info }}
      </div>

      <!-- Per-tier sources -->
      <SourcesPanel :sources="tierSources" :loading="tierSourcesLoading" />
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, watch } from 'vue'
import { useGameDataStore } from '../../stores/gameDataStore'
import type { AbilityInfo, EntitySources } from '../../types/gameData'
import SourcesPanel from '../Shared/SourcesPanel.vue'
import DataTable, { type ColumnDef } from '../Shared/DataTable.vue'

const props = defineProps<{
  tiers: AbilityInfo[]
}>()

const store = useGameDataStore()
const expandedTier = defineModel<number | null>('expandedTierId', { default: null })
const tierSources = ref<EntitySources | null>(null)
const tierSourcesLoading = ref(false)

const expandedAbility = computed(() =>
  props.tiers.find(t => t.id === expandedTier.value) ?? null
)

// Column visibility: only show columns that have data in at least one tier
const showDamage = computed(() => props.tiers.some(t => t.pve?.damage != null))
const showPowerCost = computed(() => props.tiers.some(t => t.power_cost != null || t.pve?.power_cost != null))
const showManaCost = computed(() => props.tiers.some(t => t.mana_cost != null))
const showArmorCost = computed(() => props.tiers.some(t => t.armor_cost != null))
const showHealthCost = computed(() => props.tiers.some(t => t.health_cost != null))
const showRange = computed(() => props.tiers.some(t => t.range != null))
const showCooldown = computed(() => props.tiers.some(t => t.reset_time != null))
const showRageCost = computed(() => props.tiers.some(t => t.pve?.rage_cost != null))

// Same visibility rule as the previous per-`<th>`/`<td>` `v-if`s: a column only
// appears when at least one tier has data for it.
const columns = computed<ColumnDef[]>(() => {
  const cols: ColumnDef[] = [
    { key: 'tier_index', label: 'Tier' },
    { key: 'level', label: 'Level' },
  ]
  if (showDamage.value) cols.push({ key: 'damage', label: 'Damage', align: 'right', numeric: true })
  if (showPowerCost.value) cols.push({ key: 'power_cost', label: 'Power', align: 'right', numeric: true })
  if (showManaCost.value) cols.push({ key: 'mana_cost', label: 'Mana', align: 'right', numeric: true })
  if (showArmorCost.value) cols.push({ key: 'armor_cost', label: 'Armor', align: 'right', numeric: true })
  if (showHealthCost.value) cols.push({ key: 'health_cost', label: 'Health', align: 'right', numeric: true })
  if (showRange.value) cols.push({ key: 'range', label: 'Range', align: 'right', numeric: true })
  if (showCooldown.value) cols.push({ key: 'reset_time', label: 'CD', align: 'right', numeric: true })
  if (showRageCost.value) cols.push({ key: 'rage_cost', label: 'Rage', align: 'right', numeric: true })
  return cols
})

// 1-based tier position, keyed by ability id: the row slot only carries
// `row`/`value`, so the index has to come from the tiers array itself.
const tierIndexById = computed(() => new Map(props.tiers.map((t, i) => [t.id, i + 1] as const)))

function asTier(row: Record<string, unknown>): AbilityInfo {
  return row as unknown as AbilityInfo
}

function tierRowClass(row: Record<string, unknown>): string {
  return expandedTier.value === asTier(row).id ? 'bg-surface-card' : ''
}

function onTierRowClick(payload: { row: Record<string, unknown>; index: number }) {
  toggleExpand(asTier(payload.row).id)
}

function toggleExpand(id: number) {
  expandedTier.value = expandedTier.value === id ? null : id
}

// Load sources when a tier is expanded
watch(expandedTier, async (id) => {
  if (id == null) {
    tierSources.value = null
    return
  }
  tierSourcesLoading.value = true
  try {
    tierSources.value = await store.getAbilitySources(id)
  } finally {
    tierSourcesLoading.value = false
  }
})
</script>
