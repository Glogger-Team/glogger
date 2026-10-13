<template>
  <div class="flex flex-col h-full" @keydown="onKeydown">
    <!-- Search header -->
    <div class="px-6 py-4 border-b border-border-default bg-surface-base">
      <div class="flex gap-2 items-center">
        <input
          ref="searchInput"
          type="text"
          v-model="rawQuery"
          @input="onSearchInput"
          placeholder="Search messages... (try from:player or in:channel)"
          class="flex-1 px-4 py-2 bg-surface-elevated border border-border-light rounded text-text-primary focus:outline-none focus:border-accent-gold"
        />
        <button
          @click="refresh"
          :disabled="loading"
          class="w-9 h-9 p-0 bg-surface-elevated border border-border-light rounded text-text-primary text-xl cursor-pointer transition-all flex items-center justify-center hover:bg-border-default hover:border-border-hover disabled:opacity-50 disabled:cursor-not-allowed"
          title="Refresh"
        >
          &#10227;
        </button>
      </div>

      <!-- Active filter chips -->
      <div v-if="parsed.sender || parsed.channel || parsed.textWords.length > 0" class="flex gap-2 mt-2 flex-wrap">
        <span
          v-for="(word, i) in parsed.rawTokens.length > 0 ? parsed.rawTokens : parsed.textWords"
          :key="'text-' + word + '-' + i"
          class="inline-flex items-center gap-1 px-2.5 py-1 bg-text-secondary/15 text-text-primary text-sm rounded-full"
        >
          {{ word }}
          <button
            @click="removeTextWord(word)"
            class="ml-0.5 w-4 h-4 flex items-center justify-center bg-transparent border-none text-text-muted cursor-pointer hover:text-text-primary text-xs leading-none"
          >&times;</button>
        </span>
        <span
          v-if="parsed.sender"
          class="inline-flex items-center gap-1 px-2.5 py-1 bg-accent-blue/15 text-accent-blue text-sm rounded-full"
        >
          from:{{ parsed.sender }}
          <button
            @click="removeOperator('from')"
            class="ml-0.5 w-4 h-4 flex items-center justify-center bg-transparent border-none text-accent-blue/60 cursor-pointer hover:text-accent-blue text-xs leading-none"
          >&times;</button>
        </span>
        <span
          v-if="parsed.channel"
          class="inline-flex items-center gap-1 px-2.5 py-1 bg-accent-gold/15 text-accent-gold text-sm rounded-full"
        >
          in:{{ parsed.channel }}
          <button
            @click="removeOperator('in')"
            class="ml-0.5 w-4 h-4 flex items-center justify-center bg-transparent border-none text-accent-gold/60 cursor-pointer hover:text-accent-gold text-xs leading-none"
          >&times;</button>
        </span>
      </div>

      <!-- Day-log mode: the clicked entry in its real log; the query box above
           stays live, so refining and jumping to another result is one edit away -->
      <div v-if="dayLogMode" class="mt-2 flex items-center gap-3 text-xs text-text-muted flex-wrap">
        <span>
          Showing the full log for
          <span class="font-semibold text-accent-gold">{{ dateNav.activeDay.value }}</span>
          <span v-if="dayLogCount > 0"> — {{ dayLogCount.toLocaleString() }} messages</span>
        </span>
        <button
          @click="backToResults"
          class="px-3 py-1 bg-surface-elevated border border-border-light rounded text-text-secondary text-xs cursor-pointer hover:bg-surface-hover hover:text-text-primary transition-all"
        >
          &larr; Back to results
        </button>
      </div>

      <!-- Result count -->
      <div v-else-if="resultCount !== null" class="mt-2 text-xs text-text-muted">
        {{ resultCount.toLocaleString() }} matching message{{ resultCount === 1 ? '' : 's' }} {{ dateNav.activeDay.value ? `on ${dateNav.activeDay.value}` : 'across all history' }}
      </div>
    </div>

    <!-- Results / day log -->
    <ChatMessageList
      ref="listRef"
      :messages="messages"
      :loading="loading"
      :has-more="hasMore"
      :has-newer="hasNewer"
      :show-channel="true"
      :sort-order="sortOrder"
      :clickable="!dayLogMode"
      :highlight-id="jumpTargetId ?? undefined"
      :highlight-terms="parsed.termKinds"
      :date-nav="dateNav"
      :window-start="offset"
      :window-total="dayLogMode ? dayLogCount : (resultCount ?? undefined)"
      @load-more="loadMore"
      @load-newer="loadNewer"
      @toggle-sort="toggleSort"
      @message-click="onMessageClick"
    />
  </div>
</template>

<script setup lang="ts">
import { ref, computed, onMounted, nextTick, watch } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import type { ChatMessage, ChatFilter } from '../../types/database'
import ChatMessageList from './ChatMessageList.vue'
import { parseSearchQuery } from '../../utils/parseSearchQuery'
import { useChatDateNav } from '../../composables/useChatDateNav'

const rawQuery = ref('')
const messages = ref<ChatMessage[]>([])
const loading = ref(false)
const hasMore = ref(true)
const hasNewer = ref(false)
/** Index (within the current window's query) of the FIRST loaded row */
const offset = ref(0)
const sortOrder = ref<'asc' | 'desc'>('desc')
const searchInput = ref<HTMLInputElement>()
const listRef = ref<InstanceType<typeof ChatMessageList>>()
const resultCount = ref<number | null>(null)
const LIMIT = 100

// Date navigation
const dateNav = useChatDateNav()

// Day-log mode: clicking a result opens the entry inside its day's real log
// (channel colours, rows on both sides, pagination that does not dead-end)
// instead of a fixed context window. The query box stays live above it.
const dayLogMode = ref(false)
const dayLogCount = ref(0)
const jumpTargetId = ref<number | null>(null)
let jumpTarget: ChatMessage | null = null
/** Day selection to restore when leaving day-log mode */
let preJumpDay: string | null = null
/** True while this view sets the day itself (jump / back to results), so the
 *  day watcher does not start a competing load */
let selfSelectingDay = false

let searchTimeout: number | null = null
// Query generation: bumped ONLY when the filter/query changes (page-0
// load). Ordinary pagination must NOT bump it — if it did, a page-2 start
// racing the page-0 count would discard the only count response and later
// pages never request another one, leaving the label empty. Query and search
// generations are tracked separately.
let queryGeneration = 0
let searchGeneration = 0

const parsed = computed(() => parseSearchQuery(rawQuery.value))

/** Result-list filter: parsed query (text/sender/channel) + day filter */
function resultFilter(): ChatFilter {
  const p = parsed.value
  return {
    searchText: p.text || undefined,
    sender: p.sender || undefined,
    channel: p.channel || undefined,
    ...dateNav.filterParams(),
  }
}

function fetchPage(filter: ChatFilter, start: number, limit = LIMIT): Promise<ChatMessage[]> {
  return invoke<ChatMessage[]>('get_chat_messages', {
    ...filter,
    limit,
    offset: start,
    sortOrder: sortOrder.value,
  })
}

// ─── Results mode ────────────────────────────────────────────────

async function loadResults(reset: boolean) {
  loading.value = true
  if (reset) {
    offset.value = 0
    resultCount.value = null
  }
  const countGeneration = reset ? ++queryGeneration : queryGeneration
  const generation = ++searchGeneration
  const start = reset ? 0 : offset.value + messages.value.length
  // ONE filter for the page and its count: re-deriving it after the await could
  // ask for the count of a query the rendered rows do not belong to.
  const filter = resultFilter()
  try {
    const rows = await fetchPage(filter, start)
    if (generation !== searchGeneration) return
    messages.value = reset ? rows : [...messages.value, ...rows]
    // The known total, when we have it, is authoritative for the end-of-window
    // state — `rows.length === LIMIT` keeps "more below" on a full last page.
    const known = resultCount.value
    hasMore.value = known !== null
      ? offset.value + messages.value.length < known
      : rows.length === LIMIT
    if (reset) {
      // Count over the full filter (no limit/offset)
      invoke<number>('count_chat_messages', filter)
        .then(n => {
          if (countGeneration !== queryGeneration) return
          resultCount.value = n
          hasMore.value = offset.value + messages.value.length < n
        })
        .catch(e => console.error('Failed to count messages:', e))
    }
  } catch (e) {
    console.error('Failed to search messages:', e)
  } finally {
    if (generation === searchGeneration) loading.value = false
  }
}

function exitDayLog() {
  dayLogMode.value = false
  dayLogCount.value = 0
  jumpTargetId.value = null
  jumpTarget = null
  hasNewer.value = false
}

/** Fresh result list for the current query + day filter */
function reloadResults() {
  exitDayLog()
  loadResults(true)
}

/** Leave the day log and return to the result list the jump started from */
function backToResults() {
  const day = preJumpDay
  exitDayLog()
  if (dateNav.activeDay.value !== day) {
    selfSelectingDay = true
    dateNav.jumpToDay(day)
    void nextTick().then(() => { selfSelectingDay = false })
  }
  loadResults(true)
}

// ─── Day-log mode ────────────────────────────────────────────────

/** Open the clicked entry inside its day's log, centred so the rows on both
 *  sides are the real neighbours rather than a fixed ±N sample. */
async function onMessageClick(msg: ChatMessage) {
  if (!dayLogMode.value) preJumpDay = dateNav.activeDay.value
  const day = dateNav.dayOf(msg.timestamp)
  dayLogMode.value = true
  jumpTarget = msg
  jumpTargetId.value = msg.id
  dayLogCount.value = 0
  messages.value = []
  offset.value = 0
  hasMore.value = true
  hasNewer.value = false
  if (dateNav.activeDay.value !== day) {
    selfSelectingDay = true
    dateNav.jumpToDay(day)
    await nextTick()
    selfSelectingDay = false
  }
  await seekToTarget(msg)
}

/** Load a window centred on `msg` within its day, in the current sort order. */
async function seekToTarget(msg: ChatMessage) {
  const day = dateNav.dayOf(msg.timestamp)
  const bounds = dateNav.boundsForDay(day)
  loading.value = true
  const generation = ++searchGeneration
  try {
    // Rows at or before the target's timestamp place it in the window for
    // either sort direction. Rows sharing its exact second (`group`) can be
    // several at once, so the window centres on that run rather than on its
    // first row — a wide run would otherwise push the target out of the page.
    const [total, upTo, group] = await Promise.all([
      invoke<number>('count_chat_messages', { ...bounds }),
      invoke<number>('count_chat_messages', { ...bounds, endTime: msg.timestamp }),
      invoke<number>('count_chat_messages', {
        ...bounds,
        startTime: msg.timestamp,
        endTime: msg.timestamp,
      }),
    ])
    if (generation !== searchGeneration) return
    const runSpan = Math.max(1, Math.min(group, LIMIT))
    const runStart = sortOrder.value === 'desc' ? total - upTo : upTo - group
    const start = Math.max(0, Math.min(
      runStart - Math.floor((LIMIT - runSpan) / 2),
      Math.max(0, total - LIMIT),
    ))
    const rows = await fetchPage(bounds, start)
    if (generation !== searchGeneration) return
    dayLogCount.value = total
    messages.value = rows
    offset.value = start
    hasMore.value = start + rows.length < total
    hasNewer.value = start > 0
  } catch (e) {
    console.error('Failed to open the message in its day log:', e)
  } finally {
    if (generation === searchGeneration) loading.value = false
  }
  await nextTick()
  listRef.value?.focus()
}

/** Start of the selected day (day change while browsing a day log) */
async function loadDayLogStart() {
  jumpTarget = null
  jumpTargetId.value = null
  const bounds = dateNav.filterParams()
  loading.value = true
  const generation = ++searchGeneration
  try {
    const [rows, total] = await Promise.all([
      fetchPage(bounds, 0),
      invoke<number>('count_chat_messages', { ...bounds }),
    ])
    if (generation !== searchGeneration) return
    dayLogCount.value = total
    messages.value = rows
    offset.value = 0
    hasMore.value = rows.length < total
    hasNewer.value = false
  } catch (e) {
    console.error('Failed to load the day log:', e)
  } finally {
    if (generation === searchGeneration) loading.value = false
  }
}

async function loadDayLogMore() {
  const bounds = dateNav.filterParams()
  const start = offset.value + messages.value.length
  loading.value = true
  const generation = ++searchGeneration
  try {
    const rows = await fetchPage(bounds, start)
    if (generation !== searchGeneration) return
    messages.value = [...messages.value, ...rows]
    hasMore.value = rows.length > 0 && start + rows.length < dayLogCount.value
  } catch (e) {
    console.error('Failed to load the day log:', e)
  } finally {
    if (generation === searchGeneration) loading.value = false
  }
}

async function loadDayLogNewer() {
  const bounds = dateNav.filterParams()
  const start = Math.max(0, offset.value - LIMIT)
  // Fetch exactly the rows missing above the window. Clamping at row 0 must not
  // re-fetch rows the window already holds: that duplicated them in the list and
  // left `offset` at 0 with an oversized window, so the next append resumed past
  // the real end of the window and stranded the rows in between.
  const missing = offset.value - start
  if (missing <= 0) return
  loading.value = true
  const generation = ++searchGeneration
  try {
    const rows = await fetchPage(bounds, start, missing)
    if (generation !== searchGeneration) return
    messages.value = [...rows, ...messages.value]
    offset.value = start
    hasNewer.value = rows.length > 0 && start > 0
    hasMore.value = start + messages.value.length < dayLogCount.value
  } catch (e) {
    console.error('Failed to load the day log:', e)
  } finally {
    if (generation === searchGeneration) loading.value = false
  }
}

// ─── Toolbar / pagination plumbing ───────────────────────────────

function loadMore() {
  if (loading.value) return
  if (dayLogMode.value) void loadDayLogMore()
  else void loadResults(false)
}

function loadNewer() {
  if (loading.value || !dayLogMode.value) return
  void loadDayLogNewer()
}

function refresh() {
  if (dayLogMode.value) void (jumpTarget ? seekToTarget(jumpTarget) : loadDayLogStart())
  else reloadResults()
}

function onSearchInput() {
  if (searchTimeout) clearTimeout(searchTimeout)
  searchTimeout = window.setTimeout(() => {
    reloadResults()
  }, 300)
}

async function toggleSort() {
  sortOrder.value = sortOrder.value === 'desc' ? 'asc' : 'desc'
  // Re-centre on the entry we jumped to; a plain day log restarts at its top
  if (dayLogMode.value && jumpTarget) await seekToTarget(jumpTarget)
  else refresh()
}

function removeTextWord(word: string) {
  // Remove the token exactly as it appears in the query. `\b` boundaries
  // fail on tokens starting/ending with non-word chars (quotes, `*`), so
  // match the escaped token between whitespace boundaries instead.
  const escaped = word.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')
  rawQuery.value = rawQuery.value
    .replace(new RegExp(`(?:^|(?<=\\s))${escaped}(?:(?=\\s)|$)`, 'i'), '')
    .trim()
    .replace(/\s+/g, ' ')
    .replace(/""/g, '')
  reloadResults()
}

function removeOperator(op: 'from' | 'in') {
  // Remove the operator from the raw query string
  const pattern = op === 'from'
    ? /\bfrom:(?:"[^"]*"|[\S]+)/gi
    : /\bin:(?:"[^"]*"|[\S]+)/gi
  rawQuery.value = rawQuery.value.replace(pattern, '').trim().replace(/\s+/g, ' ')
  reloadResults()
}

/** PageUp/PageDown reach the log even while the query box has focus; the list
 *  handles them itself (and stops propagation) when it holds focus. */
function onKeydown(e: KeyboardEvent) {
  if (e.key !== 'PageDown' && e.key !== 'PageUp') return
  const el = e.target as HTMLElement | null
  // Day picker / date input keep their own key behaviour
  if (el && (el.tagName === 'SELECT' || (el as HTMLInputElement).type === 'date')) return
  e.preventDefault()
  listRef.value?.pageScroll(e.key === 'PageDown' ? 1 : -1)
}

// Day filter changes reload: the day's log while browsing one, otherwise the
// (query-filtered) result list
watch(() => dateNav.activeDay.value, (day, prev) => {
  if (day === prev || selfSelectingDay) return
  hasNewer.value = false
  if (day && dayLogMode.value) void loadDayLogStart()
  else reloadResults()
})

onMounted(() => {
  loadResults(true)
  dateNav.loadDays()
  nextTick(() => searchInput.value?.focus())
})
</script>
