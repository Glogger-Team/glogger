<template>
  <PaneLayout screen-key="db-achievements" :left-pane="{ title: 'Achievements', defaultWidth: 360, minWidth: 280, maxWidth: 500 }">
    <template #left>
      <!-- Status banner if data not ready -->
      <div v-if="store.status !== 'ready'" class="p-4 text-sm">
        <span v-if="store.status === 'loading'" class="text-accent-gold"
          >⟳ Loading game data…</span
        >
        <span v-else-if="store.status === 'error'" class="text-accent-red"
          >✕ {{ store.errorMessage }}</span
        >
      </div>

      <template v-else>
      <div class="flex flex-col gap-2 h-full overflow-hidden">
        <div class="flex items-center gap-2 relative">
          <input
            v-model="query"
            class="input flex-1"
            placeholder="Search achievements…"
            autofocus />
          <span v-if="loading" class="text-accent-gold text-sm animate-spin">⟳</span>
          <span v-else-if="filtered.length" class="text-text-dim text-xs min-w-6 text-right">{{
            filtered.length
          }}</span>
        </div>

        <div v-if="!allAchievements.length && !loading" class="text-text-dim text-xs italic py-1">
          No achievements loaded
        </div>

        <div v-else-if="filtered.length === 0 && query" class="text-text-dim text-xs italic py-1">
          No achievements found for "{{ query }}"
        </div>

        <ul ref="listRef" v-else class="list-none m-0 p-0 overflow-y-auto flex-1 border border-surface-elevated">
          <li
            v-for="(ach, idx) in filtered"
            :key="ach.id"
            class="flex flex-col gap-0.5 px-2 py-1 cursor-pointer border-b border-surface-dark text-xs hover:bg-surface-row-hover"
            :class="{ 'bg-surface-card border-l-2 border-l-accent-gold': selected?.id === ach.id, 'bg-surface-elevated': selectedIndex === idx && selected?.id !== ach.id }"
            @click="selectAchievement(ach)">
            <span class="text-text-primary/75 font-medium overflow-hidden text-ellipsis whitespace-nowrap">{{ ach.name || ach.id }}</span>
            <span class="text-text-dim text-[0.65rem] font-mono">{{ ach.id }}</span>
          </li>
        </ul>
      </div>
      </template>
    </template>

    <!-- Right panel: achievement detail -->
    <div
      class="h-full overflow-y-auto border-l border-surface-elevated p-4 flex flex-col gap-4"
      :class="{ 'items-center justify-center': !selected }">
        <div v-if="!selected" class="text-border-default italic">
          Select an achievement to inspect
        </div>

        <template v-else>
          <div class="flex gap-3 items-start">
            <div class="flex-1 min-w-0">
              <div class="text-base font-bold mb-1 text-accent-gold">
                {{ selected.name || selected.id }}
              </div>
              <div class="text-xs text-text-dim mb-1">
                ID: <span class="text-text-secondary font-mono">{{ selected.id }}</span>
              </div>
              <div v-if="selected.description" class="text-xs text-text-secondary italic">
                {{ selected.description }}
              </div>
            </div>

            <button class="bg-transparent border-none text-text-dim cursor-pointer px-1 py-0 text-sm shrink-0 hover:text-accent-red" @click="clearSelection">✕</button>
          </div>

          <!-- Icon type -->
          <div v-if="selected.icon_type" class="flex flex-col gap-1.5">
            <div class="text-[0.65rem] uppercase tracking-widest text-text-dim border-b border-surface-card pb-0.5">Category</div>
            <div class="flex flex-wrap gap-1">
              <span class="text-[0.72rem] px-1.5 py-0.5 bg-surface-card border border-border-subtle text-entity-item">
                {{ selected.icon_type }}
              </span>
            </div>
          </div>
        </template>
    </div>
  </PaneLayout>
</template>

<script setup lang="ts">
import PaneLayout from "../Shared/PaneLayout.vue";
import { ref, onMounted, watch } from "vue";
import { useGameDataStore } from "../../stores/gameDataStore";
import { useKeyboard } from "../../composables/useKeyboard";
import { useDataBrowserSearch } from "../../composables/useDataBrowserSearch";
import { combineFields } from "../../utils/SearchParser";
import type { AchievementInfo } from "../../types/gameData";

const store = useGameDataStore();

const allAchievements = ref<AchievementInfo[]>([]);
const selected = ref<AchievementInfo | null>(null);
const loading = ref(false);
const selectedIndex = ref(0);
const listRef = ref<HTMLElement | null>(null);

const { query, filtered } = useDataBrowserSearch(allAchievements, {
  searchText: (ach) => combineFields(ach.name, ach.description, ach.id),
});

onMounted(async () => {
  if (store.status === "ready") {
    await loadAll();
  }
});

watch(() => store.status, async (newStatus) => {
  if (newStatus === "ready") {
    await loadAll();
  }
});

async function loadAll() {
  loading.value = true;
  try {
    allAchievements.value = await store.getAllAchievements();
  } finally {
    loading.value = false;
  }
}

watch(filtered, () => {
  selectedIndex.value = 0;
});

useKeyboard({
  listNavigation: {
    items: filtered,
    selectedIndex,
    onConfirm: (index: number) => {
      const ach = filtered.value[index];
      if (ach) selectAchievement(ach);
    },
    scrollContainerRef: listRef,
  },
});

function selectAchievement(ach: AchievementInfo) {
  selected.value = ach;
}

function clearSelection() {
  selected.value = null;
}
</script>
