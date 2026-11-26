<template>
  <div class="flex flex-col gap-3">
    <h4 class="panel-label m-0">
      Crafting Progress by Skill
    </h4>

    <div v-if="stats.length === 0" class="text-text-dim text-xs italic">
      No crafting data available. Import a character report first.
    </div>

    <div v-else class="grid grid-cols-1 gap-2">
      <div
        v-for="skill in stats"
        :key="skill.skill_name"
        class="bg-surface-base border border-surface-elevated rounded px-3 py-2 flex items-center gap-3 text-xs">
        <SkillInline :reference="skill.skill_name" :show-icon="true" class="w-32 shrink-0" />

        <!-- Progress bar -->
        <div class="flex-1 flex items-center gap-2">
          <div class="flex-1 bg-surface-dark rounded-full h-2 overflow-hidden">
            <div
              class="h-full rounded-full transition-all"
              :class="barColor(skill.completion_percent)"
              :style="{ width: `${skill.completion_percent}%` }" />
          </div>
          <span class="text-text-muted w-10 text-right shrink-0">{{ skill.completion_percent }}%</span>
        </div>

        <!-- Stats -->
        <div class="flex gap-4 shrink-0 text-text-dim">
          <span>
            <span class="text-text-primary">{{ skill.crafted_recipes }}</span>
            / {{ skill.total_recipes }} recipes
          </span>
          <span>
            <span class="text-text-primary">{{ skill.total_completions.toLocaleString() }}</span>
            crafts
          </span>
          <span v-if="skill.uncrafted_count > 0" class="text-accent-gold">
            {{ skill.uncrafted_count }} uncrafted
          </span>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import type { SkillCraftingStats } from "../../types/crafting";
import SkillInline from "../Shared/Skill/SkillInline.vue";

defineProps<{
  stats: SkillCraftingStats[]
}>();

function barColor(percent: number): string {
  if (percent >= 90) return "bg-green-500";
  if (percent >= 50) return "bg-accent-gold";
  if (percent >= 25) return "bg-yellow-600";
  return "bg-text-muted";
}
</script>
