<template>
  <section class="flex flex-col gap-3 text-xs">
    <!-- Inputs -->
    <div class="flex flex-col gap-2">
      <div class="flex flex-col gap-1">
        <label class="text-[10px] leading-tight uppercase tracking-wide text-text-secondary font-semibold">
          Item
        </label>
        <SearchableSelect
          :model-value="selectedItem"
          :options="availableItems"
          all-label="Select an item…"
          :full-width="true"
          @update:model-value="onItemChange"
        />
      </div>

      <div class="grid grid-cols-2 gap-2">
        <div class="flex flex-col gap-1">
          <label class="text-[10px] leading-tight uppercase tracking-wide text-text-secondary font-semibold">
            Target Qty
          </label>
          <NumberInput
            v-model="desiredQty"
            :min="1"
            :max="100000"
            :step="1"
            size="sm"
            placeholder="100"
          />
        </div>
        <div class="flex flex-col gap-1">
          <label class="text-[10px] leading-tight uppercase tracking-wide text-text-secondary font-semibold">
            Sell Price
          </label>
          <NumberInput
            v-model="sellPrice"
            :min="0"
            :max="10000000"
            :step="10"
            size="sm"
            placeholder="0g"
          />
        </div>
      </div>

      <div class="flex flex-col gap-1">
        <label class="text-[10px] leading-tight uppercase tracking-wide text-text-secondary font-semibold">
          Sort By
        </label>
        <div class="flex gap-0.5 bg-surface-elevated border border-border-default rounded p-0.5">
          <button
            v-for="opt in sortOptions"
            :key="opt.key"
            class="flex-1 px-2 py-1 text-[10px] rounded transition-colors"
            :class="
              sortMode === opt.key
                ? 'bg-accent-gold/20 text-accent-gold font-semibold'
                : 'text-text-secondary hover:text-text-primary'
            "
            :disabled="opt.key === 'profit' && !hasSellPrice"
            @click="sortMode = opt.key"
          >
            {{ opt.label }}
          </button>
        </div>
      </div>
    </div>

    <!-- States: loading / empty / no-data / results -->
    <div v-if="loading" class="py-2">
      <SkeletonLoader variant="text" :lines="3" />
    </div>

    <div v-else-if="error" class="text-accent-red">{{ error }}</div>

    <div v-else-if="!selectedItem" class="text-text-dim italic">
      Pick an item above to see where to get it fastest, cheapest, or most profitably.
    </div>

    <div v-else-if="(desiredQty ?? 0) <= 0" class="text-text-dim italic">
      Set a target quantity to run the numbers.
    </div>

    <div v-else-if="sortedResults.length === 0" class="text-text-dim italic">
      No survey data found for {{ selectedItem }} yet.
    </div>

    <div v-else class="flex flex-col gap-2">
      <div
        v-for="(r, idx) in sortedResults"
        :key="`${r.map_internal_name}::${r.zone ?? ''}`"
        class="rounded border px-3 py-2 flex flex-col gap-1"
        :class="idx === 0 ? 'border-accent-gold/60 bg-accent-gold/5' : 'border-border-default bg-surface-card'"
      >
        <!-- Row header -->
        <div class="flex items-baseline gap-2">
          <span class="text-text-primary font-semibold">{{ r.survey_type }}</span>
          <span v-if="idx === 0" class="text-[10px] leading-tight uppercase tracking-wider text-accent-gold font-bold">
            Best
          </span>
          <span v-if="r.zone" class="text-[10px] text-text-dim ml-auto">
            <AreaInline :reference="r.zone" />
          </span>
        </div>

        <!-- Numbers grid -->
        <div class="grid grid-cols-[repeat(auto-fit,minmax(64px,1fr))] gap-1 text-[10px] tabular-nums">
          <div>
            <div class="text-[10px] leading-tight uppercase tracking-wider text-text-dim">Yield</div>
            <div class="text-text-primary font-semibold">{{ r.effective_yield.toFixed(1) }}</div>
          </div>
          <div>
            <div class="text-[10px] leading-tight uppercase tracking-wider text-text-dim">Needed</div>
            <div class="text-text-primary font-semibold">{{ r.surveys_needed.toLocaleString() }}</div>
          </div>
          <div>
            <div class="text-[10px] leading-tight uppercase tracking-wider text-text-dim">Cost Each</div>
            <div class="text-text-secondary">{{ formatGold(r.crafting_cost) }}</div>
          </div>
          <div>
            <div class="text-[10px] leading-tight uppercase tracking-wider text-text-dim">Total Cost</div>
            <div :class="idx === 0 && sortMode === 'cost' ? 'text-accent-green font-bold' : 'text-text-primary'">
              {{ formatGold(r.total_cost) }}
            </div>
          </div>
          <div>
            <div class="text-[10px] leading-tight uppercase tracking-wider text-text-dim">Est. Time</div>
            <div :class="timeValueClass(r, idx)">
              {{ r.avg_seconds_per_survey > 0 ? formatTime(r.total_time_seconds) : 'N/A' }}
            </div>
          </div>
          <template v-if="hasSellPrice">
            <div>
              <div class="text-[10px] leading-tight uppercase tracking-wider text-text-dim">Profit</div>
              <div :class="r.profit >= 0 ? 'text-accent-green' : 'text-accent-red'">
                {{ r.profit >= 0 ? '+' : '' }}{{ formatGold(r.profit) }}
              </div>
            </div>
            <div>
              <div class="text-[10px] leading-tight uppercase tracking-wider text-text-dim">Profit/hr</div>
              <div :class="profitPerHourClass(r, idx)">
                {{ r.avg_seconds_per_survey > 0
                  ? (r.profit_per_hour >= 0 ? '+' : '') + formatGold(r.profit_per_hour) + '/hr'
                  : 'N/A' }}
              </div>
            </div>
          </template>
        </div>

        <!-- Yield breakdown footer -->
        <div class="text-[10px] text-text-dim flex flex-wrap gap-x-3">
          <span v-if="r.primary_avg > 0">
            Primary: <span class="text-text-secondary">{{ r.primary_avg.toFixed(1) }}/survey</span>
            ({{ r.primary_times_seen }}/{{ r.total_completions }})
          </span>
          <span v-if="r.bonus_per_completion > 0" class="text-accent-gold">
            Speed Bonus: {{ r.bonus_avg_per_proc.toFixed(1) }}/proc
            <span class="text-text-dim">
              ({{ r.bonus_times_seen }}/{{ r.total_completions }} · {{ r.bonus_per_completion.toFixed(2) }}/survey)
            </span>
          </span>
          <span class="ml-auto">{{ r.total_completions }} completions</span>
        </div>
      </div>

      <p class="text-[10px] text-text-dim italic">
        Yield includes primary loot + expected speed-bonus contribution. Time estimates are averaged
        from ended sessions only — they improve as you complete more runs.
      </p>
    </div>
  </section>
</template>

<script setup lang="ts">
// Item Cost Calculator — matches the legacy feature's role: given an
// item + desired quantity + sell price, rank the survey types by cost,
// time, or profit/hr.
//
// The backend command `survey_tracker_item_cost_analysis` returns the
// raw drop stats per (item, survey type, zone). The math — effective
// yield, surveys needed, total cost, total time, profit, profit/hr —
// is computed here so the user can tweak qty/sell-price without a
// round trip.
import { computed, onMounted, ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import SearchableSelect from '../Shared/SearchableSelect.vue'
import NumberInput from '../Shared/NumberInput.vue'
import AreaInline from '../Shared/Area/AreaInline.vue'
import { formatGold } from '../../composables/useRecipeCost'
import { formatDuration } from '../../composables/useTimestamp'
import type { ItemSourceAnalysis } from '../../stores/surveyTrackerStore'
import SkeletonLoader from '../Shared/SkeletonLoader.vue'

interface CalculatedResult {
  survey_type: string
  map_internal_name: string
  zone: string | null
  crafting_cost: number
  total_completions: number
  effective_yield: number
  surveys_needed: number
  total_cost: number
  total_time_seconds: number
  avg_seconds_per_survey: number
  primary_avg: number
  primary_times_seen: number
  bonus_per_completion: number
  bonus_avg_per_proc: number
  bonus_times_seen: number
  speed_bonus_rate: number
  profit: number
  profit_per_hour: number
}

const loading = ref(false)
const error = ref('')
const allData = ref<ItemSourceAnalysis[]>([])
const selectedItem = ref<string | null>(null)
const desiredQty = ref<number>(100)
const sellPrice = ref<number>(0)
const sortMode = ref<'cost' | 'time' | 'profit'>('cost')

const sortOptions = [
  { key: 'cost' as const, label: 'Cost' },
  { key: 'time' as const, label: 'Time' },
  { key: 'profit' as const, label: 'Profit/hr' },
]

const hasSellPrice = computed(() => sellPrice.value > 0)

onMounted(loadData)

async function loadData() {
  loading.value = true
  error.value = ''
  try {
    allData.value = await invoke<ItemSourceAnalysis[]>('survey_tracker_item_cost_analysis')
  } catch (e) {
    error.value = `Failed to load item analysis: ${e}`
  } finally {
    loading.value = false
  }
}

const availableItems = computed(() => {
  const set = new Set(allData.value.map(d => d.item_name))
  return [...set].sort()
})

function onItemChange(next: string | null) {
  selectedItem.value = next
}

const sortedResults = computed<CalculatedResult[]>(() => {
  if (!selectedItem.value || desiredQty.value <= 0) return []

  const qty = desiredQty.value
  const matching = allData.value.filter(d => d.item_name === selectedItem.value)

  const results: CalculatedResult[] = matching.map(d => {
    const primaryPerSurvey = d.total_completions > 0 ? d.primary_total_qty / d.total_completions : 0
    const bonusPerCompletion = d.total_completions > 0 ? d.bonus_total_qty / d.total_completions : 0
    const effectiveYield = primaryPerSurvey + bonusPerCompletion

    const surveysNeeded = effectiveYield > 0 ? Math.ceil(qty / effectiveYield) : Infinity
    const totalCost = surveysNeeded === Infinity ? 0 : surveysNeeded * d.crafting_cost
    const totalTime = surveysNeeded === Infinity ? 0 : surveysNeeded * d.avg_seconds_per_survey
    const revenue = sellPrice.value * qty
    const profit = surveysNeeded === Infinity ? 0 : revenue - totalCost
    const profitPerHour = totalTime > 0 ? (profit / totalTime) * 3600 : 0

    return {
      survey_type: d.survey_type,
      map_internal_name: d.map_internal_name,
      zone: d.zone,
      crafting_cost: d.crafting_cost,
      total_completions: d.total_completions,
      effective_yield: effectiveYield,
      surveys_needed: surveysNeeded === Infinity ? 0 : surveysNeeded,
      total_cost: totalCost,
      total_time_seconds: totalTime,
      avg_seconds_per_survey: d.avg_seconds_per_survey,
      primary_avg: primaryPerSurvey,
      primary_times_seen: d.primary_times_seen,
      bonus_per_completion: bonusPerCompletion,
      bonus_avg_per_proc: d.bonus_avg_per_proc,
      bonus_times_seen: d.bonus_times_seen,
      speed_bonus_rate: d.speed_bonus_rate,
      profit,
      profit_per_hour: profitPerHour,
    }
  }).filter(r => r.effective_yield > 0)

  if (sortMode.value === 'profit') {
    results.sort((a, b) => {
      // Rows missing time data land at the end.
      if (a.avg_seconds_per_survey <= 0 && b.avg_seconds_per_survey <= 0) return b.profit - a.profit
      if (a.avg_seconds_per_survey <= 0) return 1
      if (b.avg_seconds_per_survey <= 0) return -1
      return b.profit_per_hour - a.profit_per_hour
    })
  } else if (sortMode.value === 'time') {
    results.sort((a, b) => {
      if (a.avg_seconds_per_survey <= 0 && b.avg_seconds_per_survey <= 0) return a.total_cost - b.total_cost
      if (a.avg_seconds_per_survey <= 0) return 1
      if (b.avg_seconds_per_survey <= 0) return -1
      return a.total_time_seconds - b.total_time_seconds
    })
  } else {
    results.sort((a, b) => a.total_cost - b.total_cost)
  }

  return results
})

function timeValueClass(r: CalculatedResult, idx: number): string {
  if (r.avg_seconds_per_survey <= 0) return 'text-text-dim italic'
  if (idx === 0 && sortMode.value === 'time') return 'text-accent-green font-bold'
  return 'text-text-primary'
}

function profitPerHourClass(r: CalculatedResult, idx: number): string {
  if (r.avg_seconds_per_survey <= 0) return 'text-text-dim italic'
  if (idx === 0 && sortMode.value === 'profit') return 'text-accent-green font-bold'
  return r.profit_per_hour >= 0 ? 'text-accent-green' : 'text-accent-red'
}

function formatTime(seconds: number): string {
  if (seconds <= 0) return 'N/A'
  return formatDuration(Math.round(seconds))
}

// Expose a reload so the parent can refresh after a session completes.
defineExpose({ reload: loadData })
</script>
