<template>
  <div>
    <div class="settings-section">
      <h3>Developer Mode</h3>

      <div class="mb-4">
        <label class="flex items-center gap-2 cursor-pointer text-text-primary">
          <input
            type="checkbox"
            v-model="devMode"
            @change="saveDevMode"
            class="size-5 cursor-pointer" />
          <span>Enable Developer Mode</span>
        </label>
        <p class="mt-2 text-text-muted text-xs leading-relaxed">
          Reveals beta features, experimental tools, and additional debugging information
          throughout the app. Off by default.
        </p>
      </div>
    </div>

    <div class="settings-section">
      <h3>Data Browser</h3>

      <div class="mb-4">
        <label class="flex items-center gap-2 cursor-pointer text-text-primary">
          <input
            type="checkbox"
            v-model="showRawJson"
            @change="saveShowRawJson"
            class="size-5 cursor-pointer" />
          <span>Show Raw JSON in Data Browser</span>
        </label>
        <p class="mt-2 text-text-muted text-xs leading-relaxed">
          Displays the raw JSON data at the bottom of each entity's detail view in the
          Data Browser. Useful for debugging or exploring the full data structure.
        </p>
      </div>

      <div class="mb-4">
        <label class="flex items-center gap-2 cursor-pointer text-text-primary">
          <input
            type="checkbox"
            v-model="showUnobtainable"
            @change="saveShowUnobtainable"
            class="size-5 cursor-pointer" />
          <span>Show Unobtainable Items</span>
        </label>
        <p class="mt-2 text-text-muted text-xs leading-relaxed">
          Include items and abilities tagged as unobtainable (Lint_NotObtainable,
          Lint_NotLearnable) in search results, data browsers, and the build planner.
          These are typically developer/test entries that cannot be acquired in-game.
        </p>
      </div>

      <div class="mb-4">
        <label class="text-sm text-text-secondary mb-1 block">History size</label>
        <input
          type="number"
          v-model.number="historyMax"
          @change="saveHistoryMax"
          min="5"
          max="200"
          class="bg-surface-elevated border border-border-default rounded px-3 py-1.5 text-sm text-text-primary w-24" />
        <p class="mt-1 text-text-muted text-xs">
          Maximum number of recently viewed entries to keep in the Data Browser history (5–200).
        </p>
      </div>
    </div>

    <div class="settings-section">
      <h3>Character Report Watching</h3>

      <div class="mb-4">
        <label class="flex items-center gap-2 cursor-pointer text-text-primary">
          <input
            type="checkbox"
            v-model="autoWatchReports"
            @change="saveReportWatchSettings"
            class="size-5 cursor-pointer" />
          <span>Auto-ingest new character reports</span>
        </label>
        <p class="mt-2 text-text-muted text-xs leading-relaxed">
          Automatically imports new character exports. Reports are detected instantly via chat log
          when the game announces a save. Polling acts as a fallback for exports that happened
          while the app was closed.
        </p>
      </div>

      <div v-if="autoWatchReports" class="mb-4">
        <label class="text-sm text-text-secondary mb-1 block">Check interval (seconds)</label>
        <input
          type="number"
          v-model.number="reportWatchInterval"
          @change="saveReportWatchSettings"
          min="5"
          max="300"
          class="bg-surface-elevated border border-border-default rounded px-3 py-1.5 text-sm text-text-primary w-24" />
        <p class="mt-1 text-text-muted text-xs">
          Fallback polling interval (5–300 seconds). Chat detection handles instant imports while playing.
        </p>
      </div>
    </div>

    <template v-if="devMode">
    <div class="settings-section">
      <h3>Dual-Log Replay</h3>

      <p class="mb-3 text-text-muted text-xs leading-relaxed">
        Replay a Chat.log through the live dispatch pipeline for backfills and
        debugging. Progress and parsed status events stream back while it runs.
      </p>

      <div class="flex gap-2 mb-3">
        <div class="flex-1">
          <label class="text-xs text-text-secondary mb-1 block">Player.log</label>
          <button
            @click="pickPlayerLogForReplay"
            :disabled="replaying"
            class="btn btn-secondary text-xs w-full truncate">
            {{ replayPlayerPath ? replayPlayerPath.split(/[\\/]/).pop() : "Select Player.log..." }}
          </button>
        </div>
        <div class="flex-1">
          <label class="text-xs text-text-secondary mb-1 block">Chat.log</label>
          <button
            @click="pickChatLogForReplay"
            :disabled="replaying"
            class="btn btn-secondary text-xs w-full truncate">
            {{ replayChatPath ? replayChatPath.split(/[\\/]/).pop() : "Select Chat.log..." }}
          </button>
        </div>
      </div>

      <button
        @click="startReplay"
        :disabled="replaying || !replayPlayerPath || !replayChatPath"
        class="btn btn-primary mb-3">
        {{ replaying ? "Replaying..." : "Start Dual Replay" }}
      </button>

      <div v-if="replayProgress" class="mb-2 text-xs text-text-secondary">
        <div v-if="replayProgress.phase === 'processing'" class="mb-1">
          <div class="bg-surface-elevated rounded-full h-2 overflow-hidden">
            <div
              class="bg-accent-primary h-full transition-all duration-200"
              :style="{ width: `${Math.round((replayProgress.current / replayProgress.total) * 100)}%` }">
            </div>
          </div>
        </div>
        {{ replayProgress.detail }}
      </div>

      <div v-if="replayResultText" class="success-box">{{ replayResultText }}</div>
      <div v-if="replayError" class="error-box">{{ replayError }}</div>
    </div>
    </template>
  </div>
</template>

<script setup lang="ts">
import { ref, onUnmounted } from "vue";
import { open } from "@tauri-apps/plugin-dialog";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { useSettingsStore } from "../../stores/settingsStore";
import { useCharacterStore } from "../../stores/characterStore";
import { useDataBrowserStore } from "../../stores/dataBrowserStore";

const settingsStore = useSettingsStore();
const characterStore = useCharacterStore();
const dataBrowserStore = useDataBrowserStore();

// Dev mode
const devMode = ref(settingsStore.settings.devModeEnabled);

// Data Browser raw JSON
const showRawJson = ref(settingsStore.settings.showRawJsonInDataBrowser);

function saveShowRawJson() {
  settingsStore.updateSettings({ showRawJsonInDataBrowser: showRawJson.value });
}

// Unobtainable items visibility
const showUnobtainable = ref(settingsStore.settings.showUnobtainableItems);

// Data Browser history size
dataBrowserStore.load();
const historyMax = ref(dataBrowserStore.maxHistory);

function saveHistoryMax() {
  const clamped = Math.max(5, Math.min(200, historyMax.value));
  historyMax.value = clamped;
  dataBrowserStore.setMaxHistory(clamped);
}

function saveShowUnobtainable() {
  settingsStore.updateSettings({ showUnobtainableItems: showUnobtainable.value });
}

function saveDevMode() {
  settingsStore.updateSettings({ devModeEnabled: devMode.value });
}

// Report watching
const autoWatchReports = ref(settingsStore.settings.autoWatchReports);
const reportWatchInterval = ref(settingsStore.settings.reportWatchIntervalSeconds);

function saveReportWatchSettings() {
  const interval = Math.max(5, Math.min(300, reportWatchInterval.value));
  reportWatchInterval.value = interval;
  settingsStore.updateSettings({
    autoWatchReports: autoWatchReports.value,
    reportWatchIntervalSeconds: interval,
  });
  // Restart watching with new settings
  characterStore.stopReportWatching();
  if (autoWatchReports.value) {
    characterStore.startReportWatching();
  }
}

// ── Dual-Log Replay ──────────────────────────────────────────────────────────

interface ReplayProgressEvent {
  phase: string;
  current: number;
  total: number;
  detail: string;
}

const replayPlayerPath = ref<string | null>(null);
const replayChatPath = ref<string | null>(null);
const replaying = ref(false);
const replayProgress = ref<ReplayProgressEvent | null>(null);
const replayResultText = ref<string | null>(null);
const replayError = ref<string | null>(null);

let unlistenProgress: UnlistenFn | null = null;

async function pickPlayerLogForReplay() {
  const selected = await open({
    multiple: false,
    defaultPath: settingsStore.settings.logFilePath || settingsStore.settings.gameDataPath || undefined,
    filters: [{ name: "Log Files", extensions: ["log", "txt"] }],
  });
  if (selected) {
    replayPlayerPath.value = selected;
  }
}

async function pickChatLogForReplay() {
  const selected = await open({
    multiple: false,
    defaultPath: (settingsStore.settings.gameDataPath ? settingsStore.settings.gameDataPath + "/ChatLogs" : undefined),
    filters: [{ name: "Log Files", extensions: ["log", "txt"] }],
  });
  if (selected) {
    replayChatPath.value = selected;
  }
}

async function startReplay() {
  if (!replayPlayerPath.value || !replayChatPath.value) return;

  replaying.value = true;
  replayProgress.value = null;
  replayResultText.value = null;
  replayError.value = null;

  // Listen for progress events
  unlistenProgress = await listen<ReplayProgressEvent>("replay-progress", (event) => {
    replayProgress.value = event.payload;
  });

  try {
    const result = await invoke<{
      player_lines_processed: number;
      chat_messages_processed: number;
      player_events_emitted: number;
      survey_events_emitted: number;
      chat_status_events_emitted: number;
      loot_corrections_applied: number;
    }>("replay_dual_logs", {
      playerLogPath: replayPlayerPath.value,
      chatLogPath: replayChatPath.value,
    });

    replayResultText.value = [
      `Player: ${result.player_events_emitted} events from ${result.player_lines_processed} lines`,
      `Chat: ${result.chat_messages_processed} messages, ${result.chat_status_events_emitted} status events`,
      `Survey: ${result.survey_events_emitted} events`,
      `Loot corrections: ${result.loot_corrections_applied}`,
    ].join(" · ");
  } catch (e: any) {
    replayError.value = e.toString();
  } finally {
    replaying.value = false;
    if (unlistenProgress) {
      unlistenProgress();
      unlistenProgress = null;
    }
  }
}

onUnmounted(() => {
  if (unlistenProgress) {
    unlistenProgress();
  }
});
</script>
