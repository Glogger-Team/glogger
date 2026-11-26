<template>
  <div>
    <div class="settings-section">
      <h3 class="panel-label">CDN Game Data Versions</h3>

      <div class="mb-4">
        <div v-if="gameDataStore.cacheStatus" class="status-panel">
          <div class="status-row">
            <span class="status-label">Local Version:</span>
            <span class="status-value">{{ gameDataStore.cacheStatus.cached_version || 'None' }}</span>
          </div>
          <div class="status-row">
            <span class="status-label">CDN Version:</span>
            <span class="status-value">{{ gameDataStore.cacheStatus.remote_version || 'Unknown' }}</span>
          </div>
          <div class="status-row">
            <span class="status-label">Status:</span>
            <span class="status-value" :class="{ 'text-accent-green!': gameDataStore.cacheStatus.up_to_date }">
              {{ gameDataStore.cacheStatus.up_to_date ? 'Up to date' : 'Update available' }}
            </span>
          </div>
          <div class="status-row">
            <span class="status-label">Items:</span>
            <span class="status-value">{{ gameDataStore.cacheStatus.item_count.toLocaleString() }}</span>
          </div>
          <div class="status-row">
            <span class="status-label">Skills:</span>
            <span class="status-value">{{ gameDataStore.cacheStatus.skill_count.toLocaleString() }}</span>
          </div>
        </div>

        <div v-else class="text-text-muted text-xs">
          Game data status not yet loaded.
        </div>
      </div>
    </div>

    <div class="settings-section">
      <h3 class="panel-label">Force Rebuild</h3>

      <div class="mb-4">
        <button
          @click="forceRefreshCDN"
          :disabled="refreshing"
          class="btn btn-secondary">
          {{ refreshing ? 'Refreshing...' : 'Force Refresh CDN Data' }}
        </button>
        <p class="mt-2 text-text-muted text-xs leading-relaxed">
          Re-download all game data from the Project: Gorgon CDN and rebuild local tables.
          This will update items, skills, recipes, quests, NPCs, and abilities.
        </p>
        <div v-if="refreshError" class="error-box">{{ refreshError }}</div>
      </div>

      <div class="mb-4">
        <button
          @click="rebuildCdnTables"
          :disabled="rebuilding"
          class="btn btn-secondary btn-warning">
          {{ rebuilding ? 'Rebuilding...' : 'Rebuild Local Tables Only' }}
        </button>
        <p class="mt-2 text-text-muted text-xs leading-relaxed">
          Rebuild all CDN-derived database tables from the currently cached JSON data
          without re-downloading. Your player data will not be affected.
        </p>
        <div v-if="rebuildSuccess" class="success-box">{{ rebuildSuccess }}</div>
        <div v-if="rebuildError" class="error-box">{{ rebuildError }}</div>
      </div>
    </div>

    <div class="settings-section">
      <h3 class="panel-label">Auto-Update</h3>

      <div class="mb-4">
        <label class="flex items-center gap-2 cursor-pointer text-text-primary">
          <input
            type="checkbox"
            v-model="autoCheck"
            @change="saveAutoCheck"
            class="size-5 cursor-pointer" />
          <span>Automatically check for new game data versions</span>
        </label>
        <p class="mt-2 text-text-muted text-xs leading-relaxed">
          When enabled, the app will check the CDN for updated game data on startup.
        </p>
      </div>

      <div>
        <label class="flex items-center gap-2 cursor-pointer text-text-primary">
          <input
            type="checkbox"
            v-model="autoUpdate"
            @change="saveAutoUpdate"
            class="size-5 cursor-pointer" />
          <span>Automatically download and apply updates</span>
        </label>
        <p class="mt-2 text-text-muted text-xs leading-relaxed">
          When enabled along with auto-check, new game data will be downloaded and applied
          automatically without user interaction.
        </p>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { useSettingsStore } from "../../stores/settingsStore";
import { useGameDataStore } from "../../stores/gameDataStore";

const settingsStore = useSettingsStore();
const gameDataStore = useGameDataStore();

// CDN refresh
const refreshing = ref(false);
const refreshError = ref<string | null>(null);

async function forceRefreshCDN() {
  refreshing.value = true;
  refreshError.value = null;
  try {
    await gameDataStore.forceRefreshCdn();
  } catch (e: any) {
    refreshError.value = e.toString();
  } finally {
    refreshing.value = false;
  }
}

// CDN rebuild
const rebuilding = ref(false);
const rebuildSuccess = ref<string | null>(null);
const rebuildError = ref<string | null>(null);

async function rebuildCdnTables() {
  rebuilding.value = true;
  rebuildSuccess.value = null;
  rebuildError.value = null;
  try {
    const result = await invoke<string>('force_rebuild_cdn_tables');
    rebuildSuccess.value = result;
  } catch (e: any) {
    rebuildError.value = e.toString();
  } finally {
    rebuilding.value = false;
  }
}

// Auto-update settings
const autoCheck = ref(settingsStore.settings.autoCheckGameData);
const autoUpdate = ref(settingsStore.settings.autoUpdateGameData);

function saveAutoCheck() {
  settingsStore.updateSettings({ autoCheckGameData: autoCheck.value });
}

function saveAutoUpdate() {
  settingsStore.updateSettings({ autoUpdateGameData: autoUpdate.value });
}
</script>
