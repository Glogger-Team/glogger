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
          :gift-tracking="{ giftsThisWeek: status.giftsThisWeek, maxGifts: status.maxGifts }"
          :show-gift-tracking="status.giftTracking"
          :selected="selectedNpcKey === status.npc.key"
          @select="selectNpc(status.npc.key)"
        />
      </div>
      <!-- Gift Planner -->
      <div class="border-t border-border pt-3 flex flex-col gap-2">
        <div class="flex items-center justify-between flex-wrap gap-2">
          <h3 class="dialog-title">Gift Planner</h3>
          <button
            class="text-xs px-2 py-1 rounded bg-accent-gold/15 text-accent-gold hover:bg-accent-gold/25 disabled:opacity-40"
            :disabled="planner.loading.value || giftableStatuses.length === 0"
            @click="searchVaultGifts()">
            {{ planner.loading.value ? 'Searching vaults…' : 'Find vault gifts' }}
          </button>
        </div>
        <p v-if="giftableStatuses.length === 0" class="text-xs text-text-dim">
          All gift-tracked NPCs are at their weekly gift cap — nothing left to plan.
        </p>
        <p v-else class="text-xs text-text-dim">
          Matches vault items against gift preferences (Love first, then Like) and assigns each
          stack to the highest-benefit NPC with weekly gifts remaining. Uncheck NPCs to exclude them.
        </p>
        <div v-if="giftableStatuses.length > 0" class="flex flex-wrap gap-x-3 gap-y-1 text-xs">
          <label
            v-for="s in giftableStatuses"
            :key="s.npc.key"
            class="flex items-center gap-1 text-text-secondary cursor-pointer">
            <input
              type="checkbox"
              :checked="!planner.excludedNpcKeys.value.has(s.npc.key)"
              @change="togglePlannerNpc(s.npc.key)" />
            {{ s.npc.name }} ({{ s.maxGifts - s.giftsThisWeek }} left)
          </label>
        </div>
        <p v-if="planner.error.value" class="text-xs text-error">{{ planner.error.value }}</p>
        <p
          v-else-if="!planner.loading.value && planner.matches.value.length === 0 && plannerLoaded"
          class="text-xs text-text-dim">
          No vault items match these NPCs' preferences — try stocking up, or check Gift Watcher for carried items.
        </p>
        <div v-for="g in planner.vaultGroups.value" :key="g.vaultKey" class="flex flex-col gap-1">
          <h4 class="text-xs font-semibold text-text-secondary">{{ g.vaultLabel }}</h4>
          <DataTable
            :columns="giftGroupColumns"
            :rows="giftGroupRows(g)"
            :row-key="(row) => row.key as string"
            :compact="true"
            :hoverable="false">
            <template #cell-item="{ row }">
              {{ row.item }}<span class="text-text-dim"> ×{{ row.stack }}</span>
            </template>
            <template #cell-want="{ value }">
              <span :class="value === 'Love' ? 'text-accent-red' : 'text-accent-gold'">
                {{ value === 'Love' ? '♥' : '♦' }} {{ value }}
              </span>
            </template>
          </DataTable>
        </div>
        <div v-if="planner.assignments.value.length > 0" class="flex items-center gap-2 flex-wrap">
          <button
            class="text-xs px-2 py-1 rounded bg-accent-gold/15 text-accent-gold hover:bg-accent-gold/25 disabled:opacity-40"
            :disabled="planner.planning.value"
            @click="planner.planRoute()">
            {{ planner.planning.value ? 'Planning…' : 'Plan pickup route' }}
          </button>
          <p v-if="planner.routeError.value" class="text-xs text-error">{{ planner.routeError.value }}</p>
        </div>
        <ol v-if="planner.route.value" class="flex flex-col gap-1 text-xs">
          <li
            v-for="(step, i) in planner.route.value.steps"
            :key="i"
            class="flex gap-2 items-baseline">
            <span class="text-text-dim font-mono shrink-0">{{ i + 1 }}.</span>
            <span class="px-1 rounded bg-surface-elevated/50 text-text-secondary shrink-0">{{ step.action }}</span>
            <span>{{ step.details }}</span>
          </li>
        </ol>
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
import { useStatehelmTracker } from '../../composables/useStatehelmTracker'
import { useStatehelmPlanner, type VaultGiftGroup } from '../../composables/useStatehelmPlanner'
import { useGameStateStore } from '../../stores/gameStateStore'
import { useGameDataStore } from '../../stores/gameDataStore'
import { tierDisplayName, isTierAtOrAbove, FAVOR_TIERS } from '../../composables/useFavorTiers'
import { getServices } from '../../composables/useNpcServices'
import EmptyState from '../Shared/EmptyState.vue'
import PaneLayout from '../Shared/PaneLayout.vue'
import NpcCard from '../Shared/NPC/NpcCard.vue'
import DataTable, { type ColumnDef } from '../Shared/DataTable.vue'
import NpcDetailPanel from './NpcDetailPanel.vue'

const gameState = useGameStateStore()
const gameData = useGameDataStore()

const {
  npcStatuses,
  loading,
  loadGiftLog,
} = useStatehelmTracker()
const planner = useStatehelmPlanner()
// Columns for the per-vault gift tables (shared across vault groups).
const giftGroupColumns: ColumnDef[] = [
  { key: 'item', label: 'Item' },
  { key: 'for', label: 'For' },
  { key: 'want', label: 'Want' },
]
function giftGroupRows(g: VaultGiftGroup): Record<string, unknown>[] {
  return g.assignments.map((a) => ({
    key: `${a.match.npc_key}|${a.match.item_name}`,
    item: a.match.item_name,
    stack: a.match.stack_size,
    for: a.match.npc_name,
    want: a.match.desire,
  }))
}
// True once a vault search has completed — distinguishes "not searched yet"
// from "searched, no matches" for the empty state.
const plannerLoaded = ref(false)
// NPCs eligible for planning, independent of the planner's exclusion set so
// an excluded NPC stays visible and can be re-included.
const giftableStatuses = computed(() =>
  npcStatuses.value.filter(s => s.giftTracking && s.giftsThisWeek < s.maxGifts),
)

async function searchVaultGifts() {
  await planner.loadMatches()
  plannerLoaded.value = true
}

function togglePlannerNpc(npcKey: string) {
  planner.toggleExcluded(npcKey)
  plannerLoaded.value = false
}

onMounted(() => {
  loadGiftLog()
})

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
