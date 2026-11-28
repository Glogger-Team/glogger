<template>
  <PaneLayout
    screen-key="crafting-projects"
    :left-pane="{ title: 'Projects', defaultWidth: 220, minWidth: 180, maxWidth: 350 }"
    :right-pane="rightPaneConfig">
    <template #left>
      <ProjectSidebar />
    </template>

    <!-- Center: materials panel -->
    <ProjectMaterialsPanel
      :active-project="store.activeProject"
      :active-group-name="store.activeGroupName"
      :group-project-names="groupProjectNames"
      :group-entries="groupEntries"
      :stock-targets="stockTargets"
      :materials="projectMaterials"
      :intermediates="projectIntermediates"
      :expanded-item-ids="expandedItemIds"
      :intermediate-stock="intermediateStockMap"
      :material-needs="materialNeeds"
      :resolving="resolvingAll"
      :checking-availability="checkingAvailability"
      :pricing-mode="pricingMode"
      :customer-provides="localCustomerProvides"
      :pricing-calculation="pricingCalculation"
      :consume-buffer-pct="store.consumeBufferPct"
      @resolve="onResolve"
      @toggle-intermediate="toggleIntermediateGlobal"
      @set-all-intermediates="setAllIntermediates"
      @update-customer-provides="onCustomerProvidesChange"
      @update-buffer="onBufferChange" />

    <template v-if="!store.activeGroupName" #right>
      <ProjectRecipePanel
        :active-project="store.activeProject"
        :intermediate-expansions="intermediateExpansions"
        :stock-targets="stockTargets"
        :pricing-mode="pricingMode"
        :fee-config="localFeeConfig"
        @duplicate="duplicateProject"
        @delete="deleteProject"
        @update-qty="updateEntryQty"
        @remove="(entryId) => store.removeEntry(entryId)"
        @toggle-intermediate="toggleIntermediate"
        @update-target-stock="updateEntryTargetStock"
        @toggle-pricing="pricingMode = !pricingMode"
        @update-fee="onFeeChange"
        @save-defaults="onSaveDefaults"
        @reset-defaults="onResetDefaults" />
    </template>
  </PaneLayout>
</template>

<script setup lang="ts">
import { ref, computed, watch, onMounted } from "vue";
import PaneLayout from "../Shared/PaneLayout.vue";
import { invoke } from "@tauri-apps/api/core";
import { confirm } from "@tauri-apps/plugin-dialog";
import { useGameDataStore } from "../../stores/gameDataStore";
import { useCraftingStore } from "../../stores/craftingStore";
import { useSettingsStore } from "../../stores/settingsStore";
import { useMarketStore } from "../../stores/marketStore";
import { useViewPrefs } from "../../composables/useViewPrefs";
import { usePriceCalculator } from "../../composables/usePriceCalculator";
import type { FlattenedMaterial, IntermediateCraft, MaterialNeed, FeeConfig } from "../../types/crafting";
import { DEFAULT_FEE_CONFIG } from "../../types/crafting";
import ProjectSidebar from "./ProjectSidebar.vue";
import ProjectRecipePanel from "./ProjectRecipePanel.vue";
import ProjectMaterialsPanel from "./ProjectMaterialsPanel.vue";

const gameData = useGameDataStore();
const settingsStore = useSettingsStore();
const store = useCraftingStore();
const marketStore = useMarketStore();

const { prefs: defaultFeePrefs, update: updateDefaultFeePrefs } = useViewPrefs(
  "price-helper-defaults",
  { fee_config: DEFAULT_FEE_CONFIG as FeeConfig },
);

const { prefs: selectionPrefs, update: updateSelectionPrefs } = useViewPrefs(
  "crafting-projects-selection",
  { lastProjectId: null as number | null, lastGroupName: null as string | null },
);

// Consume-chance safety buffer (%) — persisted via view-prefs and pushed into the
// crafting store, which applies it during ingredient resolution.
const { prefs: bufferPrefs, update: updateBufferPrefs } = useViewPrefs(
  "crafting-buffer",
  { consumeBufferPct: 10 },
);
store.consumeBufferPct = bufferPrefs.value.consumeBufferPct;

function onBufferChange(pct: number) {
  const clamped = Math.max(0, Math.min(100, Math.round(pct) || 0));
  if (clamped === store.consumeBufferPct) return;
  updateBufferPrefs({ consumeBufferPct: clamped });
  store.consumeBufferPct = clamped;
  store.clearCaches();
  onResolve();
}

// ── Right pane config (hidden in group view) ────────────────────────────────

const rightPaneConfig = computed(() => {
  if (store.activeGroupName) return undefined;
  return { title: 'Configuration', defaultWidth: 420, minWidth: 320, maxWidth: 700 };
});

// ── State ─────────────────────────────────────────────────────────────────────

const resolvingAll = ref(false);
const projectMaterials = ref(new Map<string, FlattenedMaterial>());
const projectIntermediates = ref<IntermediateCraft[]>([]);
const checkingAvailability = ref(false);
const materialNeeds = ref<MaterialNeed[]>([]);

const intermediateStockMap = ref(new Map<number, number>());
const stockTargets = ref(new Map<number, { effectiveQty: number; currentStock: number }>());
const groupEntries = ref<import("../../types/crafting").CraftingProjectEntry[]>([]);
const intermediateExpansions = ref(new Map<string, boolean>());

const groupProjectNames = computed(() => {
  if (!store.activeGroupName) return [];
  return store.getProjectsInGroup(store.activeGroupName).map((p) => p.name);
});

const expandedItemIds = computed(() => {
  const ids = new Set<number>();
  for (const [key, value] of intermediateExpansions.value) {
    if (value) {
      const itemId = parseInt(key.split(":")[1], 10);
      if (!isNaN(itemId)) ids.add(itemId);
    }
  }
  return ids;
});

// ── Pricing state ─────────────────────────────────────────────────────────────

const pricingMode = ref(false);
const localFeeConfig = ref<FeeConfig>({ ...DEFAULT_FEE_CONFIG });
const localCustomerProvides = ref<Record<string, number>>({});
const materialPrices = ref(
  new Map<string, { unitPrice: number | null; source: "market" | "craft" | "vendor" | null }>(),
);

const totalCrafts = computed(() => {
  if (!store.activeProject) return 0;
  return store.activeProject.entries.reduce((sum, e) => sum + e.quantity, 0);
});

const { calculation: pricingCalcRaw } = usePriceCalculator(
  projectMaterials,
  materialPrices,
  localCustomerProvides,
  localFeeConfig,
  totalCrafts,
);

const pricingCalculation = computed(() => {
  if (!pricingMode.value) return null;
  return pricingCalcRaw.value;
});

// ── Watchers ──────────────────────────────────────────────────────────────────

watch(() => store.activeProject, (project) => {
  const map = new Map<string, boolean>();
  if (project) {
    for (const entry of project.entries) {
      for (const itemId of entry.expanded_ingredient_ids) {
        map.set(`${entry.id}:${itemId}`, true);
      }
    }
    // Load pricing data from project
    localFeeConfig.value = { ...project.fee_config };
    localCustomerProvides.value = { ...project.customer_provides };
    // Auto-enable pricing mode if project has pricing data configured
    const hasFeeConfig = project.fee_config.per_craft_fee > 0
      || project.fee_config.material_pct > 0
      || project.fee_config.flat_fee > 0;
    const hasCustomerProvides = Object.keys(project.customer_provides).length > 0;
    pricingMode.value = hasFeeConfig || hasCustomerProvides;
  } else {
    localFeeConfig.value = { ...DEFAULT_FEE_CONFIG };
    localCustomerProvides.value = {};
    pricingMode.value = false;
  }
  intermediateExpansions.value = map;

  if (project && project.entries.length > 0) {
    resolveProject();
  } else {
    projectMaterials.value = new Map();
    projectIntermediates.value = [];
    materialNeeds.value = [];
    materialPrices.value = new Map();
  }
}, { immediate: true });

watch(() => store.activeGroupName, async (groupName) => {
  if (!groupName) return;
  intermediateExpansions.value = new Map();
  stockTargets.value = new Map();
  pricingMode.value = false;
  await resolveGroup(groupName);
}, { immediate: false });

// Character switch: project rows are global, but every resolved number —
// materials, needs, intermediate stock, stock targets — is read for the
// active character+server. Clear synchronously under the new identity so the
// old character's availability never renders, then re-resolve; the
// resolveGeneration guard drops any in-flight resolve for the old identity.
watch(
  () => [settingsStore.settings.activeCharacterName, settingsStore.settings.activeServerName],
  () => {
    projectMaterials.value = new Map();
    projectIntermediates.value = [];
    materialNeeds.value = [];
    intermediateStockMap.value = new Map();
    stockTargets.value = new Map();
    onResolve();
  },
)

// Persist selection so it survives navigation
watch(() => store.activeProject, (project) => {
  if (project) updateSelectionPrefs({ lastProjectId: project.id, lastGroupName: null });
});
watch(() => store.activeGroupName, (groupName) => {
  if (groupName) updateSelectionPrefs({ lastProjectId: null, lastGroupName: groupName });
});

// Restore last selection on mount
onMounted(async () => {
  if (store.activeProject || store.activeGroupName) return;
  const { lastProjectId, lastGroupName } = selectionPrefs.value;
  if (lastProjectId != null) {
    try { await store.loadProject(lastProjectId); } catch { /* project may have been deleted */ }
  } else if (lastGroupName) {
    store.selectGroup(lastGroupName);
  }
});

function onResolve() {
  if (store.activeGroupName) {
    resolveGroup(store.activeGroupName);
  } else {
    resolveProject();
  }
}

// ── Actions ───────────────────────────────────────────────────────────────────

async function duplicateProject() {
  if (!store.activeProject) return;
  const newId = await store.duplicateProject(store.activeProject.id);
  await store.loadProject(newId);
}

async function deleteProject() {
  if (!store.activeProject) return;
  const ok = await confirm(`Delete project "${store.activeProject.name}"?`, { title: "Delete Project", kind: "warning" });
  if (!ok) return;
  await store.deleteProject(store.activeProject.id);
  projectMaterials.value = new Map();
  projectIntermediates.value = [];
  materialNeeds.value = [];
}

async function updateEntryQty(entryId: number, qty: number) {
  if (!qty || qty < 1) return;
  const entry = store.activeProject?.entries.find((e) => e.id === entryId);
  await store.updateEntry(entryId, qty, entry?.expanded_ingredient_ids ?? [], entry?.target_stock);
}

async function updateEntryTargetStock(entryId: number, targetStock: number | null) {
  const entry = store.activeProject?.entries.find((e) => e.id === entryId);
  if (!entry) return;
  await store.updateEntry(entryId, entry.quantity, entry.expanded_ingredient_ids, targetStock);
  if (projectMaterials.value.size > 0 || targetStock !== null) {
    resolveProject();
  }
}

/** Collect expanded item IDs for a single entry from the intermediateExpansions map. */
function getExpandedIdsForEntry(entryId: number): number[] {
  const ids: number[] = [];
  for (const [k, v] of intermediateExpansions.value) {
    if (v && k.startsWith(`${entryId}:`)) {
      const id = parseInt(k.split(":")[1], 10);
      if (!isNaN(id)) ids.push(id);
    }
  }
  return ids;
}

/** Persist expansion state for all entries in one IPC call (no project reload). */
async function saveExpansionsToDB(entries: import("../../types/crafting").CraftingProjectEntry[]) {
  const batchEntries = entries.map((entry) => ({
    id: entry.id,
    expanded_ingredient_ids: getExpandedIdsForEntry(entry.id),
  }));
  await invoke("batch_update_entry_expansions", { entries: batchEntries });
}

async function toggleIntermediateGlobal(itemId: number) {
  const entries = store.activeProject?.entries;
  if (!entries) return;

  const isCurrentlyExpanded = expandedItemIds.value.has(itemId);

  // Update local state for all entries
  for (const entry of entries) {
    const key = `${entry.id}:${itemId}`;
    if (isCurrentlyExpanded) {
      intermediateExpansions.value.delete(key);
    } else {
      intermediateExpansions.value.set(key, true);
    }
  }

  // Single batch DB write, then re-resolve
  await saveExpansionsToDB(entries);

  if (projectMaterials.value.size > 0) {
    resolveProject();
  }
}

async function toggleIntermediate(_entryId: number, itemId: number | null) {
  if (itemId === null) return;
  await toggleIntermediateGlobal(itemId);
}

async function setAllIntermediates(itemIds: number[], expand: boolean) {
  const entries = store.activeProject?.entries;
  if (!entries) return;

  for (const itemId of itemIds) {
    for (const entry of entries) {
      const key = `${entry.id}:${itemId}`;
      if (expand) {
        intermediateExpansions.value.set(key, true);
      } else {
        intermediateExpansions.value.delete(key);
      }
    }
  }

  // Single batch DB write, then re-resolve
  await saveExpansionsToDB(entries);

  if (projectMaterials.value.size > 0) {
    resolveProject();
  }
}

// ── Pricing actions ───────────────────────────────────────────────────────────

let pricingSaveTimeout: ReturnType<typeof setTimeout> | null = null;

function savePricingDebounced() {
  if (pricingSaveTimeout) clearTimeout(pricingSaveTimeout);
  pricingSaveTimeout = setTimeout(() => savePricing(), 500);
}

async function savePricing() {
  const project = store.activeProject;
  if (!project) return;
  await store.updateProject(
    project.id,
    project.name,
    project.notes,
    project.group_name,
    localFeeConfig.value,
    localCustomerProvides.value,
  );
}

function onFeeChange(feeConfig: FeeConfig) {
  localFeeConfig.value = feeConfig;
  savePricingDebounced();
}

function onCustomerProvidesChange(key: string, quantity: number) {
  if (quantity <= 0) {
    const { [key]: _, ...rest } = localCustomerProvides.value;
    localCustomerProvides.value = rest;
  } else {
    localCustomerProvides.value = { ...localCustomerProvides.value, [key]: quantity };
  }
  savePricingDebounced();
}

function onSaveDefaults() {
  updateDefaultFeePrefs({ fee_config: { ...localFeeConfig.value } });
}

function onResetDefaults() {
  localFeeConfig.value = { ...defaultFeePrefs.value.fee_config };
  savePricingDebounced();
}

// ── Material price resolution (for pricing mode) ─────────────────────────────

async function resolvePrices(materials: Map<string, FlattenedMaterial>, gen: number) {
  const prices = new Map<string, { unitPrice: number | null; source: "market" | "craft" | "vendor" | null }>();

  const itemIds = [...materials.values()].filter((m) => m.item_id !== null).map((m) => m.item_id!);
  const items = itemIds.length > 0 ? await gameData.resolveItemsBatch(itemIds.map(String)) : {};

  if (gen !== resolveGeneration) return;

  for (const [key, mat] of materials) {
    if (mat.item_id === null) {
      prices.set(key, { unitPrice: null, source: null });
      continue;
    }

    const item = items[String(mat.item_id)];
    const marketVal = marketStore.valuesByItemId[mat.item_id];
    const marketPrice = marketVal ? marketVal.market_value : null;
    const vendorPrice = item?.value ? item.value * 2 : null;

    const candidates: { price: number; source: "market" | "vendor" }[] = [];
    if (marketPrice !== null) candidates.push({ price: marketPrice, source: "market" });
    if (vendorPrice !== null) candidates.push({ price: vendorPrice, source: "vendor" });

    if (candidates.length > 0) {
      const best = candidates.reduce((a, b) => (a.price <= b.price ? a : b));
      prices.set(key, { unitPrice: best.price, source: best.source });
    } else {
      prices.set(key, { unitPrice: null, source: null });
    }
  }

  if (gen !== resolveGeneration) return;
  materialPrices.value = prices;
}

// ── Resolve logic ─────────────────────────────────────────────────────────────

let resolveGeneration = 0;

async function resolveGroup(groupName: string) {
  const gen = ++resolveGeneration;

  resolvingAll.value = true;
  projectMaterials.value = new Map();
  projectIntermediates.value = [];
  materialNeeds.value = [];

  try {
    const groupProjects = store.getProjectsInGroup(groupName);
    const projectIds = groupProjects.map((p) => p.id);

    const fullProjects: import("../../types/crafting").CraftingProject[] = [];
    for (const pid of projectIds) {
      const raw = await invoke<any>("get_crafting_project", { projectId: pid });
      if (gen !== resolveGeneration) return;
      fullProjects.push({
        ...raw,
        fee_config: typeof raw.fee_config === 'string' ? JSON.parse(raw.fee_config) : raw.fee_config,
        customer_provides: typeof raw.customer_provides === 'string' ? JSON.parse(raw.customer_provides) : raw.customer_provides,
      });
    }

    const allEntries = fullProjects.flatMap((p) => p.entries);
    groupEntries.value = allEntries;

    const map = new Map<string, boolean>();
    for (const entry of allEntries) {
      for (const itemId of entry.expanded_ingredient_ids) {
        map.set(`${entry.id}:${itemId}`, true);
      }
    }
    intermediateExpansions.value = map;

    const targets = await store.resolveStockTargets(allEntries);
    if (gen !== resolveGeneration) return;
    stockTargets.value = targets;

    const combinedMaterials = new Map<string, FlattenedMaterial>();
    const intermediateMap = new Map<number, IntermediateCraft>();

    const expandItemIds = new Set<number>();
    for (const project of fullProjects) {
      for (const entry of project.entries) {
        for (const itemId of entry.expanded_ingredient_ids) {
          expandItemIds.add(itemId);
        }
      }
    }

    let intermediateStock: Map<number, number> | undefined;
    if (expandItemIds.size > 0) {
      const originalStock = await store.queryItemStock(Array.from(expandItemIds));
      if (gen !== resolveGeneration) return;
      intermediateStockMap.value = originalStock;
      // Pass a mutable copy — the resolver consumes stock as entries are resolved
      // so that the same stock isn't double-counted across entries.
      intermediateStock = new Map(originalStock);
    } else {
      intermediateStockMap.value = new Map();
    }

    for (const entry of allEntries) {
      if (gen !== resolveGeneration) return;

      const targetInfo = targets.get(entry.id);
      const quantity = targetInfo ? targetInfo.effectiveQty : entry.quantity;
      if (quantity <= 0) continue;

      let recipe = await gameData.resolveRecipe(entry.recipe_name);
      if (!recipe) continue;
      // Pin variable slots to this entry's chosen combo, if any, so the resolver
      // reports the exact ingredients rather than generic keyword aggregates.
      if (entry.slot_item_ids && entry.slot_item_ids.length > 0) {
        recipe = store.pinRecipeSlots(recipe, entry.slot_item_ids);
      }

      const resolved = await store.resolveRecipeIngredients(
        recipe,
        quantity,
        false,
        new Set(),
        expandItemIds.size > 0 ? expandItemIds : undefined,
        intermediateStock,
      );

      const entryIntermediates = store.collectIntermediates(resolved.ingredients);
      for (const inter of entryIntermediates) {
        const existing = intermediateMap.get(inter.item_id);
        if (existing) {
          existing.quantity_produced += inter.quantity_produced;
          existing.crafts_needed += inter.crafts_needed;
        } else {
          intermediateMap.set(inter.item_id, { ...inter });
        }
      }

      const flat = store.flattenIngredients(resolved.ingredients);
      for (const [key, mat] of flat) {
        const existing = combinedMaterials.get(key);
        if (existing) {
          existing.quantity += mat.quantity;
          existing.expected_quantity += mat.expected_quantity;
        } else {
          combinedMaterials.set(key, { ...mat });
        }
      }
    }

    if (gen !== resolveGeneration || store.activeGroupName !== groupName) return;

    projectMaterials.value = combinedMaterials;
    projectIntermediates.value = Array.from(intermediateMap.values());
    materialNeeds.value = [];

    if (combinedMaterials.size > 0) {
      await checkProjectAvailability();
    }
  } catch (e) {
    console.error("[crafting] Group resolve failed:", e);
  } finally {
    if (gen === resolveGeneration) {
      resolvingAll.value = false;
    }
  }
}

async function resolveProject() {
  if (!store.activeProject) return;
  const projectId = store.activeProject.id;
  const gen = ++resolveGeneration;

  resolvingAll.value = true;
  projectMaterials.value = new Map();
  projectIntermediates.value = [];

  try {
    const targets = await store.resolveStockTargets(store.activeProject.entries);
    if (gen !== resolveGeneration) return;
    stockTargets.value = targets;

    const combinedMaterials = new Map<string, FlattenedMaterial>();
    const intermediateMap = new Map<number, IntermediateCraft>();

    const expandItemIds = new Set<number>();
    for (const [key, value] of intermediateExpansions.value) {
      if (value) {
        const itemId = parseInt(key.split(":")[1], 10);
        if (!isNaN(itemId)) expandItemIds.add(itemId);
      }
    }

    let intermediateStock: Map<number, number> | undefined;
    if (expandItemIds.size > 0) {
      const originalStock = await store.queryItemStock(Array.from(expandItemIds));
      if (gen !== resolveGeneration) return;
      intermediateStockMap.value = originalStock;
      // Pass a mutable copy — the resolver consumes stock as entries are resolved
      // so that the same stock isn't double-counted across entries.
      intermediateStock = new Map(originalStock);
    } else {
      intermediateStockMap.value = new Map();
    }

    // Memoize recipe lookups by name — a project can hold many entries of the
    // same recipe (e.g. one per brewing combo), and re-resolving each over IPC
    // is the main cost for large projects.
    const recipeByName = new Map<string, Awaited<ReturnType<typeof gameData.resolveRecipe>>>();
    const getRecipe = async (name: string) => {
      if (recipeByName.has(name)) return recipeByName.get(name)!;
      const r = await gameData.resolveRecipe(name);
      recipeByName.set(name, r);
      return r;
    };

    for (const entry of store.activeProject.entries) {
      if (gen !== resolveGeneration) return;

      const targetInfo = targets.get(entry.id);
      const quantity = targetInfo ? targetInfo.effectiveQty : entry.quantity;
      if (quantity <= 0) continue;

      // Resolve each entry independently — one malformed entry must not blank the
      // whole project's material list.
      try {
        let recipe = await getRecipe(entry.recipe_name);
        if (!recipe) continue;
        // Pin variable slots to this entry's chosen combo, if any, so the resolver
        // reports the exact ingredients rather than generic keyword aggregates.
        if (entry.slot_item_ids && entry.slot_item_ids.length > 0) {
          recipe = store.pinRecipeSlots(recipe, entry.slot_item_ids);
        }

        const resolved = await store.resolveRecipeIngredients(
          recipe,
          quantity,
          false,
          new Set(),
          expandItemIds.size > 0 ? expandItemIds : undefined,
          intermediateStock,
        );

        const entryIntermediates = store.collectIntermediates(resolved.ingredients);
        for (const inter of entryIntermediates) {
          const existing = intermediateMap.get(inter.item_id);
          if (existing) {
            existing.quantity_produced += inter.quantity_produced;
            existing.crafts_needed += inter.crafts_needed;
          } else {
            intermediateMap.set(inter.item_id, { ...inter });
          }
        }

        const flat = store.flattenIngredients(resolved.ingredients);
        for (const [key, mat] of flat) {
          const existing = combinedMaterials.get(key);
          if (existing) {
            existing.quantity += mat.quantity;
            existing.expected_quantity += mat.expected_quantity;
          } else {
            combinedMaterials.set(key, { ...mat });
          }
        }
      } catch (entryErr) {
        console.warn(`[crafting] Failed to resolve project entry "${entry.recipe_name}":`, entryErr);
      }
    }

    if (gen !== resolveGeneration || store.activeProject?.id !== projectId) return;

    projectMaterials.value = combinedMaterials;
    projectIntermediates.value = Array.from(intermediateMap.values());
    materialNeeds.value = [];

    if (combinedMaterials.size > 0) {
      await checkProjectAvailability();
      // Resolve prices for pricing mode
      await resolvePrices(combinedMaterials, gen);
    }
  } catch (e) {
    console.error("[crafting] Project resolve failed:", e);
    invoke("log_startup", { message: `[DIAG] resolveProject FAILED: ${e instanceof Error ? e.message : String(e)}` }).catch(() => {});
  } finally {
    if (gen === resolveGeneration) {
      resolvingAll.value = false;
    }
  }
}

async function checkProjectAvailability() {
  if (projectMaterials.value.size === 0) return;
  checkingAvailability.value = true;
  try {
    materialNeeds.value = await store.checkMaterialAvailability(projectMaterials.value);
  } catch (e) {
    console.error("[crafting] Availability check failed:", e);
  } finally {
    checkingAvailability.value = false;
  }
}
</script>
