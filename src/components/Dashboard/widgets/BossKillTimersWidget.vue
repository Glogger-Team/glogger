<template>
  <div class="flex flex-col gap-2 text-sm h-full min-h-0">
    <div v-if="timers.length === 0" class="text-xs text-text-dim italic">
      No boss/named kills yet this session.
    </div>
    <div v-else class="flex flex-col gap-1 overflow-y-auto pr-1 min-h-0 flex-1">
      <div
        v-for="t in timers"
        :key="t.enemyName"
        class="flex items-center justify-between gap-2 px-1 py-0.5 rounded hover:bg-surface-elevated"
        :title="title(t)">
        <span class="truncate text-text-primary">{{ t.enemyName }}</span>
        <span
          class="shrink-0 tabular-nums text-xs font-mono"
          :class="t.ready ? 'text-accent-green' : 'text-accent-gold'">
          {{ t.label }}
        </span>
      </div>
    </div>
    <p class="text-[10px] text-text-dim/70 leading-tight mt-auto">
      Boss/named inferred from kill fields (no log class signal); lockout is the shortest observed gap, capped at 24h, with a 24h default until observed.
    </p>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted } from "vue";
import { useFarmingStore, parseKillTsMs } from "../../../stores/farmingStore";

const store = useFarmingStore();

const now = ref(Date.now());
let timer: ReturnType<typeof setInterval> | undefined;

onMounted(() => {
  // Reseed persisted timers on fresh restart: `character-login` only fires
  // on a live login line, so without this the widget stays empty until the
  // next switch. Guarded + idempotent (CombatView precedent).
  void store.loadBossBaseline();
  timer = setInterval(() => {
    now.value = Date.now();
  }, 1000);
});
onUnmounted(() => {
  if (timer) clearInterval(timer);
});

const WIKI_MAX_LOCKOUT_SECS = 24 * 3600;

function cooldownSecs(minGap: number | null): number {
  if (minGap != null && minGap > 0) return Math.min(minGap, WIKI_MAX_LOCKOUT_SECS);
  return WIKI_MAX_LOCKOUT_SECS;
}

const timers = computed(() => {
  return store.bossTimers
    .map((t) => {
      const lastMs = parseKillTsMs(t.lastKilledAt);
      const remainMs = Number.isFinite(lastMs)
        ? lastMs + cooldownSecs(t.minGapSecs) * 1000 - now.value
        : 0;
      const ready = remainMs <= 0;
      return {
        enemyName: t.enemyName,
        minGapSecs: t.minGapSecs,
        killCount: t.killCount,
        ready,
        remainMs,
        label: formatDuration(Math.ceil(remainMs / 1000)),
      };
    })
    .sort((a, b) => {
      if (a.ready !== b.ready) return a.ready ? 1 : -1;
      return a.remainMs - b.remainMs;
    });
});

function formatDuration(secs: number): string {
  if (secs <= 0) return "Ready";
  const h = Math.floor(secs / 3600);
  const min = Math.floor((secs % 3600) / 60);
  const s = Math.floor(secs % 60);
  if (h > 0) return `${h}h ${min}m`;
  if (min > 0) return `${min}m ${s}s`;
  return `${s}s`;
}

function title(t: { minGapSecs: number | null; killCount: number }): string {
  if (t.minGapSecs != null) {
    return `Shortest gap seen ${formatDuration(t.minGapSecs)} — lockout capped at 24h (kills: ${t.killCount})`;
  }
  return `24h default until observed (kills: ${t.killCount})`;
}
</script>
