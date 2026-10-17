<template>
  <div class="flex flex-col gap-2 min-h-0 h-full">
    <div class="flex items-center justify-between shrink-0">
      <h3 class="section-heading">Combat Stats</h3>
      <div class="flex items-center gap-3">
        <input
          v-model="filter"
          type="text"
          placeholder="Filter..."
          class="px-2 py-1 bg-surface-base border border-border-default rounded text-xs text-text-primary placeholder-text-muted focus:outline-none focus:border-accent-gold/50 w-32" />
        <span class="text-xs text-text-muted">{{ filtered.length }}</span>
      </div>
    </div>

    <div class="overflow-auto flex-1 min-h-0">
      <DataTable
        :columns="columns"
        :rows="(filtered as unknown as Record<string, unknown>[])"
        :row-key="(row) => String(row.stat_key)"
        compact
        size="sm"
        empty-text="No stats match the current filter.">
        <template #cell-stat_key="{ row }">
          <span class="text-text-primary">{{ formatStatKey(row.stat_key as string) }}</span>
        </template>
        <template #cell-value="{ row }">
          <span class="text-accent-gold">{{ formatValue(row.value as number) }}</span>
        </template>
      </DataTable>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed } from 'vue'
import type { SnapshotStat } from '../../types/database'
import DataTable, { type ColumnDef } from '../Shared/DataTable.vue'

const props = defineProps<{
  stats: SnapshotStat[]
}>()

const columns: ColumnDef[] = [
  { key: 'stat_key', label: 'Stat' },
  { key: 'value', label: 'Value', align: 'right' },
]

const filter = ref('')

const filtered = computed(() => {
  const f = filter.value.toLowerCase()
  return f
    ? props.stats.filter(s => s.stat_key.toLowerCase().includes(f))
    : props.stats
})

function formatStatKey(key: string): string {
  return key.replace(/_/g, ' ').replace(/\b\w/g, c => c.toUpperCase())
}

function formatValue(v: number): string {
  return Number.isInteger(v) ? v.toLocaleString() : v.toFixed(2)
}
</script>
