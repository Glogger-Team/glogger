<template>
  <Teleport to="body">
    <Transition name="modal">
      <div v-if="show" ref="dialogRef" tabindex="-1" class="fixed inset-0 z-50 flex items-center justify-center outline-none" @keydown.escape="cancel">
        <!-- Backdrop -->
        <div class="absolute inset-0 bg-black/50" @click="cancel" />

        <!-- Dialog -->
        <div class="relative bg-surface-base border border-border-default rounded-lg shadow-xl w-[40rem] max-w-[92vw] flex flex-col max-h-[85vh]">
          <!-- Header -->
          <div class="px-4 pt-4 pb-2 shrink-0">
            <h3 class="text-sm font-semibold text-text-primary">Import Equipped Gear</h3>
            <!-- Work-in-progress warning banner -->
            <div class="mt-2 text-xs text-amber-300/90 bg-amber-500/10 border border-amber-500/30 rounded px-3 py-2 leading-snug">
              <span class="font-semibold">⚠ Work in progress —</span>
              gear import isn't fully automatic yet. The game's report can't tell which item is worn
              when you own several per slot, so imported gear still needs manual fine-tuning.
              Likely-worn items (<span class="text-accent-green">worn</span>/attuned) are pre-selected —
              review each slot before importing, then double-check mods and levels afterward.
            </div>
          </div>

          <!-- Slot rows -->
          <div class="px-4 py-2 overflow-y-auto flex flex-col gap-1.5">
            <div
              v-for="slot in EQUIPMENT_SLOTS"
              :key="slot.id"
              class="grid grid-cols-[5rem_1fr] items-center gap-3">
              <label class="text-xs text-text-secondary text-right">{{ slot.label }}</label>
              <StyledSelect
                v-if="candidatesBySlot[slot.id]?.length"
                :model-value="String(selection[slot.id] ?? -1)"
                :options="optionsForSlot(slot.id)"
                :color-class="colorClassForSlot(slot.id)"
                size="sm"
                full-width
                @update:model-value="v => selection[slot.id] = Number(v)" />
              <span v-else class="text-xs text-text-dim italic px-2 py-1">nothing equippable in report</span>
            </div>
          </div>

          <!-- Footer -->
          <div class="flex items-center justify-between gap-2 px-4 py-3 border-t border-border-default shrink-0">
            <span class="text-xs text-text-muted">{{ selectedCount }} of {{ availableSlotCount }} slots selected</span>
            <div class="flex items-center gap-2">
              <button
                class="px-3 py-1.5 text-xs font-medium rounded cursor-pointer bg-surface-elevated border border-border-default text-text-secondary hover:bg-surface-hover"
                @click="cancel">
                Cancel
              </button>
              <button
                class="px-3 py-1.5 text-xs font-medium rounded cursor-pointer bg-accent-gold/20 border border-accent-gold/40 text-accent-gold hover:bg-accent-gold/30 disabled:opacity-50 disabled:cursor-not-allowed"
                :disabled="selectedCount === 0"
                @click="confirm">
                Import {{ selectedCount || '' }}
              </button>
            </div>
          </div>
        </div>
      </div>
    </Transition>
  </Teleport>
</template>

<script setup lang="ts">
import { ref, computed, watch, nextTick } from 'vue'
import { useOverlayEscape } from '../../../composables/useOverlayEscape'
import { EQUIPMENT_SLOTS, getRarityTextColor } from '../../../types/buildPlanner'
import type { EquippedSlotCandidates, EquippedCandidate, EquippedSelection } from '../../../types/buildPlanner'
import StyledSelect from '../../Shared/StyledSelect.vue'

const props = defineProps<{
  show: boolean
  slots: EquippedSlotCandidates[]
}>()

const emit = defineEmits<{
  'update:show': [value: boolean]
  confirm: [selections: EquippedSelection[]]
}>()

/** slot id → chosen candidate index (-1 = skip) */
const selection = ref<Record<string, number>>({})

const candidatesBySlot = computed<Record<string, EquippedCandidate[]>>(() => {
  const map: Record<string, EquippedCandidate[]> = {}
  for (const s of props.slots) map[s.equip_slot] = s.candidates
  return map
})

let previouslyFocused: HTMLElement | null = null
const dialogRef = ref<HTMLElement | null>(null)

 // When the dialog opens, default every slot with candidates to the best guess
 // (index 0, which the backend ordered worn/attuned/level-first).
 watch(() => props.show, (open) => {
  if (open) {
    previouslyFocused = document.activeElement instanceof HTMLElement ? document.activeElement : null
    nextTick(() => dialogRef.value?.focus())
    const next: Record<string, number> = {}
    for (const s of props.slots) {
      next[s.equip_slot] = s.candidates.length > 0 ? 0 : -1
    }
    selection.value = next
  } else {
    previouslyFocused?.focus()
    previouslyFocused = null
  }
 }, { immediate: true })

function candidateLabel(c: EquippedCandidate): string {
  const tags: string[] = []
  if (c.is_worn) tags.push('worn')
  if (c.is_attuned) tags.push('attuned')
  const modCount = c.mods.filter(m => !m.is_augment).length
  const aug = c.mods.some(m => m.is_augment) ? '+aug' : ''
  const modStr = modCount > 0 ? `${modCount}${aug} mods` : (aug ? aug : 'no mods')
  const suffix = tags.length ? ` · ${tags.join('/')}` : ''
  return `${c.item_name ?? 'Unknown'} — L${c.slot_level} ${c.slot_rarity} · ${modStr}${suffix}`
}

function optionsForSlot(slotId: string) {
  const cands = candidatesBySlot.value[slotId] ?? []
  return [
    { value: '-1', label: '(skip — leave slot unchanged)' },
    ...cands.map((c, i) => ({ value: String(i), label: candidateLabel(c) })),
  ]
}

function colorClassForSlot(slotId: string): string {
  const idx = selection.value[slotId] ?? -1
  const cand = candidatesBySlot.value[slotId]?.[idx]
  return cand ? getRarityTextColor(cand.slot_rarity) : 'text-text-dim'
}

const availableSlotCount = computed(() =>
  props.slots.filter(s => s.candidates.length > 0).length
)

const selectedCount = computed(() =>
  Object.entries(selection.value).filter(([slotId, idx]) =>
    idx >= 0 && (candidatesBySlot.value[slotId]?.length ?? 0) > 0
  ).length
)

function confirm() {
  const selections: EquippedSelection[] = []
  for (const [slotId, idx] of Object.entries(selection.value)) {
    if (idx < 0) continue
    const cand = candidatesBySlot.value[slotId]?.[idx]
    if (cand) selections.push({ equip_slot: slotId, item: cand })
  }
  emit('confirm', selections)
  emit('update:show', false)
}

function cancel() {
  emit('update:show', false)
}

// Esc scoping: registers on the shared overlay Esc stack while open, so one
// Esc with stacked overlays closes only the topmost. The stack owns the
// capture-phase window listener (focus sits inside per-slot StyledSelects).
// StyledSelect contains its own open-dropdown Esc (stopPropagation), so an
// open dropdown closes first; the dialog closes on the next Esc.
useOverlayEscape(() => props.show, cancel);
</script>

<style scoped>
.modal-enter-active,
.modal-leave-active {
  transition: opacity 0.15s ease;
}
.modal-enter-from,
.modal-leave-to {
  opacity: 0;
}
</style>
