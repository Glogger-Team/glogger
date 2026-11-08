<template>
  <div class="flex flex-col gap-2 h-full overflow-hidden">
    <div class="text-xs text-text-muted">
      {{ rows.length }} unattributed gain{{ rows.length === 1 ? '' : 's' }} for
      {{ characterLabel }} — loot the pipeline couldn't attribute to a source.
    </div>
    <div class="flex-1 min-h-0 overflow-y-auto -mr-1 pr-1">
      <DataTable
        :columns="columns"
        :rows="(rows as unknown as Record<string, unknown>[])"
        :row-key="(row) => String(row.id)"
        :loading="loading"
        compact
        empty-variant="compact"
        empty-text="No unattributed gains."
        empty-hint="Every recorded gain has a known source.">
        <template #cell-item_name="{ row }">
          <ItemMinicard
            :reference="row.item_name as string"
            width="min"
            :bordered="false" />
        </template>
        <template #cell-quantity="{ row }">
          <span class="text-accent-gold font-semibold">{{ (row.quantity as number).toLocaleString() }}</span>
        </template>
      </DataTable>
    </div>
    <div v-if="error" class="text-[#c87e7e] text-sm">{{ error }}</div>
  </div>
</template>

<script setup lang="ts">
// Unknown-source diagnostic report: item gains whose provenance is unresolved
// (`unknown` / `uncertain` from the Player.log path, NULL from the Chat.log
// path). Newest first, capped at 100 rows.
import { computed, onMounted, ref, watch } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { useSettingsStore } from '../../stores/settingsStore'
import DataTable, { type ColumnDef } from '../Shared/DataTable.vue'
import ItemMinicard from '../Shared/Item/ItemMinicard.vue'

interface UnknownSourceRow {
  id: number
  timestamp: string
  item_name: string
  quantity: number
  context: string
  source: string
  source_kind: string | null
  source_details: string | null
}

const settingsStore = useSettingsStore()
const rows = ref<UnknownSourceRow[]>([])
const loading = ref(false)
const error = ref('')
// Load generation: a superseded request must not overwrite rows, error,
// or loading belonging to a newer load (character/server switch mid-flight,
// including A -> B -> A where a selection-key check would alias).
const latestRequest = ref(0)

const characterLabel = computed(() => {
  const c = settingsStore.settings.activeCharacterName
  const s = settingsStore.settings.activeServerName
  return c ? `${c}${s ? ` (${s})` : ''}` : 'active character'
})

 const columns: ColumnDef[] = [
   { key: 'timestamp', label: 'When' },
   { key: 'item_name', label: 'Item' },
   { key: 'quantity', label: 'Qty', align: 'right', numeric: true },
   { key: 'context', label: 'Context' },
   { key: 'source', label: 'Path' },
   { key: 'source_kind', label: 'Kind' },
   { key: 'source_details', label: 'Details', cellClass: 'max-w-0 truncate' },
 ]

async function load() {
  const character = settingsStore.settings.activeCharacterName
  const server = settingsStore.settings.activeServerName
  const request = ++latestRequest.value
  rows.value = []
   if (!character || !server) {
     rows.value = []
     error.value = ''
    if (latestRequest.value === request) loading.value = false
     return
   }
   loading.value = true
  try {
    const result = await invoke<UnknownSourceRow[]>('get_unknown_source_transactions', {
      characterName: character,
      serverName: server,
      limit: 100,
    })
    if (latestRequest.value !== request) return
    rows.value = result
    error.value = ''
  } catch (e) {
    if (latestRequest.value !== request) return
    error.value = String(e)
  } finally {
    if (latestRequest.value === request) loading.value = false
  }
}

onMounted(load)
watch(
  () => [settingsStore.settings.activeCharacterName, settingsStore.settings.activeServerName],
  load,
)
</script>
