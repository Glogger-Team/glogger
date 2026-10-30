<template>
  <div class="flex flex-col gap-2 overflow-y-auto h-full p-2">
    <!-- Search -->
    <input
      v-model="filter"
      type="text"
      placeholder="Search NPCs..."
      class="px-3 py-1.5 bg-surface-base border border-border-default rounded text-sm text-text-primary placeholder-text-muted focus:outline-none focus:border-accent-gold/50 w-full" />

    <!-- Grouping & Sorting -->
    <div class="flex flex-col gap-1.5">
      <label class="text-[10px] uppercase tracking-widest text-text-dim">Group By</label>
      <select
        v-model="groupBy"
        class="px-2 py-1.5 bg-surface-base border border-border-default rounded text-xs text-text-primary cursor-pointer w-full">
        <option value="area">Area</option>
        <option value="favor">Favor Tier</option>
        <option value="none">No Grouping</option>
      </select>
    </div>

    <div class="flex flex-col gap-1.5">
      <label class="text-[10px] uppercase tracking-widest text-text-dim">Sort By</label>
      <select
        v-model="sortBy"
        class="px-2 py-1.5 bg-surface-base border border-border-default rounded text-xs text-text-primary cursor-pointer w-full">
        <option value="favor">Favor Tier</option>
        <option value="name">Name</option>
        <option value="spent">Councils Spent</option>
      </select>
    </div>

    <!-- Favor tier filter -->
    <div class="flex flex-col gap-1.5">
      <label class="text-[10px] uppercase tracking-widest text-text-dim">Favor Tier</label>
      <select
        v-model="favorFilter"
        class="px-2 py-1.5 bg-surface-base border border-border-default rounded text-xs text-text-primary cursor-pointer w-full">
        <option value="">All Tiers</option>
        <option v-for="tier in FAVOR_TIERS" :key="tier" :value="tier">
          {{ tierDisplayName(tier) }}
        </option>
      </select>
    </div>

    <!-- Area filter -->
    <div class="flex flex-col gap-1.5">
      <label class="text-[10px] uppercase tracking-widest text-text-dim">Area</label>
      <select
        v-model="areaFilter"
        class="px-2 py-1.5 bg-surface-base border border-border-default rounded text-xs text-text-primary cursor-pointer w-full">
        <option value="">All Areas</option>
        <option v-for="area in availableAreas" :key="area" :value="area">
          {{ area }}
        </option>
      </select>
    </div>

    <!-- Toggle options -->
    <div class="flex flex-col gap-2 mt-1">
      <label class="flex items-center gap-2 text-xs text-text-secondary cursor-pointer">
        <input type="checkbox" v-model="hideNeutral" class="cursor-pointer" />
        Hide Neutral NPCs
      </label>
    </div>

    <!-- Service Filters (collapsible) -->
    <div class="mt-2 border-t border-surface-elevated pt-2">
      <button
        class="flex items-center gap-1 text-[10px] uppercase tracking-widest text-text-dim w-full cursor-pointer"
        @click="showServiceFilters = !showServiceFilters"
      >
        <span class="transition-transform" :class="showServiceFilters ? 'rotate-90' : ''">&#x25B6;</span>
        Service Filters
      </button>
      <div v-if="showServiceFilters" class="flex flex-col gap-1.5 mt-1.5 pl-1">
        <label class="flex items-center gap-2 text-xs text-text-secondary cursor-pointer">
          <input type="checkbox" v-model="onlyVendors" class="cursor-pointer" />
          Show only vendors
        </label>
        <label class="flex items-center gap-2 text-xs text-text-secondary cursor-pointer">
          <input type="checkbox" v-model="onlyStorage" class="cursor-pointer" />
          Show only storage NPCs
        </label>
        <label class="flex items-center gap-2 text-xs text-text-secondary cursor-pointer">
          <input type="checkbox" v-model="onlyTrainers" class="cursor-pointer" />
          Show only trainers
        </label>
      </div>
    </div>

    <!-- Status Filters (collapsible) -->
    <div class="border-t border-surface-elevated pt-2">
      <button
        class="flex items-center gap-1 text-[10px] uppercase tracking-widest text-text-dim w-full cursor-pointer"
        @click="showStatusFilters = !showStatusFilters"
      >
        <span class="transition-transform" :class="showStatusFilters ? 'rotate-90' : ''">&#x25B6;</span>
        Status Filters
      </button>
      <div v-if="showStatusFilters" class="flex flex-col gap-1.5 mt-1.5 pl-1">
        <label class="flex items-center gap-2 text-xs text-text-secondary cursor-pointer">
          <input type="checkbox" v-model="vendorsWithGold" class="cursor-pointer" />
          Vendors with gold remaining
        </label>
        <label class="flex items-center gap-2 text-xs text-text-secondary cursor-pointer">
          <input type="checkbox" v-model="openStorageSpaces" class="cursor-pointer" />
          NPCs with open storage spaces
        </label>
        <label class="flex items-center gap-2 text-xs text-text-secondary cursor-pointer">
          <input type="checkbox" v-model="unlockableStorage" class="cursor-pointer" />
          NPCs with unlockable storage
        </label>
        <label class="flex items-center gap-2 text-xs text-text-secondary cursor-pointer">
          <input type="checkbox" v-model="vendorsNotMaxGold" class="cursor-pointer" />
          Vendors not at max gold cap
        </label>
      </div>
    </div>

    <!-- Skill Filter (collapsible) -->
    <div class="border-t border-surface-elevated pt-2">
      <button
        class="flex items-center gap-1 text-[10px] uppercase tracking-widest text-text-dim w-full cursor-pointer"
        @click="showSkillFilter = !showSkillFilter"
      >
        <span class="transition-transform" :class="showSkillFilter ? 'rotate-90' : ''">&#x25B6;</span>
        Skill Filter
      </button>
      <div v-if="showSkillFilter" class="mt-1.5 pl-1">
        <input
          v-model="skillQuery"
          type="text"
          placeholder="Filter by trained skill..."
          class="px-2 py-1 bg-surface-base border border-border-default rounded text-xs text-text-primary placeholder-text-muted focus:outline-none focus:border-accent-gold/50 w-full" />
      </div>
    </div>

    <!-- Item Type Filter (collapsible) -->
    <div class="border-t border-surface-elevated pt-2">
      <button
        class="flex items-center gap-1 text-[10px] uppercase tracking-widest text-text-dim w-full cursor-pointer"
        @click="showItemTypeFilter = !showItemTypeFilter"
      >
        <span class="transition-transform" :class="showItemTypeFilter ? 'rotate-90' : ''">&#x25B6;</span>
        Item Type Filter
      </button>
      <div v-if="showItemTypeFilter" class="mt-1.5 pl-1">
        <input
          v-model="itemTypeQuery"
          type="text"
          placeholder="Filter by vendor item type..."
          class="px-2 py-1 bg-surface-base border border-border-default rounded text-xs text-text-primary placeholder-text-muted focus:outline-none focus:border-accent-gold/50 w-full" />
      </div>
    </div>

    <!-- Count -->
    <div class="mt-auto pt-2 border-t border-surface-elevated">
      <span class="text-xs text-text-dim">{{ filteredRows.length }} NPCs shown</span>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed } from 'vue'
import type { NpcInfo } from '../../types/gameData'
import type { SnapshotNpcFavor } from '../../types/database'
import type { GameStateFavor } from '../../types/gameState'
import { FAVOR_TIERS, tierIndex, tierDisplayName } from '../../composables/useFavorTiers'
import { hasVendor, hasStorage, hasTraining, getTrainingService, getStoreService, maxGoldCap } from '../../composables/useNpcServices'
import { useGameStateStore } from '../../stores/gameStateStore'

export interface NpcRow {
  npc_key: string
  display_name: string
  area_friendly_name: string | null
  snapshot_favor: string | null
  gamestate_favor: GameStateFavor | null
  cdnData: NpcInfo | null
  effective_tier: string
  has_gamestate_data: boolean
  snapshot_favor_points: number | null
  snapshot_councils_spent: number | null
  snapshot_notes: string | null
  snapshot_vendor_reset: string | null
}

const props = defineProps<{
  snapshotFavor: SnapshotNpcFavor[]
  favorByNpc: Record<string, GameStateFavor>
  npcsByKey: Record<string, NpcInfo>
}>()

const gameState = useGameStateStore()

const filter = ref('')
const groupBy = ref<'area' | 'favor' | 'none'>('favor')
const sortBy = ref<'favor' | 'name' | 'spent'>('favor')
const hideNeutral = ref(false)
const favorFilter = ref('')
const areaFilter = ref('')

// Service filters
const showServiceFilters = ref(false)
const onlyVendors = ref(false)
const onlyStorage = ref(false)
const onlyTrainers = ref(false)

// Status filters
const showStatusFilters = ref(false)
const vendorsWithGold = ref(false)
const openStorageSpaces = ref(false)
const unlockableStorage = ref(false)
const vendorsNotMaxGold = ref(false)

// Skill filter
const showSkillFilter = ref(false)
const skillQuery = ref('')

// Item type filter
const showItemTypeFilter = ref(false)
const itemTypeQuery = ref('')

// Interactable CDN entries that are not NPCs: no favor tiers, no gift tastes —
// boards, altars, workstations, furniture. Shared with NpcsScreen's totals so
// counts can never disagree with the grid. EventNPC_/LiveNpc_/Poi* entries are
// real NPCs and stay. Re-audit against live CDN if the grid shows furniture.
const NON_NPC_CDN_KEYS = new Set([
  'Altar_Druid',
  'BearCharmPedestal',
  'CatCharmPedestal',
  'Grainmill',
  'GuildAltar',
  'PoiBrewingWorkstation',
  'WerewolfAltar',
])
const isExcludedCdnKey = (key: string) =>
  key.endsWith('WorkOrderSign') || NON_NPC_CDN_KEYS.has(key)

// Build merged rows from the CDN NPC set with snapshot/live favor overlaid as
// tier/details. Report-only NPC keys (NPC_ prefix) with no CDN match are
// appended so favor-bearing renames are never lost; they render no card
// (NpcCard requires NpcInfo) and rejoin the grid with full data if a later CDN
// snapshot includes them. Non-NPC favor keys (BeeHiveA, Cow_Bertha, …) are game
// entities with favor meters, not NPCs — excluded everywhere.
const allRows = computed<NpcRow[]>(() => {
  const seen = new Set<string>()
  const rows: NpcRow[] = []
  const snapByKey = new Map(props.snapshotFavor.map(s => [s.npc_key, s]))

  // CDN pass: the full NPC universe, snapshot/live favor overlaid
  for (const cdn of Object.values(props.npcsByKey)) {
    if (isExcludedCdnKey(cdn.key)) continue
    seen.add(cdn.key)
    const snap = snapByKey.get(cdn.key)
    const gs = props.favorByNpc[cdn.key] ?? null
    rows.push({
      npc_key: cdn.key,
      display_name: cdn.name,
      area_friendly_name: cdn.area_friendly_name ?? null,
      snapshot_favor: snap?.favor_level ?? null,
      gamestate_favor: gs,
      cdnData: cdn,
      effective_tier: gs?.favor_tier ?? snap?.favor_level ?? 'Neutral',
      has_gamestate_data: !!gs,
      snapshot_favor_points: snap?.favor ?? null,
      snapshot_councils_spent: snap?.councils_spent_at_vendor ?? null,
      snapshot_notes: snap?.player_notes ?? null,
      snapshot_vendor_reset: snap?.vendor_reset_time ?? null,
    })
  }

  // Orphan passes: report-only NPC_ keys with no CDN match. Null area +
  // null cdnData; counted in the panel, card-less in the grid by decision.
  for (const snap of props.snapshotFavor) {
    if (seen.has(snap.npc_key) || isExcludedCdnKey(snap.npc_key) || !snap.npc_key.startsWith('NPC_')) continue
    seen.add(snap.npc_key)
    const gs = props.favorByNpc[snap.npc_key] ?? null
    rows.push({
      npc_key: snap.npc_key,
      display_name: gs?.npc_name ?? snap.npc_key.replace(/^NPC_/, ''),
      area_friendly_name: null,
      snapshot_favor: snap.favor_level,
      gamestate_favor: gs,
      cdnData: null,
      effective_tier: gs?.favor_tier ?? snap.favor_level,
      has_gamestate_data: !!gs,
      snapshot_favor_points: snap.favor ?? null,
      snapshot_councils_spent: snap.councils_spent_at_vendor ?? null,
      snapshot_notes: snap.player_notes ?? null,
      snapshot_vendor_reset: snap.vendor_reset_time ?? null,
    })
  }

  for (const [key, gs] of Object.entries(props.favorByNpc)) {
    if (seen.has(key) || isExcludedCdnKey(key) || !key.startsWith('NPC_')) continue
    rows.push({
      npc_key: key,
      display_name: gs.npc_name ?? key.replace(/^NPC_/, ''),
      area_friendly_name: null,
      snapshot_favor: null,
      gamestate_favor: gs,
      cdnData: null,
      effective_tier: gs.favor_tier ?? 'Neutral',
      has_gamestate_data: true,
      snapshot_favor_points: null,
      snapshot_councils_spent: null,
      snapshot_notes: null,
      snapshot_vendor_reset: null,
    })
  }

  return rows
})

// Collect available areas for the area dropdown
const availableAreas = computed(() => {
  const areas = new Set<string>()
  let hasUnknown = false
  for (const row of allRows.value) {
    if (row.area_friendly_name) areas.add(row.area_friendly_name)
    else hasUnknown = true
  }
  const sorted = Array.from(areas).sort()
  if (hasUnknown) sorted.push('Unknown Area')
  return sorted
})

// Filter
const filteredAndSorted = computed(() => {
  let rows = allRows.value

  if (hideNeutral.value) {
    rows = rows.filter(r => r.effective_tier !== 'Neutral')
  }

  if (favorFilter.value) {
    rows = rows.filter(r => r.effective_tier === favorFilter.value)
  }

  if (areaFilter.value) {
    if (areaFilter.value === 'Unknown Area') {
      rows = rows.filter(r => !r.area_friendly_name)
    } else {
      rows = rows.filter(r => r.area_friendly_name === areaFilter.value)
    }
  }

  const f = filter.value.toLowerCase()
  if (f) {
    rows = rows.filter(r =>
      r.display_name.toLowerCase().includes(f)
      || (r.area_friendly_name?.toLowerCase().includes(f) ?? false)
      || r.effective_tier.toLowerCase().includes(f)
      || (r.snapshot_notes?.toLowerCase().includes(f) ?? false)
    )
  }

  // Service filters — these are additive (OR) when multiple are checked
  const anyServiceFilter = onlyVendors.value || onlyStorage.value || onlyTrainers.value
  if (anyServiceFilter) {
    rows = rows.filter(r => {
      if (!r.cdnData) return false
      if (onlyVendors.value && hasVendor(r.cdnData)) return true
      if (onlyStorage.value && hasStorage(r.cdnData)) return true
      if (onlyTrainers.value && hasTraining(r.cdnData)) return true
      return false
    })
  }

  // Status filters — each is independent (AND)
  if (vendorsWithGold.value) {
    rows = rows.filter(r => {
      if (!r.cdnData || !hasVendor(r.cdnData)) return false
      const vendor = gameState.vendorByNpc[r.npc_key]
      return vendor != null && vendor.vendor_gold_available != null && vendor.vendor_gold_available > 0
    })
  }

  if (openStorageSpaces.value) {
    rows = rows.filter(r => {
      if (!r.cdnData || !hasStorage(r.cdnData)) return false
      const items = gameState.storageByVault[r.npc_key]
      const vault = gameState.storageVaultsByKey[r.npc_key]
      if (!vault) return false
      const used = items ? items.length : 0
      const total = gameState.getVaultUnlockedSlots(vault)
      return total != null && used < total
    })
  }

  if (unlockableStorage.value) {
    rows = rows.filter(r => {
      if (!r.cdnData || !hasStorage(r.cdnData)) return false
      const vault = gameState.storageVaultsByKey[r.npc_key]
      if (!vault) return true // no vault data, might be unlockable
      const currentSlots = gameState.getVaultUnlockedSlots(vault)
      const maxSlots = gameState.getVaultMaxPossibleSlots(vault)
      if (currentSlots == null || maxSlots == null) return true
      return currentSlots < maxSlots
    })
  }

  if (vendorsNotMaxGold.value) {
    rows = rows.filter(r => {
      if (!r.cdnData || !hasVendor(r.cdnData)) return false
      const vendor = gameState.vendorByNpc[r.npc_key]
      const cap = maxGoldCap(r.cdnData)
      if (!vendor || vendor.vendor_gold_max == null || !cap) return true
      return vendor.vendor_gold_max < cap.maxGold
    })
  }

  // Skill filter
  const sq = skillQuery.value.toLowerCase().trim()
  if (sq) {
    rows = rows.filter(r => {
      if (!r.cdnData) return false
      const training = getTrainingService(r.cdnData)
      if (!training) return false
      return training.skills.some(s => s.toLowerCase().includes(sq))
    })
  }

  // Item type filter
  const itq = itemTypeQuery.value.toLowerCase().trim()
  if (itq) {
    rows = rows.filter(r => {
      if (!r.cdnData) return false
      const store = getStoreService(r.cdnData)
      if (!store) return false
      return store.capIncreases.some(cap =>
        cap.itemTypes.some(t => t.toLowerCase().includes(itq))
      )
    })
  }

  // Sort
  const sorted = [...rows]
  switch (sortBy.value) {
    case 'favor':
      sorted.sort((a, b) => tierIndex(a.effective_tier) - tierIndex(b.effective_tier) || a.display_name.localeCompare(b.display_name))
      break
    case 'name':
      sorted.sort((a, b) => a.display_name.localeCompare(b.display_name))
      break
    case 'spent':
      sorted.sort((a, b) => (b.snapshot_councils_spent ?? -1) - (a.snapshot_councils_spent ?? -1) || a.display_name.localeCompare(b.display_name))
      break
  }

  return sorted
})

// Expose to parent
const filteredRows = computed(() => filteredAndSorted.value)

defineExpose({
  filteredRows,
})
</script>
