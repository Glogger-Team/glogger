<template>
  <div class="flex flex-col gap-4 h-full">
    <!-- Not tailing warning -->
    <div v-if="!coordinator.isPlayerLogTailing"
      class="p-3 bg-yellow-900/20 border border-yellow-700/40 rounded text-sm text-yellow-300 flex items-center justify-between">
      <span>Player log tailing is not active. Start tailing to track your inventory in real-time.</span>
      <button @click="startTailing" class="btn btn-secondary text-xs whitespace-nowrap ml-3">Start Tailing</button>
    </div>
    <!-- Current-client note: the game no longer writes item events, so this
         live tab stays empty even while tailing. For a snapshot view, run
         /outputitems in-game, then Character → Inventory → Import Inventory. -->
    <div v-else
      class="p-3 bg-surface-dark border border-border-default rounded text-xs text-text-dim italic">
      Live tracking is unavailable: the current game client writes no item events to
      Player.log, so this tab stays empty. For a snapshot view, export a storage
      report in-game (<span class="font-semibold">Settings → VIP</span>), then use
      <span class="font-semibold">Character → Inventory → Import Inventory</span>.
    </div>

    <!-- Summary bar -->
    <div class="flex gap-6 text-sm items-center">
      <div class="flex gap-1.5 items-baseline">
        <span class="text-text-muted">Items:</span>
        <span class="text-text-primary font-medium">{{ store.liveItemCount.toLocaleString() }}</span>
      </div>
      <div class="flex gap-1.5 items-baseline">
        <span class="text-text-muted">Total Qty:</span>
        <span class="text-text-primary font-medium">{{ store.liveTotalStacks.toLocaleString() }}</span>
      </div>
      <div v-if="coordinator.isPlayerLogTailing" class="flex gap-1.5 items-center">
        <span class="w-2 h-2 rounded-full bg-status-active animate-pulse" />
        <span class="text-text-muted text-xs">Live</span>
      </div>
    </div>

    <!-- Search -->
    <div class="flex items-center gap-3">
      <div class="relative">
        <input
          v-model="searchQuery"
          type="text"
          placeholder="Search items..."
          class="pl-7 pr-7 py-1 bg-surface-base border border-border-default rounded text-xs text-text-primary placeholder-text-muted w-48 focus:outline-none focus:border-accent-gold/50"
        />
        <span class="absolute left-2 top-1/2 -translate-y-1/2 text-text-muted text-xs pointer-events-none">&#x1F50D;</span>
        <button
          v-if="searchQuery"
          class="absolute right-1.5 top-1/2 -translate-y-1/2 text-text-muted hover:text-text-primary text-xs cursor-pointer"
          @click="searchQuery = ''"
        >&times;</button>
      </div>
      <span v-if="searchQuery" class="text-xs text-text-muted">
        {{ filteredItems.length }} match{{ filteredItems.length !== 1 ? 'es' : '' }}
      </span>
    </div>

    <!-- Empty state -->
    <EmptyState
      v-if="!store.isLivePopulated && coordinator.isPlayerLogTailing"
      primary="Waiting for inventory data."
      secondary="Log into a character in-game to populate." />

    <!-- Two-panel layout: inventory table + activity feed -->
    <div v-if="store.isLivePopulated" class="flex gap-4 flex-1 min-h-0">
      <!-- Item table -->
      <div class="flex-1 overflow-auto">
        <DataTable
          :columns="columns"
          :rows="(filteredItems as unknown as Record<string, unknown>[])"
          :row-key="(row) => String(row.instance_id)"
          :hoverable="false"
          :row-class="rowClassFn"
          compact
          size="sm"
          empty-text="No items match your search.">
          <template #cell-slot_index="{ value }">
            <span class="text-text-muted text-xs font-mono">
              {{ (value as number) >= 0 ? (value as number) : '-' }}
            </span>
          </template>
          <template #cell-item_name="{ value }">
            <ItemInline :reference="(value as string)" />
          </template>
          <template #cell-stack_size="{ value }">
            <span class="font-mono text-text-primary">
              {{ (value as number) > 0 ? (value as number).toLocaleString() : '-' }}
            </span>
          </template>
        </DataTable>
      </div>

      <!-- Activity feed -->
      <div v-if="store.liveEventLog.length > 0" class="w-64 flex-shrink-0 flex flex-col border-l border-border-default pl-4">
        <h3 class="text-xs text-text-muted font-medium mb-2">Activity</h3>
        <div class="overflow-auto flex-1 space-y-1">
          <div
            v-for="(entry, i) in store.liveEventLog"
            :key="i"
            class="text-xs py-0.5"
          >
            <span
              class="font-medium"
              :class="{
                'text-value-positive': entry.kind === 'added',
                'text-value-negative': entry.kind === 'removed',
                'text-blue-400': entry.kind === 'stack_changed',
              }"
            >
              {{ entry.kind === 'added' ? '+' : entry.kind === 'removed' ? '-' : '~' }}
            </span>
            <span class="text-text-secondary ml-1">{{ entry.item_name }}</span>
            <span class="text-text-muted ml-1">({{ entry.detail }})</span>
          </div>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed } from 'vue'
import { useGameStateStore, type LiveInventoryItem } from '../../stores/gameStateStore'
import { useCoordinatorStore } from '../../stores/coordinatorStore'
import EmptyState from '../Shared/EmptyState.vue'
import DataTable from '../Shared/DataTable.vue'
import ItemInline from '../Shared/Item/ItemInline.vue'

const store = useGameStateStore()
const coordinator = useCoordinatorStore()

const searchQuery = ref('')

const columns = [
  { key: 'slot_index', label: 'Slot', width: '3rem' },
  { key: 'item_name', label: 'Item' },
  { key: 'stack_size', label: 'Qty', align: 'right' as const, numeric: true, width: '5rem' },
]

/** Per-row styling now that DataTable owns the <tr>: the freshly-looted left
 * accent plus the original hover token (DataTable's default hover would swap
 * surface-elevated/50 for surface-row-hover). */
function rowClassFn(row: Record<string, unknown>): string {
  const classes = ['transition-colors', 'hover:bg-surface-elevated/50']
  if ((row as unknown as LiveInventoryItem).is_new) classes.push('border-l-2 border-l-accent-gold/60')
  return classes.join(' ')
}

async function startTailing() {
  try {
    await coordinator.startPlayerTailing()
  } catch (e) {
    console.error('Failed to start Player.log tailing:', e)
  }
}

const filteredItems = computed(() => {
  const q = searchQuery.value.trim().toLowerCase()
  if (!q) return store.liveItems
  return store.liveItems.filter(item => item.item_name.toLowerCase().includes(q))
})
</script>
