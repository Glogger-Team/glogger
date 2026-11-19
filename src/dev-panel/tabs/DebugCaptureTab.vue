<template>
  <div class="space-y-6">
    <div class="flex items-center justify-between">
      <h3 class="m-0 text-text-primary">Debug Capture</h3>
      <span
        v-if="status?.active"
        class="text-xs px-2 py-0.5 rounded bg-red-900/40 text-red-400 animate-pulse">
        RECORDING
      </span>
      <span
        v-else-if="status?.pending_save"
        class="text-xs px-2 py-0.5 rounded bg-yellow-900/40 text-yellow-400">
        READY TO SAVE
      </span>
    </div>
    <p class="text-text-muted text-xs">
      Capture raw Player.log and Chat.log lines along with game state snapshots
      for debugging. Lines are streamed to a temp file so long captures are safe.
    </p>

    <!-- Controls -->
    <section class="border border-border-default rounded p-4 space-y-4">
      <h4 class="text-text-secondary text-sm mb-3 mt-0">Capture Controls</h4>

      <!-- Idle state: start button -->
      <div v-if="!status?.active && !status?.pending_save" class="space-y-3">
        <button
          class="btn btn-primary text-xs"
          :disabled="starting"
          @click="startCapture">
          {{ starting ? 'Starting...' : 'Start Capture' }}
        </button>
        <p class="text-text-muted text-xs">
          Starts buffering all raw log lines to disk and takes a game state snapshot.
        </p>
      </div>

      <!-- Recording state: live stats + stop button -->
      <div v-else-if="status?.active" class="space-y-3">
        <div class="grid grid-cols-3 gap-3 text-xs">
          <div class="bg-surface-elevated rounded p-2">
            <div class="text-text-muted mb-1">Total Lines</div>
            <div class="text-text-primary text-sm">{{ (status.player_line_count + status.chat_line_count).toLocaleString() }}</div>
          </div>
          <div class="bg-surface-elevated rounded p-2">
            <div class="text-text-muted mb-1">Player.log</div>
            <div class="text-accent-gold text-sm">{{ status.player_line_count.toLocaleString() }}</div>
          </div>
          <div class="bg-surface-elevated rounded p-2">
            <div class="text-text-muted mb-1">Chat.log</div>
            <div class="text-accent-gold text-sm">{{ status.chat_line_count.toLocaleString() }}</div>
          </div>
        </div>

        <div class="text-text-muted text-xs">
          Started: <span class="text-text-secondary">{{ formatStartTime }}</span>
        </div>

        <div class="flex gap-2">
          <button
            class="btn btn-primary text-xs"
            :disabled="stopping"
            @click="stopCapture">
            {{ stopping ? 'Stopping...' : 'Stop Recording' }}
          </button>
          <button
            class="btn btn-secondary text-xs"
            :disabled="stopping"
            @click="discardCapture">
            Discard
          </button>
        </div>
      </div>

      <!-- Pending save state: review, edit notes, choose save mode -->
      <div v-else-if="status?.pending_save" class="space-y-3">
        <div class="grid grid-cols-3 gap-3 text-xs">
          <div class="bg-surface-elevated rounded p-2">
            <div class="text-text-muted mb-1">Total Lines</div>
            <div class="text-text-primary text-sm">{{ status.line_count.toLocaleString() }}</div>
          </div>
          <div class="bg-surface-elevated rounded p-2">
            <div class="text-text-muted mb-1">Player.log</div>
            <div class="text-accent-gold text-sm">{{ status.player_line_count.toLocaleString() }}</div>
          </div>
          <div class="bg-surface-elevated rounded p-2">
            <div class="text-text-muted mb-1">Chat.log</div>
            <div class="text-accent-gold text-sm">{{ status.chat_line_count.toLocaleString() }}</div>
          </div>
        </div>

        <div>
          <label class="text-xs text-text-muted block mb-1">Notes (what were you doing?)</label>
          <textarea
            v-model="notes"
            rows="3"
            placeholder="Describe what you were doing, what bug you're trying to capture, etc."
            class="w-full bg-surface-elevated border border-border-default rounded px-3 py-2 text-sm text-text-primary resize-y"
          />
        </div>

        <div class="flex gap-2 flex-wrap">
          <button
            class="btn btn-primary text-xs"
            :disabled="saving"
            @click="saveCapture('normal')">
            {{ saving ? 'Saving...' : 'Save (Normal)' }}
          </button>
          <button
            class="btn btn-secondary text-xs"
            :disabled="saving"
            @click="saveCapture('full')">
            Save (Full)
          </button>
          <button
            class="btn btn-secondary text-xs"
            :disabled="saving"
            @click="discardCapture">
            Discard
          </button>
        </div>
        <p class="text-text-muted text-xs mt-1">
          <strong>Normal</strong> filters engine noise (asset loading, animations, sounds) for easier analysis.
          <strong>Full</strong> keeps every raw line.
        </p>
      </div>
    </section>

    <!-- Last capture result -->
    <section v-if="lastResult" class="border border-border-default rounded p-4">
      <h4 class="text-text-secondary text-sm mb-3 mt-0">Last Capture</h4>
      <div class="text-xs space-y-1">
        <div v-if="lastResult.success" class="text-value-positive">
          Saved {{ lastResult.lineCount?.toLocaleString() }} lines to: {{ lastResult.path }}
        </div>
        <div v-else class="text-value-negative">
          {{ lastResult.error }}
        </div>
      </div>
    </section>

    <!-- Capture Replay -->
    <section class="border border-border-default rounded p-4 space-y-4">
      <h4 class="text-text-secondary text-sm mb-3 mt-0">Replay Capture File</h4>
      <p class="text-text-muted text-xs">
        Replay a saved capture file through the chat parser and inspect detected kills.
        No database interaction — just validates what the parser would emit.
      </p>

      <div class="flex gap-2 items-center">
        <button
          class="btn btn-secondary text-xs"
          :disabled="replaying"
          @click="pickAndReplay">
          {{ replaying ? 'Replaying...' : 'Open Capture File...' }}
        </button>
        <span v-if="replayError" class="text-value-negative text-xs">{{ replayError }}</span>
      </div>

      <template v-if="replayResult">
        <div class="grid grid-cols-2 gap-3 text-xs">
          <div class="bg-surface-elevated rounded p-2">
            <div class="text-text-muted mb-1">Lines Replayed</div>
            <div class="text-text-primary text-sm">{{ replayResult.chat_lines.toLocaleString() }}</div>
          </div>
          <div class="bg-surface-elevated rounded p-2">
            <div class="text-text-muted mb-1">Kills Detected</div>
            <div class="text-[#e87e7e] text-sm">{{ replayResult.kills.length }}</div>
          </div>
        </div>

        <!-- Kills -->
        <div v-if="replayResult.kills.length" class="space-y-1">
          <div class="text-[0.65rem] uppercase tracking-widest text-[#e87e7e] font-bold">Kills</div>
          <div
            v-for="(kill, i) in replayResult.kills"
            :key="i"
            class="flex gap-3 text-xs px-2 py-1 bg-surface-elevated rounded">
            <span class="text-text-dim font-mono shrink-0">{{ kill.timestamp.slice(11, 19) }}</span>
            <span class="text-entity-enemy">{{ kill.enemy_name }}</span>
            <span class="text-text-dim">#{{ kill.enemy_entity_id }}</span>
            <span class="text-text-muted ml-auto">{{ kill.killing_ability }}</span>
          </div>
        </div>

        <div v-if="replayResult.kills.length === 0" class="text-text-dim text-xs italic">
          No kills found in this capture.
        </div>
      </template>
    </section>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { save, open } from "@tauri-apps/plugin-dialog";

interface CaptureStatus {
  active: boolean;
  pending_save: boolean;
  started_at: string | null;
  line_count: number;
  player_line_count: number;
  chat_line_count: number;
}

interface CaptureResult {
  line_count: number;
  player_line_count: number;
  chat_line_count: number;
}

const status = ref<CaptureStatus | null>(null);
const notes = ref("");
const starting = ref(false);
const stopping = ref(false);
const saving = ref(false);
const lastResult = ref<{
  success: boolean;
  path?: string;
  lineCount?: number;
  error?: string;
} | null>(null);

let pollInterval: ReturnType<typeof setInterval> | null = null;

const formatStartTime = computed(() => {
  if (!status.value?.started_at) return "\u2014";
  try {
    return new Date(status.value.started_at).toLocaleTimeString();
  } catch {
    return status.value.started_at;
  }
});

async function refreshStatus() {
  try {
    status.value = await invoke<CaptureStatus>("debug_capture_status");
  } catch (e) {
    console.error("Failed to get capture status:", e);
  }
}

async function startCapture() {
  starting.value = true;
  try {
    await invoke("debug_capture_start");
    notes.value = "";
    lastResult.value = null;
    await refreshStatus();
  } catch (e) {
    console.error("Failed to start capture:", e);
    lastResult.value = { success: false, error: String(e) };
  } finally {
    starting.value = false;
  }
}

async function stopCapture() {
  stopping.value = true;
  try {
    await invoke<CaptureResult>("debug_capture_stop");
    await refreshStatus();
  } catch (e) {
    console.error("Failed to stop capture:", e);
    lastResult.value = { success: false, error: String(e) };
  } finally {
    stopping.value = false;
  }
}

async function saveCapture(filterMode: "normal" | "full") {
  saving.value = true;
  try {
    const filePath = await save({
      filters: [{ name: "JSON", extensions: ["json"] }],
      defaultPath: `glogger-capture-${new Date().toISOString().slice(0, 19).replace(/:/g, "-")}.json`,
    });

    if (!filePath) {
      saving.value = false;
      return;
    }

    const result = await invoke<CaptureResult>("debug_capture_save", {
      notes: notes.value,
      filterMode,
      outputPath: filePath,
    });

    lastResult.value = {
      success: true,
      path: filePath,
      lineCount: result.line_count,
    };
    await refreshStatus();
  } catch (e) {
    console.error("Failed to save capture:", e);
    lastResult.value = { success: false, error: String(e) };
    await refreshStatus();
  } finally {
    saving.value = false;
  }
}

async function discardCapture() {
  try {
    await invoke("debug_capture_discard");
    lastResult.value = null;
    await refreshStatus();
  } catch (e) {
    console.error("Failed to discard capture:", e);
  }
}

// ── Capture Replay ──────────────────────────────────────────────────────

// Loot attribution replay was retired with the Player.log pipeline: the
// backend still returns `player_lines: 0` and an empty `loot_events`, so the
// panel only renders kills. Fields kept in the type for shape parity.
interface ReplayLootEvent {
  timestamp: string;
  item_name: string | null;
  item_type_id: number | null;
  quantity: number;
  corpse_name: string | null;
  corpse_entity_id: number | null;
  instance_id: number;
  resolution: string;
}

interface ReplayKillEvent {
  timestamp: string;
  enemy_name: string;
  enemy_entity_id: string;
  killing_ability: string;
}

interface CaptureReplayResult {
  total_lines: number;
  player_lines: number;
  chat_lines: number;
  loot_events: ReplayLootEvent[];
  kills: ReplayKillEvent[];
}

const replaying = ref(false);
const replayResult = ref<CaptureReplayResult | null>(null);
const replayError = ref<string | null>(null);

async function pickAndReplay() {
  replaying.value = true;
  replayError.value = null;
  replayResult.value = null;
  try {
    const filePath = await open({
      filters: [{ name: "JSON Capture", extensions: ["json"] }],
      multiple: false,
    });
    if (!filePath) {
      replaying.value = false;
      return;
    }
    replayResult.value = await invoke<CaptureReplayResult>("replay_capture_file", {
      path: filePath,
    });
  } catch (e) {
    replayError.value = String(e);
  } finally {
    replaying.value = false;
  }
}

onMounted(() => {
  refreshStatus();
  pollInterval = setInterval(refreshStatus, 2000);
});

onUnmounted(() => {
  if (pollInterval) {
    clearInterval(pollInterval);
    pollInterval = null;
  }
});
</script>
