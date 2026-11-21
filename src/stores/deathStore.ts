import { defineStore, acceptHMRUpdate } from "pinia";
import { ref, computed } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { useSettingsStore } from "./settingsStore";

export interface CharacterDeath {
  id: number;
  character_name: string;
  server_name: string;
  died_at: string;
  killer_name: string;
  killer_entity_id: string | null;
  killing_ability: string;
  health_damage: number;
  armor_damage: number;
  area: string | null;
  damage_type: string | null;
}

export interface DeathDamageSource {
  id: number;
  death_id: number;
  event_order: number;
  timestamp: string;
  attacker_name: string;
  attacker_entity_id: string | null;
  ability_name: string;
  health_damage: number;
  armor_damage: number;
  is_crit: boolean;
}

export const useDeathStore = defineStore("deaths", () => {
  const deaths = ref<CharacterDeath[]>([]);
  const loaded = ref(false);
  /** Bumped on every invalidation (character switch, missing identity) so an
   * in-flight load drops its stale result instead of overwriting the new
   * character's rows. Same shape as characterStore.allCharacterItemsGeneration. */
  const deathsGeneration = ref(0);
  /** Cache of damage sources per death ID, loaded on demand. */
  const damageSourcesCache = ref<Record<number, DeathDamageSource[]>>({});

  async function loadDeaths() {
    const settings = useSettingsStore();
    const characterName = settings.settings.activeCharacterName;
    const serverName = settings.settings.activeServerName;
    if (!characterName || !serverName) {
      deathsGeneration.value++;
      deaths.value = [];
      loaded.value = false;
      return;
    }

    const generation = deathsGeneration.value;
    try {
      const rows = await invoke<CharacterDeath[]>("get_character_deaths", {
        characterName,
        serverName,
      });
      // Drop stale results: invalidated mid-flight, or the active identity
      // moved on while the fetch was out.
      if (generation !== deathsGeneration.value) return;
      if (
        settings.settings.activeCharacterName !== characterName ||
        settings.settings.activeServerName !== serverName
      )
        return;
      // Merge instead of overwrite: a live death event may have unshifted a
      // temp row while the fetch was in flight. Key on real id; drop temp
      // Date.now() rows whose DB row arrived (same character + server +
      // died_at + killer); never carry another identity's rows.
      const merged = new Map<number, CharacterDeath>();
      for (const row of rows) merged.set(row.id, row);
      for (const existing of deaths.value) {
        if (merged.has(existing.id)) continue;
        if (existing.character_name !== characterName || existing.server_name !== serverName) continue;
        const arrived = rows.some(
          (row) =>
            row.character_name === existing.character_name &&
            row.server_name === existing.server_name &&
            row.died_at === existing.died_at &&
            row.killer_name === existing.killer_name,
        );
        if (arrived) continue;
        merged.set(existing.id, existing);
      }
      deaths.value = [...merged.values()].sort((a, b) =>
        a.died_at < b.died_at ? 1 : a.died_at > b.died_at ? -1 : b.id - a.id,
      );
      loaded.value = true;
    } catch (e) {
      // Keep old rows on failure — never blank the list.
      console.error("[deaths] Failed to load deaths:", e);
    }
  }

  function handleDeathEvent(payload: {
    kind: string;
    timestamp: string;
    killer_name: string;
    killer_entity_id: string;
    killing_ability: string;
    health_damage: number;
    armor_damage: number;
  }) {
    const settings = useSettingsStore();
    // Prepend the new death (most recent first)
    deaths.value.unshift({
      id: Date.now(), // Temporary ID until next DB load
      character_name: settings.settings.activeCharacterName ?? "",
      server_name: settings.settings.activeServerName ?? "",
      died_at: payload.timestamp,
      killer_name: payload.killer_name,
      killer_entity_id: payload.killer_entity_id,
      killing_ability: payload.killing_ability,
      health_damage: payload.health_damage,
      armor_damage: payload.armor_damage,
      area: null, // Area comes from DB record; live events don't include it yet
      damage_type: null, // Damage type comes from DB record
    });
  }

  async function loadDamageSources(deathId: number): Promise<DeathDamageSource[]> {
    if (damageSourcesCache.value[deathId]) {
      return damageSourcesCache.value[deathId];
    }
    try {
      const sources = await invoke<DeathDamageSource[]>("get_death_damage_sources", { deathId });
      damageSourcesCache.value[deathId] = sources;
      return sources;
    } catch (e) {
      console.error("[deaths] Failed to load damage sources:", e);
      return [];
    }
  }

  // ── Computed summaries ──────────────────────────────────────────────────

  const totalDeaths = computed(() => deaths.value.length);

  const deathsByKiller = computed(() => {
    const counts = new Map<string, number>();
    for (const d of deaths.value) {
      counts.set(d.killer_name, (counts.get(d.killer_name) ?? 0) + 1);
    }
    return [...counts.entries()]
      .map(([name, count]) => ({ name, count }))
      .sort((a, b) => b.count - a.count);
  });

  const deathsByAbility = computed(() => {
    const counts = new Map<string, number>();
    for (const d of deaths.value) {
      counts.set(d.killing_ability, (counts.get(d.killing_ability) ?? 0) + 1);
    }
    return [...counts.entries()]
      .map(([name, count]) => ({ name, count }))
      .sort((a, b) => b.count - a.count);
  });

  const deathsByArea = computed(() => {
    const counts = new Map<string, number>();
    for (const d of deaths.value) {
      const area = d.area ?? "Unknown";
      counts.set(area, (counts.get(area) ?? 0) + 1);
    }
    return [...counts.entries()]
      .map(([name, count]) => ({ name, count }))
      .sort((a, b) => b.count - a.count);
  });

  const deathsByDamageType = computed(() => {
    const counts = new Map<string, number>();
    for (const d of deaths.value) {
      const dtype = d.damage_type ?? "Unknown";
      counts.set(dtype, (counts.get(dtype) ?? 0) + 1);
    }
    return [...counts.entries()]
      .map(([name, count]) => ({ name, count }))
      .sort((a, b) => b.count - a.count);
  });

  return {
    deaths,
    loaded,
    deathsGeneration,
    damageSourcesCache,
    loadDeaths,
    loadDamageSources,
    handleDeathEvent,
    totalDeaths,
    deathsByKiller,
    deathsByAbility,
    deathsByArea,
    deathsByDamageType,
  };
});

if (import.meta.hot) {
  import.meta.hot.accept(acceptHMRUpdate(useDeathStore, import.meta.hot))
}
