<template>
  <div class="flex h-full overflow-hidden" :style="{ width: `${sidebarWidth}px` }">
    <!-- Drag handle -->
    <div
      class="w-1.5 shrink-0 cursor-col-resize flex items-center justify-center hover:bg-accent-gold/20 rounded transition-colors"
      :class="{ 'bg-accent-gold/30': isResizing }"
      @mousedown="startResize"
      @dblclick="resetWidth"
    >
      <div class="w-px h-8 bg-border-default rounded-full" />
    </div>

    <!-- Sidebar content -->
    <div class="flex-1 flex flex-col min-w-0 overflow-hidden">
    <!-- Tab bar -->
    <div class="shrink-0 flex gap-px bg-surface-dark/50 border-b border-border-default px-2 pt-2 pb-0">
      <button
        v-for="tab in tabs"
        :key="tab.id"
        class="flex-1 px-2 py-1.5 bg-transparent border-none text-xs cursor-pointer transition-all rounded-t"
        :class="activeTab === tab.id
          ? 'text-accent-gold bg-surface-base border-b-2 border-b-accent-gold'
          : 'text-text-muted hover:text-text-secondary hover:bg-surface-elevated'"
        @click="activeTab = tab.id"
      >
        {{ tab.label }}
        <span v-if="tab.count > 0" class="text-[10px] text-text-dim ml-0.5">({{ tab.count }})</span>
      </button>
    </div>

    <!-- Tab content -->
    <div class="flex-1 min-h-0 overflow-hidden flex flex-col">
      <!-- History tab -->
      <template v-if="activeTab === 'history'">
        <div v-if="store.history.length === 0" class="flex-1 flex items-center justify-center text-text-muted text-xs italic p-4">
          No history yet
        </div>
        <template v-else>
          <div class="flex-1 overflow-y-auto">
            <button
              v-for="entry in store.history"
              :key="`${entry.type}:${entry.reference}:${entry.timestamp}`"
              class="w-full flex items-center gap-2 px-3 py-2 bg-transparent border-none border-b border-surface-dark/50 text-left cursor-pointer text-xs transition-colors hover:bg-surface-elevated"
              @click="navigate(entry)"
            >
              <span class="shrink-0 text-[10px] px-1 py-0.5 rounded" :class="typeBadgeClass(entry.type)">
                {{ typeLabel(entry.type) }}
              </span>
              <span class="flex-1 text-text-secondary truncate">{{ entry.label }}</span>
              <span class="shrink-0 text-[10px] text-text-dim">{{ relativeTime(entry.timestamp) }}</span>
            </button>
          </div>
          <div class="shrink-0 border-t border-border-default px-3 py-2">
            <button
              class="bg-transparent border-none text-text-muted text-[10px] cursor-pointer hover:text-accent-red transition-colors"
              @click="store.clearHistory()"
            >Clear history</button>
          </div>
        </template>
      </template>

      <!-- Favorites tab -->
      <template v-if="activeTab === 'favorites'">
        <div class="shrink-0 flex flex-col gap-1.5 px-3 py-2 border-b border-border-default">
          <input
            v-model="favSearch"
            class="input text-xs py-1"
            placeholder="Search favorites..."
          />
          <div class="flex flex-wrap gap-1">
            <button
              v-for="ft in favoriteTypeFilters"
              :key="ft.type"
              class="text-[10px] px-1.5 py-0.5 rounded border cursor-pointer transition-colors"
              :class="favTypeFilter === ft.type
                ? 'bg-accent-gold/20 border-accent-gold/40 text-accent-gold'
                : 'bg-transparent border-border-default text-text-muted hover:text-text-secondary'"
              @click="favTypeFilter = favTypeFilter === ft.type ? null : ft.type"
            >
              {{ ft.label }} ({{ ft.count }})
            </button>
          </div>
        </div>
        <div v-if="filteredFavorites.length === 0" class="flex-1 flex items-center justify-center text-text-muted text-xs italic p-4">
          {{ store.favorites.length === 0 ? 'No favorites yet' : 'No matches' }}
        </div>
        <div v-else class="flex-1 overflow-y-auto">
          <div
            v-for="entry in filteredFavorites"
            :key="`${entry.type}:${entry.reference}`"
            role="button"
            tabindex="0"
            class="w-full flex items-center gap-2 px-3 py-2 border-b border-surface-dark/50 text-left cursor-pointer text-xs transition-colors hover:bg-surface-elevated group"
            @click="navigate(entry)"
            @keydown.enter="navigate(entry)"
            @keydown.space.prevent="navigate(entry)"
          >
            <span class="shrink-0 text-[10px] px-1 py-0.5 rounded" :class="typeBadgeClass(entry.type)">
              {{ typeLabel(entry.type) }}
            </span>
            <span class="flex-1 text-text-secondary truncate">{{ entry.label }}</span>
            <button
              class="shrink-0 opacity-0 group-hover:opacity-100 bg-transparent border-none text-text-dim cursor-pointer text-xs hover:text-accent-red transition-all p-0"
              title="Remove favorite"
              @click.stop="store.removeFavorite(entry.type, entry.reference)"
            >&#x2715;</button>
          </div>
        </div>
      </template>

      <!-- Pinned tab -->
      <template v-if="activeTab === 'pinned'">
        <div v-if="shelf.pins.length === 0" class="flex-1 flex items-center justify-center text-text-muted text-xs italic p-4">
          No pinned entities
        </div>
        <template v-else>
          <div class="flex-1 overflow-y-auto">
            <div
              v-for="pin in shelf.pins"
              :key="`${pin.type}:${pin.reference}`"
              role="button"
              tabindex="0"
              class="w-full flex items-center gap-2 px-3 py-2 border-b border-surface-dark/50 text-left cursor-pointer text-xs transition-colors hover:bg-surface-elevated group"
              @click="navigate(pin)"
              @keydown.enter="navigate(pin)"
              @keydown.space.prevent="navigate(pin)"
            >
              <span class="shrink-0 text-[10px] px-1 py-0.5 rounded" :class="typeBadgeClass(pin.type)">
                {{ typeLabel(pin.type) }}
              </span>
              <span class="flex-1 text-text-secondary truncate">{{ pin.label }}</span>
              <button
                class="shrink-0 opacity-0 group-hover:opacity-100 bg-transparent border-none text-text-dim cursor-pointer text-xs hover:text-accent-red transition-all p-0"
                title="Unpin"
                @click.stop="shelf.unpin(pin.type, pin.reference)"
              >&#x2715;</button>
            </div>
          </div>
          <div class="shrink-0 border-t border-border-default px-3 py-2">
            <button
              class="bg-transparent border-none text-text-muted text-[10px] cursor-pointer hover:text-accent-red transition-colors"
              @click="clearAllPins"
            >Clear all pins</button>
          </div>
        </template>
      </template>
    </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed } from "vue";
import { useDataBrowserStore, entityTypeToTab, type FavoriteEntry, type HistoryEntry, type BrowserEntityType } from "../../stores/dataBrowserStore";
import { useReferenceShelfStore, type PinnedEntity } from "../../stores/referenceShelfStore";
import { useViewPrefs } from "../../composables/useViewPrefs";
import { usePaneResize } from "../../composables/usePaneResize";
import type { EntityType, EntityNavigationTarget } from "../../composables/useEntityNavigation";

const emit = defineEmits<{
  navigate: [target: EntityNavigationTarget];
}>();

const store = useDataBrowserStore();
const shelf = useReferenceShelfStore();

// Resizable sidebar
const DEFAULT_WIDTH = 288; // w-72
const { prefs: sidebarPrefs, update: updateSidebarPrefs } = useViewPrefs("db-sidebar", { width: DEFAULT_WIDTH });
const sidebarWidth = ref(sidebarPrefs.value.width as number);

const { isResizing, startResize, resetWidth } = usePaneResize({
  side: "right",
  minWidth: 200,
  maxWidth: 500,
  get initialWidth() { return sidebarWidth.value; },
  defaultWidth: DEFAULT_WIDTH,
  onWidthChange: (w) => { sidebarWidth.value = w; },
  onResizeEnd: (w) => { sidebarWidth.value = w; updateSidebarPrefs({ width: w }); },
});

const { prefs: tabPrefs, update: updateTabPrefs } = useViewPrefs("db-sidebar.tab", { activeTab: "history" as string });
const activeTab = computed({
  get: () => tabPrefs.value.activeTab as "history" | "favorites" | "pinned",
  set: (val: "history" | "favorites" | "pinned") => updateTabPrefs({ activeTab: val }),
});
const favSearch = ref("");
const favTypeFilter = ref<BrowserEntityType | null>(null);

const tabs = computed(() => [
  { id: "history" as const, label: "History", count: store.history.length },
  { id: "favorites" as const, label: "Favorites", count: store.favorites.length },
  { id: "pinned" as const, label: "Pinned", count: shelf.pins.length },
]);

const favoriteTypeFilters = computed(() => {
  const counts: Partial<Record<BrowserEntityType, number>> = {};
  for (const f of store.favorites) {
    counts[f.type] = (counts[f.type] ?? 0) + 1;
  }
  return Object.entries(counts).map(([type, count]) => ({
    type: type as BrowserEntityType,
    label: typeLabel(type as BrowserEntityType),
    count,
  }));
});

const filteredFavorites = computed(() => {
  let list = store.favorites;
  if (favTypeFilter.value) {
    list = list.filter((f) => f.type === favTypeFilter.value);
  }
  if (favSearch.value.trim()) {
    const q = favSearch.value.trim().toLowerCase();
    list = list.filter((f) => f.label.toLowerCase().includes(q));
  }
  return list;
});

function navigate(entry: FavoriteEntry | HistoryEntry | PinnedEntity) {
  // For browser-only types (effect, title, treasure), just switch tabs
  const tab = entityTypeToTab[entry.type];
  if (tab) {
    store.setActiveType(tab);
  }
  // For standard entity types, also trigger entity navigation
  if (entry.type !== "effect" && entry.type !== "title" && entry.type !== "treasure") {
    emit("navigate", { type: entry.type as EntityType, id: entry.reference });
  }
}

function clearAllPins() {
  // Unpin all entries
  for (const pin of [...shelf.pins]) {
    shelf.unpin(pin.type, pin.reference);
  }
}

const typeBadgeClasses: Record<BrowserEntityType, string> = {
  item: "bg-entity-item/15 text-entity-item",
  skill: "bg-entity-skill/15 text-entity-skill",
  npc: "bg-entity-npc/15 text-entity-npc",
  quest: "bg-entity-quest/15 text-entity-quest",
  recipe: "bg-entity-recipe/15 text-entity-recipe",
  ability: "bg-entity-ability/15 text-entity-ability",
  area: "bg-entity-area/15 text-entity-area",
  enemy: "bg-entity-enemy/15 text-entity-enemy",
  effect: "bg-mod-augment/15 text-mod-augment",
  lorebook: "bg-sky-500/15 text-sky-400",
  title: "bg-amber-500/15 text-amber-400",
  achievement: "bg-violet-500/15 text-violet-400",
  treasure: "bg-emerald-500/15 text-emerald-400",
};

function typeBadgeClass(type: BrowserEntityType): string {
  return typeBadgeClasses[type] ?? "bg-surface-elevated text-text-muted";
}

const typeLabels: Record<BrowserEntityType, string> = {
  item: "ITM",
  skill: "SKL",
  npc: "NPC",
  quest: "QST",
  recipe: "RCP",
  ability: "ABL",
  area: "ARE",
  enemy: "ENM",
  effect: "EFX",
  lorebook: "LOR",
  title: "TTL",
  achievement: "ACH",
  treasure: "TRS",
};

function typeLabel(type: BrowserEntityType): string {
  return typeLabels[type] ?? type.toUpperCase().slice(0, 3);
}

function relativeTime(timestamp: number): string {
  const diff = Date.now() - timestamp;
  const seconds = Math.floor(diff / 1000);
  if (seconds < 60) return "just now";
  const minutes = Math.floor(seconds / 60);
  if (minutes < 60) return `${minutes}m ago`;
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return `${hours}h ago`;
  const days = Math.floor(hours / 24);
  return `${days}d ago`;
}
</script>
