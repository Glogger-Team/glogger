<template>
  <div class="flex flex-col gap-4 h-full">
    <TabBar v-model="activeTab" :tabs="tabs" />
    <div class="flex-1 min-h-0">
      <div v-if="activeTab === 'session'" class="flex flex-col gap-4 h-full">
        <FarmingSessionCard />
      </div>
      <HistoricalTab v-else-if="activeTab === 'historical'" />
      <DatabaseTab v-else-if="activeTab === 'database'" />
      <UnknownSourceTab v-else-if="activeTab === 'sources'" />
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import TabBar from '../Shared/TabBar.vue'
import FarmingSessionCard from '../Farming/FarmingSessionCard.vue'
import HistoricalTab from '../Farming/HistoricalTab.vue'
import DatabaseTab from '../Farming/DatabaseTab.vue'
import UnknownSourceTab from '../Farming/UnknownSourceTab.vue'
import { useViewPrefs } from '../../composables/useViewPrefs'

const tabs = [
  { id: 'session', label: 'Active Session' },
  { id: 'historical', label: 'Session History' },
  { id: 'database', label: 'Database' },
  { id: 'sources', label: 'Sources' },
]

const { prefs, update } = useViewPrefs('economics.farming', { activeTab: 'session' })
const activeTab = computed({
  get: () => prefs.value.activeTab,
  set: (val: string) => update({ activeTab: val }),
})
</script>
