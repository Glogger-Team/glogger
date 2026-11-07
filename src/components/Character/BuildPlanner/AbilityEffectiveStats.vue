<template>
  <!-- Still resolving -->
  <div v-if="stats === undefined" class="mt-2 pt-2 border-t border-border-default/60 text-[10px] text-text-dim italic">
    Calculating effective stats…
  </div>

  <!-- Resolved, has combat data -->
  <div v-else-if="stats && hasAnyValue" class="mt-2 pt-2 border-t border-border-default/60 space-y-1.5">
    <!-- Computed combat values -->
    <div class="space-y-1">
      <!-- Direct damage (normal / health-specific / armor-specific) -->
      <div
        v-for="(dd, i) in stats.direct_damages"
        :key="`dd-${i}`"
        class="flex items-baseline justify-between gap-3 text-xs">
        <span class="text-text-muted">{{ directDamageLabel(dd.kind) }}</span>
        <span class="flex items-baseline gap-1.5 shrink-0">
          <span class="font-semibold" :class="valueClass(dd.value)">{{ fmt(dd.value.effective) }}</span>
          <span v-if="isMod(dd.value)" class="text-[10px] text-text-dim">was {{ fmt(dd.value.base) }}</span>
          <span v-if="pct(dd.value)" class="text-[10px] font-semibold text-value-positive">{{ pct(dd.value) }}</span>
        </span>
      </div>

      <!-- DoTs -->
      <div
        v-for="(dot, i) in stats.dots"
        :key="`dot-${i}`"
        class="flex items-baseline justify-between gap-3 text-xs">
        <span class="text-text-muted">{{ dotLabel(dot) }}</span>
        <span class="flex items-baseline gap-1.5 shrink-0">
          <span class="font-semibold" :class="valueClass(dot.per_tick)">{{ fmt(dot.total_effective) }}</span>
          <span class="text-[10px] text-text-dim">{{ fmt(dot.per_tick.effective) }}/tick ×{{ dot.num_ticks }}</span>
          <span v-if="isMod(dot.per_tick)" class="text-[10px] text-text-dim">was {{ fmt(dot.total_base) }}</span>
        </span>
      </div>

      <!-- Special values (heals / restores / etc.) -->
      <div
        v-for="(sv, i) in visibleSpecialValues"
        :key="`sv-${i}`"
        class="flex items-baseline justify-between gap-3 text-xs">
        <span class="text-text-muted">
          {{ sv.label || 'Effect' }}<span v-if="sv.suffix" class="text-text-dim"> {{ sv.suffix }}</span>
        </span>
        <span class="flex items-baseline gap-1.5 shrink-0">
          <span class="font-semibold" :class="valueClass(sv.value)">{{ fmt(sv.value.effective) }}</span>
          <span v-if="sv.value.dormant_activated" class="text-[10px] leading-tight uppercase font-semibold text-accent-gold">active</span>
          <span v-else-if="isMod(sv.value)" class="text-[10px] text-text-dim">was {{ fmt(sv.value.base) }}</span>
        </span>
      </div>

      <!-- Modified costs only (base costs already shown above by AbilityTooltip) -->
      <div
        v-for="cost in modifiedCosts"
        :key="cost.key"
        class="flex items-baseline justify-between gap-3 text-xs">
        <span class="text-text-muted">{{ cost.label }}</span>
        <span class="flex items-baseline gap-1.5 shrink-0">
          <span class="font-semibold" :class="cost.value.effective < cost.value.base ? 'text-value-positive' : 'text-text-primary'">{{ fmt(cost.value.effective) }}</span>
          <span class="text-[10px] text-text-dim">was {{ fmt(cost.value.base) }}</span>
        </span>
      </div>
    </div>

    <!-- Per-mod breakdown -->
    <div v-if="contributions.length" class="pt-1.5 border-t border-border-default/40 space-y-0.5">
      <div class="text-[10px] font-semibold uppercase tracking-wide text-text-muted">From your gear</div>
      <div v-for="(c, i) in contributions" :key="i" class="flex items-center gap-2">
        <EffectLine
          :label="c.label"
          :formatted-value="formatStatValue(c.value, c.display_type)"
          :numeric-value="c.value"
          class="flex-1" />
        <span class="text-[10px] text-text-dim shrink-0 truncate max-w-[55%]">{{ c.source }}</span>
      </div>
    </div>

    <!-- Honesty footnote -->
    <div class="text-[10px] text-text-dim italic leading-snug">
      Folds in equipped gear mods only — excludes skill-level passives and active buffs.
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import EffectLine from './EffectLine.vue'
import { formatStatValue } from '../../../composables/useBuildStats'
import { isValueModified } from '../../../types/abilityStats'
import type {
  AbilityBuildStats,
  ContributionLine,
  DotBreakdown,
  ValueBreakdown,
} from '../../../types/abilityStats'

const props = defineProps<{
  stats: AbilityBuildStats | null | undefined
}>()

function fmt(n: number): string {
  // Integers (with thousands separators) for sizeable values; one decimal for small ones.
  const r = Math.abs(n) >= 10 ? Math.round(n) : Math.round(n * 10) / 10
  return r.toLocaleString()
}

function isMod(v: ValueBreakdown): boolean {
  return isValueModified(v)
}

function valueClass(v: ValueBreakdown): string {
  return isMod(v) ? 'text-value-positive' : 'text-text-primary'
}

/** Percent increase label (e.g. "+95%") for a modified value with a non-zero base. */
function pct(v: ValueBreakdown): string {
  if (!isMod(v) || v.base <= 0) return ''
  const p = Math.round((v.effective / v.base - 1) * 100)
  return p > 0 ? `+${p}%` : `${p}%`
}

/** Label for a direct-damage component, qualified by its kind. The base "{type} Damage" gets a
 *  "to Health" / "to Armor" suffix for the armor-bypassing and armor-only hits. */
function directDamageLabel(kind: string): string {
  const t = props.stats?.damage_type
  const base = t ? `${t} Damage` : 'Damage'
  if (kind === 'health') return `${base} to Health`
  if (kind === 'armor') return `${base} to Armor`
  return base
}

function dotLabel(dot: DotBreakdown): string {
  const type = dot.damage_type ? `${dot.damage_type} ` : ''
  const dur = dot.duration ? ` over ${dot.duration}s` : ''
  return `${type}DoT${dur}`
}

/** Special values the game would actually display: hide lines that are still zero
 *  (dormant SkipIfZero effects no mod activated). */
const visibleSpecialValues = computed(() =>
  (props.stats?.special_values ?? []).filter((sv) => sv.value.effective !== 0 || sv.value.base !== 0),
)

const modifiedCosts = computed(() => {
  const out: { key: string; label: string; value: ValueBreakdown }[] = []
  if (props.stats?.power_cost && isMod(props.stats.power_cost)) {
    out.push({ key: 'power', label: 'Power Cost', value: props.stats.power_cost })
  }
  if (props.stats?.rage_cost && isMod(props.stats.rage_cost)) {
    out.push({ key: 'rage', label: 'Rage Cost', value: props.stats.rage_cost })
  }
  return out
})

const hasAnyValue = computed(() => {
  const s = props.stats
  if (!s) return false
  return s.direct_damages.length > 0 || s.dots.length > 0 || visibleSpecialValues.value.length > 0 || modifiedCosts.value.length > 0
})

/** All unique contributing mod lines across every computed value, for the breakdown. */
const contributions = computed((): ContributionLine[] => {
  const s = props.stats
  if (!s) return []
  const buckets: ValueBreakdown[] = []
  for (const dd of s.direct_damages) buckets.push(dd.value)
  for (const d of s.dots) buckets.push(d.per_tick)
  for (const sv of s.special_values) buckets.push(sv.value)
  if (s.power_cost) buckets.push(s.power_cost)
  if (s.rage_cost) buckets.push(s.rage_cost)

  const seen = new Map<string, ContributionLine>()
  for (const b of buckets) {
    for (const c of b.contributions) {
      seen.set(`${c.source}|${c.label}|${c.value}|${c.bucket}`, c)
    }
  }
  return [...seen.values()]
})
</script>
