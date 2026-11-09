<template>
  <div class="flex gap-2 items-start mb-2">
    <img
      v-if="iconSrc"
      :src="iconSrc"
      :alt="recipe.name"
      class="w-8 h-8 object-contain bg-black/30 border border-border-light rounded shrink-0" />
    <div class="flex-1">
      <div class="font-bold text-entity-recipe text-sm mb-0.5">{{ recipe.name }}</div>
      <div class="flex items-center gap-2">
        <div v-if="recipe.skill" class="text-text-muted text-xs">
          {{ recipe.skill }}<span v-if="recipe.skill_level_req"> · Lv {{ recipe.skill_level_req }}</span>
        </div>
        <span v-if="isLearned === true" class="text-accent-green text-[10px]">✓ Learned</span>
        <span v-else-if="isLearned === false" class="text-text-dim text-[10px]">Not learned</span>
      </div>
    </div>
  </div>

  <div v-if="recipe.description" class="text-text-secondary text-xs leading-relaxed mb-2 italic">
    {{ recipe.description }}
  </div>

  <div v-if="recipe.ingredients.length" class="mb-2">
    <div class="text-text-muted text-[10px] leading-tight uppercase tracking-wide mb-1">Ingredients</div>
    <div
      v-for="(ing, i) in recipe.ingredients"
      :key="i"
      class="text-text-secondary text-xs leading-relaxed pl-2 relative before:content-['•'] before:absolute before:left-0"
    >
      {{ ingredientName(ing) }} ×{{ ing.stack_size }}
      <span v-if="ing.chance_to_consume !== null && ing.chance_to_consume < 1" class="text-text-muted">
        ({{ Math.round(ing.chance_to_consume * 100) }}% consumed)
      </span>
    </div>
    <div
      v-for="(cost, i) in recipe.costs ?? []"
      :key="`cost-${i}`"
      class="text-accent-gold text-xs leading-relaxed pl-2 relative before:content-['•'] before:absolute before:left-0"
    >
      {{ formatRecipeCost(cost) }}
    </div>
  </div>

  <!-- Currency costs when the recipe has no item ingredients at all -->
  <div v-else-if="recipe.costs?.length" class="mb-2">
    <div class="text-text-muted text-[10px] leading-tight uppercase tracking-wide mb-1">Cost</div>
    <div
      v-for="(cost, i) in recipe.costs"
      :key="i"
      class="text-accent-gold text-xs leading-relaxed pl-2 relative before:content-['•'] before:absolute before:left-0"
    >
      {{ formatRecipeCost(cost) }}
    </div>
  </div>

  <div v-if="recipe.result_items.length" class="mb-2">
    <div class="text-text-muted text-[10px] leading-tight uppercase tracking-wide mb-1">Results</div>
    <div
      v-for="(res, i) in recipe.result_items"
      :key="i"
      class="text-accent-green text-xs leading-relaxed pl-2 relative before:content-['•'] before:absolute before:left-0"
    >
      {{ resultName(res) }} ×{{ res.stack_size }}
      <span v-if="res.percent_chance !== null && res.percent_chance < 100" class="text-text-muted">
        ({{ res.percent_chance }}%)
      </span>
    </div>
  </div>

  <div v-if="recipe.reward_skill_xp" class="text-text-muted text-[10px] mt-2 pt-2 border-t border-border-subtle">
    {{ recipe.reward_skill ?? recipe.skill }} +{{ recipe.reward_skill_xp }} XP
    <span v-if="recipe.reward_skill_xp_first_time" class="text-text-dim">
      (first time: +{{ recipe.reward_skill_xp_first_time }})
    </span>
  </div>

  <div v-if="costBreakdown" class="text-text-muted text-[10px] mt-2 pt-2 border-t border-border-subtle flex items-center gap-2">
    <span>Cost:</span>
    <span v-if="costBreakdown.total_cost !== null" class="text-accent-gold">{{ formatGold(costBreakdown.total_cost) }}</span>
    <span v-else class="text-text-dim italic">unknown</span>
    <span v-if="costBreakdown.cost_per_unit !== null && (recipe.result_items[0]?.stack_size ?? 1) > 1" class="text-text-dim">
      ({{ formatGold(costBreakdown.cost_per_unit) }}/ea)
    </span>
    <span v-if="costBreakdown.has_unknown_prices" class="text-text-dim text-[10px]">*partial</span>
  </div>

  <div v-if="recipe.keywords?.length" class="flex flex-wrap gap-1 mt-2">
    <span
      v-for="keyword in recipe.keywords"
      :key="keyword"
      class="bg-entity-recipe/10 text-entity-recipe px-1.5 py-0.5 rounded-sm text-[10px] leading-tight uppercase tracking-wide"
    >
      {{ keyword }}
    </span>
  </div>
</template>

<script setup lang="ts">
import { ref, onMounted } from "vue";
import { useGameDataStore } from "../../../stores/gameDataStore";
import type { RecipeInfo, RecipeIngredient, RecipeResultItem } from "../../../types/gameData/recipes";
import { useRecipeCost, formatGold } from "../../../composables/useRecipeCost";
import { formatRecipeCost } from "../../../utils/recipeCurrency";

const props = defineProps<{
  recipe: RecipeInfo;
  iconSrc: string | null;
  isLearned?: boolean | null;
}>();

const store = useGameDataStore();
const itemNames = ref<Record<string, string>>({});
const { calculate, breakdown: costBreakdown } = useRecipeCost();

onMounted(async () => {
  calculate(props.recipe);
  const ids = [
    ...props.recipe.ingredient_item_ids,
    ...props.recipe.result_item_ids,
  ].filter((id) => id > 0);
  if (ids.length === 0) return;

  const items = await store.resolveItemsBatch(ids.map(String));
  const names: Record<string, string> = {};
  for (const [id, item] of Object.entries(items)) {
    if (item) names[id] = item.name;
  }
  itemNames.value = names;
});

function ingredientName(ing: RecipeIngredient): string {
  if (ing.description) return ing.description;
  if (ing.item_id && itemNames.value[ing.item_id]) return itemNames.value[ing.item_id];
  if (ing.item_keys.length > 0) return ing.item_keys.join(" / ");
  return "Unknown";
}

function resultName(res: RecipeResultItem): string {
  if (res.item_id && itemNames.value[res.item_id]) return itemNames.value[res.item_id];
  return `Item #${res.item_id}`;
}
</script>
