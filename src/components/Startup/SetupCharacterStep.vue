<template>
  <div class="card p-6">
    <h2 class="screen-title mb-2">Select Your Character</h2>
    <p class="text-text-muted text-sm mb-6">
      Choose a character to use as your active character, or add one manually.
    </p>

    <!-- Discovered characters from Reports -->
    <div v-if="startupStore.discoveredCharacters.length > 0" class="mb-6">
      <h3 class="text-sm text-text-secondary mb-3">Found in Reports</h3>
      <div class="space-y-2">
        <button
          v-for="char in startupStore.discoveredCharacters"
          :key="`${char.character_name}-${char.server_name}`"
          @click="selectDiscovered(char)"
          class="w-full text-left px-4 py-3 rounded border transition-colors"
          :class="isSelected(char.character_name, char.server_name)
            ? 'border-accent-gold bg-accent-gold/10 text-text-primary'
            : 'border-border-default bg-surface-card text-text-secondary hover:border-border-hover hover:text-text-primary'">
          <span class="font-bold">{{ char.character_name }}</span>
          <span class="text-text-muted ml-2">{{ char.server_name }}</span>
          <span class="text-text-dim text-xs ml-2">
            ({{ char.report_count }} report{{ char.report_count !== 1 ? 's' : '' }})
          </span>
        </button>
      </div>
    </div>

    <!-- Manual entry -->
    <div class="mb-6">
      <h3 class="text-sm text-text-secondary mb-3">
        {{ startupStore.discoveredCharacters.length > 0 ? 'Or enter manually' : 'Enter your character' }}
      </h3>
      <div class="flex gap-2">
        <input
          v-model="manualName"
          placeholder="Character name"
          class="input flex-1"
          @input="clearSelection" />
        <select v-model="manualServer" class="input" @change="clearSelection">
          <option value="" disabled>Server</option>
          <option v-for="s in startupStore.serverList" :key="s" :value="s">{{ s }}</option>
        </select>
      </div>
    </div>

    <!-- Auto-load toggle -->
    <div class="mb-6">
      <label class="flex items-center gap-3 cursor-pointer text-text-primary">
        <input
          type="checkbox"
          v-model="autoLoadLast"
          class="size-5 cursor-pointer" />
        <div>
          <span class="text-sm">Auto-load last character on startup</span>
          <p class="text-text-muted text-xs mt-0.5">
            Skip character selection on future launches.
          </p>
        </div>
      </label>
    </div>

    <div class="flex justify-between">
      <button @click="back" class="btn btn-secondary">Back</button>
      <button @click="finish" :disabled="!hasSelection" class="btn btn-primary">
        Finish Setup
      </button>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed } from "vue";
import { useStartupStore, type DiscoveredCharacter } from "../../stores/startupStore";
import { useSettingsStore } from "../../stores/settingsStore";

const startupStore = useStartupStore();
const settingsStore = useSettingsStore();

const selectedName = ref("");
const selectedServer = ref("");
const manualName = ref("");
const manualServer = ref("");
const autoLoadLast = ref(true);

const hasSelection = computed(() => {
  if (selectedName.value && selectedServer.value) return true;
  if (manualName.value.trim() && manualServer.value) return true;
  return false;
});

function isSelected(name: string, server: string) {
  return selectedName.value === name && selectedServer.value === server;
}

function selectDiscovered(char: DiscoveredCharacter) {
  selectedName.value = char.character_name;
  selectedServer.value = char.server_name;
  manualName.value = "";
  manualServer.value = "";
}

function clearSelection() {
  selectedName.value = "";
  selectedServer.value = "";
}

function back() {
  startupStore.goToPhase("setup-watchers");
}

async function finish() {
  const charName = selectedName.value || manualName.value.trim();
  const serverName = selectedServer.value || manualServer.value;
  const source = selectedName.value ? "report" : "manual";

  // Save character to database
  await startupStore.saveCharacter(charName, serverName, source);

  // Set as active character in backend + settings (but don't run startup tasks yet)
  await startupStore.setActiveCharacter(charName, serverName);

  // Save auto-load preference
  await settingsStore.updateSettings({ autoLoadLastCharacter: autoLoadLast.value });

  // Complete setup — this triggers the full startup task sequence
  await startupStore.completeSetup();
}
</script>
