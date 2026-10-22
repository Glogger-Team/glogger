<template>
  <div class="flex flex-col gap-1.5">
    <div class="flex items-center justify-between border-b border-surface-card pb-0.5">
      <span class="text-[0.65rem] uppercase tracking-widest text-text-dim">
        Vendor Status
      </span>
      <button
        v-if="!editing"
        class="text-[0.6rem] text-text-muted hover:text-accent-gold transition-colors cursor-pointer"
        title="Manually set vendor gold"
        @click="startEditing"
      >
        &#x270E; Edit
      </button>
    </div>

    <!-- Edit form -->
    <div v-if="editing" class="flex flex-col gap-2 px-2 py-1.5 bg-surface-inset rounded">
      <div class="flex items-center gap-2 text-xs">
        <label class="text-text-muted w-20 shrink-0">Gold Available:</label>
        <input
          v-model.number="editGoldAvailable"
          type="number"
          min="0"
          class="flex-1 px-2 py-1 bg-surface-base border border-border-default rounded text-xs text-text-primary font-mono focus:outline-none focus:border-accent-gold/50"
        />
      </div>
      <div class="flex items-center gap-2 text-xs">
        <label class="text-text-muted w-20 shrink-0">Gold Max:</label>
        <input
          v-model.number="editGoldMax"
          type="number"
          min="0"
          class="flex-1 px-2 py-1 bg-surface-base border border-border-default rounded text-xs text-text-primary font-mono focus:outline-none focus:border-accent-gold/50"
        />
      </div>
      <div class="flex items-center gap-2 text-xs">
        <label class="text-text-muted w-20 shrink-0">Resets in:</label>
        <div class="flex items-center gap-1 flex-1">
          <input
            v-model.number="editResetDays"
            type="number"
            min="0"
            max="7"
            class="w-14 px-2 py-1 bg-surface-base border border-border-default rounded text-xs text-text-primary font-mono focus:outline-none focus:border-accent-gold/50"
          />
          <span class="text-text-dim">d</span>
          <input
            v-model.number="editResetHours"
            type="number"
            min="0"
            max="23"
            class="w-14 px-2 py-1 bg-surface-base border border-border-default rounded text-xs text-text-primary font-mono focus:outline-none focus:border-accent-gold/50"
          />
          <span class="text-text-dim">h</span>
          <span class="text-[10px] text-text-dim italic ml-1">(leave 0 for no timer)</span>
        </div>
      </div>
      <div class="flex items-center gap-2">
        <button
          class="px-2 py-0.5 text-xs bg-accent-gold/20 border border-accent-gold/40 rounded hover:bg-accent-gold/30 text-text-primary cursor-pointer"
          :disabled="saving"
          @click="saveEdit"
        >
          {{ saving ? 'Saving...' : 'Save' }}
        </button>
        <button
          class="px-2 py-0.5 text-xs text-text-muted hover:text-text-primary cursor-pointer"
          @click="editing = false"
        >
          Cancel
        </button>
      </div>
    </div>

    <template v-else-if="vendorStatus">
      <!-- Gold display -->
      <div class="flex items-center gap-2 px-2 text-xs">
        <span class="text-text-muted">Gold:</span>
        <span class="font-bold font-mono" :class="goldColorClass">
          {{ formatGold(vendorStatus.vendor_gold_available) }}
        </span>
        <span v-if="vendorStatus.vendor_gold_max != null" class="text-text-dim">
          / {{ vendorStatus.vendor_gold_max.toLocaleString() }}
        </span>
        <span v-else-if="currentCapGold != null" class="text-text-dim">
          / ~{{ currentCapGold.toLocaleString() }}
        </span>
        <span class="text-text-dim text-[0.6rem]">councils</span>
      </div>

      <!-- Timer / Reset notice -->
      <div v-if="timerExpired" class="flex items-center gap-2 px-2 text-xs">
        <span class="text-value-positive italic">Likely reset to full</span>
        <span class="text-text-dim text-[0.55rem]">(7d+ since last sale)</span>
      </div>
      <div v-else-if="resetTimeLabel" class="flex items-center gap-2 px-2 text-xs">
        <span class="text-text-muted">Resets in:</span>
        <span class="text-text-secondary">~{{ resetTimeLabel }}</span>
        <span class="text-text-dim text-[0.55rem] italic">(estimated)</span>
      </div>

      <!-- Last sell -->
      <div v-if="lastSellLabel" class="flex items-center gap-2 px-2 text-xs">
        <span class="text-text-muted">Last sold:</span>
        <span class="text-text-secondary">{{ lastSellLabel }}</span>
      </div>

      <!-- Snapshot import anchor (display-only, never feeds live math) -->
      <div v-if="snapshotCouncilsSpent != null || snapshotResetDate" class="flex items-center gap-2 px-2 text-xs">
        <span class="text-text-dim">At import:</span>
        <span v-if="snapshotCouncilsSpent != null" class="text-text-secondary">spent {{ snapshotCouncilsSpent.toLocaleString() }}c</span>
        <span v-if="snapshotResetDate" class="text-text-dim">· vendor reset {{ snapshotResetDate }}</span>
      </div>
    </template>

    <div v-else class="flex flex-col gap-1 px-2">
      <div v-if="snapshotCouncilsSpent != null || snapshotResetDate" class="text-xs text-text-secondary">
        At import: <span v-if="snapshotCouncilsSpent != null">spent {{ snapshotCouncilsSpent.toLocaleString() }}c</span><span v-if="snapshotResetDate"> · vendor reset {{ snapshotResetDate }}</span>
      </div>
      <span v-else class="text-xs text-text-dim italic">
        No vendor data yet — sell an item or click Edit to set manually
      </span>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed, ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import type { NpcInfo } from '../../../types/gameData'
import type { GameStateVendor } from '../../../types/gameState'
import { goldCapAtTier } from '../../../composables/useNpcServices'
import { useSettingsStore } from '../../../stores/settingsStore'
import { useGameStateStore } from '../../../stores/gameStateStore'

const props = defineProps<{
  vendorStatus: GameStateVendor | null
  npc: NpcInfo
  playerTier: string
  snapshotCouncilsSpent?: number | null
  snapshotVendorReset?: string | null
}>()

/** Raw ISO date portion only — vendor timestamps are machine strings, no Date math. */
const snapshotResetDate = computed(() =>
  props.snapshotVendorReset ? props.snapshotVendorReset.slice(0, 10) : null,
)

const settingsStore = useSettingsStore()
const gameState = useGameStateStore()

const currentCapGold = computed(() => {
  const cap = goldCapAtTier(props.npc, props.playerTier)
  return cap?.maxGold ?? null
})

const goldColorClass = computed(() => {
  const v = props.vendorStatus
  if (!v || v.vendor_gold_available == null) return 'text-text-secondary'
  const max = v.vendor_gold_max ?? currentCapGold.value
  if (!max || max === 0) return 'text-text-secondary'
  const ratio = v.vendor_gold_available / max
  if (ratio >= 0.7) return 'text-value-positive'
  if (ratio >= 0.3) return 'text-yellow-400'
  return 'text-value-negative'
})

function formatGold(val: number | null | undefined): string {
  if (val == null) return '?'
  return val.toLocaleString()
}

const VENDOR_RESET_HOURS = 168

const timerExpired = computed(() => {
  const start = props.vendorStatus?.vendor_gold_timer_start
  if (!start) return false
  const startDate = new Date(start + 'Z')
  if (isNaN(startDate.getTime())) return false
  return Date.now() >= startDate.getTime() + VENDOR_RESET_HOURS * 3600 * 1000
})

const resetTimeLabel = computed(() => {
  const start = props.vendorStatus?.vendor_gold_timer_start
  if (!start) return null
  const startDate = new Date(start + 'Z')
  if (isNaN(startDate.getTime())) return null
  const resetAt = new Date(startDate.getTime() + VENDOR_RESET_HOURS * 3600 * 1000)
  const now = new Date()
  const diffMs = resetAt.getTime() - now.getTime()
  if (diffMs <= 0) return null // handled by timerExpired
  const diffH = Math.floor(diffMs / (3600 * 1000))
  const days = Math.floor(diffH / 24)
  const hours = diffH % 24
  if (days > 0) return `${days}d ${hours}h`
  return `${hours}h`
})

const lastSellLabel = computed(() => {
  const ts = props.vendorStatus?.last_sell_at
  if (!ts) return null
  const date = new Date(ts + 'Z')
  if (isNaN(date.getTime())) return null
  const now = new Date()
  const diffMs = now.getTime() - date.getTime()
  if (diffMs < 0) return 'just now'
  const diffMin = Math.floor(diffMs / 60000)
  if (diffMin < 1) return 'just now'
  if (diffMin < 60) return `${diffMin}m ago`
  const diffH = Math.floor(diffMin / 60)
  if (diffH < 24) return `${diffH}h ago`
  const diffD = Math.floor(diffH / 24)
  return `${diffD}d ago`
})

// ── Manual editing ──────────────────────────────────────────────

const editing = ref(false)
const saving = ref(false)
const editGoldAvailable = ref(0)
const editGoldMax = ref(0)
const editResetDays = ref(0)
const editResetHours = ref(0)

function startEditing() {
  editGoldAvailable.value = props.vendorStatus?.vendor_gold_available ?? currentCapGold.value ?? 0
  editGoldMax.value = props.vendorStatus?.vendor_gold_max ?? currentCapGold.value ?? 0

  // Pre-populate timer from current status
  const start = props.vendorStatus?.vendor_gold_timer_start
  if (start && !timerExpired.value) {
    const startDate = new Date(start + 'Z')
    const resetAt = startDate.getTime() + VENDOR_RESET_HOURS * 3600 * 1000
    const remainingMs = Math.max(0, resetAt - Date.now())
    const remainingH = Math.floor(remainingMs / (3600 * 1000))
    editResetDays.value = Math.floor(remainingH / 24)
    editResetHours.value = remainingH % 24
  } else {
    editResetDays.value = 0
    editResetHours.value = 0
  }

  editing.value = true
}

async function saveEdit() {
  const characterName = settingsStore.settings.activeCharacterName
  const serverName = settingsStore.settings.activeServerName
  if (!characterName || !serverName) return

  const totalResetHours = editResetDays.value * 24 + editResetHours.value
  // Only pass reset hours if there's actually time remaining and gold < max
  const resetHoursRemaining = (totalResetHours > 0 && editGoldAvailable.value < editGoldMax.value)
    ? totalResetHours
    : null

  saving.value = true
  try {
    await invoke('set_manual_vendor_gold', {
      characterName,
      serverName,
      npcKey: props.npc.key,
      goldAvailable: editGoldAvailable.value,
      goldMax: editGoldMax.value,
      resetHoursRemaining,
    })
    await gameState.refreshDomain('vendor')
    editing.value = false
  } catch (e) {
    console.error('[NpcVendorSection] Failed to save vendor gold:', e)
  } finally {
    saving.value = false
  }
}
</script>
