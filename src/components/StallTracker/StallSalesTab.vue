<template>
  <div class="flex flex-col gap-3 h-full min-h-0">
    <!-- Stats header -->
    <div class="grid grid-cols-4 gap-3 flex-shrink-0">
      <StatCard
        label="TOTAL SALES"
        :value="formatNumber(store.stats?.total_sales ?? 0)" />
      <StatCard
        label="TOTAL REVENUE"
        :value="`${formatNumber(store.stats?.total_revenue ?? 0)}g`" />
      <StatCard
        label="UNIQUE BUYERS"
        :value="formatNumber(store.stats?.unique_buyers ?? 0)" />
      <StatCard
        label="UNIQUE ITEMS"
        :value="formatNumber(store.stats?.unique_items ?? 0)" />
    </div>

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
        v-model="filterBuyer"
        :options="store.filterOptions.buyers"
        all-label="All buyers" />
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
        Showing {{ rows.length.toLocaleString() }} of {{ totalCount.toLocaleString() }} sales
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
        empty-text="No sales match the current filters."
        :row-class="rowClassFn"
        @sort="handleSort">
        <template #cell-ignored="{ row }">
          <button
            type="button"
            class="text-text-secondary hover:text-text-primary"
            :title="row.ignored ? 'Click to un-mute' : 'Click to mute (excludes from stats)'"
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
        <template #cell-item="{ value }">
          <ItemInline
            v-if="value"
            :reference="(value as string)" />
          <span
            v-else
            class="text-text-secondary">—</span>
        </template>
        <template #cell-price_unit="{ value }">
          {{ formatPrice(value as number | null) }}
        </template>
        <template #cell-price_total="{ value }">
          <span class="text-accent-gold">{{ formatPrice(value as number | null) }}</span>
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
      class="text-xs text-red-400 flex-shrink-0">
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
import StatCard from './StatCard.vue'
import type { StallEvent, StallEventsPage, StallEventsParams } from '../../types/stallTracker'

const store = useStallTrackerStore()

const PAGE_SIZE = 500

const columns = [
  { key: 'ignored', label: '', sortable: false, align: 'center' as const, width: '2rem' },
  { key: 'event_timestamp', label: 'DATE', sortable: true },
  { key: 'player', label: 'BUYER', sortable: true },
  { key: 'item', label: 'ITEM', sortable: true },
  { key: 'quantity', label: 'QTY', sortable: true, numeric: true },
  { key: 'price_unit', label: 'UNIT', sortable: true, numeric: true },
  { key: 'price_total', label: 'TOTAL', sortable: true, numeric: true },
]

// DataTable column keys differ from backend sort keys in one case.
const toBackendKey: Record<string, string> = { event_timestamp: 'event_at' }
const toDisplayKey: Record<string, string> = { event_at: 'event_timestamp' }

const rows = ref<StallEvent[]>([])
const totalCount = ref(0)
const loading = ref(false)
const error = ref<string | null>(null)

const filterDateFrom = ref<string>('')
const filterDateTo = ref<string>('')
const filterBuyer = ref<string | null>(null)
const filterItem = ref<string | null>(null)

const sortBy = ref<string>('event_at')
const sortDir = ref<'asc' | 'desc'>('desc')

const displaySortKey = computed(() => toDisplayKey[sortBy.value] ?? sortBy.value)

// In-flight request token: every reload bumps it. Stale loadMore/reload
// responses (e.g. user clicked a sort header before the previous query
// returned) check the token and discard themselves rather than clobbering
// the visible state with rows from the wrong sort/filter set.
let reloadToken = 0

const hasActiveFilters = computed(
  () => !!(filterDateFrom.value || filterDateTo.value || filterBuyer.value || filterItem.value),
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
    player: filterBuyer.value,
    item: filterItem.value,
    action: 'bought',
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
    if (token !== reloadToken) return // superseded by a newer reload
    rows.value = page.rows
    totalCount.value = page.total_count
    // Keep stats header in sync with current filters.
    void store.loadStats({
      dateFrom: filterDateFrom.value || null,
      dateTo: filterDateTo.value || null,
      player: filterBuyer.value,
      item: filterItem.value,
    })
  } catch (e) {
    if (token === reloadToken) error.value = String(e)
    console.error('[StallSalesTab] reload failed:', e)
  } finally {
    if (token === reloadToken) loading.value = false
  }
}

async function loadMore() {
  if (!store.currentOwner || loading.value) return
  // Capture the current token. If a reload supersedes us mid-request,
  // we discard the appended rows so we don't mix sort orders.
  const token = reloadToken
  loading.value = true
  try {
    const page = await invoke<StallEventsPage>('get_stall_events', {
      params: buildParams(rows.value.length),
    })
    if (token !== reloadToken) return // a newer reload changed sort/filters
    rows.value.push(...page.rows)
    totalCount.value = page.total_count
  } catch (e) {
    if (token === reloadToken) error.value = String(e)
    console.error('[StallSalesTab] loadMore failed:', e)
  } finally {
    if (token === reloadToken) loading.value = false
  }
}

async function handleToggleIgnored(row: StallEvent) {
  const action = row.ignored ? 'un-mute' : 'mute'
  const ok = await confirm(
    `${action === 'mute' ? 'Mute' : 'Un-mute'} this sale?\n\n` +
      `${row.player} bought ${row.item ?? '(unknown)'} for ${formatPrice(row.price_total)}g.\n\n` +
      `${action === 'mute' ? 'Muted sales are excluded from stats but stay visible in the list.' : 'Un-muting will include this sale in stats again.'}`,
    { title: 'Stall Tracker', kind: 'info' },
  )
  if (!ok) return
  try {
    await store.toggleIgnored(row.id, !row.ignored)
    // dataVersion bump triggers reload via the watcher below.
  } catch (e) {
    error.value = String(e)
    console.error('[StallSalesTab] toggleIgnored failed:', e)
  }
}

function clearFilters() {
  filterDateFrom.value = ''
  filterDateTo.value = ''
  filterBuyer.value = null
  filterItem.value = null
}

function formatNumber(n: number): string {
  return n.toLocaleString()
}

function formatPrice(n: number | null): string {
  if (n === null) return '—'
  return Math.round(n).toLocaleString()
}

// Auto-swap inverted date range so users can't accidentally produce
// an empty result by entering "to" before "from".
watch([filterDateFrom, filterDateTo], ([from, to]) => {
  if (from && to && from > to) {
    filterDateFrom.value = to
    filterDateTo.value = from
  }
})

// Debounced reload on filter change (200ms) — avoids spamming invokes
// while the user is still typing in a date input.
let filterTimer: ReturnType<typeof setTimeout> | null = null
watch(
  [filterDateFrom, filterDateTo, filterBuyer, filterItem],
  () => {
    if (filterTimer) clearTimeout(filterTimer)
    filterTimer = setTimeout(() => void reload(), 200)
  },
)

// Refetch when the store says the data changed (live ingest, toggle, clear, import).
watch(() => store.dataVersion, () => void reload())

// Reset filter state on character switch — buyer "Alvida" from char A
// would otherwise carry over to char B and silently return zero rows.
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
