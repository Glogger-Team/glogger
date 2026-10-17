<template>
  <div class="flex flex-col gap-3 h-full min-h-0">
    <!-- Filter row -->
    <div class="flex items-center gap-2 flex-wrap flex-shrink-0">
      <DatePicker
        v-model="filterDateFrom"
        placeholder="From date" />
      <span class="text-text-secondary text-xs">–</span>
      <DatePicker
        v-model="filterDateTo"
        placeholder="To date" />
      <SearchableSelect
        v-model="filterPlayer"
        :options="store.filterOptions.players"
        all-label="All players" />
      <SearchableSelect
        v-model="filterAction"
        :options="actionOptions"
        all-label="All actions" />
      <SearchableSelect
        v-model="filterItem"
        :options="store.filterOptions.items"
        all-label="All items" />

      <button
        v-if="hasActiveFilters"
        type="button"
        class="text-xs text-accent-gold hover:underline ml-1"
        @click="clearFilters">
        Clear filters
      </button>
      <span class="text-xs text-text-secondary ml-auto">
        Showing {{ rows.length.toLocaleString() }} of {{ totalCount.toLocaleString() }} entries
      </span>
    </div>

    <!-- Table -->
    <div class="flex-1 min-h-0 overflow-auto border border-border-default rounded">
      <DataTable
        :columns="columns"
        :rows="(rows as unknown as Record<string, unknown>[])"
        :row-key="(row) => String(row.id)"
        :sort-key="displaySortKey"
        :sort-dir="sortDir"
        :loading="loading"
        :hoverable="true"
        :sticky-header="true"
        compact
        empty-text="No entries match the current filters."
        :row-class="rowClassFn"
        @sort="handleSort">
        <template #cell-ignored="{ row }">
          <button
            type="button"
            class="text-text-secondary hover:text-text-primary"
            :title="row.ignored ? 'Click to un-mute' : 'Click to mute'"
            @click="handleToggleIgnored(row as unknown as StallEvent)">
            {{ row.ignored ? '○' : '⊘' }}
          </button>
        </template>
        <template #cell-event_timestamp="{ value }">
          <span class="text-text-secondary whitespace-nowrap">{{ value }}</span>
        </template>
        <template #cell-player="{ value }">
          <span class="text-entity-player">{{ value }}</span>
        </template>
        <template #cell-action="{ value }">
          <span
            class="px-1.5 py-0.5 rounded text-[10px] uppercase font-medium"
            :class="actionBadgeClass(value as string)">
            {{ value }}
          </span>
        </template>
        <template #cell-item="{ value }">
          <ItemInline
            v-if="value"
            :reference="(value as string)" />
          <span
            v-else
            class="text-text-secondary">—</span>
        </template>
        <template #cell-quantity="{ value }">
          {{ value || '' }}
        </template>
        <template #cell-price_total="{ value }">
          <span class="text-accent-gold">{{ formatGold(value as number | null) }}</span>
        </template>
      </DataTable>
    </div>

    <!-- Load more -->
    <div
      v-if="rows.length < totalCount"
      class="flex-shrink-0 flex justify-center">
      <button
        type="button"
        class="px-4 py-1.5 text-xs bg-surface-elevated border border-border-default rounded hover:border-accent-gold/50 text-text-primary disabled:opacity-50"
        :disabled="loading"
        @click="loadMore">
        {{ loading ? 'Loading…' : `Load more (${(totalCount - rows.length).toLocaleString()} remaining)` }}
      </button>
    </div>

    <p
      v-if="error"
      class="text-xs text-value-negative flex-shrink-0">
      {{ error }}
    </p>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, watch, onMounted, onBeforeUnmount } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { confirm } from '@tauri-apps/plugin-dialog'
import { useStallTrackerStore } from '../../stores/stallTrackerStore'
import SearchableSelect from '../Shared/SearchableSelect.vue'
import DatePicker from '../Shared/DatePicker.vue'
import DataTable from '../Shared/DataTable.vue'
import ItemInline from '../Shared/Item/ItemInline.vue'
import type { StallEvent, StallEventsPage, StallEventsParams } from '../../types/stallTracker'

const store = useStallTrackerStore()

const PAGE_SIZE = 500

const columns = [
  { key: 'ignored', label: '', sortable: false, align: 'center' as const, width: '2rem' },
  { key: 'event_timestamp', label: 'DATE', sortable: true },
  { key: 'player', label: 'PLAYER', sortable: true },
  { key: 'action', label: 'ACTION', sortable: true },
  { key: 'item', label: 'ITEM', sortable: true },
  { key: 'quantity', label: 'QTY', sortable: true, numeric: true },
  { key: 'price_total', label: 'GOLD', sortable: true, numeric: true },
]

// DataTable column keys differ from backend sort keys in one case.
const toBackendKey: Record<string, string> = { event_timestamp: 'event_at' }
const toDisplayKey: Record<string, string> = { event_at: 'event_timestamp' }

/** All seven action buckets, including `unknown` so the maintenance view can
 * surface unparseable entries (Sales tab hides them by default). */
const actionOptions: string[] = [
  'bought',
  'added',
  'removed',
  'configured',
  'visible',
  'collected',
  'unknown',
]

const rows = ref<StallEvent[]>([])
const totalCount = ref(0)
const loading = ref(false)
const error = ref<string | null>(null)

const filterDateFrom = ref<string>('')
const filterDateTo = ref<string>('')
const filterPlayer = ref<string | null>(null)
const filterAction = ref<string | null>(null)
const filterItem = ref<string | null>(null)

const sortBy = ref<string>('event_at')
const sortDir = ref<'asc' | 'desc'>('desc')

const displaySortKey = computed(() => toDisplayKey[sortBy.value] ?? sortBy.value)

let reloadToken = 0

const hasActiveFilters = computed(
  () =>
    !!(
      filterDateFrom.value ||
      filterDateTo.value ||
      filterPlayer.value ||
      filterAction.value ||
      filterItem.value
    ),
)

function rowClassFn(row: Record<string, unknown>): string {
  return (row as unknown as StallEvent).ignored ? 'opacity-35' : ''
}

function handleSort(payload: { key: string; dir: 'asc' | 'desc' }) {
  sortBy.value = toBackendKey[payload.key] ?? payload.key
  sortDir.value = payload.dir
  void reload()
}

function buildParams(offset: number): StallEventsParams {
  return {
    owner: store.currentOwner,
    dateFrom: filterDateFrom.value || null,
    dateTo: filterDateTo.value || null,
    player: filterPlayer.value,
    item: filterItem.value,
    action: filterAction.value,
    // The Shop Log shows ALL actions including 'unknown'. When the user
    // explicitly filters to 'unknown', `forceAction` overrides the backend's
    // default `action != 'unknown'` filter so unparseable entries surface.
    forceAction: filterAction.value === 'unknown' ? 'unknown' : null,
    includeIgnored: true,
    sortBy: sortBy.value,
    sortDir: sortDir.value,
    limit: PAGE_SIZE,
    offset,
  }
}

async function reload() {
  if (!store.currentOwner) {
    rows.value = []
    totalCount.value = 0
    error.value = null
    return
  }
  const token = ++reloadToken
  loading.value = true
  error.value = null
  try {
    const page = await invoke<StallEventsPage>('get_stall_events', {
      params: buildParams(0),
    })
    if (token !== reloadToken) return
    rows.value = page.rows
    totalCount.value = page.total_count
  } catch (e) {
    if (token === reloadToken) error.value = String(e)
    console.error('[StallShopLogTab] reload failed:', e)
  } finally {
    if (token === reloadToken) loading.value = false
  }
}

async function loadMore() {
  if (!store.currentOwner || loading.value) return
  const token = reloadToken
  loading.value = true
  try {
    const page = await invoke<StallEventsPage>('get_stall_events', {
      params: buildParams(rows.value.length),
    })
    if (token !== reloadToken) return
    rows.value.push(...page.rows)
    totalCount.value = page.total_count
  } catch (e) {
    if (token === reloadToken) error.value = String(e)
    console.error('[StallShopLogTab] loadMore failed:', e)
  } finally {
    if (token === reloadToken) loading.value = false
  }
}

async function handleToggleIgnored(row: StallEvent) {
  const ok = await confirm(
    `${row.ignored ? 'Un-mute' : 'Mute'} this ${row.action} entry?\n\n${row.raw_message}`,
    { title: 'Stall Tracker', kind: 'info' },
  )
  if (!ok) return
  try {
    await store.toggleIgnored(row.id, !row.ignored)
  } catch (e) {
    error.value = String(e)
    console.error('[StallShopLogTab] toggleIgnored failed:', e)
  }
}

function clearFilters() {
  filterDateFrom.value = ''
  filterDateTo.value = ''
  filterPlayer.value = null
  filterAction.value = null
  filterItem.value = null
}

function formatGold(n: number | null): string {
  if (n === null || n === 0) return ''
  return n.toLocaleString()
}

function actionBadgeClass(action: string): string {
  switch (action) {
    case 'bought':
      return 'bg-green-500/15 text-green-400'
    case 'added':
      return 'bg-blue-500/15 text-blue-400'
    case 'removed':
      return 'bg-red-500/15 text-red-400'
    case 'configured':
      return 'bg-amber-500/15 text-amber-400'
    case 'visible':
      return 'bg-cyan-500/15 text-cyan-400'
    case 'collected':
      return 'bg-accent-gold/15 text-accent-gold'
    default:
      return 'bg-surface-elevated text-text-secondary'
  }
}

watch([filterDateFrom, filterDateTo], ([from, to]) => {
  if (from && to && from > to) {
    filterDateFrom.value = to
    filterDateTo.value = from
  }
})

let filterTimer: ReturnType<typeof setTimeout> | null = null
watch(
  [filterDateFrom, filterDateTo, filterPlayer, filterAction, filterItem],
  () => {
    if (filterTimer) clearTimeout(filterTimer)
    filterTimer = setTimeout(() => void reload(), 200)
  },
)

watch(() => store.dataVersion, () => void reload())
watch(
  () => store.currentOwner,
  () => {
    clearFilters()
    void reload()
  },
)

onMounted(reload)
onBeforeUnmount(() => {
  if (filterTimer) clearTimeout(filterTimer)
})
</script>
