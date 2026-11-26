<template>
  <div class="fixed inset-0 bg-surface-dark flex items-center justify-center p-8">
    <div class="w-full max-w-md">
      <div class="mb-8 text-center">
        <h1 class="h1-brand">glogger</h1>
        <p class="text-text-muted text-xs mt-1">Select Character</p>
      </div>

      <div class="card p-6">
        <!-- Existing characters -->
        <div v-if="startupStore.userCharacters.length > 0" class="space-y-2 mb-6">
          <button
            v-for="char in startupStore.userCharacters"
            :key="char.id"
            @click="select(char.character_name, char.server_name)"
            class="w-full text-left px-4 py-3 rounded border border-border-default bg-surface-card text-text-secondary hover:border-border-hover hover:text-text-primary transition-colors">
            <span class="font-bold">{{ char.character_name }}</span>
            <span class="text-text-muted ml-2">{{ char.server_name }}</span>
            <span v-if="char.is_active" class="text-accent-green text-xs ml-2">(last used)</span>
          </button>
        </div>

        <div v-else class="text-text-muted text-sm text-center mb-6 py-4">
          No characters found. Add one below.
        </div>

        <!-- Add new character -->
        <div v-if="showAdd" class="mb-4">
          <div class="flex gap-2 mb-2">
            <input
              v-model="newName"
              placeholder="Character name"
              class="input flex-1" />
            <select v-model="newServer" class="input">
              <option value="" disabled>Server</option>
              <option v-for="s in startupStore.serverList" :key="s" :value="s">{{ s }}</option>
            </select>
          </div>
          <div class="flex gap-2 justify-end">
            <button @click="showAdd = false" class="btn btn-secondary">Cancel</button>
            <button @click="addAndSelect" :disabled="!newName.trim() || !newServer" class="btn btn-primary">
              Add & Launch
            </button>
          </div>
        </div>

        <button v-else @click="showAdd = true" class="btn btn-secondary w-full">
          + Add Character
        </button>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref } from "vue";
import { useStartupStore } from "../../stores/startupStore";

const startupStore = useStartupStore();

const showAdd = ref(false);
const newName = ref("");
const newServer = ref("");

async function select(characterName: string, serverName: string) {
  await startupStore.selectCharacter(characterName, serverName);
}

async function addAndSelect() {
  const name = newName.value.trim();
  await startupStore.saveCharacter(name, newServer.value, "manual");
  await startupStore.selectCharacter(name, newServer.value);
}
</script>
