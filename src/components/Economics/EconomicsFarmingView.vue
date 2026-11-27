<template>
  <div class="flex flex-col gap-4 h-full">
    <TabBar v-model="activeTab" :tabs="tabs" />
    <div class="flex-1 min-h-0">
    <HistoricalTab v-if="activeTab === 'historical'" />
    <TimeboxedTab v-else-if="activeTab === 'timeboxed'" />
    <DatabaseTab v-else-if="activeTab === 'database'" />
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import TabBar from '../Shared/TabBar.vue'
import HistoricalTab from '../Farming/HistoricalTab.vue'
import TimeboxedTab from '../Farming/TimeboxedTab.vue'
import DatabaseTab from '../Farming/DatabaseTab.vue'
import { useViewPrefs } from '../../composables/useViewPrefs'

const tabs = [
  { id: 'historical', label: 'Session History' },
  { id: 'timeboxed', label: 'Time-boxed' },
  { id: 'database', label: 'Database' },
]

const { prefs, update } = useViewPrefs('economics.farming', { activeTab: 'historical' })
const activeTab = computed({
  get: () => (prefs.value.activeTab === 'session' || prefs.value.activeTab === 'sources' ? 'historical' : prefs.value.activeTab),
  set: (val: string) => update({ activeTab: val }),
})
</script>
