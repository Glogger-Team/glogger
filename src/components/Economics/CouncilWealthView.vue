<template>
  <div class="flex flex-col gap-4 h-full min-h-0 overflow-y-auto">
    <div class="text-[11px] text-text-dim italic">Council balance over time — your last export anchors the level, per-event chat deltas draw the line. Same windows as Activity.</div>
    <TimeRangeSelector ref="selectorRef" @change="onRangeChange" />

    <div v-if="loading" class="text-xs text-text-dim italic">Reading the window…</div>
    <div v-else-if="error" class="text-[#c87e7e] text-sm">{{ error }}</div>
    <template v-else-if="history">
      <div v-if="!history.has_anchor">
        <EmptyState variant="panel"
          primary="No export yet — import to anchor"
          secondary="Council history pins its level to your last character export. Import one, then new wallet changes will draw the line." />
      </div>
      <template v-else>
        <div class="flex gap-6 flex-wrap text-center">
          <div>
            <div class="text-[0.65rem] text-text-muted uppercase tracking-wide">Opening</div>
            <div class="text-lg font-bold text-text-primary">{{ history.opening_balance.toLocaleString() }}</div>
          </div>
          <div>
            <div class="text-[0.65rem] text-text-muted uppercase tracking-wide">Closing</div>
            <div class="text-lg font-bold text-text-primary">{{ closingBalance.toLocaleString() }}</div>
          </div>
          <div>
            <div class="text-[0.65rem] text-text-muted uppercase tracking-wide">Net</div>
            <div class="text-lg font-bold" :class="history.window_net >= 0 ? 'text-[#7ec87e]' : 'text-[#c87e7e]'">{{ signedNet }}</div>
          </div>
          <div>
            <div class="text-[0.65rem] text-text-muted uppercase tracking-wide">Events</div>
            <div class="text-lg font-bold text-text-primary">{{ history.points.length }}</div>
          </div>
        </div>

        <EmptyState v-if="history.points.length === 0" variant="panel"
          primary="Nothing in this window"
          secondary="No council gains or spends were recorded between these timestamps." />

        <div v-else class="flex-1 min-h-0 border border-border-default rounded p-2">
          <div v-if="isBucketed" class="text-[11px] text-text-dim italic px-1 pb-1">Line bucketed to {{ displayPoints.length }} of {{ history.points.length }} events — totals exact.</div>
          <VueUiXy
            :key="chartKey"
            :dataset="dataset"
            :config="config" />
        </div>
      </template>
    </template>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, watch, onMounted, onBeforeUnmount } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { VueUiXy } from 'vue-data-ui'
import type { VueUiXyConfig, VueUiXyDatasetItem } from 'vue-data-ui'
import TimeRangeSelector, { type TimeRange } from '../Shared/TimeRangeSelector.vue'
import EmptyState from '../Shared/EmptyState.vue'
import { useSettingsStore } from '../../stores/settingsStore'
import { isReplayChatStatusEvent, type ChatStatusEvent, type ReplayChatStatusEnvelope } from '../../stores/gameStateStore'
import { chartPalette as PALETTE, themeText, themeTooltipBg, themeTooltipBorder } from '../../composables/useChartTheme'
import type { CouncilHistory } from '../../types/gameState.specta'

// Inactive (v-show-hidden) instances skip listener-triggered reloads and
// reload once on flip back to active, so the Economics + Farming copies do
// not double every live council query.
const props = defineProps<{ active?: boolean }>()
const isActive = computed(() => props.active !== false)
let inactiveDirty = false

const history = ref<CouncilHistory | null>(null)
const loading = ref(false)
const error = ref<string | null>(null)
const loadedRange = ref<TimeRange | null>(null)
const loadedIdentity = ref<{ character: string; server: string } | null>(null)

const textPrimary = themeText('--color-text-primary', '#d4d4d4')
const textSecondary = themeText('--color-text-secondary', '#a0a0a0')
const tooltipBg = themeTooltipBg()
const tooltipBorder = themeTooltipBorder()

function identityOf() {
  const settings = useSettingsStore()
  const character = settings.settings.activeCharacterName
  const server = settings.settings.activeServerName
  if (!character || !server) return null
  return { character, server }
}

const closingBalance = computed(() => {
  const h = history.value
  if (!h || h.points.length === 0) return h?.opening_balance ?? 0
  return h.points[h.points.length - 1].balance
})

const signedNet = computed(() => {
  const net = history.value?.window_net ?? 0
  return `${net >= 0 ? '+' : ''}${net.toLocaleString()}`
})

function shortTs(ts: string): string {
  return ts.length >= 16 ? ts.slice(5, 16) : ts
}

// All-time windows can hold thousands of ledger events: bucket time-adjacent
// deltas down to a bounded point count so the chart render stays cheap. The
// SQL scan itself is a plain indexed range read; the render was the unbounded
// cost. Buckets preserve the exact closing balance (cumulative from the
// pinned opening), only intermediate line vertices are merged.
const MAX_CHART_POINTS = 500
const displayPoints = computed(() => {
  const h = history.value
  if (!h) return [] as Array<{ event_ts: string; balance: number }>
  if (h.points.length <= MAX_CHART_POINTS) {
    return h.points.map((p) => ({ event_ts: p.event_ts, balance: p.balance }))
  }
  const size = Math.ceil(h.points.length / MAX_CHART_POINTS)
  const out: Array<{ event_ts: string; balance: number }> = []
  let balance = h.opening_balance
  for (let i = 0; i < h.points.length; i += size) {
    const bucket = h.points.slice(i, i + size)
    // Re-anchor cumulatively: bucket deltas sum onto the running balance so
    // the last vertex still equals the true closing balance.
    for (const p of bucket) balance += p.delta
    out.push({ event_ts: bucket[bucket.length - 1].event_ts, balance })
  }
  return out
})
const isBucketed = computed(() => {
  const h = history.value
  return !!h && h.points.length > displayPoints.value.length
})

// The pinned level at the window start leads the line: without it the chart
// would start mid-air at the first event instead of at the anchored level.
const chartLabels = computed(() => {
  const h = history.value
  if (!h) return []
  return [shortTs(loadedRange.value?.start ?? ''), ...displayPoints.value.map((p) => shortTs(p.event_ts))]
})

const chartSeries = computed(() => {
  const h = history.value
  if (!h) return []
  return [h.opening_balance, ...displayPoints.value.map((p) => p.balance)]
})
const dataset = computed<VueUiXyDatasetItem[]>(() => [
  {
    name: 'Councils',
    series: chartSeries.value,
    type: 'line',
    color: PALETTE[4 % PALETTE.length],
    smooth: false,
    useArea: false,
    dataLabels: false,
  },
])

const chartKey = computed(
  () => `${loadedIdentity.value?.character ?? ''}@${loadedIdentity.value?.server ?? ''}-${loadedRange.value?.start ?? ''}-${loadedRange.value?.end ?? ''}-${displayPoints.value.length}`,
)

function formatCompact(n: number): string {
  const abs = Math.abs(n)
  if (abs >= 1_000_000) return `${(n / 1_000_000).toFixed(1).replace(/\.0$/, '')}M`
  if (abs >= 1_000) return `${(n / 1_000).toFixed(1).replace(/\.0$/, '')}k`
  return `${Math.round(n)}`
}

const config = computed<VueUiXyConfig>(() => {
  const labels = chartLabels.value
  const manyLabels = labels.length > 14
  const modulo = manyLabels ? Math.ceil(labels.length / 12) : 1
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
            yLabel: 'Councils',
            xLabel: '',
            fontSize: 11,
          },
          yAxis: {
            formatter: ({ value }) => formatCompact(value),
          },
          xAxisLabels: {
            color: textSecondary,
            show: true,
            values: labels,
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
        backgroundColor: tooltipBg,
        color: textPrimary,
        borderColor: tooltipBorder,
        borderWidth: 1,
        borderRadius: 4,
        fontSize: 12,
        showValue: true,
        showPercentage: false,
        roundingValue: 0,
      },
      userOptions: { show: false },
    },
    line: {
      strokeWidth: 2,
      radius: 3,
      useArea: false,
      useGradient: false,
      area: { useGradient: false },
      dot: { useSerieColor: true, strokeWidth: 0 },
      labels: { show: false },
    },
    showTable: false,
  }
})

// Monotonic reload sequence: a character/range switch mid-fetch must not let
// the older response plant (cross-character stale flash) or clear `loading`
// while the newer fetch is still in flight. Only the latest sequence may
// plant history, report errors, or leave the loading state.
let reloadSeq = 0

async function reload() {
  const id = identityOf()
  const range = loadedRange.value
  const seq = ++reloadSeq
  if (!id || !range || !range.start || !range.end) {
    history.value = null
    loading.value = false
    return
  }
  loading.value = true
  error.value = null
  try {
    const h = await invoke<CouncilHistory>('get_council_history', {
      characterName: id.character,
      serverName: id.server,
      startTs: range.start,
      endTs: range.end,
    })
    if (seq !== reloadSeq) return
    // Rapid range/character switches: drop a stale response that no longer
    // matches (same guard shape as the arena tips fetch).
    const now = identityOf()
    if (loadedRange.value !== range) return
    if (!now || now.character !== id.character || now.server !== id.server) return
    loadedIdentity.value = id
    history.value = h
  } catch (e: unknown) {
    if (seq !== reloadSeq) return
    error.value = String(e)
  } finally {
    if (seq === reloadSeq) loading.value = false
  }
}

function onRangeChange(range: TimeRange) {
  loadedRange.value = range
  if (!range.start || !range.end) return
  void reload()
}

const selectorRef = ref<InstanceType<typeof TimeRangeSelector> | null>(null)

function requestReload() {
  if (!isActive.value) {
    inactiveDirty = true
    return
  }
  // Preset-aware: re-apply the active preset with a fresh `now` so the window
  // end extends past mount time; bare reload() would re-query the stale
  // stored range and miss post-mount events.
  selectorRef.value?.refresh()
}

let unlisten: UnlistenFn | null = null
let unlistenStatus: UnlistenFn | null = null

watch(
  () => {
    const id = identityOf()
    return id ? `${id.character}@${id.server}` : ''
  },
  () => {
    // Clear synchronously: the previous character's line must not render
    // under the new identity while the fresh fetch is in flight.
    history.value = null
    requestReload()
  },
)

watch(isActive, (active) => {
  if (active && inactiveDirty) {
    inactiveDirty = false
    selectorRef.value?.refresh()
  }
})

onMounted(async () => {
  unlisten = await listen<string[]>('game-state-updated', (e) => {
    if (e.payload.includes('currency_estimate')) requestReload()
  })
  unlistenStatus = await listen<ChatStatusEvent | ReplayChatStatusEnvelope>('chat-status-event', (e) => {
    // Replay preview re-emits on the live channel tagged `is_replay` — never
    // reload from a replay (HoplologyWidget precedent: the envelope carries
    // no top-level `kind`, so the filter below would fall through anyway).
    if (isReplayChatStatusEvent(e.payload)) return
    if (e.payload.kind === 'CouncilsChanged' || e.payload.kind === 'CoinsLooted') requestReload()
  })
})

onBeforeUnmount(() => {
  unlisten?.()
  unlistenStatus?.()
})
</script>
