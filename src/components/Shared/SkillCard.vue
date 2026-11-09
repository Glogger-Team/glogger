<template>
  <div class="bg-surface-card border border-border-default rounded-lg p-4 min-w-50">
    <div class="text-base font-bold text-accent-gold mb-1">{{ skill.skillType }}</div>
    <div class="text-xs text-text-secondary mb-3">
      Lv <span class="text-white font-bold">{{ effectiveLevel }}</span>
      <span v-if="bonusLevels > 0" class="text-accent-gold/60 ml-1" :title="`Base ${skill.currentLevel - bonusLevels} + ${bonusLevels} bonus = ${effectiveLevel}`">({{ skill.currentLevel - bonusLevels }} + {{ bonusLevels }})</span>
    </div>

    <div class="grid grid-cols-2 gap-2 mb-3">
      <div>
        <div class="text-[10px] leading-tight text-text-muted uppercase tracking-wide">XP Gained</div>
        <div class="text-sm text-text-primary font-bold">{{ skill.xpGained.toLocaleString() }}</div>
      </div>
      <div>
        <div class="text-[10px] leading-tight text-text-muted uppercase tracking-wide">XP / Hour</div>
        <div class="text-sm text-text-primary font-bold">{{ xphr }}</div>
      </div>
      <div>
        <div class="text-[10px] leading-tight text-text-muted uppercase tracking-wide">Levels Gained</div>
        <div class="text-sm text-text-primary font-bold">{{ skill.levelsGained }}</div>
      </div>
      <div>
        <div class="text-[10px] leading-tight text-text-muted uppercase tracking-wide">Next Level</div>
        <div class="text-sm text-text-primary font-bold">{{ ttl }}</div>
      </div>
    </div>

    <div class="h-1 bg-border-default rounded-sm overflow-hidden mb-1">
      <div class="h-full bg-accent-gold rounded-sm transition-all duration-300" :style="{ width: tnlPercent + '%' }"></div>
    </div>
    <div class="text-[10px] text-text-dim">{{ skill.tnl.toLocaleString() }} XP to next level</div>
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import { useGameStateStore, type SkillSessionData } from '../../stores/gameStateStore'
import { skillTotalLevel } from '../../types/gameState'

const props = defineProps<{ skill: SkillSessionData }>()
const store = useGameStateStore()

const bonusLevels = computed(() => {
  const gs = store.skillsByName[props.skill.skillType]
  return gs?.bonus_levels ?? 0
})

const effectiveLevel = computed(() => {
  const gs = store.skillsByName[props.skill.skillType]
  return gs ? skillTotalLevel(gs) : props.skill.currentLevel
})

const xphr = computed(() => store.xpPerHour(props.skill).toLocaleString())
const ttl  = computed(() => store.timeToNextLevel(props.skill))

const tnlPercent = computed(() => {
  const total = props.skill.xpGained + props.skill.tnl
  if (total <= 0) return 0
  return Math.min(100, Math.round((props.skill.xpGained / total) * 100))
})
</script>
