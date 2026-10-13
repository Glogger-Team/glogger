<template>
  <div class="flex h-full">
    <div class="w-62.5 bg-surface-dark border-r border-border-default flex flex-col">
      <div class="p-4 border-b border-border-default">
        <h3 class="m-0 text-accent-gold text-lg font-semibold">Channels</h3>
      </div>
      <div class="flex-1 overflow-y-auto p-2">
        <button
          v-for="channel in displayChannels"
          :key="channel.name"
          class="w-full flex justify-between items-center px-4 py-3 bg-transparent border-none rounded cursor-pointer transition-all mb-1"
          :class="selectedChannel === channel.name
            ? 'bg-surface-elevated text-accent-gold'
            : 'text-text-secondary hover:bg-surface-base hover:text-text-primary/70'"
          @click="selectChannel(channel.name)"
        >
          <span class="font-medium">{{ channel.name }}</span>
          <span class="text-sm text-text-muted font-mono">{{ channel.count }}</span>
        </button>
      </div>
    </div>
    <div class="flex-1 flex flex-col overflow-hidden">
      <div class="px-6 py-4 border-b border-border-default flex justify-between items-center bg-surface-base">
        <h2 class="screen-title m-0">{{ selectedChannel || 'Select a Channel' }}</h2>
        <div class="flex gap-2 items-center">
          <input
            v-if="selectedChannel"
            type="text"
            v-model="searchText"
            @input="onSearchInput"
            placeholder="Search in channel..."
            class="px-4 py-2 bg-surface-elevated border border-border-light rounded text-text-primary w-62.5 focus:outline-none focus:border-accent-gold"
          />
          <button
            v-if="selectedChannel"
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
import type { ChatMessage, ChatFilter, ChannelStat } from '../../types/database'
import ChatMessageList from './ChatMessageList.vue'
import { useChatDateNav } from '../../composables/useChatDateNav'
import { useChatRequestGuard } from '../../composables/useChatRequestGuard'

const dateNav = useChatDateNav()
const reqGuard = useChatRequestGuard()

const selectedChannel = ref<string | null>(null)
const messages = ref<ChatMessage[]>([])
const channels = ref<ChannelStat[]>([])
const loading = ref(false)
const hasMore = ref(true)
const offset = ref(0)
const searchText = ref('')
const sortOrder = ref<'asc' | 'desc'>('desc')
const LIMIT = 100

let searchTimeout: number | null = null

const publicChannels = ['Global', 'Trade', 'Help', 'LFG']
const displayChannels = ref<Array<{ name: string, count: number }>>([])

async function loadChannels() {
  try {
    const stats = await invoke<ChannelStat[]>('get_chat_channel_stats')
    channels.value = stats

    const publicChan = stats.filter(c => publicChannels.includes(c.channel))
    const customChan = stats.filter(c =>
      !publicChannels.includes(c.channel) &&
      !['Status', 'Combat', 'Guild', 'Nearby', 'Party'].includes(c.channel)
    )

    displayChannels.value = [
      ...publicChan.map(c => ({ name: c.channel, count: c.count })),
      ...customChan.map(c => ({ name: c.channel, count: c.count }))
    ]
  } catch (e) {
    console.error('Failed to load channels:', e)
  }
}

async function selectChannel(channel: string) {
  selectedChannel.value = channel
  messages.value = []
  offset.value = 0
  searchText.value = ''
  hasMore.value = true
  await loadMessages()
}

async function loadMessages() {
  if (!selectedChannel.value) return

  loading.value = true
  const generation = reqGuard.begin()
  try {
    const filter: ChatFilter = {
      ...dateNav.filterParams(),
      channel: selectedChannel.value,
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
  loadMessages()
}

/** Restart the window at the top (refresh / search / day / sort changes) */
function reset() {
  if (!selectedChannel.value) return
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

// Day filter changes reload from the day boundary. Guard: without a
// selected channel there is nothing to scope to — the day toolbar is
// visible before any channel is chosen, and jumping days from that state
// would fill the "Select a Channel" placeholder with every channel's
// messages.
watch(() => dateNav.activeDay.value, () => {
  if (!selectedChannel.value) return
  reset()
})

onMounted(() => {
  dateNav.loadDays()
  loadChannels()
})
</script>
