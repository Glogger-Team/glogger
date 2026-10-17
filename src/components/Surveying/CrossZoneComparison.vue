<template>
  <div class="flex flex-col gap-3">
    <div v-if="zones.length > 1" class="flex items-center gap-2 text-xs">
      <span class="text-[10px] uppercase tracking-wide text-text-secondary font-semibold">Sort by</span>
      <div class="flex gap-0.5 bg-surface-elevated border border-border-default rounded p-0.5">
        <button
          v-for="col in sortColumns"
          :key="col.key"
          class="px-2 py-0.5 text-xs rounded transition-colors"
          :class="
            sortBy === col.key
              ? 'bg-accent-gold/20 text-accent-gold font-semibold'
              : 'text-text-secondary hover:text-text-primary'
          "
          @click="toggleSort(col.key)"
        >
          {{ col.label }}
          <span v-if="sortBy === col.key">{{ sortAsc ? '↑' : '↓' }}</span>
        </button>
      </div>
    </div>

    <DataTable
      :columns="columns"
      :rows="(sortedZones as unknown as Record<string, unknown>[])"
      compact
      row-clickable
      @row-click="selectZoneRow">
      <template #cell-area="{ row }">
        <span class="text-text-primary font-semibold">
          <AreaInline :reference="row.area as string" />
        </span>
      </template>
      <template #cell-total_uses="{ row }">
        <span class="text-text-primary">{{ row.total_uses }}</span>
      </template>
      <template #cell-basic_uses="{ row }">
        <span class="text-text-secondary">{{ row.basic_uses }}</span>
      </template>
      <template #cell-motherlode_uses="{ row }">
        <span class="text-text-secondary">{{ row.motherlode_uses }}</span>
      </template>
      <template #cell-multihit_uses="{ row }">
        <span class="text-text-secondary">{{ row.multihit_uses }}</span>
      </template>
      <template #cell-total_loot_qty="{ row }">
        <span class="text-accent-gold font-semibold">{{ (row.total_loot_qty as number).toLocaleString() }}</span>
      </template>
      <template #cell-bonus_rate="{ row }">
        <span :class="bonusRateClass(row as unknown as ZoneSummary)">
          {{ bonusRate(row as unknown as ZoneSummary) }}
        </span>
      </template>
      <template #cell-bonus_items_total="{ row }">
        <span class="text-text-secondary">{{ (row.bonus_items_total as number).toLocaleString() }}</span>
      </template>
    </DataTable>
  </div>
</template>

<script setup lang="ts">
// Cross-zone comparison table used on the Analytics Overview view.
// Each row is clickable to drill into that zone's detail page.
import { computed, ref } from 'vue'
import type { ZoneSummary } from '../../stores/surveyTrackerStore'
import AreaInline from '../Shared/Area/AreaInline.vue'
import DataTable, { type ColumnDef } from '../Shared/DataTable.vue'

const props = defineProps<{
  zones: ZoneSummary[]
}>()

const emit = defineEmits<{
  'select-zone': [area: string]
}>()

const columns: ColumnDef[] = [
  { key: 'area', label: 'Zone' },
  { key: 'total_uses', label: 'Surveys', align: 'right', numeric: true },
  { key: 'basic_uses', label: 'Basic', align: 'right', numeric: true },
  { key: 'motherlode_uses', label: 'Motherlode', align: 'right', numeric: true },
  { key: 'multihit_uses', label: 'Multihit', align: 'right', numeric: true },
  { key: 'total_loot_qty', label: 'Loot', align: 'right', numeric: true },
  { key: 'bonus_rate', label: 'Bonus Rate', align: 'right', numeric: true },
  { key: 'bonus_items_total', label: 'Bonus Items', align: 'right', numeric: true },
]

type SortKey = 'total_uses' | 'basic_uses' | 'total_loot_qty' | 'bonus_rate' | 'bonus_items_total'

const sortColumns: { key: SortKey; label: string }[] = [
  { key: 'total_uses', label: 'Surveys' },
  { key: 'total_loot_qty', label: 'Loot' },
  { key: 'bonus_rate', label: 'Bonus Rate' },
  { key: 'bonus_items_total', label: 'Bonus Items' },
]

const sortBy = ref<SortKey>('total_uses')
const sortAsc = ref(false)

function toggleSort(key: SortKey) {
  if (sortBy.value === key) {
    sortAsc.value = !sortAsc.value
  } else {
    sortBy.value = key
    sortAsc.value = false
  }
}

function zoneValue(z: ZoneSummary, key: SortKey): number {
  if (key === 'bonus_rate') {
    return z.basic_uses > 0 ? z.basic_uses_with_bonus / z.basic_uses : 0
  }
  return z[key]
}

const sortedZones = computed(() => {
  const list = [...props.zones]
  list.sort((a, b) => {
    const av = zoneValue(a, sortBy.value)
    const bv = zoneValue(b, sortBy.value)
    return sortAsc.value ? av - bv : bv - av
  })
  return list
})

// Row-click drill-in from the comparison table. Sorting stays with the
// toolbar above (headers were never click-sortable here).
function selectZoneRow(payload: { row: Record<string, unknown>; index: number }) {
  emit('select-zone', (payload.row as unknown as ZoneSummary).area)
}

function bonusRate(z: ZoneSummary): string {
  if (z.basic_uses === 0) return '—'
  const pct = (z.basic_uses_with_bonus / z.basic_uses) * 100
  return `${pct.toFixed(1)}%`
}

function bonusRateClass(z: ZoneSummary): string {
  if (z.basic_uses === 0) return 'text-text-dim'
  return 'text-accent-gold'
}
</script>
