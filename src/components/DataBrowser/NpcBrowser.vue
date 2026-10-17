<template>
  <PaneLayout screen-key="db-npcs" :left-pane="{ title: 'NPCs', defaultWidth: 360, minWidth: 280, maxWidth: 500 }">
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
        <!-- Area filter -->
        <div class="flex gap-2">
          <select v-model="selectedArea" class="input flex-1 cursor-pointer">
            <option value="All Areas">All Areas</option>
            <option v-for="area in availableAreas" :key="area" :value="area">
              {{ area }}
            </option>
          </select>
        </div>

        <!-- Search bar -->
        <div class="flex items-center gap-2 relative">
          <input
            v-model="query"
            class="input flex-1"
            placeholder="Search NPCs…"
            autofocus />
          <span v-if="loading" class="text-accent-gold text-sm animate-spin">⟳</span>
          <span v-else-if="filteredNpcs.length" class="text-text-dim text-xs min-w-6 text-right">{{
            filteredNpcs.length
          }}</span>
        </div>

        <div v-if="!allNpcs.length && !loading" class="text-text-dim text-xs italic py-1">
          No NPCs loaded
        </div>

        <div v-else-if="filteredNpcs.length === 0" class="text-text-dim text-xs italic py-1">
          No NPCs found
        </div>

        <ul v-else ref="listRef" class="list-none m-0 p-0 overflow-y-auto flex-1 border border-surface-elevated">
          <li
            v-for="(npc, idx) in filteredNpcs"
            :key="npc.key"
            class="flex flex-col gap-0.5 px-2 py-1.5 cursor-pointer border-b border-surface-dark text-xs hover:bg-surface-row-hover"
            :class="{
              'bg-surface-card border-l-2 border-l-accent-gold': selected?.key === npc.key,
              'bg-surface-elevated': selectedIndex === idx && selected?.key !== npc.key,
            }"
            @click="selectNpc(npc)">
            <span class="text-text-primary/75 flex-1">{{ npc.name }}</span>
            <span v-if="npc.area_friendly_name" class="text-text-dim text-[0.72rem]">{{
              npc.area_friendly_name
            }}</span>
          </li>
        </ul>
      </div>
      </template>
    </template>

    <!-- Right panel: NPC detail -->
    <div
      class="h-full overflow-y-auto border-l border-surface-elevated flex flex-col"
      :class="{ 'items-center justify-center': !selected }">
        <div v-if="!selected" class="text-border-default italic">
          Select an NPC to inspect
        </div>

        <template v-else>
          <!-- Data-browser-specific toolbar: favorite + close -->
          <div class="flex items-center justify-end gap-1 px-4 pt-3 pb-0">
            <button
              class="bg-transparent border-none cursor-pointer px-1 py-0 text-sm shrink-0 transition-colors"
              :class="isFav ? 'text-accent-gold' : 'text-text-dim hover:text-accent-gold'"
              :title="isFav ? 'Remove from favorites' : 'Add to favorites'"
              @click="dataBrowserStore.toggleFavorite({ type: 'npc', reference: selected.key, label: selected.name })"
            >&#x2605;</button>
            <button class="bg-transparent border-none text-text-dim cursor-pointer px-1 py-0 text-sm shrink-0 hover:text-accent-red" @click="clearSelection">&#x2715;</button>
          </div>

          <!-- Shared NPC detail panel (favor, vendor status, storage, gifts, preferences, quests, skills) -->
          <NpcDetailPanel
            :npc-key="selected.key"
            :snapshot-tier="selectedSnapshotTier"
            :gamestate-favor="selectedGamestateFavor"
            :cdn-data="selected"
            :vendor-status="selectedVendorStatus" />

          <!-- Data-browser extras -->
          <div class="flex flex-col gap-4 px-4 pb-4">
            <!-- Vendor Inventory (items they sell with prices) -->
            <div v-if="vendorItems.length" class="flex flex-col gap-1.5">
              <div class="text-[0.65rem] uppercase tracking-widest text-text-dim border-b border-surface-card pb-0.5">Sells Items ({{ vendorItems.length }})</div>
              <div class="max-h-80 overflow-y-auto border border-surface-dark">
                <DataTable
                  :columns="vendorColumns"
                  :rows="(vendorItems as unknown as Record<string, unknown>[])"
                  compact>
                  <template #cell-item_id="{ row }">
                    <ItemInline :reference="String(row.item_id)" />
                  </template>
                  <template #cell-value="{ row }">
                    <span class="text-text-muted font-mono">
                      <template v-if="row.value">{{ Math.ceil((row.value as number) * 1.5) }}c</template>
                      <span v-else class="text-text-dim">--</span>
                    </span>
                  </template>
                </DataTable>
              </div>
            </div>

            <!-- Raw JSON -->
            <div v-if="settingsStore.settings.showRawJsonInDataBrowser" class="flex flex-col gap-1.5">
              <div class="text-[0.65rem] uppercase tracking-widest text-text-dim border-b border-surface-card pb-0.5">Raw JSON</div>
              <pre class="bg-surface-dark border border-surface-card p-3 text-[0.72rem] text-text-muted overflow-x-auto whitespace-pre m-0 leading-relaxed">{{ JSON.stringify(selected, null, 2) }}</pre>
            </div>
          </div>
        </template>
    </div>
  </PaneLayout>
</template>

<script setup lang="ts">
import PaneLayout from "../Shared/PaneLayout.vue";
import NpcDetailPanel from "../Character/NpcDetailPanel.vue";
import ItemInline from "../Shared/Item/ItemInline.vue";
import DataTable, { type ColumnDef } from "../Shared/DataTable.vue";
import { ref, watch, computed } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { useGameDataStore } from "../../stores/gameDataStore";
import { useGameStateStore } from "../../stores/gameStateStore";
import { useCharacterStore } from "../../stores/characterStore";
import { useSettingsStore } from "../../stores/settingsStore";
import { useKeyboard } from "../../composables/useKeyboard";
import { useDataBrowserSearch, checkAreaFilter } from "../../composables/useDataBrowserSearch";
import { combineFields } from "../../utils/SearchParser";
import { useDataBrowserStore } from "../../stores/dataBrowserStore";
import type { EntityNavigationTarget } from "../../composables/useEntityNavigation";
import type { NpcInfo } from "../../types/gameData";

interface VendorItemSummary {
  item_id: number;
  name: string;
  value: number | null;
  icon_id: number | null;
}

const props = defineProps<{
  navTarget?: EntityNavigationTarget | null;
}>();

const store = useGameDataStore();
const gameState = useGameStateStore();
const characterStore = useCharacterStore();
const settingsStore = useSettingsStore();
const dataBrowserStore = useDataBrowserStore();

const isFav = computed(() =>
  selected.value ? dataBrowserStore.isFavorite("npc", selected.value.key) : false
);

const selectedSnapshotTier = computed(() => {
  if (!selected.value) return null;
  const snap = characterStore.npcFavor.find(f => f.npc_key === selected.value!.key);
  return snap?.favor_level ?? null;
});

const selectedGamestateFavor = computed(() => {
  if (!selected.value) return null;
  return gameState.favorByNpc[selected.value.key] ?? null;
});

const selectedVendorStatus = computed(() => {
  if (!selected.value) return null;
  return gameState.vendorByNpc[selected.value.key] ?? null;
});

const selectedArea = ref<string>("All Areas");
const selected = ref<NpcInfo | null>(null);
const vendorItems = ref<VendorItemSummary[]>([]);

const vendorColumns: ColumnDef[] = [
  { key: 'item_id', label: 'Item' },
  { key: 'value', label: 'Value', align: 'right', numeric: true },
];
const selectedIndex = ref(0);
const listRef = ref<HTMLElement | null>(null);

const allNpcs = computed(() =>
  Object.values(store.npcsByKey).sort((a, b) => a.name.localeCompare(b.name))
);
const loading = computed(() => allNpcs.value.length === 0 && store.status === 'loading');

// Get unique areas for the filter dropdown
const availableAreas = computed(() => {
  const areas = new Set<string>();
  let hasUnknown = false;
  allNpcs.value.forEach(npc => {
    if (npc.area_friendly_name) {
      areas.add(npc.area_friendly_name);
    } else {
      hasUnknown = true;
    }
  });
  const sorted = Array.from(areas).sort();
  if (hasUnknown) sorted.push('Unknown Area');
  return sorted;
});

// Pre-filter by area dropdown, then use unified search for text
const areaFiltered = computed(() => {
  if (selectedArea.value === "All Areas") return allNpcs.value;
  if (selectedArea.value === "Unknown Area") return allNpcs.value.filter(npc => !npc.area_friendly_name);
  return allNpcs.value.filter(npc => npc.area_friendly_name === selectedArea.value);
});

const { query, filtered: filteredNpcs } = useDataBrowserSearch(areaFiltered, {
  searchText: (npc) => combineFields(npc.name, npc.desc, npc.area_friendly_name),
  applyFilters: (npc, q) => checkAreaFilter(npc.area_friendly_name, q),
});

watch(filteredNpcs, () => {
  selectedIndex.value = 0;
});

function selectNpc(npc: NpcInfo) {
  selected.value = npc;
  dataBrowserStore.addToHistory({ type: "npc", reference: npc.key, label: npc.name });
  vendorItems.value = [];

  invoke<VendorItemSummary[]>('get_npc_vendor_items', { npcKey: npc.key })
    .then(items => { vendorItems.value = items; })
    .catch(e => { console.warn("Vendor items fetch failed:", e); });
}

function clearSelection() {
  selected.value = null;
  vendorItems.value = [];
}

useKeyboard({
  listNavigation: {
    items: filteredNpcs,
    selectedIndex,
    onConfirm: (index: number) => selectNpc(filteredNpcs.value[index]),
    scrollContainerRef: listRef,
  },
});

// Navigate to a specific NPC when navTarget changes
watch(() => props.navTarget, (target) => {
  if (!target || target.type !== 'npc') return;
  const key = String(target.id);
  if (selected.value?.key === key) return;

  const match = store.resolveNpcSync(key);
  if (match) {
    query.value = match.name;
    selectNpc(match);
  }
}, { immediate: true });
</script>
