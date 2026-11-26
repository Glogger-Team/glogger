<template>
  <div class="flex flex-col gap-3 h-full min-h-0">
    <!-- Slot header -->
    <div class="flex items-center gap-3 px-1 shrink-0">
      <h3 class="dialog-title">{{ slotLabel }}</h3>
      <span
        v-if="isFixedSlot"
        class="text-xs font-semibold px-1.5 py-0.5 rounded bg-surface-hover text-text-muted">
        fixed effects — no mods
      </span>
      <span
        v-else
        class="text-xs font-semibold px-1.5 py-0.5 rounded"
        :class="totalModCount >= store.maxModsPerSlot
          ? 'bg-green-900/30 text-value-positive'
          : totalModCount > 0
            ? 'bg-yellow-900/30 text-yellow-400'
            : 'bg-surface-hover text-text-muted'">
        {{ totalModCount }}/{{ store.maxModsPerSlot }} mods
      </span>
      <span
        v-if="store.slotHasAugment[store.selectedSlot!]"
        class="text-xs font-semibold px-1.5 py-0.5 rounded bg-purple-900/30 text-mod-augment">
        +1 augment
      </span>
    </div>

    <!-- Per-slot skill overrides -->
    <div v-if="!isFixedSlot" class="flex items-center gap-2 px-1 shrink-0">
      <label class="text-[10px] text-text-muted uppercase tracking-wider shrink-0">Item Skills:</label>
      <StyledSelect
        :model-value="store.getSlotSkillPrimary(store.selectedSlot!) ?? ''"
        :options="slotSkillPrimaryOptions"
        size="xs"
        color-class="text-blue-400"
        @update:model-value="onSlotSkillPrimaryChange" />
      <StyledSelect
        :model-value="store.getSlotSkillSecondary(store.selectedSlot!) ?? ''"
        :options="slotSkillSecondaryOptions"
        size="xs"
        color-class="text-emerald-400"
        @update:model-value="onSlotSkillSecondaryChange" />
    </div>

    <!-- Main content: Left (item card + mod slots) + Right (tabbed mods/CP) -->
    <div class="flex-1 flex gap-4 min-h-0">
      <!-- Left side: item card + mod slots -->
      <div class="w-2/5 min-w-60 min-h-0 flex flex-col gap-2">
        <!-- Item card (filled) -->
        <div v-if="resolvedItem" class="rounded border border-border-default bg-surface-elevated px-2.5 py-2 shrink-0">
          <div class="flex gap-2.5">
            <!-- Icon spanning rows -->
            <GameIcon v-if="resolvedItem.icon_id" :icon-id="resolvedItem.icon_id" :alt="resolvedItem.name" size="lg" class="shrink-0" />

            <div class="flex-1 min-w-0 flex flex-col gap-1">
              <!-- Row 1: Name + Level & Value -->
              <div class="flex items-baseline justify-between gap-2">
                <span class="text-sm font-semibold text-entity-item truncate">{{ resolvedItem.name }}</span>
                <div class="flex items-center gap-2 shrink-0 text-[10px] text-text-dim">
                  <div v-if="!isFixedSlot" class="flex items-center gap-0.5">
                    <span>Lv</span>
                    <input
                      type="number"
                      :value="store.getSlotLevel(store.selectedSlot!)"
                      min="1"
                      max="125"
                      class="bg-transparent border-b border-border-default/50 focus:border-accent-gold/50 outline-none px-0.5 py-0 text-[10px] text-text-primary w-8 text-center"
                      @change="onSlotLevelChange" />
                  </div>
                  <span v-if="resolvedItem.value" class="text-accent-gold">{{ resolvedItem.value }}g</span>
                </div>
              </div>

              <!-- Row 2: Rarity + Crafted + MW/Foretold -->
              <div v-if="!isFixedSlot" class="flex items-center justify-between gap-2">
                <StyledSelect
                  :model-value="store.getSlotRarity(store.selectedSlot!)"
                  :options="rarityOptions"
                  size="xs"
                  :color-class="getRarityTextColor(store.getSlotRarity(store.selectedSlot!))"
                  @update:model-value="onSlotRarityChange" />
                <div class="flex items-center gap-3 text-[10px]">
                  <label class="flex items-center gap-1 text-text-dim cursor-pointer">
                    <input type="checkbox" :checked="slotItem?.is_crafted ?? false" class="w-2.5 h-2.5 cursor-pointer" @change="onCraftedChange" />
                    Crafted
                  </label>
                  <label
                    v-if="store.getSlotRarity(store.selectedSlot!) === 'Legendary'"
                    class="flex items-center gap-1 text-text-dim cursor-pointer">
                    <input type="checkbox" :checked="slotItem?.is_masterwork ?? false" class="w-2.5 h-2.5 cursor-pointer" @change="onMasterworkChange" />
                    Masterwork / Foretold
                  </label>
                </div>
              </div>

              <!-- Row 3: Effects -->
              <div v-if="itemEffects.length > 0">
                <div
                  v-for="(effect, i) in itemEffects"
                  :key="i"
                  class="text-accent-green text-[11px] leading-snug">
                  {{ effect }}
                </div>
              </div>

              <!-- Row 4: Change / Clear -->
              <div class="flex items-center justify-end gap-1 mt-0.5">
                <button
                  class="text-[10px] text-text-muted hover:text-text-primary cursor-pointer px-1.5 py-0.5 rounded border border-border-default hover:border-accent-gold/30 transition-colors"
                  @click="showItemPicker = true">
                  Change
                </button>
                <button
                  class="text-[10px] text-red-400/60 hover:text-red-400 cursor-pointer px-1.5 py-0.5 rounded border border-red-700/20 hover:border-red-700/40 transition-colors"
                  @click="store.clearSlotItem()">
                  Clear
                </button>
              </div>
            </div>
          </div>
        </div>

        <!-- Item card (empty placeholder) -->
        <button
          v-else
          class="flex items-center gap-2 px-3 py-3 rounded border border-dashed border-border-default/50 bg-surface-dark text-left cursor-pointer hover:border-accent-gold/40 transition-colors shrink-0"
          @click="showItemPicker = true">
          <span class="text-xs text-text-dim">Select Base Item...</span>
        </button>

        <!-- Mod slots -->
        <div v-if="!isFixedSlot" class="flex-1 min-h-0 overflow-y-auto">
          <ModSlotList />
        </div>
      </div>

      <!-- Right side: fixed-effect slots browse items directly (no mods/CP) -->
      <div v-if="isFixedSlot" class="flex-1 min-w-0 min-h-0 border-l border-border-default/50 pl-3 flex flex-col">
        <SlotItemPicker />
      </div>

      <!-- Right side: tabbed Mods / Craft Points -->
      <div v-else class="flex-1 min-w-0 min-h-0 border-l border-border-default/50 pl-3 flex flex-col">
        <div class="flex items-center gap-0 shrink-0 border-b border-border-default/50 mb-2">
          <button
            class="px-3 py-1.5 text-xs font-medium cursor-pointer transition-colors border-b-2"
            :class="activeTab === 'mods'
              ? 'text-text-primary border-accent-gold'
              : 'text-text-muted border-transparent hover:text-text-secondary'"
            @click="activeTab = 'mods'">
            Mods
          </button>
          <button
            class="px-3 py-1.5 text-xs font-medium cursor-pointer transition-colors border-b-2"
            :class="activeTab === 'cp'
              ? 'text-text-primary border-accent-gold'
              : 'text-text-muted border-transparent hover:text-text-secondary'"
            @click="activeTab = 'cp'">
            Craft Points
          </button>
        </div>
        <div class="flex-1 min-h-0">
          <ModBrowser v-show="activeTab === 'mods'" />
          <CpRecipePanel v-show="activeTab === 'cp'" />
        </div>
      </div>
    </div>

    <!-- Item Picker Dialog -->
    <ItemPickerDialog
      :show="showItemPicker"
      @update:show="showItemPicker = $event" />
  </div>
</template>

<script setup lang="ts">
import { ref, computed, watch } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { useBuildPlannerStore } from '../../../stores/buildPlannerStore'
import { EQUIPMENT_SLOTS, getAllowedRarities, getRarityTextColor } from '../../../types/buildPlanner'
import type { ItemInfo } from '../../../types/gameData'
import StyledSelect from '../../Shared/StyledSelect.vue'
import GameIcon from '../../Shared/GameIcon.vue'
import ModSlotList from './ModSlotList.vue'
import ModBrowser from './ModBrowser.vue'
import CpRecipePanel from './CpRecipePanel.vue'
import ItemPickerDialog from './ItemPickerDialog.vue'
import SlotItemPicker from './SlotItemPicker.vue'

const store = useBuildPlannerStore()
const showItemPicker = ref(false)
const activeTab = ref<'mods' | 'cp'>('mods')
const itemEffects = ref<string[]>([])

// Reset when slot changes
watch(() => store.selectedSlot, () => {
  showItemPicker.value = false
})

const slotDef = computed(() =>
  EQUIPMENT_SLOTS.find(s => s.id === store.selectedSlot)
)

const slotLabel = computed(() => slotDef.value?.label ?? store.selectedSlot ?? '')

/** Fixed-effect slot (Racial): item pick only — no mods, rarity, level, or CP. */
const isFixedSlot = computed(() => slotDef.value?.noMods === true)

const resolvedItem = computed((): ItemInfo | null => {
  if (!store.selectedSlot) return null
  return store.resolvedSlotItems[store.selectedSlot] ?? null
})

// Resolve item effects when item changes
watch(resolvedItem, async (item) => {
  itemEffects.value = []
  if (!item?.effect_descs?.length) return
  try {
    const resolved = await invoke<Array<{ formatted: string }>>('resolve_effect_descs', {
      descs: item.effect_descs,
    })
    itemEffects.value = resolved.map(e => e.formatted)
  } catch {
    itemEffects.value = item.effect_descs
  }
}, { immediate: true })

const totalModCount = computed(() => store.slotModCounts[store.selectedSlot!] ?? 0)

const slotItem = computed(() => {
  if (!store.selectedSlot) return null
  return store.getSlotItem(store.selectedSlot)
})



const rarityOptions = computed(() => {
  if (!slotDef.value) return []
  return getAllowedRarities(slotDef.value).map(r => ({ value: r.id, label: r.label }))
})

const slotSkillPrimaryOptions = computed(() => [
  { value: '', label: store.activePreset?.skill_primary ? `${store.activePreset.skill_primary} (default)` : 'None' },
  ...store.combatSkills.map(s => ({ value: s.name, label: s.name })),
])

const slotSkillSecondaryOptions = computed(() => [
  { value: '', label: store.activePreset?.skill_secondary ? `${store.activePreset.skill_secondary} (default)` : 'None' },
  ...store.combatSkills.map(s => ({ value: s.name, label: s.name })),
])

async function onSlotRarityChange(val: string) {
  if (!store.selectedSlot || !slotDef.value) return
  const allowed = getAllowedRarities(slotDef.value).map(r => r.id)
  if (!allowed.includes(val)) return
  if (val !== 'Legendary' && slotItem.value?.is_masterwork) {
    await store.updateSlotProps(store.selectedSlot, { slot_rarity: val, is_masterwork: false })
  } else {
    await store.updateSlotProps(store.selectedSlot, { slot_rarity: val })
  }
}

async function onSlotLevelChange(e: Event) {
  if (!store.selectedSlot) return
  const val = Number((e.target as HTMLInputElement).value)
  if (val >= 1 && val <= 125) {
    await store.updateSlotProps(store.selectedSlot, { slot_level: val })
  }
}

async function onCraftedChange(e: Event) {
  if (!store.selectedSlot) return
  const checked = (e.target as HTMLInputElement).checked
  await store.updateSlotProps(store.selectedSlot, { is_crafted: checked })
}

async function onMasterworkChange(e: Event) {
  if (!store.selectedSlot) return
  const checked = (e.target as HTMLInputElement).checked
  await store.updateSlotProps(store.selectedSlot, { is_masterwork: checked })
}

async function onSlotSkillPrimaryChange(val: string) {
  if (!store.selectedSlot) return
  await store.updateSlotProps(store.selectedSlot, { slot_skill_primary: val || null })
}

async function onSlotSkillSecondaryChange(val: string) {
  if (!store.selectedSlot) return
  await store.updateSlotProps(store.selectedSlot, { slot_skill_secondary: val || null })
}
</script>
