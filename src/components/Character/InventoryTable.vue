<template>
  <div class="flex flex-col gap-2 h-full">
    <!-- Filters -->
    <div class="flex items-center gap-3 flex-wrap">
      <input
        v-model="filterText"
        type="text"
        placeholder="Filter items..."
        class="px-3 py-1.5 bg-surface-base border border-border-default rounded text-sm text-text-primary placeholder-text-muted focus:outline-none focus:border-accent-gold/50 w-48" />

      <select
        v-model="filterVault"
        class="px-3 py-1.5 bg-surface-base border border-border-default rounded text-sm text-text-primary cursor-pointer">
        <option value="">All Locations</option>
        <option v-for="v in vaultOptions" :key="v" :value="v">{{ formatVault(v) }}</option>
      </select>

      <select
        v-model="filterRarity"
        class="px-3 py-1.5 bg-surface-base border border-border-default rounded text-sm text-text-primary cursor-pointer">
        <option value="">All Rarities</option>
        <option v-for="r in rarityOptions" :key="r" :value="r">{{ r }}</option>
      </select>

      <select
        v-model="filterSlot"
        class="px-3 py-1.5 bg-surface-base border border-border-default rounded text-sm text-text-primary cursor-pointer">
        <option value="">All Slots</option>
        <option v-for="s in slotOptions" :key="s" :value="s">{{ s }}</option>
      </select>

      <span class="text-xs text-text-muted">{{ filtered.length }} items</span>
    </div>

    <!-- Table -->
    <div class="overflow-auto flex-1 min-h-0">
      <DataTable
        :columns="columns"
        :rows="(filtered as unknown as Record<string, unknown>[])"
        :sort-key="sortKey"
        :sort-dir="sortAsc ? 'asc' : 'desc'"
        compact
        size="sm"
        empty-text="No items match the current filters."
        @sort="handleSort">
        <template #cell-item_name="{ row }">
          <div class="flex flex-col">
            <ItemInline :reference="String(row.type_id)" />
            <span v-if="row.crafter" class="text-xs text-text-muted">
              Crafted by {{ row.crafter }}
            </span>
          </div>
        </template>
        <template #cell-stack_size="{ value }">
          <span class="text-text-primary">{{ value }}</span>
        </template>
        <template #cell-location="{ row }">
          <span class="text-text-secondary text-xs">{{ formatVault(getLocation(row as unknown as SnapshotItem)) }}</span>
        </template>
        <template #cell-rarity="{ row }">
          <span v-if="row.rarity" :class="rarityClass(row.rarity as string)" class="text-xs font-medium">
            {{ row.rarity }}
          </span>
        </template>
        <template #cell-slot="{ value }">
          <span class="text-text-secondary text-xs">{{ value ?? '' }}</span>
        </template>
        <template #cell-level="{ value }">
          <span class="text-text-secondary">{{ value ?? '' }}</span>
        </template>
        <template #cell-value="{ row }">
          <span v-if="row.value" class="text-accent-gold">
            {{ formatNumber((row.value as number) * (row.stack_size as number)) }}
          </span>
        </template>
        <template #cell-mods="{ row }">
          <ItemPowerList :json="row.tsys_powers as string | null" />
          <span v-if="row.tsys_imbue_power" class="text-xs text-blue-300">
            Imbue: {{ row.tsys_imbue_power }} (T{{ row.tsys_imbue_power_tier }})
          </span>
        </template>
      </DataTable>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed } from 'vue'
import type { SnapshotItem } from '../../types/database'
import DataTable, { type ColumnDef } from '../Shared/DataTable.vue'
import ItemInline from '../Shared/Item/ItemInline.vue'
import ItemPowerList from './ItemPowerList.vue'

const props = defineProps<{
  items: SnapshotItem[]
}>()

const columns: ColumnDef[] = [
  { key: 'item_name', label: 'Item', sortable: true },
  { key: 'stack_size', label: 'Qty', sortable: true, align: 'right' },
  { key: 'location', label: 'Location', sortable: true },
  { key: 'rarity', label: 'Rarity', sortable: true },
  { key: 'slot', label: 'Slot', sortable: true },
  { key: 'level', label: 'Lvl', sortable: true, align: 'right' },
  { key: 'value', label: 'Value', sortable: true, align: 'right' },
  { key: 'mods', label: 'Mods' },
]

const filterText = ref('')
const filterVault = ref('')
const filterRarity = ref('')
const filterSlot = ref('')

type SortKey = 'item_name' | 'stack_size' | 'location' | 'rarity' | 'slot' | 'level' | 'value'
const sortKey = ref<SortKey>('item_name')
const sortAsc = ref(true)

function getLocation(item: SnapshotItem): string {
  if (item.is_in_inventory) return 'Inventory'
  if (!item.storage_vault) return 'Unknown'
  return item.storage_vault
}

const vaultOptions = computed(() => {
  const vaults = new Set<string>()
  for (const item of props.items) {
    vaults.add(getLocation(item))
  }
  return [...vaults].sort()
})

const rarityOptions = computed(() => {
  const rarities = new Set<string>()
  for (const item of props.items) {
    if (item.rarity) rarities.add(item.rarity)
  }
  return ['Common', 'Uncommon', 'Rare', 'Exceptional', 'Epic', 'Legendary'].filter(r => rarities.has(r) || r === 'Common')
})

const slotOptions = computed(() => {
  const slots = new Set<string>()
  for (const item of props.items) {
    if (item.slot) slots.add(item.slot)
  }
  return [...slots].sort()
})

const rarityOrder: Record<string, number> = {
  Common: 0, Uncommon: 1, Rare: 2, Exceptional: 3, Epic: 4, Legendary: 5,
}

const filtered = computed(() => {
  const text = filterText.value.toLowerCase()
  let list = props.items.filter(item => {
    if (text && !item.item_name.toLowerCase().includes(text)) return false
    if (filterVault.value && getLocation(item) !== filterVault.value) return false
    if (filterRarity.value) {
      const itemRarity = item.rarity ?? 'Common'
      if (itemRarity !== filterRarity.value) return false
    }
    if (filterSlot.value && item.slot !== filterSlot.value) return false
    return true
  })

  list.sort((a, b) => {
    const dir = sortAsc.value ? 1 : -1
    switch (sortKey.value) {
      case 'item_name':
        return a.item_name.localeCompare(b.item_name) * dir
      case 'stack_size':
        return (a.stack_size - b.stack_size) * dir
      case 'location':
        return getLocation(a).localeCompare(getLocation(b)) * dir
      case 'rarity':
        return ((rarityOrder[a.rarity ?? 'Common'] ?? 0) - (rarityOrder[b.rarity ?? 'Common'] ?? 0)) * dir
      case 'slot':
        return (a.slot ?? '').localeCompare(b.slot ?? '') * dir
      case 'level':
        return ((a.level ?? 0) - (b.level ?? 0)) * dir
      case 'value':
        return (((a.value ?? 0) * a.stack_size) - ((b.value ?? 0) * b.stack_size)) * dir
      default:
        return 0
    }
  })

  return list
})

function toggleSort(key: SortKey) {
  if (sortKey.value === key) {
    sortAsc.value = !sortAsc.value
  } else {
    sortKey.value = key
    sortAsc.value = key === 'item_name' || key === 'location' || key === 'rarity' || key === 'slot'
  }
}

// Keeps the file's own comparator/default-direction semantics; DataTable only
// reports the clicked column and renders the arrow from sortKey/sortAsc.
function handleSort(payload: { key: string; dir: 'asc' | 'desc' }) {
  toggleSort(payload.key as SortKey)
}

function formatNumber(n: number): string {
  return n.toLocaleString()
}

const VAULT_NAMES: Record<string, string> = {
  Inventory: 'Inventory',
  CouncilVault: 'Council Vault',
  TapestryInnChest: 'Tapestry Inn Chest',
  SerbuleCommunityChest: 'Serbule Community Chest',
  DreamRealmChest: 'Dream Realm Chest',
  PovusStorageChest: 'Povus Storage Chest',
  Saddlebag: 'Saddlebag',
  Unknown: 'Unknown',
}

function formatVault(vault: string): string {
  if (VAULT_NAMES[vault]) return VAULT_NAMES[vault]
  if (vault.startsWith('*AccountStorage_')) {
    const location = vault.replace('*AccountStorage_', '')
    return `Account Storage (${location})`
  }
  if (vault.startsWith('NPC_')) {
    const npcName = vault.replace('NPC_', '')
    return `${npcName}'s Storage`
  }
  return vault
}

function rarityClass(rarity: string): string {
  switch (rarity) {
    case 'Uncommon': return 'text-rarity-uncommon'
    case 'Rare': return 'text-rarity-rare'
    case 'Exceptional': return 'text-rarity-exceptional'
    case 'Epic': return 'text-rarity-epic'
    case 'Legendary': return 'text-rarity-legendary'
    default: return 'text-text-secondary'
  }
}
</script>
