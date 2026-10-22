<template>
  <div class="flex flex-col gap-3 h-full overflow-hidden">
    <!-- Summary bar -->
    <div v-if="hasData" class="flex items-center justify-between">
      <div class="flex items-center gap-3">
        <span class="text-sm text-text-primary font-semibold">
          {{ settingsStore.settings.activeCharacterName ?? 'Character' }}
        </span>
        <span v-if="settingsStore.settings.activeServerName" class="text-xs text-text-muted">
          {{ settingsStore.settings.activeServerName }}
        </span>
      </div>
      <div class="flex items-center gap-4 text-xs">
        <div>
          <span class="text-text-muted">Known NPCs:</span>
          <span class="text-accent-gold font-bold ml-1">{{ totalNpcs }}</span>
        </div>
        <div>
          <span class="text-text-muted">Above Neutral:</span>
          <span class="text-value-positive font-bold ml-1">{{ aboveNeutral }}</span>
        </div>
      </div>
    </div>

    <!-- Empty state -->
    <EmptyState
      v-if="!hasData"
      primary="No NPC data loaded."
      secondary="Log in to a character or import a character report to see NPC relationships." />

    <!-- Three-panel layout: filters | card grid | detail -->
    <PaneLayout
      v-else
      screen-key="npcs"
      :left-pane="{ title: 'Filters', defaultWidth: 260, minWidth: 200, maxWidth: 400 }"
      :right-pane="{ title: 'Details', defaultWidth: 700, minWidth: 400, maxWidth: 1200, defaultCollapsed: !selectedNpcKey }">
      <template #left>
        <NpcFilterPanel
          ref="filterPanelRef"
          :snapshot-favor="characterStore.npcFavor"
          :favor-by-npc="gameState.favorByNpc"
          :npcs-by-key="gameData.npcsByKey" />
      </template>

      <!-- Center: card grid -->
      <div class="h-full overflow-y-auto p-3">
        <div v-if="filteredNpcs.length === 0" class="flex items-center justify-center h-full">
          <span class="text-text-dim text-sm italic">No NPCs match your filters.</span>
        </div>
        <div
          v-else
          class="grid gap-3"
          style="grid-template-columns: repeat(auto-fill, minmax(280px, 1fr))">
          <NpcCard
            v-for="row in filteredNpcs"
            :key="row.npc_key"
            :npc="row.cdnData!"
            :favor-tier="row.effective_tier"
            :vendor-status="gameState.vendorByNpc[row.npc_key] ?? null"
            :selected="selectedNpcKey === row.npc_key"
            :max-preferences="3"
            @select="selectNpc(row.npc_key)" />
        </div>
      </div>

      <template #right>
        <NpcDetailPanel
          :npc-key="selectedNpcKey"
          :snapshot-tier="selectedSnapshotTier"
          :snapshot-favor="selectedSnapshotFavor"
          :gamestate-favor="selectedGamestateFavor"
          :cdn-data="selectedCdnData" />
      </template>
    </PaneLayout>
  </div>
</template>

<script setup lang="ts">
import { ref, computed } from 'vue'
import { useCharacterStore } from '../../stores/characterStore'
import { useGameStateStore } from '../../stores/gameStateStore'
import { useGameDataStore } from '../../stores/gameDataStore'
import { useSettingsStore } from '../../stores/settingsStore'
import { useViewPrefs } from '../../composables/useViewPrefs'
import EmptyState from '../Shared/EmptyState.vue'
import PaneLayout from '../Shared/PaneLayout.vue'
import NpcFilterPanel from './NpcFilterPanel.vue'
import NpcDetailPanel from './NpcDetailPanel.vue'
import NpcCard from '../Shared/NPC/NpcCard.vue'

const characterStore = useCharacterStore()
const gameState = useGameStateStore()
const gameData = useGameDataStore()
const settingsStore = useSettingsStore()

const selectedNpcKey = ref<string | null>(null)
const filterPanelRef = ref<InstanceType<typeof NpcFilterPanel> | null>(null)

// Share the right pane's collapsed state so we can expand it on NPC selection
const { prefs: rightPanePrefs, update: updateRightPane } = useViewPrefs('npcs.pane.right', {
  collapsed: true,
  width: 700,
})

const hasData = computed(() =>
  characterStore.npcFavor.length > 0 || gameState.favor.length > 0
)

const totalNpcs = computed(() => {
  const keys = new Set<string>()
  for (const f of characterStore.npcFavor) keys.add(f.npc_key)
  for (const f of gameState.favor) keys.add(f.npc_key)
  return keys.size
})

const aboveNeutral = computed(() => {
  const keys = new Set<string>()
  for (const f of characterStore.npcFavor) {
    if (f.favor_level !== 'Neutral') keys.add(f.npc_key)
  }
  for (const f of gameState.favor) {
    if (f.favor_tier && f.favor_tier !== 'Neutral') keys.add(f.npc_key)
  }
  return keys.size
})

/** Filtered NPCs from the filter panel — only include those with CDN data (needed for NpcCard) */
const filteredNpcs = computed(() => {
  const rows = filterPanelRef.value?.filteredRows ?? []
  return rows.filter(r => r.cdnData !== null)
})

const selectedSnapshotTier = computed(() => {
  if (!selectedNpcKey.value) return null
  const snap = characterStore.npcFavor.find(f => f.npc_key === selectedNpcKey.value)
  return snap?.favor_level ?? null
})

const selectedSnapshotFavor = computed(() => {
  if (!selectedNpcKey.value) return null
  return characterStore.npcFavor.find(f => f.npc_key === selectedNpcKey.value) ?? null
})

const selectedGamestateFavor = computed(() => {
  if (!selectedNpcKey.value) return null
  return gameState.favorByNpc[selectedNpcKey.value] ?? null
})

const selectedCdnData = computed(() => {
  if (!selectedNpcKey.value) return null
  return gameData.npcsByKey[selectedNpcKey.value] ?? null
})

function selectNpc(key: string) {
  selectedNpcKey.value = key
  if (rightPanePrefs.value.collapsed) {
    updateRightPane({ collapsed: false })
  }
}
</script>
