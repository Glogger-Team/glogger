import { defineStore, acceptHMRUpdate } from "pinia";
import { ref, computed, watch } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useSettingsStore } from "./settingsStore";
import { useGameDataStore } from "./gameDataStore";
import { useGameStateStore } from "./gameStateStore";
import { useCharacterStore } from "./characterStore";
import { useCoordinatorStore } from "./coordinatorStore";
import { useMarketStore } from "./marketStore";
import { useStallTrackerStore } from "./stallTrackerStore";
import { useSurveyTrackerStore } from "./surveyTrackerStore";
import { useFarmingStore } from "./farmingStore";
import { useDeathStore } from "./deathStore";
import { useResuscitateStore } from "./resuscitateStore";
import { useWatchwordAlertStore } from "./watchwordAlertStore";
import type { PlayerEvent } from "../types/playerEvents";
import type { WatchRuleTriggered } from "../types/database";

export type StartupPhase =
  | "splash"
  | "setup-path"
  | "setup-watchers"
  | "setup-character"
  | "select-character"
  | "loading"
  | "ready";

export interface GameDataPathValidation {
  path_exists: boolean;
  player_log_found: boolean;
  chat_logs_found: boolean;
  reports_found: boolean;
}

export interface DiscoveredCharacter {
  character_name: string;
  server_name: string;
  report_count: number;
  latest_report_time: string | null;
}

export interface UserCharacter {
  id: number;
  character_name: string;
  server_name: string;
  source: string;
  is_active: boolean;
  latest_report_time: string | null;
  last_login_time: string | null;
}

export interface StartupTask {
  label: string;
  status: "pending" | "running" | "done" | "error";
  detail?: string;
}

export const useStartupStore = defineStore("startup", () => {
  const phase = ref<StartupPhase>("splash");
  const pathValidation = ref<GameDataPathValidation | null>(null);
  const discoveredCharacters = ref<DiscoveredCharacter[]>([]);
  const userCharacters = ref<UserCharacter[]>([]);
  const serverList = ref<string[]>([]);
  const startupTasks = ref<StartupTask[]>([]);
  const error = ref<string | null>(null);
  // Buffer for startup-detail events that arrive before tasks are created
  const lastDetailByTask: Record<number, string> = {};

  const isSetupWizard = computed(() => phase.value.startsWith("setup-"));

  /** Fire-and-forget startup log to the backend */
  function log(message: string) {
    invoke("log_startup", { message });
  }

  const setupStepIndex = computed(() => {
    switch (phase.value) {
      case "setup-path": return 0;
      case "setup-watchers": return 1;
      case "setup-character": return 2;
      default: return -1;
    }
  });

  async function initialize() {
    const settingsStore = useSettingsStore();

    // Register startup detail listener early so we catch events from
    // the Rust CDN background task that starts during Tauri setup()
    await listen<{ task: number; detail: string }>("startup-detail", (event) => {
      const { task, detail } = event.payload;
      const t = startupTasks.value[task];
      if (t) {
        // Only update if the task is currently running
        if (t.status === "running") {
          updateTask(task, "running", detail);
        }
      } else {
        // Task not created yet — buffer the detail so it shows when task starts
        lastDetailByTask[task] = detail;
      }
    });

    // Load settings first
    await settingsStore.initialize();

    // Load server list
    try {
      serverList.value = await invoke<string[]>("get_server_list");
    } catch (e) {
      console.error("Failed to load server list:", e);
      serverList.value = ["Dreva"];
    }

    const settings = settingsStore.settings;

    if (!settings.setupCompleted || !settings.gameDataPath) {
      log("First-time setup — entering setup wizard");
      phase.value = "setup-path";
    } else if (settings.autoLoadLastCharacter && settings.activeCharacterName) {
      log(`Auto-loading character: ${settings.activeCharacterName} on ${settings.activeServerName}`);
      phase.value = "loading";
      await runStartupTasks();
    } else {
      log("Showing character selection");
      await loadUserCharacters();
      phase.value = "select-character";
    }
  }

  async function validatePath(path: string): Promise<GameDataPathValidation> {
    const result = await invoke<GameDataPathValidation>("validate_game_data_path", { path });
    pathValidation.value = result;
    return result;
  }

  async function scanForCharacters(path: string) {
    try {
      discoveredCharacters.value = await invoke<DiscoveredCharacter[]>(
        "scan_reports_for_characters",
        { path }
      );
    } catch (e) {
      console.error("Failed to scan for characters:", e);
      discoveredCharacters.value = [];
    }
  }

  async function loadUserCharacters() {
    try {
      userCharacters.value = await invoke<UserCharacter[]>("get_user_characters");
    } catch (e) {
      console.error("Failed to load user characters:", e);
      userCharacters.value = [];
    }
  }

  async function saveCharacter(characterName: string, serverName: string, source: string) {
    await invoke("save_user_character", {
      characterName,
      serverName,
      source,
    });
  }

  /** Set active character in backend + settings without running startup tasks.
   *  Used by the setup wizard where completeSetup() triggers startup separately. */
  async function setActiveCharacter(characterName: string, serverName: string) {
    log(`Character selected: ${characterName} on ${serverName}`);
    await invoke("set_active_character", { characterName, serverName });

    const settingsStore = useSettingsStore();
    settingsStore.settings.activeCharacterName = characterName;
    settingsStore.settings.activeServerName = serverName;
  }

  /** Set active character and immediately run the full startup sequence.
   *  Used by CharacterSelect (non-wizard flow). */
  async function selectCharacter(characterName: string, serverName: string) {
    await setActiveCharacter(characterName, serverName);
    phase.value = "loading";
    await runStartupTasks();
  }

  async function completeSetup() {
    log("Setup wizard completed");
    await invoke("complete_setup");

    const settingsStore = useSettingsStore();
    settingsStore.settings.setupCompleted = true;

    phase.value = "loading";
    await runStartupTasks();
  }

  // ── Helpers for task progress ─────────────────────────────────────────

  function updateTask(index: number, status: StartupTask["status"], detail?: string) {
    if (startupTasks.value[index]) {
      startupTasks.value[index].status = status;
      // Clear detail when task completes/errors unless explicitly provided
      if (status === "done" || status === "error") {
        startupTasks.value[index].detail = detail;
      } else if (detail !== undefined) {
        startupTasks.value[index].detail = detail;
      }
    }
  }

  // ── Main startup sequence ─────────────────────────────────────────────

  async function runStartupTasks() {
    const TASK_GAME_DATA = 0;
    const TASK_LOG_CATCHUP = 1;
    const TASK_CHARACTER = 2;
    const TASK_GAME_STATE = 3;

    startupTasks.value = [
      { label: "Loading game data", status: "running", detail: lastDetailByTask[0] },
      { label: "Catching up on logs", status: "pending" },
      { label: "Loading character data", status: "pending" },
      { label: "Preparing game state", status: "pending" },
    ];

    const settingsStore = useSettingsStore();
    const gameData = useGameDataStore();
    const gameState = useGameStateStore();
    const characterStore = useCharacterStore();
    const coordinator = useCoordinatorStore();
    const marketStore = useMarketStore();
    const stallTrackerStore = useStallTrackerStore();
    const surveyTrackerStore = useSurveyTrackerStore();
    const farmingStore = useFarmingStore();
    const deathStore = useDeathStore();
    const resuscitateStore = useResuscitateStore();
    const watchwordAlertStore = useWatchwordAlertStore();

    // ── Task 1: Wait for game data (CDN) ────────────────────────────────
    // The Rust backend is already loading this in a background task spawned
    // during setup(). We just need to wait for the event.
    try {
      if (gameData.status !== "ready") {
        await new Promise<void>((resolve, reject) => {
          const unwatch = watch(
            () => gameData.status,
            (s) => {
              if (s === "ready") { unwatch(); resolve(); }
              if (s === "error") { unwatch(); reject(new Error(gameData.errorMessage || "Failed to load game data")); }
            },
            { immediate: true }
          );
        });
      }
      log("Game data ready");
      updateTask(TASK_GAME_DATA, "done");
    } catch (e) {
      log(`Game data FAILED: ${e}`);
      updateTask(TASK_GAME_DATA, "error", String(e));
      error.value = `Game data failed to load: ${e}`;
      // Game data is critical — can't proceed without it
      return;
    }

    // ── Task 2: Start log watchers, catch up, then begin live polling ───
    // We start watchers and run an initial poll BEFORE loading character
    // data so the catch-up can resolve the real active character and seed
    // game state from the log history. Only after catch-up do we know
    // who is actually playing.
    log("Starting log watchers and catching up");
    updateTask(TASK_LOG_CATCHUP, "running", "Registering event listeners...");
    try {
      // Register event listeners BEFORE starting watchers so no events are missed
      await listen("skill-update", (event: any) => {
        gameState.handleSkillUpdate(event.payload);
        farmingStore.handleSkillUpdate(event.payload);
      });

      // Phase 5 survey tracker. Each handler refreshes only what could have
      // changed; handlers are async so refreshes don't block the event loop.
      await listen("survey-tracker-session-started", (event: any) => {
        surveyTrackerStore.handleSessionStarted(event.payload);
      });
      await listen("survey-tracker-session-ended", (event: any) => {
        surveyTrackerStore.handleSessionEnded(event.payload);
      });
      await listen("survey-tracker-use-recorded", (event: any) => {
        surveyTrackerStore.handleUseRecorded(event.payload);
      });
      await listen<number>("survey-tracker-use-completed", (event) => {
        surveyTrackerStore.handleUseCompleted(event.payload);
      });
      await listen("survey-tracker-multihit-opened", (event: any) => {
        surveyTrackerStore.handleMultihitOpened(event.payload);
      });
      await listen("survey-tracker-multihit-closed", (event: any) => {
        surveyTrackerStore.handleMultihitClosed(event.payload);
      });
      await listen<PlayerEvent[]>("player-events-batch", (event) => {
        for (const pe of event.payload) {
          farmingStore.handlePlayerEvent(pe);
        }
      });
      await listen("character-death", (event: any) => {
        deathStore.handleDeathEvent(event.payload);
      });
      await listen("enemy-killed", (event: any) => {
        farmingStore.handleEnemyKilled(event.payload);
      });
      await listen("character-resuscitated", (event: any) => {
        resuscitateStore.handleResuscitateEvent(event.payload);
      });
      await listen<WatchRuleTriggered>("watch-rule-triggered", (event) => {
        watchwordAlertStore.handleWatchRuleTriggered(event.payload);
      });

      // Start log watchers if enabled
      updateTask(TASK_LOG_CATCHUP, "running", "Starting log watchers...");
      if (settingsStore.settings.autoTailPlayerLog && settingsStore.settings.gameDataPath) {
        await coordinator.startPlayerTailing();
      }
      if (settingsStore.settings.autoTailChat && settingsStore.settings.gameDataPath) {
        await coordinator.startChatTailing();
      }

      // Run one synchronous poll to process all historical log content.
      // This blocks until the catch-up is complete, so after this call
      // the active character and game state are fully resolved.
      updateTask(TASK_LOG_CATCHUP, "running", "Processing log history...");
      await invoke('poll_watchers');

      // Now start background polling on the Rust side for live updates
      updateTask(TASK_LOG_CATCHUP, "running", "Starting live polling...");
      await coordinator.startPolling();

      updateTask(TASK_LOG_CATCHUP, "done");
    } catch (e) {
      console.warn("Log watcher startup error:", e);
      updateTask(TASK_LOG_CATCHUP, "done", "Some watchers failed to start");
      // Start polling anyway so live tailing still works
      await coordinator.startPolling();
    }

    // Refresh settings from backend — the catch-up may have updated
    // the active character/server in the backend settings. Forced: the splash
    // phase already loaded them, and the catch-up's per-session attribution
    // writes the log's newest identity at the end of the poll.
    await settingsStore.initialize(true);

    // ── Task 3: Load character data from reports ────────────────────────
    log("Loading character data from reports");
    updateTask(TASK_CHARACTER, "running", "Importing reports...");
    try {
      // Auto-import latest character + inventory reports and seed game state
      await characterStore.initForActiveCharacter();
      log("Character data loaded");
      updateTask(TASK_CHARACTER, "done");
    } catch (e) {
      // Non-fatal — character data from reports is supplementary
      log(`Character data partial: ${e}`);
      console.warn("Character init had errors:", e);
      updateTask(TASK_CHARACTER, "done", "Partial — no reports found");
    }

    // ── Task 4: Load full game state from DB ────────────────────────────
    log("Loading game state from database");
    updateTask(TASK_GAME_STATE, "running", "Loading state domains...");
    try {
      // Load all game state domains (skills, inventory, favor, recipes, etc.)
      await gameState.loadAll();
      // Also load storage vault CDN metadata
      updateTask(TASK_GAME_STATE, "running", "Loading storage vaults...");
      await gameState.loadStorageVaults();
      // Load market values
      updateTask(TASK_GAME_STATE, "running", "Loading market data...");
      await marketStore.loadAll();
      // Load stall tracker stats + filter options for the active character
      updateTask(TASK_GAME_STATE, "running", "Loading stall tracker...");
      await Promise.all([
        stallTrackerStore.loadStats(),
        stallTrackerStore.loadFilterOptions(),
      ]);
      log("Game state ready");
      updateTask(TASK_GAME_STATE, "done");
    } catch (e) {
      log(`Game state load error: ${e}`);
      console.error("Game state load error:", e);
      updateTask(TASK_GAME_STATE, "error", String(e));
      // Continue anyway — some state is better than no state
    }

    // Start report folder watching (after character is loaded)
    characterStore.startReportWatching();

    // ── All critical tasks complete — app is ready ──────────────────────
    // Enable live character-login handling now that startup is finished.
    gameState.startupComplete = true;

    // If persistent farming logging is enabled, start a session now. We wait until after the
    // historical catch-up poll has drained so only live events are attributed to the session
    // (auto-starting earlier would fold the entire log history into it).
    if (settingsStore.settings.autoStartFarmingSessions && !farmingStore.sessionActive) {
      farmingStore.startSession("Auto Farming Session");
      log("Auto-started farming session (logging enabled)");
    }

    log("App is interactive");
    phase.value = "ready";
  }

  function goToPhase(newPhase: StartupPhase) {
    if (newPhase.startsWith("setup-")) {
      const stepNames: Record<string, string> = {
        "setup-path": "Game folder",
        "setup-watchers": "Log watchers",
        "setup-character": "Character selection",
      };
      log(`Setup wizard step: ${stepNames[newPhase] || newPhase}`);
    }
    phase.value = newPhase;
  }

  return {
    phase,
    pathValidation,
    discoveredCharacters,
    userCharacters,
    serverList,
    startupTasks,
    error,
    isSetupWizard,
    setupStepIndex,
    initialize,
    validatePath,
    scanForCharacters,
    loadUserCharacters,
    saveCharacter,
    setActiveCharacter,
    selectCharacter,
    completeSetup,
    runStartupTasks,
    goToPhase,
  };
});

if (import.meta.hot) {
  import.meta.hot.accept(acceptHMRUpdate(useStartupStore, import.meta.hot))
}
