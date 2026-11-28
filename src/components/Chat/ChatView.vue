<template>
  <PaneLayout screen-key="chat">
    <div class="flex flex-col h-full bg-surface-base rounded-lg overflow-hidden">
      <!-- Tab Content -->
      <div class="flex-1 overflow-hidden flex flex-col">
        <!-- visited+v-show keep-alive (EconomicsView precedent): sub-tabs stay mounted so
             switching back shows cached messages instead of remounting (onMounted reload) with a flash.
             Live updates keep flowing via each tab's useChatLiveRefresh while mounted. -->
        <ChatSearchView v-if="visited.has('search')" v-show="activeTab === 'search'" />
        <ChannelView v-if="visited.has('channels')" v-show="activeTab === 'channels'" />
        <TellsView v-if="visited.has('tells')" v-show="activeTab === 'tells'" />
        <PartyView v-if="visited.has('party')" v-show="activeTab === 'party'" />
        <NearbyView v-if="visited.has('nearby')" v-show="activeTab === 'nearby'" />
        <GuildView v-if="visited.has('guild')" v-show="activeTab === 'guild'" />
        <SystemView v-if="visited.has('system')" v-show="activeTab === 'system'" />
        <AllMessagesView v-if="visited.has('all')" v-show="activeTab === 'all'" />
        <JsonView v-if="visited.has('json')" v-show="activeTab === 'json'" />
        <WatchwordsView v-if="visited.has('watchwords')" v-show="activeTab === 'watchwords'" />
        <PoemsView v-if="visited.has('poems')" v-show="activeTab === 'poems'" />
      </div>
    </div>
  </PaneLayout>
</template>

<script setup lang="ts">
import { reactive, watch } from 'vue'
import PaneLayout from '../Shared/PaneLayout.vue'
import ChatSearchView from './ChatSearchView.vue'
import ChannelView from './ChannelView.vue'
import TellsView from './TellsView.vue'
import PartyView from './PartyView.vue'
import NearbyView from './NearbyView.vue'
import GuildView from './GuildView.vue'
import SystemView from './SystemView.vue'
import AllMessagesView from './AllMessagesView.vue'
import JsonView from './JsonView.vue'
import WatchwordsView from './WatchwordsView.vue'
import PoemsView from './PoemsView.vue'

const props = defineProps<{
  activeTab: string;
}>();

// Keep-alive set (EconomicsView precedent): first visit mounts, later visits reuse.
const visited = reactive(new Set<string>([props.activeTab]))
watch(() => props.activeTab, (tab) => { visited.add(tab) })
</script>
