<template>
  <div class="card p-6">
    <h2 class="screen-title mb-2">Locate Project Gorgon</h2>
    <p class="text-text-muted text-sm mb-6">
      Select the folder where Project Gorgon stores its data (ChatLogs, Reports, Books).
      On Windows: <code>AppData\LocalLow\Elder Game\Project Gorgon</code><br />
      On macOS: <code>~/Library/Application Support/unity.Elder Game.Project Gorgon</code>
    </p>

    <div class="flex gap-2 mb-4">
      <input
        v-model="localPath"
        @blur="onPathChange"
        @keyup.enter="onPathChange"
        placeholder="Path to Project Gorgon data folder..."
        class="input flex-1" />
      <button @click="browse" class="btn btn-secondary whitespace-nowrap">Browse</button>
    </div>

    <!-- Validation checkmarks -->
    <div v-if="validation" class="space-y-2 mb-6">
      <ValidationRow :ok="validation.path_exists" label="Folder exists" />
      <ValidationRow :ok="validation.player_log_found" label="Player.log found" />
      <ValidationRow :ok="validation.chat_logs_found" label="ChatLogs folder found" />
      <ValidationRow :ok="validation.reports_found" label="Reports folder found" />
    </div>

    <div class="flex justify-end">
      <button
        @click="next"
        :disabled="!canProceed"
        class="btn btn-primary">
        Next
      </button>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, onMounted } from "vue";
import { open } from "@tauri-apps/plugin-dialog";
import { useStartupStore } from "../../stores/startupStore";
import { useSettingsStore } from "../../stores/settingsStore";
import ValidationRow from "./ValidationRow.vue";

const startupStore = useStartupStore();
const settingsStore = useSettingsStore();

const localPath = ref(settingsStore.settings.gameDataPath);
const validation = computed(() => startupStore.pathValidation);
const canProceed = computed(() => validation.value?.path_exists === true);

onMounted(async () => {
  if (localPath.value) {
    await startupStore.validatePath(localPath.value);
  }
});

async function onPathChange() {
  if (localPath.value) {
    await startupStore.validatePath(localPath.value);
  }
}

async function browse() {
  const selected = await open({ directory: true, multiple: false, defaultPath: localPath.value || undefined });
  if (selected) {
    localPath.value = selected;
    await startupStore.validatePath(selected);
  }
}

async function next() {
  await settingsStore.updateGameDataPath(localPath.value);

  // Scan for characters while we're at it
  if (validation.value?.reports_found) {
    await startupStore.scanForCharacters(localPath.value);
  }

  startupStore.goToPhase("setup-watchers");
}
</script>
