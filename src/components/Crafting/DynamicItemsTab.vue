<template>
  <PaneLayout
    screen-key="crafting-dynamic-items"
    :left-pane="{ title: 'Item Slots', defaultWidth: 240, minWidth: 180, maxWidth: 400 }">
    <template #left>
      <div class="flex flex-col gap-1 px-2 pb-2">
        <SkeletonLoader v-if="loading" variant="text" :lines="6" />

        <button
          v-for="kw in keywords"
          :key="kw.keyword"
          class="flex items-center justify-between px-2 py-1.5 rounded text-xs text-left cursor-pointer border-none w-full"
          :class="selectedKeyword === kw.keyword
            ? 'bg-accent-gold/15 text-accent-gold border border-accent-gold/30'
            : 'bg-transparent text-text-secondary hover:bg-surface-base border border-transparent'"
          @click="selectKeyword(kw.keyword)">
          <span class="truncate">{{ kw.description }}</span>
          <span class="text-text-muted font-mono shrink-0 ml-2 text-[0.65rem]">
            {{ getEnabledCount(kw.keyword) }}/{{ getItemCount(kw.keyword) }}
          </span>
        </button>

        <div v-if="!loading && keywords.length === 0" class="text-text-dim text-xs italic px-1">
          No dynamic ingredient slots found.
        </div>
      </div>
    </template>

    <!-- Center content -->
    <div class="p-4 flex flex-col gap-4 h-full min-h-0 overflow-hidden">
      <EmptyState
        v-if="!loading && !selectedKeyword"
        variant="panel"
        primary="Select an item slot"
        secondary="Choose a dynamic ingredient slot from the list to configure which items are allowed." />

      <EmptyState
        v-else-if="!loading && keywords.length === 0"
        variant="panel"
        primary="No dynamic slots"
        secondary="No recipes with wildcard ingredient slots were found in the game data." />

      <template v-else-if="selectedKeyword">
        <!-- Header -->
        <div class="flex items-center justify-between">
          <div>
            <h3 class="text-text-primary text-base font-semibold m-0">
              {{ selectedDescription }}
            </h3>
            <div class="text-text-muted text-[0.65rem] mt-0.5">
              Keyword: <span class="font-mono text-text-dim">{{ selectedKeyword }}</span>
              — {{ enabledItems.length }} of {{ currentItems.length }} items enabled
            </div>
          </div>
          <div class="flex gap-2">
            <button
              class="btn-secondary text-xs py-1"
              @click="enableAll">
              Enable All
            </button>
            <button
              class="btn-secondary text-xs py-1"
              @click="disableAll">
              Disable All
            </button>
          </div>
        </div>

        <!-- Filter -->
        <input
          v-if="currentItems.length > 10"
          v-model="filterQuery"
          class="input w-full text-xs"
          placeholder="Filter items..." />

        <!-- Item list -->
        <div v-if="loadingItems" class="text-text-dim text-xs italic">Loading items...</div>
        <div v-else class="flex-1 min-h-0 overflow-y-auto">
          <div class="flex flex-col gap-0.5">
            <div
              v-for="item in filteredItems"
              :key="item.id"
              class="flex items-center gap-2 px-2 py-1 rounded text-xs"
              :class="isEnabled(item.id) ? 'bg-surface-base/50' : 'bg-surface-dark/30 opacity-50'">
              <button
                class="w-4 h-4 rounded border flex items-center justify-center cursor-pointer shrink-0 transition-colors"
                :class="isEnabled(item.id)
                  ? 'bg-accent-gold/20 border-accent-gold/50 text-accent-gold'
                  : 'bg-surface-dark border-border-light text-transparent hover:border-text-muted'"
                @click="toggleItem(item.id)">
                <span v-if="isEnabled(item.id)" class="text-[0.6rem] leading-none">✓</span>
              </button>
              <ItemInline :reference="String(item.id)" />
            </div>
          </div>
        </div>
      </template>
    </div>
  </PaneLayout>
</template>

<script setup lang="ts">
import { ref, computed, onMounted } from "vue";
import { useGameDataStore } from "../../stores/gameDataStore";
import { useCraftingStore } from "../../stores/craftingStore";
import type { ItemInfo } from "../../types/gameData/items";
import PaneLayout from "../Shared/PaneLayout.vue";
import SkeletonLoader from "../Shared/SkeletonLoader.vue";
import EmptyState from "../Shared/EmptyState.vue";
import ItemInline from "../Shared/Item/ItemInline.vue";

interface KeywordEntry {
  keyword: string
  description: string
  item_count: number
}

const gameData = useGameDataStore();
const craftingStore = useCraftingStore();

const loading = ref(true);
const loadingItems = ref(false);
const keywords = ref<KeywordEntry[]>([]);
const selectedKeyword = ref<string | null>(null);
const currentItems = ref<ItemInfo[]>([]);
const filterQuery = ref("");

// Cache item counts and disabled sets per keyword
const itemCountCache = ref<Map<string, number>>(new Map());
const disabledCache = ref<Map<string, Set<number>>>(new Map());

const selectedDescription = computed(() => {
  const kw = keywords.value.find((k) => k.keyword === selectedKeyword.value);
  return kw?.description ?? selectedKeyword.value ?? "";
});

const enabledItems = computed(() => {
  if (!selectedKeyword.value) return [];
  const disabled = disabledCache.value.get(selectedKeyword.value) ?? new Set();
  return currentItems.value.filter((item) => !disabled.has(item.id));
});

const filteredItems = computed(() => {
  if (!filterQuery.value.trim()) return currentItems.value;
  const q = filterQuery.value.toLowerCase();
  return currentItems.value.filter((item) => item.name.toLowerCase().includes(q));
});

function isEnabled(itemId: number): boolean {
  if (!selectedKeyword.value) return true;
  const disabled = disabledCache.value.get(selectedKeyword.value);
  return !disabled?.has(itemId);
}

function getEnabledCount(keyword: string): number {
  const total = itemCountCache.value.get(keyword) ?? 0;
  const disabled = disabledCache.value.get(keyword);
  if (!disabled) return total;
  return total - disabled.size;
}

function getItemCount(keyword: string): number {
  return itemCountCache.value.get(keyword) ?? 0;
}

function toggleItem(itemId: number) {
  if (!selectedKeyword.value) return;
  const enabled = isEnabled(itemId);
  craftingStore.setDynamicItemDisabled(selectedKeyword.value, itemId, enabled);
  // Update local cache
  refreshDisabledCache(selectedKeyword.value);
}

function enableAll() {
  if (!selectedKeyword.value) return;
  craftingStore.setAllDynamicItems(
    selectedKeyword.value,
    currentItems.value.map((i) => i.id),
    false,
  );
  refreshDisabledCache(selectedKeyword.value);
}

function disableAll() {
  if (!selectedKeyword.value) return;
  craftingStore.setAllDynamicItems(
    selectedKeyword.value,
    currentItems.value.map((i) => i.id),
    true,
  );
  refreshDisabledCache(selectedKeyword.value);
}

function refreshDisabledCache(keyword: string) {
  const updated = new Map(disabledCache.value);
  updated.set(keyword, craftingStore.getDynamicItemDisabledSet(keyword));
  disabledCache.value = updated;
}

async function selectKeyword(keyword: string) {
  selectedKeyword.value = keyword;
  filterQuery.value = "";
  loadingItems.value = true;
  try {
    const items = await gameData.getItemsByKeyword(keyword);
    currentItems.value = items;
    itemCountCache.value.set(keyword, items.length);
    refreshDisabledCache(keyword);
  } catch (e) {
    console.error("[crafting] Failed to load items for keyword:", keyword, e);
    currentItems.value = [];
  } finally {
    loadingItems.value = false;
  }
}

onMounted(async () => {
  try {
    const result = await gameData.getRecipeIngredientKeywords();
    keywords.value = result;

    // Badge totals ride along with the keywords call — no per-keyword IPC.
    // Disabled sets are local settings reads (sync, no IPC). Item lists
    // still lazy-load per selection in selectKeyword.
    for (const kw of result) {
      itemCountCache.value.set(kw.keyword, kw.item_count);
      disabledCache.value.set(kw.keyword, craftingStore.getDynamicItemDisabledSet(kw.keyword));
    }
  } catch (e) {
    console.error("[crafting] Failed to load ingredient keywords:", e);
  } finally {
    loading.value = false;
  }
});
</script>
