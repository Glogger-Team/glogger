<template>
  <div class="flex flex-col gap-3">
    <div v-if="loading" class="flex flex-col gap-3">
      <SkeletonLoader variant="rect" height="h-4" width="w-2/3" />
      <SkeletonLoader variant="rect" height="h-1.5" />
      <SkeletonLoader variant="text" :lines="3" />
    </div>

    <template v-else>
      <!-- Progress header -->
      <div class="flex items-center justify-between text-sm">
        <span
          :title="`Each Statehelm NPC accepts ${MAX_GIFTS_PER_WEEK} gifts per week — ${trackedNpcCount} tracked NPCs × ${MAX_GIFTS_PER_WEEK}. Every NPC's cap resets Monday 00:00 America/New_York.`">
          <span class="text-accent-gold font-bold">{{ totalGiftsGiven }}</span>
          <span class="text-text-muted"> of {{ totalGiftsMax }} gift slots used</span>
        </span>
        <span class="text-xs text-text-dim">{{ resetLabel }}</span>
      </div>

      <!-- Where the week's gifts went. The list below is "who's next", so a maxed
           NPC (5/5) is gone from it while still counting in the header. -->
      <div v-if="giftedThisWeek.length > 0" class="text-[11px] leading-snug text-text-dim">
        Gifted this week:
        {{ giftedThisWeek.slice(0, 4).map(g => `${g.name} ${g.gifts}/${g.max}`).join(' · ')
        }}<span v-if="giftedThisWeek.length > 4"> +{{ giftedThisWeek.length - 4 }} more</span>
      </div>

      <!-- Progress bar -->
      <div class="h-1.5 bg-surface-elevated rounded-full overflow-hidden">
        <div
          class="h-full bg-accent-gold rounded-full transition-all duration-300"
          :style="{ width: progressPct + '%' }" />
      </div>

      <!-- Combat section -->
      <div v-if="combatGiftTargets.length > 0" class="flex flex-col gap-1.5">
        <div class="text-[10px] uppercase tracking-wide text-text-dim font-semibold">Combat</div>
        <TargetRow v-for="t in combatGiftTargets" :key="t.npc.key" :target="t" />
      </div>

      <!-- Non-combat section -->
      <div v-if="nonCombatGiftTargets.length > 0" class="flex flex-col gap-1.5">
        <div class="text-[10px] uppercase tracking-wide text-text-dim font-semibold">Non-Combat</div>
        <TargetRow v-for="t in nonCombatGiftTargets" :key="t.npc.key" :target="t" />
      </div>

      <div
        v-if="combatGiftTargets.length === 0 && nonCombatGiftTargets.length === 0"
        class="text-xs italic"
        :class="totalGiftsMax > 0 ? 'text-accent-green' : 'text-text-dim'">
        {{ totalGiftsMax > 0 ? 'All relevant NPCs maxed this week!' : 'No Statehelm NPCs tracked yet.' }}
      </div>
    </template>
  </div>
</template>

<script setup lang="ts">
import { computed, onMounted, h, type FunctionalComponent } from 'vue'
import { useStatehelmTracker, MAX_GIFTS_PER_WEEK, type StatehelmGiftTarget } from '../../../composables/useStatehelmTracker'
import NpcInline from '../../Shared/NPC/NpcInline.vue'
import SkeletonLoader from '../../Shared/SkeletonLoader.vue'

const {
  combatGiftTargets,
  nonCombatGiftTargets,
  totalGiftsGiven,
  totalGiftsMax,
  trackedNpcCount,
  giftedThisWeek,
  loading,
  loadGiftLog,
  loadSkillMeta,
  nextWeekStart,
} = useStatehelmTracker()

onMounted(() => {
  loadGiftLog()
  loadSkillMeta()
})

/** A single gift-target row: NPC name (+ driving skill / equipped marker) and gift dots. */
const TargetRow: FunctionalComponent<{ target: StatehelmGiftTarget }> = (props) => {
  const t = props.target
  return h('div', { class: 'flex items-center justify-between gap-2 text-sm' }, [
    h('span', { class: 'flex items-center gap-1.5 min-w-0' }, [
      h(NpcInline, { reference: t.npc.key, npc: t.npc }),
      h('span', { class: 'text-[10px] text-text-dim truncate' }, [
        t.drivingSkill,
        t.equipped ? h('span', { class: 'text-accent-gold ml-0.5', title: 'Currently equipped' }, '✦') : null,
      ]),
    ]),
    h('span', { class: 'text-xs font-mono shrink-0 tracking-wide' },
      Array.from({ length: t.maxGifts }, (_, i) =>
        h('span', { class: i < t.giftsThisWeek ? 'text-accent-gold' : 'text-text-dim' }, '●')
      )
    ),
  ])
}

const progressPct = computed(() => {
  if (totalGiftsMax.value === 0) return 0
  return Math.round((totalGiftsGiven.value / totalGiftsMax.value) * 100)
})

/** Time until weekly reset (Monday 00:00 America/New_York — the game's own
 *  reset instant, from `useStatehelmTracker`'s shared boundary; not
 *  `weekStart + 7 days`, which is an hour off in the DST-transition weeks) */
const resetLabel = computed(() => {
  const resetTime = nextWeekStart.value
  const now = new Date()
  const diff = resetTime.getTime() - now.getTime()
  if (diff <= 0) return 'Resetting...'

  const days = Math.floor(diff / (24 * 60 * 60 * 1000))
  const hours = Math.floor((diff % (24 * 60 * 60 * 1000)) / (60 * 60 * 1000))

  if (days > 0) return `Resets in ${days}d ${hours}h`
  return `Resets in ${hours}h`
})
</script>
