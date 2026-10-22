<template>
  <Teleport to="body">
    <Transition name="db-overlay">
      <div v-show="store.isOpen" class="fixed inset-0 z-50">
        <!-- Backdrop -->
        <div class="absolute inset-0 bg-black/60 backdrop-blur-sm" @click="store.close()" />

        <!-- Overlay panel -->
        <div class="absolute inset-6 lg:inset-10 flex flex-col bg-surface-base border border-border-hover rounded-xl shadow-[0_25px_60px_-12px_rgba(0,0,0,0.5)] overflow-hidden">
          <!-- Header: type selector + close -->
          <div class="shrink-0 flex items-center gap-1.5 border-b border-border-default bg-surface-dark/50 px-4 py-2">
            <span class="text-text-muted text-xs font-mono mr-1">Data Browser</span>
            <div class="w-px h-4 bg-border-default" />
            <button
              v-for="tab in browserTypes"
              :key="tab.id"
              class="px-2.5 py-1 bg-transparent border-none text-text-secondary cursor-pointer font-mono text-xs rounded transition-all hover:bg-surface-elevated hover:text-text-primary"
              :class="{ 'text-accent-gold! bg-surface-elevated! shadow-sm': store.activeType === tab.id }"
              @click="store.setActiveType(tab.id)"
            >
              {{ tab.label }}
            </button>
            <!-- CDN Diff tab (dev mode only, far right) -->
            <template v-if="devMode">
              <div class="w-px h-4 bg-border-default" />
              <button
                class="px-2.5 py-1 bg-transparent border-none text-text-secondary cursor-pointer font-mono text-xs rounded transition-all hover:bg-surface-elevated hover:text-text-primary"
                :class="{ 'text-accent-gold! bg-surface-elevated! shadow-sm': store.activeType === 'cdn-diff' }"
                @click="store.setActiveType('cdn-diff')"
              >
                CDN Diff
              </button>
            </template>
            <div class="flex-1" />
            <kbd class="text-[0.55rem] text-text-dim bg-surface-elevated border border-border-default rounded px-1.5 py-0.5">ESC</kbd>
            <button
              class="bg-transparent border-none text-text-muted cursor-pointer text-sm hover:text-accent-red transition-colors px-1.5 py-0.5 rounded hover:bg-surface-elevated"
              title="Close"
              @click="store.close()"
            >&#x2715;</button>
          </div>

          <!-- Body: browser area + sidebar -->
          <div class="flex-1 flex min-h-0">
            <!-- Browser area -->
            <div ref="browserAreaRef" class="flex-1 min-w-0 min-h-0">
              <div v-if="visitedTypes.has('items')" v-show="store.activeType === 'items'" class="h-full">
                <ItemSearch :nav-target="store.activeType === 'items' ? navTarget : null" />
              </div>
              <div v-if="visitedTypes.has('skills')" v-show="store.activeType === 'skills'" class="h-full">
                <SkillBrowser :nav-target="store.activeType === 'skills' ? navTarget : null" />
              </div>
              <div v-if="visitedTypes.has('abilities')" v-show="store.activeType === 'abilities'" class="h-full">
                <AbilityBrowser />
              </div>
              <div v-if="visitedTypes.has('recipes')" v-show="store.activeType === 'recipes'" class="h-full">
                <RecipeBrowser :nav-target="store.activeType === 'recipes' ? navTarget : null" />
              </div>
              <div v-if="visitedTypes.has('quests')" v-show="store.activeType === 'quests'" class="h-full">
                <QuestBrowser :nav-target="store.activeType === 'quests' ? navTarget : null" />
              </div>
              <div v-if="visitedTypes.has('npcs')" v-show="store.activeType === 'npcs'" class="h-full">
                <NpcBrowser :nav-target="store.activeType === 'npcs' ? navTarget : null" />
              </div>
              <div v-if="visitedTypes.has('enemies')" v-show="store.activeType === 'enemies'" class="h-full">
                <EnemyBrowser :nav-target="store.activeType === 'enemies' ? navTarget : null" />
              </div>
              <div v-if="visitedTypes.has('areas')" v-show="store.activeType === 'areas'" class="h-full">
                <AreaBrowser :nav-target="store.activeType === 'areas' ? navTarget : null" />
              </div>
              <div v-if="visitedTypes.has('effects')" v-show="store.activeType === 'effects'" class="h-full">
                <EffectBrowser />
              </div>
              <div v-if="visitedTypes.has('lorebooks')" v-show="store.activeType === 'lorebooks'" class="h-full">
                <LoreBrowser />
              </div>
              <div v-if="visitedTypes.has('titles')" v-show="store.activeType === 'titles'" class="h-full">
                <TitleBrowser />
              </div>
              <div v-if="visitedTypes.has('achievements')" v-show="store.activeType === 'achievements'" class="h-full">
                <AchievementBrowser />
              </div>
              <div v-if="visitedTypes.has('treasure')" v-show="store.activeType === 'treasure'" class="h-full">
                <TsysBrowser />
              </div>
              <div v-if="devMode && visitedTypes.has('cdn-diff')" v-show="store.activeType === 'cdn-diff'" class="h-full">
                <CdnDiffBrowser />
              </div>
            </div>

            <!-- Sidebar (hidden on CDN Diff since it has its own right pane) -->
            <DataBrowserSidebar v-show="store.activeType !== 'cdn-diff'" class="shrink-0 border-l border-border-default bg-surface-dark/30" @navigate="handleSidebarNavigate" />
          </div>
        </div>
      </div>
    </Transition>
  </Teleport>
</template>

<script setup lang="ts">
import { ref, reactive, computed, watch, nextTick, onMounted, onBeforeUnmount } from "vue";
import { useDataBrowserStore, browserTypes, entityTypeToTab } from "../../stores/dataBrowserStore";
import { useSettingsStore } from "../../stores/settingsStore";
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
import CdnDiffBrowser from "./CdnDiffBrowser.vue";
import DataBrowserSidebar from "./DataBrowserSidebar.vue";

const settingsStore = useSettingsStore();
const devMode = computed(() => settingsStore.settings.devModeEnabled);

const store = useDataBrowserStore();
const navTarget = ref<EntityNavigationTarget | null>(null);
const visitedTypes = reactive(new Set<string>());
const browserAreaRef = ref<HTMLElement | null>(null);

// Track visited types for lazy mounting
watch(() => store.activeType, (type) => {
  visitedTypes.add(type);
}, { immediate: true });

// Also add to visited when overlay opens
watch(() => store.isOpen, (open) => {
  if (open) {
    visitedTypes.add(store.activeType);
  }
});

// Navigate to entity within the overlay
function navigateToEntity(target: EntityNavigationTarget) {
  const tab = entityTypeToTab[target.type];
  if (tab) {
    store.setActiveType(tab);
    visitedTypes.add(tab);
    navTarget.value = { ...target };
  }
}

function handleSidebarNavigate(target: EntityNavigationTarget) {
  navigateToEntity(target);
}

// Focus the search input of the active browser
function focusSearchInput() {
  nextTick(() => {
    if (!browserAreaRef.value) return;
    // Find the visible browser's search input
    const visibleBrowser = browserAreaRef.value.querySelector(':scope > div:not([style*="display: none"]) input[type="text"], :scope > div:not([style*="display: none"]) input:not([type])');
    if (visibleBrowser instanceof HTMLInputElement) {
      visibleBrowser.focus();
      visibleBrowser.select();
    }
  });
}

// Focus search input when overlay opens
watch(() => store.isOpen, (open) => {
  if (open) focusSearchInput();
});

// Also focus when switching tabs
watch(() => store.activeType, () => {
  if (store.isOpen) focusSearchInput();
});

// Global keydown for Ctrl+D and ESC
function handleGlobalKeydown(event: KeyboardEvent) {
  if ((event.ctrlKey || event.metaKey) && event.key === "d") {
    event.preventDefault();
    store.toggle();
    return;
  }
  if (event.key === "Escape" && store.isOpen) {
    event.preventDefault();
    event.stopPropagation();
    store.close();
  }
}

onMounted(() => {
  store.load();
  window.addEventListener("keydown", handleGlobalKeydown);
});

onBeforeUnmount(() => {
  window.removeEventListener("keydown", handleGlobalKeydown);
});

// Expose for App.vue to call when entity navigation is triggered
defineExpose({ navigateToEntity });
</script>

<style scoped>
.db-overlay-enter-active {
  transition: opacity 0.2s ease;
}
.db-overlay-leave-active {
  transition: opacity 0.15s ease;
}
.db-overlay-enter-from,
.db-overlay-leave-to {
  opacity: 0;
}

/* Scale the panel on enter for a popup feel */
.db-overlay-enter-active > :last-child {
  transition: transform 0.2s cubic-bezier(0.16, 1, 0.3, 1);
}
.db-overlay-enter-from > :last-child {
  transform: scale(0.97);
}
</style>
