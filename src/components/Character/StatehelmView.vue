<template>
  <PaneLayout
    screen-key="statehelm"
    :right-pane="selectedNpcKey ? { title: 'NPC Details', defaultWidth: 700, minWidth: 400, maxWidth: 1200 } : undefined">
    <div class="h-full overflow-y-auto p-3 flex flex-col gap-3">
      <!-- Header -->
      <div class="flex items-center justify-between">
        <h2 class="text-lg font-semibold text-text-primary">Statehelm Reputation</h2>
        <div class="flex items-center gap-3">
          <div v-if="filteredStatuses.length > 0" class="flex items-center gap-3">
            <span class="text-sm text-text-muted">
              {{ totalGiftsGiven }} / {{ totalGiftsMax }} gifts this week
            </span>
            <span class="text-xs text-text-dim">
              Resets {{ weekResetLabel }}
            </span>
          </div>
          <button
            @click="rescanLogs"
            :disabled="rescanning"
            class="px-3 py-1.5 bg-surface-elevated border border-border-light rounded text-text-secondary text-sm cursor-pointer transition-all hover:bg-border-default hover:text-text-primary disabled:opacity-50 disabled:cursor-not-allowed whitespace-nowrap"
            title="Re-read Player.log, Player-prev.log and archived copies, and reconcile each NPC's weekly gift count to the game's own note"
          >{{ rescanning ? 'Rescanning…' : 'Rescan logs' }}</button>
        </div>
      </div>

      <!-- Rescan status -->
      <div v-if="rescanMessage" class="text-xs" :class="rescanError ? 'text-value-negative' : 'text-text-dim'">
        {{ rescanMessage }}
      </div>

      <!-- Filters -->
      <div v-if="npcStatuses.length > 0" class="flex items-center gap-4 flex-wrap text-xs">
        <!-- Hide above favor tier -->
        <label class="flex items-center gap-1.5 text-text-secondary">
          Hide above
          <select
            v-model="hideAboveTier"
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

        <span class="text-border-default">|</span>

        <!-- Hide maxed gifts -->
        <label class="flex items-center gap-1 text-text-secondary cursor-pointer">
          <input type="checkbox" v-model="hideMaxedGifts" class="accent-accent-gold" />
          Hide maxed gifts
        </label>
      </div>

      <EmptyState
        v-if="!loading && npcStatuses.length === 0"
        primary="No giftable Statehelm NPCs found."
        secondary="NPC data may still be loading, or no Statehelm NPCs have gift preferences configured." />

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
          :gift-tracking="{ giftsThisWeek: status.giftsThisWeek, maxGifts: status.maxGifts }"
          :vendor-status="gameState.vendorByNpc[status.npc.key] ?? null"
          :show-gift-tracking="true"
          :selected="selectedNpcKey === status.npc.key"
          @select="selectNpc(status.npc.key)"
        >
          <template #gift-actions>
            <div class="flex items-center gap-0.5">
              <button
                class="w-4 h-4 flex items-center justify-center rounded text-[10px] text-text-muted hover:text-text-primary hover:bg-surface-default transition-colors"
                :class="{ 'opacity-30 pointer-events-none': status.giftsThisWeek <= 0 }"
                title="Remove a gift"
                @click.stop="removeGift(status.npc.key)">
                -
              </button>
              <button
                class="w-4 h-4 flex items-center justify-center rounded text-[10px] text-text-muted hover:text-text-primary hover:bg-surface-default transition-colors"
                :class="{ 'opacity-30 pointer-events-none': status.giftsThisWeek >= status.maxGifts }"
                title="Add a gift"
                @click.stop="addGift(status.npc.key, status.npc.name)">
                +
              </button>
            </div>
          </template>
        </NpcCard>
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
import { computed, onMounted, ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
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
  totalGiftsGiven,
  totalGiftsMax,
  loading,
  loadGiftLog,
  addGift,
  removeGift,
  nextWeekStart,
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
const hideAboveTier = ref('')
const showOnlyTrainers = ref(false)
const showOnlyVendors = ref(false)
const showOnlyStorage = ref(false)
const hideMaxedGifts = ref(false)

// Tiers available for the "hide above" dropdown (exclude Despised — hiding above Despised hides everything)
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
    // Hide above tier filter
    if (hideAboveTier.value && status.favorTier) {
      if (isTierAtOrAbove(status.favorTier, hideAboveTier.value)) return false
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

    // Hide maxed gifts
    if (hideMaxedGifts.value && status.giftsThisWeek >= status.maxGifts) return false

    return true
  })
})

const weekResetLabel = computed(() => {
  const next = new Date(nextWeekStart.value)
  const now = new Date()
  const diffMs = next.getTime() - now.getTime()
  const diffDays = Math.floor(diffMs / (1000 * 60 * 60 * 24))
  const diffHours = Math.floor((diffMs % (1000 * 60 * 60 * 24)) / (1000 * 60 * 60))
  if (diffDays > 0) return `in ${diffDays}d ${diffHours}h`
  return `in ${diffHours}h`
})

// ── Rescan logs ─────────────────────────────────────────────────────────
// Re-read the game's own Player.log / Player-prev.log / archived copies and
// reconcile each capped NPC's weekly gift count to the game's note.
const rescanning = ref(false)
const rescanMessage = ref('')
const rescanError = ref(false)

async function rescanLogs() {
  rescanning.value = true
  rescanMessage.value = ''
  rescanError.value = false
  try {
    const n = await invoke<number>('backfill_statehelm_gifts')
    await loadGiftLog()
    rescanMessage.value = n > 0
      ? `Reconciled ${n} NPC${n === 1 ? '' : 's'} from the logs.`
      : 'No new gift info found in the logs.'
  } catch (e) {
    rescanError.value = true
    rescanMessage.value = `Rescan failed: ${e}`
    console.error('[statehelm] Rescan failed:', e)
  } finally {
    rescanning.value = false
  }
}

onMounted(() => {
  loadGiftLog()
})
</script>
