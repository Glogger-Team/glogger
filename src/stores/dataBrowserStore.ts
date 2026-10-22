import { defineStore, acceptHMRUpdate } from "pinia";
import { ref } from "vue";
import { useSettingsStore } from "./settingsStore";
import type { EntityType } from "../composables/useEntityNavigation";

/** Extends EntityType with browser-only types that don't support entity navigation */
export type BrowserEntityType = EntityType | "effect" | "lorebook" | "title" | "treasure" | "achievement";

export interface FavoriteEntry {
  type: BrowserEntityType;
  reference: string;
  label: string;
}

export interface HistoryEntry {
  type: BrowserEntityType;
  reference: string;
  label: string;
  timestamp: number;
}

const PREFS_KEY = "dataBrowser";

export const browserTypes = [
  { id: "items", label: "Items" },
  { id: "skills", label: "Skills" },
  { id: "abilities", label: "Abilities" },
  { id: "recipes", label: "Recipes" },
  { id: "quests", label: "Quests" },
  { id: "npcs", label: "NPCs" },
  { id: "enemies", label: "Enemies" },
  { id: "areas", label: "Areas" },
  { id: "effects", label: "Effects" },
  { id: "lorebooks", label: "Lorebooks" },
  { id: "titles", label: "Titles" },
  { id: "achievements", label: "Achievements" },
  { id: "treasure", label: "Treasure" },
] as const;

export const entityTypeToTab: Record<string, string> = {
  item: "items",
  skill: "skills",
  ability: "abilities",
  recipe: "recipes",
  quest: "quests",
  npc: "npcs",
  enemy: "enemies",
  area: "areas",
  effect: "effects",
  lorebook: "lorebooks",
  title: "titles",
  achievement: "achievements",
  treasure: "treasure",
};

export const useDataBrowserStore = defineStore("dataBrowser", () => {
  const isOpen = ref(false);
  const activeType = ref("items");
  const favorites = ref<FavoriteEntry[]>([]);
  const history = ref<HistoryEntry[]>([]);
  const maxHistory = ref(30);
  let initialized = false;

  function load() {
    if (initialized) return;
    initialized = true;
    const settingsStore = useSettingsStore();
    const saved = settingsStore.settings.viewPreferences[PREFS_KEY] as
      | {
          activeType?: string;
          favorites?: FavoriteEntry[];
          history?: HistoryEntry[];
          maxHistory?: number;
        }
      | undefined;
    if (saved?.activeType) activeType.value = saved.activeType;
    if (saved?.favorites) favorites.value = saved.favorites;
    if (saved?.history) history.value = saved.history;
    if (saved?.maxHistory !== undefined) maxHistory.value = saved.maxHistory;
  }

  function persist() {
    const settingsStore = useSettingsStore();
    settingsStore.updateSettings({
      viewPreferences: {
        ...settingsStore.settings.viewPreferences,
        [PREFS_KEY]: {
          activeType: activeType.value,
          favorites: favorites.value,
          history: history.value,
          maxHistory: maxHistory.value,
        },
      },
    });
  }

  function open(type?: string) {
    if (type) activeType.value = type;
    isOpen.value = true;
  }

  function close() {
    isOpen.value = false;
  }

  function toggle(type?: string) {
    if (isOpen.value) {
      close();
    } else {
      open(type);
    }
  }

  function setActiveType(type: string) {
    activeType.value = type;
    persist();
  }

  function isFavorite(type: BrowserEntityType, reference: string): boolean {
    return favorites.value.some(
      (f) => f.type === type && f.reference === reference,
    );
  }

  function addFavorite(entry: FavoriteEntry) {
    if (isFavorite(entry.type, entry.reference)) return;
    favorites.value.push({ ...entry });
    persist();
  }

  function removeFavorite(type: BrowserEntityType, reference: string) {
    const idx = favorites.value.findIndex(
      (f) => f.type === type && f.reference === reference,
    );
    if (idx !== -1) {
      favorites.value.splice(idx, 1);
      persist();
    }
  }

  function toggleFavorite(entry: FavoriteEntry) {
    if (isFavorite(entry.type, entry.reference)) {
      removeFavorite(entry.type, entry.reference);
    } else {
      addFavorite(entry);
    }
  }

  function addToHistory(entry: Omit<HistoryEntry, "timestamp">) {
    // Remove existing entry if present (move to front)
    const idx = history.value.findIndex(
      (h) => h.type === entry.type && h.reference === entry.reference,
    );
    if (idx !== -1) history.value.splice(idx, 1);

    history.value.unshift({
      ...entry,
      timestamp: Date.now(),
    });

    // Cap at maxHistory
    if (history.value.length > maxHistory.value) {
      history.value.splice(maxHistory.value);
    }
    persist();
  }

  function clearHistory() {
    history.value = [];
    persist();
  }

  function setMaxHistory(n: number) {
    maxHistory.value = n;
    if (history.value.length > n) {
      history.value.splice(n);
    }
    persist();
  }

  return {
    isOpen,
    activeType,
    favorites,
    history,
    maxHistory,
    load,
    open,
    close,
    toggle,
    setActiveType,
    isFavorite,
    addFavorite,
    removeFavorite,
    toggleFavorite,
    addToHistory,
    clearHistory,
    setMaxHistory,
  };
});

if (import.meta.hot) {
  import.meta.hot.accept(acceptHMRUpdate(useDataBrowserStore, import.meta.hot))
}
