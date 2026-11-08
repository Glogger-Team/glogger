<template>
  <div class="flex flex-col gap-3 h-full min-h-0">
    <!-- Control row -->
    <div class="flex items-center gap-2 flex-wrap flex-shrink-0">
      <!-- Dropdown 1: specific material (one at a time). Selecting a material
           switches to item mode; "All materials" returns to summary mode. -->
      <SearchableSelect
        v-model="selectedItem"
        :options="store.filterOptions.items"
        all-label="All materials" />

      <!-- Dropdown 2: summary view. Picking one clears the material selection
           (exclusive modes). "Total sales" is the all-label option. -->
      <SearchableSelect
        v-model="summaryModel"
        :options="summaryOptions"
        all-label="Total sales" />

      <!-- Dropdown 3: time period. "All time" is the all-label option. -->
      <SearchableSelect
        v-model="periodModel"
        :options="periodOptions"
        all-label="All time" />

      <!-- Metric toggle: gold revenue vs units sold. -->
      <div class="inline-flex rounded border border-border-default overflow-hidden">
        <button
          v-for="m in metrics"
          :key="m.value"
          type="button"
          class="px-3 py-1 text-xs transition-colors"
          :class="
            metric === m.value
              ? 'bg-accent-gold/20 text-accent-gold font-medium'
              : 'bg-surface-elevated text-text-secondary hover:text-text-primary'
          "
          @click="metric = m.value">
          {{ m.label }}
        </button>
      </div>

      <span class="text-xs text-text-secondary ml-auto whitespace-nowrap">
        Showing: <span class="text-text-primary">{{ activeLabel }}</span>
        <span
          v-if="bucketLabel"
          class="text-text-secondary"> · {{ bucketLabel }}</span>
      </span>
    </div>

    <!-- Chart -->
    <div
      v-if="hasChart"
      class="flex-1 min-h-0 border border-border-default rounded p-2">
      <VueUiXy
        :key="chartKey"
        :dataset="dataset"
        :config="config" />
    </div>

    <div
      v-else
      class="flex-1 min-h-0 flex items-center justify-center text-text-secondary text-sm">
      <SkeletonLoader
        v-if="loading"
        variant="text"
        :lines="3"
        width="w-48" />
      <span v-else-if="error">{{ error }}</span>
      <span v-else>No sales in this period for the current selection.</span>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, watch, onMounted, onBeforeUnmount } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { VueUiXy } from 'vue-data-ui'
import type { VueUiXyConfig, VueUiXyDatasetItem } from 'vue-data-ui'
import { useStallTrackerStore } from '../../stores/stallTrackerStore'
import SearchableSelect from '../Shared/SearchableSelect.vue'
import SkeletonLoader from '../Shared/SkeletonLoader.vue'
import type {
  SalesTimeseriesResult,
  StallTimeseriesParams,
  TrendsPeriod,
} from '../../types/stallTracker'

const store = useStallTrackerStore()

// Ten muted, matte mid-tone hues — one per line. Top-10 mode draws all ten;
// Top-5 uses the first five; single-item and Total use the first.
// Index-based so each line on the chart is a distinct color.
//
// These sit a notch deeper/more saturated than true pastels on purpose: very
// light pale colors wash out toward white on the dark background and — more so
// — under Windows HDR SDR-tone-mapping. Mid-tone (≈55-65% lightness, moderate
// chroma) keeps them matte/non-vivid while holding a readable hue.
const PALETTE = [
  '#5f9ad4', // blue
  '#d18585', // dusty rose
  '#82c07b', // sage green
  '#a98fd6', // muted violet
  '#d9bd6e', // ochre / gold
  '#6dc0b6', // teal
  '#dca06e', // clay / amber
  '#c98cbf', // orchid
  '#a4b86e', // olive
  '#8395cc', // indigo / periwinkle
]

type SummaryMode = 'top5' | 'top10' | 'total'
type Metric = 'gold' | 'units'

const metrics: { value: Metric; label: string }[] = [
  { value: 'gold', label: 'Councils' },
  { value: 'units', label: 'Units' },
]

// ── Selection state ──────────────────────────────────────────────────────
// selectedItem != null → item mode. Otherwise summaryMode drives the view.
const selectedItem = ref<string | null>(null)
const summaryMode = ref<SummaryMode>('top5')
const period = ref<TrendsPeriod>('7d')
const metric = ref<Metric>('gold')

// Chart label colors resolve theme.css text tokens at runtime: vue-data-ui
// config takes plain color strings, so read the custom properties with
// getComputedStyle (fallbacks match the token values in theme.css).
function themeText(token: string, fallback: string): string {
  const value = getComputedStyle(document.documentElement).getPropertyValue(token).trim()
  return value || fallback
}

const textPrimary = themeText('--color-text-primary', '#d4d4d4')
const textSecondary = themeText('--color-text-secondary', '#a0a0a0')

// Dropdown 2 — summary view. "Total sales" maps to the all-label (null).
const summaryOptions = ['Top 5 grossing', 'Top 10 grossing']
const summaryModel = computed<string | null>({
  get: () =>
    summaryMode.value === 'total'
      ? null
      : summaryMode.value === 'top5'
        ? 'Top 5 grossing'
        : 'Top 10 grossing',
  set: (v) => {
    summaryMode.value = v === null ? 'total' : v.startsWith('Top 5') ? 'top5' : 'top10'
    // Picking a summary exits item mode (exclusive views).
    selectedItem.value = null
  },
})

// Dropdown 3 — period. "All time" maps to the all-label (null).
const periodOptions = ['Last 24 hours', 'Last 3 days', 'Last 7 days', 'Last 30 days']
const periodLabelToCode: Record<string, TrendsPeriod> = {
  'Last 24 hours': '1d',
  'Last 3 days': '3d',
  'Last 7 days': '7d',
  'Last 30 days': '1m',
}
const periodCodeToLabel: Record<TrendsPeriod, string | null> = {
  '1d': 'Last 24 hours',
  '3d': 'Last 3 days',
  '7d': 'Last 7 days',
  '1m': 'Last 30 days',
  all: null,
}
const periodModel = computed<string | null>({
  get: () => periodCodeToLabel[period.value],
  set: (v) => {
    period.value = v === null ? 'all' : periodLabelToCode[v] ?? '7d'
  },
})

const activeLabel = computed(() => {
  if (selectedItem.value) return selectedItem.value
  if (summaryMode.value === 'total') return 'Total sales'
  return summaryMode.value === 'top5' ? 'Top 5 grossing' : 'Top 10 grossing'
})

const isTotalMode = computed(() => !selectedItem.value && summaryMode.value === 'total')

const bucketLabel = computed(() => {
  switch (result.value?.bucket) {
    case 'hour':
      return 'hourly'
    case 'week':
      return 'weekly'
    case 'month':
      return 'monthly'
    case 'day':
      return 'daily'
    default:
      return ''
  }
})

// ── Data fetch ─────────────────────────────────────────────────────────
const result = ref<SalesTimeseriesResult | null>(null)
const loading = ref(false)
const error = ref<string | null>(null)
let reloadToken = 0

function buildParams(): StallTimeseriesParams {
  const base: StallTimeseriesParams = { owner: store.currentOwner, period: period.value }
  if (selectedItem.value) return { ...base, item: selectedItem.value }
  if (summaryMode.value === 'total') return base
  return { ...base, topN: summaryMode.value === 'top5' ? 5 : 10 }
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
    const r = await invoke<SalesTimeseriesResult>('get_stall_sales_timeseries', {
      params: buildParams(),
    })
    if (token !== reloadToken) return
    result.value = r
  } catch (e) {
    if (token === reloadToken) error.value = String(e)
    console.error('[StallTrendsTab] reload failed:', e)
  } finally {
    if (token === reloadToken) loading.value = false
  }
}

// ── Chart wiring ─────────────────────────────────────────────────────────
const periodLabels = computed(() => result.value?.periods.map((p) => p.label) ?? [])

const hasChart = computed(
  () => !!result.value && result.value.lines.length > 0 && result.value.periods.length > 0,
)

const dataset = computed<VueUiXyDatasetItem[]>(() => {
  if (!result.value) return []
  return result.value.lines.map((line, i) => ({
    name: line.item,
    series: metric.value === 'gold' ? line.gold : line.units,
    type: 'line',
    color: PALETTE[i % PALETTE.length],
    smooth: false,
    useArea: false,
    dataLabels: false,
  }))
})

// VueUiXy re-renders cleanly on dataset/metric/period change when keyed.
const chartKey = computed(() => `${metric.value}-${period.value}-${activeLabel.value}-${periodLabels.value.length}`)

function formatCompact(n: number): string {
  const abs = Math.abs(n)
  if (abs >= 1_000_000) return `${(n / 1_000_000).toFixed(1).replace(/\.0$/, '')}M`
  if (abs >= 1_000) return `${(n / 1_000).toFixed(1).replace(/\.0$/, '')}k`
  return `${Math.round(n)}`
}

/** Custom tooltip for Total mode: the default per-series line plus an
 * "avg cost/unit" row (Councils ÷ units for the hovered bucket). Reads the raw
 * gold/units from the result by the hovered index so the average is exact
 * regardless of which metric is currently plotted. Labels come from our own
 * date buckets (no user input), so the interpolated HTML is safe. */
function buildTotalTooltip(params: any): string {
  const line = result.value?.lines[0]
  const dp = params?.datapoint?.[0]
  const i =
    typeof params?.absoluteIndex === 'number'
      ? params.absoluteIndex
      : (params?.dateLabel?.absoluteIndex ?? -1)

  const label = params?.dateLabel?.text ?? (i >= 0 ? periodLabels.value[i] ?? '' : '')
  const color = dp?.color ?? PALETTE[0]
  const value = Number(dp?.value ?? 0)
  const valueText =
    metric.value === 'gold'
      ? `${value.toLocaleString()} Councils`
      : `${value.toLocaleString()} units`

  const gold = line && i >= 0 ? line.gold[i] : undefined
  const units = line && i >= 0 ? line.units[i] : undefined
  const avgText =
    gold != null && units != null && units > 0
      ? `${(gold / units).toLocaleString(undefined, { maximumFractionDigits: 1 })} Councils`
      : '—'

  return `
    <div style="display:flex;flex-direction:column;gap:3px;text-align:left;">
      <div style="font-weight:600;">${label}</div>
      <div style="display:flex;align-items:center;gap:6px;">
        <span style="display:inline-block;width:10px;height:10px;border-radius:2px;background:${color};"></span>
        <span>Total: ${valueText}</span>
      </div>
      <div style="opacity:0.75;">Avg cost/unit: ${avgText}</div>
    </div>`
}

const config = computed<VueUiXyConfig>(() => {
  const n = periodLabels.value.length
  const manyLabels = n > 14
  const modulo = manyLabels ? Math.ceil(n / 12) : 1
  return {
    responsive: true,
    useCssAnimation: true,
    chart: {
      fontFamily: 'inherit',
      backgroundColor: 'transparent',
      color: textSecondary,
      padding: { top: 24, right: 24, bottom: 56, left: 64 },
      grid: {
        stroke: '#3f3f46',
        showHorizontalLines: true,
        showVerticalLines: false,
        labels: {
          color: textSecondary,
          fontSize: 11,
          axis: {
            yLabel: metric.value === 'gold' ? 'Councils' : 'Units',
            xLabel: '',
            fontSize: 11,
          },
          yAxis: {
            formatter: ({ value }) => formatCompact(value),
          },
          xAxisLabels: {
            color: textSecondary,
            show: true,
            values: periodLabels.value,
            fontSize: 10,
            showOnlyAtModulo: manyLabels,
            modulo,
            rotation: -35,
          },
        },
      },
      legend: {
        show: true,
        color: textPrimary,
        fontSize: 12,
        position: 'top',
      },
      title: { show: false },
      tooltip: {
        show: true,
        backgroundColor: '#27272a',
        color: textPrimary,
        borderColor: '#3f3f46',
        borderWidth: 1,
        borderRadius: 4,
        fontSize: 12,
        showValue: true,
        showPercentage: false,
        roundingValue: 0,
        // Total mode adds an "avg cost/unit" row (Councils ÷ units for the
        // hovered bucket). Other modes keep the default per-series tooltip.
        ...(isTotalMode.value ? { customFormat: buildTotalTooltip } : {}),
      },
      userOptions: { show: false },
    },
    line: {
      strokeWidth: 2,
      radius: 3,
      useArea: false,
      // Default-true gradient lightens each line toward its center, which
      // washes our light pastel strokes out to near-white. Force solid color.
      useGradient: false,
      area: { useGradient: false },
      dot: { useSerieColor: true, strokeWidth: 0 },
      labels: { show: false },
    },
    showTable: false,
  }
})

// ── Reactivity ──────────────────────────────────────────────────────────
// Debounced reload on selection change. Metric is intentionally excluded —
// both gold and units series ship in one payload, so the toggle is instant.
let filterTimer: ReturnType<typeof setTimeout> | null = null
watch([selectedItem, summaryMode, period], () => {
  if (filterTimer) clearTimeout(filterTimer)
  filterTimer = setTimeout(() => void reload(), 200)
})

watch(() => store.dataVersion, () => void reload())

watch(
  () => store.currentOwner,
  () => {
    selectedItem.value = null
    summaryMode.value = 'top5'
    void reload()
  },
)

onMounted(reload)
onBeforeUnmount(() => {
  if (filterTimer) clearTimeout(filterTimer)
})
</script>
