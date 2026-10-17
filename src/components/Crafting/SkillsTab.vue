<template>
  <PaneLayout
    screen-key="crafting-skills"
    :left-pane="{ title: 'Skills', defaultWidth: 220, minWidth: 160, maxWidth: 360 }">
    <template #left>
      <div class="flex flex-col gap-1 px-2 pb-2">
        <div class="flex items-center justify-end px-1 pb-1">
          <select v-model="sortMode" class="input text-xs w-24">
            <option value="alpha">A–Z</option>
            <option value="level">Level</option>
            <option value="crafts">Crafts</option>
          </select>
        </div>

        <SkeletonLoader v-if="loading" variant="text" :lines="6" />

        <button
          v-for="entry in sortedSkills"
          :key="entry.skill_name"
          class="flex items-center justify-between px-2 py-1.5 rounded text-xs text-left cursor-pointer border-none w-full"
          :class="selectedSkill === entry.skill_name
            ? 'bg-accent-gold/15 text-accent-gold border border-accent-gold/30'
            : 'bg-transparent text-text-secondary hover:bg-surface-base border border-transparent'"
          @click="selectSkill(entry.skill_name)">
          <span class="truncate">{{ entry.skill_name }}</span>
          <span class="text-text-muted font-mono shrink-0 ml-2">{{ entry.level ?? '—' }}</span>
        </button>

        <div v-if="!loading && sortedSkills.length === 0" class="text-text-dim text-xs italic px-1">
          No crafting skills found.
        </div>
      </div>
    </template>

    <!-- Center content -->
    <div class="p-4 flex flex-col gap-4 h-full min-h-0 overflow-hidden">
      <EmptyState
        v-if="!loading && !selectedSkill"
        variant="panel"
        primary="Select a skill"
        secondary="Choose a crafting skill from the list to view details." />

      <EmptyState
        v-else-if="!loading && sortedSkills.length === 0"
        variant="panel"
        primary="No crafting data available"
        secondary="Import a character report to see crafting skill details." />

      <template v-else-if="selectedSkill && detail">
        <!-- Hero Header -->
        <div class="bg-surface-base border border-surface-elevated rounded-lg px-5 py-3">
          <div class="flex items-center gap-6">
            <!-- Left: crafting stats -->
            <div class="shrink-0 text-sm leading-snug">
              <div class="text-text-primary">
                <span class="font-bold font-mono">{{ formatCompact(stats?.total_completions ?? 0) }}</span>
                <span class="text-text-muted"> Total Crafts</span>
                <span class="text-text-dim"> ({{ stats?.crafted_recipes ?? 0 }} Unique)</span>
              </div>
              <div class="text-text-primary">
                <span class="font-bold font-mono">{{ stats?.crafted_recipes ?? 0 }}</span>
                <span class="text-text-muted"> Recipes Known of </span>
                <span class="font-bold font-mono">{{ stats?.total_recipes ?? 0 }}</span>
                <span class="text-text-muted"> Possible</span>
                <span class="text-text-dim"> ({{ stats?.completion_percent ?? 0 }}%)</span>
              </div>
              <div v-if="availableRecipeCount > 0" class="text-text-muted text-xs mt-0.5">
                ({{ availableRecipeCount }} Recipes Beyond Current Skill)
              </div>
            </div>

            <!-- Center: level + name + XP bar -->
            <div class="flex-1 flex flex-col items-center text-center">
              <div class="flex items-baseline gap-3 mb-1">
                <span class="text-text-primary text-2xl font-bold uppercase tracking-[0.25em] leading-none">{{ detail.totalLevel }}</span>
                <span class="text-text-primary text-2xl font-bold uppercase tracking-[0.25em] leading-none">
                  <SkillInline :reference="selectedSkill" :show-icon="true" />
                </span>
              </div>
              <div v-if="detail.bonusLevels > 0" class="text-text-muted text-xs mb-1">
                {{ detail.baseLevel }} Base + <span class="text-accent-gold">{{ detail.bonusLevels }}</span> Bonus
              </div>
              <div v-if="detail.xpNeededForNext > 0" class="w-64">
                <div class="h-2 bg-surface-dark rounded-full overflow-hidden">
                  <div
                    class="h-full bg-green-500 rounded-full transition-all duration-300"
                    :style="{ width: xpPercent + '%' }" />
                </div>
                <div class="text-xs text-text-muted mt-0.5">
                  {{ detail.xpTowardNext.toLocaleString() }} of {{ detail.xpNeededForNext.toLocaleString() }} XP
                  <span class="text-text-dim">({{ xpPercent }}%)</span>
                </div>
              </div>
              <div v-else class="text-xs text-accent-gold font-semibold">MAX LEVEL</div>
            </div>

            <!-- Right: lifetime value stats -->
            <div v-if="materialSummary" class="shrink-0 text-right text-sm leading-snug">
              <div class="text-text-muted">
                <span class="text-text-dim">estimated material cost </span>
                <span class="text-text-primary font-bold font-mono">{{ formatCompact(materialSummary.totalInputCost) }}</span>
              </div>
              <div class="text-text-muted">
                <span class="text-text-dim">estimated craft value </span>
                <span class="text-text-primary font-bold font-mono">{{ formatCompact(materialSummary.totalOutputValue) }}</span>
              </div>
              <div class="text-text-muted">
                <span class="text-text-dim">estimated total profit </span>
                <span class="font-bold font-mono" :class="profitLoss >= 0 ? 'text-green-400' : 'text-red-400'">
                  {{ profitLoss >= 0 ? '+' : '' }}{{ formatCompact(Math.abs(profitLoss)) }}
                </span>
              </div>
            </div>
            <div v-else-if="materialsLoading" class="shrink-0 text-text-dim text-xs italic">Calculating...</div>
          </div>
        </div>

        <!-- Charts (left) + Recipe list (right) — side by side -->
        <div class="flex gap-4 flex-1 min-h-0">
          <!-- Charts column — two charts side by side, each with own scrollable list -->
          <div v-if="materialSummary && (materialSummary.topInputs.length > 0 || materialSummary.topOutputs.length > 0)"
               class="flex gap-3 min-h-0" style="width: 65%; min-width: 400px;">
            <div v-if="materialSummary.topInputs.length > 0" class="flex-1 bg-surface-base border border-surface-elevated rounded p-3 flex flex-col min-w-0 min-h-0">
              <div class="text-text-dim text-[0.65rem] uppercase tracking-widest mb-2 shrink-0">Top Materials Used</div>
              <div v-if="inputChartDataset.length > 0" class="h-56 shrink-0">
                <VueUiDonut :dataset="inputChartDataset" :config="donutConfig" />
              </div>
              <div class="flex flex-col gap-1 text-xs mt-2 overflow-y-auto flex-1 min-h-0 pr-1.5">
                <div v-for="mat in allInputItems" :key="mat.itemId" class="flex items-center justify-between text-text-secondary shrink-0">
                  <span v-if="mat.isDynamic" class="text-text-muted italic">{{ mat.name }}</span>
                  <ItemInline v-else :reference="String(mat.itemId)" />
                  <span class="text-text-muted font-mono ml-2 shrink-0">&times;{{ mat.quantity.toLocaleString() }}</span>
                </div>
              </div>
            </div>

            <div v-if="materialSummary.topOutputs.length > 0" class="flex-1 bg-surface-base border border-surface-elevated rounded p-3 flex flex-col min-w-0 min-h-0">
              <div class="text-text-dim text-[0.65rem] uppercase tracking-widest mb-2 shrink-0">Top Items Crafted</div>
              <div v-if="outputChartDataset.length > 0" class="h-56 shrink-0">
                <VueUiDonut :dataset="outputChartDataset" :config="donutConfig" />
              </div>
              <div class="flex flex-col gap-1 text-xs mt-2 overflow-y-auto flex-1 min-h-0 pr-1.5">
                <div v-for="out in allOutputItems" :key="out.itemId" class="flex items-center justify-between text-text-secondary shrink-0">
                  <ItemInline :reference="String(out.itemId)" />
                  <span class="text-text-muted font-mono ml-2 shrink-0">&times;{{ out.quantity.toLocaleString() }}</span>
                </div>
              </div>
            </div>
          </div>

          <!-- Recipe list — independently scrollable -->
          <div class="flex-1 min-w-0 min-h-0 flex flex-col gap-2">
            <div class="flex items-center justify-between">
              <div class="text-[0.65rem] uppercase tracking-widest text-text-dim">
                Recipes ({{ filteredRecipes.length }})
              </div>
              <div class="flex items-center gap-2">
                <label class="flex items-center gap-1 text-xs text-text-muted cursor-pointer">
                  <input type="checkbox" v-model="hideUnlearned" class="accent-accent-gold" />
                  Hide unlearned
                </label>
                <label class="flex items-center gap-1 text-xs text-text-muted cursor-pointer">
                  <input type="checkbox" v-model="hideHighLevel" class="accent-accent-gold" />
                  Hide too high level
                </label>
                <select v-model="recipeSortMode" class="input text-xs w-24">
                  <option value="name">Name</option>
                  <option value="level">Level</option>
                  <option value="crafts">Crafts</option>
                </select>
              </div>
            </div>

            <div class="overflow-y-auto flex-1 min-h-0 pr-1.5">
              <DataTable
                :columns="recipeColumns"
                :rows="(filteredRecipes as unknown as Record<string, unknown>[])"
                compact
                empty-text="No recipes match the current filters."
                :row-class="recipeRowClass">
                <template #cell-name="{ value }">
                  <RecipeInline :reference="(value as string)" />
                </template>
                <template #cell-skill_level_req="{ value }">
                  <span class="text-text-muted font-mono">{{ value ?? '—' }}</span>
                </template>
                <template #cell-completions="{ value }">
                  <span
                    class="font-mono"
                    :class="(value as number) > 0 ? 'text-text-primary font-semibold' : 'text-text-muted'">
                    {{ (value as number) > 0 ? (value as number).toLocaleString() : '—' }}
                  </span>
                </template>
                <template #cell-reward_skill_xp="{ value }">
                  <span class="text-text-muted font-mono">{{ value ?? '—' }}</span>
                </template>
              </DataTable>
            </div>
          </div>
        </div>
      </template>
    </div>
  </PaneLayout>
</template>

<script setup lang="ts">
import { ref, computed, onMounted } from "vue";
import { useCraftingStore } from "../../stores/craftingStore";
import { useGameDataStore } from "../../stores/gameDataStore";
import { useGameStateStore } from "../../stores/gameStateStore";
import type { SkillCraftingStats } from "../../types/crafting";
import type { RecipeInfo } from "../../types/gameData/recipes";
import EmptyState from "../Shared/EmptyState.vue";
import PaneLayout from "../Shared/PaneLayout.vue";
import SkeletonLoader from "../Shared/SkeletonLoader.vue";
import SkillInline from "../Shared/Skill/SkillInline.vue";
import DataTable, { type ColumnDef } from "../Shared/DataTable.vue";
import RecipeInline from "../Shared/Recipe/RecipeInline.vue";
import ItemInline from "../Shared/Item/ItemInline.vue";
import { useMarketStore } from "../../stores/marketStore";
import { VueUiDonut } from "vue-data-ui";
import type { VueUiDonutConfig, VueUiDonutDatasetItem } from "vue-data-ui";

const craftingStore = useCraftingStore();
const gameData = useGameDataStore();
const gameState = useGameStateStore();
const marketStore = useMarketStore();

const loading = ref(false);
const sortMode = ref<"alpha" | "level" | "crafts">("alpha");
const selectedSkill = ref<string | null>(null);
const recipeSortMode = ref<"name" | "level" | "crafts">("crafts");
const hideUnlearned = ref(false);
const hideHighLevel = ref(false);

// All skill stats (shared with left pane)
const allStats = ref<SkillCraftingStats[]>([]);

// Per-skill detail
const detail = ref<{
  baseLevel: number;
  bonusLevels: number;
  totalLevel: number;
  xpTowardNext: number;
  xpNeededForNext: number;
} | null>(null);

// Recipes for the selected skill
const skillRecipes = ref<(RecipeInfo & { completions: number })[]>([]);

// Material summary
const materialsLoading = ref(false);

interface ItemAggregate {
  itemId: number;
  name: string;
  quantity: number;
  value: number;
  isDynamic?: boolean;
}

interface MaterialSummary {
  totalInputCost: number;
  totalInputItems: number;
  uniqueInputItems: number;
  totalOutputValue: number;
  totalOutputItems: number;
  topInputs: ItemAggregate[];
  topOutputs: ItemAggregate[];
  allInputs: ItemAggregate[];
  allOutputs: ItemAggregate[];
}

const materialSummary = ref<MaterialSummary | null>(null);

interface SkillListEntry {
  skill_name: string;
  level: number | null;
  crafted_recipes: number;
  total_completions: number;
}

const skillList = ref<SkillListEntry[]>([]);

const sortedSkills = computed(() => {
  const list = [...skillList.value];
  switch (sortMode.value) {
    case "alpha":
      return list.sort((a, b) => a.skill_name.localeCompare(b.skill_name));
    case "level":
      return list.sort((a, b) => (b.level ?? 0) - (a.level ?? 0));
    case "crafts":
      return list.sort((a, b) => b.total_completions - a.total_completions);
  }
  return list;
});

const stats = computed(() =>
  allStats.value.find((s) => s.skill_name === selectedSkill.value) ?? null
);

const xpPercent = computed(() => {
  if (!detail.value || detail.value.xpNeededForNext <= 0) return 100;
  return Math.round((detail.value.xpTowardNext / detail.value.xpNeededForNext) * 100);
});

/**
 * Recipes the player hasn't crafted yet but meets the requirements for:
 * - Skill level meets or exceeds skill_level_req
 * - Prerequisite recipe (if any) has been crafted at least once
 */
const availableRecipeCount = computed(() => {
  if (!detail.value) return 0;
  const completionMap = gameState.recipeCompletions;
  const playerLevel = detail.value.totalLevel;

  return skillRecipes.value.filter((r) => {
    if (r.completions > 0) return false;
    if (r.skill_level_req !== null && r.skill_level_req > playerLevel) return false;
    if (r.prereq_recipe) {
      const prereqRecipe = skillRecipes.value.find(
        (pr) => pr.internal_name === r.prereq_recipe || pr.name === r.prereq_recipe
      );
      if (prereqRecipe && completionMap[`Recipe_${prereqRecipe.id}`] === undefined) return false;
      if (prereqRecipe && (completionMap[`Recipe_${prereqRecipe.id}`] ?? 0) === 0) return false;
    }
    return true;
  }).length;
});

const profitLoss = computed(() => {
  if (!materialSummary.value) return 0;
  return materialSummary.value.totalOutputValue - materialSummary.value.totalInputCost;
});

function formatCompact(n: number): string {
  if (n >= 1_000_000) return (n / 1_000_000).toFixed(1) + "M";
  if (n >= 10_000) return (n / 1_000).toFixed(1) + "K";
  return n.toLocaleString();
}

// Chart palette — muted tones that work on dark backgrounds
const chartPalette = [
  "#6366f1", "#f59e0b", "#10b981", "#ef4444", "#8b5cf6",
  "#ec4899", "#14b8a6", "#f97316",
];

const donutConfig = computed<VueUiDonutConfig>(() => ({
  responsive: true,
  useCssAnimation: true,
  useBlurOnHover: false,
  style: {
    fontFamily: "inherit",
    chart: {
      backgroundColor: "transparent",
      color: "#a1a1aa",
      layout: {
        labels: {
          dataLabels: {
            show: true,
            hideUnderValue: 3,
          },
          percentage: {
            show: true,
            color: "#a1a1aa",
            bold: true,
            fontSize: 10,
            rounding: 1,
          },
          name: {
            show: true,
            color: "#d4d4d8",
            bold: false,
            fontSize: 10,
          },
          value: {
            show: false,
          },
          hollow: {
            show: true,
            total: {
              show: true,
              bold: true,
              fontSize: 14,
              color: "#d4d4d8",
              text: "Total",
              value: {
                color: "#e4e4e7",
                fontSize: 16,
                bold: true,
                rounding: 0,
              },
            },
            average: { show: false },
          },
        },
        donut: {
          strokeWidth: 1,
          borderWidth: 1,
          useShadow: false,
        },
      },
      legend: {
        show: false,
      },
      title: {
        text: "",
      },
      tooltip: {
        show: true,
        showValue: true,
        showPercentage: true,
        roundingValue: 0,
        roundingPercentage: 1,
        backgroundColor: "#27272a",
        color: "#d4d4d8",
        borderColor: "#3f3f46",
        borderWidth: 1,
        borderRadius: 4,
        fontSize: 12,
      },
    },
  },
  userOptions: { show: false },
  table: { show: false },
}));

function bucketSmallItems(items: ItemAggregate[]): VueUiDonutDatasetItem[] {
  if (items.length === 0) return [];
  const total = items.reduce((sum, i) => sum + i.quantity, 0);
  const threshold = total * 0.02;
  const result: VueUiDonutDatasetItem[] = [];
  let otherQty = 0;
  let colorIdx = 0;

  for (const item of items) {
    if (item.quantity >= threshold) {
      result.push({
        name: item.name,
        color: chartPalette[colorIdx % chartPalette.length],
        values: [item.quantity],
      });
      colorIdx++;
    } else {
      otherQty += item.quantity;
    }
  }

  if (otherQty > 0) {
    result.push({
      name: "Other",
      color: "#52525b",
      values: [otherQty],
    });
  }

  return result;
}

const inputChartDataset = computed<VueUiDonutDatasetItem[]>(() => {
  if (!materialSummary.value) return [];
  return bucketSmallItems(materialSummary.value.topInputs);
});

const outputChartDataset = computed<VueUiDonutDatasetItem[]>(() => {
  if (!materialSummary.value) return [];
  return bucketSmallItems(materialSummary.value.topOutputs);
});

const allInputItems = computed(() => materialSummary.value?.allInputs ?? []);
const allOutputItems = computed(() => materialSummary.value?.allOutputs ?? []);

const filteredRecipes = computed(() => {
  let recipes = [...skillRecipes.value];

  if (hideUnlearned.value) {
    recipes = recipes.filter((r) => r.completions > 0);
  }
  if (hideHighLevel.value && detail.value) {
    recipes = recipes.filter(
      (r) => r.skill_level_req === null || r.skill_level_req <= detail.value!.totalLevel
    );
  }

  switch (recipeSortMode.value) {
    case "name":
      recipes.sort((a, b) => a.name.localeCompare(b.name));
      break;
    case "level":
      recipes.sort((a, b) => (a.skill_level_req ?? 0) - (b.skill_level_req ?? 0));
      break;
    case "crafts":
      recipes.sort((a, b) => b.completions - a.completions);
      break;
  }

  return recipes;
});

const recipeColumns: ColumnDef[] = [
  { key: "name", label: "Recipe", align: "left" },
  { key: "skill_level_req", label: "Level", align: "right", width: "4rem" },
  { key: "completions", label: "Crafts", align: "right", width: "5rem", numeric: true },
  { key: "reward_skill_xp", label: "XP", align: "right", width: "4rem" },
];

function recipeRowClass(row: Record<string, unknown>): string {
  return (row.completions as number) === 0 ? "opacity-40" : "";
}

onMounted(() => loadSkills());

async function loadSkills() {
  loading.value = true;
  try {
    allStats.value = await craftingStore.getSkillCraftingStats();

    const entries: SkillListEntry[] = [];
    for (const stat of allStats.value) {
      const levelInfo = await craftingStore.getSkillLevel(stat.skill_name);
      entries.push({
        skill_name: stat.skill_name,
        level: levelInfo?.totalLevel ?? null,
        crafted_recipes: stat.crafted_recipes,
        total_completions: stat.total_completions,
      });
    }
    skillList.value = entries;

    if (entries.length > 0 && !selectedSkill.value) {
      selectSkill(entries[0].skill_name);
    }
  } catch (e) {
    console.error("[crafting-skills] Failed to load skills:", e);
  } finally {
    loading.value = false;
  }
}

function getItemPrice(itemId: number, vendorValue: number | null | undefined): number | null {
  const market = marketStore.valuesByItemId[itemId];
  if (market) return market.market_value;
  if (vendorValue) return vendorValue * 2;
  return null;
}

async function selectSkill(skillName: string) {
  selectedSkill.value = skillName;
  materialSummary.value = null;

  detail.value = await craftingStore.getSkillLevel(skillName);

  const recipes = await gameData.getRecipesForSkill(skillName);
  const completionMap = gameState.recipeCompletions;

  const enriched = recipes
    .filter((r) => r.reward_skill === skillName)
    .map((r) => ({
      ...r,
      completions: completionMap[`Recipe_${r.id}`] ?? 0,
    }));
  skillRecipes.value = enriched;

  computeMaterialSummary(enriched);
}

async function computeMaterialSummary(recipes: (RecipeInfo & { completions: number })[]) {
  const craftedRecipes = recipes.filter((r) => r.completions > 0);
  if (craftedRecipes.length === 0) return;

  materialsLoading.value = true;
  try {
    const allItemIds = new Set<number>();
    for (const r of craftedRecipes) {
      for (const ing of r.ingredients) {
        if (ing.item_id) allItemIds.add(ing.item_id);
      }
      for (const out of r.result_items) {
        allItemIds.add(out.item_id);
      }
      // Enchanted recipes have empty result_items but populated result_item_ids (from ProtoResultItems)
      if (r.result_items.length === 0) {
        for (const id of r.result_item_ids) {
          allItemIds.add(id);
        }
      }
    }

    const items = await gameData.resolveItemsBatch([...allItemIds].map(String));

    const inputMap = new Map<number, { quantity: number; cost: number; isDynamic?: boolean; dynamicName?: string }>();
    const outputMap = new Map<number, { quantity: number; value: number }>();
    let dynamicKeyCounter = -1;
    const dynamicKeyMap = new Map<string, number>();

    for (const r of craftedRecipes) {
      const crafts = r.completions;

      for (const ing of r.ingredients) {
        const chanceToConsume = ing.chance_to_consume ?? 1;
        const qty = Math.round(ing.stack_size * chanceToConsume * crafts);

        if (ing.item_id) {
          const item = items[String(ing.item_id)];
          const price = getItemPrice(ing.item_id, item?.value);

          const existing = inputMap.get(ing.item_id);
          if (existing) {
            existing.quantity += qty;
            existing.cost += (price ?? 0) * qty;
          } else {
            inputMap.set(ing.item_id, { quantity: qty, cost: (price ?? 0) * qty });
          }
        } else if (ing.item_keys.length > 0 || ing.description) {
          const dynKey = ing.item_keys.join(",") || ing.description || "unknown";
          let mapKey = dynamicKeyMap.get(dynKey);
          if (mapKey === undefined) {
            mapKey = dynamicKeyCounter--;
            dynamicKeyMap.set(dynKey, mapKey);
          }
          const dynName = ing.description || ing.item_keys.join(", ");

          const existing = inputMap.get(mapKey);
          if (existing) {
            existing.quantity += qty;
          } else {
            inputMap.set(mapKey, { quantity: qty, cost: 0, isDynamic: true, dynamicName: dynName });
          }
        }
      }

      // Use result_items when available; fall back to result_item_ids for enchanted recipes
      // (which have empty result_items but populated ProtoResultItems via result_item_ids)
      const outputs: { item_id: number; stack_size: number; percent_chance: number | null }[] =
        r.result_items.length > 0
          ? r.result_items
          : r.result_item_ids.map((id) => ({ item_id: id, stack_size: 1, percent_chance: null }));

      for (const out of outputs) {
        const chance = (out.percent_chance ?? 100) / 100;
        const qty = Math.round(out.stack_size * chance * crafts);
        const item = items[String(out.item_id)];
        const price = getItemPrice(out.item_id, item?.value);

        const existing = outputMap.get(out.item_id);
        if (existing) {
          existing.quantity += qty;
          existing.value += (price ?? 0) * qty;
        } else {
          outputMap.set(out.item_id, { quantity: qty, value: (price ?? 0) * qty });
        }
      }
    }

    const allInputsSorted = [...inputMap.entries()]
      .map(([itemId, d]) => ({
        itemId,
        name: d.isDynamic ? (d.dynamicName ?? "Unknown") : (items[String(itemId)]?.name ?? `Item #${itemId}`),
        quantity: d.quantity,
        value: d.cost,
        isDynamic: d.isDynamic,
      }))
      .sort((a, b) => b.quantity - a.quantity);

    const topInputs = allInputsSorted.slice(0, 8);

    const allOutputsSorted = [...outputMap.entries()]
      .map(([itemId, d]) => ({ itemId, name: items[String(itemId)]?.name ?? `Item #${itemId}`, quantity: d.quantity, value: d.value }))
      .sort((a, b) => b.quantity - a.quantity);

    const topOutputs = allOutputsSorted.slice(0, 8);

    let totalInputCost = 0;
    let totalInputItems = 0;
    for (const d of inputMap.values()) {
      totalInputCost += d.cost;
      totalInputItems += d.quantity;
    }

    let totalOutputValue = 0;
    let totalOutputItems = 0;
    for (const d of outputMap.values()) {
      totalOutputValue += d.value;
      totalOutputItems += d.quantity;
    }

    materialSummary.value = {
      totalInputCost: Math.round(totalInputCost),
      totalInputItems,
      uniqueInputItems: inputMap.size,
      totalOutputValue: Math.round(totalOutputValue),
      totalOutputItems,
      topInputs,
      topOutputs,
      allInputs: allInputsSorted,
      allOutputs: allOutputsSorted,
    };
  } catch (e) {
    console.error("[crafting-skills] Failed to compute material summary:", e);
  } finally {
    materialsLoading.value = false;
  }
}
</script>
