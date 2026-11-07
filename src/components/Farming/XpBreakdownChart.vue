<template>
  <div class="flex flex-col gap-2 w-72 max-w-[20rem]">
    <div class="flex items-center justify-between border-b border-border-default pb-1.5">
      <span class="text-[0.65rem] uppercase tracking-widest text-text-dim font-bold">XP by Skill</span>
      <span class="text-[0.6rem] text-text-muted uppercase tracking-wide shrink-0">
        Total <span class="text-value-positive font-bold">{{ totalXp.toLocaleString() }}</span>
      </span>
    </div>

    <div v-if="skills.length === 0" class="text-xs text-text-dim italic py-1">
      No skill XP gained.
    </div>

    <div v-else class="flex flex-col gap-1.5">
      <div v-for="skill in sortedSkills" :key="skill.name" class="flex flex-col gap-0.5">
        <div class="flex items-center justify-between text-xs">
          <SkillInline :reference="skill.name" :show-icon="true" />
          <span class="shrink-0">
            <span class="text-value-positive font-mono font-bold">+{{ skill.gained.toLocaleString() }}</span>
            <span v-if="skill.levelsGained" class="text-[10px] leading-tight text-value-neutral-warm font-bold ml-1">+{{ skill.levelsGained }}lvl</span>
          </span>
        </div>
        <div class="relative h-2.5 rounded bg-black/40 border border-border-default overflow-hidden">
          <div
            class="absolute inset-y-0 left-0 bg-[#3a5a3a] transition-[width] duration-300"
            :style="{ width: barWidth(skill.gained) + '%' }" />
        </div>
        <div v-if="skill.perHour" class="text-[10px] leading-tight text-text-dim self-end">{{ skill.perHour.toLocaleString() }}/hr</div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed } from "vue";
import SkillInline from "../Shared/Skill/SkillInline.vue";

export interface XpBreakdownSkill {
  name: string;
  gained: number;
  perHour?: number;
  levelsGained?: number;
}

const props = defineProps<{
  skills: XpBreakdownSkill[];
  totalXp: number;
}>();

const sortedSkills = computed(() => [...props.skills].sort((a, b) => b.gained - a.gained));
const maxGained = computed(() => Math.max(1, ...props.skills.map((s) => s.gained)));

function barWidth(gained: number): number {
  return Math.min(100, Math.max(2, (gained / maxGained.value) * 100));
}
</script>
