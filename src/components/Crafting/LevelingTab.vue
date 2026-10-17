<template>
  <div class="flex flex-col gap-3 h-[calc(100vh-200px)]">
    <!-- Top bar: mirrors the content columns below — the skill/level/XP controls
         span the recipe-list panel's width, and the plan header lines up with the
         center plan box (spacer matches the resize handle). -->
    <div class="flex items-end shrink-0">
      <!-- Left column: same width as the recipe panel below -->
      <div
        class="flex items-end gap-4 flex-wrap shrink-0 pr-1"
        :style="{ width: `${recipePanelWidth}px` }">
        <!-- Skill selector -->
        <div class="flex flex-col gap-1">
          <label class="text-text-dim text-xs">Skill</label>
          <select v-model="state.selectedSkill" class="input text-xs w-48" @change="onSkillChange">
            <option value="">Select a skill...</option>
            <option v-for="skill in craftingSkills" :key="skill" :value="skill">
              {{ skill }}
            </option>
          </select>
        </div>

        <template v-if="state.selectedSkill">
          <!-- Current level (editable, shows total level) -->
          <div class="flex flex-col gap-1">
            <label class="text-text-dim text-xs">Level</label>
            <div class="flex items-center gap-1.5">
              <input
                v-model.number="state.currentLevel"
                type="number"
                min="0"
                class="input w-16 text-center text-xs" />
              <button
                v-if="state.snapshotLevel !== null && state.snapshotLevel !== state.currentLevel"
                class="text-text-muted text-[0.6rem] cursor-pointer bg-transparent border-none hover:text-accent-gold underline"
                @click="state.currentLevel = state.snapshotLevel!">
                reset ({{ state.snapshotLevel }})
              </button>
            </div>
          </div>

          <!-- Starting XP (XP already earned toward current level) -->
          <div class="flex flex-col gap-1">
            <label class="text-text-dim text-xs">Starting XP</label>
            <div class="flex items-center gap-1.5">
              <input
                v-model.number="state.startingXp"
                type="number"
                min="0"
                :max="state.xpTable[baseLevel] ?? 99999"
                class="input w-20 text-center text-xs"
                :disabled="state.planLevels.length > 0"
                :title="state.planLevels.length > 0 ? 'Clear your plan to change starting XP' : 'XP already earned toward current level'" />
              <button
                v-if="gameStateXp !== null && state.startingXp !== gameStateXp && state.planLevels.length === 0"
                class="text-text-muted text-[0.6rem] cursor-pointer bg-transparent border-none hover:text-accent-gold underline whitespace-nowrap"
                @click="state.startingXp = gameStateXp!">
                use current ({{ gameStateXp!.toLocaleString() }})
              </button>
            </div>
          </div>

          <!-- XP buff -->
          <div class="flex flex-col gap-1">
            <label class="text-text-dim text-xs">XP Buff</label>
            <div class="flex items-center gap-1">
              <input
                v-model.number="state.xpBuffPercent"
                type="number"
                min="0"
                max="500"
                step="1"
                class="input w-16 text-center text-xs"
                placeholder="0" />
              <span class="text-text-muted text-[0.65rem]">%</span>
              <span class="text-accent-gold text-[0.65rem] font-semibold ml-1">({{ effectiveMultiplier }}×)</span>
            </div>
          </div>
        </template>
      </div>

      <!-- Spacer matching the resize handle below, so the plan header aligns with
           the center plan box -->
      <div class="w-1.5 mx-1 shrink-0" aria-hidden="true" />

      <!-- Plan header: lines up with the center plan box -->
      <div
        v-if="state.selectedSkill && state.xpTable.length > 0"
        class="flex-1 flex flex-col gap-0.5">
        <span class="text-text-dim text-xs">Planning: Lv {{ planningLevel }} → {{ planningLevel + 1 }}</span>
        <span class="text-text-primary text-xs font-semibold">
          {{ currentLevelXpAccumulated.toLocaleString() }} / {{ currentLevelXpNeeded.toLocaleString() }} XP
        </span>
      </div>
    </div>

    <!-- Main content -->
    <div v-if="!state.selectedSkill" class="flex-1 flex items-center justify-center">
      <EmptyState primary="Select a crafting skill to get started" />
    </div>

    <div v-else class="flex flex-1 min-h-0">
      <!-- Left panel: Recipe list -->
      <div class="shrink-0 flex flex-col gap-2 min-h-0 pr-1" :style="{ width: `${recipePanelWidth}px` }">
        <!-- Filters -->
        <div class="flex items-center gap-3 text-xs">
          <label class="flex items-center gap-1.5 text-text-dim cursor-pointer">
            <input v-model="hideUnknown" type="checkbox" class="accent-accent-gold" />
            Hide unknown
          </label>
          <label class="flex items-center gap-1.5 text-text-dim cursor-pointer">
            <input v-model="hideReducedXp" type="checkbox" class="accent-accent-gold" />
            Hide reduced XP
          </label>
          <label class="flex items-center gap-1.5 text-text-dim cursor-pointer">
            <input v-model="showCosts" type="checkbox" class="accent-accent-gold" />
            Costs
          </label>
          <label class="flex items-center gap-1.5 text-text-dim cursor-pointer">
            <input v-model="showXp" type="checkbox" class="accent-accent-gold" />
            XP
          </label>
          <div class="flex items-center gap-3 ml-auto">
            <div class="flex items-center gap-1">
              <label class="text-text-dim text-[0.65rem]">+N:</label>
              <input
                v-model.number="addQuantity"
                type="number"
                min="1"
                class="input w-14 text-center text-[0.65rem]" />
            </div>
            <div class="flex items-center gap-1">
              <label class="text-text-dim text-[0.65rem]">Min lvl:</label>
              <input
                v-model.number="minLevel"
                type="number"
                min="0"
                class="input w-14 text-center text-[0.65rem]" />
            </div>
          </div>
        </div>

        <!-- Recipe list -->
        <div class="flex-1 overflow-y-auto border border-surface-elevated rounded">
          <div v-if="loading" class="p-4">
            <SkeletonLoader variant="text" :lines="8" />
          </div>
          <div v-else-if="filteredRecipes.length === 0" class="p-4 text-text-muted text-xs text-center">
            No matching recipes found.
          </div>
          <div
            v-for="r in filteredRecipes"
            :key="r.recipe.id"
            class="flex items-center gap-1.5 px-2 py-1 text-xs border-b border-surface-elevated hover:bg-surface-elevated/50 group"
            :class="recipeRowClass(r)">
            <!-- Level req -->
            <span class="text-text-muted font-mono w-7 text-right shrink-0 text-[0.65rem]">
              {{ r.recipe.skill_level_req ?? 0 }}
            </span>
            <!-- Cost -->
            <span v-if="showCosts" class="text-text-muted font-mono w-14 text-right shrink-0 text-[0.65rem]">
              {{ r.cost !== null ? `${r.cost.toLocaleString()}g` : '—' }}
            </span>
            <!-- Separator -->
            <span class="text-text-muted/40 shrink-0">-</span>
            <!-- Action buttons (left of name) -->
            <div class="flex items-center gap-0.5 shrink-0 opacity-0 group-hover:opacity-100 transition-opacity">
              <button
                class="text-[0.6rem] px-1.5 py-0.5 rounded bg-accent-gold/10 text-accent-gold hover:bg-accent-gold/20 border-none cursor-pointer"
                title="Add one craft to the plan"
                @click="addOnce(r)">
                +1
              </button>
              <button
                class="text-[0.6rem] px-1.5 py-0.5 rounded bg-blue-900/30 text-blue-400 hover:bg-blue-900/50 border-none cursor-pointer"
                :title="`Add ${addQuantity || 1} to the plan`"
                @click="addMultiple(r, addQuantity || 1)">
                +N
              </button>
              <button
                class="text-[0.6rem] px-1.5 py-0.5 rounded bg-green-900/30 text-green-400 hover:bg-green-900/50 border-none cursor-pointer"
                title="Add enough crafts to reach next level"
                @click="addToLevel(r)">
                +Lvl
              </button>
            </div>
            <!-- Recipe name -->
            <span class="truncate min-w-0">
              <RecipeInline :reference="r.recipe.name" inherit-color />
            </span>
            <!-- XP info — show the combined first-craft total while the first-time bonus
                 is still available, then fall back to base XP once it's been used. -->
            <span
              v-if="showXp"
              class="text-[0.6rem] shrink-0 ml-auto"
              :class="r.firstTimeXp > 0 ? 'text-accent-gold' : 'text-text-muted'"
              :title="r.firstTimeXp > 0
                ? `${(r.effectiveXp + r.effectiveFirstTimeXp).toLocaleString()} XP first craft (incl. +${r.effectiveFirstTimeXp.toLocaleString()} first-time bonus), ${r.effectiveXp.toLocaleString()} XP thereafter`
                : `${r.effectiveXp.toLocaleString()} XP per craft`">
              {{ (r.firstTimeXp > 0 ? r.effectiveXp + r.effectiveFirstTimeXp : r.effectiveXp).toLocaleString() }}xp
            </span>
          </div>
        </div>
      </div>

      <!-- Resize handle -->
      <div
        class="w-1.5 shrink-0 cursor-col-resize flex items-center justify-center hover:bg-accent-gold/20 rounded transition-colors mx-1"
        @mousedown="onResizeStart">
        <div class="w-px h-8 bg-border-light" />
      </div>

      <!-- Center panel: Leveling plan -->
      <div class="flex-1 overflow-y-auto border border-surface-elevated rounded p-3 min-h-0">
        <div v-if="state.planLevels.length === 0" class="text-text-muted text-xs text-center py-8">
          Add recipes from the list to build your leveling plan.
        </div>

        <div v-else class="flex flex-col gap-3">
          <div v-for="(lvl, idx) in state.planLevels" :key="`${lvl.from_level}-${lvl.to_level}`">
            <!-- Level header -->
            <div class="flex items-center gap-2 mb-1.5">
              <span class="text-text-secondary text-xs font-semibold">
                Lv {{ lvl.from_level + state.bonusLevels }} → {{ lvl.to_level + state.bonusLevels }}
              </span>
              <span class="text-text-muted text-[0.65rem]">
                {{ lvl.xp_accumulated.toLocaleString() }} / {{ lvl.xp_needed.toLocaleString() }} XP
              </span>
              <!-- Progress indicator -->
              <span
                v-if="lvl.xp_accumulated >= lvl.xp_needed"
                class="text-green-400 text-[0.6rem] font-semibold">
                DONE
              </span>
              <div class="flex-1 border-b border-border-default ml-2" />
              <!-- Clear level button (only for topmost incomplete level) -->
              <button
                v-if="idx === 0 && lvl.xp_accumulated < lvl.xp_needed"
                class="text-text-muted text-[0.6rem] cursor-pointer bg-transparent border-none hover:text-accent-red"
                title="Clear this level's entries"
                @click="clearCurrentLevel">
                clear
              </button>
            </div>

            <!-- XP progress bar -->
            <div class="w-full h-1 bg-surface-base rounded-full mb-2 overflow-hidden">
              <div
                class="h-full rounded-full transition-all duration-300"
                :class="lvl.xp_accumulated >= lvl.xp_needed ? 'bg-green-500' : 'bg-accent-gold'"
                :style="{ width: `${Math.min(100, (lvl.xp_accumulated / lvl.xp_needed) * 100)}%` }" />
            </div>

            <!-- Entries -->
            <div v-if="lvl.entries.length === 0" class="text-text-muted text-[0.65rem] pl-2">
              No recipes added yet.
            </div>
            <div
              v-for="(entry, eIdx) in lvl.entries"
              :key="`${lvl.from_level}-${eIdx}`"
              class="flex items-center gap-2 px-2 py-1 text-xs bg-surface-base border border-surface-elevated rounded mb-1">
              <span class="text-text-primary truncate flex-1">{{ entry.recipe_name }}</span>
              <span class="text-text-primary font-mono shrink-0 flex items-center gap-0.5">
                ×<input
                  :value="entry.craft_count"
                  type="number"
                  min="1"
                  class="input w-12 text-center text-xs py-0 px-0.5 font-mono"
                  @change="updateEntryCount(idx, eIdx, Math.max(1, parseInt(($event.target as HTMLInputElement).value) || 1))" />
              </span>
              <span class="text-accent-gold text-[0.65rem] shrink-0">
                {{ entry.total_xp.toLocaleString() }} XP
              </span>
              <span v-if="entry.estimated_cost > 0" class="text-text-muted text-[0.65rem] shrink-0">
                {{ entry.estimated_cost.toLocaleString() }}g
              </span>
              <!-- Remove entry -->
              <button
                class="text-text-muted text-[0.6rem] cursor-pointer bg-transparent border-none hover:text-accent-red"
                title="Remove this entry"
                @click="removeEntry(idx, eIdx)">
                ×
              </button>
            </div>
          </div>

          <!-- Plan totals + actions -->
          <div class="bg-surface-elevated rounded p-2 border border-border-light mt-2">
            <div class="grid grid-cols-2 gap-x-4 gap-y-0.5 text-[0.65rem]">
              <div>
                <span class="text-text-dim">Levels planned:</span>
                <span class="text-text-primary font-semibold ml-1">
                  {{ state.currentLevel }} → {{ planningLevel + (currentLevelComplete ? 1 : 0) }}
                </span>
              </div>
              <div>
                <span class="text-text-dim">Total crafts:</span>
                <span class="text-text-primary font-semibold ml-1">
                  {{ totalCrafts.toLocaleString() }}
                </span>
              </div>
              <div v-if="totalCost > 0">
                <span class="text-text-dim">Est. total cost:</span>
                <span class="text-accent-red font-semibold ml-1">{{ totalCost.toLocaleString() }}g</span>
              </div>
            </div>
            <div class="flex gap-2 mt-2">
              <button
                class="btn-primary text-[0.65rem] py-1 px-3"
                :disabled="creatingProject"
                @click="onCreateProject">
                {{ creatingProject ? 'Creating...' : 'Create Project' }}
              </button>
              <button
                class="text-[0.65rem] px-3 py-1 rounded bg-surface-base text-text-muted hover:text-accent-red border border-border-light cursor-pointer"
                @click="clearPlan">
                Clear Plan
              </button>
            </div>
          </div>
        </div>
      </div>

      <!-- Right panel: Materials summary -->
      <div class="w-72 shrink-0 overflow-y-auto border border-surface-elevated rounded p-3 min-h-0 flex flex-col gap-2">
        <h4 class="text-text-secondary text-xs font-semibold uppercase tracking-wide m-0">Materials</h4>

        <div v-if="state.planLevels.length === 0" class="text-text-muted text-[0.65rem] text-center py-4">
          Materials will appear here as you add recipes to your plan.
        </div>

        <div v-else-if="materialsLoading" class="text-text-muted text-[0.65rem] text-center py-4">
          Calculating materials...
        </div>

        <template v-else-if="aggregatedMaterials.length > 0">
          <div class="text-text-muted text-[0.65rem] mb-1">
            {{ aggregatedMaterials.length }} items needed
          </div>
          <DataTable
            :columns="materialColumns"
            :row-key="(row) => String(row.key)"
            compact
            :rows="(aggregatedMaterials as unknown as Record<string, unknown>[])">
            <template #cell-item_name="{ row }">
              <template v-if="row.is_dynamic">
                <span class="text-accent-gold/60 text-[0.65rem] mr-1">&#9670;</span>
                <span class="text-text-secondary text-[0.65rem]">{{ row.item_name }}</span>
              </template>
              <ItemInline v-else-if="row.item_id !== null" :reference="(row.item_name as string)" />
              <span v-else class="text-text-muted text-[0.65rem]">{{ row.item_name }}</span>
            </template>
            <template #cell-expected_quantity="{ value }">
              <span class="font-mono text-text-primary text-[0.65rem]">{{ Math.ceil(value as number).toLocaleString() }}</span>
            </template>
          </DataTable>
        </template>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, onMounted, watch } from "vue";
import { storeToRefs } from "pinia";
import { invoke } from "@tauri-apps/api/core";
import { useGameDataStore } from "../../stores/gameDataStore";
import { useCraftingStore } from "../../stores/craftingStore";
import { useGameStateStore } from "../../stores/gameStateStore";
import type { RecipeInfo } from "../../types/gameData/recipes";
import type { FlattenedMaterial } from "../../types/crafting";
import { xpDropOffMultiplier } from "../../utils/craftingXp";
import DataTable, { type ColumnDef } from "../Shared/DataTable.vue";
import EmptyState from "../Shared/EmptyState.vue";
import SkeletonLoader from "../Shared/SkeletonLoader.vue";
import ItemInline from "../Shared/Item/ItemInline.vue";
import RecipeInline from "../Shared/Recipe/RecipeInline.vue";

const gameData = useGameDataStore();
const craftingStore = useCraftingStore();
const gameStateStore = useGameStateStore();

// ── Persistent state (lives in the store) ─────────────────────────────────

const { levelingState: state } = storeToRefs(craftingStore);

// ── Local-only UI state ───────────────────────────────────────────────────

const craftingSkills = ref<string[]>([]);
const loading = ref(false);
const creatingProject = ref(false);
const materialsLoading = ref(false);

// Shared quantity input for "+N" button
const addQuantity = ref<number>(1);

// Recipe panel resize. Default sized so the filter toolbar (Hide unknown …
// Min lvl) fits on one line without the checkbox labels wrapping or the Min-lvl
// input spilling past the panel's right edge.
const recipePanelWidth = ref(600);

function onResizeStart(e: MouseEvent) {
  const startX = e.clientX;
  const startWidth = recipePanelWidth.value;

  function onMouseMove(ev: MouseEvent) {
    const delta = ev.clientX - startX;
    recipePanelWidth.value = Math.max(280, Math.min(800, startWidth + delta));
  }
  function onMouseUp() {
    document.removeEventListener("mousemove", onMouseMove);
    document.removeEventListener("mouseup", onMouseUp);
  }
  document.addEventListener("mousemove", onMouseMove);
  document.addEventListener("mouseup", onMouseUp);
}

// Filters (local — not worth persisting)
const hideUnknown = ref(false);
const hideReducedXp = ref(false);
const showCosts = ref(false);
const showXp = ref(true);
const minLevel = ref(0);

// Recipe list data (rebuilt from store state on mount/skill change)
interface EnrichedRecipe {
  recipe: RecipeInfo
  isKnown: boolean
  isCrafted: boolean
  /** True when the base skill level is at/past the recipe's drop-off level */
  isDropOff: boolean
  isTooHigh: boolean
  /** Base XP per craft (before buff) */
  xpPerCraft: number
  /** Extra XP added on top of one regular (buffed + drop-off) craft so the
   *  first-craft total is base × FIRST_CRAFT_XP_MULTIPLIER. 0 once crafted. */
  firstTimeXp: number
  /** XP drop-off multiplier (0..1) at the current planning base level — repeat crafts only */
  dropOffMult: number
  /** Per-craft XP for the 2nd+ crafts: base × buff × drop-off */
  effectiveXp: number
  /** Same as firstTimeXp; the first-craft bonus is never buffed or dropped off */
  effectiveFirstTimeXp: number
  cost: number | null
}

const allRecipes = ref<EnrichedRecipe[]>([]);

// Aggregated materials for the plan
const aggregatedMaterials = ref<FlattenedMaterial[]>([]);

const materialColumns: ColumnDef[] = [
  { key: "item_name", label: "Item", align: "left" },
  { key: "expected_quantity", label: "Qty", align: "right", width: "3.5rem", numeric: true },
];

// Game state XP (for "use current" button)
const gameStateXp = ref<number | null>(null);

// ── Computed ─────────────────────────────────────────────────────────────────

const effectiveMultiplier = computed(() =>
  (1 + (state.value.xpBuffPercent || 0) / 100).toFixed(2),
);

const multiplier = computed(() => 1 + (state.value.xpBuffPercent || 0) / 100);

// A recipe's first craft awards a flat 4× its base XP. This first-craft total is
// fixed: it is NOT scaled by the XP buff and NOT reduced by the skill-level drop-off.
// The over-level drop-off applies only to repeat (2nd+) crafts.
const FIRST_CRAFT_XP_MULTIPLIER = 4;

/** Base level (total minus bonus) — used for XP table indexing */
const baseLevel = computed(() => state.value.currentLevel - state.value.bonusLevels);

/** The base level we're currently planning for (topmost incomplete level) — used for XP table indexing */
const planningBaseLevel = computed(() => {
  if (state.value.planLevels.length === 0) return baseLevel.value;
  const top = state.value.planLevels[0];
  return top.from_level;
});

/**
 * Total level (planning base + synergy/bonus) — used for recipe-unlock checks only.
 * Do NOT use this for XP drop-off: the over-level penalty is based on base level, since
 * synergy/bonus levels don't count toward it. Use planningBaseLevel for drop-off.
 */
const planningLevel = computed(() => planningBaseLevel.value + state.value.bonusLevels);

const currentLevelComplete = computed(() => {
  if (state.value.planLevels.length === 0) return false;
  const top = state.value.planLevels[0];
  return top.xp_accumulated >= top.xp_needed;
});

const currentLevelXpNeeded = computed(() => {
  if (state.value.planLevels.length === 0) {
    return state.value.xpTable[baseLevel.value] ?? 0;
  }
  return state.value.planLevels[0].xp_needed;
});

const currentLevelXpAccumulated = computed(() => {
  if (state.value.planLevels.length === 0) return state.value.startingXp;
  return state.value.planLevels[0].xp_accumulated;
});

const totalCrafts = computed(() =>
  state.value.planLevels.reduce((sum, lvl) =>
    sum + lvl.entries.reduce((s, e) => s + e.craft_count, 0), 0),
);

const totalCost = computed(() =>
  state.value.planLevels.reduce((sum, lvl) =>
    sum + lvl.entries.reduce((s, e) => s + e.estimated_cost, 0), 0),
);

const filteredRecipes = computed(() => {
  return allRecipes.value.filter((r) => {
    if (hideUnknown.value && !r.isKnown) return false;
    if (hideReducedXp.value && r.isDropOff) return false;
    if ((r.recipe.skill_level_req ?? 0) < minLevel.value) return false;
    return true;
  });
});

// ── Init ─────────────────────────────────────────────────────────────────────

onMounted(async () => {
  const allSkills = await gameData.getAllSkills();
  const skillsWithRecipes: string[] = [];
  for (const skill of allSkills) {
    if (!skill.xp_table) continue;
    const recipes = await gameData.getRecipesForSkill(skill.name);
    if (recipes.length > 0) {
      skillsWithRecipes.push(skill.name);
    }
  }
  craftingSkills.value = skillsWithRecipes.sort((a, b) => a.localeCompare(b));

  // If we have persisted state, rebuild recipe list
  if (state.value.selectedSkill) {
    loading.value = true;
    await loadRecipes();
    await recomputeMaterials();
    loading.value = false;
  }
});

// ── Skill change ─────────────────────────────────────────────────────────────

async function onSkillChange() {
  state.value.planLevels = [];
  allRecipes.value = [];
  state.value.xpTable = [];
  state.value.snapshotLevel = null;
  state.value.currentLevel = 0;
  state.value.bonusLevels = 0;
  state.value.startingXp = 0;
  gameStateXp.value = null;
  aggregatedMaterials.value = [];

  if (!state.value.selectedSkill) return;

  loading.value = true;

  // Get current level from game state
  const skillData = await craftingStore.getSkillLevel(state.value.selectedSkill);
  if (skillData) {
    state.value.snapshotLevel = skillData.totalLevel;
    state.value.currentLevel = skillData.totalLevel;
    state.value.bonusLevels = skillData.bonusLevels;
    state.value.startingXp = skillData.xpTowardNext;
    gameStateXp.value = skillData.xpTowardNext;
  }

  // Load XP table
  try {
    state.value.xpTable = await invoke<number[]>("get_xp_table_for_skill", { skillName: state.value.selectedSkill });
  } catch {
    state.value.xpTable = [];
  }

  // Load recipes
  await loadRecipes();
  loading.value = false;
}

async function loadRecipes() {
  const skillInfo = await gameData.resolveSkill(state.value.selectedSkill);
  const skillInternalName = skillInfo?.internal_name ?? state.value.selectedSkill;

  const raw = await gameData.getRecipesForSkill(state.value.selectedSkill);
  // Filter to recipes that actually reward XP in this skill
  const relevant = raw.filter(
    (r) => r.reward_skill === skillInternalName
      && ((r.reward_skill_xp ?? 0) > 0 || (r.reward_skill_xp_first_time ?? 0) > 0),
  );

  const completionMap = gameStateStore.recipeCompletions;
  const knownKeys = gameStateStore.knownRecipeKeys;
  const hasCompletionData = Object.keys(completionMap).length > 0;

  // Batch load costs
  const costMap = new Map<number, number>();
  if (showCosts.value) {
    await loadCosts(relevant, costMap);
  }

  // Effective level (base + synergy/bonus) gates which recipes are unlocked.
  const pLevel = planningLevel.value;
  // The over-level drop-off is computed from the BASE skill level only — synergy/bonus
  // levels do NOT count toward it. It applies to repeat crafts, never the first craft.
  const pBaseLevel = planningBaseLevel.value;

  allRecipes.value = relevant
    .map((recipe) => {
      const key = `Recipe_${recipe.id}`;
      const isKnown = !hasCompletionData || key in completionMap;
      const isCrafted = knownKeys.has(key);
      const levelReq = recipe.skill_level_req ?? 0;
      const dropOff = recipe.reward_skill_xp_drop_off_level;
      const isDropOff = dropOff !== null && dropOff !== undefined && pBaseLevel >= dropOff;
      const isTooHigh = levelReq > pLevel;

      const xpPerCraft = recipe.reward_skill_xp ?? 0;
      const dropOffMult = xpDropOffMultiplier(pBaseLevel, dropOff);
      // Repeat (2nd+) crafts: base × buff × drop-off.
      const effectiveXp = Math.round(xpPerCraft * multiplier.value * dropOffMult);
      // The first craft awards a flat 4× base XP total (unbuffed, no drop-off).
      // Store the first-time bonus as the amount added on top of one repeat craft so
      // that effectiveXp + firstTimeXp == base × FIRST_CRAFT_XP_MULTIPLIER.
      const hasFirstTime = !isCrafted && (recipe.reward_skill_xp_first_time ?? 0) > 0;
      const firstTimeXp = hasFirstTime
        ? Math.max(0, xpPerCraft * FIRST_CRAFT_XP_MULTIPLIER - effectiveXp)
        : 0;

      return {
        recipe,
        isKnown,
        isCrafted,
        isDropOff,
        isTooHigh,
        xpPerCraft,
        firstTimeXp,
        dropOffMult,
        effectiveXp,
        effectiveFirstTimeXp: firstTimeXp,
        cost: costMap.get(recipe.id) ?? null,
      };
    })
    .sort((a, b) => (a.recipe.skill_level_req ?? 0) - (b.recipe.skill_level_req ?? 0));
}

async function loadCosts(recipes: RecipeInfo[], costMap: Map<number, number>) {
  for (const recipe of recipes) {
    const cost = await craftingStore.estimateRecipeCost(recipe);
    costMap.set(recipe.id, cost);
  }
}

// Re-enrich recipes when planning level or multiplier changes
watch([planningLevel, multiplier], () => {
  if (allRecipes.value.length === 0) return;
  refreshRecipeState();
});

// Re-scale already-planned entries live as the XP buff changes (repeat crafts only;
// first crafts stay at the flat base × 4 with no bonus — see recomputeEntryXpForBuff).
watch(multiplier, () => {
  if (state.value.planLevels.length === 0) return;
  recomputeEntryXpForBuff();
});

// Reload costs when showCosts toggled on
watch(showCosts, async (show) => {
  if (!show || allRecipes.value.length === 0) return;
  const costMap = new Map<number, number>();
  const recipes = allRecipes.value.map((r) => r.recipe);
  await loadCosts(recipes, costMap);
  allRecipes.value = allRecipes.value.map((r) => ({
    ...r,
    cost: costMap.get(r.recipe.id) ?? r.cost,
  }));
});

function refreshRecipeState() {
  const pLevel = planningLevel.value;       // effective (base + synergy) — recipe unlock
  const pBaseLevel = planningBaseLevel.value; // base only — XP drop-off
  const mult = multiplier.value;

  allRecipes.value = allRecipes.value.map((r) => {
    const dropOff = r.recipe.reward_skill_xp_drop_off_level;
    const isDropOff = dropOff !== null && dropOff !== undefined && pBaseLevel >= dropOff;
    const isTooHigh = (r.recipe.skill_level_req ?? 0) > pLevel;
    const dropOffMult = xpDropOffMultiplier(pBaseLevel, dropOff);
    const effectiveXp = Math.round(r.xpPerCraft * mult * dropOffMult);
    // First craft total stays at base × 4 (unbuffed, no drop-off); recompute the bonus
    // so effectiveXp + firstTimeXp keeps equalling that total as buff/level change.
    const firstTimeXp = r.firstTimeXp > 0
      ? Math.max(0, r.xpPerCraft * FIRST_CRAFT_XP_MULTIPLIER - effectiveXp)
      : 0;

    return {
      ...r,
      isDropOff,
      isTooHigh,
      dropOffMult,
      effectiveXp,
      firstTimeXp,
      effectiveFirstTimeXp: firstTimeXp,
    };
  });
}

// ── Color coding ─────────────────────────────────────────────────────────────

function recipeRowClass(r: EnrichedRecipe): string {
  if (r.isTooHigh) return "text-blue-400/70";
  if (r.isDropOff) {
    if (!r.isKnown) return "text-red-900";
    if (!r.isCrafted) return "text-green-800";
    return "text-text-muted/50"; // grey
  }
  // Good XP range
  if (!r.isKnown) return "text-red-400";
  if (!r.isCrafted) return "text-green-400";
  return "text-text-primary"; // white
}

// ── Materials computation ─────────────────────────────────────────────────

/** Map of loaded recipes by ID for material resolution */
const recipeMap = computed(() => {
  const map = new Map<number, RecipeInfo>();
  for (const r of allRecipes.value) {
    map.set(r.recipe.id, r.recipe);
  }
  return map;
});

async function recomputeMaterials() {
  if (state.value.planLevels.length === 0) {
    aggregatedMaterials.value = [];
    return;
  }

  materialsLoading.value = true;

  // Aggregate recipe craft counts across all levels
  const recipeQuantities = new Map<number, number>();
  for (const lvl of state.value.planLevels) {
    for (const entry of lvl.entries) {
      recipeQuantities.set(
        entry.recipe_id,
        (recipeQuantities.get(entry.recipe_id) ?? 0) + entry.craft_count,
      );
    }
  }

  // Resolve ingredients for each recipe and flatten
  const combined = new Map<string, FlattenedMaterial>();

  for (const [recipeId, craftCount] of recipeQuantities) {
    const recipe = recipeMap.value.get(recipeId);
    if (!recipe) continue;

    // Leveling plans are in crafts, not output items — pass quantityIsCrafts so
    // multi-yield recipes (e.g. 2 items per craft) don't understate materials.
    const resolved = await craftingStore.resolveRecipeIngredients(
      recipe,
      craftCount,
      false,
      new Set(),
      undefined,
      undefined,
      true,
    );
    const flat = craftingStore.flattenIngredients(resolved.ingredients);

    for (const [key, mat] of flat) {
      const existing = combined.get(key);
      if (existing) {
        existing.quantity += mat.quantity;
        existing.expected_quantity += mat.expected_quantity;
      } else {
        combined.set(key, { ...mat });
      }
    }
  }

  aggregatedMaterials.value = [...combined.values()].sort((a, b) =>
    a.item_name.localeCompare(b.item_name),
  );
  materialsLoading.value = false;
}

// Debounced material recomputation — triggers when the plan changes
let materialTimer: ReturnType<typeof setTimeout> | null = null;

watch(
  () => state.value.planLevels.map(l => l.entries.length + l.xp_accumulated),
  () => {
    if (materialTimer) clearTimeout(materialTimer);
    materialTimer = setTimeout(() => recomputeMaterials(), 300);
  },
  { deep: true },
);

// ── Plan actions ─────────────────────────────────────────────────────────────

function ensureCurrentLevel() {
  if (state.value.planLevels.length === 0 || state.value.planLevels[0].xp_accumulated >= state.value.planLevels[0].xp_needed) {
    // Need a new level at the top — use base level for XP table indexing
    const lvl = planningBaseLevel.value + (state.value.planLevels.length > 0 && state.value.planLevels[0].xp_accumulated >= state.value.planLevels[0].xp_needed ? 1 : 0);
    const xpNeeded = state.value.xpTable[lvl] ?? 0;
    if (xpNeeded <= 0) return; // Can't level further

    // For the very first level in the plan, pre-populate with starting XP
    // (XP the player has already earned toward this level)
    const isFirstLevel = state.value.planLevels.length === 0;
    const initialXp = isFirstLevel ? state.value.startingXp : 0;

    state.value.planLevels.unshift({
      from_level: lvl,
      to_level: lvl + 1,
      xp_needed: xpNeeded,
      xp_accumulated: Math.min(initialXp, xpNeeded),
      entries: [],
    });
  }
}

function addOnce(r: EnrichedRecipe) {
  ensureCurrentLevel();
  if (state.value.planLevels.length === 0) return;

  const top = state.value.planLevels[0];
  const totalXpForOne = r.effectiveXp + r.effectiveFirstTimeXp;
  const costForOne = r.cost ?? 0;

  top.entries.push({
    recipe_id: r.recipe.id,
    recipe_name: r.recipe.name,
    craft_count: 1,
    xp_per_craft: r.xpPerCraft,
    xp_first_time: r.firstTimeXp,
    xp_drop_off_mult: r.dropOffMult,
    total_xp: totalXpForOne,
    estimated_cost: costForOne,
  });
  top.xp_accumulated += totalXpForOne;

  // Mark first-time bonus as used for this recipe
  if (r.firstTimeXp > 0) {
    markFirstTimeUsed(r);
  }

  // If we've hit the XP target, auto-advance
  checkAutoAdvance();
}

function addMultiple(r: EnrichedRecipe, count: number) {
  if (count <= 0 || r.effectiveXp <= 0) return;

  const costPerCraft = r.cost ?? 0;
  let remaining = count;
  let firstTimePending = r.firstTimeXp > 0;

  while (remaining > 0) {
    ensureCurrentLevel();
    if (state.value.planLevels.length === 0) return;

    const top = state.value.planLevels[0];
    const xpRoom = top.xp_needed - top.xp_accumulated;
    if (xpRoom <= 0) {
      // Level already full, ensureCurrentLevel should have created a new one
      // but if xpTable ran out, bail
      break;
    }

    // Figure out how many crafts fit in this level
    let craftsThisLevel: number;
    let xpThisLevel: number;
    let firstTimeUsed = false;

    if (firstTimePending) {
      // First craft gets the bonus
      const firstCraftXp = r.effectiveXp + r.effectiveFirstTimeXp;
      if (firstCraftXp >= xpRoom) {
        // Just the first craft fills (or exceeds) this level
        craftsThisLevel = 1;
        xpThisLevel = firstCraftXp;
        firstTimeUsed = true;
      } else {
        // First craft fits, see how many more fit
        const roomAfterFirst = xpRoom - firstCraftXp;
        const additionalCrafts = Math.min(remaining - 1, Math.ceil(roomAfterFirst / r.effectiveXp));
        craftsThisLevel = 1 + additionalCrafts;
        xpThisLevel = firstCraftXp + additionalCrafts * r.effectiveXp;
        firstTimeUsed = true;
      }
    } else {
      craftsThisLevel = Math.min(remaining, Math.ceil(xpRoom / r.effectiveXp));
      xpThisLevel = craftsThisLevel * r.effectiveXp;
    }

    top.entries.push({
      recipe_id: r.recipe.id,
      recipe_name: r.recipe.name,
      craft_count: craftsThisLevel,
      xp_per_craft: r.xpPerCraft,
      xp_first_time: firstTimeUsed ? r.firstTimeXp : 0,
      xp_drop_off_mult: r.dropOffMult,
      total_xp: xpThisLevel,
      estimated_cost: costPerCraft * craftsThisLevel,
    });
    top.xp_accumulated += xpThisLevel;

    if (firstTimeUsed) {
      markFirstTimeUsed(r);
      firstTimePending = false;
    }

    remaining -= craftsThisLevel;

    // Advance level if full
    checkAutoAdvance();
  }
}

function addToLevel(r: EnrichedRecipe) {
  ensureCurrentLevel();
  if (state.value.planLevels.length === 0) return;

  const top = state.value.planLevels[0];
  const remaining = top.xp_needed - top.xp_accumulated;
  if (remaining <= 0) return;

  // First craft might include first-time bonus
  let xpFromFirst = 0;
  let craftsUsed = 0;

  if (r.firstTimeXp > 0) {
    // First craft gets bonus
    xpFromFirst = r.effectiveXp + r.effectiveFirstTimeXp;
    craftsUsed = 1;
    if (xpFromFirst >= remaining) {
      // One craft with bonus is enough
      const costForOne = r.cost ?? 0;
      top.entries.push({
        recipe_id: r.recipe.id,
        recipe_name: r.recipe.name,
        craft_count: 1,
        xp_per_craft: r.xpPerCraft,
        xp_first_time: r.firstTimeXp,
        xp_drop_off_mult: r.dropOffMult,
        total_xp: xpFromFirst,
        estimated_cost: costForOne,
      });
      top.xp_accumulated += xpFromFirst;
      markFirstTimeUsed(r);
      checkAutoAdvance();
      return;
    }
  }

  // Remaining after potential first-time craft
  const afterBonus = remaining - xpFromFirst;
  const additionalCrafts = r.effectiveXp > 0 ? Math.ceil(afterBonus / r.effectiveXp) : 0;
  const planTotalCrafts = craftsUsed + additionalCrafts;

  if (planTotalCrafts <= 0) return;

  const totalXp = xpFromFirst + additionalCrafts * r.effectiveXp;
  const entryCost = (r.cost ?? 0) * planTotalCrafts;

  top.entries.push({
    recipe_id: r.recipe.id,
    recipe_name: r.recipe.name,
    craft_count: planTotalCrafts,
    xp_per_craft: r.xpPerCraft,
    xp_first_time: craftsUsed > 0 ? r.firstTimeXp : 0,
    xp_drop_off_mult: r.dropOffMult,
    total_xp: totalXp,
    estimated_cost: entryCost,
  });
  top.xp_accumulated += totalXp;

  if (r.firstTimeXp > 0 && craftsUsed > 0) {
    markFirstTimeUsed(r);
  }

  checkAutoAdvance();
}

function markFirstTimeUsed(r: EnrichedRecipe) {
  // Update the recipe in the list so subsequent adds don't include first-time bonus
  allRecipes.value = allRecipes.value.map((ar) => {
    if (ar.recipe.id !== r.recipe.id) return ar;
    return {
      ...ar,
      isCrafted: true,
      firstTimeXp: 0,
      effectiveFirstTimeXp: 0,
    };
  });
}

function checkAutoAdvance() {
  // If the current level is complete, recipe state needs refreshing
  // (drop-off and too-high will change for the next level)
  if (state.value.planLevels.length > 0 && state.value.planLevels[0].xp_accumulated >= state.value.planLevels[0].xp_needed) {
    refreshRecipeState();
  }
}

function removeEntry(levelIdx: number, entryIdx: number) {
  const lvl = state.value.planLevels[levelIdx];
  const entry = lvl.entries[entryIdx];
  lvl.xp_accumulated -= entry.total_xp;
  lvl.entries.splice(entryIdx, 1);

  // If level is now empty, remove it (unless it's the only level)
  if (lvl.entries.length === 0 && state.value.planLevels.length > 1) {
    state.value.planLevels.splice(levelIdx, 1);
  }

  // If the removed entry used a first-time bonus, check if any other entry
  // in the plan still claims it. If not, restore the recipe's first-time XP.
  if (entry.xp_first_time > 0) {
    const stillUsed = state.value.planLevels.some(l =>
      l.entries.some(e => e.recipe_id === entry.recipe_id && e.xp_first_time > 0),
    );
    if (!stillUsed) {
      // Restore the exact static bonus that was consumed by this entry. The
      // entry already stored it as `xp_first_time` (= total first-craft XP − base)
      // when it was added, with no buff multiplier and no drop-off applied. Reusing
      // it avoids recomputing from the mutated recipe copy.
      const restored = entry.xp_first_time;
      allRecipes.value = allRecipes.value.map(ar => {
        if (ar.recipe.id !== entry.recipe_id) return ar;
        return {
          ...ar,
          isCrafted: false,
          firstTimeXp: restored,
          effectiveFirstTimeXp: restored,
        };
      });
    }
  }

  refreshRecipeState();
}

function updateEntryCount(levelIdx: number, entryIdx: number, newCount: number) {
  if (newCount < 1) return;
  const lvl = state.value.planLevels[levelIdx];
  const entry = lvl.entries[entryIdx];
  const oldXp = entry.total_xp;
  const oldCost = entry.estimated_cost;
  const costPerCraft = entry.craft_count > 0 ? oldCost / entry.craft_count : 0;

  // Recalculate XP: the first-time bonus applies once (it already encodes the flat
  // base × 4 first-craft total, unbuffed and drop-off-free). Repeat crafts get the
  // buff and the over-level drop-off captured when the entry was added.
  const mult = 1 + (state.value.xpBuffPercent || 0) / 100;
  const dropOffMult = entry.xp_drop_off_mult ?? 1;
  const effectiveFirstTime = entry.xp_first_time;
  const effectiveXpPerCraft = Math.round(entry.xp_per_craft * mult * dropOffMult);

  entry.craft_count = newCount;
  entry.total_xp = (entry.xp_first_time > 0 ? effectiveFirstTime : 0) + newCount * effectiveXpPerCraft;
  entry.estimated_cost = costPerCraft * newCount;

  lvl.xp_accumulated += entry.total_xp - oldXp;

  refreshRecipeState();
}

/**
 * Re-scale every planned entry to the current XP buff. Repeat (2nd+) crafts scale with
 * the buff and the entry's captured over-level drop-off; the first craft of a recipe is
 * a flat base × FIRST_CRAFT_XP_MULTIPLIER and gains NOTHING from any XP bonus, so we
 * re-derive the stored first-time bonus against the new buff to keep that total pinned.
 * xp_accumulated is adjusted by each entry's delta so the starting-XP seed on the
 * bottom level is preserved.
 */
function recomputeEntryXpForBuff() {
  const mult = multiplier.value;
  for (const lvl of state.value.planLevels) {
    for (const entry of lvl.entries) {
      const dropOffMult = entry.xp_drop_off_mult ?? 1;
      const effectiveXpPerCraft = Math.round(entry.xp_per_craft * mult * dropOffMult);
      const hasFirst = entry.xp_first_time > 0;
      if (hasFirst) {
        entry.xp_first_time = Math.max(0, entry.xp_per_craft * FIRST_CRAFT_XP_MULTIPLIER - effectiveXpPerCraft);
      }
      const oldXp = entry.total_xp;
      entry.total_xp = (hasFirst ? entry.xp_first_time : 0) + entry.craft_count * effectiveXpPerCraft;
      lvl.xp_accumulated += entry.total_xp - oldXp;
    }
  }
}

function clearCurrentLevel() {
  if (state.value.planLevels.length === 0) return;
  const top = state.value.planLevels[0];
  top.entries = [];
  top.xp_accumulated = 0;

  // If there are completed levels below and this one is now empty, remove it
  if (state.value.planLevels.length > 1) {
    state.value.planLevels.shift();
  }

  refreshRecipeState();
}

function clearPlan() {
  state.value.planLevels = [];
  aggregatedMaterials.value = [];
}

async function onCreateProject() {
  if (state.value.planLevels.length === 0) return;
  creatingProject.value = true;
  try {
    const fromLvl = state.value.currentLevel;
    const toLvl = planningLevel.value + (currentLevelComplete.value ? 1 : 0);
    const name = `${state.value.selectedSkill} Lv${fromLvl}→${toLvl}`;
    await craftingStore.createProjectFromLevelingPlan(name);
  } finally {
    creatingProject.value = false;
  }
}
</script>
