<template>
  <div class="flex flex-col gap-2 text-sm h-full min-h-0">
    <!-- Stats bar -->
    <div class="flex items-center justify-between px-1 shrink-0">
      <div class="flex items-center gap-2">
        <span class="text-xs text-text-muted">Studied:</span>
        <span class="text-xs font-mono text-accent-gold">{{ stats.total_studied }}</span>
      </div>
      <div v-if="cooldownRemaining > 0" class="flex items-center gap-1">
        <span class="text-[10px] text-text-dim">Cooldown:</span>
        <span class="text-xs font-mono text-amber-400">{{ formatCooldown(cooldownRemaining) }}</span>
      </div>
      <div v-else-if="stats.last_studied_at" class="flex items-center gap-1">
        <span class="text-[10px] text-green-400">Ready to study</span>
      </div>
    </div>

    <!-- Tab bar -->
    <div class="flex gap-1 shrink-0">
      <button
        v-for="tab in tabs"
        :key="tab.key"
        class="px-2 py-1 text-xs rounded transition-colors cursor-pointer"
        :class="activeTab === tab.key
          ? 'bg-accent-gold/20 text-accent-gold border border-accent-gold/40'
          : 'text-text-muted hover:text-text-primary hover:bg-surface-elevated'"
        @click="activeTab = tab.key">
        {{ tab.label }}
        <span v-if="tab.key === 'to-study' && toStudyItems.length > 0"
              class="ml-1 text-[10px] opacity-70">({{ toStudyItems.length }})</span>
      </button>
    </div>

    <!-- Search -->
    <input
      v-model="search"
      type="text"
      :placeholder="activeTab === 'studied' ? 'Search studied items...' : 'Search equipment...'"
      class="w-full px-2 py-1 rounded bg-surface-2 border border-border text-sm text-text-primary placeholder:text-text-dim focus:outline-none focus:border-accent-blue shrink-0" />

    <!-- Studied tab -->
    <div v-if="activeTab === 'studied'" class="flex-1 overflow-y-auto min-h-0">
      <div v-if="filteredStudies.length === 0 && studies.length > 0" class="text-xs text-text-dim italic py-2">
        No items match your search.
      </div>
      <div v-else-if="studies.length === 0" class="text-xs text-text-dim italic py-2">
        No items studied yet. Study equipment in-game or open the Hoplology skill report to backfill.
      </div>
      <div v-else class="flex flex-col gap-0.5">
        <div
          v-for="study in filteredStudies"
          :key="study.id"
          class="flex items-center justify-between gap-2 py-0.5 px-2 rounded hover:bg-surface-elevated/50">
          <ItemInline :reference="study.item_name" class="text-xs truncate min-w-0" />
          <span class="text-[10px] text-text-dim whitespace-nowrap shrink-0">
            {{ study.source === 'report' ? 'from report' : formatStudyDate(study.studied_at) }}
          </span>
        </div>
      </div>
    </div>

    <!-- To Study tab -->
    <div v-if="activeTab === 'to-study'" class="flex-1 overflow-y-auto min-h-0">
      <div v-if="toStudyLoading" class="text-xs text-text-dim italic py-2">
        Checking equipment...
      </div>
      <div v-else-if="filteredToStudy.length === 0 && toStudyItems.length > 0" class="text-xs text-text-dim italic py-2">
        No items match your search.
      </div>
      <div v-else-if="toStudyItems.length === 0" class="text-xs text-text-dim italic py-2">
        No unstudied equipment found in inventory or storage.
      </div>
      <div v-else class="flex flex-col gap-0.5">
        <div
          v-for="item in filteredToStudy"
          :key="item.name + item.location"
          class="flex items-center justify-between gap-2 py-0.5 px-2 rounded hover:bg-surface-elevated/50">
          <ItemInline :reference="item.name" class="text-xs truncate min-w-0" />
          <span class="text-[10px] text-text-dim whitespace-nowrap shrink-0">
            {{ item.location }}
          </span>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted, watch } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { useSettingsStore } from '../../../stores/settingsStore'
import { useGameStateStore } from '../../../stores/gameStateStore'
import { useGameDataStore } from '../../../stores/gameDataStore'
import { parseUtc, formatDateShort } from '../../../composables/useTimestamp'
import ItemInline from '../../Shared/Item/ItemInline.vue'

interface HoplologyStudy {
  id: number
  character_name: string
  server_name: string
  item_name: string
  studied_at: string
  source: string
}

interface HoplologyStats {
  total_studied: number
  last_studied_at: string | null
  last_studied_item: string | null
}

interface ToStudyItem {
  name: string
  location: string // "Inventory" or vault name
}

const COOLDOWN_MS = 5 * 60 * 1000 // 5 minutes

const tabs = [
  { key: 'studied', label: 'Studied' },
  { key: 'to-study', label: 'To Study' },
] as const

const settings = useSettingsStore()
const gameState = useGameStateStore()
const gameData = useGameDataStore()

const activeTab = ref<'studied' | 'to-study'>('studied')
const studies = ref<HoplologyStudy[]>([])
const stats = ref<HoplologyStats>({ total_studied: 0, last_studied_at: null, last_studied_item: null })
const search = ref('')
const now = ref(Date.now())
const toStudyItems = ref<ToStudyItem[]>([])
const toStudyLoading = ref(false)

let refreshInterval: ReturnType<typeof setInterval> | null = null
let unlisten: UnlistenFn | null = null
let unlistenStatus: UnlistenFn | null = null

const cooldownRemaining = computed(() => {
  if (!stats.value.last_studied_at) return 0
  const lastStudied = parseUtc(stats.value.last_studied_at).getTime()
  if (isNaN(lastStudied)) return 0
  const remaining = Math.ceil((lastStudied + COOLDOWN_MS - now.value) / 1000)
  return Math.max(0, remaining)
})

const filteredStudies = computed(() => {
  if (!search.value.trim()) return studies.value
  const q = search.value.toLowerCase()
  return studies.value.filter(s => s.item_name.toLowerCase().includes(q))
})

const filteredToStudy = computed(() => {
  if (!search.value.trim()) return toStudyItems.value
  const q = search.value.toLowerCase()
  return toStudyItems.value.filter(i => i.name.toLowerCase().includes(q))
})

function formatCooldown(seconds: number): string {
  const m = Math.floor(seconds / 60)
  const s = seconds % 60
  return `${m}:${s.toString().padStart(2, '0')}`
}

function formatStudyDate(dateStr: string): string {
  return formatDateShort(dateStr)
}

async function loadData() {
  const char = settings.settings.activeCharacterName
  const server = settings.settings.activeServerName
  if (!char || !server) return

  try {
    const [studyList, studyStats] = await Promise.all([
      invoke<HoplologyStudy[]>('get_hoplology_studies', {
        characterName: char,
        serverName: server,
      }),
      invoke<HoplologyStats>('get_hoplology_stats', {
        characterName: char,
        serverName: server,
      }),
    ])
    studies.value = studyList
    stats.value = studyStats
  } catch (e) {
    console.error('Failed to load hoplology data:', e)
  }
}

async function loadToStudyItems() {
  toStudyLoading.value = true
  try {
    // Collect unique item names from inventory and storage
    const candidates = new Map<string, string>() // name -> location

    for (const item of gameState.inventory) {
      if (!candidates.has(item.item_name)) {
        candidates.set(item.item_name, 'Inventory')
      }
    }
    for (const item of gameState.storage) {
      if (!candidates.has(item.item_name)) {
        candidates.set(item.item_name, item.vault_key)
      }
    }

    if (candidates.size === 0) {
      toStudyItems.value = []
      return
    }

    // Resolve candidates AND studied names through the same batch call so both
    // sides compare canonical CDN base names. Studied rows store base names,
    // but resolve them anyway: a CDN rename would otherwise split the compare
    // (raw studied vs resolved candidate). Unresolved names fall back to raw.
    const studiedRaw = studies.value.map(s => s.item_name)
    const allNames = [...new Set([...candidates.keys(), ...studiedRaw])]
    const resolved = await gameData.resolveItemsBatch(allNames)
    const studiedNames = new Set(
      studiedRaw.map(n => (resolved[n]?.name ?? n).toLowerCase()),
    )

    // Deduplicate by base name: different crafted variants resolve to the same base item
    const seen = new Set<string>()
    const items: ToStudyItem[] = []
    for (const [rawName, location] of candidates) {
      const info = resolved[rawName]
      if (!info?.equip_slot) continue

      // Use the CDN base name for dedup and studied-check
      const baseName = info.name.toLowerCase()
      if (studiedNames.has(baseName) || seen.has(baseName)) continue
      seen.add(baseName)

      items.push({ name: info.name, location })
    }
    items.sort((a, b) => a.name.localeCompare(b.name))
    toStudyItems.value = items
  } catch (e) {
    console.error('Failed to load to-study items:', e)
  } finally {
    toStudyLoading.value = false
  }
}

// Reload to-study items when switching to that tab or when studies change
watch(activeTab, (tab) => {
  if (tab === 'to-study') loadToStudyItems()
})
watch(studies, () => {
  if (activeTab.value === 'to-study') loadToStudyItems()
})

onMounted(async () => {
  await loadData()

  // Refresh the cooldown timer every second while active, else every 30s
  refreshInterval = setInterval(() => {
    now.value = Date.now()
  }, cooldownRemaining.value > 0 ? 1000 : 30_000)

  unlisten = await listen<string[]>('game-state-updated', (event) => {
    if (event.payload.includes('hoplology')) {
      loadData()
    }
    // Refresh to-study when inventory or storage changes
    if (activeTab.value === 'to-study' &&
        (event.payload.includes('inventory') || event.payload.includes('storage'))) {
      loadToStudyItems()
    }
  })

  // Also listen for real-time study events from chat
  unlistenStatus = await listen('chat-status-event', (event: any) => {
    if (event.payload?.kind === 'ItemStudied') {
      // Refresh data and reset cooldown timer
      loadData()
      now.value = Date.now()
      // Switch to 1-second interval for cooldown display
      if (refreshInterval) clearInterval(refreshInterval)
      refreshInterval = setInterval(() => {
        now.value = Date.now()
        // Switch back to 30s interval once cooldown expires
        if (cooldownRemaining.value <= 0 && refreshInterval) {
          clearInterval(refreshInterval)
          refreshInterval = setInterval(() => { now.value = Date.now() }, 30_000)
        }
      }, 1000)
    }
  })
})

onUnmounted(() => {
  if (refreshInterval) clearInterval(refreshInterval)
  if (unlisten) unlisten()
  if (unlistenStatus) unlistenStatus()
})
</script>
