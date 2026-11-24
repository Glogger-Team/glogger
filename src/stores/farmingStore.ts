import { defineStore, acceptHMRUpdate } from "pinia";
import { ref, computed } from "vue";
import type { EnemyKilledPayload } from "./combatStore";

// Boss/named kill loot timers. The log carries no explicit class signal, so a
// boss/named enemy is inferred from the fields EnemyKilled already carries:
//   - name starts with Boss/Elite/Named, or
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

/** Parse a kill timestamp to epoch ms. Backend stamps are UTC
 * "%Y-%m-%d %H:%M:%S" (no zone suffix, fixed at the parser), so the
 * zone-free shape gets a `Z`; an already-zoned ISO string passes through
 * untouched so a stray zone is never double-suffixed into NaN. */
export function parseKillTsMs(ts: string): number {
  const trimmed = ts.trim();
  if (/[zZ]$|[+-]\d{2}:?\d{2}$/.test(trimmed)) {
    const ms = Date.parse(trimmed);
    return Number.isFinite(ms) ? ms : NaN;
  }
  const ms = Date.parse(trimmed.replace(" ", "T") + "Z");
  return Number.isFinite(ms) ? ms : NaN;
}

export const useFarmingStore = defineStore("farming", () => {
  // enemy name -> sorted kill timestamps (session only)
  const bossKills = ref<Record<string, string[]>>({});

  function handleEnemyKilled(payload: EnemyKilledPayload) {
    if (!isBossLike(payload.enemy_name, payload.health_damage, payload.armor_damage)) return;
    const name = payload.enemy_name;
    const list = bossKills.value[name] ?? [];
    // Exact-timestamp repeats are duplicate emits of one kill, not new kills.
    if (list.includes(payload.timestamp)) return;
    list.push(payload.timestamp);
    // keep sorted ascending by time
    list.sort((a, b) => {
      const ma = parseKillTsMs(a);
      const mb = parseKillTsMs(b);
      if (!Number.isFinite(ma) || !Number.isFinite(mb)) return a.localeCompare(b);
      return ma - mb;
    });
    bossKills.value = { ...bossKills.value, [name]: list };
  }

  function minGapSecs(times: string[]): number | null {
    if (times.length < 2) return null;
    let min: number | null = null;
    for (let i = 1; i < times.length; i++) {
      const a = parseKillTsMs(times[i - 1]);
      const b = parseKillTsMs(times[i]);
      if (!Number.isFinite(a) || !Number.isFinite(b)) continue;
      const gap = (b - a) / 1000;
      // Exact repeats are deduped on insert, so any positive gap is two
      // distinct kills and counts toward the observed lockout.
      if (gap <= 0) continue;
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
