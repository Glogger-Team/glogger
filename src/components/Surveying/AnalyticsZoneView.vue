<template>
  <div class="flex flex-col gap-4">
    <!-- Zone summary stats -->
    <div class="grid grid-cols-2 xl:grid-cols-4 gap-2">
      <div class="bg-surface-elevated border border-border-default rounded px-3 py-2">
        <div class="text-[10px] uppercase tracking-wider text-text-secondary">Zone</div>
        <div class="text-base text-text-primary font-semibold mt-0.5">
          <AreaInline :reference="zone.area" />
        </div>
      </div>
      <StatCard
        label="Surveys"
        :value="zone.total_uses.toLocaleString()"
        :sub="`${zone.basic_uses} basic · ${zone.motherlode_uses} motherlode · ${zone.multihit_uses} multihit`"
      />
      <StatCard
        label="Bonus Rate"
        :value="bonusRate"
        :sub="bonusRateSub"
      />
      <StatCard
        label="Bonus Items"
        :value="hasBonus ? zone.bonus_items_total.toLocaleString() : '—'"
        :sub="hasBonus ? 'total bonus drops' : 'bonus data unavailable'"
      />
    </div>

    <!-- survey-bonus-dark: unstamped aggregates read zero; show unavailable, never a measured zero. -->
    <p v-if="!hasBonus" class="text-text-dim text-xs italic px-1">
      Speed bonus applies to back-to-back Basic uses only.
    </p>

    <!-- Survey types in this zone — one card each -->
    <div v-if="typesInZone.length > 0" class="grid grid-cols-1 xl:grid-cols-2 2xl:grid-cols-3 gap-3">
      <ZoneRewardsCard
        v-for="t in typesInZone"
        :key="`${t.map_internal_name}::${t.area ?? ''}`"
        :map-display-name="t.map_display_name"
        :category="kindToCategory(t)"
        :total-completions="t.total_uses"
        :crafting-cost="0"
        :items="t.items"
      />
    </div>

    <!-- Zone-wide items rollup -->
    <section class="bg-surface-card border border-border-default rounded p-3 flex flex-col gap-2">
      <header class="flex items-baseline justify-between">
        <h3 class="text-xs uppercase tracking-widest text-accent-blue font-semibold">
          All Items from <AreaInline :reference="zone.area" />
        </h3>
        <span class="text-[10px] text-text-dim">{{ zone.items.length }} unique</span>
      </header>
      <ZoneRewardsCard
        map-display-name=""
        :total-completions="zone.total_uses"
        :items="zone.items"
      />
    </section>
  </div>
</template>

<script setup lang="ts">
// Zone detail view — summary stats at the top, one ZoneRewardsCard per
// survey type that's been used in this zone, plus a zone-wide rollup.
// Data comes from the existing analytics command; no extra queries.
import { computed } from 'vue'
import type { SurveyAnalytics, SurveyTypeSummary, ZoneSummary } from '../../stores/surveyTrackerStore'
import StatCard from './StatCard.vue'
import ZoneRewardsCard from './ZoneRewardsCard.vue'
import AreaInline from '../Shared/Area/AreaInline.vue'

const props = defineProps<{
  zone: ZoneSummary
  analytics: SurveyAnalytics
}>()

const typesInZone = computed(() =>
  props.analytics.survey_types.filter(t => (t.area ?? '(unknown)') === props.zone.area),
)

// survey-bonus-dark: mirrors the session-detail unavailable pattern —
// aggregates without bonus stamps show unavailable, never a measured zero.
const hasBonus = computed(() => props.zone.basic_uses_with_bonus > 0)

const bonusRate = computed(() => {
  if (props.zone.basic_uses === 0 || props.zone.basic_uses_with_bonus === 0) return '—'
  return `${((props.zone.basic_uses_with_bonus / props.zone.basic_uses) * 100).toFixed(1)}%`
})
const bonusRateSub = computed(() => {
  if (props.zone.basic_uses === 0) return 'no basic surveys'
  if (props.zone.basic_uses_with_bonus === 0) return 'bonus data unavailable'
  return `${props.zone.basic_uses_with_bonus} of ${props.zone.basic_uses} basic`
})

function kindToCategory(t: SurveyTypeSummary): string {
  // Mineral vs mining lives in the survey_types CDN table but isn't on
  // the analytics payload; infer from the internal-name prefix the
  // same way the backend's item_cost_analysis_rows does.
  return t.map_internal_name.startsWith('MiningSurvey') ? 'mining' : 'mineral'
}
</script>
