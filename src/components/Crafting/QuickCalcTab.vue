<template>
  <div class="flex gap-4 h-[calc(100vh-200px)]">
    <!-- Left panel: recipe search + selection -->
    <div class="w-80 shrink-0 flex flex-col gap-3">
      <h3 class="dialog-title m-0">Quick Calculator</h3>

      <!-- Search -->
      <input
        v-model="searchQuery"
        class="input"
        placeholder="Search recipes..."
        @input="debouncedSearch" />

      <!-- Results list -->
      <ul
        v-if="searchResults.length > 0 && !selectedRecipe"
        class="list-none m-0 p-0 overflow-y-auto flex-1 border border-surface-elevated rounded">
        <li
          v-for="recipe in searchResults"
          :key="recipe.id"
          class="flex items-baseline gap-2 px-3 py-1.5 cursor-pointer border-b border-surface-dark text-xs hover:bg-surface-row-hover"
          @click="selectRecipe(recipe)">
          <span class="text-text-muted text-[0.72rem] min-w-12 shrink-0">
            [{{ recipe.skill ?? '?' }} {{ recipe.skill_level_req ?? 0 }}]
          </span>
          <span class="text-text-primary/75 flex-1 overflow-hidden text-ellipsis whitespace-nowrap">
            {{ recipe.name }}
          </span>
        </li>
      </ul>

      <!-- Selected recipe summary -->
      <div v-if="selectedRecipe" class="flex flex-col gap-3">
        <div class="bg-surface-elevated rounded p-3 border border-border-light">
          <div class="flex items-center justify-between mb-2">
            <span class="text-accent-gold font-semibold text-sm">{{ selectedRecipe.name }}</span>
            <button
              class="text-text-muted text-xs cursor-pointer bg-transparent border-none hover:text-text-primary"
              @click="clearSelection">
              clear
            </button>
          </div>
          <div class="text-text-dim text-xs space-y-0.5">
            <div v-if="selectedRecipe.skill">
              Skill: <span class="text-text-secondary">{{ selectedRecipe.skill }}</span>
              <span v-if="selectedRecipe.skill_level_req"> (Lv {{ selectedRecipe.skill_level_req }})</span>
            </div>
            <div v-if="selectedRecipe.reward_skill_xp">
              XP: <span class="text-text-secondary">{{ selectedRecipe.reward_skill_xp }}</span>
              <span v-if="selectedRecipe.reward_skill_xp_first_time" class="text-accent-gold">
                ({{ selectedRecipe.reward_skill_xp_first_time }} first time)
              </span>
            </div>
            <div v-if="selectedRecipe.reward_skill && selectedRecipe.reward_skill !== selectedRecipe.skill" class="text-yellow-400/80">
              XP goes to: {{ selectedRecipe.reward_skill }}
            </div>
          </div>
        </div>

        <!-- Quantity controls -->
        <div class="flex flex-col gap-2">
          <div class="flex items-center gap-2">
            <label class="text-text-dim text-xs w-20">Craft count:</label>
            <input
              v-model.number="craftCount"
              type="number"
              min="1"
              class="input w-24 text-center"
              @change="onCraftCountChange" />
          </div>
          <div class="flex items-center gap-2">
            <label class="text-text-dim text-xs w-20">Output qty:</label>
            <input
              v-model.number="desiredOutput"
              type="number"
              min="1"
              class="input w-24 text-center"
              @change="onDesiredOutputChange" />
            <span v-if="outputPerCraft > 1" class="text-text-muted text-xs">({{ outputPerCraft }}/craft)</span>
          </div>
        </div>

        <!-- Options -->
        <label class="flex items-center gap-2 text-text-dim text-xs cursor-pointer">
          <input
            v-model="expandIntermediates"
            type="checkbox"
            class="accent-accent-gold" />
          Expand intermediate ingredients
        </label>

        <button
          class="btn-primary text-xs py-1.5"
          :disabled="resolving"
          @click="resolve">
          {{ resolving ? 'Calculating...' : 'Calculate' }}
        </button>
      </div>

      <!-- Empty state -->
      <EmptyState v-if="!selectedRecipe && searchResults.length === 0 && searchQuery" variant="compact" :primary="`No recipes found for &quot;${searchQuery}&quot;`" />
      <EmptyState v-if="!selectedRecipe && !searchQuery" variant="compact" primary="Search for a recipe" secondary="Calculate ingredient requirements for any recipe." />
    </div>

    <!-- Right panel: results -->
    <div class="flex-1 overflow-y-auto border border-surface-elevated rounded p-4">
      <div v-if="!resolved" class="flex items-center justify-center h-full text-border-default italic">
        Select a recipe and click Calculate
      </div>

      <div v-else class="flex flex-col gap-4">
        <!-- Summary bar -->
        <div class="flex gap-6 items-baseline text-sm">
          <div>
            <span class="text-text-dim">Crafts:</span>
            <span class="text-text-primary font-semibold ml-1">{{ resolved.craft_count }}x</span>
          </div>
          <div>
            <span class="text-text-dim">Output:</span>
            <span class="text-text-primary font-semibold ml-1">{{ resolved.desired_quantity }}</span>
          </div>
          <div v-if="resolved.total_xp">
            <span class="text-text-dim">Total XP:</span>
            <span class="text-accent-gold font-semibold ml-1">{{ resolved.total_xp.toLocaleString() }}</span>
            <span v-if="resolved.reward_skill" class="text-text-muted text-xs ml-1">({{ resolved.reward_skill }})</span>
          </div>
          <div v-if="resolved.estimated_cost">
            <span class="text-text-dim">Est. cost:</span>
            <span class="text-accent-red font-semibold ml-1">{{ resolved.estimated_cost.toLocaleString() }}g</span>
          </div>
          <div v-for="cc in resolved.currency_costs" :key="cc.currency">
            <span class="text-text-dim">{{ currencyLabel(cc.currency) }}:</span>
            <span class="text-accent-gold font-semibold ml-1 tabular-nums">{{ cc.total.toLocaleString() }}</span>
            <span v-if="resolved.craft_count > 1" class="text-text-muted text-xs ml-1">
              ({{ cc.per_craft.toLocaleString() }}/craft)
            </span>
          </div>
        </div>

        <!-- Probabilistic warning -->
        <div
          v-if="hasProbabilisticOutput"
          class="bg-yellow-900/20 border border-yellow-600/30 rounded px-3 py-2 text-xs text-yellow-300/80">
          This recipe has a probabilistic output. Craft count is calculated based on expected rates — actual results may vary.
        </div>

        <!-- Ingredient tree -->
        <div>
          <h4 class="panel-label mb-2">Ingredients</h4>
          <IngredientTreeNode
            v-for="ing in resolved.ingredients"
            :key="ing.item_id ?? ing.item_name"
            :ingredient="ing"
            :depth="0" />
        </div>

        <!-- Action buttons -->
        <div class="flex gap-2">
          <button
            class="btn-secondary text-xs py-1.5"
            :disabled="checkingAvailability"
            @click="checkAvailability">
            {{ checkingAvailability ? 'Checking...' : 'Check Inventory' }}
          </button>
          <button
            v-if="selectedRecipe && !craftingStore.tracker?.active"
            class="btn-secondary text-xs py-1.5"
            @click="startTracking">
            Track Crafting
          </button>
        </div>

        <!-- Live crafting panel -->
        <LiveCraftingPanel />

        <!-- Material availability -->
        <MaterialSummary v-if="materialNeeds.length > 0" :needs="materialNeeds" />

        <!-- Pickup list -->
        <PickupList v-if="materialNeeds.length > 0" :needs="materialNeeds" />

        <!-- Shopping list -->
        <ShoppingList v-if="materialNeeds.length > 0" :needs="materialNeeds" />
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, watch } from "vue";
import { useGameDataStore } from "../../stores/gameDataStore";
import { useCraftingStore } from "../../stores/craftingStore";
import { useSettingsStore } from "../../stores/settingsStore";
import type { RecipeInfo } from "../../types/gameData/recipes";
import type { ResolvedRecipe, MaterialNeed } from "../../types/crafting";
import EmptyState from "../Shared/EmptyState.vue";
import IngredientTreeNode from "./IngredientTreeNode.vue";
import MaterialSummary from "./MaterialSummary.vue";
import PickupList from "./PickupList.vue";
import ShoppingList from "./ShoppingList.vue";
import LiveCraftingPanel from "./LiveCraftingPanel.vue";
import { currencyLabel } from "../../utils/recipeCurrency";
const gameData = useGameDataStore();
const craftingStore = useCraftingStore();
const settingsStore = useSettingsStore();

const searchQuery = ref("");
const searchResults = ref<RecipeInfo[]>([]);
const selectedRecipe = ref<RecipeInfo | null>(null);
const craftCount = ref(1);
const desiredOutput = ref(1);
const expandIntermediates = ref(false);
const resolving = ref(false);
const resolved = ref<ResolvedRecipe | null>(null);
const checkingAvailability = ref(false);
const materialNeeds = ref<MaterialNeed[]>([]);

let searchTimeout: ReturnType<typeof setTimeout> | null = null;

const outputPerCraft = computed(() => {
  if (!selectedRecipe.value) return 1;
  return selectedRecipe.value.result_items[0]?.stack_size ?? 1;
});

const hasProbabilisticOutput = computed(() => {
  if (!selectedRecipe.value) return false;
  return selectedRecipe.value.result_items.some(
    (r) => r.percent_chance !== null && r.percent_chance < 100
  );
});

function debouncedSearch() {
  if (searchTimeout) clearTimeout(searchTimeout);
  searchTimeout = setTimeout(async () => {
    if (!searchQuery.value.trim()) {
      searchResults.value = [];
      return;
    }
    searchResults.value = await gameData.searchRecipes(searchQuery.value, 30);
  }, 250);
}

function selectRecipe(recipe: RecipeInfo) {
  selectedRecipe.value = recipe;
  searchResults.value = [];
  craftCount.value = 1;
  desiredOutput.value = outputPerCraft.value;
  resolved.value = null;
}

function clearSelection() {
  selectedRecipe.value = null;
  resolved.value = null;
  materialNeeds.value = [];
  searchQuery.value = "";
  searchResults.value = [];
  craftCount.value = 1;
  desiredOutput.value = 1;
}

function onCraftCountChange() {
  if (craftCount.value < 1) craftCount.value = 1;
  desiredOutput.value = craftCount.value * outputPerCraft.value;
}

function onDesiredOutputChange() {
  if (desiredOutput.value < 1) desiredOutput.value = 1;
  craftCount.value = Math.ceil(desiredOutput.value / outputPerCraft.value);
}

async function resolve() {
  if (!selectedRecipe.value) return;
  resolving.value = true;
  materialNeeds.value = [];
  try {
    resolved.value = await craftingStore.resolveRecipeIngredients(
      selectedRecipe.value,
      desiredOutput.value,
      expandIntermediates.value,
    );
    // Sync craft count from resolver (it accounts for probabilistic outputs)
    craftCount.value = resolved.value.craft_count;
  } catch (e) {
    console.error("[crafting] Resolve failed:", e);
  } finally {
    resolving.value = false;
  }
}

// Character switch: availability is resolved per character+server, so drop
// the old character's needs synchronously and release the spinner — a stale
// in-flight check's finally-guard correctly refuses to clear it under the
// new identity, so the watcher owns the reset and the stale guard stays a
// no-op. No auto re-run: re-check is one click under the current identity.
watch(
  () => [settingsStore.settings.activeCharacterName, settingsStore.settings.activeServerName],
  () => {
    materialNeeds.value = [];
    checkingAvailability.value = false;
  },
);

async function checkAvailability() {
  if (!resolved.value) return;
  const characterName = settingsStore.settings.activeCharacterName;
  const serverName = settingsStore.settings.activeServerName;
  checkingAvailability.value = true;
  try {
    const flat = craftingStore.flattenIngredients(resolved.value.ingredients);
    const needs = await craftingStore.checkMaterialAvailability(flat);
    // Rapid character switch: drop the stale needs instead of flashing
    // another character's stock numbers into this view.
    if (
      settingsStore.settings.activeCharacterName !== characterName ||
      settingsStore.settings.activeServerName !== serverName
    )
      return;
    materialNeeds.value = needs;
  } catch (e) {
    console.error("[crafting] Availability check failed:", e);
  } finally {
    // A stale check must not clear the spinner owned by the newer check.
    if (
      settingsStore.settings.activeCharacterName === characterName &&
      settingsStore.settings.activeServerName === serverName
    )
      checkingAvailability.value = false;
  }
}

async function startTracking() {
  if (!selectedRecipe.value) return;
  await craftingStore.startQuickCalcTracking(selectedRecipe.value, desiredOutput.value);
}
</script>
