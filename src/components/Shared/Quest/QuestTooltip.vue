<template>
  <div class="mb-2">
    <div class="font-bold text-entity-quest text-sm mb-0.5">{{ quest.raw.Name ?? quest.internal_name }}</div>
    <div class="flex gap-2 text-xs">
      <span v-if="quest.raw.Level" class="text-text-secondary">Lv {{ quest.raw.Level }}</span>
      <span v-if="quest.raw.DisplayedLocation" class="text-entity-area">{{ quest.raw.DisplayedLocation }}</span>
    </div>
  </div>

  <div v-if="quest.raw.Description" class="text-text-secondary text-xs leading-relaxed mb-2 italic">
    {{ quest.raw.Description }}
  </div>

  <div class="flex flex-col gap-1 mb-2 text-xs">
    <div v-if="quest.raw.Objectives?.length" class="text-text-muted">
      {{ quest.raw.Objectives.length }} objective{{ quest.raw.Objectives.length > 1 ? 's' : '' }}
    </div>
    <div v-if="woState" class="text-text-secondary">
      <template v-if="woState.completedBy.length">Done: {{ woState.completedBy.join(', ') }}</template><template v-if="woState.completedBy.length && woState.activeBy.length"> • </template><template v-if="woState.activeBy.length">Active: {{ woState.activeBy.join(', ') }}</template>
    </div>
    <div v-if="rewardSummary" class="text-accent-green">{{ rewardSummary }}</div>
    <div v-if="quest.raw.FavorNpc" class="text-entity-npc text-xs">
      Favor: {{ quest.raw.FavorNpc }}
      <span v-if="quest.raw.Reward_Favor" class="text-text-muted">(+{{ quest.raw.Reward_Favor }})</span>
    </div>
  </div>

  <div v-if="quest.raw.Keywords?.length" class="flex flex-wrap gap-1">
    <span
      v-for="keyword in quest.raw.Keywords"
      :key="keyword"
      class="bg-entity-quest/10 text-entity-quest px-1.5 py-0.5 rounded-sm text-[0.65rem] uppercase tracking-wide"
    >
      {{ keyword }}
    </span>
  </div>

  <div v-if="quest.raw.IsCancellable === false" class="text-accent-red text-[0.65rem] mt-2 pt-1 border-t border-border-subtle">
    Not cancellable
  </div>
</template>

<script setup lang="ts">
import { computed, onMounted } from "vue";
import type { QuestInfo } from "../../../types/gameData";
import { useCharacterStore } from "../../../stores/characterStore";

const props = defineProps<{
  quest: QuestInfo;
}>();

const characterStore = useCharacterStore();
const woState = computed(() => characterStore.workOrderStateFor(props.quest.internal_name));

onMounted(() => {
  void characterStore.loadAllCharacterWorkOrders();
});


const rewardSummary = computed(() => {
  const parts: string[] = [];
  const rewards = props.quest.raw.Rewards;
  if (rewards?.length) {
    const totalXp = rewards.reduce((sum, r) => sum + (r.Xp ?? 0), 0);
    if (totalXp > 0) parts.push(`${totalXp} XP`);
  }
  const items = props.quest.raw.Rewards_Items;
  if (items?.length) {
    parts.push(`${items.length} item${items.length > 1 ? 's' : ''}`);
  }
  return parts.join(", ");
});
</script>
