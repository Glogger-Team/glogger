<template>
  <div
    class="flex-1 overflow-y-auto border border-surface-elevated p-4 flex flex-col gap-4"
    :class="{ 'items-center justify-center': !npcKey }">
    <div v-if="!npcKey" class="text-border-default italic">
      Select an NPC to inspect
    </div>

    <template v-else-if="cdnData">
      <!-- Header -->
      <div class="flex flex-col gap-1">
        <div class="flex items-center justify-between gap-2">
          <div class="text-accent-gold text-base font-bold">{{ cdnData.name }}</div>
          <button
            class="p-0.5 rounded transition-colors shrink-0"
            :class="pinned
              ? 'text-accent-blue hover:text-accent-blue-bright'
              : 'text-text-muted hover:text-text-default'"
            :title="pinned ? 'Unpin from shelf' : 'Pin to shelf'"
            @click.stop="togglePin"
          >
            <svg class="w-3.5 h-3.5" viewBox="0 0 24 24" :fill="pinned ? 'currentColor' : 'none'" stroke="currentColor" stroke-width="2">
              <path d="M12 2L12 12M9 4L12 2L15 4" />
              <path d="M5 12H19" />
              <path d="M12 12V22" />
            </svg>
          </button>
        </div>
        <div v-if="cdnData.area_friendly_name" class="text-xs">
          <AreaInline :reference="cdnData.area_name ?? cdnData.area_friendly_name" />
        </div>
        <div v-if="cdnData.desc" class="text-xs text-text-secondary italic mt-1">
          {{ cdnData.desc }}
        </div>
      </div>

      <!-- Favor -->
      <NpcFavorSection
        :snapshot-tier="snapshotTier"
        :snapshot-favor-points="snapshotFavor?.favor ?? null"
        :snapshot-notes="snapshotFavor?.player_notes ?? null"
        :gamestate-favor="gamestateFavor"
        :npc-key="npcKey"
        :npc="cdnData" />
      <!-- Vendor Status -->
      <NpcVendorSection
        v-if="hasVendorService"
        :vendor-status="vendorStatus"
        :npc="cdnData"
        :player-tier="effectiveTier"
        :snapshot-councils-spent="snapshotFavor?.councils_spent_at_vendor ?? null"
        :snapshot-vendor-reset="snapshotFavor?.vendor_reset_time ?? null" />

      <!-- Vendor Buys Summary -->
      <div v-if="vendorCapAtTier" class="text-xs text-text-secondary px-2">
        <span class="text-text-muted">Buys:</span>
        {{ vendorCapAtTier.itemTypes.join(', ') }}
        <span class="text-text-dim">(up to {{ vendorCapAtTier.maxGold.toLocaleString() }}c each)</span>
      </div>

      <!-- Services -->
      <NpcServicesSection
        :npc="cdnData"
        :player-tier="effectiveTier" />

      <!-- Preferences -->
      <NpcPreferencesSection :npc="cdnData" />

      <!-- Giftable Items in Inventory & Storage -->
      <NpcInventoryGiftsSection :npc="cdnData" />

      <!-- Quests -->
      <NpcQuestsSection :npc-key="npcKey" />

      <!-- Storage -->
      <NpcStorageSection
        v-if="hasStorageService"
        :npc-key="npcKey"
        :npc="cdnData"
        :favor-tier="effectiveTier" />

      <!-- Trained Skills (if not already shown in services) -->
      <div v-if="cdnData.trains_skills?.length && !hasTrainingService" class="flex flex-col gap-1.5">
        <div class="text-[10px] uppercase tracking-widest text-text-dim border-b border-surface-card pb-0.5">Training</div>
        <div class="flex flex-wrap gap-1">
          <SkillInline
            v-for="skill in cdnData.trains_skills"
            :key="skill"
            :reference="skill" />
        </div>
      </div>
    </template>

    <!-- Fallback: no CDN data but we have favor info -->
    <template v-else-if="npcKey">
      <div class="flex flex-col gap-2">
        <div class="text-accent-gold text-base font-bold">{{ displayName }}</div>
        <NpcFavorSection
          :snapshot-tier="snapshotTier"
          :snapshot-favor-points="snapshotFavor?.favor ?? null"
          :snapshot-notes="snapshotFavor?.player_notes ?? null"
          :gamestate-favor="gamestateFavor" />
        <div class="text-xs text-text-dim italic">No additional data available for this NPC.</div>
      </div>
    </template>
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import type { NpcInfo } from '../../types/gameData'
import type { GameStateFavor, GameStateVendor } from '../../types/gameState'
import { hasTraining as npcHasTraining, hasVendor, hasStorage, goldCapAtTier } from '../../composables/useNpcServices'
import { useGameStateStore } from '../../stores/gameStateStore'
import { useReferenceShelfStore } from '../../stores/referenceShelfStore'
import NpcFavorSection from './NpcDetailSections/NpcFavorSection.vue'
import NpcVendorSection from './NpcDetailSections/NpcVendorSection.vue'
import NpcStorageSection from './NpcDetailSections/NpcStorageSection.vue'
import NpcInventoryGiftsSection from './NpcDetailSections/NpcInventoryGiftsSection.vue'
import NpcServicesSection from './NpcDetailSections/NpcServicesSection.vue'
import NpcPreferencesSection from './NpcDetailSections/NpcPreferencesSection.vue'
import NpcQuestsSection from './NpcDetailSections/NpcQuestsSection.vue'
import SkillInline from '../Shared/Skill/SkillInline.vue'
import AreaInline from '../Shared/Area/AreaInline.vue'

const props = defineProps<{
  npcKey: string | null
  snapshotTier: string | null
  snapshotFavor?: import('../../types/database').SnapshotNpcFavor | null
  gamestateFavor: GameStateFavor | null
  cdnData: NpcInfo | null
  vendorStatus?: GameStateVendor | null
}>()

const gameState = useGameStateStore()
const shelf = useReferenceShelfStore()

const pinned = computed(() => {
  if (!props.npcKey || !props.cdnData) return false
  return shelf.isPinned('npc', props.npcKey)
})

function togglePin() {
  if (!props.npcKey || !props.cdnData) return
  shelf.togglePin({ type: 'npc', reference: props.npcKey, label: props.cdnData.name })
}

const effectiveTier = computed(() =>
  props.gamestateFavor?.favor_tier ?? props.snapshotTier ?? 'Neutral'
)

const displayName = computed(() =>
  props.cdnData?.name ?? props.gamestateFavor?.npc_name ?? props.npcKey?.replace(/^NPC_/, '') ?? ''
)

const hasTrainingService = computed(() => {
  if (!props.cdnData) return false
  return npcHasTraining(props.cdnData)
})

const hasVendorService = computed(() => {
  if (!props.cdnData) return false
  return hasVendor(props.cdnData)
})

const hasStorageService = computed(() => {
  if (!props.cdnData) return false
  return hasStorage(props.cdnData)
})

const vendorCapAtTier = computed(() => {
  if (!props.cdnData || !hasVendorService.value) return null
  return goldCapAtTier(props.cdnData, effectiveTier.value)
})

const vendorStatus = computed(() => {
  if (props.vendorStatus !== undefined) return props.vendorStatus
  if (!props.npcKey) return null
  return gameState.vendorByNpc[props.npcKey] ?? null
})
</script>
