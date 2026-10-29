<template>
  <div class="flex flex-col gap-2">
    <div class="flex items-center justify-between">
      <h3 class="text-sm font-medium text-text-secondary">Character Reports</h3>
      <button
        v-if="snapshots.length >= 2"
        class="px-3 py-1 bg-transparent border border-border-default text-text-secondary rounded cursor-pointer text-xs transition-all hover:bg-surface-elevated hover:text-text-primary"
        :disabled="compareIds.length !== 2"
        @click="emitCompare">
        Compare Selected ({{ compareIds.length }}/2)
      </button>
    </div>

    <div class="flex flex-wrap gap-2">
      <div
        v-for="snap in snapshots"
        :key="snap.id"
        class="flex items-center gap-2 p-2 border border-border-default rounded text-sm cursor-pointer transition-all hover:bg-surface-elevated"
        :class="{ 'bg-surface-elevated! border-accent-gold/40!': selected?.id === snap.id }"
        @click="emit('select', snap)">
        <input
          v-if="snapshots.length >= 2"
          type="checkbox"
          :checked="compareIds.includes(snap.id)"
          class="cursor-pointer accent-[#d4a843]"
          @click.stop="toggleCompare(snap.id)" />
        <div>
          <div class="text-text-primary">{{ formatTimestamp(snap.snapshot_timestamp) }}</div>
          <div class="text-xs text-text-muted">{{ snap.race }} · {{ snap.skill_count }} skills</div>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref } from 'vue'
import type { CharacterSnapshotSummary } from '../../types/database'
import { formatDateTimeFull } from '../../composables/useTimestamp'

defineProps<{
  snapshots: CharacterSnapshotSummary[]
  selected: CharacterSnapshotSummary | null
}>()

const emit = defineEmits<{
  select: [snapshot: CharacterSnapshotSummary]
  compare: [oldId: number, newId: number]
}>()

const compareIds = ref<number[]>([])

function toggleCompare(id: number) {
  const idx = compareIds.value.indexOf(id)
  if (idx >= 0) {
    compareIds.value.splice(idx, 1)
  } else if (compareIds.value.length < 2) {
    compareIds.value.push(id)
  } else {
    compareIds.value.shift()
    compareIds.value.push(id)
  }
}

function emitCompare() {
  if (compareIds.value.length === 2) {
    const sorted = [...compareIds.value].sort((a, b) => a - b)
    emit('compare', sorted[0], sorted[1])
  }
}

function formatTimestamp(ts: string): string {
  return formatDateTimeFull(ts)
}
</script>
