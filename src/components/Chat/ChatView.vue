<template>
  <PaneLayout screen-key="chat">
    <div class="flex flex-col h-full bg-surface-base rounded-lg overflow-hidden">
      <!-- Tab Content -->
      <div class="flex-1 overflow-hidden flex flex-col">
        <ChatSearchView v-if="activeTab === 'search'" />
        <ChannelView v-else-if="activeTab === 'channels'" />
        <TellsView v-else-if="activeTab === 'tells'" />
        <PartyView v-else-if="activeTab === 'party'" />
        <NearbyView v-else-if="activeTab === 'nearby'" />
        <GuildView v-else-if="activeTab === 'guild'" />
        <SystemView v-else-if="activeTab === 'system'" />
        <AllMessagesView v-else-if="activeTab === 'all'" />
        <JsonView v-else-if="activeTab === 'json'" />
        <WatchwordsView v-else-if="activeTab === 'watchwords'" />
        <PoemsView v-else-if="activeTab === 'poems'" />
      </div>
    </div>
  </PaneLayout>
</template>

<script setup lang="ts">
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

defineProps<{
  activeTab: string;
}>();
</script>
