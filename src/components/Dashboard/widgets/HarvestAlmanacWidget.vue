<template>
  <div class="flex flex-col gap-2 text-sm">
    <div v-if="loading" class="text-xs text-text-dim italic">Loading...</div>

    <div v-else-if="entries.length === 0" class="text-xs text-text-dim italic">
      No almanac data yet. Read the Almanac of Corruption in Statehelm to capture today's foci.
    </div>

    <template v-else>
      <!-- Live foci: stale-current rows (ended, never re-read) are filtered
           out of Today's Focus above — the empty state names the re-read. -->
      <template v-if="currentFoci.length > 0">
        <!-- Today's focus monsters -->
        <div v-for="entry in currentFoci" :key="'c-' + entry.monster_name"
          class="rounded bg-surface-elevated px-2 py-1.5 border border-accent-gold/30">
          <div class="flex items-center gap-1 text-xs text-accent-gold font-semibold mb-1">
            <span>Today's Focus</span>
          </div>
          <div class="flex items-center gap-1 flex-wrap">
            <EnemyInline :reference="entry.monster_name" />
          </div>
          <div v-if="entry.description" class="text-xs text-text-muted mt-0.5 italic">
            {{ entry.description }}
          </div>

          <!-- Zones resolved from your kill history — the almanac never says where. -->
          <div v-if="zonesFor(entry.monster_name).length > 0" class="flex items-center gap-1 flex-wrap mt-1">
            <span class="text-xs text-text-dim">Found in</span>
            <template v-for="(z, i) in zonesFor(entry.monster_name)" :key="z.zone">
              <span v-if="i > 0" class="text-text-dim text-xs">·</span>
              <AreaInline :reference="z.zone" />
              <span class="text-[10px] text-text-dim">({{ z.total_kills }})</span>
            </template>
          </div>
          <div v-else-if="!zonesLoading" class="text-xs text-text-dim mt-1 italic">
            No kill history yet — zone unknown.
          </div>

          <div v-if="entry.remaining" class="text-xs font-mono mt-0.5"
            :class="entry.remaining <= 3600 ? 'text-red-400' : 'text-text-secondary'">
            {{ formatRemaining(entry.remaining) }} remaining
          </div>
        </div>
      </template>
      <div v-else class="text-xs text-text-dim italic">
        Yesterday's focus expired — re-read the Almanac of Corruption in Statehelm to capture today's foci.
      </div>

      <!-- Upcoming foci -->
      <div v-if="upcomingFoci.length > 0" class="mt-1">
        <div class="text-xs text-text-dim font-semibold mb-1 uppercase tracking-wider">Upcoming</div>
        <div v-for="entry in upcomingFoci" :key="'u-' + entry.monster_name"
          class="flex items-center justify-between gap-2 py-1 px-1">
          <div class="flex items-center gap-1 flex-wrap min-w-0">
            <EnemyInline :reference="entry.monster_name" />
          </div>
          <span v-if="entry.startsIn" class="text-xs font-mono text-text-dim whitespace-nowrap shrink-0">
            {{ formatRemaining(entry.startsIn) }}
          </span>
        </div>
      </div>

      <!-- Capture time -->
      <div class="text-[10px] text-text-dim mt-1">
        Last updated: {{ formatCapturedAt(entries[0]?.captured_at) }}
      </div>
    </template>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted, watch } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { useSettingsStore } from '../../../stores/settingsStore'
import EnemyInline from '../../Shared/Enemy/EnemyInline.vue'
import AreaInline from '../../Shared/Area/AreaInline.vue'

interface HarvestEntry {
  monster_name: string
  description: string | null
  event_start: string | null
  event_end: string | null
  is_current: boolean
  captured_at: string
}

interface EntryWithTiming extends HarvestEntry {
  remaining?: number
  startsIn?: number
}

interface EnemySearchResult {
  enemy_name: string
  zone: string | null
  total_kills: number
  distinct_loot_items: number
}

interface ZoneHit {
  zone: string
  total_kills: number
}

const MAX_ZONES = 3

const settings = useSettingsStore()
const entries = ref<HarvestEntry[]>([])
const loading = ref(true)
const zonesLoading = ref(false)
const now = ref(Date.now())
// monster_name (as the almanac spells it) -> zones from the kill database
const zoneHits = ref<Record<string, ZoneHit[]>>({})

let refreshInterval: ReturnType<typeof setInterval> | null = null
let unlisten: UnlistenFn | null = null

const currentFoci = computed<EntryWithTiming[]>(() =>
  entries.value
    .filter(e => e.is_current)
    .map(e => {
      const remaining = e.event_end
        ? Math.max(0, Math.floor((new Date(e.event_end).getTime() - now.value) / 1000))
        : undefined
      return { ...e, remaining }
    })
    // Stale-current rows (ended, never re-read) leave Today's Focus: the
    // backend already sorts them after live rows, and the empty state below
    // names the re-read. `event_end == null` never expires.
    .filter(e => e.event_end == null || (e.remaining ?? 0) > 0)
)

const upcomingFoci = computed<EntryWithTiming[]>(() =>
  entries.value
    .filter(e => !e.is_current)
    .map(e => {
      const startsIn = e.event_start
        ? Math.max(0, Math.floor((new Date(e.event_start).getTime() - now.value) / 1000))
        : undefined
      return { ...e, startsIn }
    })
)

function zonesFor(monster: string): ZoneHit[] {
  return zoneHits.value[monster] ?? []
}

// The almanac writes monsters in the plural ("Skeleton Swordsmen") while the
// kill database stores the singular entity name ("Skeleton Swordsman"). Only
// the head noun is de-pluralized — modifiers like "Chaos" must stay intact.
function singularizeWord(word: string): string {
  const lower = word.toLowerCase()
  if (lower.endsWith('men') && word.length > 3) return word.slice(0, -3) + 'man'
  if (lower.endsWith('ies') && word.length > 4) return word.slice(0, -3) + 'y'
  if (lower.endsWith('ves') && word.length > 4) return word.slice(0, -3) + 'f'
  if (/(sses|shes|ches|xes)$/.test(lower)) return word.slice(0, -2)
  if (lower.endsWith('s') && !/(ss|us|is|os)$/.test(lower)) return word.slice(0, -1)
  return word
}

function singularizePhrase(name: string): string {
  const words = name.trim().split(/\s+/)
  if (words.length === 0) return name
  words[words.length - 1] = singularizeWord(words[words.length - 1])
  return words.join(' ')
}

// Resolve one almanac monster to the zones it's been killed in. Queries on the
// singular head noun (a cheap substring match server-side), then keeps only the
// rows that genuinely correspond to the focus — a bare "Swordsman" needle also
// drags in "Orcish Swordsman", which is a different monster entirely.
async function resolveZones(monster: string): Promise<ZoneHit[]> {
  const singular = singularizePhrase(monster)
  const words = singular.toLowerCase().split(/\s+/)
  const needle = words[words.length - 1]
  if (!needle) return []

  let results: EnemySearchResult[]
  try {
    results = await invoke<EnemySearchResult[]>('search_database_enemies', {
      query: needle,
      scope: 'combined',
      limit: null,
      combatSkills: null,
    })
  } catch (e) {
    console.error('Failed to resolve harvest almanac zones:', e)
    return []
  }

  const exact = results.filter(r => r.enemy_name.toLowerCase() === singular.toLowerCase())
  // Fall back to names carrying every word of the focus, so "Skeleton Swordsmen"
  // still finds "Elite Skeleton Swordsman" when no exact row exists.
  const pool = exact.length > 0
    ? exact
    : results.filter(r => {
        const n = r.enemy_name.toLowerCase()
        return words.every(w => n.includes(w))
      })

  const byZone = new Map<string, number>()
  for (const r of pool) {
    if (!r.zone) continue
    byZone.set(r.zone, (byZone.get(r.zone) ?? 0) + r.total_kills)
  }

  return [...byZone.entries()]
    .map(([zone, total_kills]) => ({ zone, total_kills }))
    .sort((a, b) => b.total_kills - a.total_kills)
    .slice(0, MAX_ZONES)
}

async function loadZones() {
  const monsters = currentFoci.value.map(e => e.monster_name)
  if (monsters.length === 0) {
    zoneHits.value = {}
    return
  }

  zonesLoading.value = true
  try {
    const resolved = await Promise.all(monsters.map(m => resolveZones(m)))
    const next: Record<string, ZoneHit[]> = {}
    monsters.forEach((m, i) => { next[m] = resolved[i] })
    zoneHits.value = next
  } finally {
    zonesLoading.value = false
  }
}

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
    entries.value = await invoke<HarvestEntry[]>('get_harvest_almanac', {
      characterName: char,
      serverName: server,
    })
  } catch (e) {
    console.error('Failed to load harvest almanac:', e)
  } finally {
    loading.value = false
  }
}

// Re-resolve zones whenever the focus list changes (new almanac reading).
watch(
  () => currentFoci.value.map(e => e.monster_name).join('|'),
  () => { loadZones() },
)

onMounted(async () => {
  await loadAlmanac()
  await loadZones()

  refreshInterval = setInterval(() => {
    now.value = Date.now()
  }, 30_000)

  unlisten = await listen<string[]>('game-state-updated', (event) => {
    if (event.payload.includes('harvest_almanac')) {
      loadAlmanac()
    }
  })
})

onUnmounted(() => {
  if (refreshInterval) clearInterval(refreshInterval)
  if (unlisten) unlisten()
})
</script>
