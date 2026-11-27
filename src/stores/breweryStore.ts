import { defineStore, acceptHMRUpdate } from "pinia";
import { ref, computed } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { useSettingsStore } from "./settingsStore";
import type {
  BrewingRecipe,
  BrewingIngredient,
  BrewingCategory,
  BrewingDiscovery,
  BrewingScanResult,
  TsysPowerInfo,
} from "../types/gameData/brewing";
import { CATEGORY_ORDER, CATEGORY_LABELS } from "../types/gameData/brewing";

export const useBreweryStore = defineStore("brewery", () => {
  // ── State ──────────────────────────────────────────────────────────────────

  const recipes = ref<BrewingRecipe[]>([]);
  const ingredients = ref<BrewingIngredient[]>([]);
  const discoveries = ref<BrewingDiscovery[]>([]);
  const loading = ref(false);
  const loaded = ref(false);
  const scanning = ref(false);
  const error = ref<string | null>(null);

  const selectedRecipeId = ref<number | null>(null);
  const searchQuery = ref("");
  const categoryFilter = ref<BrewingCategory | "all">("all");

  // Right panel + effect search state
  const effectSearchQuery = ref("");
  const selectedEffect = ref<string | null>(null);

  /** Resolved TSys power info, keyed by "power:tier" */
  const powerInfoMap = ref<Map<string, TsysPowerInfo>>(new Map());

  // ── Computed ───────────────────────────────────────────────────────────────

  /** Ingredient lookup by item ID */
  const ingredientById = computed(() => {
    const map = new Map<number, BrewingIngredient>();
    for (const ing of ingredients.value) {
      map.set(ing.item_id, ing);
    }
    return map;
  });

  /** Recipe lookup by ID */
  const recipeById = computed(() => {
    const map = new Map<number, BrewingRecipe>();
    for (const r of recipes.value) {
      map.set(r.recipe_id, r);
    }
    return map;
  });

  /** Discoveries grouped by recipe ID */
  const discoveriesByRecipe = computed(() => {
    const map = new Map<number, BrewingDiscovery[]>();
    for (const d of discoveries.value) {
      if (!map.has(d.recipe_id)) {
        map.set(d.recipe_id, []);
      }
      map.get(d.recipe_id)!.push(d);
    }
    return map;
  });

  /** Discovery count per recipe */
  const discoveryCountByRecipe = computed(() => {
    const map = new Map<number, number>();
    for (const [recipeId, discs] of discoveriesByRecipe.value) {
      map.set(recipeId, discs.length);
    }
    return map;
  });

  /**
   * Combination-space progress per recipe, derived entirely from CDN data.
   *
   * `total` is the product of each variable slot's valid-ingredient count — i.e.
   * every material combination the game permits for that recipe, whether or not
   * the player has ever brewed it. `discovered` is the number of *distinct*
   * combos the player has recorded (keyed by the sorted ingredient-id set, which
   * uniquely identifies one grid cell because a recipe's slots never share
   * ingredients). Recipes without variable slots are omitted (nothing to explore).
   */
  const comboStatsByRecipe = computed(() => {
    const map = new Map<number, { discovered: number; total: number; remaining: number }>();
    for (const r of recipes.value) {
      const slots = r.variable_slots;
      if (!slots || slots.length === 0) continue;

      let total = 1;
      for (const s of slots) {
        total *= s.valid_item_ids.length;
      }
      // If any slot has no resolvable items, the total is unknown (0).
      if (!Number.isFinite(total)) total = 0;

      const seen = new Set<string>();
      for (const d of discoveriesByRecipe.value.get(r.recipe_id) ?? []) {
        seen.add([...d.ingredient_ids].sort((a, b) => a - b).join(","));
      }
      const discovered = Math.min(seen.size, total || seen.size);
      const remaining = total > 0 ? Math.max(0, total - discovered) : 0;
      map.set(r.recipe_id, { discovered, total, remaining });
    }
    return map;
  });

  /** Rolled-up combination progress across every recipe with a known combo space. */
  const overallComboStats = computed(() => {
    let discovered = 0;
    let total = 0;
    for (const s of comboStatsByRecipe.value.values()) {
      if (s.total > 0) {
        discovered += s.discovered;
        total += s.total;
      }
    }
    return { discovered, total, remaining: Math.max(0, total - discovered) };
  });

  /** Total discoveries across all recipes */
  const totalDiscoveries = computed(() => discoveries.value.length);

  /** Discoveries for the currently selected recipe */
  const selectedRecipeDiscoveries = computed(() => {
    if (selectedRecipeId.value === null) return [];
    return discoveriesByRecipe.value.get(selectedRecipeId.value) ?? [];
  });

  /** Recipes grouped by category */
  const recipesByCategory = computed(() => {
    const groups = new Map<BrewingCategory, BrewingRecipe[]>();
    for (const cat of CATEGORY_ORDER) {
      groups.set(cat, []);
    }
    for (const r of recipes.value) {
      groups.get(r.category)?.push(r);
    }
    return groups;
  });

  /** Category counts for filter pills */
  const categoryCounts = computed(() => {
    const counts = new Map<BrewingCategory | "all", number>();
    counts.set("all", recipes.value.length);
    for (const cat of CATEGORY_ORDER) {
      counts.set(cat, recipesByCategory.value.get(cat)?.length ?? 0);
    }
    return counts;
  });

  /** Filtered + searched recipes, grouped by category */
  const filteredRecipesByCategory = computed(() => {
    const q = searchQuery.value.toLowerCase().trim();
    const catFilter = categoryFilter.value;

    const groups: { category: BrewingCategory; label: string; recipes: BrewingRecipe[] }[] = [];

    for (const cat of CATEGORY_ORDER) {
      if (catFilter !== "all" && catFilter !== cat) continue;

      let catRecipes = recipesByCategory.value.get(cat) ?? [];
      if (q) {
        catRecipes = catRecipes.filter(
          (r) =>
            r.name.toLowerCase().includes(q) ||
            (r.internal_name?.toLowerCase().includes(q) ?? false)
        );
      }

      if (catRecipes.length > 0) {
        groups.push({
          category: cat,
          label: CATEGORY_LABELS[cat],
          recipes: catRecipes,
        });
      }
    }
    return groups;
  });

  /** The currently selected recipe */
  const selectedRecipe = computed(() => {
    if (selectedRecipeId.value === null) return null;
    return recipes.value.find((r) => r.recipe_id === selectedRecipeId.value) ?? null;
  });

  /** Total recipe count after filtering */
  const filteredCount = computed(() => {
    return filteredRecipesByCategory.value.reduce(
      (sum, group) => sum + group.recipes.length,
      0
    );
  });

  // ── Effect search computed ──────────────────────────────────────────────

  /** Get resolved power info for a discovery */
  function getPowerInfo(power: string, tier: number): TsysPowerInfo | undefined {
    return powerInfoMap.value.get(`${power}:${tier}`);
  }

  /**
   * All unique effects as searchable entries.
   * Each entry has the resolved effect descriptions as searchable text.
   */
  const effectEntries = computed(() => {
    // Group discoveries by power name (not power:tier, since same power
    // at different tiers is the same "effect" just different strength)
    const byPower = new Map<string, {
      power: string;
      effectLabel: string | null;
      raceRestriction: string | null;
      discoveryCount: number;
      descriptions: string[]; // resolved human-readable effect descriptions
      skill: string | null;
      prefix: string | null;
      suffix: string | null;
    }>();

    for (const d of discoveries.value) {
      const info = getPowerInfo(d.power, d.power_tier);
      const existing = byPower.get(d.power);

      if (existing) {
        existing.discoveryCount++;
        if (!existing.effectLabel && d.effect_label) {
          existing.effectLabel = d.effect_label;
        }
      } else {
        byPower.set(d.power, {
          power: d.power,
          effectLabel: d.effect_label,
          raceRestriction: d.race_restriction,
          discoveryCount: 1,
          descriptions: info?.tier_effects ?? [],
          skill: info?.skill ?? null,
          prefix: info?.prefix ?? null,
          suffix: info?.suffix ?? null,
        });
      }
    }

    return [...byPower.values()].sort((a, b) => {
      if (a.raceRestriction && !b.raceRestriction) return 1;
      if (!a.raceRestriction && b.raceRestriction) return -1;
      const aName = a.effectLabel ?? a.power;
      const bName = b.effectLabel ?? b.power;
      return aName.localeCompare(bName);
    });
  });

  /** Filtered effects based on search query — searches descriptions, labels, and power names */
  const filteredEffects = computed(() => {
    const q = effectSearchQuery.value.toLowerCase().trim();
    if (!q) return effectEntries.value;
    return effectEntries.value.filter((e) => {
      const label = e.effectLabel?.toLowerCase() ?? "";
      const power = e.power.toLowerCase();
      const descs = e.descriptions.join(" ").toLowerCase();
      const skill = e.skill?.toLowerCase() ?? "";
      return label.includes(q) || power.includes(q) || descs.includes(q) || skill.includes(q);
    });
  });

  /** Discoveries for the selected effect (across all recipes), enriched with power info */
  const selectedEffectDiscoveries = computed(() => {
    if (!selectedEffect.value) return [];
    return discoveries.value.filter((d) => d.power === selectedEffect.value);
  });

  // ── Actions ────────────────────────────────────────────────────────────────

  async function loadBrewingData() {
    if (loaded.value || loading.value) return;
    loading.value = true;
    error.value = null;

    try {
      const [recipesData, ingredientsData] = await Promise.all([
        invoke<BrewingRecipe[]>("get_brewing_recipes"),
        invoke<BrewingIngredient[]>("get_brewing_ingredients"),
      ]);
      recipes.value = recipesData;
      ingredients.value = ingredientsData;
      loaded.value = true;
    } catch (e) {
      error.value = `Failed to load brewing data: ${e}`;
      console.error(error.value);
    } finally {
      loading.value = false;
    }
  }

  async function loadDiscoveries(character: string) {
    try {
      const rows = await invoke<BrewingDiscovery[]>(
        "get_brewing_discoveries",
        { character }
      );
      // Rapid character switch: a newer load is in flight (or done). Drop the
      // stale rows instead of flashing another character's discoveries.
      const settingsStore = useSettingsStore();
      if (settingsStore.settings.activeCharacterName !== character) return;
      discoveries.value = rows;
      // Bulk-fetch TSys info for all discovered powers
      await fetchAllPowerInfo();
    } catch (e) {
      console.error("Failed to load brewing discoveries:", e);
    }
  }

  /** Fetch TSys power info for all unique power+tier combos in discoveries */
  async function fetchAllPowerInfo() {
    const needed: [string, number][] = [];
    const seen = new Set<string>();
    for (const d of discoveries.value) {
      const key = `${d.power}:${d.power_tier}`;
      if (!seen.has(key) && !powerInfoMap.value.has(key)) {
        seen.add(key);
        needed.push([d.power, d.power_tier]);
      }
    }
    if (needed.length === 0) return;

    try {
      const result = await invoke<Record<string, TsysPowerInfo>>(
        "get_tsys_power_info_batch",
        { powers: needed }
      );
      const newMap = new Map(powerInfoMap.value);
      for (const [key, info] of Object.entries(result)) {
        newMap.set(key, info);
      }
      powerInfoMap.value = newMap;
    } catch (e) {
      console.error("Failed to fetch TSys power info batch:", e);
    }
  }

  async function scanAllSnapshots(character: string): Promise<BrewingScanResult | null> {
    if (scanning.value) return null;
    scanning.value = true;
    try {
      const result = await invoke<BrewingScanResult>(
        "scan_all_snapshots_for_brewing",
        { character }
      );
      await loadDiscoveries(character);
      return result;
    } catch (e) {
      console.error("Failed to scan snapshots for brewing:", e);
      error.value = `Scan failed: ${e}`;
      return null;
    } finally {
      scanning.value = false;
    }
  }

  async function importCsv(character: string): Promise<BrewingScanResult | null> {
    if (scanning.value) return null;

    const settingsStore = useSettingsStore();
    const { open } = await import("@tauri-apps/plugin-dialog");
    const filePath = await open({
      defaultPath: settingsStore.settings.gameDataPath || undefined,
      filters: [{ name: "CSV", extensions: ["csv"] }],
      multiple: false,
    });
    if (!filePath) return null;

    scanning.value = true;
    try {
      const result = await invoke<BrewingScanResult>(
        "import_brewing_discoveries_csv",
        { filePath, character }
      );
      await loadDiscoveries(character);
      return result;
    } catch (e) {
      console.error("Failed to import brewing CSV:", e);
      error.value = `CSV import failed: ${e}`;
      return null;
    } finally {
      scanning.value = false;
    }
  }

  async function addManualDiscovery(
    character: string,
    recipeId: number,
    ingredientIds: number[],
    effectLabel?: string
  ): Promise<BrewingDiscovery | null> {
    try {
      const disc = await invoke<BrewingDiscovery>("add_brewing_discovery_manual", {
        character,
        recipeId,
        ingredientIds,
        effectLabel: effectLabel || null,
      });
      // Add to local state (or replace if it already existed via ON CONFLICT)
      const idx = discoveries.value.findIndex((d) => d.id === disc.id);
      if (idx >= 0) {
        discoveries.value[idx] = disc;
      } else {
        discoveries.value.unshift(disc);
      }
      return disc;
    } catch (e) {
      console.error("Failed to add manual discovery:", e);
      error.value = `Add failed: ${e}`;
      return null;
    }
  }

  async function deleteDiscovery(discoveryId: number) {
    try {
      await invoke("delete_brewing_discovery", { discoveryId });
      discoveries.value = discoveries.value.filter((d) => d.id !== discoveryId);
    } catch (e) {
      console.error("Failed to delete discovery:", e);
      error.value = `Delete failed: ${e}`;
    }
  }

  function selectRecipe(recipeId: number) {
    selectedRecipeId.value = recipeId;
    selectedEffect.value = null; // clear effect selection when picking a recipe
  }

  function clearSelection() {
    selectedRecipeId.value = null;
  }

  function selectEffect(power: string) {
    selectedEffect.value = power;
    selectedRecipeId.value = null; // clear recipe selection when picking an effect
  }

  function clearEffectSelection() {
    selectedEffect.value = null;
  }

  /**
   * Called after an inventory import to auto-scan for new brewing discoveries.
   * Only does work if the brewery store has been loaded (user has visited the tab).
   */
  async function onInventoryImported(character: string) {
    if (!loaded.value) return;
    try {
      const result = await invoke<BrewingScanResult>(
        "scan_all_snapshots_for_brewing",
        { character }
      );
      if (result.new_discoveries > 0) {
        await loadDiscoveries(character);
        console.log(`Brewery: auto-scan found ${result.new_discoveries} new discoveries`);
      }
    } catch (e) {
      console.warn("Brewery auto-scan failed:", e);
    }
  }

  return {
    // State
    recipes,
    ingredients,
    discoveries,
    loading,
    loaded,
    scanning,
    error,
    selectedRecipeId,
    searchQuery,
    categoryFilter,
    effectSearchQuery,
    selectedEffect,
    powerInfoMap,
    // Computed
    ingredientById,
    recipeById,
    discoveriesByRecipe,
    discoveryCountByRecipe,
    comboStatsByRecipe,
    overallComboStats,
    totalDiscoveries,
    selectedRecipeDiscoveries,
    recipesByCategory,
    categoryCounts,
    filteredRecipesByCategory,
    selectedRecipe,
    filteredCount,
    effectEntries,
    filteredEffects,
    selectedEffectDiscoveries,
    // Functions
    getPowerInfo,
    // Actions
    loadBrewingData,
    loadDiscoveries,
    scanAllSnapshots,
    importCsv,
    addManualDiscovery,
    selectRecipe,
    clearSelection,
    deleteDiscovery,
    selectEffect,
    clearEffectSelection,
    onInventoryImported,
  };
});

// Enable proper Pinia hot-module replacement: without this, editing a store action
// during `tauri dev` leaves the already-instantiated singleton running the old code.
if (import.meta.hot) {
  import.meta.hot.accept(acceptHMRUpdate(useBreweryStore, import.meta.hot));
}
