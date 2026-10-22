<template>
  <div class="flex flex-col gap-2">
    <div class="text-[10px] uppercase tracking-widest text-text-dim border-b border-surface-card pb-0.5">
      Favor
    </div>

    <!-- Current favor summary -->
    <div class="flex items-center gap-3">
      <span
        class="text-xs px-2 py-0.5 rounded border font-semibold"
        :class="favorBadgeClasses(effectiveTier)">
        {{ tierDisplayName(effectiveTier) }}
      </span>
      <span v-if="snapshotTier && gamestateTier && snapshotTier !== gamestateTier" class="text-[10px] text-text-dim">
        Snapshot: <span :class="favorColor(snapshotTier)">{{ tierDisplayName(snapshotTier) }}</span>
      </span>
      <span v-if="snapshotFavorPoints != null" class="text-[10px] text-text-dim">
        {{ snapshotFavorPoints.toLocaleString() }} favor (snapshot)
      </span>
      <span v-if="gamestateFavor?.cumulative_delta" class="text-[10px] text-text-dim">
        Session: <span class="text-accent-gold">{{ gamestateFavor.cumulative_delta > 0 ? '+' : '' }}{{ gamestateFavor.cumulative_delta.toFixed(1) }}</span> favor
      </span>
    </div>

    <!-- Player notes from the snapshot (read-only) -->
    <div v-if="snapshotNotes?.length" class="text-xs text-text-secondary italic border-l-2 border-border-subtle pl-2">
      &ldquo;{{ snapshotNotes }}&rdquo;
     </div>

    <!-- Tier ladder -->
    <div class="flex flex-col gap-0.5">
      <div
        v-for="tier in FAVOR_TIERS"
        :key="tier"
        class="flex items-center gap-2 px-2 py-0.5 rounded text-xs"
        :class="tierRowClasses(tier)">
        <!-- Status indicator -->
        <span class="w-4 text-center shrink-0">
          <template v-if="tier === effectiveTier">&#x25C6;</template>
          <template v-else-if="isUnlocked(tier)">&#x2713;</template>
          <template v-else>&#x2022;</template>
        </span>

        <!-- Tier name -->
        <span class="min-w-24">{{ tierDisplayName(tier) }}</span>

        <!-- Storage slots at this tier -->
        <span v-if="slotsAtTier(tier) != null" class="text-[10px] text-text-dim">
          {{ slotsAtTier(tier) }} slots
        </span>

        <!-- Vendor gold cap at this tier -->
        <span v-if="goldCapExactAtTier(tier) != null" class="text-[10px] text-text-dim">
          {{ formatGold(goldCapExactAtTier(tier)!) }}
        </span>

        <!-- Points to reach (for tiers above current) -->
        <span v-if="!isUnlocked(tier) && pointsNeeded(tier) !== null" class="text-text-dim text-[10px] ml-auto">
          {{ pointsNeeded(tier) }} favor
        </span>

        <!-- Services unlocked at this tier -->
        <span v-if="servicesUnlockedAt(tier).length" class="flex gap-1 ml-auto">
          <span
            v-for="svc in servicesUnlockedAt(tier)"
            :key="svc"
            class="text-[10px] px-1 py-px rounded bg-surface-elevated text-text-dim">
            {{ svc }}
          </span>
        </span>

        <!-- Progress bar for current tier -->
        <div
          v-if="tier === effectiveTier && progressPercent !== null"
          class="flex-1 flex items-center gap-1.5 ml-2">
          <div class="flex-1 h-1 bg-border-default rounded-sm overflow-hidden">
            <div class="h-full bg-accent-gold/60 rounded-sm" :style="{ width: progressPercent + '%' }"></div>
          </div>
          <span class="text-[10px] text-text-dim shrink-0">{{ progressPercent }}%</span>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import type { NpcInfo } from '../../../types/gameData'
import type { GameStateFavor } from '../../../types/gameState'
import { useGameStateStore } from '../../../stores/gameStateStore'
import {
  getServices,
  getStoreService,
} from '../../../composables/useNpcServices'
import {
  FAVOR_TIERS,
  tierIndex,
  isTierAtOrAbove,
  favorColor,
  favorBadgeClasses,
  tierDisplayName,
  pointsToNextTier,
} from '../../../composables/useFavorTiers'

const props = defineProps<{
  snapshotTier: string | null
  snapshotFavorPoints?: number | null
  snapshotNotes?: string | null
  gamestateFavor: GameStateFavor | null
  npcKey?: string | null
  npc?: NpcInfo | null
}>()

const gameState = useGameStateStore()

const gamestateTier = computed(() => props.gamestateFavor?.favor_tier ?? null)
const effectiveTier = computed(() => gamestateTier.value ?? props.snapshotTier ?? 'Neutral')

function isUnlocked(tier: string): boolean {
  return isTierAtOrAbove(effectiveTier.value, tier)
}

function tierRowClasses(tier: string): string {
  if (tier === effectiveTier.value) {
    return 'bg-accent-gold/10 border border-accent-gold/30'
  }
  if (isUnlocked(tier)) {
    return 'text-text-secondary'
  }
  return 'text-text-dim opacity-50'
}

/** Points needed to reach a tier from the tier below it */
function pointsNeeded(tier: string): number | null {
  const idx = tierIndex(tier)
  if (idx >= FAVOR_TIERS.length - 1) return null
  const tierBelow = FAVOR_TIERS[idx + 1]
  return pointsToNextTier(tierBelow)
}

// ── Storage slots per tier ────────────────────────────────────────────

const vaultLevels = computed<Record<string, number> | null>(() => {
  if (!props.npcKey) return null
  const vault = gameState.storageVaultsByKey[props.npcKey]
  return vault?.levels ?? null
})

function slotsAtTier(tier: string): number | null {
  if (!vaultLevels.value) return null
  return vaultLevels.value[tier] ?? null
}

// ── Vendor gold cap per tier ──────────────────────────────────────────

const capIncreasesByTier = computed<Record<string, number>>(() => {
  if (!props.npc) return {}
  const store = getStoreService(props.npc)
  if (!store || !store.capIncreases.length) return {}
  const map: Record<string, number> = {}
  for (const cap of store.capIncreases) {
    map[cap.tier] = cap.maxGold
  }
  return map
})

function goldCapExactAtTier(tier: string): number | null {
  return capIncreasesByTier.value[tier] ?? null
}

function formatGold(amount: number): string {
  return amount.toLocaleString() + 'c'
}

// ── Services unlocked at tier ─────────────────────────────────────────

const SERVICE_LABELS: Record<string, string> = {
  Store: 'Vendor',
  Storage: 'Storage',
  Training: 'Training',
  Barter: 'Barter',
  Consignment: 'Consignment',
}

const servicesByUnlockTier = computed<Record<string, string[]>>(() => {
  if (!props.npc) return {}
  const services = getServices(props.npc)
  const map: Record<string, string[]> = {}
  for (const svc of services) {
    const label = SERVICE_LABELS[svc.type] ?? svc.type
    if (!map[svc.favor]) map[svc.favor] = []
    map[svc.favor].push(label)
  }
  return map
})

function servicesUnlockedAt(tier: string): string[] {
  return servicesByUnlockTier.value[tier] ?? []
}

/** Estimated progress within current tier based on cumulative delta */
const progressPercent = computed(() => {
  if (!props.gamestateFavor?.cumulative_delta) return null
  const needed = pointsToNextTier(effectiveTier.value)
  if (!needed) return null
  const delta = props.gamestateFavor.cumulative_delta
  if (delta <= 0) return 0
  return Math.min(100, Math.round((delta / needed) * 100))
})
</script>
