<template>
  <div class="relative" ref="containerRef">
    <button
      class="bg-transparent border-none text-text-muted text-xs cursor-pointer rounded transition-all hover:text-text-secondary text-left px-0 py-0"
      @click="toggleDropdown"
      title="Switch character">
      <span v-if="activeServer" class="text-text-muted">{{ activeServer }}</span>
      <span v-if="activeServer && activeCharacter" class="text-text-muted"> — </span>
      <span class="text-text-secondary">{{ activeCharacter ?? 'No Character' }}</span>
    </button>

    <!-- Dropdown -->
    <div
      v-if="open"
      class="absolute left-0 top-full mt-1 w-72 bg-surface-card border border-border-default rounded shadow-lg z-50 overflow-hidden">
      <!-- Character list -->
      <div class="max-h-64 overflow-y-auto">
        <button
          v-for="char in characters"
          :key="`${char.character_name}-${char.server_name}`"
          class="w-full text-left px-3 py-2 text-sm hover:bg-surface-elevated transition-colors flex items-center justify-between group"
          :class="isSelected(char) ? 'bg-surface-elevated text-text-primary' : 'text-text-secondary'"
          @click="switchTo(char)">
          <span>
            <span class="font-bold">{{ char.character_name }}</span>
            <span class="text-text-muted ml-1.5">{{ char.server_name }}</span>
          </span>
          <button
            v-if="!isSelected(char)"
            class="opacity-0 group-hover:opacity-100 text-text-muted hover:text-red-400 bg-transparent border-none cursor-pointer text-xs px-1 transition-opacity"
            title="Delete character"
            @click.stop="confirmDelete(char)">
            ✕
          </button>
        </button>

        <div v-if="characters.length === 0" class="px-3 py-4 text-text-muted text-sm text-center">
          No characters found
        </div>
      </div>

      <!-- Add character -->
      <div class="border-t border-border-default">
        <div v-if="showAdd" class="p-2 space-y-2">
          <input
            v-model="newName"
            placeholder="Character name"
            class="input w-full text-sm"
            @keydown.enter="addCharacter" />
          <div class="flex gap-2">
            <input
              v-model="newServer"
              placeholder="Server"
              list="server-suggestions"
              class="input flex-1 text-sm" />
            <datalist id="server-suggestions">
              <option v-for="s in serverList" :key="s" :value="s" />
            </datalist>
            <button
              class="btn btn-primary text-xs px-2 py-1"
              :disabled="!newName.trim() || !newServer.trim()"
              @click="addCharacter">
              Add
            </button>
            <button class="btn btn-secondary text-xs px-2 py-1" @click="showAdd = false">
              Cancel
            </button>
          </div>
        </div>
        <button
          v-else
          class="w-full text-left px-3 py-2 text-sm text-text-muted hover:bg-surface-elevated hover:text-text-primary transition-colors"
          @click="showAdd = true">
          + Add Character
        </button>
      </div>
    </div>

    <!-- Delete confirmation -->
    <div
      v-if="deleteTarget"
      class="absolute left-0 top-full mt-1 w-72 bg-surface-card border border-border-default rounded shadow-lg z-50 p-4">
      <p class="text-sm text-text-primary mb-3">
        Delete <strong>{{ deleteTarget.character_name }}</strong>
        <span class="text-text-muted">({{ deleteTarget.server_name }})</span>?
      </p>
      <p class="text-xs text-text-muted mb-3">
        All data for this character on this server will be permanently removed.
      </p>
      <div class="flex gap-2 justify-end">
        <button class="btn btn-secondary text-xs" @click="deleteTarget = null">Cancel</button>
        <button class="btn text-xs bg-red-600 hover:bg-red-500 text-white border-none" @click="doDelete">Delete</button>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { useSettingsStore } from '../stores/settingsStore'
import { useGameStateStore } from '../stores/gameStateStore'
import { useCharacterStore } from '../stores/characterStore'
import type { UserCharacter } from '../stores/startupStore'

defineProps<{
  isActive: boolean
}>()

const settingsStore = useSettingsStore()
const gameStateStore = useGameStateStore()
const characterStore = useCharacterStore()

const open = ref(false)
const showAdd = ref(false)
const newName = ref('')
const newServer = ref('')
const deleteTarget = ref<UserCharacter | null>(null)
const characters = ref<UserCharacter[]>([])
const serverList = ref<string[]>([])
const containerRef = ref<HTMLElement | null>(null)

const activeCharacter = computed(() => settingsStore.settings.activeCharacterName)
const activeServer = computed(() => settingsStore.settings.activeServerName)

function isSelected(char: UserCharacter): boolean {
  return char.character_name === activeCharacter.value
    && char.server_name === activeServer.value
}

async function loadCharacters() {
  try {
    characters.value = await invoke<UserCharacter[]>('get_user_characters')
    serverList.value = await invoke<string[]>('get_server_list')
  } catch (e) {
    console.error('Failed to load characters:', e)
  }
}

async function toggleDropdown() {
  if (open.value) {
    open.value = false
    return
  }
  deleteTarget.value = null
  showAdd.value = false
  await loadCharacters()
  open.value = true
}

async function switchTo(char: UserCharacter) {
  if (isSelected(char)) {
    open.value = false
    return
  }

  await invoke('set_active_character', {
    characterName: char.character_name,
    serverName: char.server_name,
  })

  settingsStore.settings.activeCharacterName = char.character_name
  settingsStore.settings.activeServerName = char.server_name

  // Reload all data for the new character
  gameStateStore.resetSessionSkills()
  gameStateStore.clearLiveInventory()
  await gameStateStore.loadAll()
  characterStore.initForActiveCharacter()

  open.value = false
}

function confirmDelete(char: UserCharacter) {
  open.value = false
  deleteTarget.value = char
}

async function doDelete() {
  if (!deleteTarget.value) return

  const char = deleteTarget.value
  await invoke('delete_character', {
    characterName: char.character_name,
    serverName: char.server_name,
  })

  // If we deleted the active character, clear settings
  if (isSelected(char)) {
    settingsStore.settings.activeCharacterName = null
    settingsStore.settings.activeServerName = null
  }

  deleteTarget.value = null
  // Deleted character's rows are gone from both report tables — clear both
  // cross-character caches so tooltips/search don't show a ghost. Bump
  // generations so an in-flight load started before the delete drops its
  // stale result instead of repopulating the ghost.
  characterStore.allCharacterItems = []
  characterStore.allCharacterItemsLoaded = false
  characterStore.allCharacterItemsGeneration++
  characterStore.allCharacterWorkOrders = []
  characterStore.allCharacterWorkOrdersLoaded = false
  characterStore.allCharacterWorkOrdersGeneration++
  // Reopen the picker
  await loadCharacters()
  open.value = true
}

async function addCharacter() {
  const name = newName.value.trim()
  const server = newServer.value.trim()
  if (!name || !server) return

  await invoke('save_user_character', {
    characterName: name,
    serverName: server,
    source: 'manual',
  })

  newName.value = ''
  newServer.value = ''
  showAdd.value = false

  // Switch to the new character
  await invoke('set_active_character', {
    characterName: name,
    serverName: server,
  })

  settingsStore.settings.activeCharacterName = name
  settingsStore.settings.activeServerName = server

  gameStateStore.resetSessionSkills()
  gameStateStore.clearLiveInventory()
  await gameStateStore.loadAll()
  characterStore.initForActiveCharacter()

  open.value = false
}

// Close dropdown on outside click
function handleClickOutside(event: MouseEvent) {
  if (containerRef.value && !containerRef.value.contains(event.target as Node)) {
    open.value = false
    deleteTarget.value = null
  }
}

onMounted(() => {
  document.addEventListener('click', handleClickOutside)
})

onUnmounted(() => {
  document.removeEventListener('click', handleClickOutside)
})
</script>
