<template>
  <PaneLayout
    screen-key="statehelm"
    :right-pane="selectedNpcKey ? { title: 'NPC Details', defaultWidth: 700, minWidth: 400, maxWidth: 1200 } : undefined">
    <div class="h-full overflow-y-auto p-3 flex flex-col gap-3">
      <!-- Header -->
      <div class="flex items-center justify-between">
        <h2 class="screen-title">Statehelm Reputation</h2>
      </div>

      <!-- Filters -->
      <div v-if="npcStatuses.length > 0" class="flex items-center gap-4 flex-wrap text-xs">
        <!-- Text search across NPC and gift names (applies to all tabs) -->
        <input
          v-model="searchQuery"
          type="text"
          placeholder="Search NPCs or gifts…"
          class="bg-surface-elevated border border-border-default rounded px-1.5 py-0.5 text-xs text-text-primary placeholder:text-text-dim w-44" />
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

      <!-- View tabs: all three render from the same filtered+searched statuses -->
      <TabBar v-if="npcStatuses.length > 0" v-model="activeTab" :tabs="tabs" />

      <EmptyState
        v-if="!loading && npcStatuses.length === 0"
        primary="No Statehelm NPCs found."
        secondary="NPC data may still be loading — the CDN game data is fetched in the background at startup." />

      <EmptyState
        v-else-if="npcStatuses.length > 0 && searchedStatuses.length === 0"
        primary="All NPCs hidden by filters."
        secondary="Adjust the filter controls or search above to show NPCs." />

      <!-- By NPC: collapsible cards (header always visible, body expands) -->
      <div v-if="searchedStatuses.length > 0 && activeTab === 'npc'" class="flex flex-col gap-2">
        <div class="flex items-center justify-end gap-2 text-xs">
          <button
            class="text-text-dim hover:text-text-primary"
            @click="expandAllNpcs()">
            Expand all
          </button>
          <span class="text-border-default">|</span>
          <button
            class="text-text-dim hover:text-text-primary"
            @click="collapseAllNpcs()">
            Collapse all
          </button>
        </div>
        <div class="grid grid-cols-2 xl:grid-cols-3 2xl:grid-cols-4 min-[1800px]:grid-cols-5 gap-2">
          <div v-for="status in searchedStatuses" :key="status.npc.key" class="flex flex-col gap-1 min-w-0">
            <button
              class="flex items-center gap-2 px-2 py-1 rounded border text-left"
              :class="selectedNpcKey === status.npc.key ? 'border-green-700/40 bg-surface-elevated' : 'border-border-default bg-surface-elevated/50 hover:bg-surface-elevated'"
              :title="isCollapsed(status.npc.key) ? 'Expand' : 'Collapse'"
              @click="toggleCollapsed(status.npc.key)">
              <span class="text-text-dim font-mono text-xs w-3 shrink-0">{{ isCollapsed(status.npc.key) ? '▸' : '▾' }}</span>
              <span class="text-xs font-medium text-text-primary flex-1 truncate">{{ status.npc.name }}</span>
              <span v-if="status.favorTier" class="text-[0.6rem] text-text-dim shrink-0">{{ tierDisplayName(status.favorTier) }}</span>
              <span class="text-[0.6rem] text-text-dim shrink-0">{{ status.maxGifts - status.giftsThisWeek }} left</span>
            </button>
            <NpcCard
              v-show="!isCollapsed(status.npc.key)"
              :npc="status.npc"
              :favor-tier="status.favorTier"
              :vendor-status="gameState.vendorByNpc[status.npc.key] ?? null"
              :gift-tracking="{ giftsThisWeek: status.giftsThisWeek, maxGifts: status.maxGifts }"
              :show-gift-tracking="status.giftTracking"
              :show-header="false"
              :selected="selectedNpcKey === status.npc.key"
              @select="selectNpc(status.npc.key)"
            />
          </div>
        </div>
      </div>

      <!-- By Gift: one row per gift, listing the NPCs that want it -->
      <div v-if="searchedStatuses.length > 0 && activeTab === 'gift'" class="flex flex-col gap-2">
        <p v-if="!plannerLoaded" class="text-xs text-text-dim">
          Run “Find vault gifts” in the Gift Planner to sort by vault stock.
        </p>
        <EmptyState
          v-if="giftGroups.length === 0"
          primary="No gift preferences found."
          secondary="The shown NPCs have no recorded likes or loves." />
        <div
          v-for="g in giftGroups"
          :key="g.key"
          class="bg-surface-elevated border border-border-default rounded px-2.5 py-2 flex flex-col gap-1.5">
          <div class="flex items-center gap-2 min-w-0">
            <span
              class="text-[0.6rem] px-1 py-0.5 rounded border min-w-8 text-center shrink-0"
              :class="g.want === 'Love' ? 'bg-red-900/30 border-red-700/40 text-red-300' : 'bg-green-900/30 border-green-700/40 text-green-300'">
              {{ g.want === 'Love' ? '♥' : '♦' }} {{ g.want }}
            </span>
            <span class="text-sm font-medium text-text-primary truncate">{{ g.label }}</span>
            <span class="text-xs text-text-dim shrink-0">({{ g.entries.length }} NPC{{ g.entries.length === 1 ? '' : 's' }})</span>
            <span v-if="plannerLoaded" class="text-xs text-accent-gold shrink-0">×{{ giftStockCounts.get(g.key) ?? 0 }} in vaults</span>
          </div>
          <div class="flex flex-wrap gap-1">
            <button
              v-for="e in g.entries"
              :key="e.npcKey"
              class="text-xs px-1.5 py-0.5 rounded bg-surface-inset border hover:border-accent-gold/50"
              :class="selectedNpcKey === e.npcKey ? 'border-green-700/40 text-text-primary' : 'border-border-default text-text-secondary'"
              :title="e.remaining > 0 ? `${e.remaining} weekly gifts left` : 'At the weekly gift cap'"
              @click="selectNpc(e.npcKey)">
              {{ e.npcName }}
              <span class="text-text-dim">{{ e.remaining > 0 ? `(${e.remaining} left)` : '(capped)' }}</span>
            </button>
          </div>
        </div>
      </div>

      <!-- Shopping List: flat consolidated gift + qty (actionable slots only) -->
      <div v-if="searchedStatuses.length > 0 && activeTab === 'list'" class="flex flex-col gap-2">
        <EmptyState
          v-if="shoppingRows.length === 0 && giftGroups.length === 0"
          primary="No gift preferences found."
          secondary="The shown NPCs have no recorded likes or loves." />
        <EmptyState
          v-else-if="shoppingRows.length === 0"
          primary="Nothing left to shop for."
          secondary="Every NPC wanting these gifts is at the weekly cap." />
        <div
          v-else
          class="flex flex-col rounded border border-border-default overflow-hidden">
          <div class="grid grid-cols-[minmax(0,1.4fr)_minmax(0,2fr)_auto_auto] gap-2 px-2 py-1 text-[0.6rem] uppercase tracking-widest text-text-dim border-b border-border-default bg-surface-elevated items-baseline">
            <span>Gift</span>
            <span>For</span>
            <span>Want</span>
            <span class="text-right">Qty</span>
          </div>
          <div
            v-for="r in shoppingRows"
            :key="r.key"
            class="grid grid-cols-[minmax(0,1.4fr)_minmax(0,2fr)_auto_auto] gap-2 px-2 py-1 text-xs border-b border-border-default/50 last:border-0 bg-surface-elevated/50 items-baseline">
            <span class="text-text-primary truncate">{{ r.item }}</span>
            <span class="text-text-secondary truncate" :title="r.npcs">{{ r.npcs }}</span>
            <span :class="r.want === 'Love' ? 'text-accent-red' : 'text-accent-gold'" class="whitespace-nowrap">
              {{ r.want === 'Love' ? '♥' : '♦' }} {{ r.want }}
            </span>
            <span class="text-right font-mono text-text-primary">×{{ r.qty }}</span>
          </div>
        </div>
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
import type { NpcPreference } from '../../types/gameData/npcs'
import EmptyState from '../Shared/EmptyState.vue'
import PaneLayout from '../Shared/PaneLayout.vue'
import TabBar, { type Tab } from '../Shared/TabBar.vue'
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

// ── Tabs + search ───────────────────────────────────────────────────────────
// All three tabs render from `searchedStatuses`: the tier/service filters
// plus the text search over NPC and gift names.
const activeTab = ref('npc')
const tabs: Tab[] = [
  { id: 'npc', label: 'By NPC' },
  { id: 'gift', label: 'By Gift' },
  { id: 'list', label: 'Shopping List' },
]
const searchQuery = ref('')

function prefLabel(pref: NpcPreference): string {
  return pref.name ?? pref.keywords.join(', ')
}

// A "want" is a giftable preference — loves and likes. Hates (and anything
// else) are never something to shop for, so they stay out of the gift views
// (they still show on the NPC cards themselves).
function isGiftableDesire(desire: string): boolean {
  const d = desire.toLowerCase()
  return d === 'love' || d === 'like'
}

const searchedStatuses = computed(() => {
  const q = searchQuery.value.trim().toLowerCase()
  if (!q) return filteredStatuses.value
  return filteredStatuses.value.filter(status => {
    if (status.npc.name.toLowerCase().includes(q)) return true
    return (status.npc.preferences ?? []).some(p => prefLabel(p).toLowerCase().includes(q))
  })
})

// ── Collapsible NPC cards ───────────────────────────────────────────────────
// Header row per card is always visible; the NpcCard body expands beneath it.
// Default is expanded (previous behavior); collapse-all gives the scannable
// overview for the 20+ card grid.
const collapsedNpcKeys = ref<Set<string>>(new Set())
function isCollapsed(npcKey: string): boolean {
  return collapsedNpcKeys.value.has(npcKey)
}
function toggleCollapsed(npcKey: string) {
  const next = new Set(collapsedNpcKeys.value)
  if (next.has(npcKey)) next.delete(npcKey)
  else next.add(npcKey)
  collapsedNpcKeys.value = next
}
function expandAllNpcs() {
  collapsedNpcKeys.value = new Set()
}
function collapseAllNpcs() {
  collapsedNpcKeys.value = new Set(searchedStatuses.value.map(s => s.npc.key))
}

// ── By Gift + Shopping List ─────────────────────────────────────────────────
interface GiftWantEntry {
  npcKey: string
  npcName: string
  remaining: number
  desire: string
  prefName: string | null
  prefKeywords: string[]
}
interface GiftGroup {
  key: string
  label: string
  want: 'Love' | 'Like'
  entries: GiftWantEntry[]
}

// Desire rank mirroring the backend matcher (`desire_rank` in
// `src-tauri/src/cdn_commands.rs`): Love outranks Like, anything else is
// unmatchable and never reaches the frontend groups.
function giftDesireRank(desire: string): number {
  const d = desire.toLowerCase()
  if (d === 'love') return 1
  if (d === 'like') return 0
  return -1
}

// Vault stock per gift-group key, routed from the planner matcher output
// (`planner.matches`, the `plan_statehelm_gifts` rows) — there is no second
// matcher. A "name" row lands on the group whose preference name equals the
// item name (the backend's `npc_favor_by_item_name` key); a "keyword:K" row
// lands on the group of that NPC's winning preference containing K (highest
// desire rank, then pref — the backend's `best` dedup order). The count is
// the sum of attributed row stacks, exactly what the planner tables show
// per row — a stack wanted by two NPCs counts toward both groups, same as
// the planner's per-row display. Empty until the planner runs (see
// `plannerLoaded`): unknown groups keep alphabetical position.
const giftStockCounts = computed<Map<string, number>>(() => {
  const out = new Map<string, number>()
  if (!plannerLoaded.value) return out
  for (const m of planner.matches.value) {
    const status = searchedStatuses.value.find(s => s.npc.key === m.npc_key)
    if (!status) continue
    let groupKey: string | null = null
    if (m.match_type === 'name') {
      groupKey = m.item_name.toLowerCase()
    } else if (m.match_type.startsWith('keyword:')) {
      const kw = m.match_type.slice('keyword:'.length)
      let best: { key: string; rank: number; pref: number } | null = null
      for (const p of status.npc.preferences ?? []) {
        if (giftDesireRank(p.desire) < 0) continue
        if (!p.keywords.includes(kw) && !p.keywords.some(k => k.toLowerCase() === kw.toLowerCase())) continue
        const key = (p.name ?? p.keywords.join(', ')).toLowerCase()
        const rank = giftDesireRank(p.desire)
        if (!best || rank > best.rank || (rank === best.rank && p.pref > best.pref)) {
          best = { key, rank, pref: p.pref }
        }
      }
      groupKey = best?.key ?? null
    }
    if (!groupKey) continue
    out.set(groupKey, (out.get(groupKey) ?? 0) + Math.max(0, m.stack_size))
  }
  return out
})

// One group per distinct gift label across the searched statuses; the group's
// want is Love if any wanting NPC loves it. Sorted Love-first, then by vault
// stock descending once the planner has run (zero-count groups sink to the
// bottom of their tier but stay listed); alphabetical otherwise.
const giftGroups = computed<GiftGroup[]>(() => {
  const byKey = new Map<string, GiftGroup>()
  for (const status of searchedStatuses.value) {
    const remaining = status.maxGifts - status.giftsThisWeek
    for (const pref of status.npc.preferences ?? []) {
      if (!isGiftableDesire(pref.desire)) continue
      const label = prefLabel(pref)
      const key = label.toLowerCase()
      let group = byKey.get(key)
      if (!group) {
        group = {
          key,
          label,
          want: pref.desire.toLowerCase() === 'love' ? 'Love' : 'Like',
          entries: [],
        }
        byKey.set(key, group)
      } else if (pref.desire.toLowerCase() === 'love') {
        group.want = 'Love'
      }
      group.entries.push({
        npcKey: status.npc.key,
        npcName: status.npc.name,
        remaining,
        desire: pref.desire,
        prefName: pref.name ?? null,
        prefKeywords: [...pref.keywords],
      })
    }
  }
  const groups = [...byKey.values()]
  for (const g of groups) g.entries.sort((a, b) => a.npcName.localeCompare(b.npcName))
  const stocked = plannerLoaded.value
  groups.sort((a, b) => {
    if (a.want !== b.want) return a.want === 'Love' ? -1 : 1
    if (stocked) {
      const diff = (giftStockCounts.value.get(b.key) ?? 0) - (giftStockCounts.value.get(a.key) ?? 0)
      if (diff !== 0) return diff
    }
    return a.label.localeCompare(b.label)
  })
  // Narrow groups to the search: keep a group when the gift label matches or
  // when a wanting NPC's name matches.
  const q = searchQuery.value.trim().toLowerCase()
  if (!q) return groups
  return groups.filter(g =>
    g.label.toLowerCase().includes(q) ||
    g.entries.some(e => e.npcName.toLowerCase().includes(q)),
  )
})

// Flat consolidated gift + qty list (plain div rows — not a DataTable — so the
// grid-standards DataTable census in TODO.md stays untouched). Only actionable
// slots count: NPCs at the weekly cap contribute 0 and drop out, so an empty
// list means capped — not missing preferences (that case has its own empty
// state).
interface ShoppingRow {
  key: string
  item: string
  want: 'Love' | 'Like'
  npcs: string
  qty: number
}
const shoppingRows = computed<ShoppingRow[]>(() =>
  [...giftGroups.value]
    // Shopping List keeps the historical order (Love-first, then A–Z) even
    // once the By Gift tab re-sorts by vault stock.
    .sort((a, b) => {
      if (a.want !== b.want) return a.want === 'Love' ? -1 : 1
      return a.label.localeCompare(b.label)
    })
    .map((g) => {
      const actionable = g.entries.filter(e => e.remaining > 0)
      return {
        key: g.key,
        item: g.label,
        want: g.want,
        npcs: actionable.map(e => e.npcName).join(', '),
        qty: actionable.reduce((sum, e) => sum + e.remaining, 0),
      }
    })
    .filter(r => r.qty > 0),
)

</script>
