<template>
  <div class="flex flex-col gap-3 h-full min-h-0">
    <!-- Stats header -->
    <div class="grid grid-cols-4 gap-3 flex-shrink-0">
      <StatCard
        label="ITEMS IN STOCK"
        :value="formatNumber(inStockCount)" />
      <StatCard
        label="EST. SHOP VALUE"
        :value="`${formatNumber(result?.estimated_value ?? 0)}g`" />
      <StatCard
        label="SOLD"
        :value="formatNumber(result?.total_sold ?? 0)" />
      <StatCard
        label="AVG DAILY REVENUE"
        :value="`${formatNumber(Math.round(result?.avg_daily_revenue ?? 0))}g`" />
    </div>

    <!-- Sales period dropdown + estimation note -->
    <div class="flex items-center justify-between flex-wrap gap-2 flex-shrink-0">
      <p class="text-xs text-text-secondary italic">
        Inventory is estimated from shop log events. It may be incomplete if older log data is missing.
      </p>
      <label class="flex items-center gap-2 text-xs text-text-secondary">
        Sales period
        <select
          v-model.number="periodDays"
          class="bg-surface-elevated border border-border-default rounded px-2 py-1 text-xs text-text-primary focus:outline-none focus:border-accent-gold/50">
          <option
            v-for="p in periodOptions"
            :key="p.value"
            :value="p.value">
            {{ p.label }}
          </option>
        </select>
      </label>
    </div>

    <!-- In-stock table -->
    <div class="flex-1 min-h-0 overflow-auto border border-border-default rounded">
      <DataTable
        :columns="inStockColumns"
        :rows="(sortedInStockItems as unknown as Record<string, unknown>[])"
        :row-key="(row) => String(row.item)"
        :sort-key="sortBy"
        :sort-dir="sortDir"
        :loading="loading"
        :hoverable="true"
        :sticky-header="true"
        compact
        empty-text="No items currently in stock."
        @sort="handleInStockSort">
        <template #cell-item="{ value }">
          <ItemInline :reference="(value as string)" />
        </template>
        <template #cell-price="{ row }">
          <span v-if="(row as unknown as InventoryItem).price_tiers.length === 1">
            {{ formatPriceTier((row as unknown as InventoryItem).price_tiers[0]) }}
          </span>
          <span
            v-else-if="(row as unknown as InventoryItem).price_tiers.length > 1"
            class="flex flex-col items-end gap-0.5">
            <span
              v-for="(tier, idx) in (row as unknown as InventoryItem).price_tiers"
              :key="idx"
              class="text-[11px]">
              {{ tier.qty }}&times;{{ formatPriceTier(tier) }}
            </span>
          </span>
          <span
            v-else
            class="text-text-secondary">—</span>
        </template>
        <template #cell-estimated_value="{ value }">
          <span class="text-accent-gold">{{ formatGold(value as number) }}</span>
        </template>
        <template #cell-avg_per_day="{ value }">
          {{ (value as number).toFixed(1) }}
        </template>
        <template #cell-last_activity_at="{ value }">
          <span class="text-text-secondary whitespace-nowrap">{{ formatShortDate(value as string | null) }}</span>
        </template>
      </DataTable>
    </div>

    <!-- Recently Sold Out collapsible — custom (not AccordionSection) so the
         window dropdown can be a sibling of the toggle button rather than
         nested inside it (a <select> inside a <button> would be invalid HTML
         and the click handlers fight each other). -->
    <div
      v-if="result"
      class="flex-shrink-0 border border-surface-elevated rounded">
      <div class="flex items-center justify-between px-3 py-2 hover:bg-surface-elevated/50 transition-colors">
        <button
          type="button"
          class="flex items-center gap-2 bg-transparent border-none cursor-pointer text-left flex-1"
          @click="soldOutOpen = !soldOutOpen">
          <span
            class="text-text-secondary text-xs transition-transform"
            :class="{ 'rotate-90': soldOutOpen }">
            ▶
          </span>
          <h4 class="text-text-secondary text-xs font-semibold uppercase tracking-wide m-0">
            Recently Sold Out ({{ recentlySoldOut.length }})
          </h4>
        </button>
        <label class="flex items-center gap-1 text-[10px] text-text-secondary">
          Window
          <select
            v-model.number="soldOutDays"
            class="bg-surface-elevated border border-border-default rounded px-1.5 py-0.5 text-[10px] text-text-primary focus:outline-none focus:border-accent-gold/50">
            <option :value="1">1 day</option>
            <option :value="3">3 days</option>
            <option :value="7">7 days</option>
            <option :value="14">14 days</option>
          </select>
        </label>
      </div>
      <div
        v-show="soldOutOpen"
        class="px-3 pb-3 max-h-64 overflow-auto">
        <DataTable
          :columns="soldOutColumns"
          :rows="(sortedRecentlySoldOut as unknown as Record<string, unknown>[])"
          :row-key="(row) => String(row.item)"
          :sort-key="soldOutSortBy"
          :sort-dir="soldOutSortDir"
          :hoverable="true"
          compact
          empty-text="Nothing has sold out in the selected window."
          empty-hint=""
          :row-class="soldOutRowClass"
          @sort="handleSoldOutSort">
          <template #cell-item="{ value }">
            <ItemInline :reference="(value as string)" />
          </template>
          <template #cell-last_known_price="{ value }">
            {{ formatLastPrice(value as number | null) }}
          </template>
          <template #cell-avg_per_day="{ value }">
            {{ (value as number).toFixed(1) }}
          </template>
          <template #cell-last_activity_at="{ value }">
            <span class="text-text-secondary whitespace-nowrap">{{ formatShortDate(value as string | null) }}</span>
          </template>
        </DataTable>
      </div>
    </div>

    <p
      v-if="error"
      class="text-xs text-red-400 flex-shrink-0">
      {{ error }}
    </p>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, watch, onMounted } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { useStallTrackerStore } from '../../stores/stallTrackerStore'
import StatCard from './StatCard.vue'
import DataTable from '../Shared/DataTable.vue'
import ItemInline from '../Shared/Item/ItemInline.vue'
import type {
  InventoryItem,
  InventoryResult,
  PriceTier,
  StallInventoryParams,
} from '../../types/stallTracker'

const store = useStallTrackerStore()

/** All-time sentinel: any value >= 99999 collapses to "all time" in the
 * Rust aggregator. 100000 is the canonical UI value. */
const ALL_TIME = 100000

const periodOptions: { value: number; label: string }[] = [
  { value: 1, label: 'Last day' },
  { value: 2, label: 'Last 2 days' },
  { value: 7, label: 'Last 7 days' },
  { value: 14, label: 'Last 14 days' },
  { value: 30, label: 'Last 30 days' },
  { value: ALL_TIME, label: 'All time' },
]

const inStockColumns = [
  { key: 'item', label: 'ITEM', sortable: true },
  { key: 'quantity', label: 'QTY', sortable: true, numeric: true },
  { key: 'price', label: 'PRICE', sortable: false, align: 'right' as const },
  { key: 'estimated_value', label: 'EST. VALUE', sortable: true, numeric: true },
  { key: 'period_sold', label: 'SOLD', sortable: true, numeric: true },
  { key: 'avg_per_day', label: 'AVG/DAY', sortable: true, numeric: true },
  { key: 'last_activity_at', label: 'LAST', sortable: true, align: 'right' as const },
]

const soldOutColumns = [
  { key: 'item', label: 'ITEM', sortable: true },
  { key: 'last_known_price', label: 'LAST PRICE', sortable: true, numeric: true },
  { key: 'period_sold', label: 'SOLD', sortable: true, numeric: true },
  { key: 'avg_per_day', label: 'AVG/DAY', sortable: true, numeric: true },
  { key: 'last_activity_at', label: 'LAST ACTIVITY', sortable: true, align: 'right' as const },
]

const periodDays = ref<number>(7)
const soldOutDays = ref<number>(3)
const soldOutOpen = ref<boolean>(false)

// Frontend-only sorting for the In Stock table. Inventory is computed in
// Rust by `aggregate_inventory` and returned all-at-once (item count is
// bounded — usually well under 100 per character — so backend pagination
// isn't needed). The default 'item asc' matches the BTreeMap order from the
// aggregator.
type InventorySortKey =
  | 'item'
  | 'quantity'
  | 'estimated_value'
  | 'period_sold'
  | 'avg_per_day'
  | 'last_activity_at'
const sortBy = ref<InventorySortKey>('item')
const sortDir = ref<'asc' | 'desc'>('asc')

// Recently Sold Out has its own independent sort state — different default
// (most recently sold out first) and a different column set (LAST PRICE
// instead of QTY/EST. VALUE since sold-out items have neither).
type SoldOutSortKey =
  | 'item'
  | 'last_known_price'
  | 'period_sold'
  | 'avg_per_day'
  | 'last_activity_at'
const soldOutSortBy = ref<SoldOutSortKey>('last_activity_at')
const soldOutSortDir = ref<'asc' | 'desc'>('desc')

const result = ref<InventoryResult | null>(null)
const loading = ref(false)
const error = ref<string | null>(null)

let reloadToken = 0

const inStockItems = computed<InventoryItem[]>(() =>
  result.value ? result.value.items.filter((i) => i.quantity > 0) : [],
)

const inStockCount = computed(() => inStockItems.value.length)

const sortedInStockItems = computed<InventoryItem[]>(() => {
  const items = [...inStockItems.value]
  const dir = sortDir.value === 'asc' ? 1 : -1
  items.sort((a, b) => {
    const va = a[sortBy.value]
    const vb = b[sortBy.value]
    // String compare for item / last_activity_at, numeric otherwise.
    if (typeof va === 'string' && typeof vb === 'string') {
      return va.localeCompare(vb) * dir
    }
    if (va === null && vb === null) return 0
    if (va === null) return 1
    if (vb === null) return -1
    return ((va as number) - (vb as number)) * dir
  })
  return items
})

function handleInStockSort(payload: { key: string; dir: 'asc' | 'desc' }) {
  sortBy.value = payload.key as InventorySortKey
  sortDir.value = payload.dir
}

const sortedRecentlySoldOut = computed<InventoryItem[]>(() => {
  const items = [...recentlySoldOut.value]
  const dir = soldOutSortDir.value === 'asc' ? 1 : -1
  items.sort((a, b) => {
    const va = a[soldOutSortBy.value]
    const vb = b[soldOutSortBy.value]
    if (typeof va === 'string' && typeof vb === 'string') {
      return va.localeCompare(vb) * dir
    }
    if (va === null && vb === null) return 0
    if (va === null) return 1
    if (vb === null) return -1
    return ((va as number) - (vb as number)) * dir
  })
  return items
})

function handleSoldOutSort(payload: { key: string; dir: 'asc' | 'desc' }) {
  soldOutSortBy.value = payload.key as SoldOutSortKey
  soldOutSortDir.value = payload.dir
}

function soldOutRowClass(): string {
  return 'opacity-60 hover:opacity-100'
}

/** Recently-sold-out: items with quantity 0 whose last activity falls within
 * the most recent N **distinct active dates** (not calendar days). The
 * backend exposes `active_dates` newest-first specifically so we can slice
 * here without recomputing the window — see plan 10.2. */
const recentlySoldOut = computed<InventoryItem[]>(() => {
  if (!result.value) return []
  const windowDates = new Set(result.value.active_dates.slice(0, soldOutDays.value))
  return result.value.items.filter((item) => {
    if (item.quantity > 0) return false
    if (!item.last_activity_at) return false
    return windowDates.has(item.last_activity_at.slice(0, 10))
  })
})

function buildParams(): StallInventoryParams {
  return {
    owner: store.currentOwner,
    periodDays: periodDays.value,
  }
}

async function reload() {
  if (!store.currentOwner) {
    result.value = null
    error.value = null
    return
  }
  const token = ++reloadToken
  loading.value = true
  error.value = null
  try {
    const r = await invoke<InventoryResult>('get_stall_inventory', { params: buildParams() })
    if (token !== reloadToken) return
    result.value = r
  } catch (e) {
    if (token === reloadToken) error.value = String(e)
    console.error('[StallInventoryTab] reload failed:', e)
  } finally {
    if (token === reloadToken) loading.value = false
  }
}

function formatNumber(n: number): string {
  return n.toLocaleString()
}

function formatGold(n: number): string {
  return `${n.toLocaleString()}g`
}

function formatPriceTier(tier: PriceTier): string {
  if (tier.price === null) return '—'
  return formatGold(Math.round(tier.price))
}

/** Recently Sold Out reads the dedicated `last_known_price` field — survives
 * the sellout collapse where `price_tiers` becomes empty. */
function formatLastPrice(price: number | null): string {
  if (price === null) return '—'
  return formatGold(Math.round(price))
}

function formatShortDate(iso: string | null): string {
  if (!iso) return '—'
  // event_at is "YYYY-MM-DD HH:MM:SS"; render the date part as "Apr 13".
  const datePart = iso.slice(0, 10)
  const d = new Date(`${datePart}T00:00:00`)
  if (Number.isNaN(d.getTime())) return datePart
  return d.toLocaleDateString(undefined, { month: 'short', day: 'numeric' })
}

watch(periodDays, () => void reload())
watch(() => store.dataVersion, () => void reload())
watch(
  () => store.currentOwner,
  () => {
    periodDays.value = 7
    soldOutDays.value = 3
    soldOutOpen.value = false
    void reload()
  },
)

onMounted(reload)
</script>
