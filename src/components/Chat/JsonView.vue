<template>
  <div class="flex flex-col h-full">
    <div class="px-6 py-4 border-b border-border-default flex justify-between items-center bg-surface-base">
      <h2 class="screen-title m-0">JSON</h2>
      <div class="flex gap-2 items-center">
        <input
          type="text"
          v-model="searchText"
          @input="onSearchInput"
          placeholder="Search JSON lines..."
          class="px-4 py-2 bg-surface-elevated border border-border-light rounded text-text-primary w-75 focus:outline-none focus:border-accent-gold"
        />
        <button v-if="pendingCount > 0" @click="showNewest" class="px-3 h-9 bg-surface-elevated border border-accent-gold rounded text-accent-gold text-sm font-medium cursor-pointer transition-all flex items-center hover:bg-border-default" title="Show new messages">↓ {{ pendingCount }} new</button>
        <button
          @click="reset"
          :disabled="loading"
          class="w-9 h-9 p-0 bg-surface-elevated border border-border-light rounded text-text-primary text-xl cursor-pointer transition-all flex items-center justify-center hover:bg-border-default hover:border-border-hover disabled:opacity-50 disabled:cursor-not-allowed"
          title="Refresh"
        >
          ⟳
        </button>
      </div>
    </div>
    <ChatMessageList
      :messages="messages"
      :loading="loading"
      :has-more="hasMore"
      :show-channel="true"
      :sort-order="sortOrder"
      :date-nav="dateNav"
      @load-more="loadMore"
      @toggle-sort="toggleSort"
    />
  </div>
</template>

<script setup lang="ts">
import { ref, onMounted, watch } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import type { ChatMessage, ChatFilter } from '../../types/database'
import ChatMessageList from './ChatMessageList.vue'
import { useChatDateNav } from '../../composables/useChatDateNav'
import { useChatRequestGuard } from '../../composables/useChatRequestGuard'
import { useChatLiveRefresh } from '../../composables/useChatLiveRefresh'

const dateNav = useChatDateNav('only')
const reqGuard = useChatRequestGuard()

const messages = ref<ChatMessage[]>([])
const loading = ref(false)
const hasMore = ref(true)
const offset = ref(0)
const hasOlderPages = ref(false)
const searchText = ref('')
const sortOrder = ref<'asc' | 'desc'>('desc')
const LIMIT = 100

const { pendingCount, showNewest } = useChatLiveRefresh({ sortOrder, hasOlderPages, dateNav, reset })

let searchTimeout: number | null = null

async function loadMessages() {
  loading.value = true
  const generation = reqGuard.begin()
  try {
    const filter: ChatFilter = {
      ...dateNav.filterParams(),
      jsonScope: 'only',
      searchText: searchText.value || undefined,
      limit: LIMIT,
      offset: offset.value,
      sortOrder: sortOrder.value,
    }

    const newMessages = await invoke<ChatMessage[]>('get_chat_messages', filter)

    if (!reqGuard.isCurrent(generation)) return
    if (offset.value === 0) {
      messages.value = newMessages
    } else {
      messages.value = [...messages.value, ...newMessages]
    }

    hasMore.value = newMessages.length === LIMIT
    offset.value += newMessages.length
  } catch (e) {
    console.error('Failed to load messages:', e)
  } finally {
    if (reqGuard.isCurrent(generation)) loading.value = false
  }
}

function loadMore() {
  if (loading.value) return
  hasOlderPages.value = true
  loadMessages()
}

/** Restart the window at the top (manual reload / search / day / sort changes) */
function reset() {
  pendingCount.value = 0
  hasOlderPages.value = false
  offset.value = 0
  hasMore.value = true
  loadMessages()
}

function onSearchInput() {
  if (searchTimeout) clearTimeout(searchTimeout)
  searchTimeout = window.setTimeout(() => {
    reset()
  }, 300)
}

function toggleSort() {
  sortOrder.value = sortOrder.value === 'desc' ? 'asc' : 'desc'
  reset()
}

// Day filter changes restart the window at the top
watch(() => dateNav.activeDay.value, () => {
  reset()
})

onMounted(() => {
  dateNav.loadDays()
  loadMessages()
})
</script>
