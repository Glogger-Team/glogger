<template>
  <div class="flex flex-col gap-2">
    <FilterBar v-model="filter" placeholder="Filter recipes..." :result-count="filtered.length" result-label="recipes" />

    <div class="overflow-auto max-h-[60vh]">
      <DataTable
        :columns="columns"
        :rows="(filtered as unknown as Record<string, unknown>[])"
        :row-key="(row) => String(row.recipe_key)"
        :sort-key="sortKey"
        :sort-dir="sortAsc ? 'asc' : 'desc'"
        compact
        empty-text="No recipes"
        @sort="onSort">
        <template #cell-recipe_key="{ row }">
          <span class="text-text-primary">{{ row.recipe_key }}</span>
        </template>
        <template #cell-completions="{ row }">
          <span class="text-text-secondary">{{ (row.completions as number).toLocaleString() }}</span>
        </template>
      </DataTable>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed } from 'vue'
import type { SnapshotRecipeCompletion } from '../../types/database'
import DataTable from '../Shared/DataTable.vue'
import FilterBar from '../Shared/FilterBar.vue'

const props = defineProps<{
  recipes: SnapshotRecipeCompletion[]
}>()

const filter = ref('')
const sortKey = ref<string>('recipe_key')
const sortAsc = ref(true)

const columns = [
  { key: 'recipe_key', label: 'Recipe', sortable: true },
  { key: 'completions', label: 'Completions', sortable: true, align: 'right' as const, numeric: true },
]

const filtered = computed(() => {
  const f = filter.value.toLowerCase()
  const list = f
    ? props.recipes.filter(r => r.recipe_key.toLowerCase().includes(f))
    : [...props.recipes]

  list.sort((a, b) => {
    const dir = sortAsc.value ? 1 : -1
    if (sortKey.value === 'completions') return (a.completions - b.completions) * dir
    return a.recipe_key.localeCompare(b.recipe_key) * dir
  })

  return list
})

function onSort(payload: { key: string; dir: 'asc' | 'desc' }) {
  sortKey.value = payload.key
  sortAsc.value = payload.dir === 'asc'
}
</script>
