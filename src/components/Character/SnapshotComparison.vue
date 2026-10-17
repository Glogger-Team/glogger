<template>
  <div class="flex flex-col gap-2">
    <div v-if="diffs.length === 0" class="text-sm text-text-muted p-2">
      Select two snapshots above and click "Compare Selected" to see skill changes.
    </div>

    <div v-else class="overflow-auto max-h-[60vh]">
      <DataTable
        :columns="columns"
        :rows="(sortedDiffs as unknown as Record<string, unknown>[])"
        :row-key="(row) => String(row.skill_name)"
        compact
        size="sm">
        <template #cell-skill_name="{ value }">
          <span class="text-text-primary">{{ value }}</span>
        </template>
        <template #cell-old_level="{ value }">
          <span class="text-text-muted">{{ value }}</span>
        </template>
        <template #cell-new_level="{ value }">
          <span class="text-text-primary">{{ value }}</span>
        </template>
        <template #cell-level_change="{ row }">
          <span :class="changeColor(row.level_change as number)">
            {{ (row.level_change as number) > 0 ? `+${row.level_change}` : row.level_change || '-' }}
          </span>
        </template>
        <template #cell-xp_change="{ row }">
          <span class="text-text-secondary">{{ formatXpChange(row as unknown as SkillDiff) }}</span>
        </template>
      </DataTable>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import type { SkillDiff } from '../../types/database'
import DataTable, { type ColumnDef } from '../Shared/DataTable.vue'

const props = defineProps<{
  diffs: SkillDiff[]
}>()

const columns: ColumnDef[] = [
  { key: 'skill_name', label: 'Skill' },
  { key: 'old_level', label: 'Old Level', align: 'right' },
  { key: 'new_level', label: 'New Level', align: 'right' },
  { key: 'level_change', label: 'Change', align: 'right' },
  { key: 'xp_change', label: 'XP Change', align: 'right' },
]

const sortedDiffs = computed(() => {
  return [...props.diffs].sort((a, b) => {
    // Changed skills first, then alphabetical
    const aChanged = a.level_change !== 0 || a.old_xp !== a.new_xp
    const bChanged = b.level_change !== 0 || b.old_xp !== b.new_xp
    if (aChanged !== bChanged) return aChanged ? -1 : 1
    return a.skill_name.localeCompare(b.skill_name)
  })
})

function changeColor(change: number): string {
  if (change > 0) return 'text-value-positive'
  if (change < 0) return 'text-value-negative'
  return 'text-text-muted'
}

function formatXpChange(diff: SkillDiff): string {
  const change = diff.new_xp - diff.old_xp
  if (change === 0 && diff.level_change === 0) return '-'
  if (change > 0) return `+${change.toLocaleString()}`
  return change.toLocaleString()
}
</script>
