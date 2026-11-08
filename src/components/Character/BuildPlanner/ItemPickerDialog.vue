<template>
  <Teleport to="body">
    <Transition name="modal">
      <div v-if="show" class="fixed inset-0 z-50 flex items-center justify-center">
        <div class="absolute inset-0 bg-black/60" @click="close" />

        <div class="relative bg-surface-base border border-border-default rounded-lg shadow-xl w-[90vw] max-w-275 h-[85vh] flex flex-col">
          <!-- Header -->
          <div class="flex items-center justify-between px-4 pt-4 pb-2 shrink-0">
            <h3 class="text-sm font-semibold text-text-primary">Select Base Item — {{ slotLabel }}</h3>
            <button
              class="text-text-muted hover:text-text-primary text-lg cursor-pointer leading-none"
              @click="close">
              &times;
            </button>
          </div>

          <!-- Two-panel layout -->
          <div class="flex-1 flex flex-col md:flex-row gap-4 px-4 pb-4 min-h-0 overflow-y-auto md:overflow-hidden">
            <!-- Left: item search/browse -->
            <div class="flex-1 min-w-0 flex flex-col min-h-0">
              <SlotItemPicker
                preview-mode
                @preview="onPreview"
                @selected="onSelected" />
            </div>

            <!-- Right: item preview -->
            <div class="w-full md:w-96 shrink-0 flex flex-col min-h-0 border-l-0 md:border-l border-t md:border-t-0 border-border-default/50 pt-4 md:pt-0 pl-0 md:pl-4 max-h-[45%] md:max-h-none overflow-y-auto">
              <template v-if="previewItem">
                <div class="flex-1 overflow-y-auto">
                  <ItemTooltip :item="previewItem" :icon-src="previewIconSrc" />

                  <!-- Skill requirements -->
                  <div v-if="previewItem.skill_reqs && Object.keys(previewItem.skill_reqs).length > 0" class="mt-2 pt-2 border-t border-border-default/30">
                    <span class="text-[10px] text-text-muted uppercase tracking-wider">Skill Requirements</span>
                    <div class="flex flex-wrap gap-2 mt-1">
                      <span
                        v-for="(level, skill) in previewItem.skill_reqs"
                        :key="String(skill)"
                        class="text-xs text-text-secondary">
                        {{ skill }} {{ level }}
                      </span>
                    </div>
                  </div>

                  <!-- Equip slot / armor type -->
                  <div v-if="previewArmorType || previewItem.equip_slot" class="mt-2 pt-2 border-t border-border-default/30">
                    <div class="flex items-center gap-2 text-xs">
                      <span v-if="previewItem.equip_slot" class="text-text-muted">
                        Slot: <span class="text-text-secondary">{{ previewItem.equip_slot }}</span>
                      </span>
                      <span v-if="previewArmorType" class="px-1.5 py-0.5 rounded text-[10px]" :class="armorTypeBadge(previewArmorType)">
                        {{ previewArmorType }}
                      </span>
                      <span v-if="previewItem.craft_points" class="text-text-muted">
                        {{ previewItem.craft_points }} CP
                      </span>
                    </div>
                  </div>
                </div>

                <!-- Select button -->
                <button
                  class="mt-3 w-full px-4 py-2 text-sm font-medium rounded cursor-pointer transition-colors bg-accent-gold/20 border border-accent-gold/40 text-accent-gold hover:bg-accent-gold/30 shrink-0"
                  @click="confirmSelection">
                  Select {{ previewItem.name }}
                </button>
              </template>

              <div v-else class="flex-1 flex items-center justify-center text-xs text-text-dim">
                Click an item to preview it here.
              </div>
            </div>
          </div>
        </div>
      </div>
    </Transition>
  </Teleport>
</template>

<script setup lang="ts">
import { ref, computed, watch } from 'vue'
import { useOverlayEscape } from '../../../composables/useOverlayEscape'
import { convertFileSrc } from '@tauri-apps/api/core'
import { useBuildPlannerStore } from '../../../stores/buildPlannerStore'
import { useGameDataStore } from '../../../stores/gameDataStore'
import { EQUIPMENT_SLOTS, getArmorTypeFromKeywords } from '../../../types/buildPlanner'
import type { ArmorType } from '../../../types/buildPlanner'
import type { ItemInfo } from '../../../types/gameData'
import SlotItemPicker from './SlotItemPicker.vue'
import ItemTooltip from '../../Shared/Item/ItemTooltip.vue'

const props = defineProps<{
  show: boolean
}>()

const emit = defineEmits<{
  'update:show': [value: boolean]
}>()

const store = useBuildPlannerStore()
const gameData = useGameDataStore()

const previewItem = ref<ItemInfo | null>(null)
const previewIconSrc = ref<string | null>(null)

const slotLabel = computed(() =>
  EQUIPMENT_SLOTS.find(s => s.id === store.selectedSlot)?.label ?? store.selectedSlot ?? ''
)

const previewArmorType = computed((): ArmorType | null => {
  if (!previewItem.value?.keywords) return null
  return getArmorTypeFromKeywords(previewItem.value.keywords)
})

function armorTypeBadge(type: string): string {
  switch (type) {
    case 'Cloth': return 'bg-blue-900/30 text-blue-300'
    case 'Leather': return 'bg-amber-900/30 text-amber-300'
    case 'Metal': return 'bg-slate-600/30 text-slate-300'
    case 'Organic': return 'bg-green-900/30 text-green-300'
    default: return 'bg-surface-hover text-text-dim'
  }
}

async function onPreview(item: ItemInfo) {
  previewItem.value = item
  previewIconSrc.value = null
  if (item.icon_id) {
    try {
      const path = await gameData.getIconPath(item.icon_id)
      previewIconSrc.value = convertFileSrc(path)
    } catch {
      // Icon not available
    }
  }
}

async function confirmSelection() {
  if (!previewItem.value || !store.selectedSlot) return
  await store.setSlotItem(store.selectedSlot, previewItem.value.id, previewItem.value.name)
  close()
}

function onSelected() {
  close()
}

function close() {
  previewItem.value = null
  previewIconSrc.value = null
  emit('update:show', false)
}

// Esc scoping: registers on the shared overlay Esc stack while open, so one
// Esc with stacked overlays closes only the topmost. The stack owns the
// capture-phase window listener (the search input owns focus on open).
// StyledSelect contains its own open-dropdown Esc (stopPropagation), so an
// open dropdown closes first; the dialog closes on the next Esc.
useOverlayEscape(() => props.show, close);

watch(() => store.selectedSlot, () => {
  previewItem.value = null
  previewIconSrc.value = null
})
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
