<template>
  <div class="flex flex-col gap-4 h-full overflow-y-auto">
    <div class="text-[11px] text-text-dim italic">Time-boxed analytics over recorded rows — kills, loot gains, and XP awards between two timestamps. Councils have no per-event ledger yet (running estimate only), so they are not windowable.</div>
    <TimeboxedPanel />
  </div>
</template>

<script setup lang="ts">
import TimeboxedPanel from '../Shared/TimeboxedPanel.vue'
</script>
