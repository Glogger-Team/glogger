<!-- DARK until LocalPlayer: lines recur (hidden 2026-10-28) -->
<template>
  <div class="flex flex-col gap-2 text-sm">
    <div v-if="loading" class="text-xs text-text-dim italic">Loading...</div>

    <div v-else-if="entries.length === 0" class="text-xs text-text-dim italic">
      No almanac data yet. Open the Gardening Almanac in-game to capture bonus events.
    </div>

    <template v-else>
      <!-- Case C book-stale banner: rows exist but no live current event and
           no future upcoming remain. The single guidance surface — both
           per-section re-read lines below stay suppressed while this shows. -->
      <div v-if="isBookStale" class="text-xs text-text-secondary italic rounded bg-surface-elevated px-2 py-1.5 border border-accent-gold/30">
        Almanac data is out of date — re-read the book in game.
      </div>

      <!-- Live seasons: stale-current rows (ended, never re-read) are filtered
           out of the current display above — the empty state names the re-read. -->
      <template v-if="currentEvents.length > 0">
        <!-- Current event -->
        <div v-for="entry in currentEvents" :key="'c-' + entry.crop_name + entry.zone_name"
          class="rounded bg-surface-elevated px-2 py-1.5 border border-accent-gold/30">
          <div class="flex items-center gap-1 text-xs text-accent-gold font-semibold mb-1">
            <span>Active Bonus</span>
          </div>
          <div class="flex items-center gap-1 flex-wrap">
            <ItemInline :reference="entry.crop_name" />
            <span class="text-text-muted">in</span>
            <AreaInline :reference="entry.zone_name" />
          </div>
          <div class="text-xs text-text-muted mt-0.5">
            Extra yield at harvest
          </div>
          <div v-if="entry.remaining" class="text-xs font-mono mt-0.5"
            :class="entry.remaining <= 3600 ? 'text-red-400' : 'text-text-secondary'">
            {{ formatRemaining(entry.remaining) }} remaining
          </div>
        </div>
      </template>
      <div v-else-if="!isBookStale" class="text-xs text-text-dim italic">
        Yesterday's bonus expired — re-read the Gardening Almanac in-game to capture the current season.
      </div>

      <!-- Upcoming events -->
      <div v-if="upcomingEvents.length > 0" class="mt-1">
        <div class="text-xs text-text-dim font-semibold mb-1 uppercase tracking-wider">Upcoming</div>
        <div v-for="entry in upcomingEvents" :key="'u-' + entry.crop_name + entry.zone_name"
          class="flex items-center justify-between gap-2 py-1 px-1">
          <div class="flex items-center gap-1 flex-wrap min-w-0">
            <ItemInline :reference="entry.crop_name" />
            <span class="text-text-muted text-xs">in</span>
            <AreaInline :reference="entry.zone_name" />
          </div>
          <span v-if="entry.startsIn" class="text-xs font-mono text-text-dim whitespace-nowrap shrink-0">
            {{ formatRemaining(entry.startsIn) }}
          </span>
          <!-- Overdue upcoming (startsIn == 0 with an event_start): per-row note
               instead of a frozen zero countdown. Suppressed under the Case C
               banner so the book nags exactly once. -->
          <span v-else-if="entry.event_start != null && !isBookStale" class="text-xs italic text-text-dim whitespace-nowrap shrink-0">
            Should have started — re-read the book to confirm.
          </span>
        </div>
      </div>

      <!-- Capture time -->
      <div class="text-[10px] text-text-dim mt-1">
        Book last read: {{ formatCapturedAt(entries[0]?.captured_at) }}
      </div>
    </template>
  </div>
</template>

<script setup lang="ts">
import { parseUtc } from '../../../composables/useTimestamp'
import { ref, computed, onMounted, onUnmounted } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { useSettingsStore } from '../../../stores/settingsStore'
import ItemInline from '../../Shared/Item/ItemInline.vue'
import AreaInline from '../../Shared/Area/AreaInline.vue'

interface AlmanacEntry {
  crop_name: string
  zone_name: string
  event_start: string | null
  event_end: string | null
  is_current: boolean
  captured_at: string
}

interface EntryWithTiming extends AlmanacEntry {
  remaining?: number
  startsIn?: number
}

const settings = useSettingsStore()
const entries = ref<AlmanacEntry[]>([])
const loading = ref(true)
const now = ref(Date.now())

let refreshInterval: ReturnType<typeof setInterval> | null = null
let unlisten: UnlistenFn | null = null

const currentEvents = computed<EntryWithTiming[]>(() =>
  entries.value
    .filter(e => e.is_current)
    .map(e => {
      const remaining = e.event_end
        ? Math.max(0, Math.floor((parseUtc(e.event_end).getTime() - now.value) / 1000))
        : undefined
      return { ...e, remaining }
    })
    // Stale-current rows (ended, never re-read) leave the current display:
    // the backend already sorts them after live rows, and the empty state
    // below names the re-read. `event_end == null` never expires.
    .filter(e => e.event_end == null || (e.remaining ?? 0) > 0)
)

const upcomingEvents = computed<EntryWithTiming[]>(() =>
  entries.value
    .filter(e => !e.is_current)
    .map(e => {
      const startsIn = e.event_start
        ? Math.max(0, Math.floor((parseUtc(e.event_start).getTime() - now.value) / 1000))
        : undefined
      return { ...e, startsIn }
    })
)

// Stale-book signal: rows exist but nothing live remains — no live current
// event (currentEvents already keeps only event_end == null || remaining > 0)
// and no future upcoming (startsIn == null is dateless, not overdue).
// Case C shows the one shared banner while this holds; per-section and
// per-row re-read lines stay suppressed. Per-widget (not shared): three
// lines over widget-local computeds — a helper would thread the same
// inputs through and save nothing.
const hasLiveCurrent = computed(() => currentEvents.value.length > 0)

const hasFutureUpcoming = computed(() =>
  upcomingEvents.value.some(e => e.startsIn == null || e.startsIn > 0),
)

const isBookStale = computed(
  () => entries.value.length > 0 && !hasLiveCurrent.value && !hasFutureUpcoming.value,
)

function formatRemaining(seconds: number): string {
  if (seconds <= 0) return 'Now!'
  const d = Math.floor(seconds / 86400)
  const h = Math.floor((seconds % 86400) / 3600)
  const m = Math.ceil((seconds % 3600) / 60)
  if (d > 0) return h > 0 ? `${d}d ${h}h` : `${d}d`
  if (h > 0) return m > 0 ? `${h}h ${m}m` : `${h}h`
  return `${m}m`
}

function formatCapturedAt(iso: string | undefined): string {
  if (!iso) return 'Unknown'
  try {
    const dt = new Date(iso)
    return dt.toLocaleString(undefined, {
      month: 'short', day: 'numeric',
      hour: 'numeric', minute: '2-digit',
    })
  } catch {
    return iso
  }
}

async function loadAlmanac() {
  const char = settings.settings.activeCharacterName
  const server = settings.settings.activeServerName
  if (!char || !server) {
    loading.value = false
    return
  }

  try {
    entries.value = await invoke<AlmanacEntry[]>('get_garden_almanac', {
      characterName: char,
      serverName: server,
    })
  } catch (e) {
    console.error('Failed to load garden almanac:', e)
  } finally {
    loading.value = false
  }
}

onMounted(async () => {
  await loadAlmanac()

  refreshInterval = setInterval(() => {
    now.value = Date.now()
  }, 30_000)

  unlisten = await listen<string[]>('game-state-updated', (event) => {
    if (event.payload.includes('garden_almanac')) {
      loadAlmanac()
    }
  })
})

onUnmounted(() => {
  if (refreshInterval) clearInterval(refreshInterval)
  if (unlisten) unlisten()
})
</script>
