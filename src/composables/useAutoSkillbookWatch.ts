import { computed, ref, watch } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { useSettingsStore } from "../stores/settingsStore";
import { useGameDataStore } from "../stores/gameDataStore";
import { useViewPrefs } from "./useViewPrefs";
import type {
  CharacterSnapshotSummary,
  SnapshotSkillAbility,
  WatchRule,
} from "../types/database";
import type {
  GameStateInventoryItem,
  GameStateSkill,
  GameStateStorageItem,
} from "../types/gameState";
import type { SkillInfo } from "../types/gameData";

interface AutoWatchPrefs {
  futureSkills: string[];
  [key: string]: unknown;
}



/**
 * Auto-watch skillbooks (TODO #17) — composition over the landed pieces.
 *
 * Two modes, both generating an in-memory session set held backend-side
 * (never persisted — manual `watchRules` are the only persisted rules):
 * (1) books for currently-trained skills (live game-state skills),
 * (2) "future skill" mode: the player-selected skill list (persisted in
 * viewPreferences) is watched even when the level is too low to use the book.
 *
 * "Learnable" filtering (known abilities + owned books) runs in the backend
 * `suggest_auto_skillbook_rules` command; this composable only gathers its
 * inputs from the already-scoped commands.
 */
export function useAutoSkillbookWatch() {
  const settingsStore = useSettingsStore();
  const gameData = useGameDataStore();
  const { prefs, update } = useViewPrefs<AutoWatchPrefs>("watchwords-auto", {
    futureSkills: [],
  });

  const characterName = computed(() => settingsStore.settings.activeCharacterName);
  const serverName = computed(() => settingsStore.settings.activeServerName);

  function requireIdentity(): { characterName: string; serverName: string } | null {
    const character = characterName.value;
    const server = serverName.value;
    if (!character || !server) return null;
    return { characterName: character, serverName: server };
  }

  /** Mode (1) input: live trained-skill names for the active character+server. */
  async function loadTrainedSkills(): Promise<string[]> {
    const identity = requireIdentity();
    if (!identity) return [];
    const skills = await invoke<GameStateSkill[]>("get_game_state_skills", identity);
    return skills.map((s) => s.skill_name);
  }

  /** Known ability keys from the latest character report snapshot. Empty when no report exists. */
  async function loadKnownAbilities(): Promise<string[]> {
    const identity = requireIdentity();
    if (!identity) return [];
    const snapshots = await invoke<CharacterSnapshotSummary[]>("get_character_snapshots", {
      characterName: identity.characterName,
      serverName: identity.serverName,
    });
    if (snapshots.length === 0) return [];
    // get_character_snapshots returns timestamp DESC, so index 0 is the
    // latest — never max id (a never-before-seen older report can insert
    // with a fresh id and outrank the true latest).
    const abilities = await invoke<SnapshotSkillAbility[]>("get_snapshot_skill_abilities", {
      snapshotId: snapshots[0].id,
    });
    return abilities.map((a) => a.ability_key);
  }

  /** Owned book names from live inventory + storage (any case; backend matches case-insensitively). */
  async function loadOwnedNames(): Promise<string[]> {
    const identity = requireIdentity();
    if (!identity) return [];
    const [inventory, storage] = await Promise.all([
      invoke<GameStateInventoryItem[]>("get_game_state_inventory", identity),
      invoke<GameStateStorageItem[]>("get_game_state_storage", identity),
    ]);
    return [
      ...inventory.map((i) => i.item_name),
      ...storage.map((s) => s.item_name),
    ];
  }

  /** All CDN skills for the mode-(2) picker (display names). */
  async function loadAllSkillNames(): Promise<string[]> {
    const skills: SkillInfo[] = await gameData.getAllSkills();
    return skills.map((s) => s.name).sort((a, b) => a.localeCompare(b));
  }

  /**
   * Suggest session rules for `skills`: gathers known/owned/manual inputs,
   * then delegates mapping + filtering + rule-building to the backend.
   * Throws when no character+server is active. Nothing is persisted.
   */
  async function suggestFor(skills: string[]): Promise<WatchRule[]> {
    if (!requireIdentity()) {
      throw new Error("Select a character and server first.");
    }
    if (skills.length === 0) return [];
    const s = settingsStore.settings;
    const [knownAbilities, ownedNames] = await Promise.all([
      loadKnownAbilities(),
      loadOwnedNames(),
    ]);
    // Dedupe against ALL persisted (manual) rules: same book never double-fires.
    const existingNames = s.watchRules.flatMap((r) => r.conditions.map((c) => c.value));
    return invoke<WatchRule[]>("suggest_auto_skillbook_rules", {
      skills,
      blockedSkills: s.autoSkillbookBlockedSkills,
      knownAbilities,
      ownedNames,
      dismissedNames: s.autoSkillbookDismissals,
      existingNames,
      enabled: s.autoSkillbookWatchEnabled,
      nextId: Date.now(),
    });
  }

  // ── Ephemeral session set ─────────────────────────────────────────────
  // Held backend-side in AutoWatchState (never persisted). Rebuilt at
  // session load and on character switch via initAutoWatch(), plus on any
  // control/input change below (debounced).

  const ephemeralRules = ref<WatchRule[]>([]);
  const rebuilding = ref(false);
  let rebuildTimer: ReturnType<typeof setTimeout> | undefined = undefined;
  let watchersInstalled = false;

  async function refreshEphemeral(): Promise<void> {
    try {
      ephemeralRules.value = await invoke<WatchRule[]>("get_ephemeral_auto_rules");
    } catch (e) {
      console.error("[auto-watch] Failed to read session set:", e);
    }
  }

  async function setSessionRules(rules: WatchRule[]): Promise<void> {
    await invoke("set_ephemeral_auto_rules", { rules });
    ephemeralRules.value = [...rules];
  }

  async function rebuildAutoWatch(): Promise<void> {
    // Re-entrancy guard (B P2 carries over to the generation path).
    if (rebuilding.value) return;
    rebuilding.value = true;
    try {
      const s = settingsStore.settings;
      if (!s.autoSkillbookWatchEnabled || !requireIdentity()) {
        // Toggle OFF (or no character): zero generation, manual unaffected.
        await setSessionRules([]);
        return;
      }
      const trained = await loadTrainedSkills();
      const selected: string[] = [...prefs.value.futureSkills];
      const rules = await suggestFor([...trained, ...selected]);
      await setSessionRules(rules);
    } catch (e) {
      console.error("[auto-watch] Rebuild failed:", e);
    } finally {
      rebuilding.value = false;
    }
  }

  function scheduleRebuild(): void {
    clearTimeout(rebuildTimer);
    rebuildTimer = setTimeout(() => {
      void rebuildAutoWatch();
    }, 750);
  }

  /** Install once (called from startup after game state is ready): initial
   * rebuild plus watchers for character/server, toggle, block-list,
   * dismissals, future-skill picks, and manual-rule edits (dedupe). */
  function initAutoWatch(): void {
    if (watchersInstalled) return;
    watchersInstalled = true;
    void rebuildAutoWatch();
    watch(
      () => [
        settingsStore.settings.activeCharacterName,
        settingsStore.settings.activeServerName,
        settingsStore.settings.autoSkillbookWatchEnabled,
        settingsStore.settings.autoSkillbookBlockedSkills,
        settingsStore.settings.autoSkillbookDismissals,
        settingsStore.settings.watchRules,
        prefs.value.futureSkills,
      ],
      () => scheduleRebuild(),
      { deep: true },
    );
  }

  // ── Persisted controls (the only persisted pieces besides manual rules)

  const autoWatchEnabled = computed(() => settingsStore.settings.autoSkillbookWatchEnabled);

  async function setAutoWatchEnabled(on: boolean): Promise<void> {
    await settingsStore.updateSettings({ autoSkillbookWatchEnabled: on });
  }

  const blockedSkills = computed(() => settingsStore.settings.autoSkillbookBlockedSkills);

  async function setBlockedSkills(skills: string[]): Promise<void> {
    await settingsStore.updateSettings({ autoSkillbookBlockedSkills: skills });
  }

  const dismissals = computed(() => settingsStore.settings.autoSkillbookDismissals);

  async function dismissBook(name: string): Promise<void> {
    const needle = name.toLowerCase();
    const cur = settingsStore.settings.autoSkillbookDismissals;
    if (cur.some((n) => n.toLowerCase() === needle)) return;
    await settingsStore.updateSettings({ autoSkillbookDismissals: [...cur, name] });
  }

  async function undismissBook(name: string): Promise<void> {
    const needle = name.toLowerCase();
    await settingsStore.updateSettings({
      autoSkillbookDismissals: settingsStore.settings.autoSkillbookDismissals.filter(
        (n) => n.toLowerCase() !== needle,
      ),
    });
  }

  function setFutureSkills(skills: string[]): void {
    update({ futureSkills: skills });
  }

  return {
    prefs,
    loadTrainedSkills,
    loadAllSkillNames,
    suggestFor,
    setFutureSkills,
    ephemeralRules,
    rebuilding,
    refreshEphemeral,
    rebuildAutoWatch,
    initAutoWatch,
    autoWatchEnabled,
    setAutoWatchEnabled,
    blockedSkills,
    setBlockedSkills,
    dismissals,
    dismissBook,
    undismissBook,
  };
}
