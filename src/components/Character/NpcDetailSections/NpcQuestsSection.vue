<template>
  <div v-if="sortedQuests.length" class="flex flex-col gap-1.5">
    <div class="text-[0.65rem] uppercase tracking-widest text-text-dim border-b border-surface-card pb-0.5">
      Quests ({{ sortedQuests.length }})
    </div>

    <div class="flex flex-col gap-0.5">
      <div
        v-for="quest in sortedQuests"
        :key="quest.internal_name"
        class="flex items-center gap-2 px-2 py-0.5 text-xs bg-surface-inset rounded">
        <QuestInline :reference="quest.internal_name" />
        <span v-if="isRepeatable(quest)" class="text-[10px] text-cyan-400 shrink-0" title="Repeatable">
          &#x21BB;
        </span>
        <span v-if="quest.raw.Reward_Favor" class="text-[0.6rem] text-accent-gold font-mono ml-auto shrink-0">
          +{{ quest.raw.Reward_Favor }}
        </span>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import type { QuestInfo } from '../../../types/gameData'
import { useGameDataStore } from '../../../stores/gameDataStore'
import QuestInline from '../../Shared/Quest/QuestInline.vue'

const props = defineProps<{
  npcKey: string
}>()

const gameData = useGameDataStore()

function isRepeatable(quest: QuestInfo): boolean {
  return (quest.raw.ReuseTime_Minutes != null && quest.raw.ReuseTime_Minutes > 0) ||
    (quest.raw.ReuseTime_Days != null && quest.raw.ReuseTime_Days > 0)
}

const sortedQuests = computed(() => {
  const quests = gameData.getQuestsForNpc(props.npcKey)
  return [...quests].sort((a, b) => {
    const aRepeat = isRepeatable(a) ? 1 : 0
    const bRepeat = isRepeatable(b) ? 1 : 0
    if (aRepeat !== bRepeat) return aRepeat - bRepeat
    return (b.raw.Reward_Favor ?? 0) - (a.raw.Reward_Favor ?? 0)
  })
})
</script>
