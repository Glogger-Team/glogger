import { defineStore, acceptHMRUpdate } from "pinia";
import { ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import type {
  EnemyKillStats,
} from "../types/farming";

export const useFarmingStore = defineStore("farming", () => {
  // All-time per-enemy loot stats, fetched lazily from the DB on hover and
  // cached by enemy name. Only surviving surface: historic readers use it via
  // fetchEnemyStats (loot-table hover details in the frozen History/Database tabs).
  const enemyStatsCache = ref<Record<string, EnemyKillStats>>({});

  // Lazily fetch (and cache) all-time loot stats for an enemy from the DB.
  async function fetchEnemyStats(enemyName: string): Promise<EnemyKillStats | null> {
    const cached = enemyStatsCache.value[enemyName];
    if (cached) return cached;
    try {
      const stats = await invoke<EnemyKillStats>("get_enemy_kill_stats", { enemyName, scope: "combined" });
      enemyStatsCache.value[enemyName] = stats;
      return stats;
    } catch (e) {
      console.error("[farming] Failed to fetch enemy stats:", e);
      return null;
    }
  }

  return {
    enemyStatsCache,
    fetchEnemyStats,
  };
});

if (import.meta.hot) {
  import.meta.hot.accept(acceptHMRUpdate(useFarmingStore, import.meta.hot))
}
