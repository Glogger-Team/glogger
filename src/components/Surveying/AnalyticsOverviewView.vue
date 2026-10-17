<template>
  <div class="flex flex-col gap-4">
    <!-- Top stat cards -->
    <div class="grid grid-cols-2 xl:grid-cols-4 gap-2">
      <StatCard
        label="Total Surveys"
        :value="analytics.total_uses.toLocaleString()"
        :sub="`${analytics.total_basic_uses.toLocaleString()} basic · ${motherlodeCount} motherlode · ${multihitCount} multihit`"
      />
      <StatCard
        label="Speed Bonus Rate"
        :value="bonusRateDisplay"
        :sub="bonusRateSub"
      />
      <StatCard
        label="Bonus Items"
        :value="analytics.bonus_items_total.toLocaleString()"
        sub="total bonus drops"
      />
      <StatCard
        label="Zones"
        :value="String(analytics.zones.length)"
        :sub="`${analytics.survey_types.length} survey types`"
      />
    </div>

    <!-- Cross-zone comparison -->
    <section
      v-if="analytics.zones.length > 0"
      class="bg-surface-card border border-border-default rounded p-3 flex flex-col gap-2"
    >
      <header class="flex items-baseline justify-between">
        <h3 class="text-xs uppercase tracking-widest text-accent-blue font-semibold">
          Cross-Zone Comparison
        </h3>
        <span class="text-[10px] text-text-dim">click a zone to drill in</span>
      </header>
      <CrossZoneComparison :zones="analytics.zones" @select-zone="emitZone" />
    </section>

    <!-- All survey types -->
    <section class="bg-surface-card border border-border-default rounded p-3 flex flex-col gap-2">
      <header class="flex items-baseline justify-between">
        <h3 class="text-xs uppercase tracking-widest text-accent-blue font-semibold">
          All Survey Types
        </h3>
        <span class="text-[10px] text-text-dim">sorted by most-used · click to drill in</span>
      </header>
      <DataTable
        :columns="columns"
        :rows="(analytics.survey_types as unknown as Record<string, unknown>[])"
        compact
        row-clickable
        @row-click="selectTypeRow">
        <template #cell-map_display_name="{ row }">
          <span class="text-text-primary font-semibold">{{ row.map_display_name }}</span>
        </template>
        <template #cell-area="{ row }">
          <AreaInline v-if="row.area" :reference="row.area as string" />
          <span v-else class="text-text-dim">—</span>
        </template>
        <template #cell-kind="{ row }">
          <span class="text-text-secondary">{{ row.kind }}</span>
        </template>
        <template #cell-total_uses="{ row }">
          <span class="text-text-primary">{{ row.total_uses }}</span>
        </template>
        <template #cell-total_loot_qty="{ row }">
          <span class="text-accent-gold font-semibold">{{ (row.total_loot_qty as number).toLocaleString() }}</span>
        </template>
        <template #cell-avg_loot_per_use="{ row }">
          <span class="text-text-muted">
            {{ row.avg_loot_per_use !== null ? (row.avg_loot_per_use as number).toFixed(1) : '—' }}
          </span>
        </template>
        <template #cell-bonus_rate="{ row }">
          <span :class="typeBonusRateClass(row as unknown as SurveyTypeSummary)">
            {{ typeBonusRate(row as unknown as SurveyTypeSummary) }}
          </span>
        </template>
      </DataTable>
    </section>
  </div>
</template>

<script setup lang="ts">
// Overview view shown when no zone or type is selected. Top metric
// cards + cross-zone comparison + all-survey-types table. Rows in
// both tables are clickable to drill into a zone or type detail view.
import { computed } from 'vue'
import type { SurveyAnalytics, SurveyTypeSummary } from '../../stores/surveyTrackerStore'
import StatCard from './StatCard.vue'
import CrossZoneComparison from './CrossZoneComparison.vue'
import AreaInline from '../Shared/Area/AreaInline.vue'
import DataTable, { type ColumnDef } from '../Shared/DataTable.vue'

const props = defineProps<{
  analytics: SurveyAnalytics
}>()

const emit = defineEmits<{
  'select-zone': [area: string]
  'select-type': [payload: { map: string; area: string | null }]
}>()

function emitZone(area: string) {
  emit('select-zone', area)
}

const columns: ColumnDef[] = [
  { key: 'map_display_name', label: 'Survey Type' },
  { key: 'area', label: 'Zone' },
  { key: 'kind', label: 'Kind' },
  { key: 'total_uses', label: 'Done', align: 'right', numeric: true },
  { key: 'total_loot_qty', label: 'Items', align: 'right', numeric: true },
  { key: 'avg_loot_per_use', label: 'Avg/Use', align: 'right', numeric: true },
  { key: 'bonus_rate', label: 'Bonus Rate', align: 'right', numeric: true },
]

// Row-click drill-in from the all-survey-types table.
function selectTypeRow(payload: { row: Record<string, unknown>; index: number }) {
  const t = payload.row as unknown as SurveyTypeSummary
  emit('select-type', { map: t.map_internal_name, area: t.area })
}

const motherlodeCount = computed(() =>
  props.analytics.zones.reduce((sum, z) => sum + z.motherlode_uses, 0).toLocaleString(),
)
const multihitCount = computed(() =>
  props.analytics.zones.reduce((sum, z) => sum + z.multihit_uses, 0).toLocaleString(),
)

const bonusRateDisplay = computed(() => {
  const a = props.analytics
  if (a.total_basic_uses === 0) return '—'
  return `${((a.basic_uses_with_bonus / a.total_basic_uses) * 100).toFixed(1)}%`
})
const bonusRateSub = computed(() => {
  const a = props.analytics
  if (a.total_basic_uses === 0) return 'no basic surveys yet'
  return `${a.basic_uses_with_bonus} of ${a.total_basic_uses} basic`
})

function typeBonusRate(t: SurveyTypeSummary): string {
  if (t.kind !== 'basic' || t.total_uses === 0) return '—'
  return `${((t.uses_with_bonus / t.total_uses) * 100).toFixed(1)}%`
}
function typeBonusRateClass(t: SurveyTypeSummary): string {
  if (t.kind !== 'basic' || t.total_uses === 0) return 'text-text-dim'
  return 'text-accent-gold'
}
</script>
