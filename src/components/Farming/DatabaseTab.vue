<template>
  <div class="flex flex-col gap-4 h-full overflow-hidden">
    <!-- Scope + import/export toolbar -->
    <div class="flex items-center justify-between gap-3 flex-wrap">
      <div
        class="flex items-center gap-1 bg-surface-dark border border-border-default rounded-lg p-1"
        :class="scopeDisabled ? 'opacity-40' : ''"
        :title="scopeDisabled ? 'Harvested items are recorded locally only — no scope.' : ''">
        <button
          v-for="opt in scopeOptions"
          :key="opt.value"
          @click="scope = opt.value"
          :disabled="scopeDisabled"
          :class="[
            'px-3 py-1.5 text-xs rounded font-medium transition-colors',
            scopeDisabled ? 'cursor-not-allowed' : 'cursor-pointer',
            scope === opt.value
              ? 'bg-entity-item/20 text-entity-item'
              : 'text-text-secondary hover:text-text-primary'
          ]">
          {{ opt.label }}
        </button>
      </div>

      <!-- Equipped Skills loadout selector — filters drop tables to data collected
           under the chosen combat-skill pair. Defaults to your in-game loadout. -->
      <div
        class="flex flex-col items-center gap-1"
        :class="loadoutDisabled ? 'opacity-40 pointer-events-none' : ''"
        :title="loadoutDisabled ? 'Harvested items aren’t affected by combat skills.' : ''">
        <span class="text-[0.6rem] uppercase tracking-widest text-text-dim font-bold leading-none">Equipped Skills</span>
        <div class="flex items-center gap-1">
          <select
            v-model="selectedSkill1"
            @change="onLoadoutChange"
            class="px-2 py-1 text-xs bg-surface-card border border-border-light rounded text-text-primary outline-none focus:border-entity-item cursor-pointer max-w-[8.5rem]">
            <option value="">— Any —</option>
            <option v-for="s in combatSkillOptions" :key="s.internal_name" :value="s.internal_name">{{ s.name }}</option>
          </select>
          <span class="text-text-dim text-xs">+</span>
          <select
            v-model="selectedSkill2"
            @change="onLoadoutChange"
            class="px-2 py-1 text-xs bg-surface-card border border-border-light rounded text-text-primary outline-none focus:border-entity-item cursor-pointer max-w-[8.5rem]">
            <option value="">— Any —</option>
            <option v-for="s in combatSkillOptions" :key="s.internal_name" :value="s.internal_name">{{ s.name }}</option>
          </select>
          <button
            v-if="liveLoadoutAvailable"
            @click="resetToLiveLoadout"
            title="Reset to your in-game equipped skills"
            class="text-text-dim hover:text-entity-item cursor-pointer text-sm leading-none px-1">↺</button>
          <button
            v-if="selectedSkill1 || selectedSkill2"
            @click="clearLoadout"
            title="Set both skills to Any — show data from every loadout"
            class="text-text-dim hover:text-entity-item cursor-pointer text-sm leading-none px-1">✕</button>
        </div>
      </div>

      <div class="flex items-center gap-2">
        <select
          v-model="exportFormat"
          title="Export file format — CSV opens in a spreadsheet; SQLite is a portable, loss-free .db file."
          class="px-2 py-1.5 text-xs bg-surface-card border border-border-light rounded text-text-secondary outline-none focus:border-entity-item cursor-pointer">
          <option value="csv">CSV</option>
          <option value="sqlite">SQLite</option>
        </select>
        <button
          @click="doExport"
          :disabled="exporting"
          class="px-3 py-1.5 text-xs bg-[#2a2a3a]! border border-[#4a4a5a]! rounded text-text-secondary cursor-pointer transition-all font-medium hover:bg-[#3a3a4e] hover:text-text-primary disabled:opacity-50 disabled:cursor-not-allowed">
          {{ exporting ? "Exporting…" : "Export My Data" }}
        </button>
        <button
          @click="doImport"
          :disabled="importing"
          class="px-3 py-1.5 text-xs bg-[#2a3a2a]! border border-[#4a5a4a]! rounded text-value-positive! cursor-pointer transition-all font-medium hover:bg-[#3a4a3a] disabled:opacity-50 disabled:cursor-not-allowed">
          {{ importing ? "Importing…" : "Import Database" }}
        </button>
      </div>
    </div>

    <p v-if="importMessage" class="text-xs text-value-positive">{{ importMessage }}</p>
    <p v-if="errorMessage" class="text-xs text-value-negative">{{ errorMessage }}</p>

    <!-- New kill/loot tracking stopped with the 10/21 game client -->
    <div class="text-[11px] text-text-dim italic"
      title="The current game client no longer writes the loot lines glogger used for new kills and drops. Imported databases still merge.">
      New kills and loot stopped with the 10/21 game client — historic and imported data below. Loot drops are no longer tracked.
    </div>


    <!-- Search / filter -->
    <div class="flex items-center gap-3">
      <div class="flex items-center gap-1 bg-surface-dark border border-border-default rounded-lg p-1 shrink-0">
        <button
          v-for="opt in searchTargetOptions"
          :key="opt.value"
          @click="searchTarget = opt.value"
          :class="[
            'px-3 py-1.5 text-xs rounded font-medium transition-colors cursor-pointer',
            searchTarget === opt.value
              ? 'bg-entity-item/20 text-entity-item'
              : 'text-text-secondary hover:text-text-primary'
          ]">
          {{ opt.label }}
        </button>
      </div>
      <div class="relative flex-1">
        <input
          v-model="query"
          type="text"
          :placeholder="filterPlaceholder"
          class="w-full px-3 py-2 pr-8 text-sm bg-surface-card border border-border-light rounded text-text-primary placeholder-text-dim outline-none focus:border-entity-item"
        />
        <button
          v-if="query"
          @click="query = ''"
          class="absolute right-2 top-1/2 -translate-y-1/2 text-text-dim hover:text-text-primary cursor-pointer text-sm leading-none"
          title="Clear filter">
          ✕
        </button>
      </div>
    </div>

    <!-- Results -->
    <div class="flex-1 min-h-0 overflow-y-auto">
      <EmptyState
        v-if="loading && results.length === 0"
        variant="compact"
        primary="Loading…"
        secondary="Reading your drop-rate database." />
      <EmptyState
        v-else-if="results.length === 0 && query.trim().length > 0"
        variant="compact"
        primary="No matches"
        :secondary="`No ${entityNoun} contains “${query.trim()}”.`" />
      <EmptyState
        v-else-if="results.length === 0"
        variant="compact"
        :primary="emptyPrimary"
        :secondary="emptySecondary" />

      <div v-else class="flex flex-col gap-1">
        <div class="text-[0.65rem] text-text-dim px-1 pb-0.5">
          {{ results.length.toLocaleString() }}
          {{ entityNoun }}{{ results.length === 1 ? '' : 's' }}
          <template v-if="query.trim()">matching “{{ query.trim() }}”</template>
        </div>

        <!-- Enemy results — one row per (monster, zone) -->
        <template v-if="searchTarget === 'enemies'">
          <div
            v-for="row in enemyResults"
            :key="enemyRowKey(row)"
            class="rounded text-xs bg-black/20 border border-border-default">
            <div
              class="flex items-center justify-between px-3 py-2 cursor-pointer hover:bg-black/30 transition-colors"
              @click="toggleEnemyExpanded(enemyRowKey(row))">
              <div class="flex items-center gap-2 min-w-0">
                <EnemyInline :reference="row.enemy_name" />
                <AreaInline v-if="row.zone" :reference="row.zone" class="text-[0.65rem] text-text-dim truncate" />
                <span v-else class="text-[0.65rem] text-text-dim italic truncate">Unknown zone</span>
              </div>
              <div class="flex items-center gap-3 shrink-0">
                <span class="text-text-secondary">{{ row.total_kills.toLocaleString() }} kills</span>
                <span class="text-text-dim">{{ row.distinct_loot_items }} loot items</span>
                <span class="text-text-dim">{{ expandedEnemies.has(enemyRowKey(row)) ? '▲' : '▼' }}</span>
              </div>
            </div>
            <div v-if="expandedEnemies.has(enemyRowKey(row))" class="border-t border-border-default px-3 py-2">
              <EnemyDropTable :enemy-name="row.enemy_name" :scope="scope" :zone="row.zone" :combat-skills="selectedLoadout" />
            </div>
          </div>
        </template>

        <!-- Item results -->
        <template v-else-if="searchTarget === 'items'">
          <div
            v-for="row in itemResults"
            :key="row.item_name"
            class="rounded text-xs bg-black/20 border border-border-default">
            <div
              class="flex items-center justify-between px-3 py-2 cursor-pointer hover:bg-black/30 transition-colors"
              @click="toggleItemExpanded(row.item_name)">
              <ItemInline :reference="row.item_name" />
              <div class="flex items-center gap-3 shrink-0">
                <span class="text-text-secondary">{{ row.total_quantity.toLocaleString() }} looted</span>
                <span class="text-text-dim">{{ row.distinct_enemies }} sources</span>
                <span class="text-text-dim">{{ expandedItems.has(row.item_name) ? '▲' : '▼' }}</span>
              </div>
            </div>
            <div v-if="expandedItems.has(row.item_name)" class="border-t border-border-default px-3 py-2">
              <ItemDropBreakdownTable :item-name="row.item_name" :scope="scope" :combat-skills="selectedLoadout" />
            </div>
          </div>
        </template>

        <!-- Harvested results (skinning/butchering) -->
        <template v-else>
          <div
            v-for="row in harvestedResults"
            :key="row.item_name"
            class="rounded text-xs bg-black/20 border border-border-default">
            <div
              class="flex items-center justify-between px-3 py-2 cursor-pointer hover:bg-black/30 transition-colors"
              @click="toggleHarvestedExpanded(row.item_name)">
              <ItemInline :reference="row.item_name" />
              <div class="flex items-center gap-3 shrink-0">
                <span class="text-text-secondary">{{ row.total_quantity.toLocaleString() }} harvested</span>
                <span class="text-text-dim">{{ row.distinct_corpses }} {{ row.distinct_corpses === 1 ? 'corpse' : 'corpses' }}</span>
                <span class="text-text-dim">{{ expandedHarvested.has(row.item_name) ? '▲' : '▼' }}</span>
              </div>
            </div>
            <div v-if="expandedHarvested.has(row.item_name)" class="border-t border-border-default px-3 py-2">
              <ExtractDetailTable :item-name="row.item_name" />
            </div>
          </div>
        </template>
      </div>
    </div>

    <!-- Imported sources management -->
    <div v-if="importedSources.length > 0" class="bg-surface-dark border border-border-default rounded-lg p-3 max-h-40 overflow-y-auto shrink-0">
      <div class="text-[0.65rem] uppercase tracking-widest text-text-dim font-bold mb-1">Imported Sources</div>
      <div class="text-[0.65rem] text-text-dim mb-2">Removing an entry only clears it from this list — the imported data stays merged into your database.</div>
      <div class="flex flex-col gap-1">
        <div
          v-for="src in importedSources"
          :key="src.source_label"
          class="flex items-center justify-between px-2 py-1.5 rounded text-xs bg-black/20 border border-border-default">
          <span class="text-text-secondary truncate">{{ src.display_name }}</span>
          <div class="flex items-center gap-3 shrink-0">
            <span class="text-text-dim">{{ src.enemy_count }} enemies</span>
            <span class="text-text-dim">{{ formatDate(src.imported_at) }}</span>
            <button
              @click="removeSource(src.source_label)"
              class="text-value-negative hover:text-[#e87e7e] cursor-pointer">
              Remove
            </button>
          </div>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, watch, onMounted } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { save, open } from "@tauri-apps/plugin-dialog";
import type {
  DatabaseScope,
  EnemySearchResult,
  ItemSearchResult,
  HarvestSearchResult,
  ImportSummary,
  ImportedSource,
} from "../../types/farming";
import EmptyState from "../Shared/EmptyState.vue";
import ItemInline from "../Shared/Item/ItemInline.vue";
import EnemyInline from "../Shared/Enemy/EnemyInline.vue";
import AreaInline from "../Shared/Area/AreaInline.vue";
import EnemyDropTable from "./EnemyDropTable.vue";
import ItemDropBreakdownTable from "./ItemDropBreakdownTable.vue";
import ExtractDetailTable from "./ExtractDetailTable.vue";
import { formatDateTimeShort } from "../../composables/useTimestamp";
import { useGameDataStore } from "../../stores/gameDataStore";
import { useGameStateStore } from "../../stores/gameStateStore";
import { useSettingsStore } from "../../stores/settingsStore";
import { useViewPrefs } from "../../composables/useViewPrefs";
import type { SkillInfo } from "../../types/gameData/skills";

const scope = ref<DatabaseScope>("combined");
const scopeOptions: Array<{ value: DatabaseScope; label: string }> = [
  { value: "mine", label: "My Data" },
  { value: "imported", label: "Imported" },
  { value: "combined", label: "Combined" },
];

type SearchTarget = "enemies" | "items" | "harvested";
const searchTarget = ref<SearchTarget>("enemies");
const searchTargetOptions: Array<{ value: SearchTarget; label: string }> = [
  { value: "enemies", label: "Monsters" },
  { value: "items", label: "Items" },
  { value: "harvested", label: "Harvested" },
];

// Scope only applies to kill/loot data (mine vs imported); harvested extracts
// are recorded locally only.
const scopeDisabled = computed(() => searchTarget.value === "harvested");

// Full lists for the current target + scope, loaded up front. The text box
// filters these client-side so results isolate instantly as you type.
const query = ref("");
const loading = ref(false);
const allEnemies = ref<EnemySearchResult[]>([]);
const allItems = ref<ItemSearchResult[]>([]);
const allHarvested = ref<HarvestSearchResult[]>([]);

const enemyResults = computed(() => {
  const q = query.value.trim().toLowerCase();
  if (!q) return allEnemies.value;
  return allEnemies.value.filter((r) => r.enemy_name.toLowerCase().includes(q));
});
// A monster appears once per zone, so rows are identified by monster + zone.
function enemyRowKey(row: EnemySearchResult): string {
  return `${row.enemy_name}|${row.zone ?? ""}`;
}
const itemResults = computed(() => {
  const q = query.value.trim().toLowerCase();
  if (!q) return allItems.value;
  return allItems.value.filter((r) => r.item_name.toLowerCase().includes(q));
});
const harvestedResults = computed(() => {
  const q = query.value.trim().toLowerCase();
  if (!q) return allHarvested.value;
  return allHarvested.value.filter((r) => r.item_name.toLowerCase().includes(q));
});
const results = computed(() => {
  if (searchTarget.value === "enemies") return enemyResults.value;
  if (searchTarget.value === "items") return itemResults.value;
  return harvestedResults.value;
});

const entityNoun = computed(() =>
  searchTarget.value === "enemies" ? "monster" : searchTarget.value === "items" ? "item" : "harvested item",
);
const filterPlaceholder = computed(() =>
  searchTarget.value === "enemies"
    ? "Filter monsters…"
    : searchTarget.value === "items"
      ? "Filter items…"
      : "Filter harvested items…",
);
const emptyPrimary = computed(() =>
  searchTarget.value === "enemies"
    ? "No monsters recorded yet"
    : searchTarget.value === "items"
      ? "No items recorded yet"
      : "No harvested items yet",
);
const emptySecondary = computed(() =>
  searchTarget.value === "harvested"
    ? "Harvest records are historic only — the 10/21 client stopped the lines that filled this list. Import a database to add more."
    : "Drop records are historic only — the 10/21 client stopped new kills and loot. Import a database to add more.",
);

const expandedEnemies = ref<Set<string>>(new Set());
const expandedItems = ref<Set<string>>(new Set());
const expandedHarvested = ref<Set<string>>(new Set());

function toggleEnemyExpanded(name: string) {
  if (expandedEnemies.value.has(name)) expandedEnemies.value.delete(name);
  else expandedEnemies.value.add(name);
  expandedEnemies.value = new Set(expandedEnemies.value);
}

function toggleItemExpanded(name: string) {
  if (expandedItems.value.has(name)) expandedItems.value.delete(name);
  else expandedItems.value.add(name);
  expandedItems.value = new Set(expandedItems.value);
}

function toggleHarvestedExpanded(name: string) {
  if (expandedHarvested.value.has(name)) expandedHarvested.value.delete(name);
  else expandedHarvested.value.add(name);
  expandedHarvested.value = new Set(expandedHarvested.value);
}

// ── Equipped Skills (combat loadout) filter ─────────────────────────────
const gameDataStore = useGameDataStore();
const gameStateStore = useGameStateStore();
const settingsStore = useSettingsStore();

// Persisted export prefs: the serial counter (drives the suggested filename) and
// the chosen share format (CSV by default, or a portable SQLite .db file).
type ExportFormat = "csv" | "sqlite";
const { prefs: exportPrefs, update: updateExportPrefs } = useViewPrefs("database", {
  exportSerial: 0,
  exportFormat: "csv" as ExportFormat,
});
const exportFormat = ref<ExportFormat>(exportPrefs.value.exportFormat ?? "csv");
watch(exportFormat, (f) => updateExportPrefs({ exportFormat: f }));

const combatSkillOptions = ref<SkillInfo[]>([]);
const selectedSkill1 = ref("");
const selectedSkill2 = ref("");
// True once the user hand-picks a loadout — stops auto-following the live one.
const manualOverride = ref(false);

// Loadout only applies to kill/loot drop tables, not harvested extracts.
const loadoutDisabled = computed(() => searchTarget.value === "harvested");

// The character's current in-game loadout as internal skill names. Reactive:
// gameStateStore.activeSkills is refreshed live by the `active_skills`
// game-state-updated event, so this follows skill swaps / zone reloads.
const liveLoadout = computed<{ s1: string; s2: string }>(() => {
  const a = gameStateStore.activeSkills;
  if (!a) return { s1: "", s2: "" };
  const byId = (id: number) => combatSkillOptions.value.find((s) => s.id === id)?.internal_name ?? "";
  return { s1: byId(a.skill1_id), s2: byId(a.skill2_id) };
});
const liveLoadoutAvailable = computed(() => !!(liveLoadout.value.s1 || liveLoadout.value.s2));

// Normalized, order-independent loadout key (matches the backend's `skillA+skillB`).
// Both slots must be set to form a valid pair; otherwise no loadout filter.
const selectedLoadout = computed<string | null>(() => {
  const parts = [selectedSkill1.value, selectedSkill2.value].filter((s) => !!s);
  if (parts.length < 2) return null;
  return [...parts].sort().join("+");
});

async function loadCombatSkills() {
  try {
    const all = await gameDataStore.getAllSkills();
    combatSkillOptions.value = all
      .filter((s) => s.combat === true)
      .sort((a, b) => a.name.localeCompare(b.name));
  } catch (e) {
    console.error("[database-tab] Failed to load combat skills:", e);
  }
}

// Auto-follow the live in-game loadout until the user overrides it manually.
watch(
  liveLoadout,
  (lv) => {
    if (!manualOverride.value) {
      selectedSkill1.value = lv.s1;
      selectedSkill2.value = lv.s2;
    }
  },
  { immediate: true },
);

// A manual dropdown change pins the selection (stops auto-follow). The reload is
// driven by the [searchTarget, scope, selectedLoadout] watch below.
function onLoadoutChange() {
  manualOverride.value = true;
}

// Clear both slots to "Any" — drops the loadout filter entirely. Counts as a
// manual pick so it isn't immediately overwritten by the live loadout again.
function clearLoadout() {
  manualOverride.value = true;
  selectedSkill1.value = "";
  selectedSkill2.value = "";
}

// Snap back to the live in-game loadout and resume auto-following.
function resetToLiveLoadout() {
  manualOverride.value = false;
  selectedSkill1.value = liveLoadout.value.s1;
  selectedSkill2.value = liveLoadout.value.s2;
}

// Load the full list for the active target + scope (empty query, no limit).
async function loadDatabase() {
  loading.value = true;
  try {
    if (searchTarget.value === "enemies") {
      allEnemies.value = await invoke<EnemySearchResult[]>("search_database_enemies", {
        query: "",
        scope: scope.value,
        limit: null,
        combatSkills: selectedLoadout.value,
      });
    } else if (searchTarget.value === "items") {
      allItems.value = await invoke<ItemSearchResult[]>("search_database_items", {
        query: "",
        scope: scope.value,
        limit: null,
        combatSkills: selectedLoadout.value,
      });
    } else {
      allHarvested.value = await invoke<HarvestSearchResult[]>("search_database_harvested", {
        query: "",
        limit: null,
      });
    }
  } catch (e) {
    console.error("[database-tab] Failed to load database:", e);
  } finally {
    loading.value = false;
  }
}

// Reload whenever the box (Monsters/Items/Harvested), scope, or equipped-skill
// loadout changes; clear stale expansions.
watch([searchTarget, scope, selectedLoadout], () => {
  expandedEnemies.value = new Set();
  expandedItems.value = new Set();
  expandedHarvested.value = new Set();
  loadDatabase();
});

// ── Import / Export ─────────────────────────────────────────────────────

const exporting = ref(false);
const importing = ref(false);
const importMessage = ref("");
const errorMessage = ref("");
const importedSources = ref<ImportedSource[]>([]);

async function loadImportedSources() {
  try {
    importedSources.value = await invoke<ImportedSource[]>("list_imported_sources");
  } catch (e) {
    console.error("[database-tab] Failed to load imported sources:", e);
  }
}

// Suggested export filename: "<charname>-<NNNN>.<ext>", where NNNN is a 4-digit
// serial that increments each successful export (e.g. oddio-0001, oddio-0002) and
// <ext> follows the chosen format (csv or db). The counter persists per-install in
// view prefs; the character name is the active character (sanitized, lowercased)
// or "glogger" when none is loaded.
function exportFileName(serial: number): string {
  const raw = settingsStore.settings.activeCharacterName || "glogger";
  const name = raw.toLowerCase().replace(/[^a-z0-9_-]+/g, "") || "glogger";
  const ext = exportFormat.value === "sqlite" ? "db" : "csv";
  return `${name}-${String(serial).padStart(4, "0")}.${ext}`;
}

async function doExport() {
  errorMessage.value = "";
  importMessage.value = "";
  exporting.value = true;
  try {
    const nextSerial = (exportPrefs.value.exportSerial ?? 0) + 1;
    const filters =
      exportFormat.value === "sqlite"
        ? [{ name: "SQLite Database", extensions: ["db"] }]
        : [{ name: "CSV", extensions: ["csv"] }];
    const filePath = await save({
      filters,
      defaultPath: exportFileName(nextSerial),
    });
    if (!filePath) return;
    const count = await invoke<number>("export_kill_loot_database", { path: filePath });
    // Only advance the serial once the export actually succeeds.
    updateExportPrefs({ exportSerial: nextSerial });
    importMessage.value = `Exported ${count} enemies' drop data to ${filePath}.`;
  } catch (e) {
    errorMessage.value = `Export failed: ${e}`;
  } finally {
    exporting.value = false;
  }
}

async function doImport() {
  errorMessage.value = "";
  importMessage.value = "";
  importing.value = true;
  try {
    const filePath = await open({
      filters: [{ name: "Drop data", extensions: ["csv", "json", "db", "sqlite", "sqlite3"] }],
      multiple: false,
    });
    if (!filePath) return;
    const summary = await invoke<ImportSummary>("import_kill_loot_database", { path: filePath as string });
    importMessage.value = `Merged ${summary.enemies_imported} enemies (${summary.loot_rows_imported} loot rows) from "${summary.source_label}" into your database.`;
    await loadImportedSources();
    await loadDatabase();
  } catch (e) {
    errorMessage.value = `Import failed: ${e}`;
  } finally {
    importing.value = false;
  }
}

async function removeSource(sourceLabel: string) {
  try {
    await invoke("delete_imported_source", { sourceLabel });
    await loadImportedSources();
  } catch (e) {
    console.error("[database-tab] Failed to remove import source:", e);
  }
}

function formatDate(isoStr: string): string {
  return formatDateTimeShort(isoStr);
}

onMounted(async () => {
  await loadCombatSkills();
  loadDatabase();
  loadImportedSources();
});
</script>
