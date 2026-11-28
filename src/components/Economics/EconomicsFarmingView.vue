<template>
  <div class="flex flex-col gap-4 h-full">
    <TabBar v-model="activeTab" :tabs="tabs" />
    <div class="flex-1 min-h-0">
    <TimeboxedTab v-if="visited.has('timeboxed')" v-show="activeTab === 'timeboxed'" />
    <DatabaseTab v-if="visited.has('database')" v-show="activeTab === 'database'" />
    <CouncilWealthView v-if="visited.has('wealth')" v-show="activeTab === 'wealth'" />
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed, reactive, watch } from 'vue'
import TabBar from '../Shared/TabBar.vue'
import TimeboxedTab from '../Farming/TimeboxedTab.vue'
import DatabaseTab from '../Farming/DatabaseTab.vue'
import CouncilWealthView from './CouncilWealthView.vue'
import { useViewPrefs } from '../../composables/useViewPrefs'

const tabs = [
  { id: 'timeboxed', label: 'Activity' },
  { id: 'database', label: 'Database' },
  { id: 'wealth', label: 'Wealth' },
]

// HistoricalTab.vue is intentionally left on disk (may be imported elsewhere);
// any stored 'historical' value (or the older 'session'/'sources' ids) lands on timeboxed.
const { prefs, update } = useViewPrefs('economics.farming', { activeTab: 'timeboxed' })
const activeTab = computed({
  get: () => (prefs.value.activeTab === 'timeboxed' || prefs.value.activeTab === 'database' || prefs.value.activeTab === 'wealth' ? prefs.value.activeTab : 'timeboxed'),
  set: (val: string) => update({ activeTab: val }),
})

// Keep-alive set (App.vue precedent): first visit mounts, later visits reuse.
const visited = reactive(new Set<string>([activeTab.value]))
watch(activeTab, (tab) => { visited.add(tab) })
</script>
