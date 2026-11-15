import { defineStore, acceptHMRUpdate } from "pinia";
import { ref, computed } from "vue";
import { invoke } from "@tauri-apps/api/core";
import type { CombatLogEntry } from "../types/combat";
import type { HistoricalFarmingSession } from "../types/farming";

export interface CombatKillTally {
  count: number;
  abilities: Record<string, number>;
  lastSeen: string;
}

export interface CombatXpTally {
  gained: number;
  prodigy: number;
  levels: number;
  lastLevel: number | null;
}

export interface EnemyKilledPayload {
  timestamp: string;
  enemy_name: string;
  enemy_entity_id: string;
  killing_ability: string;
  health_damage: number;
  armor_damage: number;
}

export interface DeathPayload {
  timestamp: string;
  killer_name: string;
  killing_ability: string;
}

export type CombatChatStatusPayload =
  | { kind: "XpGained"; timestamp: string; skill: string; amount: number }
  | { kind: "ProdigyXpGained"; timestamp: string; skill: string; amount: number }
  | { kind: "LevelUp"; timestamp: string; skill: string; level: number; xp: number }
  | { kind: "CombatWisdomEarned"; timestamp: string; amount: number; source_name: string | null; verb: string; zone: string | null };

export const useCombatStore = defineStore("combat", () => {
  const kills = ref<Record<string, CombatKillTally>>({});
  const xp = ref<Record<string, CombatXpTally>>({});
  const log = ref<CombatLogEntry[]>([]);
  const startedAt = ref<string | null>(null);
  const historicKills = ref<Record<string, number>>({});
  const historicXp = ref<Record<string, number>>({});
  const baselineLoaded = ref(false);

  async function loadHistoricBaseline() {
    if (baselineLoaded.value) return;
    try {
      const sessions = await invoke<HistoricalFarmingSession[]>("get_farming_sessions", { limit: 500 });
      const killTotals: Record<string, number> = {};
      const xpTotals: Record<string, number> = {};
      for (const s of sessions) {
        for (const k of s.kills ?? []) {
          killTotals[k.enemy_name] = (killTotals[k.enemy_name] ?? 0) + k.kill_count;
        }
        for (const sk of s.skills ?? []) {
          xpTotals[sk.skill_name] = (xpTotals[sk.skill_name] ?? 0) + sk.xp_gained;
        }
      }
      historicKills.value = killTotals;
      historicXp.value = xpTotals;
    } catch (e) {
      console.error("[combat] Failed to load historic baseline:", e);
    } finally {
      baselineLoaded.value = true;
    }
  }

  function ensureStarted(timestamp: string) {
    if (!startedAt.value) startedAt.value = timestamp;
  }

  function pushLog(entry: CombatLogEntry) {
    log.value.unshift(entry);
    if (log.value.length > 500) log.value.length = 500;
  }

  function handleEnemyKilled(payload: EnemyKilledPayload) {
    ensureStarted(payload.timestamp);
    const existing = kills.value[payload.enemy_name] ?? { count: 0, abilities: {}, lastSeen: payload.timestamp };
    existing.count += 1;
    existing.abilities[payload.killing_ability] = (existing.abilities[payload.killing_ability] ?? 0) + 1;
    existing.lastSeen = payload.timestamp;
    kills.value[payload.enemy_name] = existing;
    pushLog({
      kind: "kill",
      timestamp: payload.timestamp,
      label: `Killed ${payload.enemy_name}`,
      detail: `${payload.killing_ability} (${payload.health_damage}h + ${payload.armor_damage}a)`,
    });
  }

  function handleChatStatus(payload: CombatChatStatusPayload) {
    switch (payload.kind) {
      case "XpGained": {
        ensureStarted(payload.timestamp);
        const entry = xp.value[payload.skill] ?? { gained: 0, prodigy: 0, levels: 0, lastLevel: null };
        entry.gained += payload.amount;
        xp.value[payload.skill] = entry;
        break;
      }
      case "ProdigyXpGained": {
        ensureStarted(payload.timestamp);
        const entry = xp.value[payload.skill] ?? { gained: 0, prodigy: 0, levels: 0, lastLevel: null };
        entry.gained += payload.amount;
        entry.prodigy += payload.amount;
        xp.value[payload.skill] = entry;
        break;
      }
      case "LevelUp": {
        ensureStarted(payload.timestamp);
        const entry = xp.value[payload.skill] ?? { gained: 0, prodigy: 0, levels: 0, lastLevel: null };
        entry.levels += 1;
        entry.lastLevel = payload.level;
        xp.value[payload.skill] = entry;
        pushLog({
          kind: "level",
          timestamp: payload.timestamp,
          label: `${payload.skill} reached level ${payload.level}`,
        });
        break;
      }
      case "CombatWisdomEarned": {
        ensureStarted(payload.timestamp);
        pushLog({
          kind: "wisdom",
          timestamp: payload.timestamp,
          label: `+${payload.amount} Combat Wisdom — ${payload.source_name ?? "Prodigy Level"}`,
        });
        break;
      }
    }
  }

  function handleDeathEvent(payload: DeathPayload) {
    ensureStarted(payload.timestamp);
    pushLog({
      kind: "death",
      timestamp: payload.timestamp,
      label: `Killed by ${payload.killer_name}`,
      detail: payload.killing_ability,
    });
  }

  function reset() {
    kills.value = {};
    xp.value = {};
    log.value = [];
    startedAt.value = null;
  }

  const liveKills = computed(() =>
    Object.values(kills.value).reduce((sum, k) => sum + k.count, 0),
  );

  const lifetimeKills = computed(() => {
    const historic = Object.values(historicKills.value).reduce((sum, n) => sum + n, 0);
    return historic + liveKills.value;
  });

  const liveXp = computed(() =>
    Object.values(xp.value).reduce((sum, x) => sum + x.gained, 0),
  );

  function elapsedHours(): number {
    if (!startedAt.value) return 0;
    // Backend chat stamps are UTC "YYYY-MM-DD HH:MM:SS" (no zone suffix);
    // bare Date.parse would read them as local time and shift start by the TZ offset.
    const ms = Date.now() - Date.parse(startedAt.value.replace(' ', 'T') + 'Z');
    if (!Number.isFinite(ms) || ms <= 0) return 0;
    return ms / 3_600_000;
  }

  const killsPerHour = computed(() => {
    const hours = elapsedHours();
    if (hours <= 0) return 0;
    return Math.round(liveKills.value / hours);
  });

  const xpPerHour = computed(() => {
    const hours = elapsedHours();
    if (hours <= 0) return 0;
    return Math.round(liveXp.value / hours);
  });

  const killRows = computed(() =>
    Object.entries(kills.value)
      .map(([name, tally]) => {
        const topAbility = Object.entries(tally.abilities).sort((a, b) => b[1] - a[1])[0]?.[0] ?? "";
        return {
          name,
          live: tally.count,
          lifetime: tally.count + (historicKills.value[name] ?? 0),
          topAbility,
          lastSeen: tally.lastSeen,
        };
      })
      .sort((a, b) => b.live - a.live),
  );

  const xpRows = computed(() =>
    Object.entries(xp.value)
      .map(([skill, tally]) => ({
        skill,
        gained: tally.gained,
        prodigy: tally.prodigy,
        levels: tally.levels,
        lastLevel: tally.lastLevel,
      }))
      .sort((a, b) => b.gained - a.gained),
  );

  return {
    kills,
    xp,
    log,
    startedAt,
    historicKills,
    historicXp,
    baselineLoaded,
    loadHistoricBaseline,
    handleEnemyKilled,
    handleChatStatus,
    handleDeathEvent,
    reset,
    liveKills,
    lifetimeKills,
    liveXp,
    killsPerHour,
    xpPerHour,
    killRows,
    xpRows,
  };
});

if (import.meta.hot) {
  import.meta.hot.accept(acceptHMRUpdate(useCombatStore, import.meta.hot))
}
