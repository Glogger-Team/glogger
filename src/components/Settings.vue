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
        <GeneralSettings
          v-if="activeTab === 'general'" />

        <AppSettingsTab
          v-else-if="activeTab === 'app'" />

        <ChatLogsSettings
          v-else-if="activeTab === 'chat-logs'" />

        <NotificationsSettings
          v-else-if="activeTab === 'notifications'" />

        <UserDataSettings
          v-else-if="activeTab === 'user-data'" />

        <GameDataSettings
          v-else-if="activeTab === 'game-data'" />

        <AdvancedSettings
          v-else-if="activeTab === 'advanced'" />
        <GameStateDebug
          v-else-if="activeTab === 'game-state'" />
      </div>
    </div>

    <div class="settings-section mt-4 text-text-secondary text-sm">
      <p class="m-0">Settings are automatically saved to: <code class="text-accent-gold bg-surface-dark px-1.5 py-0.5 rounded-sm text-xs break-all">{{ settingsStore.settingsFilePath || 'Loading...' }}</code></p>
    </div>
  </div>
  </PaneLayout>
</template>

<script setup lang="ts">
import { ref, computed } from "vue";
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
</script>
