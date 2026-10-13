<template>
  <div class="flex flex-col h-full bg-surface-base">
    <!-- Date navigation toolbar (always visible while date nav is active) -->
    <div v-if="dateNav" class="flex items-center gap-2 px-4 py-2 border-b border-border-default bg-surface-base flex-wrap">
      <label class="text-xs text-text-muted" for="chat-day-select">Day</label>
      <select
        id="chat-day-select"
        :value="dateNav.activeDay.value ?? ''"
        @change="onDaySelect(($event.target as HTMLSelectElement).value)"
        class="px-2 py-1 bg-surface-elevated border border-border-light rounded text-text-primary text-xs cursor-pointer focus:outline-none focus:border-accent-gold max-w-45"
      >
        <option value="">All history</option>
        <option v-for="d in dateNav.days.value" :key="d.day" :value="d.day">
          {{ d.day }} ({{ d.count }})
        </option>
      </select>
      <input
        type="date"
        :value="dateNav.activeDay.value ?? ''"
        :min="minDay"
        :max="maxDay"
        @change="onDayInput(($event.target as HTMLInputElement).value)"
        class="px-2 py-1 bg-surface-elevated border border-border-light rounded text-text-primary text-xs cursor-pointer focus:outline-none focus:border-accent-gold"
        title="Jump to a specific day"
      />
      <template v-if="dateNav.isPastView.value">
        <button
          @click="dateNav.stepDay(-1)"
          :disabled="!dateNav.canStepDay(-1)"
          class="px-2 py-1 bg-surface-elevated border border-border-light rounded text-text-primary text-xs cursor-pointer hover:bg-border-default disabled:opacity-40 disabled:cursor-not-allowed"
          title="Newer day"
        >&#9664;</button>
        <span class="text-xs font-semibold text-accent-gold">{{ dateNav.activeDay.value }}</span>
        <button
          @click="dateNav.stepDay(1)"
          :disabled="!dateNav.canStepDay(1)"
          class="px-2 py-1 bg-surface-elevated border border-border-light rounded text-text-primary text-xs cursor-pointer hover:bg-border-default disabled:opacity-40 disabled:cursor-not-allowed"
          title="Older day"
        >&#9654;</button>
        <button
          @click="dateNav.jumpToDay(null)"
          class="px-3 py-1 bg-accent-gold/90 text-surface-base text-xs font-semibold rounded-full cursor-pointer hover:bg-accent-gold transition-all"
          title="Clear the day filter and return to live browsing"
        >
          &#8593; Back to Live
        </button>
      </template>
      <!-- Sort order: kept here (not in the scrolling list) so it stays
           reachable once a day is several pages long -->
      <button
        v-if="sortOrder"
        @click="emit('toggle-sort')"
        class="ml-auto px-3 py-1 bg-surface-elevated border border-border-default text-text-secondary text-xs rounded cursor-pointer hover:bg-surface-hover hover:text-text-primary transition-all"
        :title="sortOrder === 'desc' ? 'Showing newest first — click for oldest first' : 'Showing oldest first — click for newest first'"
      >
        {{ sortOrder === 'desc' ? '&#9660; Newest first' : '&#9650; Oldest first' }}
      </button>
    </div>
    <!-- Initial loading (no messages yet) -->
    <div v-if="loading && messages.length === 0" class="flex flex-col items-center justify-center flex-1 text-text-muted">
      <div class="w-10 h-10 border-3 border-border-default border-t-accent-gold rounded-full animate-spin mb-4"></div>
      <p>Loading messages...</p>
    </div>
    <div v-else-if="!loading && messages.length === 0" class="flex flex-col items-center justify-center flex-1 text-text-muted">
      <p class="my-1">No messages found</p>
      <p class="text-sm text-text-dim">Try importing chat logs from the Management tab</p>
    </div>
    <div
      v-else
      class="relative flex-1 overflow-y-auto p-4 focus:outline-none"
      ref="messagesContainer"
      tabindex="0"
      @scroll="onScroll"
      @keydown="onKeydown"
    >
        <!-- Jump to present -->
        <div class="sticky top-0 z-10 flex justify-end items-center pb-2">
          <button
            v-if="showJumpToPresent"
            @click="jumpToPresent"
            class="px-4 py-1 bg-accent-gold/90 text-surface-base text-xs font-semibold rounded-full shadow-lg cursor-pointer hover:bg-accent-gold transition-all"
          >
            &#8593; Jump to Present
          </button>
        </div>
        <!-- Newer/older rows exist above the loaded window (day-log jumps land
             mid-day): streaming them in keeps the scroll from dead-ending -->
        <div v-if="hasNewer" class="py-2 text-center">
          <button
            v-if="!loading"
            @click="requestLoadNewer"
            class="px-6 py-2 bg-surface-elevated border border-border-light text-text-primary rounded cursor-pointer transition-all hover:bg-border-default hover:border-border-hover text-sm"
          >
            &#9650; Load {{ upDirection }} messages
          </button>
          <span v-else class="inline-flex items-center text-text-muted text-sm">
            <span class="w-5 h-5 border-2 border-border-default border-t-accent-gold rounded-full animate-spin mr-2"></span>
            Loading {{ upDirection }} messages...
          </span>
        </div>
        <!-- Tell conversation layout -->
        <template v-if="isTellView">
          <template v-for="(group, gi) in dayGroups" :key="'g-' + gi">
            <div v-if="group.day" class="text-center my-3">
              <span class="px-3 py-1 bg-surface-elevated border border-border-light rounded-full text-xs font-semibold text-text-secondary">
                {{ group.day }}
              </span>
            </div>
            <div
              v-for="msg in group.messages"
              :key="msg.id"
              class="flex mb-1"
              :class="msg.from_player ? 'justify-end' : 'justify-start'"
            >
              <div
                class="max-w-[75%] rounded-lg px-3 py-2 leading-relaxed"
                :class="msg.from_player
                  ? 'bg-accent-blue/15 rounded-br-sm'
                  : 'bg-surface-elevated rounded-bl-sm'"
              >
                <div class="flex items-baseline gap-2 mb-0.5">
                  <span class="text-xs font-semibold" :class="msg.from_player ? 'text-accent-blue' : 'text-accent-gold'">
                    {{ msg.from_player ? 'You' : msg.sender || 'Unknown' }}
                  </span>
                  <span class="text-text-dim text-xs">{{ formatTime(msg.timestamp) }}</span>
                </div>
                <span class="text-text-primary text-sm break-words">
                  <MessageWithItemLinks v-if="msg.item_links && msg.item_links.length > 0" :message="msg.message" :item-links="msg.item_links" :highlight-terms="highlightTerms ?? []" />
                  <ChatHighlighted v-else-if="(highlightTerms ?? []).length > 0" :message="msg.message" :terms="highlightTerms ?? []" />
                  <template v-else>{{ msg.message }}</template>
                </span>
              </div>
            </div>
          </template>
        </template>
        <!-- Standard channel layout -->
        <template v-else>
          <template v-for="(group, gi) in dayGroups" :key="'g-' + gi">
            <div v-if="group.day" class="text-center my-3">
              <span class="px-3 py-1 bg-surface-elevated border border-border-light rounded-full text-xs font-semibold text-text-secondary">
                {{ group.day }}
              </span>
            </div>
            <div
              v-for="msg in group.messages"
              :key="msg.id"
              :ref="(el) => { if (highlightId === msg.id) highlightEl = el as HTMLElement }"
              class="flex gap-2 p-2 mb-1 rounded leading-relaxed transition-colors hover:bg-surface-card"
              :class="{
                'opacity-70': msg.is_system,
                'bg-accent-blue/5 hover:bg-accent-blue/10': msg.from_player === true && highlightId !== msg.id,
                'cursor-pointer': clickable,
                'bg-accent-gold/10 border-l-2 border-accent-gold': highlightId === msg.id,
              }"
              @click="clickable ? emit('message-click', msg) : undefined"
            >
              <span class="shrink-0 whitespace-nowrap text-text-muted text-sm">{{ formatTime(msg.timestamp) }}</span>
              <span
                v-if="showChannel && msg.channel"
                class="shrink-0 font-semibold text-sm"
                :class="channelColorClass(msg.channel)"
              >
                [{{ msg.channel }}]
              </span>
              <span v-if="msg.sender" class="shrink-0 text-sender font-medium">{{ formatSender(msg) }}:</span>
              <span class="flex-1 text-text-primary break-words" :class="{ 'text-text-system italic': msg.is_system }">
                <MessageWithItemLinks v-if="msg.item_links && msg.item_links.length > 0" :message="msg.message" :item-links="msg.item_links" :highlight-terms="highlightTerms ?? []" />
                <ChatHighlighted v-else-if="(highlightTerms ?? []).length > 0" :message="msg.message" :terms="highlightTerms ?? []" />
                <template v-else>{{ msg.message }}</template>
              </span>
            </div>
          </template>
        </template>
        <!-- Inline loading indicator for pagination -->
        <div v-if="loading && messages.length > 0" class="flex items-center justify-center py-4 text-text-muted">
          <div class="w-6 h-6 border-2 border-border-default border-t-accent-gold rounded-full animate-spin mr-3"></div>
          <span class="text-sm">Loading more messages...</span>
        </div>
        <!-- Manual load more fallback (in case scroll detection misses) -->
        <div v-else-if="hasMore && !loading" class="py-4 text-center">
          <button
            @click="requestLoadMore"
            class="px-6 py-2 bg-surface-elevated border border-border-light text-text-primary rounded cursor-pointer transition-all hover:bg-border-default hover:border-border-hover text-sm"
          >
            Load More Messages
          </button>
        </div>
      </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, watch, nextTick } from 'vue'
import type { ChatMessage } from '../../types/database'
import MessageWithItemLinks from './MessageWithItemLinks.vue'
import ChatHighlighted, { type HighlightTerm } from './ChatHighlighted.vue'
import { formatSmart, getDisplayOffsetSeconds } from '../../composables/useTimestamp'
import { displayDayOf } from '../../utils/displayDay'
import type { ChatDateNav } from '../../composables/useChatDateNav'

const props = defineProps<{
  messages: ChatMessage[]
  loading: boolean
  showChannel?: boolean
  hasMore?: boolean
  /** Rows newer than the loaded window exist (set when a day jump lands mid-day) */
  hasNewer?: boolean
  autoScroll?: boolean
  sortOrder?: 'asc' | 'desc'
  clickable?: boolean
  highlightId?: number
  /** Search terms highlighted in message bodies (FTS results) */
  highlightTerms?: HighlightTerm[]
  /** Date navigation state; omit to hide the date toolbar entirely */
  dateNav?: ChatDateNav
}>()

const isTellView = computed(() => {
  return props.messages.length > 0
    && props.messages[0].channel === 'Tell'
    && !props.showChannel
})

/** Messages grouped by display-timezone day; `day` is null for the first
 *  group when the list spans only one day (no header needed for live
 *  browsing). */
const dayGroups = computed(() => {
  const offset = getDisplayOffsetSeconds()
  const groups: Array<{ day: string | null, messages: ChatMessage[] }> = []
  let current: { day: string | null, messages: ChatMessage[] } | null = null
  let currentDay: string | null = null
  let anyDayChange = false
  for (const msg of props.messages) {
    const day = displayDayOf(msg.timestamp, offset)
    if (!current || day !== currentDay) {
      if (current && day !== currentDay) anyDayChange = true
      current = { day, messages: [] }
      groups.push(current)
      currentDay = day
    }
    current.messages.push(msg)
  }
  // Only show headers when the loaded window actually spans multiple days
  if (!anyDayChange && groups.length === 1) {
    groups[0].day = null
  }
  return groups
})

// Date picker bounds: only days that actually have messages
const minDay = computed(() =>
  props.dateNav && props.dateNav.days.value.length > 0
    ? props.dateNav.days.value[props.dateNav.days.value.length - 1].day
    : undefined
)
const maxDay = computed(() =>
  props.dateNav && props.dateNav.days.value.length > 0
    ? props.dateNav.days.value[0].day
    : undefined
)

const emit = defineEmits<{
  'load-more': []
  'load-newer': []
  'toggle-sort': []
  'message-click': [msg: ChatMessage]
}>()

const messagesContainer = ref<HTMLElement>()
const highlightEl = ref<HTMLElement | null>(null)
const showJumpToPresent = ref(false)

let loadMoreEmitted = false
let loadNewerEmitted = false

/** Top of the list holds newer rows when sorting newest-first, older when
 *  sorting oldest-first. */
const upDirection = computed(() => (props.sortOrder === 'asc' ? 'older' : 'newer'))

function requestLoadMore() {
  if (props.loading || !props.hasMore) return
  loadMoreEmitted = true
  emit('load-more')
}

function requestLoadNewer() {
  if (props.loading || !props.hasNewer) return
  loadNewerEmitted = true
  emit('load-newer')
}

function onScroll() {
  if (!messagesContainer.value) return

  const el = messagesContainer.value

  // Show "Jump to Present" when scrolled more than 300px from the top
  showJumpToPresent.value = el.scrollTop > 300

  if (props.loading) return
  const distanceFromBottom = el.scrollHeight - el.scrollTop - el.clientHeight

  // Trigger load when within 200px of either edge
  if (props.hasMore && !loadMoreEmitted && distanceFromBottom < 200) {
    loadMoreEmitted = true
    emit('load-more')
  }
  if (props.hasNewer && !loadNewerEmitted && el.scrollTop < 200) {
    requestLoadNewer()
  }
}

// Reset emit guards when loading state changes
watch(() => props.loading, (isLoading) => {
  if (!isLoading) {
    loadMoreEmitted = false
    loadNewerEmitted = false
  }
})

function pageScroll(dir: 1 | -1) {
  const el = messagesContainer.value
  if (!el) return
  el.scrollBy({ top: dir * Math.max(120, el.clientHeight * 0.9) })
}

/** Focus the log so PageUp/PageDown drive it (used after a jump) */
function focus() {
  messagesContainer.value?.focus()
}

// PageUp/PageDown/Home/End drive the list while it has focus (holding a page
// key streams pages through the edge auto-loaders). stopPropagation keeps a
// parent (ChatSearchView) from double-handling the same keystroke.
function onKeydown(e: KeyboardEvent) {
  const el = messagesContainer.value
  if (!el) return
  if (e.key === 'PageDown') pageScroll(1)
  else if (e.key === 'PageUp') pageScroll(-1)
  else if (e.key === 'Home') el.scrollTo({ top: 0 })
  else if (e.key === 'End') el.scrollTo({ top: el.scrollHeight })
  else return
  e.preventDefault()
  e.stopPropagation()
}

function jumpToPresent() {
  if (!messagesContainer.value) return
  messagesContainer.value.scrollTo({ top: 0, behavior: 'smooth' })
}

function onDaySelect(value: string) {
  props.dateNav?.jumpToDay(value || null)
}

function onDayInput(value: string) {
  if (!value) return
  // Clamp typed dates to the known day range (native min/max covers the picker;
  // typed entry is validated here).
  if (minDay.value && value < minDay.value) value = minDay.value
  if (maxDay.value && value > maxDay.value) value = maxDay.value
  props.dateNav?.jumpToDay(value)
}

function channelColorClass(channel: string): string {
  const map: Record<string, string> = {
    global: 'text-channel-global',
    trade: 'text-channel-trade',
    help: 'text-channel-help',
    guild: 'text-channel-guild',
    nearby: 'text-channel-nearby',
    status: 'text-channel-status',
    combat: 'text-channel-combat',
    lfg: 'text-channel-lfg',
    party: 'text-channel-party',
  }
  return map[channel.toLowerCase()] || 'text-text-secondary'
}

function formatTime(timestamp: string): string {
  return formatSmart(timestamp)
}

function formatSender(msg: ChatMessage): string {
  if (msg.channel === 'Tell' && msg.from_player !== null && msg.from_player !== undefined) {
    return msg.from_player ? 'YOU' : msg.sender || 'Unknown'
  }
  return msg.sender || 'Unknown'
}

watch(() => props.messages, async (next, prev) => {
  // Pre-flush: the DOM still holds the previous rows here, so this is the
  // height to re-anchor against when rows arrive above the window.
  const heightBefore = messagesContainer.value?.scrollHeight ?? 0
  await nextTick()
  const el = messagesContainer.value
  if (!el) return
  // Prepend (rows above the window) keeps the last row and grows the list;
  // a plain append keeps the first row. Anything else is a fresh window.
  const prepended = !!prev && prev.length > 0 && next.length > prev.length
    && next[0]?.id !== prev[0]?.id
    && next[next.length - 1]?.id === prev[prev.length - 1]?.id
  if (prepended) {
    el.scrollTop += el.scrollHeight - heightBefore
    return
  }
  if (props.autoScroll) {
    el.scrollTop = el.scrollHeight
    return
  }
  if (props.highlightId) {
    // Instant, not smooth: a smooth scroll fires intermediate scroll events
    // (scrollTop < 200 on the way up) that would trip the top edge auto-loader
    // and prepend a page right out from under the row being scrolled to.
    highlightEl.value?.scrollIntoView({ block: 'center' })
  }
})

// Day jumps/clears load a new window, but the scroll container keeps its old
// position. Reset to the top of the new window — correct for BOTH sort orders
// (desc = newest of the day, asc = oldest of the day). A jump highlighted to a
// specific row skips the reset: that row's own scroll-into-view positions the
// window. `post` flush so the guard sees the highlight prop in the same flush
// the jump set it (pre-flush watchers still hold the previous props).
watch(() => props.dateNav?.activeDay.value, async () => {
  if (!props.dateNav || !messagesContainer.value) return
  await nextTick()
  if (props.highlightId) return
  messagesContainer.value.scrollTop = 0
}, { flush: 'post' })

defineExpose({ pageScroll, focus })
</script>
