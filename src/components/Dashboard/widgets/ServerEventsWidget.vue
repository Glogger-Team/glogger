<template>
  <div class="flex flex-col gap-1.5 text-sm">
    <div v-if="upcoming.length === 0" class="text-xs text-text-dim italic">
      No events for this server yet — open the gear menu to add some.
    </div>

    <div v-for="e in upcoming" :key="e.id"
      class="rounded px-2 py-1.5 border"
      :class="e.inProgress
        ? 'bg-accent-gold/10 border-accent-gold/40'
        : 'bg-surface-base/30 border-border-default/30'">
      <div class="flex items-center gap-2">
        <span class="text-xs text-text-primary font-semibold truncate flex-1 min-w-0">{{ e.name }}</span>
        <span
          class="text-xs font-mono whitespace-nowrap shrink-0"
          :class="e.inProgress ? 'text-accent-gold font-bold' : urgent(e) ? 'text-red-400' : 'text-text-dim'">
          {{ e.inProgress ? 'Live now!' : formatCountdown(e.secondsUntil) }}
        </span>
      </div>
      <div class="text-[11px] text-text-muted truncate">
        {{ formatEastern(e.nextStart) }} · {{ e.location }}
      </div>
      <div v-if="e.note" class="text-[11px] text-text-dim truncate">{{ e.note }}</div>
    </div>

    <div class="text-[10px] text-text-dim">
      Weekly jams + seasonal events for {{ activeServer ?? 'all servers' }}. Gear menu edits the schedule.
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import { useSettingsStore } from '../../../stores/settingsStore'
import {
  useServerEvents,
  formatEastern,
  formatCountdown,
  type UpcomingEvent,
} from '../../../composables/useServerEvents'

const { upcoming } = useServerEvents()
const settings = useSettingsStore()
const activeServer = computed(() => settings.settings.activeServerName)

/** Flag when a jam starts within the hour. */
function urgent(e: UpcomingEvent): boolean {
  return !e.inProgress && e.secondsUntil <= 3600
}
</script>
