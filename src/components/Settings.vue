<template>
  <PaneLayout screen-key="settings">
  <div class="max-w-225 p-4 h-full overflow-y-auto">
    <h2 class="text-accent-gold mt-0 mb-6 text-2xl">Settings</h2>

    <div class="flex gap-6 min-h-125">
      <nav class="flex flex-col gap-1 min-w-40 border-r border-border-default pr-4">
        <button
          v-for="tab in tabs"
          :key="tab.id"
          @click="activeTab = tab.id"
          class="px-4 py-2.5 bg-transparent border-none rounded text-text-secondary cursor-pointer text-sm text-left transition-all whitespace-nowrap hover:text-text-primary hover:bg-surface-base"
          :class="{ 'text-accent-gold! bg-surface-base! border-l-2 border-l-accent-gold pl-3.5': activeTab === tab.id }">
          {{ tab.label }}
        </button>
      </nav>

      <div class="flex-1 min-w-0">
        <!-- visited+v-show keep-alive (EconomicsView precedent): settings tabs are
             store-bound forms, so kept-alive tabs never go stale; switching back
             preserves scroll/field state instead of remounting. -->
        <GeneralSettings v-if="visited.has('general')" v-show="activeTab === 'general'" />
        <AppSettingsTab v-if="visited.has('app')" v-show="activeTab === 'app'" />
        <ChatLogsSettings v-if="visited.has('chat-logs')" v-show="activeTab === 'chat-logs'" />
        <NotificationsSettings v-if="visited.has('notifications')" v-show="activeTab === 'notifications'" />
        <UserDataSettings v-if="visited.has('user-data')" v-show="activeTab === 'user-data'" />
        <GameDataSettings v-if="visited.has('game-data')" v-show="activeTab === 'game-data'" />
        <AdvancedSettings v-if="visited.has('advanced')" v-show="activeTab === 'advanced'" />
        <GameStateDebug v-if="visited.has('game-state')" v-show="activeTab === 'game-state'" />
      </div>
    </div>

    <div class="settings-section mt-4 text-text-secondary text-sm">
      <p class="m-0">Settings are automatically saved to: <code class="text-accent-gold bg-surface-dark px-1.5 py-0.5 rounded-sm text-xs break-all">{{ settingsStore.settingsFilePath || 'Loading...' }}</code></p>
    </div>
  </div>
  </PaneLayout>
</template>

<script setup lang="ts">
import { ref, computed, reactive, watch } from "vue";
import { useSettingsStore } from "../stores/settingsStore";
import PaneLayout from "./Shared/PaneLayout.vue";
import GeneralSettings from "./Settings/GeneralSettings.vue";
import AppSettingsTab from "./Settings/AppSettingsTab.vue";
import ChatLogsSettings from "./Settings/ChatLogsSettings.vue";
import NotificationsSettings from "./Settings/NotificationsSettings.vue";
import UserDataSettings from "./Settings/UserDataSettings.vue";
import GameDataSettings from "./Settings/GameDataSettings.vue";
import AdvancedSettings from "./Settings/AdvancedSettings.vue";
import GameStateDebug from "./Settings/GameStateDebug.vue";

const settingsStore = useSettingsStore();

type TabId = 'general' | 'app' | 'chat-logs' | 'notifications' | 'user-data' | 'game-data' | 'advanced' | 'game-state';

const allTabs: { id: TabId; label: string; devOnly?: boolean }[] = [
  { id: 'general', label: 'General' },
  { id: 'app', label: 'App Settings' },
  { id: 'chat-logs', label: 'Chat Logs' },
  { id: 'notifications', label: 'Notifications' },
  { id: 'user-data', label: 'User Data' },
  { id: 'game-data', label: 'Game Data' },
  { id: 'advanced', label: 'Advanced' },
  { id: 'game-state', label: 'Game State', devOnly: true },
];

const tabs = computed(() =>
  allTabs.filter(t => !t.devOnly || !settingsStore.settings.devModeEnabled)
);

const activeTab = ref<TabId>('general');

// Keep-alive set (EconomicsView precedent): first visit mounts, later visits reuse.
// Settings tabs bind directly to the reactive settings store (character-independent),
// so a kept-alive tab cannot go stale.
const visited = reactive(new Set<TabId>([activeTab.value]));
watch(activeTab, (tab) => { visited.add(tab); });
</script>
