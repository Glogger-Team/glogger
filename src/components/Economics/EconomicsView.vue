<template>
  <PaneLayout screen-key="economics">
    <div class="flex flex-col gap-4 h-full">
      <!-- Market Prices (default). visited+v-show keep-alive (App.vue precedent):
        sub-tabs stay mounted so switching back shows cached data instead of
        remounting (TimeRangeSelector onMounted refetch) with a loading flash. -->
      <MarketView v-if="visited.has('market')" v-show="activeTab === 'market'" />

      <!-- Farming -->
      <FarmingView v-if="visited.has('farming')" v-show="activeTab === 'farming'" :active="isActive && activeTab === 'farming'" />

      <!-- Surveying -->
      <SurveyView v-if="visited.has('surveying')" v-show="activeTab === 'surveying'" :active="isActive && activeTab === 'surveying'" />

      <!-- Stall Tracker -->
      <StallTrackerView v-if="visited.has('stall-tracker')" v-show="activeTab === 'stall-tracker'" />

      <!-- Council Wealth -->
      <CouncilWealthView v-if="visited.has('wealth')" v-show="activeTab === 'wealth'" :active="isActive && activeTab === 'wealth'" />
    </div>
  </PaneLayout>
</template>

<script setup lang="ts">
import { computed, reactive, watch } from 'vue'
import PaneLayout from '../Shared/PaneLayout.vue'
import MarketView from '../Market/MarketView.vue'
import FarmingView from './EconomicsFarmingView.vue'
import SurveyView from './EconomicsSurveyView.vue'
import StallTrackerView from '../StallTracker/StallTrackerView.vue'
import CouncilWealthView from './CouncilWealthView.vue'

const props = withDefaults(defineProps<{
  activeTab: string;
  active?: boolean;
}>(), { active: true });

// Top-level visibility ANDed with the local sub-tab below: a hidden Economics
// tab leaves every child inactive even when its sub-tab is selected.
const isActive = computed(() => props.active !== false)

// Keep-alive set (App.vue precedent): first visit mounts, later visits reuse.
const visited = reactive(new Set<string>([props.activeTab]))
watch(() => props.activeTab, (tab) => { visited.add(tab) })
</script>
