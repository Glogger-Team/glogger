<template>
  <div class="flex flex-col gap-2 min-h-0 h-full">
    <h3 class="section-heading shrink-0">Currencies</h3>
    <div class="overflow-auto flex-1 min-h-0">
      <DataTable
        :columns="columns"
        :rows="(currencies as unknown as Record<string, unknown>[])"
        compact
        size="sm"
        empty-text="No currencies.">
        <template #cell-currency_key="{ row }">
          <span class="text-text-primary">{{ formatKey(row.currency_key as string) }}</span>
        </template>
        <template #cell-amount="{ row }">
          <span class="text-accent-gold">{{ (row.amount as number).toLocaleString() }}</span>
        </template>
      </DataTable>
    </div>
  </div>
</template>

<script setup lang="ts">
import type { SnapshotCurrency } from '../../types/database'
import DataTable, { type ColumnDef } from '../Shared/DataTable.vue'

defineProps<{
  currencies: SnapshotCurrency[]
}>()

const columns: ColumnDef[] = [
  { key: 'currency_key', label: 'Currency' },
  { key: 'amount', label: 'Amount', align: 'right' },
]

function formatKey(key: string): string {
  return key.replace(/_/g, ' ').replace(/\b\w/g, c => c.toUpperCase())
}
</script>
