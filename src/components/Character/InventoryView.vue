<template>
  <div class="flex flex-col gap-4 h-full">
    <!-- Snapshot selector + import -->
    <div class="flex items-center gap-4">
      <label class="text-sm text-text-secondary">Inventory Snapshot</label>
      <select
        v-if="store.inventorySnapshots.length"
        class="bg-surface-elevated border border-border-default rounded px-3 py-1.5 text-sm text-text-primary cursor-pointer min-w-70"
        :value="store.selectedInventorySnapshot?.id"
        @change="onSnapshotChange">
        <option v-for="snap in store.inventorySnapshots" :key="snap.id" :value="snap.id">
          {{ formatTimestamp(snap.snapshot_timestamp) }} — {{ snap.item_count }} items
        </option>
      </select>
      <span v-if="store.inventorySnapshots.length > 1" class="text-xs text-text-muted">
        {{ store.inventorySnapshots.length }} snapshots
      </span>
      <button
        class="px-4 py-2 bg-accent-gold/20 border border-accent-gold/40 text-accent-gold rounded cursor-pointer text-sm font-medium transition-all hover:bg-accent-gold/30"
        :disabled="store.loading"
        @click="handleImport">
        Import Inventory
      </button>

      <button
        class="ml-auto px-3 py-1.5 text-xs cursor-pointer rounded border transition-colors whitespace-nowrap"
        :class="searchAllChars
          ? 'bg-accent-gold/20 text-accent-gold border-accent-gold/40'
          : 'bg-surface-base text-text-secondary border-border-default hover:text-text-primary hover:bg-surface-elevated'"
        :title="searchAllChars
          ? 'Showing storage across all characters on this server'
          : 'Search the storage of every character on this server'"
        @click="searchAllChars = !searchAllChars">
        {{ searchAllChars ? '✓ All Characters' : 'All Characters' }}
      </button>
    </div>

    <!-- Import feedback -->
    <div v-if="store.lastInventoryImport && !store.lastInventoryImport.was_duplicate"
      class="p-3 bg-green-900/20 border border-green-700/40 rounded text-sm text-green-300">
      Imported {{ store.lastInventoryImport.items_imported }} items for
      {{ store.lastInventoryImport.character_name }} ({{ store.lastInventoryImport.server_name }})
    </div>
    <div v-if="store.lastInventoryImport?.was_duplicate"
      class="p-3 bg-yellow-900/20 border border-yellow-700/40 rounded text-sm text-yellow-300">
      Inventory snapshot already imported (duplicate).
    </div>

    <!-- Summary bar -->
    <div v-if="store.inventorySummary" class="flex gap-6 text-sm">
      <div class="flex gap-1.5 items-baseline">
        <span class="text-text-muted">Items:</span>
        <span class="text-text-primary font-medium">{{ store.inventorySummary.total_items.toLocaleString() }}</span>
      </div>
      <div class="flex gap-1.5 items-baseline">
        <span class="text-text-muted">Stacks:</span>
        <span class="text-text-primary font-medium">{{ store.inventorySummary.total_stacks.toLocaleString() }}</span>
      </div>
      <div class="flex gap-1.5 items-baseline">
        <span class="text-text-muted">Unique:</span>
        <span class="text-text-primary font-medium">{{ store.inventorySummary.unique_items.toLocaleString() }}</span>
      </div>
      <div class="flex gap-1.5 items-baseline">
        <span class="text-text-muted">Total Value:</span>
        <span class="text-accent-gold font-medium">{{ store.inventorySummary.total_value.toLocaleString() }}</span>
      </div>
      <div class="flex gap-1.5 items-baseline">
        <span class="text-text-muted">Locations:</span>
        <span class="text-text-primary font-medium">{{ Object.keys(store.inventorySummary.items_by_vault).length }}</span>
      </div>
    </div>

    <!-- Search + View / Grouping controls -->
    <div v-if="store.selectedInventorySnapshot || searchAllChars" class="flex items-center gap-3 flex-wrap">
      <div class="relative">
        <input
          v-model="searchQuery"
          type="text"
          :placeholder="searchAllChars ? 'Search all characters…' : 'Search items...'"
          class="pl-7 pr-7 py-1 bg-surface-base border border-border-default rounded text-xs text-text-primary placeholder-text-muted w-48 focus:outline-none focus:border-accent-gold/50"
        />
        <span class="absolute left-2 top-1/2 -translate-y-1/2 text-text-muted text-xs pointer-events-none">&#x1F50D;</span>
        <button
          v-if="searchQuery"
          class="absolute right-1.5 top-1/2 -translate-y-1/2 text-text-muted hover:text-text-primary text-xs cursor-pointer"
          @click="searchQuery = ''"
        >&times;</button>
      </div>
      <span v-if="searchAllChars" class="text-xs text-text-muted">
        {{ crossCharFiltered.length }} item{{ crossCharFiltered.length !== 1 ? 's' : '' }}
        across all characters
      </span>
      <span v-else-if="searchQuery" class="text-xs text-text-muted">
        {{ filteredItems.length }} match{{ filteredItems.length !== 1 ? 'es' : '' }}
      </span>
    </div>
    <div v-if="store.selectedInventorySnapshot && !searchAllChars" class="flex items-center gap-3 flex-wrap">
      <div v-if="groupMode !== 'totals'" class="flex items-center gap-1.5">
        <span class="text-xs text-text-muted">View:</span>
        <div class="flex border border-border-default rounded overflow-hidden">
          <button
            v-for="mode in viewModes"
            :key="mode.value"
            class="px-2.5 py-1 text-xs cursor-pointer transition-colors"
            :class="viewMode === mode.value
              ? 'bg-accent-gold/20 text-accent-gold'
              : 'bg-surface-base text-text-secondary hover:text-text-primary hover:bg-surface-elevated'"
            @click="viewMode = mode.value"
          >{{ mode.label }}</button>
        </div>
      </div>

      <div class="flex items-center gap-1.5">
        <span class="text-xs text-text-muted">Group:</span>
        <select
          v-model="groupMode"
          class="px-2 py-1 bg-surface-base border border-border-default rounded text-xs text-text-primary cursor-pointer">
          <option value="none">No Grouping</option>
          <option value="storage">By Storage Location</option>
          <option value="zone">By Zone</option>
          <option value="totals">Show Totals</option>
        </select>
      </div>

      <div v-if="viewMode !== 'detail' && groupMode !== 'totals'" class="flex items-center gap-1.5">
        <span class="text-xs text-text-muted">Sort:</span>
        <select
          v-model="sortMode"
          class="px-2 py-1 bg-surface-base border border-border-default rounded text-xs text-text-primary cursor-pointer">
          <option value="slot">Slot Number</option>
          <option value="alpha">Alphabetical</option>
          <option value="count">Count</option>
          <option value="value">Value</option>
        </select>
      </div>

      <template v-if="groupMode !== 'none'">
        <button
          class="px-2 py-1 text-xs text-text-secondary hover:text-text-primary cursor-pointer"
          @click="expandAll"
        >Expand All</button>
        <button
          class="px-2 py-1 text-xs text-text-secondary hover:text-text-primary cursor-pointer"
          @click="collapseAll"
        >Collapse All</button>
      </template>
    </div>

    <!-- No data -->
    <EmptyState
      v-if="!searchAllChars && !store.inventorySnapshots.length && !store.loading"
      variant="panel"
      primary="No inventory data found"
      secondary="Use /outputitems in-game, then import the report." />

    <!-- All-characters storage search -->
    <div v-if="searchAllChars" class="overflow-auto flex-1 min-h-0">
      <div v-if="store.allCharacterItemsLoading && !store.allCharacterItems.length"
        class="text-xs text-text-muted py-6 text-center">
        Loading storage for all characters…
      </div>
      <EmptyState
        v-else-if="!store.allCharacterItems.length"
        variant="panel"
        primary="No inventory data for any character"
        secondary="Use /outputitems in-game on each character, then import their reports." />
      <EmptyState
        v-else-if="crossCharFiltered.length === 0"
        variant="compact"
        :primary="`No items matching &quot;${searchQuery}&quot;`" />
      <DataTable
        v-else
        :columns="crossCharColumns"
        :rows="(crossCharDisplayRows as unknown as Record<string, unknown>[])"
        :hoverable="false"
        :row-class="crossCharRowClass"
        @row-click="onCrossCharRowClick">
        <template #cell-item="{ row }">
          <ItemInline v-if="row.kind === 'item'" :reference="row.name as string" :type-id="row.typeId as number" />
          <span v-else class="pl-8 text-text-secondary">{{ row.charName }}</span>
        </template>
        <template #cell-total="{ row }">
          <span v-if="row.kind === 'item'" class="font-medium text-text-primary">{{ (row.qty as number).toLocaleString() }}</span>
          <span v-else class="text-text-secondary">{{ (row.qty as number).toLocaleString() }}</span>
        </template>
        <template #cell-characters="{ row }">
          <span v-if="row.kind === 'item'" class="text-text-muted">
            {{ row.characterCount }} {{ (row.characterCount as number) === 1 ? 'character' : 'characters' }}
          </span>
          <span v-else class="text-text-muted">{{ formatVault(row.location as string) }}</span>
        </template>
      </DataTable>
    </div>

    <!-- Content -->
    <div v-else-if="store.selectedInventorySnapshot" class="overflow-auto flex-1 min-h-0">
      <!-- No search results -->
      <EmptyState v-if="searchQuery && filteredItems.length === 0" variant="compact" :primary="`No items matching &quot;${searchQuery}&quot;`" />

      <!-- Ungrouped -->
      <template v-else-if="groupMode === 'none'">
        <component :is="viewComponent" :items="sortItems(filteredItems)" />
      </template>

      <!-- Totals mode: consolidated by item name -->
      <template v-else-if="groupMode === 'totals'">
        <DataTable
          :columns="totalColumns"
          :rows="(totalDisplayRows as unknown as Record<string, unknown>[])"
          :sort-key="totalSort"
          :sort-dir="totalSortAsc ? 'asc' : 'desc'"
          :hoverable="false"
          :row-class="totalRowClass"
          @sort="handleTotalSort"
          @row-click="onTotalRowClick">
          <template #cell-name="{ row }">
            <ItemInline v-if="row.kind === 'item'" :reference="row.name as string" :type-id="row.typeId as number" />
            <span v-else class="pl-8 text-text-muted">{{ row.locationLabel }}</span>
          </template>
          <template #cell-qty="{ row }">
            <span v-if="row.kind === 'item'" class="font-medium text-text-primary">{{ (row.qty as number).toLocaleString() }}</span>
            <span v-else class="text-text-secondary">{{ (row.qty as number).toLocaleString() }}</span>
          </template>
          <template #cell-locations="{ row }">
            <span v-if="row.kind === 'item'" class="text-text-muted">
              {{ row.locationCount }} {{ (row.locationCount as number) === 1 ? 'location' : 'locations' }}
            </span>
          </template>
        </DataTable>
      </template>

      <!-- Grouped by location/zone -->
      <template v-else>
        <div v-for="group in groupedItems" :key="group.label" class="mb-2">
          <button
            class="flex items-center gap-2 w-full pb-1 mb-2 border-b border-border-default/50 cursor-pointer hover:border-border-default transition-colors text-left"
            @click="toggleGroup(group.label)"
          >
            <span class="text-text-muted text-xs w-4">{{ collapsedGroups.has(group.label) ? '\u25B6' : '\u25BC' }}</span>
            <h3 class="text-sm font-medium text-text-primary">{{ group.label }}</h3>
            <span class="text-xs text-text-muted">({{ group.items.length }} stacks)</span>
          </button>
          <component v-if="!collapsedGroups.has(group.label)" :is="viewComponent" :items="sortItems(group.items)" />
        </div>
      </template>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, reactive, computed, onMounted, watch, type Component } from 'vue'
import { useCharacterStore } from '../../stores/characterStore'
import { useGameDataStore } from '../../stores/gameDataStore'
import { useGameStateStore } from '../../stores/gameStateStore'
import { useViewPrefs } from '../../composables/useViewPrefs'
import { formatDateTimeFull } from '../../composables/useTimestamp'
import type { SnapshotItem } from '../../types/database'
import EmptyState from '../Shared/EmptyState.vue'
import DataTable, { type ColumnDef } from '../Shared/DataTable.vue'
import ItemInline from '../Shared/Item/ItemInline.vue'
import InventoryTable from './InventoryTable.vue'
import InventorySmallGrid from './InventorySmallGrid.vue'
import InventoryLargeGrid from './InventoryLargeGrid.vue'
import InventoryItemPanel from './InventoryItemPanel.vue'

const store = useCharacterStore()
const gameStore = useGameDataStore()
const gameState = useGameStateStore()

type ViewMode = 'small-grid' | 'large-grid' | 'panel' | 'detail'
type GroupMode = 'none' | 'storage' | 'zone' | 'totals'
type SortMode = 'slot' | 'alpha' | 'count' | 'value'

// Persist view settings across navigation
const { prefs, update: updatePrefs } = useViewPrefs<Record<string, unknown>>('inventoryView', {
  viewMode: 'detail' as string,
  groupMode: 'storage' as string,
  sortMode: 'slot' as string,
  searchQuery: '' as string,
  searchAllChars: false as boolean,
})

const viewMode = ref<ViewMode>((prefs.value.viewMode as ViewMode) || 'detail')
const groupMode = ref<GroupMode>((prefs.value.groupMode as GroupMode) || 'storage')
const sortMode = ref<SortMode>((prefs.value.sortMode as SortMode) || 'slot')
const searchQuery = ref((prefs.value.searchQuery as string) || '')
const searchAllChars = ref((prefs.value.searchAllChars as boolean) || false)

// Persist changes back
watch([viewMode, groupMode, sortMode, searchQuery, searchAllChars], () => {
  updatePrefs({
    viewMode: viewMode.value,
    groupMode: groupMode.value,
    sortMode: sortMode.value,
    searchQuery: searchQuery.value,
    searchAllChars: searchAllChars.value,
  })
})

// ── Cross-character storage search ────────────────────────────────────────────
// When enabled, search spans the latest inventory snapshot of every character
// on the active server so the user can find which character holds an item.

// Load (and lazily backfill) the aggregated dataset the first time the toggle
// is turned on, or immediately if it was persisted as enabled.
watch(searchAllChars, (on) => {
  if (on) store.loadAllCharacterItems()
}, { immediate: true })

interface CrossCharEntry {
  character: string
  location: string
  qty: number
}

interface CrossCharRow {
  name: string
  typeId: number
  total: number
  characterCount: number
  entries: CrossCharEntry[]
}

const crossCharFiltered = computed<CrossCharRow[]>(() => {
  const q = searchQuery.value.trim().toLowerCase()
  const map = new Map<string, { typeId: number; total: number; entries: CrossCharEntry[]; chars: Set<string> }>()

  for (const item of store.allCharacterItems) {
    if (q && !item.item_name.toLowerCase().includes(q)) continue
    const location = item.is_in_inventory ? 'Inventory' : (item.storage_vault || 'Unknown')
    let entry = map.get(item.item_name)
    if (!entry) { entry = { typeId: item.type_id, total: 0, entries: [], chars: new Set() }; map.set(item.item_name, entry) }
    entry.total += item.stack_size
    entry.entries.push({ character: item.character_name, location, qty: item.stack_size })
    entry.chars.add(item.character_name)
  }

  return [...map.entries()]
    .map(([name, data]) => ({
      name,
      typeId: data.typeId,
      total: data.total,
      characterCount: data.chars.size,
      entries: data.entries.sort(
        (a, b) => a.character.localeCompare(b.character) || a.location.localeCompare(b.location),
      ),
    }))
    .sort((a, b) => a.name.localeCompare(b.name))
})

const expandedCrossChar = reactive(new Set<string>())
function toggleCrossCharRow(name: string) {
  if (expandedCrossChar.has(name)) expandedCrossChar.delete(name)
  else expandedCrossChar.add(name)
}

// Flattened row model for the shared DataTable: one row per item plus one row
// per expanded character/location entry.
interface CrossCharDisplayRow {
  kind: 'item' | 'entry'
  name: string
  typeId: number
  qty: number
  characterCount: number
  charName: string
  location: string
}

const crossCharColumns: ColumnDef[] = [
  { key: 'item', label: 'Item' },
  { key: 'total', label: 'Total Qty', numeric: true },
  { key: 'characters', label: 'Characters', align: 'right' },
]

const crossCharDisplayRows = computed<CrossCharDisplayRow[]>(() => {
  const out: CrossCharDisplayRow[] = []
  for (const row of crossCharFiltered.value) {
    out.push({
      kind: 'item',
      name: row.name,
      typeId: row.typeId,
      qty: row.total,
      characterCount: row.characterCount,
      charName: '',
      location: '',
    })
    if (!expandedCrossChar.has(row.name)) continue
    for (const entry of row.entries) {
      out.push({
        kind: 'entry',
        name: row.name,
        typeId: row.typeId,
        qty: entry.qty,
        characterCount: 0,
        charName: entry.character,
        location: entry.location,
      })
    }
  }
  return out
})

// Only item rows are interactive/hoverable, matching the original markup;
// hover lives in rowClass because `hoverable` applies table-wide.
function crossCharRowClass(row: Record<string, unknown>): string {
  return row.kind === 'item' ? 'hover:bg-surface-row-hover cursor-pointer' : 'bg-surface-inset/30'
}

function onCrossCharRowClick(payload: { row: Record<string, unknown> }) {
  if (payload.row.kind === 'item') toggleCrossCharRow(payload.row.name as string)
}

// While searching all characters, auto-expand matches so the owning character
// and location are visible without an extra click. Collapse again when cleared.
watch([searchQuery, searchAllChars], ([q, on]) => {
  if (!on) return
  expandedCrossChar.clear()
  if (q.trim()) {
    for (const row of crossCharFiltered.value) expandedCrossChar.add(row.name)
  }
})

const viewModes = [
  { value: 'small-grid' as ViewMode, label: 'Small Icons' },
  { value: 'large-grid' as ViewMode, label: 'Large Icons' },
  { value: 'panel' as ViewMode, label: 'Item Panel' },
  { value: 'detail' as ViewMode, label: 'Detailed List' },
]

const viewComponent = computed<Component>(() => {
  switch (viewMode.value) {
    case 'small-grid': return InventorySmallGrid
    case 'large-grid': return InventoryLargeGrid
    case 'panel': return InventoryItemPanel
    case 'detail': return InventoryTable
  }
})

// ── Collapse state ──────────────────────────────────────────────────────────

const collapsedGroups = reactive(new Set<string>())

// Keys that should default to expanded
const ALWAYS_EXPANDED = new Set(['Inventory', 'Saddlebag', 'Inventory + Saddlebag'])

function initCollapseState(groups: ItemGroup[]) {
  collapsedGroups.clear()
  for (const group of groups) {
    if (!ALWAYS_EXPANDED.has(group.label)) {
      collapsedGroups.add(group.label)
    }
  }
}

function toggleGroup(label: string) {
  if (collapsedGroups.has(label)) {
    collapsedGroups.delete(label)
  } else {
    collapsedGroups.add(label)
  }
}

function expandAll() {
  collapsedGroups.clear()
}

function collapseAll() {
  for (const group of groupedItems.value) {
    collapsedGroups.add(group.label)
  }
}

// ── Vault name formatting ───────────────────────────────────────────────────

function formatVault(vault: string): string {
  if (vault === 'Inventory') return 'Inventory'
  if (vault === 'Unknown') return 'Unknown'
  // Look up CDN vault metadata for the proper display name
  const vaultInfo = gameState.storageVaultsByKey[vault]
  if (vaultInfo?.npc_friendly_name) return vaultInfo.npc_friendly_name
  // Fallback for vaults not in CDN data
  if (vault.startsWith('*AccountStorage_')) {
    return `Account Storage (${vault.replace('*AccountStorage_', '')})`
  }
  return vault
}

function getLocation(item: SnapshotItem): string {
  if (item.is_in_inventory) return 'Inventory'
  if (!item.storage_vault) return 'Unknown'
  return item.storage_vault
}

// ── Zone data ───────────────────────────────────────────────────────────────

interface VaultZoneEntry {
  areaKey: string | null
  areaName: string | null
}

const vaultZoneMap = ref<Record<string, VaultZoneEntry>>({})

async function loadVaultZones() {
  try {
    const zones = await gameStore.getStorageVaultZones()
    const map: Record<string, VaultZoneEntry> = {}
    for (const z of zones) {
      map[z.vault_key] = { areaKey: z.area_key, areaName: z.area_name }
    }
    vaultZoneMap.value = map
  } catch (e) {
    console.warn('Failed to load vault zones:', e)
  }
}

// Load vault zones when grouping by zone
watch(groupMode, (mode) => {
  if (mode === 'zone' && Object.keys(vaultZoneMap.value).length === 0) {
    loadVaultZones()
  }
}, { immediate: true })

// ── Search ──────────────────────────────────────────────────────────────────

const filteredItems = computed<SnapshotItem[]>(() => {
  const q = searchQuery.value.trim().toLowerCase()
  if (!q) return store.inventoryItems
  return store.inventoryItems.filter(item => item.item_name.toLowerCase().includes(q))
})

// Expand all groups while searching so results aren't hidden
watch(searchQuery, (q) => {
  if (q.trim()) expandAll()
})

// ── Sorting ─────────────────────────────────────────────────────────────────

function sortItems(items: SnapshotItem[]): SnapshotItem[] {
  if (sortMode.value === 'slot') return items
  const sorted = [...items]
  switch (sortMode.value) {
    case 'alpha':
      sorted.sort((a, b) => a.item_name.localeCompare(b.item_name))
      break
    case 'count':
      sorted.sort((a, b) => b.stack_size - a.stack_size)
      break
    case 'value':
      sorted.sort((a, b) => (b.value ?? 0) - (a.value ?? 0))
      break
  }
  return sorted
}

// ── Grouping logic ──────────────────────────────────────────────────────────

interface ItemGroup {
  label: string
  sortKey: string
  items: SnapshotItem[]
}

function getZoneForVault(vaultKey: string): string {
  const zone = vaultZoneMap.value[vaultKey]
  if (zone?.areaName) return zone.areaName

  // Account storage keys often encode the zone name
  if (vaultKey.startsWith('*AccountStorage_')) {
    const loc = vaultKey.replace('*AccountStorage_', '')
    const cdnZone = vaultZoneMap.value[`*AccountStorage_${loc}`]
    if (cdnZone?.areaName) return cdnZone.areaName
    return loc
  }

  return 'Unknown Zone'
}

const INVENTORY_VAULTS = new Set(['Inventory', 'Saddlebag'])

const groupedItems = computed<ItemGroup[]>(() => {
  const items = filteredItems.value
  if (groupMode.value === 'storage') {
    const groups = new Map<string, SnapshotItem[]>()
    for (const item of items) {
      const loc = getLocation(item)
      if (!groups.has(loc)) groups.set(loc, [])
      groups.get(loc)!.push(item)
    }
    return [...groups.entries()]
      .map(([key, groupItems]) => ({
        label: formatVault(key),
        sortKey: key === 'Inventory' ? '!0' : key === 'Saddlebag' ? '!1' : key,
        items: groupItems,
      }))
      .sort((a, b) => a.sortKey.localeCompare(b.sortKey))
  }

  if (groupMode.value === 'zone') {
    const groups = new Map<string, SnapshotItem[]>()
    for (const item of items) {
      const loc = getLocation(item)
      // Merge Inventory + Saddlebag into one group
      const zone = INVENTORY_VAULTS.has(loc) ? 'Inventory + Saddlebag' : getZoneForVault(loc)
      if (!groups.has(zone)) groups.set(zone, [])
      groups.get(zone)!.push(item)
    }
    return [...groups.entries()]
      .map(([zone, groupItems]) => ({
        label: zone,
        sortKey: zone === 'Inventory + Saddlebag' ? '!0' : zone,
        items: groupItems,
      }))
      .sort((a, b) => a.sortKey.localeCompare(b.sortKey))
  }

  return []
})

// Re-init collapse state when groups change (snapshot or group mode change)
watch(groupedItems, (groups) => {
  if (groups.length > 0) initCollapseState(groups)
})

// ── Totals mode ─────────────────────────────────────────────────────────────

interface TotalRow {
  name: string
  typeId: number
  total: number
  locations: { vault: string; qty: number }[]
}

const expandedTotals = reactive(new Set<string>())
const totalSort = ref<'name' | 'qty'>('name')
const totalSortAsc = ref(true)

function toggleTotalRow(name: string) {
  if (expandedTotals.has(name)) expandedTotals.delete(name)
  else expandedTotals.add(name)
}

function toggleTotalSort(col: 'name' | 'qty') {
  if (totalSort.value === col) totalSortAsc.value = !totalSortAsc.value
  else { totalSort.value = col; totalSortAsc.value = col === 'name' }
}

const itemTotals = computed<TotalRow[]>(() => {
  const map = new Map<string, { typeId: number; total: number; locs: Map<string, number> }>()
  for (const item of filteredItems.value) {
    const vault = getLocation(item)
    let entry = map.get(item.item_name)
    if (!entry) { entry = { typeId: item.type_id, total: 0, locs: new Map() }; map.set(item.item_name, entry) }
    entry.total += item.stack_size
    entry.locs.set(vault, (entry.locs.get(vault) || 0) + item.stack_size)
  }
  return [...map.entries()].map(([name, data]) => ({
    name,
    typeId: data.typeId,
    total: data.total,
    locations: [...data.locs.entries()].map(([vault, qty]) => ({ vault, qty })).sort((a, b) => a.vault.localeCompare(b.vault)),
  }))
})

const sortedTotals = computed(() => {
  const rows = [...itemTotals.value]
  const dir = totalSortAsc.value ? 1 : -1
  if (totalSort.value === 'name') rows.sort((a, b) => dir * a.name.localeCompare(b.name))
  else rows.sort((a, b) => dir * (a.total - b.total))
  return rows
})

// Flattened row model for the shared DataTable: one row per item plus one row
// per expanded storage location. Column keys double as sort keys.
interface TotalDisplayRow {
  kind: 'item' | 'location'
  name: string
  typeId: number
  qty: number
  locationCount: number
  locationLabel: string
}

const totalColumns: ColumnDef[] = [
  { key: 'name', label: 'Item', sortable: true },
  { key: 'qty', label: 'Total Qty', sortable: true, numeric: true },
  { key: 'locations', label: 'Locations', align: 'right' },
]

const totalDisplayRows = computed<TotalDisplayRow[]>(() => {
  const out: TotalDisplayRow[] = []
  for (const row of sortedTotals.value) {
    out.push({
      kind: 'item',
      name: row.name,
      typeId: row.typeId,
      qty: row.total,
      locationCount: row.locations.length,
      locationLabel: '',
    })
    if (!expandedTotals.has(row.name)) continue
    for (const loc of row.locations) {
      out.push({
        kind: 'location',
        name: row.name,
        typeId: row.typeId,
        qty: loc.qty,
        locationCount: 0,
        locationLabel: formatVault(loc.vault),
      })
    }
  }
  return out
})

// Only item rows are interactive/hoverable, matching the original markup;
// hover lives in rowClass because `hoverable` applies table-wide.
function totalRowClass(row: Record<string, unknown>): string {
  return row.kind === 'item' ? 'hover:bg-surface-row-hover cursor-pointer' : 'bg-surface-inset/30'
}

function onTotalRowClick(payload: { row: Record<string, unknown> }) {
  if (payload.row.kind === 'item') toggleTotalRow(payload.row.name as string)
}

// DataTable only reports the clicked column; the comparator and per-column
// default directions stay here.
function handleTotalSort(payload: { key: string; dir: 'asc' | 'desc' }) {
  toggleTotalSort(payload.key as 'name' | 'qty')
}

// ── Snapshot management ─────────────────────────────────────────────────────

function onSnapshotChange(event: Event) {
  const id = Number((event.target as HTMLSelectElement).value)
  const snap = store.inventorySnapshots.find(s => s.id === id)
  if (snap) {
    store.selectInventorySnapshot(snap)
  }
}

async function handleImport() {
  await store.importInventoryReport()
}

function formatTimestamp(ts: string): string {
  return formatDateTimeFull(ts)
}

onMounted(() => {
  if (!store.selectedInventorySnapshot && store.inventorySnapshots.length === 0) {
    store.initInventoryForActiveCharacter()
  }
  gameState.loadStorageVaults()
})
</script>
