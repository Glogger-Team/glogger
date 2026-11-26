<template>
  <div class="flex flex-col gap-3 text-sm">
    <SkeletonLoader v-if="loading" variant="text" :lines="4" />

    <EmptyState
      v-else-if="recipeItems.length === 0"
      variant="compact"
      primary="No recipe items found."
      secondary="Recipe scrolls and skill books will appear here when detected in your inventory." />

    <template v-else>
      <!-- Known duplicates (safe to sell/trade) -->
      <div v-if="knownDuplicates.length > 0">
        <h3 class="section-heading mb-1.5">
          Already Known ({{ knownDuplicates.length }})
        </h3>
        <div class="flex flex-col gap-0.5 overflow-y-auto max-h-40 pr-1">
          <div
            v-for="item in knownDuplicates"
            :key="item.item_id"
            class="flex items-center gap-2 py-1 px-2 rounded text-xs hover:bg-surface-elevated/50">
            <ItemInline :reference="item.item_name" />
            <span v-if="item.stack_size > 1" class="text-text-muted font-mono shrink-0">
              x{{ item.stack_size }}
            </span>
            <span class="ml-auto text-green-400 text-xs shrink-0">safe to sell</span>
          </div>
        </div>
      </div>

      <!-- Ready to learn (meets requirements, not yet known) -->
      <div v-if="readyToLearn.length > 0">
        <h3 class="section-heading mb-1.5">
          Ready to Learn ({{ readyToLearn.length }})
        </h3>
        <div class="flex flex-col gap-0.5 overflow-y-auto max-h-40 pr-1">
          <div
            v-for="item in readyToLearn"
            :key="item.item_id"
            class="flex items-center gap-2 py-1 px-2 rounded text-xs hover:bg-surface-elevated/50">
            <ItemInline :reference="item.item_name" />
            <span class="ml-auto text-accent-blue text-xs shrink-0">can learn</span>
          </div>
        </div>
      </div>

      <!-- Not yet learnable (missing skill requirements) -->
      <div v-if="notYetLearnable.length > 0">
        <h3 class="section-heading mb-1.5">
          Missing Requirements ({{ notYetLearnable.length }})
        </h3>
        <div class="flex flex-col gap-0.5 overflow-y-auto max-h-40 pr-1">
          <div
            v-for="item in notYetLearnable"
            :key="item.item_id"
            class="flex flex-col gap-0.5 py-1 px-2 rounded text-xs hover:bg-surface-elevated/50">
            <div class="flex items-center gap-2">
              <ItemInline :reference="item.item_name" />
              <span class="ml-auto text-text-muted text-xs shrink-0">needs skills</span>
            </div>
            <div class="text-text-dim pl-4">
              <span v-for="(req, i) in item.unmet_requirements" :key="i">
                {{ req.skill_name }} {{ req.current }}/{{ req.required }}<span v-if="i < item.unmet_requirements.length - 1">, </span>
              </span>
            </div>
          </div>
        </div>
      </div>

      <!-- Summary -->
      <div class="text-xs text-text-dim border-t border-border-default pt-2">
        {{ recipeItems.length }} recipe item{{ recipeItems.length !== 1 ? 's' : '' }} in inventory
      </div>
    </template>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted, watch } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { useSettingsStore } from '../../../stores/settingsStore'
import { useGameStateStore } from '../../../stores/gameStateStore'
import EmptyState from '../../Shared/EmptyState.vue'
import SkeletonLoader from '../../Shared/SkeletonLoader.vue'
import ItemInline from '../../Shared/Item/ItemInline.vue'

interface UnmetRequirement {
  skill_name: string
  required: number
  current: number
}

interface RecipeItemMatch {
  item_id: number
  item_name: string
  icon_id: number | null
  stack_size: number
  bestow_recipe_keys: string[]
  bestow_recipe_names: string[]
  all_known: boolean
  meets_requirements: boolean
  unmet_requirements: UnmetRequirement[]
}

const settings = useSettingsStore()
const gameState = useGameStateStore()
const recipeItems = ref<RecipeItemMatch[]>([])
const loading = ref(false)

const knownDuplicates = computed(() => recipeItems.value.filter(i => i.all_known))
const readyToLearn = computed(() => recipeItems.value.filter(i => !i.all_known && i.meets_requirements))
const notYetLearnable = computed(() => recipeItems.value.filter(i => !i.all_known && !i.meets_requirements))

async function load(showLoading = true) {
  const characterName = settings.settings.activeCharacterName
  const serverName = settings.settings.activeServerName
  if (!characterName || !serverName) return

  if (showLoading) loading.value = true
  try {
    recipeItems.value = await invoke<RecipeItemMatch[]>('find_recipe_items_in_inventory', {
      characterName,
      serverName,
    })
  } catch (e) {
    console.error('[RecipeItemsWidget] Failed to load:', e)
  } finally {
    loading.value = false
  }
}

onMounted(load)

// Debounced reload when inventory item names change — avoids flashing on every
// single item event.  We watch a lightweight fingerprint (sorted item name list)
// instead of deep-watching the entire inventory/storage arrays.
const inventoryFingerprint = computed(() =>
  Object.keys(gameState.inventoryItemCounts).sort().join('\0')
)
const storageFingerprint = computed(() =>
  gameState.storage.map(s => `${s.item_name}:${s.stack_size}`).sort().join('\0')
)

let reloadTimer: ReturnType<typeof setTimeout> | undefined
function scheduleReload() {
  clearTimeout(reloadTimer)
  reloadTimer = setTimeout(() => load(false), 500)
}

watch(inventoryFingerprint, scheduleReload)
watch(storageFingerprint, scheduleReload)

onUnmounted(() => clearTimeout(reloadTimer))
</script>
