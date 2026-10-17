<template>
  <div class="flex flex-col gap-2 min-h-0 h-full">
    <div class="flex items-center justify-between shrink-0">
      <h3 class="section-heading">Player Attributes</h3>
      <div class="flex items-center gap-2">
        <input
          v-model="filter"
          type="text"
          placeholder="Filter..."
          class="px-2 py-1 bg-surface-base border border-border-default rounded text-xs text-text-primary placeholder-text-muted focus:outline-none focus:border-accent-gold/50 w-24" />
        <span class="text-xs text-text-muted">{{ filtered.length }}</span>
      </div>
    </div>

    <div v-if="loading" class="text-xs text-text-muted italic">Loading...</div>

    <div v-else-if="attributes.length === 0" class="text-xs text-text-dim italic">
      No attribute data yet. Attributes populate when you enter a zone in-game.
    </div>

    <div v-else class="flex-1 overflow-y-auto min-h-0">
      <DataTable
        :columns="columns"
        :rows="(filtered as unknown as Record<string, unknown>[])"
        :row-key="(row) => String(row.attribute_name)"
        compact
        size="sm">
        <template #cell-attribute_name="{ value }">
          <span class="text-text-primary">{{ formatAttrName(value as string) }}</span>
        </template>
        <template #cell-current_value="{ row }">
          <span class="text-accent-gold font-mono">{{ fmtVal(row.current_value as number) }}</span>
        </template>
        <template #cell-min_value="{ row }">
          <span
            class="font-mono"
            :class="(row.min_value as number) < (row.current_value as number) ? 'text-blue-400' : 'text-text-dim'">
            {{ fmtVal(row.min_value as number) }}
          </span>
        </template>
        <template #cell-max_value="{ row }">
          <span
            class="font-mono"
            :class="(row.max_value as number) > (row.current_value as number) ? 'text-green-400' : 'text-text-dim'">
            {{ fmtVal(row.max_value as number) }}
          </span>
        </template>
      </DataTable>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { useSettingsStore } from '../../stores/settingsStore'
import DataTable, { type ColumnDef } from '../Shared/DataTable.vue'

interface AttributeExtreme {
  attribute_name: string
  current_value: number
  min_value: number
  max_value: number
}

const columns: ColumnDef[] = [
  { key: 'attribute_name', label: 'Attribute' },
  { key: 'current_value', label: 'Current', align: 'right' },
  { key: 'min_value', label: 'Min', align: 'right' },
  { key: 'max_value', label: 'Max', align: 'right' },
]

const settings = useSettingsStore()
const attributes = ref<AttributeExtreme[]>([])
const loading = ref(false)
const filter = ref('')

let unlisten: UnlistenFn | null = null

const filtered = computed(() => {
  const f = filter.value.toLowerCase()
  if (!f) return attributes.value
  return attributes.value.filter(a =>
    a.attribute_name.toLowerCase().includes(f)
  )
})

function formatAttrName(name: string): string {
  return name.replace(/_/g, ' ').replace(/\b\w/g, c => c.toUpperCase())
}

function fmtVal(value: number): string {
  return Number.isInteger(value) ? value.toLocaleString() : value.toLocaleString(undefined, { maximumFractionDigits: 1 })
}

async function loadAttributes() {
  const char = settings.settings.activeCharacterName
  const server = settings.settings.activeServerName
  if (!char || !server) return

  loading.value = true
  try {
    attributes.value = await invoke<AttributeExtreme[]>('get_attribute_extremes', {
      characterName: char,
      serverName: server,
    })
  } catch (e) {
    console.error('Failed to load attribute extremes:', e)
  } finally {
    loading.value = false
  }
}

onMounted(async () => {
  await loadAttributes()
  unlisten = await listen<string[]>('game-state-updated', (event) => {
    if (event.payload.includes('attributes')) {
      loadAttributes()
    }
  })
})

onUnmounted(() => {
  if (unlisten) unlisten()
})
</script>
