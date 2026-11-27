<template>
  <div class="h-screen flex flex-col bg-surface-base text-text-primary overflow-hidden">
    <!-- Header -->
    <div class="shrink-0 flex items-center justify-between px-4 py-2 border-b border-border-default">
      <h1 class="h1-brand">Dev Panel</h1>
      <span class="text-text-muted text-xs">glogger dev tools</span>
    </div>

    <!-- Tabs -->
    <div class="shrink-0 flex border-b border-border-default px-2">
      <button
        v-for="tab in tabs"
        :key="tab.id"
        class="px-3 py-2 bg-transparent border-none text-text-secondary cursor-pointer text-xs rounded-t transition-all hover:bg-surface-elevated hover:text-text-primary"
        :class="{ 'text-accent-gold! bg-surface-elevated!': activeTab === tab.id }"
        @click="activeTab = tab.id">
        {{ tab.label }}
      </button>
    </div>

    <!-- Tab content -->
    <div class="flex-1 min-h-0 overflow-y-auto p-4">
      <GameStateTab v-if="activeTab === 'game-state'" />
      <ComponentShowcaseTab v-else-if="activeTab === 'showcase'" />
      <TestingHelpersTab v-else-if="activeTab === 'testing'" />
      <DebugCaptureTab v-else-if="activeTab === 'debug-capture'" />
    </div>

    <ToastContainer />
  </div>
</template>

<script setup lang="ts">
import { ref } from "vue";
import GameStateTab from "./tabs/GameStateTab.vue";
import ComponentShowcaseTab from "./tabs/ComponentShowcaseTab.vue";
import TestingHelpersTab from "./tabs/TestingHelpersTab.vue";
import DebugCaptureTab from "./tabs/DebugCaptureTab.vue";
import ToastContainer from "../components/Shared/ToastContainer.vue";

const tabs = [
  { id: "game-state", label: "Game State" },
  { id: "showcase", label: "Component Showcase" },
  { id: "testing", label: "Testing Helpers" },
  { id: "debug-capture", label: "Debug Capture" },
];

const activeTab = ref("game-state");
</script>
