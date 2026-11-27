<template>
  <div class="flex flex-col h-full">
    <h3 class="section-heading mb-2">Recent Transactions</h3>

    <EmptyState
      v-if="store.liveEventLog.length === 0"
      variant="compact"
      primary="No transactions yet."
      secondary="Pick up or drop items to see activity here." />

    <div v-else class="flex flex-col gap-0.5 overflow-y-auto max-h-64 pr-1">
      <div
        v-for="(event, i) in store.liveEventLog"
        :key="`${event.timestamp}-${event.item_name}-${event.detail}-${i}`"
        class="flex items-center gap-2 py-1 px-2 rounded text-xs hover:bg-surface-elevated/50">
        <!-- Kind indicator -->
        <span
          class="w-1.5 h-1.5 rounded-full shrink-0"
          :class="kindColor(event.kind)" />

        <!-- Timestamp -->
        <span class="text-text-dim font-mono shrink-0">{{ formatTs(event.timestamp) }}</span>

        <!-- Item name -->
        <ItemInline :reference="event.item_name" />

        <!-- Detail -->
        <span class="text-text-muted ml-auto shrink-0">{{ event.detail }}</span>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { useGameStateStore, type InventoryEventKind } from '../../stores/gameStateStore'
import { formatAnyTimestamp as formatTs } from '../../composables/useTimestamp'
import EmptyState from '../Shared/EmptyState.vue'
import ItemInline from '../Shared/Item/ItemInline.vue'

const store = useGameStateStore()

function kindColor(kind: InventoryEventKind): string {
  switch (kind) {
    case 'added': return 'bg-status-active'
    case 'removed': return 'bg-status-inactive'
    case 'stack_changed': return 'bg-status-warning'
  }
}
</script>
