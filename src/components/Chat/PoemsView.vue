<template>
  <div class="flex flex-col h-full">
    <!-- ══════════════ List mode ══════════════ -->
    <template v-if="selectedIndex === -1">
      <!-- Header + search -->
      <div class="px-6 py-4 border-b border-border-default bg-surface-base">
        <div class="flex justify-between items-center mb-3">
          <h2 class="screen-title m-0">
            Poems
            <span class="text-text-dim text-sm font-normal ml-1">({{ poems.length }})</span>
          </h2>
          <div class="flex items-center gap-2">
            <button
              @click="scanLog"
              :disabled="scanning"
              class="px-3 py-1.5 bg-surface-elevated border border-border-light rounded text-text-secondary text-sm cursor-pointer transition-all hover:bg-border-default hover:text-text-primary disabled:opacity-50 disabled:cursor-not-allowed whitespace-nowrap"
              title="Scan the full Player.log for poems recited before glogger started watching"
            >{{ scanning ? 'Scanning…' : 'Scan Player.log' }}</button>
            <button
              @click="refresh"
              :disabled="loading"
              class="w-9 h-9 p-0 bg-surface-elevated border border-border-light rounded text-text-primary text-xl cursor-pointer transition-all flex items-center justify-center hover:bg-border-default hover:border-border-hover disabled:opacity-50 disabled:cursor-not-allowed"
              title="Refresh"
            >&#10227;</button>
          </div>
        </div>
        <input
          type="text"
          v-model="searchQuery"
          placeholder="Search poems by title, author, or text..."
          class="w-full px-4 py-2 bg-surface-elevated border border-border-light rounded text-text-primary focus:outline-none focus:border-accent-gold"
        />
        <div v-if="scanMessage" class="mt-2 text-xs" :class="scanError ? 'text-value-negative' : 'text-text-dim'">
          {{ scanMessage }}
        </div>
      </div>

      <!-- Table -->
      <div class="flex-1 overflow-y-auto">
        <div v-if="loading" class="p-6 text-text-dim text-sm">Loading poems...</div>

        <div v-else-if="filteredPoems.length === 0" class="p-6 text-text-dim text-sm">
          <template v-if="poems.length === 0">
            <p class="italic mb-3">
              No poems recorded yet. Poems are captured automatically when another
              player recites one at a poetry podium while your Player.log is being watched.
            </p>
            <button
              @click="scanLog"
              :disabled="scanning"
              class="px-4 py-2 bg-surface-elevated border border-border-light rounded text-text-primary text-sm cursor-pointer transition-all hover:bg-border-default hover:border-border-hover disabled:opacity-50 disabled:cursor-not-allowed"
            >{{ scanning ? 'Scanning…' : 'Scan Player.log for past poems' }}</button>
          </template>
          <template v-else><span class="italic">No poems match "{{ searchQuery }}".</span></template>
        </div>

        <DataTable
          v-else
          :columns="columns"
          :rows="(filteredPoems as unknown as Record<string, unknown>[])"
          :hoverable="false"
          :sticky-header="true"
          :row-class="rowClassFn"
          row-clickable
          @row-click="({ index }) => openPoem(index)">
          <template #cell-title="{ row }">
            <div v-if="row.title" class="text-text-primary font-medium truncate" v-html="renderInline(row.title as string)"></div>
            <div v-else class="text-text-dim font-medium italic truncate">(untitled)</div>
            <div v-if="preview(row.content as string)" class="text-text-dim text-xs truncate mt-0.5">
              {{ preview(row.content as string) }}
            </div>
          </template>
          <template #cell-author="{ value }">
            <span class="text-accent-blue whitespace-nowrap">{{ value as string }}</span>
          </template>
          <template #cell-recorded_at="{ value }">
            <span class="text-text-dim font-mono text-xs whitespace-nowrap">
              {{ formatTs(value as string) }}
            </span>
          </template>
        </DataTable>
      </div>
    </template>

    <!-- ══════════════ Detail mode ══════════════ -->
    <template v-else-if="current">
      <!-- Detail header -->
      <div class="px-6 py-3 border-b border-border-default bg-surface-base flex items-center justify-between gap-3">
        <button
          @click="closePoem"
          class="px-3 py-1.5 bg-surface-elevated border border-border-light rounded text-text-secondary text-sm cursor-pointer transition-all hover:bg-border-default hover:text-text-primary flex items-center gap-1.5"
        >
          <span class="text-base leading-none">&#8592;</span> Back
        </button>
        <div class="flex items-center gap-2">
          <span class="text-text-dim text-xs font-mono">{{ selectedIndex + 1 }} / {{ filteredPoems.length }}</span>
          <button
            @click="prev"
            :disabled="selectedIndex <= 0"
            class="w-8 h-8 bg-surface-elevated border border-border-light rounded text-text-primary cursor-pointer transition-all flex items-center justify-center hover:bg-border-default disabled:opacity-40 disabled:cursor-not-allowed"
            title="Previous poem (↑)"
          >&#8593;</button>
          <button
            @click="next"
            :disabled="selectedIndex >= filteredPoems.length - 1"
            class="w-8 h-8 bg-surface-elevated border border-border-light rounded text-text-primary cursor-pointer transition-all flex items-center justify-center hover:bg-border-default disabled:opacity-40 disabled:cursor-not-allowed"
            title="Next poem (↓)"
          >&#8595;</button>
        </div>
      </div>

      <!-- Poem body -->
      <div class="flex-1 overflow-y-auto px-6 py-8">
        <div class="max-w-2xl mx-auto">
          <h1
            v-if="current.title"
            class="text-2xl font-semibold text-text-primary mb-1 leading-tight"
            v-html="renderInline(current.title)"
          ></h1>
          <h1 v-else class="text-2xl font-semibold text-text-dim italic mb-1">(untitled)</h1>

          <div class="flex items-center gap-2 text-sm text-text-dim mb-6 pb-4 border-b border-border-default">
            <span>by <span class="text-accent-blue font-medium">{{ current.author }}</span></span>
            <span class="text-text-muted">&middot;</span>
            <span class="font-mono text-xs">{{ formatTs(current.recorded_at) }}</span>
          </div>

          <div
            v-if="current.content"
            class="poem-body text-text-secondary text-[15px] leading-relaxed"
            v-html="renderPoem(current.content)"
          ></div>
          <div v-else class="text-text-dim italic text-sm">This poem had no text — just a title.</div>
        </div>
      </div>
    </template>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted, watch } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { formatDateTimeFull as formatTs } from '../../composables/useTimestamp'
import DataTable from '../Shared/DataTable.vue'

interface Poem {
  id: number
  author: string
  title: string
  content: string
  recorded_at: string
}

const columns = [
  { key: 'title', label: 'Title' },
  { key: 'author', label: 'Author' },
  { key: 'recorded_at', label: 'Recorded' },
]

/** Keep the original hover token (DataTable's default would use
 * surface-row-hover) — row-click/cursor-pointer come from `row-clickable`. */
function rowClassFn(): string {
  return 'transition-colors hover:bg-surface-elevated'
}

const poems = ref<Poem[]>([])
const loading = ref(false)
const searchQuery = ref('')
const selectedIndex = ref(-1)
const scanning = ref(false)
const scanMessage = ref('')
const scanError = ref(false)
let unlisten: UnlistenFn | null = null

const filteredPoems = computed(() => {
  const q = searchQuery.value.trim().toLowerCase()
  if (!q) return poems.value
  return poems.value.filter(
    (p) =>
      p.title.toLowerCase().includes(q) ||
      p.author.toLowerCase().includes(q) ||
      p.content.toLowerCase().includes(q),
  )
})

const current = computed<Poem | null>(() =>
  selectedIndex.value >= 0 ? filteredPoems.value[selectedIndex.value] ?? null : null,
)

// Keep the detail pane coherent: if the filter (or a reload) shrinks the
// list below the selected index, fall back to list mode so the pane never
// renders blank with no way back.
watch(filteredPoems, (list) => {
  if (selectedIndex.value >= list.length) selectedIndex.value = -1
})

async function loadPoems() {
  loading.value = true
  try {
    poems.value = await invoke<Poem[]>('get_poems')
  } catch (e) {
    console.error('[poems] Failed to load poems:', e)
  } finally {
    loading.value = false
  }
}

function refresh() {
  loadPoems()
}

async function scanLog() {
  scanning.value = true
  scanMessage.value = ''
  scanError.value = false
  try {
    const added = await invoke<number>('scan_player_log_for_poems')
    await loadPoems()
    scanError.value = false
    scanMessage.value =
      added > 0
        ? `Found ${added} new poem${added === 1 ? '' : 's'} in Player.log.`
        : 'No new poems found in Player.log.'
  } catch (e) {
    scanError.value = true
    scanMessage.value = `Scan failed: ${e}`
    console.error('[poems] Scan failed:', e)
  } finally {
    scanning.value = false
  }
}

function openPoem(idx: number) {
  selectedIndex.value = idx
}

function closePoem() {
  selectedIndex.value = -1
}

function prev() {
  if (selectedIndex.value > 0) selectedIndex.value--
}

function next() {
  if (selectedIndex.value < filteredPoems.value.length - 1) selectedIndex.value++
}

/** First non-empty line of a poem, for the list preview (plain text). */
function preview(content: string): string {
  if (!content) return ''
  const line = content.split('\n').find((l) => l.trim().length > 0) ?? ''
  return decodeEntities(line.replace(/<\/?[a-z][^>]*>/gi, '')).trim()
}

function decodeEntities(s: string): string {
  return s
    .replace(/&lt;/g, '<')
    .replace(/&gt;/g, '>')
    .replace(/&quot;/g, '"')
    .replace(/&#39;/g, "'")
    .replace(/&amp;/g, '&')
}

const ALLOWED_TAGS = ['<b>', '</b>', '<i>', '</i>']

/**
 * Render game rich-text safely. The game entity-escapes player-typed
 * `< > &` (so they arrive as `&lt; &gt; &amp;`) and uses only real `<b>`/`<i>`
 * tags for formatting. We swap those whitelisted tags for Private-Use-Area
 * sentinels (built at runtime — no invisible chars in source), escape every
 * other raw angle bracket, then restore the whitelisted tags. Existing entities
 * pass through untouched. `breaks` turns real newlines into <br>.
 */
function sanitizeRich(text: string, breaks: boolean): string {
  if (!text) return ''
  let s = text
  ALLOWED_TAGS.forEach((tag, i) => {
    const sentinel = String.fromCharCode(0xe000 + i)
    s = s.split(tag).join(sentinel).split(tag.toUpperCase()).join(sentinel)
  })
  s = s.replace(/</g, '&lt;').replace(/>/g, '&gt;')
  ALLOWED_TAGS.forEach((tag, i) => {
    s = s.split(String.fromCharCode(0xe000 + i)).join(tag)
  })
  if (breaks) s = s.replace(/\n/g, '<br>')
  return s
}

function renderInline(text: string): string {
  return sanitizeRich(text, false)
}

function renderPoem(text: string): string {
  return sanitizeRich(text, true)
}

function onKeydown(e: KeyboardEvent) {
  if (selectedIndex.value === -1) return
  if (e.key === 'Escape') {
    closePoem()
  } else if (e.key === 'ArrowLeft' || e.key === 'ArrowUp') {
    prev()
    e.preventDefault()
  } else if (e.key === 'ArrowRight' || e.key === 'ArrowDown') {
    next()
    e.preventDefault()
  }
}

onMounted(async () => {
  await loadPoems()
  unlisten = await listen<string[]>('game-state-updated', (event) => {
    if (event.payload.includes('poems')) loadPoems()
  })
  window.addEventListener('keydown', onKeydown)
})

onUnmounted(() => {
  if (unlisten) unlisten()
  window.removeEventListener('keydown', onKeydown)
})
</script>

<style scoped>
/* Preserve the poem's own line breaks/indentation while still rendering the
   inline <b>/<i> tags emitted by renderPoem(). */
.poem-body {
  white-space: pre-wrap;
  word-break: break-word;
}
</style>
