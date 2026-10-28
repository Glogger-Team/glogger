<!-- DARK until LocalPlayer: lines recur (hidden 2026-10-28) -->
<template>
  <div class="flex flex-col gap-3 text-sm">
    <div v-if="messages.length === 0" class="text-xs text-text-dim italic">
      No pigeon or stall note messages recorded yet.
    </div>
    <div v-else class="flex flex-col gap-1.5">
      <div
        v-for="msg in messages"
        :key="msg.id"
        class="flex flex-col gap-0.5 py-1 border-b border-border-default last:border-0"
      >
        <div class="flex items-center justify-between gap-2">
          <div class="flex items-center gap-1.5 min-w-0">
            <!-- Direction arrow -->
            <span
              class="text-xs shrink-0"
              :class="msg.direction === 'received' ? 'text-value-positive' : 'text-value-negative'"
              :title="msg.direction === 'received' ? 'Received' : 'Sent'"
            >
              {{ msg.direction === 'received' ? '\u2B07' : '\u2B06' }}
            </span>
            <!-- Message type icon -->
            <span class="text-xs shrink-0" :title="msg.message_type === 'pigeon' ? 'Pigeon' : 'Stall Note'">
              {{ msg.message_type === 'pigeon' ? '\uD83D\uDC26' : '\uD83D\uDCDD' }}
            </span>
            <!-- Player name -->
            <span class="text-text-primary font-medium truncate">
              {{ msg.other_player }}
            </span>
          </div>
          <span class="text-text-dim text-xs font-mono whitespace-nowrap shrink-0">
            {{ formatTs(msg.timestamp) }}
          </span>
        </div>
        <!-- Message body -->
        <div v-if="msg.body" class="text-xs text-text-secondary pl-6 truncate">
          {{ msg.body }}
        </div>
        <!-- Attached item -->
        <div v-if="msg.item_name" class="text-xs text-text-secondary pl-6 flex items-center gap-1">
          <span class="text-text-dim">Attached:</span>
          <ItemInline :reference="msg.item_name" />
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, onMounted, onUnmounted } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { useSettingsStore } from '../../../stores/settingsStore'
import { formatAnyTimestamp as formatTs } from '../../../composables/useTimestamp'
import ItemInline from '../../Shared/Item/ItemInline.vue'

interface PlayerMessage {
  id: number
  character_name: string
  server_name: string
  timestamp: string
  message_type: string
  direction: string
  other_player: string
  body: string
  item_name: string | null
}

const settings = useSettingsStore()
const messages = ref<PlayerMessage[]>([])
let unlisten: UnlistenFn | null = null

async function loadMessages() {
  const char = settings.settings.activeCharacterName
  const server = settings.settings.activeServerName
  if (!char || !server) return

  try {
    messages.value = await invoke<PlayerMessage[]>('get_player_messages', {
      characterName: char,
      serverName: server,
      limit: 20,
    })
  } catch (e) {
    console.error('[messages-widget] Failed to load messages:', e)
  }
}

onMounted(async () => {
  await loadMessages()

  unlisten = await listen<string[]>('game-state-updated', (event) => {
    if (event.payload.includes('player_messages')) {
      loadMessages()
    }
  })
})

onUnmounted(() => {
  if (unlisten) unlisten()
})
</script>
