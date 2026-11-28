<template>
  <div class="flex flex-col gap-4 h-full overflow-y-auto">
    <div class="text-[11px] text-text-dim italic">Activity analytics over recorded rows — kills, loot gains, and XP awards between two timestamps. Councils keep a per-event ledger too — see the Wealth tab for the balance line.</div>
    <TimeboxedPanel :active="active" />
  </div>
</template>

<script setup lang="ts">
import TimeboxedPanel from '../Shared/TimeboxedPanel.vue'

withDefaults(defineProps<{ active?: boolean }>(), { active: true })
</script>
