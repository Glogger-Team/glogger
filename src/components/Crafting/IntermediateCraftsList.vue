<template>
  <div v-if="intermediates.length > 0">
    <h4 v-if="!bare" class="panel-label mb-2">
      Intermediate Crafts
    </h4>
    <div class="flex flex-col gap-1">
      <div
        v-for="craft in intermediates"
        :key="craft.recipe_id"
        class="flex items-center gap-2 px-2 py-1.5 bg-surface-dark/30 border border-surface-elevated/50 rounded text-xs">
        <RecipeInline :reference="craft.recipe_name" />
        <span class="text-text-muted">→</span>
        <ItemInline :reference="craft.item_name" />
        <span class="ml-auto text-text-primary shrink-0">
          ×{{ craft.crafts_needed }} crafts
        </span>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import type { IntermediateCraft } from "../../types/crafting";
import RecipeInline from "../Shared/Recipe/RecipeInline.vue";
import ItemInline from "../Shared/Item/ItemInline.vue";

withDefaults(defineProps<{
  intermediates: IntermediateCraft[]
  bare?: boolean
}>(), {
  bare: false,
});
</script>
