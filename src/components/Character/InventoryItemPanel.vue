<template>
  <div class="grid grid-cols-[repeat(auto-fill,minmax(12rem,1fr))] gap-2 [&>div]:flex [&>div]:w-full">
    <EntityTooltipWrapper
      v-for="item in items"
      :key="item.id"
      border-class="border-entity-item/50"
      @hover="() => loadItem(item)"
    >
      <button
        class="w-full flex items-center gap-2 bg-surface-card border border-border-default rounded px-2 py-1.5 cursor-pointer hover:bg-surface-elevated hover:border-border-hover transition-colors"
        @click="handleClick(item)"
      >
        <div class="w-8 h-8 shrink-0 rounded-sm bg-black/30 border border-border-light flex items-center justify-center">
          <img
            v-if="iconCache[item.type_id]"
            :src="iconCache[item.type_id]"
            :alt="item.item_name"
            class="w-7 h-7 object-contain" />
          <span v-else class="text-[10px] text-text-muted">?</span>
        </div>
        <div class="flex flex-col items-start min-w-0 gap-0.5">
          <span class="text-entity-item text-xs font-medium truncate w-full">{{ item.item_name }}</span>
          <div class="flex gap-2 text-[0.65rem] text-text-muted">
            <span v-if="item.stack_size > 1">x{{ item.stack_size }}</span>
            <span v-if="item.rarity && item.rarity !== 'Common'" :class="rarityClass(item.rarity)">{{ item.rarity }}</span>
          </div>
        </div>
      </button>
      <template #tooltip>
        <ItemTooltip v-if="itemDataCache[item.type_id]" :item="itemDataCache[item.type_id]!" :icon-src="iconCache[item.type_id] ?? null" />
        <div v-else class="text-text-primary text-xs">{{ item.item_name }}</div>
      </template>
    </EntityTooltipWrapper>
  </div>
</template>

<script setup lang="ts">
import { reactive, watch } from 'vue'
import { convertFileSrc } from '@tauri-apps/api/core'
import { useGameDataStore } from '../../stores/gameDataStore'
import { useEntityNavigation } from '../../composables/useEntityNavigation'
import type { SnapshotItem } from '../../types/database'
import type { ItemInfo } from '../../types/gameData'
import EntityTooltipWrapper from '../Shared/EntityTooltipWrapper.vue'
import ItemTooltip from '../Shared/Item/ItemTooltip.vue'

const props = defineProps<{ items: SnapshotItem[] }>()

const store = useGameDataStore()
const { navigateToEntity } = useEntityNavigation()

const iconCache = reactive<Record<number, string>>({})
const itemDataCache = reactive<Record<number, ItemInfo>>({})

async function loadItem(item: SnapshotItem) {
  if (itemDataCache[item.type_id]) return
  try {
    const data = await store.resolveItem(item.type_id)
    if (!data) return
    itemDataCache[item.type_id] = data
    if (data.icon_id) {
      const path = await store.getIconPath(data.icon_id)
      iconCache[item.type_id] = convertFileSrc(path)
    }
  } catch { /* skip */ }
}

function handleClick(item: SnapshotItem) {
  navigateToEntity({ type: 'item', id: item.item_name })
}

// Eagerly load icons for visible items
async function preloadIcons(items: SnapshotItem[]) {
  const seen = new Set<number>()
  for (const item of items) {
    if (seen.has(item.type_id) || iconCache[item.type_id]) continue
    seen.add(item.type_id)
    loadItem(item)
  }
}

watch(() => props.items, (items) => preloadIcons(items), { immediate: true })

function rarityClass(rarity: string): string {
  switch (rarity) {
    case 'Uncommon': return 'text-rarity-uncommon'
    case 'Rare': return 'text-rarity-rare'
    case 'Exceptional': return 'text-rarity-exceptional'
    case 'Epic': return 'text-rarity-epic'
    case 'Legendary': return 'text-rarity-legendary'
    default: return 'text-text-secondary'
  }
}
</script>

