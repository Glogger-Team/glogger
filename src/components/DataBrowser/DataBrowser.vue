<template>
  <PaneLayout screen-key="data-browser">
    <div class="flex flex-col h-full">
      <div class="flex-1 min-h-0">
        <!-- visited+v-show keep-alive (EconomicsView precedent): browsers stay mounted so
             switching back shows cached results instead of remounting with a flash.
             CDN status + nav-target changes arrive via each browser's store/navTarget watchers. -->
        <ItemSearch v-if="visited.has('items')" v-show="activeTab === 'items'" :nav-target="props.navTarget" />
        <SkillBrowser v-if="visited.has('skills')" v-show="activeTab === 'skills'" :nav-target="props.navTarget" />
        <AbilityBrowser v-if="visited.has('abilities')" v-show="activeTab === 'abilities'" />
        <RecipeBrowser v-if="visited.has('recipes')" v-show="activeTab === 'recipes'" :nav-target="props.navTarget" />
        <QuestBrowser v-if="visited.has('quests')" v-show="activeTab === 'quests'" :nav-target="props.navTarget" />
        <NpcBrowser v-if="visited.has('npcs')" v-show="activeTab === 'npcs'" :nav-target="props.navTarget" />
        <EnemyBrowser v-if="visited.has('enemies')" v-show="activeTab === 'enemies'" :nav-target="props.navTarget" />
        <AreaBrowser v-if="visited.has('areas')" v-show="activeTab === 'areas'" :nav-target="props.navTarget" />
        <EffectBrowser v-if="visited.has('effects')" v-show="activeTab === 'effects'" />
        <LoreBrowser v-if="visited.has('lorebooks')" v-show="activeTab === 'lorebooks'" />
        <TitleBrowser v-if="visited.has('titles')" v-show="activeTab === 'titles'" />
        <AchievementBrowser v-if="visited.has('achievements')" v-show="activeTab === 'achievements'" />
        <TsysBrowser v-if="visited.has('treasure')" v-show="activeTab === 'treasure'" />
      </div>
    </div>
  </PaneLayout>
</template>

<script setup lang="ts">
import { reactive, watch } from "vue";
import PaneLayout from "../Shared/PaneLayout.vue";
import type { EntityNavigationTarget } from "../../composables/useEntityNavigation";
import ItemSearch from "./ItemSearch.vue";
import SkillBrowser from "./SkillBrowser.vue";
import AbilityBrowser from "./AbilityBrowser.vue";
import RecipeBrowser from "./RecipeBrowser.vue";
import QuestBrowser from "./QuestBrowser.vue";
import NpcBrowser from "./NpcBrowser.vue";
import EnemyBrowser from "./EnemyBrowser.vue";
import AreaBrowser from "./AreaBrowser.vue";
import EffectBrowser from "./EffectBrowser.vue";
import LoreBrowser from "./LoreBrowser.vue";
import TitleBrowser from "./TitleBrowser.vue";
import AchievementBrowser from "./AchievementBrowser.vue";
import TsysBrowser from "./TsysBrowser.vue";

const props = defineProps<{
  navTarget?: EntityNavigationTarget | null;
  activeTab: string;
}>();

// Keep-alive set (EconomicsView precedent): first visit mounts, later visits reuse.
const visited = reactive(new Set<string>([props.activeTab]));
watch(() => props.activeTab, (tab) => { visited.add(tab); });
</script>
