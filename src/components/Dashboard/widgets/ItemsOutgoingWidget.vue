<!-- DARK until LocalPlayer: lines recur (hidden 2026-10-28) -->
<template>
  <ActivityFeed
    :entries="store.itemsOutgoing"
    dot-color="bg-status-inactive"
    empty-text="No items lost."
    empty-hint="Sold, stored, and consumed items appear here."
    show-item-links
    unit="items"
    quantity-prefix
    :warning-tooltip="ACCURACY_WARNING" />
</template>

<script setup lang="ts">
import { useGameStateStore } from '../../../stores/gameStateStore'
import ActivityFeed from '../ActivityFeed.vue'

const store = useGameStateStore()

const ACCURACY_WARNING = 'Right now Glogger is doing the best it can to try to infer and figure out quantities, stack sizes, etc. However, due to limitations in the log files that is not a straightforward task. Do not be surprised if this is wrong! The best way to ensure Glogger has an accurate picture of your inventory is always Export Storage as JSON in Settings → VIP.'
</script>
