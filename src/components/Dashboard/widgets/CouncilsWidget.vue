<template>
  <div class="flex flex-col gap-2">
    <!-- Estimated council balance: last export anchor + live chat deltas -->
    <div class="flex items-baseline justify-between gap-2 px-1">
      <div class="flex items-baseline gap-1.5">
        <span class="text-xl font-mono text-accent-gold">{{ estimateLabel }}</span>
        <span class="text-xs text-text-dim">councils</span>
      </div>
      <span
        v-if="estimate?.has_anchor"
        class="text-[0.65rem] text-text-dim italic cursor-help"
        :title="ESTIMATE_TOOLTIP">
        ≈ est. · anchored {{ anchorAgo }}
      </span>
      <span v-else class="text-[0.65rem] text-text-dim italic">
        no export yet — import to anchor
      </span>
    </div>

    <ActivityFeed
      :entries="store.councilChanges"
      dot-color="bg-status-warning"
      empty-text="No council changes."
      empty-hint="Vendor sales, loot, and council transactions appear here."
      unit="councils"
      signed-total
      :warning-tooltip="ACCURACY_WARNING" />
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import { useGameStateStore } from '../../../stores/gameStateStore'
import ActivityFeed from '../ActivityFeed.vue'

const store = useGameStateStore()
const estimate = computed(() => store.currencyEstimate)

const estimateLabel = computed(() => {
  const e = estimate.value
  if (!e || !e.has_anchor) return '—'
  return e.estimated.toLocaleString()
})

/** Relative age of the export anchor (anchor_at is UTC "YYYY-MM-DD HH:MM:SS"). */
const anchorAgo = computed(() => {
  const at = estimate.value?.anchor_at
  if (!at) return ''
  const ms = Date.parse(at.replace(' ', 'T') + 'Z')
  if (Number.isNaN(ms)) return ''
  const mins = Math.max(0, Math.round((Date.now() - ms) / 60000))
  if (mins < 1) return 'just now'
  if (mins < 60) return `${mins}m ago`
  const hrs = Math.floor(mins / 60)
  if (hrs < 24) return `${hrs}h ago`
  return `${Math.floor(hrs / 24)}d ago`
})

const ESTIMATE_TOOLTIP =
  'Estimated council balance: your last character export plus wallet changes seen in chat since. Income (loot, sales, gifts) is tracked well, but most spending is invisible in the logs, so this drifts high until your next export re-anchors it.'

const ACCURACY_WARNING = 'Right now Glogger is doing the best it can to try to infer and figure out quantities, stack sizes, etc. However, due to limitations in the log files that is not a straightforward task. Do not be surprised if this is wrong! The best way to ensure Glogger has an accurate picture of your inventory is always Export Storage as JSON in Settings → VIP.'
</script>
