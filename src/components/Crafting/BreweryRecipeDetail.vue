<template>
  <div class="p-4 flex flex-col gap-4">
    <!-- Header -->
    <div class="flex items-center justify-between">
      <div>
        <h2 class="text-sm font-bold text-text-primary m-0">{{ recipe.name }}</h2>
        <div class="text-xs text-text-muted mt-0.5">
          <span>Level {{ recipe.skill_level_req }}</span>
          <span class="mx-1.5 opacity-30">·</span>
          <span>{{ recipe.xp }} XP</span>
          <span v-if="recipe.usage_delay_message" class="mx-1.5 opacity-30">·</span>
          <span v-if="recipe.usage_delay_message" class="text-text-dim">{{ recipe.usage_delay_message }}</span>
        </div>
      </div>
      <span class="text-xs uppercase tracking-widest text-text-dim border border-border-light rounded px-2 py-0.5">
        {{ categoryLabel }}
      </span>
    </div>

    <!-- Description -->
    <p v-if="recipe.description" class="text-xs text-text-secondary m-0 leading-relaxed">
      {{ recipe.description }}
    </p>

    <!-- Fixed Ingredients -->
    <div v-if="recipe.fixed_ingredients.length > 0">
      <div class="text-[0.65rem] uppercase tracking-widest text-text-dim border-b border-surface-card pb-0.5 mb-1.5">
        Fixed Ingredients
      </div>
      <div class="flex flex-col gap-1">
        <div
          v-for="(ing, i) in recipe.fixed_ingredients"
          :key="i"
          class="flex items-center gap-2 text-xs">
          <span class="font-mono text-text-muted w-6 text-right shrink-0">{{ ing.stack_size }}x</span>
          <ItemInline :reference="String(ing.item_id)" />
          <span v-if="getOwnedCount(ing.item_id) > 0" class="text-xs text-accent-green font-mono">
            (×{{ getOwnedCount(ing.item_id) }})
          </span>
          <span v-if="ing.chance_to_consume != null && ing.chance_to_consume < 1"
            class="text-text-dim text-xs">
            ({{ Math.round(ing.chance_to_consume * 100) }}% consumed)
          </span>
        </div>
      </div>
    </div>

    <!-- Variable Ingredient Slots -->
    <div v-if="recipe.variable_slots.length > 0">
      <div class="text-[0.65rem] uppercase tracking-widest text-text-dim border-b border-surface-card pb-0.5 mb-1.5">
        Variable Ingredient Slots
        <span class="normal-case tracking-normal text-text-dim ml-1">({{ recipe.variable_slots.length }} slots determine the effect)</span>
      </div>
      <div class="flex flex-col gap-3">
        <div v-for="(slot, i) in recipe.variable_slots" :key="i" class="bg-surface-base border border-surface-elevated rounded px-3 py-2">
          <div class="flex items-center gap-2 mb-1.5">
            <span class="text-xs font-mono text-accent-gold bg-accent-gold/10 rounded px-1.5 py-0.5">
              {{ slot.keyword }}
            </span>
            <span class="text-text-muted text-xs">{{ slot.stack_size }}x needed</span>
          </div>
          <div class="flex flex-wrap gap-x-2 gap-y-1">
            <span
              v-for="itemId in slot.valid_item_ids"
              :key="itemId"
              class="text-xs inline-flex items-center gap-0.5">
              <ItemInline :reference="String(itemId)" />
              <span v-if="getOwnedCount(itemId) > 0" class="text-xs text-accent-green font-mono">
                (×{{ getOwnedCount(itemId) }})
              </span>
            </span>
          </div>
          <div v-if="slot.valid_item_ids.length === 0" class="text-xs text-text-dim italic">
            No matching items found in CDN data
          </div>
        </div>
      </div>
    </div>

    <!-- Effect Pool Info -->
    <div v-if="recipe.brew_item_effect">
      <div class="text-[0.65rem] uppercase tracking-widest text-text-dim border-b border-surface-card pb-0.5 mb-1.5">
        Possible Effect Categories
        <span class="normal-case tracking-normal text-text-dim ml-1">(your brew will get one of these)</span>
      </div>
      <div class="flex flex-wrap gap-1.5">
        <span
          v-for="pool in dedupedPools"
          :key="pool"
          :title="getPoolDescription(pool)"
          :class="[
            'text-xs px-2 py-0.5 rounded border cursor-default',
            isPlaceholderPool(pool)
              ? 'border-accent-warning/30 text-accent-warning bg-accent-warning/5'
              : pool.startsWith('RacialBonuses')
                ? 'border-accent-red/30 text-accent-red/80 bg-accent-red/5'
                : 'border-border-light text-text-secondary bg-surface-base',
          ]">
          {{ getPoolLabel(pool) }}
          <span v-if="isPlaceholderPool(pool)" class="ml-1 opacity-60">(not yet implemented)</span>
          <span v-if="pool.startsWith('RacialBonuses')" class="ml-1 opacity-60">(may be race-locked)</span>
        </span>
      </div>
      <div class="text-xs text-text-dim mt-1.5">
        Tier {{ recipe.brew_item_effect.tier }}
        <span class="mx-1 opacity-30">·</span>
        {{ recipe.brew_item_effect.ingredient_slots.length }} variable slot{{ recipe.brew_item_effect.ingredient_slots.length === 1 ? '' : 's' }} determine which effect you get
      </div>
    </div>

    <!-- No variable slots message for simple recipes -->
    <div v-if="recipe.variable_slots.length === 0 && !recipe.brew_item_effect"
      class="text-xs text-text-dim italic bg-surface-base border border-surface-elevated rounded px-3 py-2">
      This recipe has no variable ingredient slots — the output is always the same.
    </div>

    <!-- Discoveries -->
    <div v-if="discoveries.length > 0">
      <div class="text-[0.65rem] uppercase tracking-widest text-text-dim border-b border-surface-card pb-0.5 mb-1.5 flex items-center gap-1.5">
        <button
          class="flex items-center gap-1 uppercase tracking-widest text-text-dim hover:text-text-secondary cursor-pointer bg-transparent border-none p-0"
          :title="layoutPrefs.discoveriesCollapsed ? 'Expand discoveries' : 'Collapse discoveries'"
          @click="updateLayout({ discoveriesCollapsed: !layoutPrefs.discoveriesCollapsed })">
          <span class="inline-block transition-transform text-[0.7rem] leading-none" :class="layoutPrefs.discoveriesCollapsed ? '-rotate-90' : ''">▾</span>
          Your Discoveries
        </button>
        <span class="normal-case tracking-normal text-text-dim">({{ discoveries.length }} found)</span>
      </div>
      <div
        v-show="!layoutPrefs.discoveriesCollapsed"
        ref="discoveriesEl"
        class="overflow-auto resize-y min-h-24 border border-surface-elevated rounded px-2"
        :style="{ height: discoveriesHeight + 'px' }">
      <table class="text-xs w-full">
        <thead>
          <tr class="text-xs uppercase tracking-wider text-text-dim">
            <th
              v-for="(slot, si) in recipe.variable_slots"
              :key="si"
              class="text-left pt-1 pb-1 font-normal w-36 sticky top-0 z-10 bg-surface-base"
              :title="slot.keyword">
              Slot {{ si + 1 }}
            </th>
            <th class="text-left pt-1 pb-1 font-normal sticky top-0 z-10 bg-surface-base">Effect</th>
            <th class="text-left pt-1 pb-1 font-normal sticky top-0 z-10 bg-surface-base">Req</th>
            <th class="text-left pt-1 pb-1 font-normal sticky top-0 z-10 bg-surface-base">Race</th>
            <th class="w-6 sticky top-0 z-10 bg-surface-base"></th>
          </tr>
        </thead>
        <tbody>
          <tr
            v-for="disc in discoveries"
            :key="disc.id"
            class="border-t border-surface-card align-top group">
            <td
              v-for="(ingId, si) in paddedIngredients(disc)"
              :key="si"
              class="py-1.5 pr-2 w-36">
              <template v-if="ingId !== null">
                <div class="inline-flex items-center gap-0.5">
                  <ItemInline :reference="String(ingId)" />
                  <span v-if="getOwnedCount(ingId) > 0" class="text-xs text-accent-green font-mono">
                    ×{{ getOwnedCount(ingId) }}
                  </span>
                </div>
              </template>
              <span v-else class="text-text-dim">—</span>
            </td>
            <td class="py-1.5 pr-3">
              <div class="flex flex-col gap-0.5">
                <span v-if="disc.effect_label" class="text-accent-gold font-semibold">{{ disc.effect_label }}</span>
                <span v-else class="text-text-secondary">{{ disc.power }}</span>
                <template v-if="getPowerInfo(disc)">
                  <div
                    v-for="(effect, ei) in getPowerInfo(disc)!.tier_effects"
                    :key="ei"
                    class="text-xs text-text-secondary leading-snug">
                    {{ effect }}
                  </div>
                </template>
                <div v-else class="text-xs text-text-dim">{{ disc.power }} (T{{ disc.power_tier }})</div>
              </div>
            </td>
            <td class="py-1.5 pr-3">
              <span v-if="getPowerInfo(disc)?.skill" class="text-xs text-text-muted whitespace-nowrap">
                {{ getPowerInfo(disc)!.skill }}
              </span>
              <span v-else class="text-text-dim">—</span>
            </td>
            <td class="py-1.5 pr-1">
              <span
                v-if="disc.race_restriction"
                class="text-xs px-1.5 py-0.5 rounded bg-accent-red/10 text-accent-red border border-accent-red/20 whitespace-nowrap">
                {{ disc.race_restriction }}
              </span>
              <span v-else class="text-text-dim">—</span>
            </td>
            <td class="py-1.5">
              <button
                class="text-text-dim hover:text-accent-red cursor-pointer bg-transparent border-none opacity-0 group-hover:opacity-100 transition-opacity text-xs"
                title="Delete this discovery"
                @click="confirmDelete(disc)">
                ✕
              </button>
            </td>
          </tr>
        </tbody>
      </table>
      </div>
    </div>

    <!-- No discoveries yet prompt -->
    <div v-else-if="recipe.variable_slots.length > 0"
      class="text-xs text-text-dim italic bg-surface-base border border-surface-elevated rounded px-3 py-2">
      No discoveries for this recipe yet. Use the form below, scan Storage reports, or import a CSV.
    </div>

    <!-- Add Discovery form -->
    <div v-if="recipe.variable_slots.length > 0 && characterName">
      <div class="flex items-center gap-2 mb-1.5">
        <div class="text-[0.65rem] uppercase tracking-widest text-text-dim border-b border-surface-card pb-0.5 flex-1">
          Add Discovery
        </div>
        <button
          v-if="!showAddForm"
          class="text-xs px-2 py-0.5 rounded border border-border-light text-text-muted hover:text-accent-gold hover:border-accent-gold/40 cursor-pointer transition-colors bg-transparent"
          @click="showAddForm = true">
          + Add
        </button>
      </div>

      <div v-if="showAddForm" class="bg-surface-base border border-surface-elevated rounded px-3 py-2.5 flex flex-col gap-2.5">
        <!-- Ingredient selectors — one per variable slot -->
        <div v-for="(slot, si) in recipe.variable_slots" :key="si" class="flex flex-col gap-1">
          <label class="text-[0.65rem] uppercase tracking-wider text-text-dim">
            Slot {{ si + 1 }}
            <span class="normal-case tracking-normal text-text-dim ml-1">({{ slot.keyword }})</span>
          </label>
          <select
            v-model="addFormSlots[si]"
            class="bg-surface-elevated border border-border-default rounded px-2 py-1 text-xs text-text-primary w-full">
            <option :value="null">-- Select ingredient --</option>
            <option
              v-for="itemId in slot.valid_item_ids"
              :key="itemId"
              :value="itemId">
              {{ ingredientById.get(itemId)?.name ?? `Item #${itemId}` }}
            </option>
          </select>
        </div>

        <!-- Optional effect label -->
        <div class="flex flex-col gap-1">
          <label class="text-[0.65rem] uppercase tracking-wider text-text-dim">
            Effect Label
            <span class="normal-case tracking-normal text-text-dim ml-1">(optional — e.g. "Partier's" or paste the tooltip text)</span>
          </label>
          <input
            v-model="addFormEffectLabel"
            type="text"
            placeholder="e.g. Orcs gain +38 Max Power"
            class="input text-xs w-full" />
        </div>

        <!-- Actions -->
        <div class="flex items-center gap-2">
          <button
            class="text-xs px-2.5 py-1 rounded border border-accent-gold/40 text-accent-gold hover:bg-accent-gold/10 cursor-pointer transition-colors bg-transparent disabled:opacity-40 disabled:cursor-not-allowed"
            :disabled="!addFormValid || addFormSaving"
            @click="submitAddForm">
            {{ addFormSaving ? 'Saving...' : 'Save Discovery' }}
          </button>
          <button
            class="text-xs px-2 py-1 rounded border border-border-light text-text-muted hover:text-text-secondary cursor-pointer transition-colors bg-transparent"
            @click="resetAddForm">
            Cancel
          </button>
          <span v-if="addFormError" class="text-xs text-accent-red ml-2">{{ addFormError }}</span>
        </div>
      </div>
    </div>

    <!-- Untried combinations -->
    <div v-if="recipe.variable_slots.length > 0 && recipeComboStat">
      <div class="text-[0.65rem] uppercase tracking-widest text-text-dim border-b border-surface-card pb-0.5 mb-1.5 flex items-center justify-between gap-2">
        <button
          v-if="missingCombos.length > 0"
          class="flex items-center gap-1 uppercase tracking-widest text-text-dim hover:text-text-secondary cursor-pointer bg-transparent border-none p-0"
          :title="layoutPrefs.untriedCollapsed ? 'Expand untried combinations' : 'Collapse untried combinations'"
          @click="updateLayout({ untriedCollapsed: !layoutPrefs.untriedCollapsed })">
          <span class="inline-block transition-transform text-[0.7rem] leading-none" :class="layoutPrefs.untriedCollapsed ? '-rotate-90' : ''">▾</span>
          Untried Combinations
        </button>
        <span v-else>Untried Combinations</span>
        <span
          class="normal-case tracking-normal font-mono"
          :class="recipeComboStat.remaining === 0 ? 'text-accent-green' : 'text-text-muted'">
          {{ recipeComboStat.discovered }}/{{ recipeComboStat.total }} discovered · {{ recipeComboStat.remaining }} left
        </span>
      </div>

      <!-- Progress bar -->
      <div class="h-1.5 w-full bg-surface-base rounded overflow-hidden mb-2">
        <div
          class="h-full rounded transition-all"
          :class="recipeComboStat.remaining === 0 ? 'bg-accent-green' : 'bg-accent-gold/70'"
          :style="{ width: `${recipeComboStat.total > 0 ? (recipeComboStat.discovered / recipeComboStat.total) * 100 : 0}%` }" />
      </div>

      <div v-if="recipeComboStat.remaining === 0" class="text-xs text-accent-green italic">
        All {{ recipeComboStat.total }} material combinations discovered for this recipe. 🎉
      </div>

      <template v-else>
        <div v-if="!layoutPrefs.untriedCollapsed && missingCombos.length > 0" class="mt-2 border border-surface-elevated rounded overflow-hidden">
          <!-- Selection toolbar -->
          <div class="flex items-center justify-between gap-2 px-3 py-1.5 border-b border-surface-card bg-surface-base">
            <div class="flex items-center gap-2 text-xs">
              <button
                class="text-text-muted hover:text-accent-gold cursor-pointer bg-transparent border border-border-light hover:border-accent-gold/30 rounded px-1.5 py-0.5 transition-colors"
                @click="selectAllVisible">
                Select all
              </button>
              <button
                v-if="selectedCount > 0"
                class="text-text-muted hover:text-accent-gold cursor-pointer bg-transparent border border-border-light hover:border-accent-gold/30 rounded px-1.5 py-0.5 transition-colors"
                @click="clearComboSelection">
                Clear
              </button>
              <span class="text-text-dim">{{ selectedCount }} selected</span>
            </div>
            <button
              class="text-xs px-2 py-0.5 rounded border cursor-pointer transition-colors bg-transparent disabled:opacity-40 disabled:cursor-not-allowed"
              :class="selectedCount > 0
                ? 'border-accent-gold/50 text-accent-gold hover:bg-accent-gold/10'
                : 'border-border-light text-text-muted'"
              :disabled="selectedCount === 0"
              @click="openProjectDialog">
              + Create crafting project
            </button>
          </div>

          <!-- Missing combos -->
          <div
            ref="untriedEl"
            class="overflow-y-auto resize-y min-h-32"
            :style="{ height: untriedHeight + 'px' }">
            <div
              v-for="(sug, i) in missingCombosCapped"
              :key="i"
              class="flex items-center gap-2 px-3 py-1 border-b border-surface-card last:border-b-0"
              :class="isComboSelected(sug.ingredientIds) ? 'bg-accent-gold/5' : ''">
              <input
                type="checkbox"
                class="accent-accent-gold cursor-pointer shrink-0"
                :checked="isComboSelected(sug.ingredientIds)"
                @change="toggleCombo(sug.ingredientIds)" />
              <span
                class="text-xs font-mono shrink-0 w-8"
                :class="sug.ownedCount === sug.totalCount ? 'text-accent-green' : 'text-text-dim'">
                {{ sug.ownedCount }}/{{ sug.totalCount }}
              </span>
              <div class="flex flex-wrap gap-x-2 gap-y-0.5">
                <span
                  v-for="ingId in sug.ingredientIds"
                  :key="ingId"
                  class="text-xs inline-flex items-center gap-0.5">
                  <span
                    class="w-1.5 h-1.5 rounded-full inline-block shrink-0"
                    :class="hasItem(ingId) ? 'bg-accent-green' : 'bg-surface-elevated border border-border-light'" />
                  <ItemInline :reference="String(ingId)" />
                </span>
              </div>
            </div>
            <div
              v-if="missingCombos.length > missingCombosCapped.length"
              class="px-3 py-1.5 text-xs text-text-dim italic">
              …and {{ missingCombos.length - missingCombosCapped.length }} more — narrow the list or select from above
            </div>
          </div>
        </div>
      </template>
    </div>

    <!-- Create-project dialog -->
    <div
      v-if="showProjectDialog"
      class="fixed inset-0 z-50 flex items-center justify-center bg-black/50"
      @click.self="showProjectDialog = false">
      <div class="bg-surface-card border border-border-default rounded-lg p-4 w-96 shadow-lg flex flex-col gap-3">
        <div class="flex items-center justify-between">
          <h3 class="text-sm font-bold text-text-primary m-0">Create crafting project</h3>
          <button class="text-text-dim hover:text-text-secondary cursor-pointer bg-transparent border-none" @click="showProjectDialog = false">✕</button>
        </div>
        <p class="text-xs text-text-muted m-0">
          {{ selectedCount }} untried combo{{ selectedCount === 1 ? '' : 's' }} of
          <span class="text-text-secondary">{{ recipe.name }}</span> → one entry each (qty 1). Materials roll up in the project.
        </p>

        <!-- Mode toggle -->
        <div class="flex gap-1">
          <button
            class="flex-1 text-xs px-2 py-1 rounded border cursor-pointer transition-colors"
            :class="projectMode === 'new' ? 'border-accent-gold/50 text-accent-gold bg-accent-gold/10' : 'border-border-light text-text-muted bg-transparent hover:text-text-primary'"
            @click="projectMode = 'new'">
            New project
          </button>
          <button
            class="flex-1 text-xs px-2 py-1 rounded border cursor-pointer transition-colors disabled:opacity-40 disabled:cursor-not-allowed"
            :class="projectMode === 'existing' ? 'border-accent-gold/50 text-accent-gold bg-accent-gold/10' : 'border-border-light text-text-muted bg-transparent hover:text-text-primary'"
            :disabled="craftingStore.projects.length === 0"
            @click="projectMode = 'existing'">
            Existing project
          </button>
        </div>

        <input
          v-if="projectMode === 'new'"
          v-model="newProjectName"
          type="text"
          placeholder="Project name"
          class="input text-xs w-full" />
        <select
          v-else
          v-model="existingProjectId"
          class="bg-surface-elevated border border-border-default rounded px-2 py-1 text-xs text-text-primary w-full">
          <option :value="null" disabled>-- Select a project --</option>
          <option v-for="p in craftingStore.projects" :key="p.id" :value="p.id">
            {{ p.name }} ({{ p.entry_count }})
          </option>
        </select>

        <div class="flex items-center gap-2">
          <button
            class="text-xs px-2.5 py-1 rounded border border-accent-gold/40 text-accent-gold hover:bg-accent-gold/10 cursor-pointer transition-colors bg-transparent disabled:opacity-40 disabled:cursor-not-allowed"
            :disabled="creatingProject || (projectMode === 'existing' && existingProjectId === null) || (projectMode === 'new' && !newProjectName.trim())"
            @click="confirmCreateProject">
            {{ creatingProject ? 'Working…' : (projectMode === 'new' ? 'Create & open' : 'Add & open') }}
          </button>
          <button
            class="text-xs px-2 py-1 rounded border border-border-light text-text-muted hover:text-text-secondary cursor-pointer transition-colors bg-transparent"
            @click="showProjectDialog = false">
            Cancel
          </button>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed, ref, watch, onMounted, onBeforeUnmount } from "vue";
import { confirm } from "@tauri-apps/plugin-dialog";
import ItemInline from "../Shared/Item/ItemInline.vue";
import type { BrewingRecipe, BrewingIngredient, BrewingDiscovery } from "../../types/gameData/brewing";
import { CATEGORY_LABELS, getPoolLabel, getPoolDescription } from "../../types/gameData/brewing";
import { useBreweryStore } from "../../stores/breweryStore";
import { useGameStateStore } from "../../stores/gameStateStore";
import { useSettingsStore } from "../../stores/settingsStore";
import { useCraftingStore } from "../../stores/craftingStore";
import { useViewNavigation } from "../../composables/useViewNavigation";
import { useViewPrefs } from "../../composables/useViewPrefs";
import { useToast } from "../../composables/useToast";

const props = defineProps<{
  recipe: BrewingRecipe;
  ingredientById: Map<number, BrewingIngredient>;
  discoveries: BrewingDiscovery[];
}>();

const store = useBreweryStore();
const gameState = useGameStateStore();
const settingsStore = useSettingsStore();
const craftingStore = useCraftingStore();
const { navigateToView } = useViewNavigation();
const toast = useToast();

const characterName = computed(() => settingsStore.settings.activeCharacterName);

// ── Persisted layout: collapse state + resizable list heights ────────────────
const { prefs: layoutPrefs, update: updateLayout } = useViewPrefs(
  "crafting-brewery.recipeDetail",
  { discoveriesCollapsed: false, discoveriesHeight: 320, untriedCollapsed: true, untriedHeight: 384 },
);

const discoveriesEl = ref<HTMLElement | null>(null);
const untriedEl = ref<HTMLElement | null>(null);
const discoveriesHeight = ref(layoutPrefs.value.discoveriesHeight);
const untriedHeight = ref(layoutPrefs.value.untriedHeight);

let resizeObserver: ResizeObserver | null = null;

onMounted(() => {
  resizeObserver = new ResizeObserver((entries) => {
    for (const entry of entries) {
      const el = entry.target as HTMLElement;
      const h = Math.round(el.offsetHeight);
      if (h <= 1) continue; // hidden via v-show — ignore the transient 0-height
      if (el === discoveriesEl.value && h !== discoveriesHeight.value) {
        discoveriesHeight.value = h;
        updateLayout({ discoveriesHeight: h });
      } else if (el === untriedEl.value && h !== untriedHeight.value) {
        untriedHeight.value = h;
        updateLayout({ untriedHeight: h });
      }
    }
  });

  // Observe each resizable region as it mounts/unmounts (both sit behind v-if/v-show).
  watch(discoveriesEl, (el, prev) => {
    if (prev) resizeObserver?.unobserve(prev);
    if (el) resizeObserver?.observe(el);
  }, { immediate: true });
  watch(untriedEl, (el, prev) => {
    if (prev) resizeObserver?.unobserve(prev);
    if (el) resizeObserver?.observe(el);
  }, { immediate: true });
});

onBeforeUnmount(() => {
  resizeObserver?.disconnect();
  resizeObserver = null;
});

// Session-stable random seed for shuffling untried combos
const sessionSeed = Math.random();

// ── Add Discovery form state ────────────────────────────────────────────────

const showAddForm = ref(false);
const addFormSlots = ref<(number | null)[]>([]);
const addFormEffectLabel = ref("");
const addFormSaving = ref(false);
const addFormError = ref("");

// Reset slot selections when recipe changes
watch(
  () => props.recipe.recipe_id,
  () => {
    resetAddForm();
  }
);

const addFormValid = computed(() => {
  // Every variable slot must have a selected ingredient
  return props.recipe.variable_slots.every((_, i) => addFormSlots.value[i] != null);
});

function resetAddForm() {
  showAddForm.value = false;
  addFormSlots.value = props.recipe.variable_slots.map(() => null);
  addFormEffectLabel.value = "";
  addFormSaving.value = false;
  addFormError.value = "";
}

async function submitAddForm() {
  if (!addFormValid.value || !characterName.value) return;
  addFormSaving.value = true;
  addFormError.value = "";

  const ingredientIds = addFormSlots.value.filter((id): id is number => id != null);
  const effectLabel = addFormEffectLabel.value.trim() || undefined;

  const result = await store.addManualDiscovery(
    characterName.value,
    props.recipe.recipe_id,
    ingredientIds,
    effectLabel
  );

  addFormSaving.value = false;

  if (result) {
    // Reset form but keep it open for rapid entry
    addFormSlots.value = props.recipe.variable_slots.map(() => null);
    addFormEffectLabel.value = "";
    addFormError.value = "";
  } else {
    addFormError.value = store.error ?? "Failed to save discovery";
  }
}

const categoryLabel = computed(() => CATEGORY_LABELS[props.recipe.category]);

const dedupedPools = computed(() => {
  if (!props.recipe.brew_item_effect) return [];
  return [...new Set(props.recipe.brew_item_effect.effect_pools)];
});

function isPlaceholderPool(pool: string): boolean {
  return pool.startsWith("TBD");
}

/** Get the TSys power info from the store's bulk-fetched cache */
function getPowerInfo(disc: BrewingDiscovery) {
  return store.getPowerInfo(disc.power, disc.power_tier);
}

/** Get owned count for an item by type ID */
function getOwnedCount(itemTypeId: number): number {
  const ingredient = props.ingredientById.get(itemTypeId);
  if (!ingredient) return 0;
  return gameState.ownedItemCounts[ingredient.name] ?? 0;
}

function hasItem(itemTypeId: number): boolean {
  return getOwnedCount(itemTypeId) > 0;
}

/** Pad ingredient IDs to match the number of variable slots */
function paddedIngredients(disc: BrewingDiscovery): (number | null)[] {
  const slotCount = props.recipe.variable_slots.length;
  const result: (number | null)[] = [...disc.ingredient_ids];
  while (result.length < slotCount) result.push(null);
  return result;
}

async function confirmDelete(disc: BrewingDiscovery) {
  const label = disc.effect_label ?? disc.power;
  const ok = await confirm(`Delete discovery "${label}"? This cannot be undone.`, {
    title: "Delete Discovery",
    kind: "warning",
  });
  if (ok) {
    store.deleteDiscovery(disc.id);
  }
}

// ── Untried combinations ─────────────────────────────────────────────────────

/** CDN-derived combo progress for this recipe (discovered / total / remaining). */
const recipeComboStat = computed(() => store.comboStatsByRecipe.get(props.recipe.recipe_id));

/** Set of sorted ingredient ID arrays that have already been discovered */
const discoveredCombos = computed(() => {
  const set = new Set<string>();
  for (const d of props.discoveries) {
    const key = [...d.ingredient_ids].sort((a, b) => a - b).join(",");
    set.add(key);
  }
  return set;
});

interface Suggestion {
  ingredientIds: number[];
  ownedCount: number;
  totalCount: number;
}

/**
 * Every untried material combination for this recipe, ordered so the ones whose
 * ingredients the player already owns come first (then stable-shuffled per
 * session). Enumerates the full cartesian product of the variable slots, so it
 * covers combinations the player has never touched — not just ones seen in a
 * CSV. Guarded against pathological explosion, though no real recipe approaches
 * the cap.
 */
const missingCombos = computed((): Suggestion[] => {
  const slots = props.recipe.variable_slots;
  if (slots.length === 0) return [];

  const slotOptions = slots.map((s) => s.valid_item_ids);
  const totalCombos = slotOptions.reduce((acc, opts) => acc * Math.max(opts.length, 1), 1);
  if (totalCombos > 5000) return []; // safety valve

  const untried: Suggestion[] = [];
  for (const combo of cartesian(slotOptions)) {
    const key = [...combo].sort((a, b) => a - b).join(",");
    if (discoveredCombos.value.has(key)) continue;
    const ownedCount = combo.filter((id) => hasItem(id)).length;
    untried.push({ ingredientIds: combo, ownedCount, totalCount: combo.length });
  }

  // Most-owned ingredients first, then stable-shuffle within each tier.
  untried.sort((a, b) => {
    if (b.ownedCount !== a.ownedCount) return b.ownedCount - a.ownedCount;
    return seededHash(a.ingredientIds) - seededHash(b.ingredientIds);
  });
  return untried;
});

/** Cap on how many rows we render in the full missing list at once. */
const MISSING_RENDER_CAP = 500;
const missingCombosCapped = computed(() => missingCombos.value.slice(0, MISSING_RENDER_CAP));

// The untried-list expand state lives in the persisted `layoutPrefs.untriedCollapsed`
// (toggled from the section label), so it survives recipe switches like the
// discoveries collapse does. Only the per-recipe selection/dialog reset here.
watch(() => props.recipe.recipe_id, () => {
  clearComboSelection();
  showProjectDialog.value = false;
});

// ── Combo selection → crafting project ──────────────────────────────────────

/**
 * Selected untried combos, keyed by their slot-ordered ingredient IDs joined
 * with "-". Because item IDs are positive integers, the key round-trips back to
 * the ingredient array via split, so we don't need to hold the arrays too.
 */
const selectedComboKeys = ref<Set<string>>(new Set());

function comboKey(ids: number[]): string {
  return ids.join("-");
}
function isComboSelected(ids: number[]): boolean {
  return selectedComboKeys.value.has(comboKey(ids));
}
function toggleCombo(ids: number[]) {
  const next = new Set(selectedComboKeys.value);
  const key = comboKey(ids);
  if (next.has(key)) next.delete(key);
  else next.add(key);
  selectedComboKeys.value = next;
}
function selectAllVisible() {
  const next = new Set(selectedComboKeys.value);
  for (const c of missingCombosCapped.value) next.add(comboKey(c.ingredientIds));
  selectedComboKeys.value = next;
}
function clearComboSelection() {
  selectedComboKeys.value = new Set();
}
const selectedCount = computed(() => selectedComboKeys.value.size);

/** The selected combos as slot-ordered ingredient-ID arrays (recovered from keys). */
const selectedCombos = computed<number[][]>(() =>
  [...selectedComboKeys.value].map((k) => k.split("-").map(Number)),
);

// Create-project dialog state
const showProjectDialog = ref(false);
const projectMode = ref<"new" | "existing">("new");
const newProjectName = ref("");
const existingProjectId = ref<number | null>(null);
const creatingProject = ref(false);

async function openProjectDialog() {
  if (selectedCount.value === 0) return;
  newProjectName.value = `${props.recipe.name} — untried`;
  existingProjectId.value = null;
  projectMode.value = "new";
  showProjectDialog.value = true;
  await craftingStore.loadProjects();
}

async function confirmCreateProject() {
  const combos = selectedCombos.value;
  if (combos.length === 0) return;
  creatingProject.value = true;
  try {
    let projectId: number;
    if (projectMode.value === "existing" && existingProjectId.value !== null) {
      projectId = existingProjectId.value;
    } else {
      const name = newProjectName.value.trim() || `${props.recipe.name} — untried`;
      projectId = await craftingStore.createProject(name);
    }
    // One entry per combo, each pinned to its exact ingredients (qty 1).
    await craftingStore.addComboEntries(projectId, props.recipe.recipe_id, props.recipe.name, combos, 1);
    await craftingStore.loadProject(projectId);
    toast.success(
      `Added ${combos.length} combo${combos.length === 1 ? "" : "s"} to the project.`,
    );
    showProjectDialog.value = false;
    clearComboSelection();
    navigateToView({ view: "crafting", subTab: "projects" });
  } catch (e) {
    toast.error(`Failed to create project: ${e}`);
  } finally {
    creatingProject.value = false;
  }
}

/** Cartesian product of arrays */
function cartesian(arrays: number[][]): number[][] {
  if (arrays.length === 0) return [[]];
  const [first, ...rest] = arrays;
  const restProduct = cartesian(rest);
  const result: number[][] = [];
  for (const item of first) {
    for (const combo of restProduct) {
      result.push([item, ...combo]);
    }
  }
  return result;
}

/** Simple seeded hash for stable-per-session shuffling */
function seededHash(ids: number[]): number {
  let h = sessionSeed * 2147483647;
  for (const id of ids) {
    h = ((h * 31) + id) % 2147483647;
  }
  return h;
}
</script>
