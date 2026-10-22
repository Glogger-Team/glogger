<template>
  <div
    v-if="pinnedNpcs.length > 0"
    class="fixed bottom-6 left-0 right-0 z-40 bg-surface-base border-t border-border-default px-3 py-1.5 flex items-center gap-3 overflow-x-auto"
  >
    <div
      v-for="entry in pinnedNpcs"
      :key="entry.key"
      class="flex items-center gap-2 bg-surface-elevated rounded px-2.5 py-1 border border-border-default min-w-0 shrink-0"
      style="max-width: 320px"
    >
      <!-- NPC name (clickable) -->
      <NpcInline :reference="entry.key" class="shrink-0" />

      <!-- Favor badge -->
      <span
        v-if="entry.favorTier"
        class="text-[10px] px-1 py-0.5 rounded border shrink-0"
        :class="favorBadgeClasses(entry.favorTier)"
      >
        {{ tierDisplayName(entry.favorTier) }}
        <span v-if="entry.snapshotFavorPoints != null" class="opacity-70">({{ entry.snapshotFavorPoints.toLocaleString() }})</span>
      </span>

      <!-- Vendor gold -->
      <span v-if="entry.vendorGold != null" class="text-[10px] text-text-secondary shrink-0 whitespace-nowrap">
        <span class="text-accent-gold">$</span>
        {{ entry.vendorGold.toLocaleString() }}<span v-if="entry.vendorGoldMax != null" class="text-text-dim">/{{ entry.vendorGoldMax.toLocaleString() }}</span>
      </span>
      <span v-else-if="entry.snapshotCouncilsSpent != null" class="text-[10px] text-text-dim shrink-0 whitespace-nowrap">
        spent {{ entry.snapshotCouncilsSpent.toLocaleString() }}
      </span>
      <!-- Storage -->
      <span v-if="entry.storageUsed != null" class="text-[10px] text-text-secondary shrink-0 whitespace-nowrap">
        <span class="text-cyan-400">&#x25A3;</span>
        {{ entry.storageUsed }}<span v-if="entry.storageTotal != null" class="text-text-dim">/{{ entry.storageTotal }}</span>
      </span>

      <!-- Vendor timer -->
      <span v-if="entry.timerRemaining" class="text-[10px] text-text-dim shrink-0 whitespace-nowrap">
        &#x23F1; {{ entry.timerRemaining }}
      </span>

      <!-- Unpin button -->
      <button
        class="text-text-dim hover:text-text-default text-xs leading-none ml-auto cursor-pointer shrink-0"
        title="Unpin NPC"
        @click="unpinNpc(entry.key)"
      >
        &#x2715;
      </button>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import { usePinnedNpc } from '../../../composables/usePinnedNpc'
import { useGameDataStore } from '../../../stores/gameDataStore'
import { useGameStateStore } from '../../../stores/gameStateStore'
import { useCharacterStore } from '../../../stores/characterStore'
import { favorBadgeClasses, tierDisplayName } from '../../../composables/useFavorTiers'
import { hasVendor, hasStorage } from '../../../composables/useNpcServices'
import NpcInline from './NpcInline.vue'

const { pinnedNpcKeys, unpinNpc } = usePinnedNpc()
const gameData = useGameDataStore()
const gameState = useGameStateStore()
const characterStore = useCharacterStore()

interface PinnedNpcEntry {
  key: string
  favorTier: string | null
  snapshotFavorPoints: number | null
  snapshotCouncilsSpent: number | null
  vendorGold: number | null
  vendorGoldMax: number | null
  storageUsed: number | null
  storageTotal: number | null
  timerRemaining: string | null
}

const pinnedNpcs = computed<PinnedNpcEntry[]>(() => {
  return pinnedNpcKeys.value.map((npcKey) => {
    const npc = gameData.npcsByKey[npcKey] ?? null

    // Favor
    const gsf = gameState.favorByNpc[npcKey]
    const snapFavor = characterStore.npcFavor.find(f => f.npc_key === npcKey)
    const favorTier = gsf?.favor_tier ?? snapFavor?.favor_level ?? null

    // Vendor
    const vendorData = gameState.vendorByNpc[npcKey] ?? null
    const showVendor = npc ? hasVendor(npc) : false
    const vendorGold = showVendor ? (vendorData?.vendor_gold_available ?? null) : null
    const vendorGoldMax = showVendor ? (vendorData?.vendor_gold_max ?? null) : null

    // Vendor timer
    let timerRemaining: string | null = null
    if (vendorData?.vendor_gold_timer_start) {
      const start = new Date(vendorData.vendor_gold_timer_start + 'Z')
      const resetAt = new Date(start.getTime() + 168 * 60 * 60 * 1000)
      const now = new Date()
      const remaining = resetAt.getTime() - now.getTime()
      if (remaining > 0) {
        const hours = Math.floor(remaining / (1000 * 60 * 60))
        const days = Math.floor(hours / 24)
        const remainingHours = hours % 24
        timerRemaining = days > 0 ? `${days}d ${remainingHours}h` : `${hours}h`
      }
    }

    // Storage
    const showStorage = npc ? hasStorage(npc) : false
    const storageItems = gameState.storageByVault[npcKey]
    const storageUsed = showStorage && storageItems ? storageItems.length : null
    const vault = gameState.storageVaultsByKey[npcKey]
    const storageTotal = showStorage && vault ? gameState.getVaultUnlockedSlots(vault) : null

    return {
      key: npcKey,
      favorTier,
      snapshotFavorPoints: snapFavor?.favor ?? null,
      snapshotCouncilsSpent: snapFavor?.councils_spent_at_vendor ?? null,
      vendorGold,
      vendorGoldMax,
      storageUsed,
      storageTotal,
      timerRemaining,
    }
  })
})
</script>
