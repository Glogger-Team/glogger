<template>
  <div
    class="bg-surface-elevated rounded border px-2.5 py-2 flex flex-col gap-1.5 cursor-pointer hover:bg-surface-elevated/80 transition-colors"
    :class="[
      selected ? 'border-green-700/40' : 'border-border-default',
      compact ? 'px-2 py-1.5 gap-1' : '',
    ]"
    @click="emit('select')"
  >
    <!-- Header: NPC name + pin + favor badge -->
    <div v-if="showHeader" class="flex items-center justify-between gap-1">
      <NpcInline :reference="npc.key" />
      <div class="flex items-center gap-1 shrink-0">
        <button
          class="p-0.5 rounded transition-colors"
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
        <span
          v-if="showFavor && favorTier"
          class="text-[0.65rem] px-1.5 py-0.5 rounded border shrink-0"
          :class="favorBadgeClasses(favorTier)"
        >
          {{ tierDisplayName(favorTier) }}
        </span>
      </div>
    </div>

    <!-- Area / Zone -->
    <AreaInline v-if="npc.area_friendly_name" :reference="npc.area_friendly_name" class="text-[0.6rem]" />

    <!-- Gift tracking -->
    <div v-if="showGiftTracking && giftTracking" class="flex flex-col gap-1">
      <div class="flex items-center justify-between text-xs">
        <span class="text-text-secondary">Gifts</span>
        <div class="flex items-center gap-1.5">
          <span :class="giftTracking.giftsThisWeek >= giftTracking.maxGifts ? 'text-value-positive' : 'text-text-primary'">
            {{ giftTracking.giftsThisWeek }} / {{ giftTracking.maxGifts }}
          </span>
          <slot name="gift-actions" />
        </div>
      </div>
      <!-- Gift dots -->
      <div class="flex items-center gap-1">
        <div
          v-for="i in giftTracking.maxGifts"
          :key="i"
          class="w-2 h-2 rounded-full"
          :class="i <= giftTracking.giftsThisWeek
            ? 'bg-status-active'
            : 'bg-surface-default border border-border-default'"
        />
      </div>
    </div>

    <!-- Vendor gold -->
    <div v-if="showVendorGold && storeService" class="flex items-center gap-1.5 text-xs">
      <span class="text-accent-gold">$</span>
      <span class="text-text-secondary">Gold:</span>
      <span v-if="vendorStatus?.vendor_gold_available != null" class="text-text-primary">
        {{ vendorStatus.vendor_gold_available.toLocaleString() }}
        <span v-if="vendorStatus.vendor_gold_max != null" class="text-text-dim">
          / {{ vendorStatus.vendor_gold_max.toLocaleString() }}
        </span>
      </span>
      <span v-else class="text-text-dim">unknown</span>
      <span v-if="vendorTimerExpired" class="text-value-positive text-[0.6rem] italic">
        (likely reset)
      </span>
      <span v-else-if="timerRemaining" class="text-text-dim text-[0.6rem]">
        (resets in {{ timerRemaining }})
      </span>
    </div>

    <!-- Storage -->
    <div v-if="showStorage && storageService" class="flex items-center gap-1.5 text-xs">
      <span class="text-cyan-400">&#x25A3;</span>
      <span class="text-text-secondary">Storage:</span>
      <span v-if="storageSlotsUsed != null && storageSlotsTotal != null" class="text-text-primary">
        {{ storageSlotsUsed }} / {{ storageSlotsTotal }}
        <span class="text-text-dim text-[0.6rem]">
          ({{ Math.round((storageSlotsUsed / storageSlotsTotal) * 100) }}%)
        </span>
      </span>
      <span v-else class="text-text-dim">available</span>
    </div>

    <!-- Preferences -->
    <div v-if="showPreferences && limitedPreferences.length" class="flex flex-col gap-0.5">
      <div class="text-[0.6rem] uppercase tracking-widest text-text-dim border-b border-surface-card pb-0.5 mb-0.5">
        Preferences
      </div>
      <div
        v-for="(pref, i) in limitedPreferences"
        :key="i"
        class="flex items-center gap-1.5 px-1.5 py-0.5 text-xs bg-surface-inset rounded"
      >
        <span
          class="text-[0.6rem] px-1 py-0.5 rounded border min-w-8 text-center shrink-0"
          :class="desireBadgeClasses(pref.desire)"
        >
          {{ pref.desire }}
        </span>
        <span class="text-text-secondary flex-1 truncate">
          {{ pref.name ?? pref.keywords.join(', ') }}
        </span>
        <span
          class="text-[0.6rem] shrink-0 font-mono"
          :class="pref.pref > 0 ? 'text-value-positive' : 'text-value-negative'"
        >
          {{ pref.pref > 0 ? '+' : '' }}{{ pref.pref }}
        </span>
      </div>
    </div>

    <!-- Services summary -->
    <div v-if="showServices && !compact && services.length > 0" class="flex flex-col gap-1">
      <!-- Training -->
      <div v-for="training in trainingServices" :key="'t'" class="flex items-center gap-1.5 text-xs">
        <span class="text-entity-skill">&#x2726;</span>
        <span class="text-text-secondary">Trains:</span>
        <span class="flex flex-wrap gap-1">
          <SkillInline
            v-for="skill in training.skills"
            :key="skill"
            :reference="skill"
            :show-icon="false"
          />
        </span>
      </div>

      <!-- Storage details -->
      <div v-for="storage in storageServices" :key="'s'" class="flex items-start gap-1.5 text-xs">
        <span class="text-cyan-400 mt-0.5">&#x25A3;</span>
        <div class="flex flex-col">
          <span class="text-text-secondary">
            Storage
            <span class="text-text-dim text-[0.6rem]">({{ tierDisplayName(storage.favor) }}+)</span>
          </span>
          <span v-if="storage.spaceIncreases.length" class="text-[0.6rem] text-text-dim">
            +space at:
            <span v-for="(tier, j) in storage.spaceIncreases" :key="tier">
              <span :class="favorColor(tier)">{{ tierDisplayName(tier) }}</span><span v-if="j < storage.spaceIncreases.length - 1">, </span>
            </span>
          </span>
        </div>
      </div>

      <!-- Vendor details -->
      <div v-for="store in storeServices" :key="'v'" class="flex items-start gap-1.5 text-xs">
        <span class="text-accent-gold mt-0.5">$</span>
        <div class="flex flex-col">
          <span class="text-text-secondary">
            Vendor
            <span class="text-text-dim text-[0.6rem]">({{ tierDisplayName(store.favor) }}+)</span>
          </span>
          <div v-if="store.capIncreases.length" class="flex flex-col text-[0.6rem] text-text-dim">
            <span v-for="cap in store.capIncreases" :key="cap.tier">
              <span :class="favorColor(cap.tier)">{{ tierDisplayName(cap.tier) }}</span>:
              <span class="text-accent-gold">{{ cap.maxGold.toLocaleString() }}</span>c
              <span v-if="cap.itemTypes.length"> — {{ cap.itemTypes.join(', ') }}</span>
            </span>
          </div>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import type { NpcInfo, NpcPreference } from '../../../types/gameData/npcs'
import type { GameStateVendor } from '../../../types/gameState'
import type { StoreService, TrainingService, StorageService } from '../../../types/npcServices'
import { getServices } from '../../../composables/useNpcServices'
import { favorColor, favorBadgeClasses, tierDisplayName } from '../../../composables/useFavorTiers'
import { useReferenceShelfStore } from '../../../stores/referenceShelfStore'
import AreaInline from '../Area/AreaInline.vue'
import NpcInline from './NpcInline.vue'
import SkillInline from '../Skill/SkillInline.vue'

interface GiftTracking {
  giftsThisWeek: number
  maxGifts: number
}

const props = withDefaults(defineProps<{
  npc: NpcInfo
  favorTier?: string | null
  vendorStatus?: GameStateVendor | null
  storageSlotsUsed?: number | null
  storageSlotsTotal?: number | null
  compact?: boolean
  selected?: boolean
  giftTracking?: GiftTracking | null
  showFavor?: boolean
  showHeader?: boolean
  showVendorGold?: boolean
  showStorage?: boolean
  showGiftTracking?: boolean
  showPreferences?: boolean
  showServices?: boolean
  maxPreferences?: number
}>(), {
  favorTier: null,
  vendorStatus: null,
  storageSlotsUsed: null,
  storageSlotsTotal: null,
  compact: false,
  selected: false,
  giftTracking: null,
  showFavor: true,
  showHeader: true,
  showVendorGold: true,
  showStorage: true,
  showGiftTracking: false,
  showPreferences: true,
  showServices: true,
  maxPreferences: 5,
})

const emit = defineEmits<{
  select: []
}>()

const shelf = useReferenceShelfStore()
const pinned = computed(() => shelf.isPinned('npc', props.npc.key))
function togglePin() {
  shelf.togglePin({ type: 'npc', reference: props.npc.key, label: props.npc.name })
}

// Parse NPC services
const services = computed(() => getServices(props.npc))

const trainingServices = computed(() =>
  services.value.filter((s): s is TrainingService => s.type === 'Training')
)
const storeServices = computed(() =>
  services.value.filter((s): s is StoreService => s.type === 'Store')
)
const storageServices = computed(() =>
  services.value.filter((s): s is StorageService => s.type === 'Storage')
)

// Convenience: first store/storage service for the summary rows
const storeService = computed(() => storeServices.value[0] ?? null)
const storageService = computed(() => storageServices.value[0] ?? null)

// Sorted + limited preferences
const limitedPreferences = computed<NpcPreference[]>(() => {
  const sorted = [...(props.npc.preferences ?? [])].sort((a, b) => b.pref - a.pref)
  return sorted.slice(0, props.maxPreferences)
})

// Vendor gold timer — expired detection
const vendorTimerExpired = computed(() => {
  if (!props.vendorStatus?.vendor_gold_timer_start) return false
  const start = new Date(props.vendorStatus.vendor_gold_timer_start + 'Z')
  return Date.now() >= start.getTime() + 168 * 60 * 60 * 1000
})

// Vendor gold timer
const timerRemaining = computed(() => {
  if (!props.vendorStatus?.vendor_gold_timer_start) return null
  const start = new Date(props.vendorStatus.vendor_gold_timer_start + 'Z')
  const resetAt = new Date(start.getTime() + 168 * 60 * 60 * 1000) // 168 hours
  const now = new Date()
  const remaining = resetAt.getTime() - now.getTime()
  if (remaining <= 0) return null
  const hours = Math.floor(remaining / (1000 * 60 * 60))
  const days = Math.floor(hours / 24)
  const remainingHours = hours % 24
  return days > 0 ? `${days}d ${remainingHours}h` : `${hours}h`
})

function desireBadgeClasses(desire: string): string {
  switch (desire.toLowerCase()) {
    case 'love':
      return 'bg-red-900/30 border-red-700/40 text-red-300'
    case 'like':
      return 'bg-green-900/30 border-green-700/40 text-green-300'
    case 'hate':
      return 'bg-red-900/40 border-red-600/50 text-red-400'
    default:
      return 'bg-surface-elevated border-border-default text-text-muted'
  }
}
</script>
