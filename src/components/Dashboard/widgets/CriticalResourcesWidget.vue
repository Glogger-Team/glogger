<template>
  <div class="flex flex-col gap-1">
    <div
      v-for="item in trackedItems"
      :key="item.name"
      class="flex items-center justify-between gap-2 py-1 px-1 rounded text-sm"
      :class="item.count > 0 ? '' : 'opacity-40'">
      <ItemInline :reference="item.name" />
      <span class="font-mono text-accent-gold shrink-0">{{ item.count.toLocaleString() }}</span>
    </div>

    <div v-if="trackedItems.length === 0" class="text-xs text-text-dim italic">
      No tracked items configured.
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import { useGameStateStore } from '../../../stores/gameStateStore'
import { useViewPrefs } from '../../../composables/useViewPrefs'
import { CRITICAL_RESOURCES_DEFAULTS, type CriticalResourcesPrefs } from './criticalResourcesPrefs'
import ItemInline from '../../Shared/Item/ItemInline.vue'

const gameState = useGameStateStore()
const { prefs } = useViewPrefs<CriticalResourcesPrefs>('widget.critical-resources', CRITICAL_RESOURCES_DEFAULTS)

const trackedItems = computed(() =>
  prefs.value.trackedItems.map(name => ({
    name,
    count: gameState.ownedItemCounts[name] ?? 0,
  }))
)
</script>
