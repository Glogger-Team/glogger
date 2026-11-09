<template>
  <div
    class="flex gap-2 px-2 py-1 text-sm hover:bg-gray-800 rounded"
    :class="messageClass"
  >
    <span class="text-gray-500 flex-shrink-0 text-xs">
      {{ formatTime(message.timestamp) }}
    </span>
    <span
      v-if="message.channel"
      class="flex-shrink-0 font-semibold"
      :class="channelClass"
    >
      [{{ message.channel }}]
    </span>
    <span v-if="message.sender" class="flex-shrink-0 font-semibold text-sender">
      {{ message.sender }}:
    </span>
    <span class="flex-1 break-words" :class="messageTextClass">
      <template v-if="message.message && message.item_links && message.item_links.length > 0">
        <MessageWithItemLinks :message="message.message" :item-links="message.item_links" />
      </template>
      <template v-else-if="message.message">
        {{ message.message }}
      </template>
      <template v-else>
        <span class="text-gray-500 italic">(empty message)</span>
      </template>
    </span>
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import type { ChatMessage } from '../../types/database'
import MessageWithItemLinks from './MessageWithItemLinks.vue'
import { formatTimeFull } from '../../composables/useTimestamp'

const props = defineProps<{
  message: ChatMessage
}>()

function formatTime(timestamp: string): string {
  return formatTimeFull(timestamp)
}

const messageClass = computed(() => {
  if (props.message.is_system) {
    return 'opacity-75'
  }
  return ''
})

const messageTextClass = computed(() => {
  if (props.message.is_system) {
    return 'text-gray-300 italic'
  }
  return 'text-gray-100'
})

const channelClass = computed(() => {
  const channel = props.message.channel?.toLowerCase()

  const channelColors: Record<string, string> = {
    'global': 'text-channel-global',
    'trade': 'text-channel-trade',
    'help': 'text-channel-help',
    'guild': 'text-channel-guild',
    'nearby': 'text-channel-nearby',
    'status': 'text-channel-status',
    'combat': 'text-channel-combat',
    'lfg': 'text-channel-lfg',
  }

  return channelColors[channel || ''] || 'text-text-muted'
})
</script>
