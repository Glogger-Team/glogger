<template>
  <div class="flex h-full">
    <div class="w-62.5 bg-surface-dark border-r border-border-default flex flex-col">
      <div class="p-4 border-b border-border-default">
        <h3 class="m-0 text-accent-gold text-lg font-semibold">Conversations</h3>
      </div>
      <div class="flex-1 overflow-y-auto p-2">
        <button
          v-for="conv in conversations"
          :key="conv.name"
          class="w-full flex justify-between items-center px-4 py-3 bg-transparent border-none rounded cursor-pointer transition-all mb-1"
          :class="selectedConversation === conv.name
            ? 'bg-surface-elevated text-accent-gold'
            : 'text-text-secondary hover:bg-surface-base hover:text-text-primary/70'"
          @click="selectConversation(conv.name)"
        >
          <span class="font-medium">{{ conv.name }}</span>
          <span class="text-sm text-text-muted font-mono">{{ conv.count }}</span>
        </button>
        <div v-if="conversations.length === 0" class="py-8 px-4 text-center text-text-muted">
          <p class="my-1">No conversations yet</p>
          <p class="text-sm text-text-dim">Import chat logs to see tell history</p>
        </div>
      </div>
    </div>
    <div class="flex-1 flex flex-col overflow-hidden">
      <div v-if="selectedConversation" class="px-6 py-4 border-b border-border-default flex justify-between items-center bg-surface-base">
        <h2 class="screen-title m-0">{{ selectedConversation }}</h2>
        <div class="flex gap-2">
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
      <div v-if="!selectedConversation" class="flex-1 flex flex-col items-center justify-center text-text-muted bg-surface-base">
        <p class="my-1">Select a conversation to view messages</p>
      </div>
      <ChatMessageList
        v-else
        :messages="messages"
        :loading="loading"
        :has-more="hasMore"
        :sort-order="sortOrder"
        :date-nav="dateNav"
        @load-more="loadMore"
        @toggle-sort="toggleSort"
      />
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, onMounted, watch } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import type { ChatMessage, ChatFilter } from '../../types/database'
import ChatMessageList from './ChatMessageList.vue'
import { useChatDateNav } from '../../composables/useChatDateNav'
import { useChatRequestGuard } from '../../composables/useChatRequestGuard'

const dateNav = useChatDateNav()
const reqGuard = useChatRequestGuard()

interface Conversation {
  name: string
  count: number
}

const selectedConversation = ref<string | null>(null)
const messages = ref<ChatMessage[]>([])
const conversations = ref<Conversation[]>([])
const loading = ref(false)
const hasMore = ref(true)
const offset = ref(0)
const sortOrder = ref<'asc' | 'desc'>('desc')
const LIMIT = 100

async function loadConversations() {
  try {
    const convs = await invoke<Array<{ channel: string, count: number }>>('get_tell_conversations')
    conversations.value = convs.map(c => ({ name: c.channel, count: c.count }))
  } catch (e) {
    console.error('Failed to load conversations:', e)
  }
}

async function selectConversation(name: string) {
  selectedConversation.value = name
  messages.value = []
  offset.value = 0
  hasMore.value = true
  await loadMessages()
}

async function loadMessages() {
  if (!selectedConversation.value) return

  loading.value = true
  const generation = reqGuard.begin()
  try {
    const filter: ChatFilter = {
      ...dateNav.filterParams(),
      tellPartner: selectedConversation.value,
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
  loadMessages()
}

/** Restart the window at the top (refresh / day / sort changes) */
function reset() {
  if (!selectedConversation.value) return
  offset.value = 0
  hasMore.value = true
  loadMessages()
}

function toggleSort() {
  sortOrder.value = sortOrder.value === 'desc' ? 'asc' : 'desc'
  reset()
}

// Day filter changes reload from the day boundary
watch(() => dateNav.activeDay.value, () => {
  reset()
})

onMounted(() => {
  dateNav.loadDays()
  loadConversations()
})
</script>
