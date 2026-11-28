<template>
  <PaneLayout screen-key="crafting">
    <div class="h-full min-h-0 flex flex-col">
      <div class="flex-1 min-h-0 h-full">
        <!-- visited+v-show keep-alive (EconomicsView precedent): sub-tabs stay mounted so
             switching back shows cached data instead of remounting (onMounted reload) with a flash. -->
        <QuickCalcTab v-if="visited.has('quick-calc')" v-show="activeTab === 'quick-calc'" />
        <ProjectsTab v-if="visited.has('projects')" v-show="activeTab === 'projects'" />
        <LevelingTab v-if="visited.has('leveling')" v-show="activeTab === 'leveling'" />
        <HistoryTab v-if="visited.has('history')" v-show="activeTab === 'history'" />
        <WorkOrdersTab v-if="visited.has('work-orders')" v-show="activeTab === 'work-orders'" />
        <CooksHelperTab v-if="visited.has('cooks-helper')" v-show="activeTab === 'cooks-helper'" />
        <BreweryTab v-if="visited.has('brewery')" v-show="activeTab === 'brewery'" />
        <SkillsTab v-if="visited.has('skills')" v-show="activeTab === 'skills'" />
        <RecipeBookTab v-if="visited.has('recipe-book')" v-show="activeTab === 'recipe-book'" />
        <DynamicItemsTab v-if="visited.has('dynamic-items')" v-show="activeTab === 'dynamic-items'" />
      </div>
    </div>
  </PaneLayout>
</template>

<script setup lang="ts">
import { reactive, watch } from "vue";
import PaneLayout from "../Shared/PaneLayout.vue";
import QuickCalcTab from "./QuickCalcTab.vue";
import ProjectsTab from "./ProjectsTab.vue";
import LevelingTab from "./LevelingTab.vue";
import HistoryTab from "./HistoryTab.vue";
import WorkOrdersTab from "./WorkOrdersTab.vue";
import CooksHelperTab from "./CooksHelperTab.vue";
import BreweryTab from "./BreweryTab.vue";
import SkillsTab from "./SkillsTab.vue";
import RecipeBookTab from "./RecipeBookTab.vue";
import DynamicItemsTab from "./DynamicItemsTab.vue";

const props = defineProps<{
  activeTab: string;
}>();

// Keep-alive set (EconomicsView precedent): first visit mounts, later visits reuse.
const visited = reactive(new Set<string>([props.activeTab]));
watch(() => props.activeTab, (tab) => { visited.add(tab); });
</script>
