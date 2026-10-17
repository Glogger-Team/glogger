<template>
  <section class="bg-surface-card border border-border-default rounded p-3 flex flex-col gap-2 h-full overflow-hidden">
    <header class="flex items-baseline justify-between shrink-0">
      <h3 class="text-[10px] uppercase tracking-widest text-text-secondary font-semibold">
        Loot Found
      </h3>
      <span class="text-[10px] text-text-dim">{{ rows.length }} unique</span>
    </header>

    <div class="flex-1 min-h-0 overflow-y-auto -mr-1 pr-1">
      <DataTable
        :columns="columns"
        :rows="(rows as unknown as Record<string, unknown>[])"
        :row-key="(row) => String(row.item_name)"
        compact
        empty-text="No loot attributed yet.">
        <template #cell-item_name="{ row }">
          <ItemMinicard
            :reference="row.item_name as string"
            width="min"
            :bordered="false" />
        </template>
        <template #cell-total_qty="{ row }">
          <span class="text-accent-gold font-semibold">{{ (row.total_qty as number).toLocaleString() }}</span>
        </template>
        <template #cell-bonus_qty="{ row }">
          <span v-if="(row.bonus_qty as number) > 0" class="text-text-dim">
            {{ row.primary_qty }}<span class="text-text-muted"> + </span><span class="text-accent-gold">{{ row.bonus_qty }}</span>
          </span>
          <span v-else class="text-text-dim">—</span>
        </template>
        <template #cell-total_value="{ row }">
          <span :class="row.unit_value !== null ? 'text-text-secondary' : 'text-text-dim italic'">
            {{ row.total_value !== null ? formatGold(row.total_value as number) : '—' }}
          </span>
        </template>
      </DataTable>
    </div>
  </section>
</template>

<script setup lang="ts">
// Loot overview table for the center panel. Shows each item as an
// ItemMinicard row (with inline market-value editing via the ??? button)
// plus quantity, primary/bonus split, and total value.
import { computed } from 'vue'
import type { LootSummaryRow } from '../../stores/surveyTrackerStore'
import DataTable, { type ColumnDef } from '../Shared/DataTable.vue'
import ItemMinicard from '../Shared/Item/ItemMinicard.vue'
import { formatGold } from '../../composables/useRecipeCost'

const props = defineProps<{
  rows: LootSummaryRow[]
}>()

const hasBonus = computed(() => props.rows.some(r => r.bonus_qty > 0))

// `Split` only appears when at least one row has bonus loot, mirroring the
// previous per-`<th>` `v-if`.
const columns = computed<ColumnDef[]>(() => {
  const cols: ColumnDef[] = [
    { key: 'item_name', label: 'Item' },
    // The minicard makes these rows tall; the numbers align with its first line.
    { key: 'total_qty', label: 'Qty', align: 'right', numeric: true, cellClass: 'align-top pt-2' },
  ]
  if (hasBonus.value) cols.push({ key: 'bonus_qty', label: 'Split', align: 'right', numeric: true, cellClass: 'align-top pt-2' })
  cols.push({ key: 'total_value', label: 'Value', align: 'right', numeric: true, cellClass: 'align-top pt-2' })
  return cols
})
</script>
