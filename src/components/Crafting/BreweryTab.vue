<template>
  <PaneLayout
    screen-key="crafting-brewery"
    :left-pane="{ title: 'Recipes', defaultWidth: 280, minWidth: 200, maxWidth: 420 }"
    :right-pane="{ title: 'Effects', defaultWidth: 300, minWidth: 220, maxWidth: 500 }">
    <template #left>
      <div class="flex flex-col gap-1 h-full min-h-0">
        <!-- Search -->
        <div class="px-2 pt-1">
          <input
            v-model="store.searchQuery"
            type="text"
            placeholder="Search recipes..."
            class="input text-xs w-full" />
        </div>

        <!-- Category filter pills -->
        <div class="flex flex-wrap gap-1 px-2 pb-1">
          <button
            v-for="option in categoryOptions"
            :key="option.value"
            :class="[
              'text-[0.6rem] px-1.5 py-0.5 rounded border cursor-pointer transition-colors',
              store.categoryFilter === option.value
                ? 'bg-accent-gold/20 border-accent-gold/40 text-accent-gold'
                : 'bg-transparent border-border-light text-text-muted hover:text-text-primary',
            ]"
            @click="store.categoryFilter = option.value">
            {{ option.label }}
            <span class="opacity-60">({{ option.count }})</span>
          </button>
        </div>

        <!-- Recipe list -->
        <div class="flex-1 min-h-0 overflow-y-auto">
          <div v-if="store.loading" class="px-2 py-4">
            <SkeletonLoader variant="text" :lines="8" />
          </div>

          <div v-else-if="store.filteredCount === 0" class="text-text-dim text-xs italic px-2 py-4 text-center">
            No recipes found.
          </div>

          <template v-else>
            <div v-for="group in store.filteredRecipesByCategory" :key="group.category">
              <!-- Category header -->
              <div class="text-[0.65rem] uppercase tracking-widest text-text-dim px-2 pt-2 pb-0.5 border-b border-surface-card sticky top-0 bg-surface-base z-10">
                {{ group.label }}
              </div>

              <!-- Recipe items -->
              <button
                v-for="recipe in group.recipes"
                :key="recipe.recipe_id"
                class="flex items-center justify-between px-2 py-1.5 text-xs text-left cursor-pointer border-none w-full"
                :class="store.selectedRecipeId === recipe.recipe_id
                  ? 'bg-accent-gold/15 text-accent-gold'
                  : 'bg-transparent text-text-secondary hover:bg-surface-base'"
                @click="store.selectRecipe(recipe.recipe_id)">
                <span class="truncate">{{ recipe.name }}</span>
                <span class="flex items-center gap-1.5 shrink-0 ml-2">
                  <span
                    v-if="comboStat(recipe.recipe_id)"
                    class="text-xs font-mono"
                    :class="comboStat(recipe.recipe_id)!.remaining === 0
                      ? 'text-accent-green'
                      : comboStat(recipe.recipe_id)!.discovered > 0
                        ? 'text-accent-gold/80'
                        : 'text-text-dim'"
                    :title="`${comboStat(recipe.recipe_id)!.discovered} of ${comboStat(recipe.recipe_id)!.total} material combinations discovered · ${comboStat(recipe.recipe_id)!.remaining} remaining`">
                    {{ comboStat(recipe.recipe_id)!.discovered }}/{{ comboStat(recipe.recipe_id)!.total }}
                  </span>
                  <span class="text-text-muted font-mono text-xs">Lv{{ recipe.skill_level_req }}</span>
                </span>
              </button>
            </div>
          </template>
        </div>

        <!-- Footer with counts + action buttons -->
        <div v-if="!store.loading" class="flex items-center justify-between px-2 py-1.5 border-t border-surface-card">
          <span class="text-xs text-text-muted">
            {{ store.filteredCount }} recipes
            <template v-if="store.overallComboStats.total > 0">
              · <span class="font-mono" :title="`${store.overallComboStats.discovered} of ${store.overallComboStats.total} material combinations discovered across all recipes · ${store.overallComboStats.remaining} remaining`">{{ store.overallComboStats.discovered }}/{{ store.overallComboStats.total }} combos</span>
            </template>
          </span>
          <div v-if="characterName" class="flex gap-1">
            <button
              class="text-xs px-1.5 py-0.5 rounded border border-border-light text-text-muted hover:text-accent-gold hover:border-accent-gold/40 cursor-pointer transition-colors bg-transparent"
              :disabled="store.scanning"
              @click="handleScan"
              title="Scan all Storage reports for brewing discoveries">
              {{ store.scanning ? 'Scanning...' : 'Scan' }}
            </button>
            <button
              class="text-xs px-1.5 py-0.5 rounded border border-border-light text-text-muted hover:text-accent-gold hover:border-accent-gold/40 cursor-pointer transition-colors bg-transparent"
              :disabled="store.scanning"
              @click="handleCsvImport"
              title="Import discoveries from a CSV file">
              Import
            </button>
            <button
              class="text-xs px-1 py-0.5 rounded border border-border-light text-text-muted hover:text-accent-blue hover:border-accent-blue/40 cursor-pointer transition-colors bg-transparent"
              @click="showCsvHelp = true"
              title="CSV format help">
              ?
            </button>
            <button
              v-if="store.totalDiscoveries > 0"
              class="text-xs px-1.5 py-0.5 rounded border border-border-light text-text-muted hover:text-accent-gold hover:border-accent-gold/40 cursor-pointer transition-colors bg-transparent"
              @click="handleCsvExport"
              title="Export discoveries to CSV">
              Export
            </button>
          </div>
        </div>

        <!-- CSV Help Modal -->
        <div v-if="showCsvHelp" class="absolute inset-0 z-20 flex items-center justify-center bg-black/50" @click.self="showCsvHelp = false">
          <div class="bg-surface-card border border-border-default rounded-lg p-4 max-w-lg shadow-lg">
            <div class="flex items-center justify-between mb-3">
              <h3 class="text-sm font-bold text-text-primary m-0">CSV Import Format</h3>
              <button class="text-text-dim hover:text-text-secondary cursor-pointer bg-transparent border-none" @click="showCsvHelp = false">✕</button>
            </div>
            <div class="text-xs text-text-secondary flex flex-col gap-2">
              <p class="m-0"><strong>Required columns:</strong> <code class="text-accent-gold">recipe_name</code> and at least one ingredient column.</p>
              <p class="m-0"><strong>Ingredient columns:</strong> <code class="text-accent-gold">ingredient1</code> through <code class="text-accent-gold">ingredient4</code> — use in-game item names (e.g., "Corn", "Groxmax Powder"). Empty cells are fine for recipes with fewer slots.</p>
              <p class="m-0"><strong>Effect columns</strong> (at least one recommended):</p>
              <ul class="m-0 pl-4 flex flex-col gap-0.5">
                <li><code class="text-accent-gold">effect_desc</code> — what the brew does, e.g., "Orcs gain +38 Max Power" or "Archery Base Damage % +20%". This is the most natural way to record effects — just paste the in-game tooltip text.</li>
                <li><code class="text-accent-gold">effect_name</code> — the drink's prefix/suffix, e.g., "Partier's" or "of Elfinity"</li>
              </ul>
              <p class="m-0"><strong>Advanced columns</strong> (optional):</p>
              <ul class="m-0 pl-4 flex flex-col gap-0.5">
                <li><code class="text-accent-gold">power</code> — internal TSysPower name (e.g., "BrewingLumberjack")</li>
                <li><code class="text-accent-gold">power_tier</code>, <code class="text-accent-gold">type_id</code>, <code class="text-accent-gold">item_name</code></li>
              </ul>
              <p class="m-0 text-text-dim">At minimum, <code>recipe_name</code> + ingredients records what you tried. Adding <code>effect_desc</code> or <code>effect_name</code> records what you got. We'll auto-match effect text to the game's internal data.</p>
              <div class="bg-surface-base rounded p-2 mt-1 font-mono text-[0.65rem] text-text-dim overflow-x-auto whitespace-nowrap">
                recipe_name,ingredient1,ingredient2,ingredient3,ingredient4,effect_desc<br>
                Dwarven Stout,Corn,Green Apple,Groxmax Powder,Cinnamon,Rakshasa gain +38 Max Power<br>
                Dwarven Stout,Corn,Pear,Groxmax Powder,Cinnamon,Rakshasa earn +11.8% Combat XP<br>
                Rice Wine,Rattus Root,Tomato,Walnuts,Pansy,Chance to Forage Extra Mushrooms +25%
              </div>
            </div>
          </div>
        </div>
      </div>
    </template>

    <!-- Center: detail panel -->
    <div class="h-full overflow-y-auto">
      <!-- Experimental banner -->
      <div class="bg-accent-warning/10 border-b border-accent-warning/20 px-4 py-1.5 text-xs text-accent-warning flex items-center gap-2">
        <span class="font-bold">Experimental</span>
        <span class="text-accent-warning/70">This feature is prone to bugs, and the layout/interface is likely to change significantly between versions.</span>
      </div>

      <EmptyState
        v-if="store.loading"
        variant="panel"
        primary="Loading brewing data..."
        secondary="Parsing CDN recipe and item data." />

      <!-- Effect search results (when an effect is selected in the right panel) -->
      <BreweryEffectResults
        v-else-if="store.selectedEffect" />

      <!-- Recipe detail (when a recipe is selected in the left panel) -->
      <BreweryRecipeDetail
        v-else-if="store.selectedRecipe"
        :recipe="store.selectedRecipe"
        :ingredient-by-id="store.ingredientById"
        :discoveries="store.selectedRecipeDiscoveries" />

      <EmptyState
        v-else
        variant="panel"
        primary="Select a recipe or effect"
        secondary="Choose a recipe from the left panel, or search for an effect in the right panel." />
    </div>

    <!-- Right pane: effect search -->
    <template #right>
      <BreweryEffectPanel />
    </template>
  </PaneLayout>
</template>

<script setup lang="ts">
import { onMounted, computed, ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import PaneLayout from "../Shared/PaneLayout.vue";
import SkeletonLoader from "../Shared/SkeletonLoader.vue";
import EmptyState from "../Shared/EmptyState.vue";
import BreweryRecipeDetail from "./BreweryRecipeDetail.vue";
import BreweryEffectPanel from "./BreweryEffectPanel.vue";
import BreweryEffectResults from "./BreweryEffectResults.vue";
import { useBreweryStore } from "../../stores/breweryStore";
import { useSettingsStore } from "../../stores/settingsStore";
import { useToast as useToastComposable } from "../../composables/useToast";
import { CATEGORY_LABELS } from "../../types/gameData/brewing";
import type { BrewingCategory } from "../../types/gameData/brewing";

const store = useBreweryStore();
const settingsStore = useSettingsStore();
const toast = useToastComposable();
const showCsvHelp = ref(false);

const characterName = computed(() => settingsStore.settings.activeCharacterName);

/** Combo-space progress for a recipe (undefined for recipes with no variable slots). */
function comboStat(recipeId: number) {
  const s = store.comboStatsByRecipe.get(recipeId);
  return s && s.total > 0 ? s : undefined;
}

const categoryOptions = computed(() => {
  const options: { value: BrewingCategory | "all"; label: string; count: number }[] = [
    { value: "all", label: "All", count: store.categoryCounts.get("all") ?? 0 },
  ];
  for (const [cat, count] of store.categoryCounts) {
    if (cat === "all" || count === 0) continue;
    options.push({
      value: cat as BrewingCategory,
      label: CATEGORY_LABELS[cat as BrewingCategory],
      count,
    });
  }
  return options;
});

async function handleScan() {
  if (!characterName.value) return;
  const result = await store.scanAllSnapshots(characterName.value);
  if (result) {
    if (result.new_discoveries > 0) {
      toast.success(
        `Found ${result.new_discoveries} new brewing discovery${result.new_discoveries === 1 ? '' : 'ies'} across ${result.total_brewing_items} brewed items.`
      );
    } else if (result.total_brewing_items > 0) {
      toast.info(`Scanned ${result.total_brewing_items} brewed items — no new discoveries.`);
    } else {
      toast.info("No brewed items found in Storage reports.");
    }
  }
}

async function handleCsvImport() {
  if (!characterName.value) return;
  const result = await store.importCsv(characterName.value);
  if (result) {
    if (result.new_discoveries > 0) {
      toast.success(
        `Imported ${result.new_discoveries} new discovery${result.new_discoveries === 1 ? '' : 'ies'} from CSV.`
      );
    } else if (result.total_brewing_items > 0) {
      toast.info(`CSV had ${result.total_brewing_items} entries — all already known.`);
    } else {
      toast.warn("No valid brewing entries found in CSV. Check the format.");
    }
  }
}

async function handleCsvExport() {
  if (store.discoveries.length === 0) return;

  const { save } = await import("@tauri-apps/plugin-dialog");
  const filePath = await save({
    filters: [{ name: "CSV", extensions: ["csv"] }],
    defaultPath: "brewing_discoveries.csv",
  });
  if (!filePath) return;

  // Build CSV content
  const header = "recipe_name,ingredient1,ingredient2,ingredient3,ingredient4,effect_name,power,power_tier,item_name,type_id";
  const lines = [header];

  for (const disc of store.discoveries) {
    const recipe = store.recipeById.get(disc.recipe_id);
    const recipeName = recipe?.name ?? "";
    const typeId = recipe?.result_item_id ?? "";
    const ingredients = disc.ingredient_ids.map((id) => store.ingredientById.get(id)?.name ?? String(id));
    while (ingredients.length < 4) ingredients.push("");

    const row = [
      csvEscape(recipeName),
      ...ingredients.map(csvEscape),
      csvEscape(disc.effect_label ?? ""),
      csvEscape(disc.power),
      String(disc.power_tier),
      csvEscape(disc.item_name ?? ""),
      String(typeId),
    ].join(",");
    lines.push(row);
  }

  try {
    await invoke("export_text_file", { filePath, content: lines.join("\n") });
    toast.success(`Exported ${store.discoveries.length} discoveries to CSV.`);
  } catch (e) {
    toast.error(`Export failed: ${e}`);
  }
}

function csvEscape(value: string): string {
  if (value.includes(",") || value.includes('"') || value.includes("\n")) {
    return `"${value.replace(/"/g, '""')}"`;
  }
  return value;
}

onMounted(async () => {
  await store.loadBrewingData();
  if (characterName.value) {
    store.loadDiscoveries(characterName.value);
  }
});
</script>
