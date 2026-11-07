<template>
  <div class="flex flex-col gap-3">
    <!-- Header area with edit toggle -->
    <div class="flex items-center justify-between">
      <button
        class="text-xs text-text-dim hover:text-text-muted transition-colors"
        title="Edit watched NPCs"
        @click="editing = !editing"
      >
        {{ editing ? '✕ Done' : '⚙ Edit' }}
      </button>
      <div class="flex items-center gap-2">
        <select
          v-if="!editing && watchedNpcKeys.length > 0"
          v-model="zoneFilter"
          title="Filter by zone"
          class="bg-surface-elevated text-text text-xs rounded px-1.5 py-0.5 border border-border-dim focus:border-accent-blue focus:outline-none max-w-32"
          @change="saveZoneFilter"
        >
          <option value="All">All zones</option>
          <option v-for="zone in availableZones" :key="zone" :value="zone">{{ zone }}</option>
        </select>
        <button
          v-if="!editing && zoneFilter !== ALL_ZONES"
          :title="`Add all NPCs in ${zoneFilter} to watched`"
          class="text-xs text-text-dim hover:text-text-muted transition-colors whitespace-nowrap"
          @click="addAllInZone"
        >
          Add all in {{ zoneFilter }}
        </button>
        <span class="text-xs text-text-dim">
          {{ watchedNpcKeys.length }} NPC{{ watchedNpcKeys.length === 1 ? '' : 's' }} watched
        </span>
      </div>
    </div>

    <!-- Edit mode: NPC search + selection -->
    <div v-if="editing" class="flex flex-col gap-2">
      <!-- Top bar: watched count + clear -->
      <div class="flex items-center justify-between">
        <span class="text-xs text-text-dim">
          {{ watchedNpcKeys.length }} NPC{{ watchedNpcKeys.length === 1 ? '' : 's' }} watched
        </span>
        <button
          v-if="watchedNpcKeys.length > 0"
          class="text-xs text-accent-red hover:underline"
          @click="clearWatched"
        >Clear all</button>
      </div>
      <div class="relative">
        <input
          v-model="searchQuery"
          type="text"
          placeholder="Search NPCs by name, zone, role, or skill..."
          class="w-full bg-surface-elevated text-text text-sm rounded px-2 py-1.5 border border-border-dim focus:border-accent-blue focus:outline-none"
          @input="onSearchInput"
        />
        <!-- Search results dropdown -->
        <div
          v-if="searchResults.length > 0 && searchQuery.length >= 2"
          class="absolute z-10 top-full left-0 right-0 mt-1 bg-surface-elevated border border-border-dim rounded shadow-lg max-h-48 overflow-y-auto"
        >
          <div class="flex items-center justify-between px-2 py-1 text-xs border-b border-border-dim sticky top-0 bg-surface-elevated">
            <span class="text-text-dim">{{ searchResults.length }} result{{ searchResults.length === 1 ? '' : 's' }}</span>
            <button class="text-accent-blue hover:underline" @click="addAllSearchResults">Add all</button>
          </div>
          <button
            v-for="npc in searchResults"
            :key="npc.key"
            class="w-full text-left px-2 py-1.5 text-sm hover:bg-surface-hover transition-colors"
            :class="watchedNpcKeys.includes(npc.key) ? 'text-text-dim' : 'text-text'"
            @click="toggleNpc(npc.key)"
          >
            <span>{{ npc.name }}</span>
            <span v-if="npc.area_friendly_name" class="text-text-dim text-xs ml-1">({{ npc.area_friendly_name }})</span>
            <span v-if="watchedNpcKeys.includes(npc.key)" class="text-accent-green text-xs ml-1">watching</span>
          </button>
        </div>
      </div>

      <!-- Watched NPC chips -->
      <div v-if="watchedNpcKeys.length > 0" class="flex flex-wrap gap-1.5">
        <span
          v-for="key in watchedNpcKeys"
          :key="key"
          class="inline-flex items-center gap-1 bg-surface-elevated text-entity-npc text-xs rounded-full px-2 py-0.5 border border-entity-npc/20"
        >
          {{ resolveNpcName(key) }}
          <button
            class="text-text-dim hover:text-accent-red transition-colors"
            @click="removeNpc(key)"
          >×</button>
        </span>
      </div>
      <p v-else class="text-xs text-text-dim italic">
        Search and select NPCs to watch for gift-worthy items in your inventory.
      </p>
    </div>

    <!-- Main display: matches per NPC -->
    <template v-if="!editing">
      <div v-if="watchedNpcKeys.length === 0" class="text-xs text-text-dim italic">
        No NPCs watched. Click ⚙ Edit to add some.
      </div>

      <div v-else-if="loading" class="text-xs text-text-dim italic">
        Resolving items...
      </div>

      <div v-else-if="groupedMatches.length === 0" class="text-xs text-text-dim italic">
        No watched NPCs in this zone.
      </div>

      <div v-else class="flex flex-col gap-3">
        <div v-for="group in groupedMatches" :key="group.zone" class="flex flex-col gap-2">
          <div class="text-xs font-semibold text-text-dim uppercase tracking-wide border-b border-border-dim pb-0.5">
            {{ group.zone }}
          </div>
          <div
            v-for="entry in group.entries"
            :key="entry.npcKey"
            class="flex flex-col gap-1"
          >
            <NpcInline :reference="entry.npcKey" />

            <div v-if="entry.matches.length > 0" class="flex flex-col gap-0.5 pl-3">
              <div
                v-for="match in entry.matches"
                :key="match.itemName"
                class="flex items-center justify-between text-sm"
              >
                <ItemInline :reference="match.itemName" :show-icon="false" />
                <span class="text-xs text-text-muted shrink-0 ml-2">
                  ×{{ match.count }}
                  <span
                    class="ml-1"
                    :class="match.desire === 'Love' ? 'text-accent-red' : 'text-accent-gold'"
                  >{{ match.desire === 'Love' ? '♥' : '♦' }}</span>
                </span>
              </div>
            </div>

            <div v-else class="text-xs text-text-dim italic pl-3">
              No matching items
            </div>
          </div>
        </div>
      </div>
    </template>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, watch, onMounted } from 'vue'
import { useGameDataStore } from '../../../stores/gameDataStore'
import { useGameStateStore } from '../../../stores/gameStateStore'
import { findGiftableItems } from '../../../composables/useNpcGiftMatching'
import { getServices } from '../../../composables/useNpcServices'
import type { NpcInfo } from '../../../types/gameData/npcs'
import type { ItemInfo } from '../../../types/gameData/items'
import NpcInline from '../../Shared/NPC/NpcInline.vue'
import ItemInline from '../../Shared/Item/ItemInline.vue'
const STORAGE_KEY = 'giftWatcher.watchedNpcs'
const ZONE_STORAGE_KEY = 'giftWatcher.zoneFilter'
const UNKNOWN_ZONE = 'Unknown'
const ALL_ZONES = 'All'

const gameData = useGameDataStore()
const gameState = useGameStateStore()

const editing = ref(false)
const searchQuery = ref('')
const searchResults = ref<NpcInfo[]>([])
const watchedNpcKeys = ref<string[]>([])
const zoneFilter = ref<string>(ALL_ZONES)
const resolvedItems = ref<Record<string, ItemInfo>>({})
const loading = ref(false)

// ── Persistence ──────────────────────────────────────────────────────────────

function loadWatchedNpcs() {
  try {
    const stored = localStorage.getItem(STORAGE_KEY)
    if (stored) watchedNpcKeys.value = JSON.parse(stored)
  } catch {
    watchedNpcKeys.value = []
  }
  try {
    zoneFilter.value = localStorage.getItem(ZONE_STORAGE_KEY) ?? ALL_ZONES
  } catch {
    zoneFilter.value = ALL_ZONES
  }
}

function saveWatchedNpcs() {
  localStorage.setItem(STORAGE_KEY, JSON.stringify(watchedNpcKeys.value))
}

function saveZoneFilter() {
  localStorage.setItem(ZONE_STORAGE_KEY, zoneFilter.value)
}

function clampZoneFilter() {
  if (zoneFilter.value === ALL_ZONES) return
  const zones = new Set(watchedNpcKeys.value.map(resolveNpcZone))
  if (!zones.has(zoneFilter.value)) {
    zoneFilter.value = ALL_ZONES
    saveZoneFilter()
  }
}

// ── NPC search (one box: name / zone / role / skill + bulk add) ─────────────────


const ROLE_SEARCH_LABELS: Record<string, string> = {
  Store: 'vendor',
  Training: 'trainer',
  Storage: 'storage',
  Barter: 'barter',
  Consignment: 'consignment',
}

function npcSearchText(npc: NpcInfo): string {
  const parts = [npc.area_friendly_name ?? '']
  for (const svc of getServices(npc)) {
    parts.push(svc.type.toLowerCase())
    const label = ROLE_SEARCH_LABELS[svc.type]
    if (label && label !== svc.type.toLowerCase()) parts.push(label)
  }
  for (const skill of npc.trains_skills ?? []) parts.push(skill)
  return parts.join(' ').toLowerCase()
}


function clearWatched() {
  watchedNpcKeys.value = []
  saveWatchedNpcs()
}

let searchTimeout: ReturnType<typeof setTimeout> | null = null

function onSearchInput() {
  if (searchTimeout) clearTimeout(searchTimeout)
  if (searchQuery.value.length < 2) {
    searchResults.value = []
    return
  }
  searchTimeout = setTimeout(async () => {
    const q = searchQuery.value.toLowerCase()
    const results = await gameData.searchNpcs(searchQuery.value)
    // Backend matches name/desc only — match zone/role/skill text client-side.
    // Ensure the map is loaded first; early-return no-op when already loaded.
    await gameData.loadAllNpcsMap()
    const extra = Object.values(gameData.npcsByKey).filter(npc =>
      !results.some(r => r.key === npc.key) && npcSearchText(npc).includes(q),
    )
    searchResults.value = [...results, ...extra].sort((a, b) => a.name.localeCompare(b.name))
  }, 200)
}

function addAllSearchResults() {
  const watched = new Set(watchedNpcKeys.value)
  const fresh = searchResults.value.map(n => n.key).filter(k => !watched.has(k))
  searchQuery.value = ''
  searchResults.value = []
  if (fresh.length === 0) return
  watchedNpcKeys.value = [...watchedNpcKeys.value, ...fresh]
  saveWatchedNpcs()
}
function addAllInZone() {
  if (zoneFilter.value === ALL_ZONES) return
  gameData.loadAllNpcsMap().then(() => {
    const watched = new Set(watchedNpcKeys.value)
    const fresh = Object.values(gameData.npcsByKey)
      .filter(n => (n.area_friendly_name ?? UNKNOWN_ZONE) === zoneFilter.value)
      .map(n => n.key)
      .filter(k => !watched.has(k))
    if (fresh.length === 0) return
    watchedNpcKeys.value = [...watchedNpcKeys.value, ...fresh]
    saveWatchedNpcs()
    clampZoneFilter()
  })
}
function toggleNpc(key: string) {
  if (watchedNpcKeys.value.includes(key)) {
    removeNpc(key)
  } else {
    watchedNpcKeys.value = [...watchedNpcKeys.value, key]
    saveWatchedNpcs()
  }
  searchQuery.value = ''
  searchResults.value = []
}

function removeNpc(key: string) {
  watchedNpcKeys.value = watchedNpcKeys.value.filter(k => k !== key)
  saveWatchedNpcs()
}

function resolveNpcName(key: string): string {
  const npc = gameData.resolveNpcSync(key)
  return npc?.name ?? key
}

// ── Item resolution ──────────────────────────────────────────────────────────

// Stable fingerprint of inventory item names — avoids re-resolving when only
// stack sizes change (Object.keys is the same set of names).
const inventoryItemNames = computed(() => Object.keys(gameState.inventoryItemCounts))
const itemNamesFingerprint = computed(() => inventoryItemNames.value.slice().sort().join('\0'))

async function resolveInventoryItems(showLoading = true) {
  const names = inventoryItemNames.value
  if (names.length === 0) {
    resolvedItems.value = {}
    return
  }
  if (showLoading) loading.value = true
  try {
    resolvedItems.value = await gameData.resolveItemsBatch(names)
  } catch (e) {
    console.warn('GiftWatcher: failed to resolve items', e)
  } finally {
    loading.value = false
  }
}

// Only re-resolve when the actual set of item names changes, not on every
// stack size update.  Skip the loading flash on incremental updates.
watch(itemNamesFingerprint, () => resolveInventoryItems(false))

// (zone-clamp watcher lives below groupedMatches, after availableZones exists)
// ── Gift matching ────────────────────────────────────────────────────────────

interface NpcMatchEntry {
  npcKey: string
  zone: string
  matches: { itemName: string; count: number; desire: string }[]
}

interface ZoneGroup {
  zone: string
  entries: NpcMatchEntry[]
}

function resolveNpcZone(key: string): string {
  return gameData.resolveNpcSync(key)?.area_friendly_name ?? UNKNOWN_ZONE
}

const availableZones = computed<string[]>(() => {
  const zones = new Set<string>()
  for (const key of watchedNpcKeys.value) zones.add(resolveNpcZone(key))
  return [...zones].sort((a, b) =>
    a === UNKNOWN_ZONE ? 1 : b === UNKNOWN_ZONE ? -1 : a.localeCompare(b),
  )
})

const npcMatches = computed<NpcMatchEntry[]>(() => {
  const counts = gameState.inventoryItemCounts
  const items: { name: string; keywords: string[] }[] = []

  for (const [name, count] of Object.entries(counts)) {
    if (count <= 0) continue
    const resolved = resolvedItems.value[name]
    if (resolved) {
      items.push({ name: resolved.name, keywords: resolved.keywords })
    }
  }

  return watchedNpcKeys.value.map(key => {
    const npc = gameData.resolveNpcSync(key)
    if (!npc) return { npcKey: key, zone: UNKNOWN_ZONE, matches: [] }

    const giftable = findGiftableItems(npc, items)
    return {
      npcKey: key,
      zone: npc.area_friendly_name ?? UNKNOWN_ZONE,
      matches: giftable.map(g => ({
        itemName: g.item.name,
        count: counts[g.item.name] ?? 0,
        desire: g.bestPref.desire,
      })),
    }
  })
})

const groupedMatches = computed<ZoneGroup[]>(() => {
  const filtered = zoneFilter.value === ALL_ZONES
    ? npcMatches.value
    : npcMatches.value.filter(e => e.zone === zoneFilter.value)
  const groups = new Map<string, NpcMatchEntry[]>()
  for (const entry of filtered) {
    const list = groups.get(entry.zone) ?? []
    list.push(entry)
    groups.set(entry.zone, list)
  }
  return [...groups.entries()]
    .sort(([a], [b]) =>
      a === UNKNOWN_ZONE ? 1 : b === UNKNOWN_ZONE ? -1 : a.localeCompare(b),
    )
    .map(([zone, entries]) => ({
      zone,
      entries: [...entries].sort((a, b) => {
        const totalA = a.matches.reduce((sum, m) => sum + m.count, 0)
        const totalB = b.matches.reduce((sum, m) => sum + m.count, 0)
        if (totalB !== totalA) return totalB - totalA
        return resolveNpcName(a.npcKey).localeCompare(resolveNpcName(b.npcKey))
      }),
    }))
})

// Reset a stale zone selection when the watched set (or its zones) changes.
// Guard on the NPC map being loaded: before loadAllNpcsMap resolves every
// key zones to Unknown, which would wipe a saved filter to All.
watch(availableZones, () => {
  if (Object.keys(gameData.npcsByKey).length > 0) clampZoneFilter()
})

onMounted(async () => {
  loadWatchedNpcs()
  loading.value = true
  try {
    await gameData.loadAllNpcsMap()
    clampZoneFilter()
    await resolveInventoryItems(false)
  } finally {
    loading.value = false
  }

  // Open edit mode automatically if no NPCs are watched
  if (watchedNpcKeys.value.length === 0) editing.value = true
})
</script>
