<template>
  <div class="flex flex-col gap-3 min-w-64">
    <!-- Currently tracked (scrollable, capped height) -->
    <div class="flex flex-col min-h-0">
      <div class="text-xs font-bold text-text-secondary uppercase tracking-wide mb-1 shrink-0">Tracked Stats</div>
      <div v-if="prefs.trackedStats.length === 0" class="text-xs text-text-dim italic">None</div>
      <div v-else class="overflow-y-auto max-h-32">
        <div v-for="key in prefs.trackedStats" :key="key" class="flex items-center justify-between gap-2 py-0.5">
          <span class="text-xs text-text-primary truncate">{{ labelFor(key) }}</span>
          <button
            class="text-xs text-red-400 hover:text-red-300 cursor-pointer shrink-0"
            @click="removeStat(key)">
            Remove
          </button>
        </div>
      </div>
    </div>

    <!-- Search to add -->
    <div class="shrink-0">
      <div class="text-xs font-bold text-text-secondary uppercase tracking-wide mb-1">Add Stat</div>
      <input
        v-model="search"
        type="text"
        placeholder="Search attributes..."
        class="w-full px-2 py-1 bg-surface-base border border-border-default rounded text-xs text-text-primary placeholder-text-muted focus:outline-none focus:border-accent-gold/50" />
    </div>

    <!-- Search results (always has room for 5 rows) -->
    <div class="overflow-y-auto min-h-30 max-h-40">
      <div
        v-for="option in filteredOptions"
        :key="option.key"
        class="flex items-center justify-between gap-2 py-0.5 cursor-pointer hover:bg-surface-elevated/50 px-1 rounded"
        @click="addStat(option.key)">
        <span class="text-xs text-text-primary truncate">{{ option.label }}</span>
        <span v-if="option.value != null" class="text-xs text-text-dim shrink-0">{{ option.value }}</span>
      </div>
      <div v-if="filteredOptions.length === 0" class="text-xs text-text-dim italic py-1">
        No matching stats found.
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed } from 'vue'
import { useGameStateStore } from '../../../stores/gameStateStore'
import { useViewPrefs } from '../../../composables/useViewPrefs'
import { STAT_TRACKER_DEFAULTS, COMPUTED_STATS, type StatTrackerPrefs } from './statTrackerPrefs'

const store = useGameStateStore()
const { prefs, update } = useViewPrefs<StatTrackerPrefs>('widget.stat-tracker', STAT_TRACKER_DEFAULTS)
const search = ref('')

function formatAttrName(name: string): string {
  return name
    .replace(/_/g, ' ')
    .toLowerCase()
    .replace(/\b\w/g, c => c.toUpperCase())
    .replace(/\bXp\b/g, 'XP')
}

function labelFor(key: string): string {
  if (key.startsWith('computed:')) {
    return COMPUTED_STATS[key]?.label ?? key
  }
  return formatAttrName(key)
}

interface StatOption {
  key: string
  label: string
  value: string | null
}

const allOptions = computed<StatOption[]>(() => {
  const tracked = new Set(prefs.value.trackedStats)
  const options: StatOption[] = []

  // Computed stats first
  for (const [key, def] of Object.entries(COMPUTED_STATS)) {
    if (!tracked.has(key)) {
      options.push({ key, label: def.label, value: null })
    }
  }

  // Raw attributes from game state
  for (const attr of store.attributes) {
    if (!tracked.has(attr.attribute_name)) {
      options.push({
        key: attr.attribute_name,
        label: formatAttrName(attr.attribute_name),
        value: Number.isInteger(attr.value)
          ? attr.value.toLocaleString()
          : attr.value.toLocaleString(undefined, { maximumFractionDigits: 1 }),
      })
    }
  }

  return options
})

const filteredOptions = computed(() => {
  const q = search.value.toLowerCase()
  if (!q) return allOptions.value
  return allOptions.value.filter(o => o.label.toLowerCase().includes(q) || o.key.toLowerCase().includes(q))
})

function addStat(key: string) {
  if (!prefs.value.trackedStats.includes(key)) {
    update({ trackedStats: [...prefs.value.trackedStats, key] })
  }
  search.value = ''
}

function removeStat(key: string) {
  update({ trackedStats: prefs.value.trackedStats.filter(k => k !== key) })
}
</script>
