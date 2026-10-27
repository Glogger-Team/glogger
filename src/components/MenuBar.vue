<template>
  <div class="fixed top-0 left-0 right-0 z-50">
    <!-- Primary nav row -->
    <div class="grid grid-cols-[auto_1fr_auto] items-center px-4 py-2 bg-surface-base border-b border-border-default">
      <!-- Left: Identity block + Data Browser -->
      <div class="flex items-center gap-4">
        <div class="flex flex-col leading-tight">
          <div class="flex items-center gap-2">
            <component
              :is="settingsStore.settings.devModeEnabled ? 'button' : 'span'"
              class="text-accent-gold font-bold text-lg"
              :class="settingsStore.settings.devModeEnabled ? 'cursor-pointer hover:text-accent-gold-bright bg-transparent border-none p-0 font-bold transition-colors' : ''"
              :title="settingsStore.settings.devModeEnabled ? 'Open Dev Panel' : undefined"
              @click="settingsStore.settings.devModeEnabled && openDevPanel()"
            >glogger</component>
            <div class="flex items-center gap-1.5">
              <span
                class="w-2 h-2 rounded-full cursor-pointer"
                :class="isPlayerLogTailing ? 'bg-status-active' : 'bg-status-inactive'"
                :title="isPlayerLogTailing ? 'Player.log: tailing (click to stop)' : 'Player.log: not tailing (click to start)'"
                @click="togglePlayerLog"
              />
              <span
                class="w-2 h-2 rounded-full cursor-pointer"
                :class="isChatLogTailing ? 'bg-status-active' : 'bg-status-inactive'"
                :title="isChatLogTailing ? 'Chat log: tailing (click to stop)' : 'Chat log: not tailing (click to start)'"
                @click="toggleChatLog"
              />
              <span
                class="w-2 h-2 rounded-full cursor-pointer"
                :class="isFarmingAutoStart ? 'bg-status-active' : 'bg-status-inactive'"
                :title="isFarmingAutoStart ? 'Auto-log farming sessions: on (click to stop & save the current session)' : 'Auto-log farming sessions: off (click to start logging)'"
                @click="toggleFarmingAutoStart"
              />
            </div>
          </div>
          <CharacterPicker :isActive="currentView === 'character'" />
        </div>
        <div class="border-l border-border-default h-8" />
        <button
          class="px-3 py-1.5 bg-transparent border-none text-text-secondary cursor-pointer font-mono text-sm rounded transition-all hover:bg-surface-elevated hover:text-text-primary"
          :class="{ 'bg-surface-elevated! text-accent-gold!': dataBrowserStore.isOpen }"
          @click="dataBrowserStore.toggle()"
          title="Data Browser (Ctrl+D)">
          Data Browser
        </button>
      </div>

      <!-- Center: Navigation links -->
      <div class="flex justify-center gap-1">
        <button
          v-for="item in navItems"
          :key="item.view"
          class="px-3 py-1.5 bg-transparent border-none text-text-secondary cursor-pointer font-mono text-sm rounded transition-all hover:bg-surface-elevated hover:text-text-primary"
          :class="{ 'bg-surface-elevated! text-accent-gold!': currentView === item.view }"
          @click="emit('navigate', item.view)">
          {{ item.label }}
        </button>
      </div>

      <!-- Right: Search, Settings, Help -->
      <div class="flex items-center justify-end gap-1">
        <button
          class="px-3 py-1.5 bg-transparent border-none text-text-secondary cursor-pointer text-sm rounded transition-all leading-none hover:bg-surface-elevated hover:text-text-primary font-mono flex items-center gap-1.5"
          :class="{ 'bg-surface-elevated! text-accent-gold!': currentView === 'search' }"
          @click="emit('navigate', 'search')"
          title="Search (Ctrl+F for quick search)">
          Search
          <kbd class="text-[0.55rem] text-text-muted bg-surface-elevated border border-border-default rounded px-1 py-0.5 leading-none">Ctrl+F</kbd>
        </button>
        <button
          class="px-3 py-1.5 bg-transparent border-none cursor-pointer text-xl rounded transition-all leading-none hover:bg-surface-elevated"
          :class="isAllTailing ? 'text-accent-gold hover:text-accent-gold-bright' : 'text-text-secondary hover:text-text-primary'"
          @click="toggleAllTailing"
          :title="allTailingTooltip">
          📡
        </button>
        <button
          class="px-3 py-1.5 bg-transparent border-none text-text-secondary cursor-pointer text-xl rounded transition-all leading-none hover:bg-surface-elevated hover:text-text-primary"
          :class="{ 'bg-surface-elevated! text-accent-gold!': currentView === 'settings' }"
          @click="emit('navigate', 'settings')"
          title="Settings">
          ⚙
        </button>
        <button
          class="relative px-3 py-1.5 bg-transparent border-none text-text-secondary cursor-pointer text-xl rounded transition-all leading-none hover:bg-surface-elevated hover:text-text-primary"
          :class="{ 'bg-surface-elevated! text-accent-gold!': helpOpen }"
          @click="emit('toggleHelp')"
          :title="updateStore.updateAvailable ? `Update available: v${updateStore.latestVersion}` : 'Help'">
          ?
          <span
            v-if="updateStore.updateAvailable"
            class="absolute top-0.5 right-0.5 w-2 h-2 rounded-full bg-accent-blue animate-pulse"
          />
        </button>
      </div>
    </div>

    <!-- Sub-tab flyout row -->
    <div
      class="overflow-hidden bg-surface-base border-b border-border-default transition-all duration-250 ease-out"
      :class="currentTabs.length ? 'subtab-open' : 'subtab-closed'"
    >
      <div class="flex justify-center gap-1 px-4 py-1.5">
        <button
          v-for="tab in currentTabs"
          :key="tab.id"
          class="px-3 py-1 bg-transparent border-none text-text-secondary cursor-pointer font-mono text-xs rounded transition-all hover:bg-surface-elevated hover:text-text-primary"
          :class="{ 'text-accent-gold! bg-surface-elevated!': activeSubTabs[currentView] === tab.id }"
          @click="selectSubTab(tab.id)">
          {{ tab.label }}
        </button>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed, reactive, watch } from "vue";
import { useCoordinatorStore } from "../stores/coordinatorStore";
import { useDataBrowserStore } from "../stores/dataBrowserStore";
import { useFarmingStore } from "../stores/farmingStore";
import { useSettingsStore } from "../stores/settingsStore";
import { useUpdateStore } from "../stores/updateStore";
import { useKeyboard } from "../composables/useKeyboard";
import { useDevPanel } from "../composables/useDevPanel";
import { useViewPrefs } from "../composables/useViewPrefs";
import { useToast } from "../composables/useToast";
import CharacterPicker from "./CharacterPicker.vue";

const settingsStore = useSettingsStore();
const updateStore = useUpdateStore();
const { openDevPanel } = useDevPanel();
const toast = useToast();

export type AppView = "dashboard" | "character" | "inventory" | "crafting" | "economics" | "chat" | "data-browser" | "search" | "settings";

interface SubTab {
  id: string;
  label: string;
}

// All sub-tab definitions, keyed by view
const viewTabs: Partial<Record<AppView, SubTab[]>> = {
  character: [
    { id: "skills", label: "Skills" },
    { id: "stats", label: "Stats" },
    { id: "npcs", label: "NPCs" },
    { id: "quests", label: "Quests" },
    { id: "deaths", label: "Deaths" },
    { id: "gourmand", label: "Gourmand" },
    { id: "statehelm", label: "Statehelm" },
    { id: "build-planner", label: "Build Planner" },
    { id: "account", label: "Account" },
  ],
  inventory: [
    // Live tab kept visible but dark: the current game client writes no
    // Player.log item events, so the tab renders the no-new-data banner
    // (see LiveInventoryTab) instead of an empty tracker. Remove the entry
    // (and the wrapper branch) only if live tracking is deleted outright.
    { id: "live", label: "Live" },
    { id: "storage", label: "Storage" },
    { id: "vaults", label: "Vaults" },
    { id: "consolidate", label: "Consolidate" },
  ],
  crafting: [
    { id: "quick-calc", label: "Quick Calc" },
    { id: "projects", label: "Projects" },
    { id: "leveling", label: "Leveling" },
    { id: "history", label: "History" },
    { id: "work-orders", label: "Work Orders" },
    { id: "cooks-helper", label: "Cook's Helper" },
    { id: "brewery", label: "Brewery" },
    { id: "skills", label: "Skills" },
    { id: "recipe-book", label: "Recipe Book" },
    { id: "dynamic-items", label: "Dynamic Items" },
  ],
  economics: [
    { id: "market", label: "Market Prices" },
    { id: "farming", label: "Farming" },
    { id: "surveying", label: "Surveying" },
    { id: "stall-tracker", label: "Stall Tracker" },
  ],
  chat: [
    { id: "search", label: "Search" },
    { id: "channels", label: "Channels" },
    { id: "tells", label: "Tells" },
    { id: "party", label: "Party" },
    { id: "nearby", label: "Nearby" },
    { id: "guild", label: "Guild" },
    { id: "system", label: "System" },
    { id: "all", label: "All Messages" },
    { id: "watchwords", label: "Watchwords" },
    // Poems kept visible but dark: new recitals are Player.log-only (dark
    // on the current client) — the tab carries the no-new-data banner plus
    // the historic list and the old-log backfill scan.
    { id: "poems", label: "Poems" },
  ],
};

// Default sub-tab per view
const defaultSubTabs: Partial<Record<AppView, string>> = {
  character: "skills",
  inventory: "storage",
  crafting: "quick-calc",
  economics: "market",
  chat: "search",
};

const coordinatorStore = useCoordinatorStore();
const dataBrowserStore = useDataBrowserStore();
const farmingStore = useFarmingStore();
const isPlayerLogTailing = computed(() => coordinatorStore.isPlayerLogTailing);
const isChatLogTailing = computed(() => coordinatorStore.isChatLogTailing);

// Third status light: persistent auto-logging of farming sessions. Reflects the saved setting;
// clicking it (or the farming-tab checkbox) starts/stops a session via the shared store action.
const isFarmingAutoStart = computed(() => settingsStore.settings.autoStartFarmingSessions);

async function toggleFarmingAutoStart() {
  await farmingStore.setAutoStart(!isFarmingAutoStart.value);
}

async function togglePlayerLog() {
  if (isPlayerLogTailing.value) {
    await coordinatorStore.stopPlayerTailing();
  } else {
    await coordinatorStore.startPlayerTailing();
  }
}

async function toggleChatLog() {
  if (isChatLogTailing.value) {
    await coordinatorStore.stopChatTailing();
  } else {
    await coordinatorStore.startChatTailing();
  }
}

// Master toggle: enable/disable live tailing of all available log files at once.
const isAllTailing = computed(() => isPlayerLogTailing.value && isChatLogTailing.value);
const allTailingTooltip = computed(() =>
  isAllTailing.value
    ? "Live tailing: on (click to stop all log tailing)"
    : "Live tailing: off (click to tail all log files)"
);

async function toggleAllTailing() {
  if (isAllTailing.value) {
    // All on → stop everything that's running.
    if (isPlayerLogTailing.value) await coordinatorStore.stopPlayerTailing();
    if (isChatLogTailing.value) await coordinatorStore.stopChatTailing();
    return;
  }

  // Some/none on → start whatever isn't running, then do the "refresh" half of
  // the old manual workaround automatically: ensure background polling is live
  // and force an immediate catch-up read so on-disk content lands without an
  // app restart.
  try {
    if (!isPlayerLogTailing.value) await coordinatorStore.startPlayerTailing();
    if (!isChatLogTailing.value) await coordinatorStore.startChatTailing();
    await coordinatorStore.startPolling();
    await coordinatorStore.pollWatchers();
    await coordinatorStore.refreshStatus();
  } catch (e) {
    toast.error(`Couldn't start live tailing: ${e}`);
    return;
  }

  // The in-game half can't be automated — glogger only reads the log files and
  // can't make Project Gorgon flush fresh data. Remind the user to do that step
  // so the newest events actually reach disk.
  toast.info(
    "Live tailing on. For the freshest data, save a fresh log in-game (it can't be done from here) — new lines are picked up automatically."
  );
}

const navItems: { view: AppView; label: string }[] = [
  { view: "dashboard", label: "Dashboard" },
  { view: "character", label: "Character" },
  { view: "inventory", label: "Inventory" },
  { view: "crafting", label: "Crafting" },
  { view: "economics", label: "Economics" },
  { view: "chat", label: "Chat Logs" },
];

const props = defineProps<{
  currentView: AppView;
  helpOpen?: boolean;
}>();

const emit = defineEmits<{
  navigate: [view: AppView];
  "update:subTab": [tab: string];
  toggleHelp: [];
}>();

// Persist active sub-tab per view across navigation and app restarts
const { prefs: subTabPrefs, update: updateSubTabPrefs } = useViewPrefs('menuBar.subTabs', {
  ...defaultSubTabs,
} as Record<string, string>);

// Track active sub-tab per view — initialized from persisted prefs
const activeSubTabs = reactive<Record<string, string>>({ ...subTabPrefs.value });

const currentTabs = computed(() => viewTabs[props.currentView] ?? []);

const hasTabs = computed(() => currentTabs.value.length > 0);

// One resolver for every sub-tab assignment: an unknown id (stale pref,
// programmatic nav, keyboard cycle) falls back to the view's first tab
// instead of emitting an id no child handles (blank pane).
function resolveSubTab(view: string, tabId: string | undefined): string {
  const tabs = viewTabs[view as AppView] ?? [];
  if (tabId !== undefined && tabs.some((t) => t.id === tabId)) return tabId;
  return tabs[0]?.id ?? tabId ?? "";
}

function selectSubTab(tabId: string) {
  const resolved = resolveSubTab(props.currentView, tabId);
  activeSubTabs[props.currentView] = resolved;
  updateSubTabPrefs({ [props.currentView]: resolved });
  emit("update:subTab", resolved);
}

// Emit current sub-tab whenever the view changes (so the child view gets the right tab).
watch(() => props.currentView, (view) => {
  const resolved = resolveSubTab(view, activeSubTabs[view]);
  if (resolved !== activeSubTabs[view]) {
    activeSubTabs[view] = resolved;
    updateSubTabPrefs({ [view]: resolved });
  }
  emit("update:subTab", resolved);
}, { immediate: true });

// Keyboard tab cycling for the sub-tab row
const tabIds = computed(() => currentTabs.value.map(t => t.id));
const activeTabRef = computed({
  get: () => activeSubTabs[props.currentView] ?? "",
  set: (val: string) => {
    activeSubTabs[props.currentView] = val;
    updateSubTabPrefs({ [props.currentView]: val });
    emit("update:subTab", val);
  },
});

useKeyboard({
  tabCycling: {
    tabs: tabIds,
    activeTab: activeTabRef,
  },
});

// Expose for parent to read current sub-tab
defineExpose({
  activeSubTabs,
  resolveSubTab,
  viewTabs,
  hasTabs,
});
</script>

<style scoped>
.subtab-open {
  max-height: 3rem;
  opacity: 1;
}

.subtab-closed {
  max-height: 0;
  opacity: 0;
  border-bottom-color: transparent;
}
</style>
