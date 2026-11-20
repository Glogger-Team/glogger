<template>
  <div class="flex flex-col gap-4 h-full">
    <TabBar v-model="activeTab" :tabs="tabs" />
    <div class="flex-1 min-h-0">
      <HistoricalTab v-if="activeTab === 'historical'" />
      <DatabaseTab v-else-if="activeTab === 'database'" />
      <UnknownSourceTab v-else-if="activeTab === 'sources'" />
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import TabBar from '../Shared/TabBar.vue'
import HistoricalTab from '../Farming/HistoricalTab.vue'
import DatabaseTab from '../Farming/DatabaseTab.vue'
import UnknownSourceTab from '../Farming/UnknownSourceTab.vue'
import { useViewPrefs } from '../../composables/useViewPrefs'

const tabs = [
  { id: 'historical', label: 'Session History' },
  { id: 'database', label: 'Database' },
  { id: 'sources', label: 'Unattributed' },
]

const { prefs, update } = useViewPrefs('economics.farming', { activeTab: 'historical' })
const activeTab = computed({
  get: () => (prefs.value.activeTab === 'session' ? 'historical' : prefs.value.activeTab),
  set: (val: string) => update({ activeTab: val }),
})
</script>
