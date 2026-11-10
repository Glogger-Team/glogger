<template>
  <div>
    <div class="settings-section">
      <h3>Manual Import</h3>

      <div class="mb-4">
        <button
          @click="selectAndParseFile"
          :disabled="props.parsing || importingFile || importingFolder"
          class="btn btn-secondary">
          {{ importingFile ? "Importing…" : props.parsing ? "Parsing..." : "Parse a Chat Log File" }}
        </button>

        <button
          @click="parseFolder"
          :disabled="props.parsing || importingFile || importingFolder"
          class="btn btn-secondary ml-2">
          {{ importingFolder ? "Importing…" : props.parsing ? "Parsing..." : "Parse Entire Folder" }}
        </button>

        <p class="mt-2 text-text-muted text-xs leading-relaxed">
          Import chat messages from individual log files or an entire ChatLogs folder.
        </p>

        <div v-if="importingFolder" class="mt-3 max-w-100">
          <div class="flex justify-between text-xs text-text-secondary mb-1">
            <span class="truncate">Importing {{ scanFileName || "…" }}</span>
            <span class="ml-2 shrink-0">{{ scanCurrent }} / {{ scanTotal }}</span>
          </div>
          <div class="h-1.5 w-full rounded bg-surface-card overflow-hidden">
            <div class="h-full bg-accent-gold transition-all" :style="{ width: scanPercent + '%' }" />
          </div>
        </div>
        <div v-if="parseResult" class="success-box">{{ parseResult }}</div>
        <div v-if="parseError" class="error-box">{{ parseError }}</div>
        <div v-if="props.error" class="error-box">Player.log error: {{ props.error }}</div>
      </div>
    </div>

    <div class="settings-section">
      <h3>Excluded Channels</h3>
      <p class="text-text-muted text-xs leading-relaxed mb-4">
        Messages from these channels will not be imported when parsing chat logs.
      </p>

      <div class="flex flex-wrap gap-2 mb-4">
        <div
          v-for="(channel, index) in localExcludedChannels"
          :key="index"
          class="flex items-center gap-1 bg-surface-card border border-border-light rounded px-2 py-1 text-xs">
          <span class="text-text-primary">{{ channel }}</span>
          <button @click="removeChannel(index)" class="bg-transparent border-none text-text-secondary cursor-pointer text-lg px-1 py-0 leading-none hover:text-accent-red" title="Remove">&times;</button>
        </div>
      </div>

      <div class="flex gap-2 items-center">
        <input
          v-model="newChannel"
          @keyup.enter="addChannel"
          placeholder="Channel name..."
          class="input max-w-62" />
        <button @click="addChannel" :disabled="!newChannel.trim()" class="btn btn-secondary">
          Add Channel
        </button>
      </div>

      <button @click="resetChannels" class="btn btn-secondary mt-3">
        Reset to Defaults
      </button>
    </div>

    <div class="settings-section">
      <h3>Chat Log Storage</h3>

      <div class="mb-4">
        <div v-if="chatStats" class="status-panel">
          <div class="status-row">
            <span class="status-label">Chat Messages:</span>
            <span class="status-value">{{ chatStats.messageCount?.toLocaleString() ?? '0' }}</span>
          </div>
          <div class="status-row">
            <span class="status-label">Storage Used:</span>
            <span class="status-value">{{ formatBytes(chatStats.sizeBytes ?? 0) }}</span>
          </div>
        </div>
        <button @click="loadChatStats" class="btn btn-secondary" :disabled="loadingChatStats">
          {{ loadingChatStats ? 'Loading...' : 'Refresh Stats' }}
        </button>
      </div>
    </div>

    <div class="settings-section">
      <h3>Delete Chat Data</h3>

      <div class="mb-4">
        <button
          @click="deleteAllChatMessages"
          :disabled="deleting || !confirmDeleteAll"
          class="btn btn-secondary btn-danger">
          {{ deleting ? 'Deleting...' : 'Delete All Chat Messages' }}
        </button>

        <label class="flex items-center gap-2 cursor-pointer text-accent-red mt-2">
          <input
            type="checkbox"
            v-model="confirmDeleteAll"
            class="size-5 cursor-pointer" />
          <span>I understand this will delete ALL chat messages permanently</span>
        </label>

        <label class="flex items-center gap-2 cursor-pointer text-text-primary mt-2">
          <input
            type="checkbox"
            v-model="alsoDeleteWatchRules"
            class="size-5 cursor-pointer" />
          <span>Also delete all watch rules</span>
        </label>

        <p class="mt-2 text-text-muted text-xs leading-relaxed">
          Permanently delete all chat messages and reset import positions so logs can be re-imported.
          Optionally clear all watch rules as well.
        </p>

        <div v-if="deleteResult" class="success-box">{{ deleteResult }}</div>
        <div v-if="deleteError" class="error-box">{{ deleteError }}</div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted } from "vue";
import { open } from "@tauri-apps/plugin-dialog";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { useSettingsStore } from "../../stores/settingsStore";

const props = defineProps<{
  parsing: boolean;
  error: string;
}>();

const settingsStore = useSettingsStore();

const DEFAULT_EXCLUDED = [
  "System", "Error", "Emotes", "Action Emotes", "NPC Chatter", "Status", "Combat"
];

// Excluded channels
const localExcludedChannels = ref<string[]>([...settingsStore.settings.excludedChatChannels]);
const newChannel = ref("");

function addChannel() {
  const name = newChannel.value.trim();
  if (name && !localExcludedChannels.value.includes(name)) {
    localExcludedChannels.value.push(name);
    saveExcludedChannels();
  }
  newChannel.value = "";
}

function removeChannel(index: number) {
  localExcludedChannels.value.splice(index, 1);
  saveExcludedChannels();
}

function resetChannels() {
  localExcludedChannels.value = [...DEFAULT_EXCLUDED];
  saveExcludedChannels();
}

function saveExcludedChannels() {
  settingsStore.updateSettings({ excludedChatChannels: [...localExcludedChannels.value] });
}

// Manual parsing
const parseResult = ref<string | null>(null);
const parseError = ref<string | null>(null);
const importingFile = ref(false);
const importingFolder = ref(false);
const scanCurrent = ref(0);
const scanTotal = ref(0);
const scanFileName = ref("");
const scanPercent = computed(() =>
  scanTotal.value > 0 ? Math.round((scanCurrent.value / scanTotal.value) * 100) : 0
);
let unlistenScan: UnlistenFn | null = null;

onUnmounted(() => {
  if (unlistenScan) unlistenScan();
});

async function selectAndParseFile() {
  const selected = await open({
    multiple: false,
    defaultPath: (settingsStore.settings.gameDataPath ? settingsStore.settings.gameDataPath + "/ChatLogs" : undefined),
    filters: [{ name: "Chat Log Files", extensions: ["log", "txt"] }],
  });
  if (selected) {
    parseResult.value = null;
    parseError.value = null;
    importingFile.value = true;
    try {
      const result = await invoke<{ messages_imported: number }>('scan_chat_log_file', { path: selected });
      parseResult.value = `Imported ${result.messages_imported} messages from file.`;
    } catch (e: any) {
      parseError.value = `Import failed: ${e}`;
      console.error('Failed to parse chat log file:', e);
    } finally {
      importingFile.value = false;
    }
  }
}

async function parseFolder() {
  const selected = await open({
    directory: true,
    multiple: false,
    defaultPath: (settingsStore.settings.gameDataPath ? settingsStore.settings.gameDataPath + "/ChatLogs" : undefined),
  });
  if (selected) {
    parseResult.value = null;
    parseError.value = null;
    if (!unlistenScan) {
      try {
        unlistenScan = await listen<{ current: number; total: number; file_name: string }>(
          "chat-scan-progress",
          (event) => {
            scanCurrent.value = event.payload.current;
            scanTotal.value = event.payload.total;
            scanFileName.value = event.payload.file_name;
          }
        );
      } catch (e: any) {
        parseError.value = `Import failed: ${e}`;
        console.error('Failed to subscribe to scan progress:', e);
        return;
      }
    }
    importingFolder.value = true;
    scanCurrent.value = 0;
    scanTotal.value = 0;
    scanFileName.value = "";
    try {
      const result = await invoke<{ messages_imported: number }>('scan_chat_logs', { path: selected });
      parseResult.value = `Imported ${result.messages_imported} messages from folder.`;
    } catch (e: any) {
      parseError.value = `Import failed: ${e}`;
      console.error('Failed to parse chat log folder:', e);
    } finally {
      importingFolder.value = false;
    }
  }
}

// Chat stats
interface ChatStats {
  messageCount: number;
  sizeBytes: number;
}

const chatStats = ref<ChatStats | null>(null);
const loadingChatStats = ref(false);

async function loadChatStats() {
  loadingChatStats.value = true;
  try {
    const stats = await invoke<{ total_messages: number; messages_size_bytes: number }>('get_chat_stats');
    chatStats.value = {
      messageCount: stats.total_messages,
      sizeBytes: stats.messages_size_bytes,
    };
  } catch (e: any) {
    console.error('Failed to load chat stats:', e);
  } finally {
    loadingChatStats.value = false;
  }
}

function formatBytes(bytes: number): string {
  if (bytes === 0) return '0 Bytes';
  const k = 1024;
  const sizes = ['Bytes', 'KB', 'MB', 'GB'];
  const i = Math.floor(Math.log(bytes) / Math.log(k));
  return Math.round((bytes / Math.pow(k, i)) * 100) / 100 + ' ' + sizes[i];
}

// Delete all chat data
const confirmDeleteAll = ref(false);
const alsoDeleteWatchRules = ref(false);
const deleting = ref(false);
const deleteResult = ref<string | null>(null);
const deleteError = ref<string | null>(null);

async function deleteAllChatMessages() {
  if (!confirmDeleteAll.value) return;

  deleting.value = true;
  deleteResult.value = null;
  deleteError.value = null;

  try {
    const deleted = await invoke<number>('delete_all_chat_messages');
    let msg = `Deleted ${deleted.toLocaleString()} chat messages. Import positions have been reset.`;

    if (alsoDeleteWatchRules.value) {
      await settingsStore.updateSettings({ watchRules: [] });
      msg += ' Watch rules have been cleared.';
    }

    deleteResult.value = msg;
    confirmDeleteAll.value = false;
    alsoDeleteWatchRules.value = false;
    await loadChatStats();
  } catch (e: any) {
    deleteError.value = e.toString();
  } finally {
    deleting.value = false;
  }
}

onMounted(() => {
  loadChatStats();
});
</script>
