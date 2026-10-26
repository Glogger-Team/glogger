<template>
  <div class="flex flex-col gap-1.5 text-sm">
    <div v-if="displayStats.length === 0" class="text-xs text-text-dim italic">
      No stats tracked. Use the gear icon to add stats.
    </div>
    <div
      v-for="stat in displayStats"
      :key="stat.key"
      class="flex items-center justify-between gap-2 py-0.5">
      <span class="text-xs text-text-primary truncate">{{ stat.label }}</span>
      <span class="text-xs text-accent-gold shrink-0">{{ stat.display }}</span>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import { useGameStateStore } from '../../../stores/gameStateStore'
import { useViewPrefs } from '../../../composables/useViewPrefs'
import { STAT_TRACKER_DEFAULTS, COMPUTED_STATS, type StatTrackerPrefs } from './statTrackerPrefs'

const store = useGameStateStore()
const { prefs } = useViewPrefs<StatTrackerPrefs>('widget.stat-tracker', STAT_TRACKER_DEFAULTS)

function formatAttrName(name: string): string {
  return name
    .replace(/_/g, ' ')
    .toLowerCase()
    .replace(/\b\w/g, c => c.toUpperCase())
    .replace(/\bXp\b/g, 'XP')
}

function formatValue(value: number): string {
  if (Number.isInteger(value)) return value.toLocaleString()
  // Percentage-like mods (e.g. 1.15 → +15%)
  if (value > 0 && value < 100 && !Number.isInteger(value)) {
    return value.toLocaleString(undefined, { maximumFractionDigits: 2 })
  }
  return value.toLocaleString(undefined, { maximumFractionDigits: 1 })
}

const displayStats = computed(() => {
  return prefs.value.trackedStats.map(key => {
    if (key.startsWith('computed:')) {
      const def = COMPUTED_STATS[key]
      const label = def?.label ?? key
      let value = 0
      if (key === 'computed:total_skill_levels') {
        value = store.skills.reduce((sum, s) => sum + s.level + s.bonus_levels, 0)
      } else if (key === 'computed:skill_count') {
        value = store.skills.length
      }
      return { key, label, display: formatValue(value) }
    }
    // Raw attribute
    const attrValue = store.attributesByName[key]
    return {
      key,
      label: formatAttrName(key),
      display: attrValue != null ? formatValue(attrValue) : '—',
    }
  })
})
</script>
