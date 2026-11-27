<template>
  <div class="p-4 flex flex-col gap-4">
    <!-- Header -->
    <div class="flex items-center justify-between">
      <div>
        <h2 class="display-title">
          {{ effectInfo?.effectLabel ?? store.selectedEffect }}
        </h2>
        <div v-if="resolvedInfo" class="flex flex-col gap-0.5 mt-1">
          <span
            v-for="(desc, i) in resolvedInfo.tier_effects"
            :key="i"
            class="text-xs text-text-secondary">
            {{ desc }}
          </span>
        </div>
        <div v-if="resolvedInfo?.skill" class="text-xs text-text-dim mt-1">
          Requires {{ resolvedInfo.skill }}
        </div>
      </div>
      <div class="flex items-center gap-2">
        <span v-if="effectInfo?.raceRestriction"
          class="text-xs px-1.5 py-0.5 rounded bg-accent-red/10 text-accent-red border border-accent-red/20">
          {{ effectInfo.raceRestriction }} only
        </span>
        <button
          class="text-text-dim hover:text-text-secondary cursor-pointer bg-transparent border-none text-sm"
          @click="store.clearEffectSelection()"
          title="Close effect view">
          ✕
        </button>
      </div>
    </div>

    <!-- Results header -->
    <div class="text-[10px] uppercase tracking-widest text-text-dim border-b border-surface-card pb-0.5">
      {{ store.selectedEffectDiscoveries.length }} recipe{{ store.selectedEffectDiscoveries.length === 1 ? '' : 's' }} discovered
    </div>

    <!-- Discovery cards sorted by tier (strongest first) -->
    <div class="flex flex-col gap-2">
      <div
        v-for="disc in sortedDiscoveries"
        :key="disc.id"
        class="bg-surface-base border rounded px-3 py-2"
        :class="availabilityClass(disc)">

        <!-- Recipe name + tier -->
        <div class="flex items-center justify-between mb-1.5">
          <button
            class="text-xs text-text-primary hover:text-accent-gold cursor-pointer bg-transparent border-none p-0 text-left font-semibold"
            @click="store.selectRecipe(disc.recipe_id)">
            {{ getRecipeName(disc.recipe_id) }}
          </button>
          <div class="flex items-center gap-2">
            <span v-if="hasAllIngredients(disc)" class="text-xs text-accent-green font-semibold">
              ✓ Can brew
            </span>
            <span v-else-if="hasSomeIngredients(disc)" class="text-xs text-accent-gold">
              Partial
            </span>
            <span class="text-xs text-text-dim">
              T{{ disc.power_tier }}
            </span>
          </div>
        </div>

        <!-- Ingredients grid -->
        <div class="flex flex-wrap gap-x-3 gap-y-1">
          <div
            v-for="ingId in disc.ingredient_ids"
            :key="ingId"
            class="flex items-center gap-1 text-xs">
            <span
              class="w-2 h-2 rounded-full shrink-0"
              :class="hasItem(ingId) ? 'bg-accent-green' : 'bg-surface-elevated border border-border-light'" />
            <ItemInline :reference="String(ingId)" />
            <span v-if="getOwnedCount(ingId) > 0" class="text-xs text-accent-green">
              ×{{ getOwnedCount(ingId) }}
            </span>
          </div>
        </div>

        <!-- Fixed ingredients for the recipe -->
        <div v-if="getRecipeFixedIngredients(disc.recipe_id).length > 0"
          class="flex flex-wrap gap-x-3 gap-y-1 mt-1 pt-1 border-t border-surface-card">
          <div
            v-for="fixed in getRecipeFixedIngredients(disc.recipe_id)"
            :key="fixed.item_id"
            class="flex items-center gap-1 text-xs text-text-dim">
            <span
              class="w-2 h-2 rounded-full shrink-0"
              :class="hasItemById(fixed.item_id) ? 'bg-accent-green/50' : 'bg-surface-elevated border border-border-light'" />
            <span class="text-xs">{{ fixed.stack_size }}×</span>
            <ItemInline :reference="String(fixed.item_id)" />
          </div>
        </div>
      </div>
    </div>

    <div v-if="store.selectedEffectDiscoveries.length === 0"
      class="text-xs text-text-dim italic text-center py-4">
      No discoveries for this effect.
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed } from "vue";
import ItemInline from "../Shared/Item/ItemInline.vue";
import { useBreweryStore } from "../../stores/breweryStore";
import { useGameStateStore } from "../../stores/gameStateStore";
import type { BrewingDiscovery } from "../../types/gameData/brewing";

const store = useBreweryStore();
const gameState = useGameStateStore();

const ownedCounts = computed(() => gameState.ownedItemCounts);

/** The effect entry metadata for the selected effect */
const effectInfo = computed(() => {
  return store.effectEntries.find((e) => e.power === store.selectedEffect) ?? null;
});

/** Resolved TSys power info for the selected effect (use first discovery's tier) */
const resolvedInfo = computed(() => {
  const first = store.selectedEffectDiscoveries[0];
  if (!first) return null;
  return store.getPowerInfo(first.power, first.power_tier) ?? null;
});

/** Discoveries sorted by tier (highest first), then by availability */
const sortedDiscoveries = computed(() => {
  return [...store.selectedEffectDiscoveries].sort((a, b) => {
    // Can-brew first
    const aHasAll = hasAllIngredients(a);
    const bHasAll = hasAllIngredients(b);
    if (aHasAll && !bHasAll) return -1;
    if (!aHasAll && bHasAll) return 1;

    // Partial next
    const aHasSome = hasSomeIngredients(a);
    const bHasSome = hasSomeIngredients(b);
    if (aHasSome && !bHasSome) return -1;
    if (!aHasSome && bHasSome) return 1;

    // Then by tier (higher = stronger)
    return b.power_tier - a.power_tier;
  });
});

function getRecipeName(recipeId: number): string {
  return store.recipeById.get(recipeId)?.name ?? `Recipe #${recipeId}`;
}

function getRecipeFixedIngredients(recipeId: number) {
  return store.recipeById.get(recipeId)?.fixed_ingredients ?? [];
}

function getOwnedCount(itemTypeId: number): number {
  const ingredient = store.ingredientById.get(itemTypeId);
  if (!ingredient) return 0;
  return ownedCounts.value[ingredient.name] ?? 0;
}

function hasItem(itemTypeId: number): boolean {
  return getOwnedCount(itemTypeId) > 0;
}

function hasItemById(itemId: number): boolean {
  // For fixed ingredients — need to look up by item ID in the full items data
  // Use ingredientById first, fall back to checking all items
  const ing = store.ingredientById.get(itemId);
  if (ing) return (ownedCounts.value[ing.name] ?? 0) > 0;
  // Fixed ingredients might not be in the brewing ingredients list
  // We'd need the full item name — for now just show unknown
  return false;
}

function hasAllIngredients(disc: BrewingDiscovery): boolean {
  return disc.ingredient_ids.every((id) => hasItem(id));
}

function hasSomeIngredients(disc: BrewingDiscovery): boolean {
  return disc.ingredient_ids.some((id) => hasItem(id));
}

function availabilityClass(disc: BrewingDiscovery): string {
  if (hasAllIngredients(disc)) return "border-accent-green/40";
  if (hasSomeIngredients(disc)) return "border-accent-gold/25";
  return "border-surface-elevated";
}
</script>
