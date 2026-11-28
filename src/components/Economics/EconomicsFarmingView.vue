<template>
  <div class="flex flex-col gap-4 h-full">
    <TabBar v-model="activeTab" :tabs="tabs" />
    <div class="flex-1 min-h-0">
    <TimeboxedTab v-if="activeTab === 'timeboxed'" />
    <DatabaseTab v-else-if="activeTab === 'database'" />
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import TabBar from '../Shared/TabBar.vue'
import TimeboxedTab from '../Farming/TimeboxedTab.vue'
import DatabaseTab from '../Farming/DatabaseTab.vue'
import { useViewPrefs } from '../../composables/useViewPrefs'

const tabs = [
  { id: 'timeboxed', label: 'Time-boxed' },
  { id: 'database', label: 'Database' },
]

// HistoricalTab.vue is intentionally left on disk (may be imported elsewhere);
// any stored 'historical' value (or the older 'session'/'sources' ids) lands on timeboxed.
const { prefs, update } = useViewPrefs('economics.farming', { activeTab: 'timeboxed' })
const activeTab = computed({
  get: () => (prefs.value.activeTab === 'timeboxed' || prefs.value.activeTab === 'database' ? prefs.value.activeTab : 'timeboxed'),
  set: (val: string) => update({ activeTab: val }),
})
</script>
