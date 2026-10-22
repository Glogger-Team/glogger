<template>
  <div
    class="flex flex-col gap-0 px-1.5 py-0.5 text-xs rounded hover:bg-surface-inset"
    @contextmenu="onContextMenu"
  >
    <div class="flex items-center justify-between">
      <div class="flex items-center gap-1.5 min-w-0 flex-1">
        <NpcInline :reference="entry.npcKey" class="shrink-0" />
        <span v-if="entry.area" class="text-[10px] text-text-dim truncate">{{ entry.area }}</span>
      </div>
      <div class="flex items-center gap-1.5 shrink-0">
        <!-- Untracked vendor: show quick-set actions -->
        <template v-if="entry.goldAvailable == null && viewMode === 'character'">
          <span class="text-text-dim italic">untracked</span>
          <button
            v-if="entry.goldMax != null"
            class="px-1 py-0 text-[10px] text-accent-gold hover:text-accent-gold-bright cursor-pointer"
            title="Set to full cap"
            @click="emit('setToCap', entry)"
          >
            set full
          </button>
          <button
            class="px-1 py-0 text-[10px] text-text-muted hover:text-text-primary cursor-pointer"
            title="Set custom value"
            @click="emit('startQuickEdit', entry)"
          >
            set...
          </button>
        </template>
        <!-- Tracked vendor -->
        <template v-else>
          <span class="font-mono" :class="goldColorClass">
            {{ entry.goldAvailable != null ? entry.goldAvailable.toLocaleString() : '?' }}
          </span>
          <span v-if="entry.goldMax != null" class="text-text-dim">
            / {{ entry.goldMax.toLocaleString() }}
          </span>
          <span v-if="entry.resetLabel" class="text-[10px]" :class="entry.assumedReset ? 'text-value-positive italic' : 'text-text-dim'">
            {{ entry.resetLabel }}
          </span>
          <span v-if="entry.snapshotSpent != null" class="text-[10px] text-text-dim italic">
            (spent {{ entry.snapshotSpent.toLocaleString() }})
          </span>
        </template>
      </div>
    </div>
    <!-- Quick edit inline -->
    <div
      v-if="quickEditKey === entry.npcKey"
      class="flex items-center gap-1.5 pl-3 mt-0.5"
    >
      <input
        :value="quickEditValue"
        type="number"
        min="0"
        class="w-20 px-1.5 py-0.5 bg-surface-base border border-border-default rounded text-[10px] text-text-primary font-mono focus:outline-none focus:border-accent-gold/50"
        @input="emit('update:quickEditValue', Number(($event.target as HTMLInputElement).value))"
        @keydown.enter="emit('saveQuickEdit', entry)"
        @keydown.escape="emit('cancelQuickEdit')"
      />
      <button
        class="px-1 py-0 text-[10px] text-accent-gold hover:text-accent-gold-bright cursor-pointer"
        @click="emit('saveQuickEdit', entry)"
      >
        save
      </button>
      <button
        class="px-1 py-0 text-[10px] text-text-muted hover:text-text-primary cursor-pointer"
        @click="emit('cancelQuickEdit')"
      >
        cancel
      </button>
    </div>
    <!-- Per-character breakdown in aggregate mode -->
    <div
      v-if="viewMode === 'all' && entry.characterBreakdown && entry.characterBreakdown.length > 1"
      class="flex flex-col gap-0 pl-3 mt-0.5"
    >
      <div
        v-for="cb in entry.characterBreakdown"
        :key="cb.characterName"
        class="flex items-center justify-between text-[10px] text-text-dim"
      >
        <span>{{ cb.characterName }}</span>
        <span class="font-mono">
          {{ cb.goldAvailable != null ? cb.goldAvailable.toLocaleString() : '?' }}
          <span v-if="cb.goldMax != null"> / {{ cb.goldMax.toLocaleString() }}</span>
        </span>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import NpcInline from '../../Shared/NPC/NpcInline.vue'

interface VendorEntry {
  npcKey: string
  npcName: string
  area: string | null
  goldAvailable: number | null
  goldMax: number | null
  resetLabel: string | null
  assumedReset: boolean
  itemCategories: string[]
  rawItemTypes: string[]
  characterBreakdown: { characterName: string; goldAvailable: number | null; goldMax: number | null }[] | null
  snapshotSpent: number | null
}

const props = defineProps<{
  entry: VendorEntry
  viewMode: 'character' | 'all'
  quickEditKey: string | null
  quickEditValue: number
}>()

const emit = defineEmits<{
  setToCap: [entry: VendorEntry]
  startQuickEdit: [entry: VendorEntry]
  saveQuickEdit: [entry: VendorEntry]
  cancelQuickEdit: []
  'update:quickEditValue': [value: number]
  openContextMenu: [payload: { entry: VendorEntry; x: number; y: number }]
}>()

// Right-click menu is only meaningful in single-character mode, since edits
// always target the active character (in aggregate mode a row is a sum).
function onContextMenu(e: MouseEvent) {
  if (props.viewMode !== 'character') return
  e.preventDefault()
  emit('openContextMenu', { entry: props.entry, x: e.clientX, y: e.clientY })
}

const goldColorClass = computed(() => {
  const v = props.entry
  if (v.goldAvailable == null) return 'text-text-secondary'
  if (v.assumedReset) return 'text-value-positive'
  if (!v.goldMax || v.goldMax === 0) return 'text-text-secondary'
  const ratio = v.goldAvailable / v.goldMax
  if (ratio >= 0.7) return 'text-value-positive'
  if (ratio >= 0.3) return 'text-yellow-400'
  return 'text-value-negative'
})
</script>
