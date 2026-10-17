<template>
  <div class="flex flex-col gap-4 h-[calc(100vh-200px)] overflow-y-auto">
    <div class="flex items-center justify-between">
      <h3 class="text-text-primary text-sm font-semibold m-0">Crafting History</h3>
      <button
        class="text-text-muted text-xs cursor-pointer bg-transparent border border-border-light rounded px-2 py-0.5 hover:text-text-primary hover:border-border-default"
        :disabled="loading"
        @click="refresh">
        {{ loading ? 'Loading...' : 'Refresh' }}
      </button>
    </div>

    <div v-if="loading" class="space-y-3">
      <SkeletonLoader variant="rect" height="h-10" />
      <DataTableSkeleton :columns="5" :rows="6" />
    </div>

    <EmptyState v-else-if="!hasData" variant="panel" primary="No crafting data available" secondary="Import a character report to see crafting history and stats." />

    <template v-else>
      <!-- Skill progress overview -->
      <SkillCraftingProgress :stats="skillStats" />

      <!-- Top crafted recipes -->
      <div class="flex flex-col gap-2">
        <div class="flex items-center justify-between">
          <h4 class="text-text-secondary text-xs font-semibold uppercase tracking-wide m-0">
            Top Crafted Recipes
          </h4>
          <!-- Skill filter -->
          <select v-model="filterSkill" class="input text-xs w-44">
            <option value="">All skills</option>
            <option v-for="skill in availableSkills" :key="skill" :value="skill">
              {{ skill }}
            </option>
          </select>
        </div>

        <DataTable
          :columns="columns"
          :rows="recipeRows"
          :row-key="(row) => String(row.recipe_key)"
          compact
          :hoverable="false"
          empty-variant="compact"
          :empty-text="emptyText">
          <template #cell-idx="{ value }">
            <span class="text-text-muted">{{ value }}</span>
          </template>
          <template #cell-recipe_name="{ value }">
            <RecipeInline :reference="(value as string)" />
          </template>
          <template #cell-reward_skill="{ value }">
            <SkillInline v-if="value" :reference="(value as string)" />
            <span v-else class="text-text-muted">—</span>
          </template>
          <template #cell-skill_level_req="{ value }">
            <span class="text-text-muted">{{ value ?? '—' }}</span>
          </template>
          <template #cell-completions="{ value }">
            <span class="text-text-primary font-mono font-semibold">{{ (value as number).toLocaleString() }}</span>
          </template>
        </DataTable>

        <div v-if="hasMoreRecipes" class="flex justify-center">
          <button
            class="text-text-muted text-xs cursor-pointer bg-transparent border-none hover:text-accent-gold underline"
            @click="showAll = !showAll">
            {{ showAll ? 'Show top 50' : `Show all ${totalFilteredCount} recipes` }}
          </button>
        </div>
      </div>

      <!-- Summary stats -->
      <div class="flex gap-6 text-xs text-text-dim border-t border-border-light pt-3">
        <div>
          Total unique recipes crafted:
          <span class="text-text-primary font-semibold">{{ totalCrafted }}</span>
        </div>
        <div>
          Total completions:
          <span class="text-text-primary font-semibold">{{ totalCompletions.toLocaleString() }}</span>
        </div>
        <div>
          Uncrafted recipes (first-time bonus opportunities):
          <span class="text-accent-gold font-semibold">{{ totalUncrafted }}</span>
        </div>
      </div>
    </template>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, onMounted } from "vue";
import { useCraftingStore } from "../../stores/craftingStore";
import type { CraftingHistoryRecipe, SkillCraftingStats } from "../../types/crafting";
import DataTable, { type ColumnDef } from "../Shared/DataTable.vue";
import EmptyState from "../Shared/EmptyState.vue";
import SkeletonLoader from "../Shared/SkeletonLoader.vue";
import DataTableSkeleton from "../Shared/DataTableSkeleton.vue";
import SkillCraftingProgress from "./SkillCraftingProgress.vue";
import RecipeInline from "../Shared/Recipe/RecipeInline.vue";
import SkillInline from "../Shared/Skill/SkillInline.vue";

const craftingStore = useCraftingStore();

const loading = ref(false);
const history = ref<CraftingHistoryRecipe[]>([]);
const skillStats = ref<SkillCraftingStats[]>([]);
const filterSkill = ref("");
const showAll = ref(false);

const hasData = computed(() => history.value.length > 0 || skillStats.value.length > 0);

const availableSkills = computed(() => {
  const skills = new Set<string>();
  for (const r of history.value) {
    if (r.reward_skill) skills.add(r.reward_skill);
  }
  return Array.from(skills).sort();
});

const allFilteredRecipes = computed(() => {
  if (!filterSkill.value) return history.value;
  return history.value.filter((r) => r.reward_skill === filterSkill.value);
});

const totalFilteredCount = computed(() => allFilteredRecipes.value.length);

const filteredRecipes = computed(() => {
  if (showAll.value) return allFilteredRecipes.value;
  return allFilteredRecipes.value.slice(0, 50);
});

const hasMoreRecipes = computed(() => allFilteredRecipes.value.length > 50);

const columns: ColumnDef[] = [
  { key: "idx", label: "#", align: "left", width: "2rem" },
  { key: "recipe_name", label: "Recipe", align: "left" },
  { key: "reward_skill", label: "Skill", align: "left", width: "7rem" },
  { key: "skill_level_req", label: "Level", align: "right", width: "4rem" },
  { key: "completions", label: "Crafts", align: "right", width: "5rem", numeric: true },
];

/** Rows carry a 1-based display index so the first column can render it. */
const recipeRows = computed(
  () =>
    filteredRecipes.value.map((r, i) => ({ ...r, idx: i + 1 })) as unknown as Record<string, unknown>[],
);

const emptyText = computed(() =>
  filterSkill.value ? `No recipes found for ${filterSkill.value}.` : "No recipes found.",
);

const totalCrafted = computed(() => history.value.filter((r) => r.completions > 0).length);

const totalCompletions = computed(() =>
  history.value.reduce((sum, r) => sum + r.completions, 0),
);

const totalUncrafted = computed(() =>
  skillStats.value.reduce((sum, s) => sum + s.uncrafted_count, 0),
);

onMounted(() => refresh());

async function refresh() {
  loading.value = true;
  try {
    const [h, s] = await Promise.all([
      craftingStore.getCraftingHistory(),
      craftingStore.getSkillCraftingStats(),
    ]);
    history.value = h;
    skillStats.value = s;
  } catch (e) {
    console.error("[crafting] Failed to load history:", e);
  } finally {
    loading.value = false;
  }
}
</script>
