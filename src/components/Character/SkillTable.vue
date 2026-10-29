<template>
  <div class="flex flex-col gap-2 min-h-0 h-full">
    <div class="flex items-center justify-between shrink-0">
      <h3 class="section-heading">Skills</h3>
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
        :row-key="(row) => String(row.skill_name)"
        :sort-key="sortKey"
        :sort-dir="sortAsc ? 'asc' : 'desc'"
        compact
        size="sm"
        empty-variant="compact"
        :empty-text="emptyText"
        @sort="handleSort">
        <template #cell-skill_name="{ value }">
          <span class="text-text-primary">{{ value }}</span>
        </template>
        <template #cell-level="{ value }">
          <span class="text-accent-gold">{{ value }}</span>
        </template>
        <template #cell-bonus_levels="{ row }">
          <span class="text-text-muted">
            {{ (row.bonus_levels as number) > 0 ? `+${row.bonus_levels}` : '' }}
          </span>
        </template>
        <template #cell-xp_needed_for_next="{ row }">
          <span v-if="row.xp_needed_for_next === -1" class="text-text-secondary">MAX</span>
          <span v-else class="text-text-secondary">
            {{ formatNumber(row.xp_toward_next as number) }} / {{ formatNumber(row.xp_needed_for_next as number) }}
          </span>
        </template>
      </DataTable>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed } from 'vue'
import type { SnapshotSkillLevel } from '../../types/database'
import DataTable, { type ColumnDef } from '../Shared/DataTable.vue'

const props = defineProps<{
  skills: SnapshotSkillLevel[]
}>()

const columns: ColumnDef[] = [
  { key: 'skill_name', label: 'Skill', sortable: true },
  { key: 'level', label: 'Level', sortable: true, align: 'right' },
  { key: 'bonus_levels', label: 'Bonus', align: 'right' },
  { key: 'xp_needed_for_next', label: 'XP Progress', align: 'right' },
]

const filter = ref('')
const sortKey = ref<'skill_name' | 'level'>('skill_name')
const sortAsc = ref(true)

const filtered = computed(() => {
  const f = filter.value.toLowerCase()
  const list = f
    ? props.skills.filter(s => s.skill_name.toLowerCase().includes(f))
    : [...props.skills]

  list.sort((a, b) => {
    const dir = sortAsc.value ? 1 : -1
    if (sortKey.value === 'level') return (a.level - b.level) * dir
    return a.skill_name.localeCompare(b.skill_name) * dir
  })

  return list
})

// Only blame the filter when one is set: an empty snapshot must not claim the
// filters are to blame (same rule as InventoryTable).
const emptyText = computed(() =>
  filter.value ? 'No skills match the current filter.' : 'No skills in this report.')

function toggleSort(key: 'skill_name' | 'level') {
  if (sortKey.value === key) {
    sortAsc.value = !sortAsc.value
  } else {
    sortKey.value = key
    sortAsc.value = key === 'skill_name'
  }
}

// Keeps the file's own comparator/default-direction semantics; DataTable only
// reports the clicked column and renders the arrow from sortKey/sortAsc.
function handleSort({ key }: { key: string }) {
  toggleSort(key as 'skill_name' | 'level')
}

function formatNumber(n: number): string {
  return n.toLocaleString()
}
</script>
