<template>
  <PaneLayout screen-key="economics">
    <div class="flex flex-col gap-4 h-full">
      <!-- Market Prices (default) -->
      <template v-if="activeTab === 'market'">
        <MarketView />
      </template>

      <!-- Farming -->
      <template v-else-if="activeTab === 'farming'">
        <FarmingView />
      </template>

      <!-- Surveying -->
      <template v-else-if="activeTab === 'surveying'">
        <SurveyView />
      </template>

      <!-- Stall Tracker -->
      <template v-else-if="activeTab === 'stall-tracker'">
        <StallTrackerView />
      </template>

      <!-- Council Wealth -->
      <template v-else-if="activeTab === 'wealth'">
        <CouncilWealthView />
      </template>
    </div>
  </PaneLayout>
</template>

<script setup lang="ts">
import PaneLayout from '../Shared/PaneLayout.vue'
import MarketView from '../Market/MarketView.vue'
import FarmingView from './EconomicsFarmingView.vue'
import SurveyView from './EconomicsSurveyView.vue'
import StallTrackerView from '../StallTracker/StallTrackerView.vue'
import CouncilWealthView from './CouncilWealthView.vue'

defineProps<{
  activeTab: string;
}>();
</script>
