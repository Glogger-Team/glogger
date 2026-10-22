<template>
  <div class="flex flex-col gap-2 overflow-hidden h-full">
    <!-- Filters -->
    <div class="flex items-center gap-2 flex-wrap">
      <input
        v-model="filter"
        type="text"
        placeholder="Filter NPCs..."
        class="px-3 py-1.5 bg-surface-base border border-border-default rounded text-sm text-text-primary placeholder-text-muted focus:outline-none focus:border-accent-gold/50 w-40" />

      <select
        v-model="groupBy"
        class="px-2 py-1.5 bg-surface-base border border-border-default rounded text-xs text-text-primary cursor-pointer">
        <option value="area">By Area</option>
        <option value="favor">By Favor</option>
        <option value="none">No Grouping</option>
      </select>

      <select
        v-model="sortBy"
        class="px-2 py-1.5 bg-surface-base border border-border-default rounded text-xs text-text-primary cursor-pointer">
        <option value="favor">Favor</option>
        <option value="name">Name</option>
        <option value="spent">Councils Spent</option>
      </select>

      <label class="flex items-center gap-1 text-[0.65rem] text-text-muted cursor-pointer">
        <input type="checkbox" v-model="hideNeutral" class="cursor-pointer" />
        Hide neutral
      </label>

      <span class="text-xs text-text-dim ml-auto">{{ totalVisible }} NPCs</span>
    </div>

    <!-- NPC list -->
    <div class="overflow-y-auto flex-1 border border-surface-elevated">
      <template v-for="group in groupedNpcs" :key="group.label">
        <div
          v-if="group.label"
          class="sticky top-0 z-10 bg-surface-base px-2 py-1 text-[0.65rem] uppercase tracking-widest text-text-dim border-b border-surface-elevated cursor-pointer flex items-center gap-1"
          @click="toggleGroup(group.label)">
          <span class="text-text-secondary text-xs">{{ collapsedGroups.has(group.label) ? '▶' : '▼' }}</span>
          {{ group.label }} ({{ group.npcs.length }})
        </div>

        <template v-if="!group.label || !collapsedGroups.has(group.label)">
          <div
            v-for="row in group.npcs"
            :key="row.npc_key"
            class="flex items-center gap-2 px-2 py-1 cursor-pointer border-b border-surface-dark text-xs hover:bg-surface-row-hover"
            :class="{
              'bg-surface-card border-l-2 border-l-accent-gold': selectedNpcKey === row.npc_key,
            }"
            @click="$emit('select', row.npc_key)">
            <span class="flex-1 text-text-primary/75 truncate">{{ row.display_name }}</span>

            <!-- Area (small) -->
            <span v-if="row.area_friendly_name" class="text-text-dim text-[0.6rem] truncate max-w-20 shrink-0">
              {{ row.area_friendly_name }}
            </span>

            <!-- Game state activity dot -->
            <span
              v-if="row.has_gamestate_data"
              class="w-1.5 h-1.5 rounded-full bg-accent-gold shrink-0"
              title="Live favor data"></span>

            <!-- Favor badge -->
            <span
              class="text-[0.65rem] px-1.5 py-0.5 rounded border shrink-0 min-w-16 text-center"
              :class="favorBadgeClasses(row.effective_tier)">
              {{ tierDisplayName(row.effective_tier) }}
            </span>
          </div>
        </template>
      </template>

      <div v-if="totalVisible === 0" class="text-text-dim text-xs italic p-4 text-center">
        No NPCs match your filters.
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed } from 'vue'
import type { NpcInfo } from '../../types/gameData'
import type { SnapshotNpcFavor } from '../../types/database'
import type { GameStateFavor } from '../../types/gameState'
import { FAVOR_TIERS, tierIndex, favorBadgeClasses, tierDisplayName } from '../../composables/useFavorTiers'

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

interface NpcGroup {
  label: string
  npcs: NpcRow[]
}

const props = defineProps<{
  snapshotFavor: SnapshotNpcFavor[]
  favorByNpc: Record<string, GameStateFavor>
  npcsByKey: Record<string, NpcInfo>
  selectedNpcKey: string | null
}>()

defineEmits<{
  select: [npcKey: string]
}>()

const filter = ref('')
const groupBy = ref<'area' | 'favor' | 'none'>('favor')
const sortBy = ref<'favor' | 'name' | 'spent'>('favor')
const hideNeutral = ref(false)
const collapsedGroups = ref(new Set<string>())

function toggleGroup(label: string) {
  const next = new Set(collapsedGroups.value)
  if (next.has(label)) next.delete(label)
  else next.add(label)
  collapsedGroups.value = next
}

// Build merged rows from all data sources
const allRows = computed<NpcRow[]>(() => {
  const seen = new Set<string>()
  const rows: NpcRow[] = []

  // Start from snapshot NPCs
  for (const snap of props.snapshotFavor) {
    seen.add(snap.npc_key)
    const cdn = props.npcsByKey[snap.npc_key] ?? null
    const gs = props.favorByNpc[snap.npc_key] ?? null
    const effectiveTier = gs?.favor_tier ?? snap.favor_level
    rows.push({
      npc_key: snap.npc_key,
      display_name: cdn?.name ?? gs?.npc_name ?? snap.npc_key.replace(/^NPC_/, ''),
      area_friendly_name: cdn?.area_friendly_name ?? null,
      snapshot_favor: snap.favor_level,
      gamestate_favor: gs,
      cdnData: cdn,
      effective_tier: effectiveTier,
      has_gamestate_data: !!gs,
      snapshot_favor_points: snap.favor ?? null,
      snapshot_councils_spent: snap.councils_spent_at_vendor ?? null,
      snapshot_notes: snap.player_notes ?? null,
      snapshot_vendor_reset: snap.vendor_reset_time ?? null,
    })
  }

  // Add game-state-only NPCs (discovered this session but not in snapshot)
  for (const [key, gs] of Object.entries(props.favorByNpc)) {
    if (seen.has(key)) continue
    const cdn = props.npcsByKey[key] ?? null
    rows.push({
      npc_key: key,
      display_name: cdn?.name ?? gs.npc_name ?? key.replace(/^NPC_/, ''),
      area_friendly_name: cdn?.area_friendly_name ?? null,
      snapshot_favor: null,
      gamestate_favor: gs,
      cdnData: cdn,
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

// Filter
const filteredRows = computed(() => {
  let rows = allRows.value

  if (hideNeutral.value) {
    rows = rows.filter(r => r.effective_tier !== 'Neutral')
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

  return rows
})

// Sort
const sortedRows = computed(() => {
  const rows = [...filteredRows.value]
  switch (sortBy.value) {
    case 'favor':
      rows.sort((a, b) => tierIndex(a.effective_tier) - tierIndex(b.effective_tier) || a.display_name.localeCompare(b.display_name))
      break
    case 'name':
      rows.sort((a, b) => a.display_name.localeCompare(b.display_name))
      break
    case 'spent':
      rows.sort((a, b) => (b.snapshot_councils_spent ?? -1) - (a.snapshot_councils_spent ?? -1) || a.display_name.localeCompare(b.display_name))
      break
  }
  return rows
})

// Group
const groupedNpcs = computed<NpcGroup[]>(() => {
  const rows = sortedRows.value

  if (groupBy.value === 'none') {
    return [{ label: '', npcs: rows }]
  }

  if (groupBy.value === 'favor') {
    return FAVOR_TIERS
      .map(tier => ({
        label: tierDisplayName(tier),
        npcs: rows.filter(r => r.effective_tier === tier),
      }))
      .filter(g => g.npcs.length > 0)
  }

  // Group by area
  const areaMap = new Map<string, NpcRow[]>()
  for (const row of rows) {
    const area = row.area_friendly_name ?? 'Unknown'
    if (!areaMap.has(area)) areaMap.set(area, [])
    areaMap.get(area)!.push(row)
  }
  return Array.from(areaMap.entries())
    .sort(([a], [b]) => a.localeCompare(b))
    .map(([label, npcs]) => ({ label, npcs }))
})

const totalVisible = computed(() => filteredRows.value.length)
</script>
