<template>
  <section class="bg-surface-card border border-border-default rounded p-3 flex flex-col gap-2">
    <header class="flex items-baseline gap-2">
      <h4 class="text-xs font-semibold" :class="categoryTextClass">
        {{ mapDisplayName }}
      </h4>
      <span class="text-[10px] text-text-dim">
        {{ totalCompletions }} completed
        <template v-if="(craftingCost ?? 0) > 0">
          · cost {{ formatGold(craftingCost ?? 0) }}
        </template>
      </span>
    </header>

    <DataTable
      :columns="columns"
      :rows="(items as unknown as Record<string, unknown>[])"
      compact
      empty-text="No loot data recorded.">
      <template #cell-item_name="{ row }">
        <ItemInline :reference="row.item_name as string" />
      </template>
      <template #cell-total_qty="{ row }">
        <span class="text-accent-gold font-semibold">{{ (row.total_qty as number).toLocaleString() }}</span>
      </template>
      <template #cell-primary_qty="{ row }">
        <span class="text-text-primary">{{ (row.primary_qty as number).toLocaleString() }}</span>
      </template>
      <template #cell-bonus_qty="{ row }">
        <span :class="(row.bonus_qty as number) > 0 ? 'text-accent-gold' : 'text-text-dim'">
          {{ (row.bonus_qty as number).toLocaleString() }}
        </span>
      </template>
      <template #cell-times_received="{ row }">
        <span class="text-text-secondary">{{ (row.times_received as number).toLocaleString() }}</span>
      </template>
      <template #cell-avg_per_drop="{ row }">
        <span class="text-text-muted">
          {{ (row.times_received as number) > 0
            ? ((row.total_qty as number) / (row.times_received as number)).toFixed(1)
            : '—' }}
        </span>
      </template>
    </DataTable>
  </section>
</template>

<script setup lang="ts">
// Per-survey-type or per-zone card listing item drops attributed to this
// scope. Used as the building block for both ZoneView (one card per
// survey type the zone contains) and TypeView (one card for the chosen
// type). Takes a pre-resolved items list so the caller owns the group-by
// logic — this component just renders.
import { computed } from 'vue'
import type { ItemSummary } from '../../stores/surveyTrackerStore'
import DataTable, { type ColumnDef } from '../Shared/DataTable.vue'
import ItemInline from '../Shared/Item/ItemInline.vue'
import { formatGold } from '../../composables/useRecipeCost'

const props = defineProps<{
  mapDisplayName: string
  /** `'mineral'` / `'mining'` — tints the header. */
  category?: string
  totalCompletions: number
  craftingCost?: number
  items: ItemSummary[]
}>()

const categoryTextClass = computed(() => {
  if (props.category === 'mineral') return 'text-accent-blue'
  if (props.category === 'mining') return 'text-accent-red'
  return 'text-text-primary'
})

const columns: ColumnDef[] = [
  { key: 'item_name', label: 'Item' },
  { key: 'total_qty', label: 'Total', align: 'right', numeric: true },
  { key: 'primary_qty', label: 'Primary', align: 'right', numeric: true },
  { key: 'bonus_qty', label: 'Bonus', align: 'right', numeric: true },
  { key: 'times_received', label: 'Drops', align: 'right', numeric: true },
  { key: 'avg_per_drop', label: 'Avg/Drop', align: 'right', numeric: true },
]
</script>
