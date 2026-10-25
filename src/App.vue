<template>
  <div class="h-full flex flex-col overflow-hidden">
    <!-- Startup phases -->
    <StartupSplash v-if="startup.phase === 'splash'" />

    <StartupLayout v-else-if="startup.isSetupWizard" :current-step="startup.setupStepIndex">
      <SetupPathStep v-if="startup.phase === 'setup-path'" />
      <SetupWatchersStep v-else-if="startup.phase === 'setup-watchers'" />
      <SetupCharacterStep v-else-if="startup.phase === 'setup-character'" />
    </StartupLayout>

    <CharacterSelect v-else-if="startup.phase === 'select-character'" />

    <StartupProgress v-else-if="startup.phase === 'loading'" :tasks="startup.startupTasks" :error-message="startup.error" />

    <!-- Main app -->
    <template v-else-if="startup.phase === 'ready'">
      <div class="flex-1 flex flex-col min-h-0 overflow-hidden">
        <MenuBar
          ref="menuBarRef"
          :currentView="currentView"
          :helpOpen="showHelp"
          @navigate="navigateToView"
          @update:sub-tab="onSubTabChange"
          @toggleHelp="showHelp = !showHelp"
        />

        <div class="flex-1 flex flex-col p-4 min-h-0 transition-[padding] duration-250 ease-out" :class="hasSubTabs ? 'pt-28' : 'pt-20'">
          <div class="flex-1 min-h-0">
            <div v-if="visited.has('dashboard')" v-show="currentView === 'dashboard'" class="h-full">
              <DashboardView />
            </div>
            <div v-if="visited.has('character')" v-show="currentView === 'character'" class="h-full">
              <CharacterView :active-tab="activeSubTab" />
            </div>
            <div v-if="visited.has('inventory')" v-show="currentView === 'inventory'" class="h-full">
              <InventoryWrapper :active-tab="activeSubTab" />
            </div>
            <div v-if="visited.has('crafting')" v-show="currentView === 'crafting'" class="h-full">
              <CraftingView :active-tab="activeSubTab" />
            </div>
            <div v-if="visited.has('economics')" v-show="currentView === 'economics'" class="h-full">
              <EconomicsView :active-tab="activeSubTab" />
            </div>
            <div v-if="visited.has('chat')" v-show="currentView === 'chat'" class="h-full">
              <ChatView :active-tab="activeSubTab" />
            </div>
            <!-- Data browser is now an overlay, not a view -->
            <div v-if="visited.has('search')" v-show="currentView === 'search'" class="h-full">
              <SearchView @navigate="handleSearchNavigate" />
            </div>
            <div v-if="visited.has('settings')" v-show="currentView === 'settings'" class="h-full">
              <Settings
                :parsing="parsing"
                :error="error"
                :onParseLog="parseLog" />
            </div>
          </div>
        </div>
      </div>

      <!-- Reference Shelf -->
      <ReferenceShelf />

      <!-- Bottom bar -->
      <div class="shrink-0 h-6 bg-surface-base border-t border-border-default flex items-center justify-between px-3">
        <span class="text-text-dim text-[10px]">Glogger by Zenith of Dreva -- fork maintained by TwinkleofToes, built on the oddio fork -- Some portions copyright 2026 Elder Game, LLC.</span>
        <div v-if="updateStore.updateAvailable && !updateStore.dismissed" class="flex items-center gap-1.5 text-[10px]">
          <template v-if="updateStore.installing">
            <span class="text-accent-blue">
              Updating... {{ updateStore.downloadProgress }}%
            </span>
          </template>
          <template v-else>
            <button
              class="flex items-center gap-1.5 text-accent-blue hover:text-accent-blue-bright transition-colors cursor-pointer"
              @click="updateStore.downloadAndInstall()"
            >
              <span class="inline-block w-1.5 h-1.5 rounded-full bg-accent-blue animate-pulse" />
              Update to v{{ updateStore.latestVersion }}
            </button>
            <button
              class="text-text-dim hover:text-text-default transition-colors cursor-pointer"
              title="Dismiss"
              @click="updateStore.dismiss()"
            >&#10005;</button>
          </template>
          <span v-if="updateStore.installError" class="text-accent-red">{{ updateStore.installError }}</span>
        </div>
      </div>

      <DataBrowserOverlay ref="dataBrowserOverlayRef" />
      <HelpOverlay
        :show="showHelp"
        @update:show="showHelp = $event"
        @navigate="showHelp = false; navigateToView($event as AppView)"
      />
      <ToastContainer />
      <CdnUpdateModal
        :show="gameDataStore.cdnUpdateAvailable && !gameDataStore.cdnDismissed"
        :current-version="gameDataStore.cdnCurrentVersion"
        :remote-version="gameDataStore.cdnRemoteVersion"
        :restarting="cdnRestarting"
        @dismiss="gameDataStore.dismissCdnUpdate()"
        @restart="handleCdnRestart"
      />
      <QuickSearchOverlay
        :show="showQuickSearch"
        @update:show="showQuickSearch = $event"
        @navigate="handleSearchNavigate"
      />
    </template>
  </div>
</template>

<script setup lang="ts">
import { ref, reactive, computed, onMounted, onBeforeUnmount } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { useSettingsStore } from "./stores/settingsStore";
import { useStartupStore } from "./stores/startupStore";
import { useUpdateStore } from "./stores/updateStore";
import { useGameDataStore } from "./stores/gameDataStore";
import { useGameStateStore } from "./stores/gameStateStore";
import { relaunch } from "@tauri-apps/plugin-process";
import { provideEntityNavigation } from "./composables/useEntityNavigation";
import { useDataBrowserStore, entityTypeToTab } from "./stores/dataBrowserStore";
import { provideViewNavigation } from "./composables/useViewNavigation";
import MenuBar, { type AppView } from "./components/MenuBar.vue";
import DashboardView from "./components/Dashboard/DashboardView.vue";
import CharacterView from "./components/Character/CharacterView.vue";
import InventoryWrapper from "./components/Inventory/InventoryWrapper.vue";
import CraftingView from "./components/Crafting/CraftingView.vue";
import EconomicsView from "./components/Economics/EconomicsView.vue";
import ChatView from "./components/Chat/ChatView.vue";
import DataBrowserOverlay from "./components/DataBrowser/DataBrowserOverlay.vue";
import SearchView from "./components/Search/SearchView.vue";
import HelpOverlay from "./components/Help/HelpOverlay.vue";
import Settings from "./components/Settings.vue";
import StartupSplash from "./components/Startup/StartupSplash.vue";
import StartupLayout from "./components/Startup/StartupLayout.vue";
import StartupProgress from "./components/Startup/StartupProgress.vue";
import SetupPathStep from "./components/Startup/SetupPathStep.vue";
import SetupWatchersStep from "./components/Startup/SetupWatchersStep.vue";
import SetupCharacterStep from "./components/Startup/SetupCharacterStep.vue";
import CharacterSelect from "./components/Startup/CharacterSelect.vue";
import ToastContainer from "./components/Shared/ToastContainer.vue";
import QuickSearchOverlay from "./components/Search/QuickSearchOverlay.vue";
import ReferenceShelf from "./components/Shared/ReferenceShelf/ReferenceShelf.vue";
import CdnUpdateModal from "./components/Shared/CdnUpdateModal.vue";
import { useToast } from "./composables/useToast";
import { useViewPrefs } from "./composables/useViewPrefs";
import type { SearchResult } from "./composables/useUnifiedSearch";

const settingsStore = useSettingsStore();
const toast = useToast();
const startup = useStartupStore();
const updateStore = useUpdateStore();
const gameDataStore = useGameDataStore();
const dataBrowserStore = useDataBrowserStore();
const dataBrowserOverlayRef = ref<InstanceType<typeof DataBrowserOverlay> | null>(null);

const cdnRestarting = ref(false);

async function handleCdnRestart() {
  cdnRestarting.value = true;
  try {
    await gameDataStore.forceRefreshCdn();
    await relaunch();
  } catch (e: any) {
    cdnRestarting.value = false;
    toast.error(`CDN update failed: ${e}`);
  }
}

const error = ref("");
const parsing = ref(false);
const { prefs: appPrefs, update: updateAppPrefs } = useViewPrefs('app', { currentView: 'dashboard' as string });
const currentView = ref<AppView>(appPrefs.value.currentView as AppView || "dashboard");
const visited = reactive(new Set<AppView>([currentView.value]));
const menuBarRef = ref<InstanceType<typeof MenuBar> | null>(null);
const activeSubTab = ref("");

const showQuickSearch = ref(false);
const showHelp = ref(false);

const hasSubTabs = computed(() => menuBarRef.value?.hasTabs ?? false);

function onSubTabChange(tab: string) {
  activeSubTab.value = tab;
}

// Global Ctrl+F / Ctrl+K to open quick search
function handleGlobalKeydown(event: KeyboardEvent) {
  if ((event.ctrlKey || event.metaKey) && (event.key === "f" || event.key === "k")) {
    event.preventDefault();
    showQuickSearch.value = true;
  }
}
onMounted(() => {
  window.addEventListener("keydown", handleGlobalKeydown);
});
onBeforeUnmount(() => {
  window.removeEventListener("keydown", handleGlobalKeydown);
});

function handleSearchNavigate(result: SearchResult) {
  // If the result has an entity type, open the data browser overlay
  if (result.navigation.entityType && result.navigation.entityId) {
    dataBrowserOverlayRef.value?.navigateToEntity({
      type: result.navigation.entityType as any,
      id: result.navigation.entityId,
    });
    dataBrowserStore.open(entityTypeToTab[result.navigation.entityType]);
    return;
  }

  // Otherwise navigate to the view + sub-tab
  navigateToView(result.navigation.view);
  if (result.navigation.subTab && menuBarRef.value) {
    menuBarRef.value.activeSubTabs[result.navigation.view] = result.navigation.subTab;
    activeSubTab.value = result.navigation.subTab;
  }
}

provideViewNavigation((target) => {
  const view = target.view as AppView;
  visited.add(view);
  currentView.value = view;
  if (target.subTab && menuBarRef.value) {
    menuBarRef.value.activeSubTabs[view] = target.subTab;
    activeSubTab.value = target.subTab;
  }
});

provideEntityNavigation((target) => {
  dataBrowserOverlayRef.value?.navigateToEntity(target);
  const tab = entityTypeToTab[target.type];
  if (tab) dataBrowserStore.open(tab);
});

let unlistenDevToast: UnlistenFn | null = null;

onMounted(async () => {
  await startup.initialize();
  updateStore.startPolling();

  // Listen for toast events from the dev panel window
  unlistenDevToast = await listen<{ type: string; message: string }>("dev-toast", (event) => {
    const { type, message } = event.payload;
    if (type === "success") toast.success(message);
    else if (type === "warning") toast.warn(message);
    else if (type === "error") toast.error(message);
    else toast.info(message);
  });
});

onBeforeUnmount(() => {
  unlistenDevToast?.();
});

async function parseLog() {
  error.value = "";
  const gameStateStore = useGameStateStore();
  gameStateStore.resetSessionSkills();
  parsing.value = true;
  try {
    const path = settingsStore.settings.logFilePath || settingsStore.getPlayerLogPath();
    const jobId = await invoke<string>("parse_log", { path });
    // parse_log returns immediately; poll the job registry for the outcome.
    // Poll is bounded (~10 min): a stuck Running job must time out rather
    // than spin parsing=true forever. A missing registry entry is a failure,
    // not success (entry absent = evicted backend or lock failure).
    let finished = false;
    for (let i = 0; i < 1200; i++) {
      await new Promise(r => setTimeout(r, 500));
      const status = await invoke<{ state: string; error?: string } | null>("get_parse_log_status", { jobId });
      if (!status) throw new Error("Log parse job not found — please retry");
      if (status.state === "done") { finished = true; break; }
      if (status.state === "failed") throw new Error(status.error ?? "Log parse failed");
    }
    if (!finished) throw new Error("Log parse timed out");
  } catch (e) {
    error.value = String(e);
  } finally {
    parsing.value = false;
  }
}

function navigateToView(view: AppView) {
  visited.add(view);
  currentView.value = view;
  updateAppPrefs({ currentView: view });
}
</script>
