<template>
  <div>
    <div class="settings-section">
      <h3>User Data Storage</h3>

      <div class="mb-4">
        <div v-if="dbStats" class="status-panel">
          <div class="status-row">
            <span class="status-label">Total Database Size:</span>
            <span class="status-value">{{ formatBytes(dbStats.total_size_bytes) }}</span>
          </div>
          <div class="status-row">
            <span class="status-label">Player Data Size:</span>
            <span class="status-value">{{ formatBytes(dbStats.player_data_size_bytes) }}</span>
          </div>
          <div class="status-row">
            <span class="status-label">Reclaimable (Compact):</span>
            <span class="status-value">{{ formatBytes(dbStats.free_bytes) }}</span>
          </div>
          <div class="status-row">
            <span class="status-label">Item Transactions:</span>
            <span class="status-value">{{ dbStats.item_transactions_count.toLocaleString() }}</span>
          </div>
          <div class="status-row">
            <span class="status-label">Chat Messages:</span>
            <span class="status-value">{{ dbStats.chat_messages_count.toLocaleString() }}</span>
          </div>
          <div class="status-row">
            <span class="status-label">Market Prices:</span>
            <span class="status-value">{{ dbStats.market_prices_count.toLocaleString() }}</span>
          </div>
          <div class="status-row">
            <span class="status-label">Sales History:</span>
            <span class="status-value">{{ dbStats.sales_history_count.toLocaleString() }}</span>
          </div>
          <div class="status-row">
            <span class="status-label">Survey Sessions:</span>
            <span class="status-value">{{ dbStats.survey_sessions_count.toLocaleString() }}</span>
          </div>
          <div class="status-row">
            <span class="status-label">Event Log:</span>
            <span class="status-value">{{ dbStats.event_log_count.toLocaleString() }}</span>
          </div>
        </div>

        <div v-if="dbStats && dbStats.largest_tables.length" class="status-panel mt-3">
          <div class="status-row font-semibold">
            <span class="status-label">Largest Tables</span>
            <span class="status-value">On-disk size</span>
          </div>
          <div
            v-for="t in dbStats.largest_tables"
            :key="t.name"
            class="status-row">
            <span class="status-label font-mono text-xs">{{ t.name }}</span>
            <span class="status-value">{{ formatBytes(t.size_bytes) }}</span>
          </div>
        </div>

        <div class="flex items-center gap-2 mt-3">
          <button @click="loadStats" class="btn btn-secondary" :disabled="loadingStats">
            {{ loadingStats ? 'Loading...' : 'Refresh Statistics' }}
          </button>
          <button @click="compactDatabase" class="btn btn-secondary" :disabled="compacting">
            {{ compacting ? 'Compacting...' : 'Compact Database' }}
          </button>
        </div>
        <p class="mt-2 text-text-muted text-xs leading-relaxed">
          Compact rebuilds the database file to return free space to your disk. It does
          not delete any data — safe to run any time.
        </p>
        <div v-if="compactResult" class="success-box">{{ compactResult }}</div>
        <div v-if="compactError" class="error-box">{{ compactError }}</div>
      </div>
    </div>

    <div class="settings-section">
      <h3>Backup &amp; Export</h3>

      <div class="mb-4">
        <button @click="backupData" :disabled="backingUp" class="btn btn-secondary">
          {{ backingUp ? 'Backing up...' : 'Backup User Data' }}
        </button>
        <p class="mt-2 text-text-muted text-xs leading-relaxed">
          Export all user data (survey history, market prices, sales history) to a backup file.
        </p>
        <div v-if="backupResult" class="success-box">{{ backupResult }}</div>
        <div v-if="backupError" class="error-box">{{ backupError }}</div>
      </div>
    </div>

    <div class="settings-section">
      <h3>Auto-Purge</h3>

      <div class="mb-4">
        <label class="flex items-center gap-2 cursor-pointer text-text-primary">
          <input
            type="checkbox"
            v-model="autoPurgeEnabled"
            @change="handleAutoPurgeToggle"
            class="size-5 cursor-pointer" />
          <span>Automatically purge old user data on startup</span>
        </label>

        <div v-if="autoPurgeEnabled" class="mt-4 p-4 bg-surface-dark border border-border-default rounded">
          <label class="block text-text-secondary mb-1">Delete data older than:</label>
          <div class="flex items-center gap-2 mt-2">
            <input
              type="number"
              v-model.number="autoPurgeDays"
              @blur="handleAutoPurgeDaysChange"
              min="1"
              max="9999"
              class="input w-25"
              placeholder="Days" />
            <span class="text-text-secondary">days</span>
          </div>
        </div>

        <p class="mt-2 text-text-muted text-xs leading-relaxed">
          When enabled, player data older than the specified number of days is automatically
          deleted on app startup — including item transactions and stored chat history (the
          two largest tables), plus market prices, sales, surveys, and events. Your raw chat
          <code>.log</code> files on disk are never touched.
        </p>
      </div>
    </div>

    <div class="settings-section">
      <h3>Manual Purge</h3>

      <div class="mb-4">
        <label class="block text-text-secondary mb-1">Delete records older than:</label>
        <div class="flex items-center gap-2 mt-2">
          <input
            type="number"
            v-model.number="purgeDays"
            min="1"
            max="9999"
            class="input w-25"
            placeholder="Days" />
          <span class="text-text-secondary mr-2">days</span>
          <button
            @click="purgeOldData"
            :disabled="purging || !purgeDays || purgeDays < 1"
            class="btn btn-secondary btn-warning">
            {{ purging ? 'Purging...' : 'Purge Old Data' }}
          </button>
        </div>
      </div>

      <div class="mb-4">
        <button
          @click="purgeAllData"
          :disabled="purging || !confirmPurgeAll"
          class="btn btn-secondary btn-danger">
          {{ purging ? 'Purging...' : 'Purge ALL User Data' }}
        </button>

        <label class="flex items-center gap-2 cursor-pointer text-accent-red mt-2">
          <input
            type="checkbox"
            v-model="confirmPurgeAll"
            class="size-5 cursor-pointer" />
          <span>I understand this will delete ALL user data permanently</span>
        </label>

        <p class="mt-2 text-text-muted text-xs leading-relaxed">
          Permanently delete ALL user data: item transactions, stored chat history, market
          prices, sales, surveys, and events. CDN data and your raw chat <code>.log</code>
          files on disk are NOT affected. This cannot be undone.
        </p>
      </div>

      <div v-if="purgeResult" class="info-box">
        <strong>Purge Complete:</strong>
        <ul class="mt-2 mb-0 pl-6">
          <li class="my-1">Item Transactions: {{ purgeResult.item_transactions_deleted.toLocaleString() }} deleted</li>
          <li class="my-1">Chat Messages: {{ purgeResult.chat_messages_deleted.toLocaleString() }} deleted</li>
          <li class="my-1">Market Prices: {{ purgeResult.market_prices_deleted.toLocaleString() }} deleted</li>
          <li class="my-1">Sales History: {{ purgeResult.sales_deleted.toLocaleString() }} deleted</li>
          <li class="my-1">Survey Sessions: {{ purgeResult.survey_sessions_deleted.toLocaleString() }} deleted</li>
          <li class="my-1">Event Log: {{ purgeResult.events_deleted.toLocaleString() }} deleted</li>
          <li class="my-1 font-semibold">Space reclaimed: {{ formatBytes(purgeResult.bytes_reclaimed) }}</li>
        </ul>
      </div>

      <div v-if="purgeError" class="error-box">{{ purgeError }}</div>
    </div>

    <div class="settings-section">
      <h3>Reports Import</h3>

      <p class="mt-2 text-text-muted text-xs leading-relaxed">
        Automatic report importing is live — see Advanced &gt; Character Report Watching.
      </p>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, onMounted } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { save } from "@tauri-apps/plugin-dialog";
import { useSettingsStore } from "../../stores/settingsStore";
import type {
  DatabaseStats,
  PurgeResult,
  CompactResult,
} from "../../types/database";

const settingsStore = useSettingsStore();

const dbStats = ref<DatabaseStats | null>(null);
const loadingStats = ref(false);

async function loadStats() {
  loadingStats.value = true;
  try {
    dbStats.value = await invoke<DatabaseStats>('get_database_stats');
  } catch (e: any) {
    console.error('Failed to load database stats:', e);
  } finally {
    loadingStats.value = false;
  }
}

function formatBytes(bytes: number): string {
  if (bytes === 0) return '0 Bytes';
  const k = 1024;
  const sizes = ['Bytes', 'KB', 'MB', 'GB'];
  const i = Math.floor(Math.log(bytes) / Math.log(k));
  return Math.round((bytes / Math.pow(k, i)) * 100) / 100 + ' ' + sizes[i];
}

// Backup
const backingUp = ref(false);
const backupResult = ref<string | null>(null);
const backupError = ref<string | null>(null);

async function backupData() {
  const filePath = await save({
    filters: [{ name: "SQLite Database", extensions: ["db"] }],
    defaultPath: "glogger-backup.db",
  });
  if (!filePath) return;

  backingUp.value = true;
  backupResult.value = null;
  backupError.value = null;
  try {
    await invoke('backup_database', { path: filePath });
    backupResult.value = `Backup saved to: ${filePath}`;
  } catch (e: any) {
    backupError.value = e.toString();
  } finally {
    backingUp.value = false;
  }
}

// Auto-purge
const autoPurgeEnabled = ref(settingsStore.settings.autoPurgeEnabled || false);
const autoPurgeDays = ref(settingsStore.settings.autoPurgeDays || 90);

function handleAutoPurgeToggle() {
  settingsStore.updateSettings({ autoPurgeEnabled: autoPurgeEnabled.value });
  if (autoPurgeEnabled.value && !settingsStore.settings.autoPurgeDays) {
    settingsStore.updateSettings({ autoPurgeDays: 90 });
  }
}

function handleAutoPurgeDaysChange() {
  if (autoPurgeDays.value && autoPurgeDays.value > 0) {
    settingsStore.updateSettings({ autoPurgeDays: autoPurgeDays.value });
  }
}

// Compact (VACUUM)
const compacting = ref(false);
const compactResult = ref<string | null>(null);
const compactError = ref<string | null>(null);

async function compactDatabase() {
  compacting.value = true;
  compactResult.value = null;
  compactError.value = null;
  try {
    const r = await invoke<CompactResult>('compact_database');
    compactResult.value =
      r.bytes_reclaimed > 0
        ? `Reclaimed ${formatBytes(r.bytes_reclaimed)} (now ${formatBytes(r.bytes_after)}).`
        : `Already compact — nothing to reclaim.`;
    await loadStats();
  } catch (e: any) {
    compactError.value = e.toString();
  } finally {
    compacting.value = false;
  }
}

// Manual purge
const purgeDays = ref<number>(90);
const purging = ref(false);
const confirmPurgeAll = ref(false);
const purgeResult = ref<PurgeResult | null>(null);
const purgeError = ref<string | null>(null);

async function purgeOldData() {
  if (!purgeDays.value || purgeDays.value < 1) return;

  purging.value = true;
  purgeResult.value = null;
  purgeError.value = null;

  try {
    const result = await invoke<PurgeResult>('purge_player_data', {
      options: {
        older_than_days: purgeDays.value,
        purge_all: false,
      }
    });
    purgeResult.value = result;
    await loadStats();
  } catch (e: any) {
    purgeError.value = e.toString();
  } finally {
    purging.value = false;
  }
}

async function purgeAllData() {
  if (!confirmPurgeAll.value) return;

  purging.value = true;
  purgeResult.value = null;
  purgeError.value = null;

  try {
    const result = await invoke<PurgeResult>('purge_player_data', {
      options: {
        older_than_days: null,
        purge_all: true,
      }
    });
    purgeResult.value = result;
    confirmPurgeAll.value = false;
    await loadStats();
  } catch (e: any) {
    purgeError.value = e.toString();
  } finally {
    purging.value = false;
  }
}

onMounted(() => {
  loadStats();
});
</script>
