<template>
  <div class="flex flex-col gap-2">
    <!-- Current moon phase -->
    <div v-if="phase" class="flex items-center gap-2 text-sm">
      <span class="text-lg">{{ phase.emoji }}</span>
      <span class="text-text-secondary">{{ phase.label }}</span>
      <span v-if="nextPhase" class="text-text-dim text-xs ml-auto">
        {{ nextPhase.label }} in {{ nextPhase.days }}d
      </span>
    </div>

    <!-- Filter controls -->
    <div class="flex items-center gap-2">
      <div class="flex rounded border border-border overflow-hidden text-xs">
        <button
          v-for="opt in filterOptions"
          :key="opt.value"
          class="px-2.5 py-1 transition-colors"
          :class="
            filterMode === opt.value
              ? 'bg-accent-blue text-white'
              : 'bg-surface-2 text-text-secondary hover:bg-surface-3'
          "
          @click="filterMode = opt.value">
          {{ opt.label }}
        </button>
      </div>
      <input
        v-model="search"
        type="text"
        placeholder="Search..."
        class="flex-1 px-2 py-1 rounded bg-surface-2 border border-border text-sm text-text-primary placeholder:text-text-dim focus:outline-none focus:border-accent-blue" />
    </div>

    <!-- Mushroom table -->
    <div class="overflow-y-auto max-h-80">
      <DataTable
        :columns="columns"
        :rows="(filteredMushrooms as unknown as Record<string, unknown>[])"
        :row-key="(row) => String(row.name)"
        :sort-key="sortKey"
        :sort-dir="sortAsc ? 'asc' : 'desc'"
        compact
        size="sm"
        empty-variant="compact"
        empty-text="No mushrooms match your filter."
        :row-class="rowClass"
        @sort="onSort">
        <template #cell-name="{ row }">
          <ItemInline :reference="(row.name as string)" />
        </template>
        <template #cell-mycology_req="{ row }">
          <span class="text-text-dim font-mono">{{ (row as unknown as MushroomRow).mycology_req ?? 'N/A' }}</span>
        </template>
        <template #cell-grow_hours="{ row }">
          <span class="text-text-dim font-mono">{{ (row as unknown as MushroomRow).grow_hours }}h</span>
        </template>
        <template #cell-substrate="{ row }">
          <span class="text-accent-gold">{{ (row as unknown as MushroomRow).very_well_substrate }}</span>
          <span class="text-text-dim"> / {{ (row as unknown as MushroomRow).adequate_substrate ?? '??' }}</span>
        </template>
        <template #cell-moon="{ row }">
          <span
            v-for="p in ((row as unknown as MushroomRow).robust_phases as MoonPhaseName[])"
            :key="p"
            class="mr-1"
            :class="phase?.name === p ? 'text-value-positive' : 'text-text-dim'"
            :title="phaseLabel(p) + ' (robust)'">
            {{ phaseEmoji(p) }}
          </span>
          <span class="text-text-dim mx-0.5">|</span>
          <span
            v-for="p in ((row as unknown as MushroomRow).poor_phases as MoonPhaseName[])"
            :key="p"
            class="mr-1"
            :class="phase?.name === p ? 'text-value-negative' : 'text-text-dim'"
            :title="phaseLabel(p) + ' (poor)'">
            {{ phaseEmoji(p) }}
          </span>
        </template>
      </DataTable>
    </div>

    <div class="text-xs text-text-dim flex items-center gap-1">
      <span class="text-value-positive">&#9679;</span> extra yield
      <span class="text-value-negative ml-2">&#9679;</span> reduced yield
      <span class="ml-2">Substrate: <span class="text-accent-gold">very well</span> / adequate</span>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, onMounted } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { useMoonPhase, ALL_PHASES, type MoonPhaseName } from '../../../composables/useMoonPhase'
import type { MushroomFarmingInfo as MushroomRow } from '../../../types/gameData/mushroomFarming'
import DataTable from '../../Shared/DataTable.vue'
import ItemInline from '../../Shared/Item/ItemInline.vue'

const { phase, daysUntil } = useMoonPhase()

const search = ref('')
const filterMode = ref<'all' | 'robust' | 'poor'>('robust')
const sortKey = ref<'name' | 'mycology_req' | 'grow_hours'>('mycology_req')

const filterOptions = [
  { value: 'all' as const, label: 'All' },
  { value: 'robust' as const, label: 'Extra Yield' },
  { value: 'poor' as const, label: 'Reduced Yield' },
]
const sortAsc = ref(true)

const columns = [
  { key: 'name', label: 'Mushroom', sortable: true },
  { key: 'mycology_req', label: 'Myco', sortable: true, align: 'center' as const },
  { key: 'grow_hours', label: 'Time', sortable: true, align: 'center' as const },
  { key: 'substrate', label: 'Substrate', sortable: false },
  { key: 'moon', label: 'Moon', sortable: false },
]

type MushroomInfo = MushroomRow

const mushrooms = ref<MushroomInfo[]>([])

onMounted(async () => {
  try {
    mushrooms.value = await invoke<MushroomInfo[]>('get_mushroom_farming')
  } catch (e) {
    console.error('[MushroomFarmingWidget] Failed to load mushroom data:', e)
  }
})
const PHASE_EMOJI: Record<MoonPhaseName, string> = Object.fromEntries(
  ALL_PHASES.map(p => [p.name, p.emoji])
) as Record<MoonPhaseName, string>

const PHASE_LABELS: Record<MoonPhaseName, string> = Object.fromEntries(
  ALL_PHASES.map(p => [p.name, p.label])
) as Record<MoonPhaseName, string>

function phaseEmoji(name: MoonPhaseName): string {
  return PHASE_EMOJI[name] ?? '?'
}

function phaseLabel(name: MoonPhaseName): string {
  return PHASE_LABELS[name] ?? name
}

const nextPhase = computed(() => daysUntil.value.length > 0 ? daysUntil.value[0] : null)

function isRobustNow(m: MushroomInfo): boolean {
  return !!phase.value && m.robust_phases.includes(phase.value.name)
}

function isPoorNow(m: MushroomInfo): boolean {
  return !!phase.value && m.poor_phases.includes(phase.value.name)
}

function rowClass(row: Record<string, unknown>): string {
  const m = row as unknown as MushroomInfo
  if (isRobustNow(m)) return 'bg-green-500/5'
  if (isPoorNow(m)) return 'bg-red-500/5'
  return ''
}

function onSort(payload: { key: string; dir: 'asc' | 'desc' }) {
  sortKey.value = payload.key as 'name' | 'mycology_req' | 'grow_hours'
  sortAsc.value = payload.dir === 'asc'
}

const filteredMushrooms = computed(() => {
  const q = search.value.toLowerCase().trim()
  let result = mushrooms.value.filter(m => {
    if (q && !m.name.toLowerCase().includes(q)) return false
    if (filterMode.value === 'robust' && !isRobustNow(m)) return false
    if (filterMode.value === 'poor' && !isPoorNow(m)) return false
    return true
  })

  result.sort((a, b) => {
    let cmp = 0
    if (sortKey.value === 'name') cmp = a.name.localeCompare(b.name)
    else if (sortKey.value === 'mycology_req') cmp = (a.mycology_req ?? -1) - (b.mycology_req ?? -1)
    else if (sortKey.value === 'grow_hours') cmp = a.grow_hours - b.grow_hours
    return sortAsc.value ? cmp : -cmp
  })

  return result
})
</script>
