<template>
  <div class="flex flex-col gap-2">
    <div class="text-xs font-bold text-text-secondary uppercase tracking-wide mb-1">Show Sections</div>
    <label v-for="opt in sectionOptions" :key="opt.key" class="flex items-center gap-2 cursor-pointer">
      <input
        type="checkbox"
        :checked="!!prefs[opt.key]"
        :disabled="opt.parent ? !prefs[opt.parent] : false"
        class="accent-accent-gold"
        @change="update({ [opt.key]: ($event.target as HTMLInputElement).checked })">
      <span :class="{ 'text-text-dim': opt.parent && !prefs[opt.parent] }">{{ opt.label }}</span>
    </label>

    <!-- Currency toggles -->
    <div class="text-xs font-bold text-text-secondary uppercase tracking-wide mt-2 mb-1">Currencies</div>
    <label v-for="cur in currencyOptions" :key="cur.key" class="flex items-center gap-2 cursor-pointer">
      <input
        type="checkbox"
        :checked="prefs[cur.key] !== false"
        class="accent-accent-gold"
        @change="update({ [cur.key]: ($event.target as HTMLInputElement).checked })">
      <span>{{ cur.label }}</span>
    </label>
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import { useViewPrefs } from '../../composables/useViewPrefs'
import { useGameStateStore } from '../../stores/gameStateStore'
import {
  CONTEXT_BAR_DEFAULTS,
  CURRENCY_DISPLAY_ORDER,
  formatCurrencyDisplayName,
  type ContextBarPrefs,
} from './contextBarPrefs'

const store = useGameStateStore()

const { prefs, update } = useViewPrefs<ContextBarPrefs>('widget.context-bar', CONTEXT_BAR_DEFAULTS)

type PrefKey = keyof ContextBarPrefs

const sectionOptions: { key: PrefKey; label: string; parent?: PrefKey }[] = [
  { key: 'showCharacter', label: 'Character Name' },
  { key: 'showServer', label: 'Server Name' },
  { key: 'showZone', label: 'Current Zone' },
  { key: 'showGameTime', label: 'Game Time' },
  { key: 'showServerTime', label: 'Server Time' },
  { key: 'showLocalTime', label: 'Local Time' },
  { key: 'use24h', label: '24-hour format' },
  { key: 'showMoon', label: 'Moon Phase' },
  { key: 'showWeather', label: 'Weather' },
]

function formatCurrencyLabel(name: string): string {
  return formatCurrencyDisplayName(name)
}

/** Build currency toggle list from known currencies + any the character actually has. */
const currencyOptions = computed(() => {
  // Start with known currencies in display order
  const seen = new Set<string>()
  const options: { key: string; label: string }[] = []

  for (const name of CURRENCY_DISPLAY_ORDER) {
    seen.add(name)
    options.push({
      key: `currency_${name}`,
      label: formatCurrencyLabel(name),
    })
  }

  // Add any currencies the character has that aren't in the known list
  for (const c of store.currencies) {
    const lower = c.currency_name.toLowerCase()
    if (!seen.has(lower)) {
      seen.add(lower)
      options.push({
        key: `currency_${lower}`,
        label: formatCurrencyLabel(c.currency_name),
      })
    }
  }

  return options
})
</script>
