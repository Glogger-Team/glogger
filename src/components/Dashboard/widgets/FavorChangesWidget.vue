<!-- DARK until LocalPlayer: lines recur (hidden 2026-10-28) -->
<template>
  <ActivityFeed
    :entries="store.favorChanges"
    dot-color="bg-purple-500"
    empty-text="No favor changes."
    empty-hint="NPC favor gains and losses appear here."
    show-npc-links
    unit="favor"
    signed-total />
</template>

<script setup lang="ts">
import { useGameStateStore } from '../../../stores/gameStateStore'
import ActivityFeed from '../ActivityFeed.vue'

const store = useGameStateStore()
</script>
