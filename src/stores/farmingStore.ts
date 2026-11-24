import { defineStore, acceptHMRUpdate } from "pinia";
import { ref, computed } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { useSettingsStore } from "./settingsStore";
import type { EnemyKilledPayload } from "./combatStore";

export interface BossKillRow {
  enemy_name: string;
  killed_at: string;
  health_damage: number;
  armor_damage: number;
}

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
  // enemy name -> sorted kill timestamps (live session plus `enemy_kills`
  // baseline re-seed via loadBossBaseline after restart/character switch)
  const bossKills = ref<Record<string, string[]>>({});
  function insertBossKill(name: string, timestamp: string) {
    const list = bossKills.value[name] ?? [];
    // Exact-timestamp repeats are duplicate emits of one kill, not new kills.
    if (list.includes(timestamp)) return;
    list.push(timestamp);
    // keep sorted ascending by time
    list.sort((a, b) => {
      const ma = parseKillTsMs(a);
      const mb = parseKillTsMs(b);
      if (!Number.isFinite(ma) || !Number.isFinite(mb)) return a.localeCompare(b);
      return ma - mb;
    });
    bossKills.value = { ...bossKills.value, [name]: list };
  }

  function handleEnemyKilled(payload: EnemyKilledPayload) {
    if (!isBossLike(payload.enemy_name, payload.health_damage, payload.armor_damage)) return;
    insertBossKill(payload.enemy_name, payload.timestamp);
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

  const bossBaselineLoaded = ref(false);

  // Persisted timers: re-seed boss kill timestamps from `enemy_kills` after
  // restart/character switch. The backend pre-filter is coarse; `isBossLike`
  // stays the single authority here, and inserts run through the same
  // dedup+sort path as live kills so a replayed live kill never doubles.
  async function loadBossBaseline() {
    if (bossBaselineLoaded.value) return;
    const settings = useSettingsStore();
    const characterName = settings.settings.activeCharacterName ?? null;
    const serverName = settings.settings.activeServerName ?? null;
    try {
      const rows = await invoke<BossKillRow[]>("get_boss_kill_times", {
        characterName,
        serverName,
      });
      // Rapid character switches: resetForCharacter cleared the flag and a
      // newer load is in flight (or done). Drop stale rows instead of mixing
      // another character's kills into this view.
      const current = useSettingsStore();
      if (
        current.settings.activeCharacterName !== characterName ||
        current.settings.activeServerName !== serverName
      )
        return;
      for (const row of rows) {
        if (!isBossLike(row.enemy_name, row.health_damage, row.armor_damage)) continue;
        insertBossKill(row.enemy_name, row.killed_at);
      }
      bossBaselineLoaded.value = true;
    } catch (e) {
      console.error("[farming] Failed to load boss baseline:", e);
    }
  }

  function reset() {
    bossKills.value = {};
    bossBaselineLoaded.value = false;
  }

  function resetForCharacter() {
    reset();
  }

  return {
    bossKills,
    bossTimers,
    handleEnemyKilled,
    loadBossBaseline,
    reset,
    resetForCharacter,
    isBossLike,
  };
});

if (import.meta.hot) {
  import.meta.hot.accept(acceptHMRUpdate(useFarmingStore, import.meta.hot));
}
