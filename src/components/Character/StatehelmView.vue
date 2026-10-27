<template>
  <PaneLayout
    screen-key="statehelm"
    :right-pane="selectedNpcKey ? { title: 'NPC Details', defaultWidth: 700, minWidth: 400, maxWidth: 1200 } : undefined">
    <div class="h-full overflow-y-auto p-3 flex flex-col gap-3">
      <!-- Header -->
      <div class="flex items-center justify-between">
        <h2 class="text-lg font-semibold text-text-primary">Statehelm Reputation</h2>
      </div>

      <!-- Filters -->
      <div v-if="npcStatuses.length > 0" class="flex items-center gap-4 flex-wrap text-xs">
        <!-- Hide at-or-above favor tier -->
        <label class="flex items-center gap-1.5 text-text-secondary">
          Hide at/above
          <select
            v-model="hideAtOrAboveTier"
            class="bg-surface-elevated border border-border-default rounded px-1.5 py-0.5 text-xs text-text-primary cursor-pointer">
            <option value="">Off</option>
            <option v-for="tier in filterableTiers" :key="tier" :value="tier">
              {{ tierDisplayName(tier) }}
            </option>
          </select>
        </label>

        <!-- Show only filters -->
        <label class="flex items-center gap-1 text-text-secondary cursor-pointer">
          <input type="checkbox" v-model="showOnlyTrainers" class="accent-accent-gold" />
          Trainers
        </label>
        <label class="flex items-center gap-1 text-text-secondary cursor-pointer">
          <input type="checkbox" v-model="showOnlyVendors" class="accent-accent-gold" />
          Vendors
        </label>
        <label class="flex items-center gap-1 text-text-secondary cursor-pointer">
          <input type="checkbox" v-model="showOnlyStorage" class="accent-accent-gold" />
          Storage
        </label>

      </div>

      <EmptyState
        v-if="!loading && npcStatuses.length === 0"
        primary="No Statehelm NPCs found."
        secondary="NPC data may still be loading — the CDN game data is fetched in the background at startup." />

      <EmptyState
        v-else-if="npcStatuses.length > 0 && filteredStatuses.length === 0"
        primary="All NPCs hidden by filters."
        secondary="Adjust the filter controls above to show NPCs." />

      <!-- NPC Cards -->
      <div v-if="filteredStatuses.length > 0" class="grid grid-cols-2 xl:grid-cols-3 2xl:grid-cols-4 min-[1800px]:grid-cols-5 gap-2">
        <NpcCard
          v-for="status in filteredStatuses"
          :key="status.npc.key"
          :npc="status.npc"
          :favor-tier="status.favorTier"
          :vendor-status="gameState.vendorByNpc[status.npc.key] ?? null"
          :selected="selectedNpcKey === status.npc.key"
          @select="selectNpc(status.npc.key)"
        />
      </div>
    </div>

    <template #right>
      <NpcDetailPanel
        :npc-key="selectedNpcKey"
        :snapshot-tier="selectedSnapshotTier"
        :gamestate-favor="selectedGamestateFavor"
        :cdn-data="selectedCdnData"
        :vendor-status="selectedVendorStatus" />
    </template>
  </PaneLayout>
</template>

<script setup lang="ts">
import { computed, ref } from 'vue'
import { useStatehelmTracker } from '../../composables/useStatehelmTracker'
import { useGameStateStore } from '../../stores/gameStateStore'
import { useGameDataStore } from '../../stores/gameDataStore'
import { tierDisplayName, isTierAtOrAbove, FAVOR_TIERS } from '../../composables/useFavorTiers'
import { getServices } from '../../composables/useNpcServices'
import EmptyState from '../Shared/EmptyState.vue'
import PaneLayout from '../Shared/PaneLayout.vue'
import NpcCard from '../Shared/NPC/NpcCard.vue'
import NpcDetailPanel from './NpcDetailPanel.vue'

const gameState = useGameStateStore()
const gameData = useGameDataStore()

const {
  npcStatuses,
  loading,
} = useStatehelmTracker()

// Selection state
const selectedNpcKey = ref<string | null>(null)

function selectNpc(key: string) {
  selectedNpcKey.value = selectedNpcKey.value === key ? null : key
}

// Computed data for the detail panel
const selectedSnapshotTier = computed(() => {
  if (!selectedNpcKey.value) return null
  const status = npcStatuses.value.find(s => s.npc.key === selectedNpcKey.value)
  return status?.favorTier ?? null
})

const selectedGamestateFavor = computed(() => {
  if (!selectedNpcKey.value) return null
  return gameState.favorByNpc[selectedNpcKey.value] ?? null
})

const selectedCdnData = computed(() => {
  if (!selectedNpcKey.value) return null
  return gameData.npcsByKey[selectedNpcKey.value] ?? null
})

const selectedVendorStatus = computed(() => {
  if (!selectedNpcKey.value) return null
  return gameState.vendorByNpc[selectedNpcKey.value] ?? null
})

// Filter state
const hideAtOrAboveTier = ref('')
const showOnlyTrainers = ref(false)
const showOnlyVendors = ref(false)
const showOnlyStorage = ref(false)


// Tiers available for the "hide at/above" dropdown (exclude Despised — hiding at/above Despised hides everything)
const filterableTiers = FAVOR_TIERS.filter(t => t !== 'Despised')

// Enrich statuses with parsed services
const enrichedStatuses = computed(() => {
  return npcStatuses.value.map(status => {
    const services = getServices(status.npc)
    return {
      ...status,
      hasTraining: services.some(s => s.type === 'Training'),
      hasVendor: services.some(s => s.type === 'Store'),
      hasStorage: services.some(s => s.type === 'Storage'),
    }
  })
})

const filteredStatuses = computed(() => {
  return enrichedStatuses.value.filter(status => {
    // Hide at-or-above tier filter
    if (hideAtOrAboveTier.value && status.favorTier) {
      if (isTierAtOrAbove(status.favorTier, hideAtOrAboveTier.value)) return false
    }

    // Service filters (additive — if any are checked, NPC must match at least one)
    const anyServiceFilter = showOnlyTrainers.value || showOnlyVendors.value || showOnlyStorage.value
    if (anyServiceFilter) {
      const matches =
        (showOnlyTrainers.value && status.hasTraining) ||
        (showOnlyVendors.value && status.hasVendor) ||
        (showOnlyStorage.value && status.hasStorage)
      if (!matches) return false
    }

    return true
  })
})

</script>
