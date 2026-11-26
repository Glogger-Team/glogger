<template>
  <div class="flex flex-col gap-2">
    <p v-if="plannableCount === 0" class="text-sm text-text-dim">
      All Statehelm NPCs are at their weekly gift cap.
    </p>
    <p v-else class="text-sm">
      <span class="font-semibold text-accent-gold">{{ remainingTotal }}</span>
      <span class="text-text-secondary">
        gift{{ remainingTotal === 1 ? '' : 's' }} left across {{ plannableCount }}
        NPC{{ plannableCount === 1 ? '' : 's' }} — vault matches waiting in the planner.
      </span>
    </p>
    <button
      class="text-xs px-2 py-1 rounded bg-accent-gold/15 text-accent-gold hover:bg-accent-gold/25 self-start"
      @click="openPlanner">
      Open Gift Planner
    </button>
  </div>
</template>

<script setup lang="ts">
import { computed, onMounted } from 'vue'
import { useStatehelmTracker } from '../../../composables/useStatehelmTracker'
import { useViewNavigation } from '../../../composables/useViewNavigation'

const tracker = useStatehelmTracker()
const { navigateToView } = useViewNavigation()

const plannable = computed(() =>
  tracker.npcStatuses.value.filter((s) => s.giftTracking && s.giftsThisWeek < s.maxGifts),
)
const plannableCount = computed(() => plannable.value.length)
const remainingTotal = computed(() =>
  plannable.value.reduce((sum, s) => sum + (s.maxGifts - s.giftsThisWeek), 0),
)

function openPlanner() {
  navigateToView({ view: 'character', subTab: 'statehelm' })
}

onMounted(() => {
  tracker.loadGiftLog()
  tracker.loadSkillMeta()
})
</script>
