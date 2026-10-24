<template>
  <div class="flex flex-col gap-0 px-1.5 py-0.5 text-xs rounded hover:bg-surface-inset">
    <div class="flex items-center justify-between">
      <div class="flex items-center gap-1.5 min-w-0 flex-1">
        <NpcInline :reference="entry.npcKey" class="shrink-0" />
        <span v-if="entry.area" class="text-[10px] text-text-dim truncate">{{ entry.area }}</span>
        <span v-if="otherCategories.length > 0" class="text-[10px] text-text-dim truncate" :title="'Also in: ' + otherCategories.join(', ')">
          · {{ otherCategories.join(' · ') }}
        </span>
      </div>
      <div class="flex items-center gap-1.5 shrink-0">
        <span class="font-mono text-accent-gold">
          {{ entry.earnedCurrent.toLocaleString() }}
        </span>
        <span class="text-text-dim">
          / {{ entry.earnedLifetime.toLocaleString() }}
        </span>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import NpcInline from '../../Shared/NPC/NpcInline.vue'

interface EarnedVendorEntry {
  npcKey: string
  npcName: string
  area: string | null
  earnedCurrent: number
  earnedLifetime: number
  itemCategories: string[]
  rawItemTypes: string[]
}

const props = defineProps<{
  entry: EarnedVendorEntry
  /** Bucket the row is rendered under — hidden from the "also in" hint. */
  currentCategory?: string | null
}>()

const otherCategories = computed(() =>
  props.entry.itemCategories.filter(c => c !== props.currentCategory),
)
</script>
