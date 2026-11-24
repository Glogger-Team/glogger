import { defineStore, acceptHMRUpdate } from "pinia";
import { ref, computed } from "vue";
import type { EnemyKilledPayload } from "./combatStore";

// Boss/named kill loot timers. The log carries no explicit class signal, so a
// boss/named enemy is inferred from the fields EnemyKilled already carries:
//   - name starts with Boss/Elite/Named, or is a single capitalised proper noun
//     (generic mobs are usually "Adjective Noun" pairs), or
//   - the kill's total damage (health + armour) is large (high-HP boss).
// Reverse tracking (enemy_kills) already exists; this store only owns the
// session-side per-boss last-kill timestamps and observed reuse gaps.

export interface BossKillEntry {
  enemyName: string;
  killTimes: string[];
}


function isBossLike(name: string, healthDamage: number, armorDamage: number): boolean {
  const trimmed = name.trim();
  if (/^(Boss|Elite|Named)\b/i.test(trimmed)) return true;
  if (healthDamage + armorDamage >= 8000) return true;
  return false;
}

function parseTsMs(ts: string): number {
  const ms = Date.parse(ts.replace(" ", "T") + "Z");
  return Number.isFinite(ms) ? ms : NaN;
}

export const useFarmingStore = defineStore("farming", () => {
  // enemy name -> sorted kill timestamps (session only)
  const bossKills = ref<Record<string, string[]>>({});

  function handleEnemyKilled(payload: EnemyKilledPayload) {
    if (!isBossLike(payload.enemy_name, payload.health_damage, payload.armor_damage)) return;
    const name = payload.enemy_name;
    const list = bossKills.value[name] ?? [];
    list.push(payload.timestamp);
    // keep sorted ascending by time
    list.sort((a, b) => {
      const ma = parseTsMs(a);
      const mb = parseTsMs(b);
      if (!Number.isFinite(ma) || !Number.isFinite(mb)) return a.localeCompare(b);
      return ma - mb;
    });
    bossKills.value = { ...bossKills.value, [name]: list };
  }

  function minGapSecs(times: string[]): number | null {
    if (times.length < 2) return null;
    let min: number | null = null;
    for (let i = 1; i < times.length; i++) {
      const a = parseTsMs(times[i - 1]);
      const b = parseTsMs(times[i]);
      if (!Number.isFinite(a) || !Number.isFinite(b)) continue;
      const gap = (b - a) / 1000;
      if (gap < 60) continue; // skip duplicate emits
      if (min === null || gap < min) min = gap;
    }
    return min;
  }

  const bossTimers = computed(() =>
    Object.entries(bossKills.value).map(([name, times]) => ({
      enemyName: name,
      killTimes: times,
      lastKilledAt: times[times.length - 1],
      killCount: times.length,
      minGapSecs: minGapSecs(times),
    })),
  );

  function reset() {
    bossKills.value = {};
  }

  function resetForCharacter() {
    reset();
  }

  return {
    bossKills,
    bossTimers,
    handleEnemyKilled,
    reset,
    resetForCharacter,
    isBossLike,
  };
});

if (import.meta.hot) {
  import.meta.hot.accept(acceptHMRUpdate(useFarmingStore, import.meta.hot));
}
