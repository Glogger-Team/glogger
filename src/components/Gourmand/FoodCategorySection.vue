<template>
  <div class="mb-6">
    <div class="mb-3">
      <div class="flex items-center justify-between">
        <h3 class="text-text-primary font-bold uppercase tracking-wide text-sm">
          {{ title }}
          <span class="text-text-muted font-normal ml-2">{{ eatenCount }} / {{ scopedFoods.length }} eaten</span>
        </h3>
      </div>
      <p v-if="hint" class="text-accent-warning text-xs mt-1">{{ hint }}</p>
    </div>

    <!-- Card view -->
    <div v-if="viewMode === 'card'" class="grid grid-cols-[repeat(auto-fill,minmax(220px,1fr))] gap-1.5">
      <FoodCard
        v-for="food in visibleFoods"
        :key="food.item_id"
        :food="food"
        :eaten="isEaten(food)"
        :count="getCount(food)"
        :manually-marked="isManuallyMarked(food)"
        :selected="isSelected(food)"
        :selectable="selectable"
        :can-eat="canEatFood(food)"
        @select="$emit('select', $event)"
        @toggle="$emit('toggle', $event)"
      />
    </div>

    <!-- List view -->
    <div v-else class="flex flex-col">
      <FoodListRow
        v-for="food in visibleFoods"
        :key="food.item_id"
        :food="food"
        :eaten="isEaten(food)"
        :count="getCount(food)"
        :manually-marked="isManuallyMarked(food)"
        :selected="isSelected(food)"
        :selectable="selectable"
        :can-eat="canEatFood(food)"
        @select="$emit('select', $event)"
        @toggle="$emit('toggle', $event)"
      />
    </div>

    <div v-if="visibleFoods.length === 0" class="text-text-muted text-sm py-4 text-center">
      {{ hideEaten || hideUnusable || sourceFilter !== 'all'
        ? 'All foods hidden by filters.'
        : 'No foods in this category.' }}
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import type { FoodItem } from '../../types/gourmand'
import { foodSourceRank } from '../../types/gourmand'
import type { FoodSourceFilter } from '../../stores/gourmandStore'
import { matchesSourceFilter } from '../../stores/gourmandStore'
import FoodCard from './FoodCard.vue'
import FoodListRow from './FoodListRow.vue'

const props = defineProps<{
  title: string
  /** Optional attention note under the header — used by the Unknown Categories bucket. */
  hint?: string
  foods: FoodItem[]
  eatenFoods: Map<string, number>
  manuallyMarkedFoods: Set<string>
  hideEaten: boolean
  hideUnusable: boolean
  sourceFilter: FoodSourceFilter
  sortMode: 'level' | 'alpha' | 'food-level' | 'source'
  sortAsc: boolean
  viewMode: 'card' | 'list'
  selectable: boolean
  gourmandLevel: number | null
  selectedFood: FoodItem | null
}>()

defineEmits<{
  select: [food: FoodItem]
  toggle: [food: FoodItem]
}>()

// The source filter narrows what this section is *about*, so the header count
// follows it. The hide toggles only decide what's drawn, so they don't.
const scopedFoods = computed(() =>
  props.foods.filter(f => matchesSourceFilter(f, props.sourceFilter)),
)

const eatenCount = computed(() => scopedFoods.value.filter(f => props.eatenFoods.has(f.name)).length)

const visibleFoods = computed(() => {
  let foods = [...scopedFoods.value]

  // Filter
  if (props.hideEaten) {
    foods = foods.filter(f => !props.eatenFoods.has(f.name))
  }
  if (props.hideUnusable) {
    foods = foods.filter(f => canEatFood(f))
  }

  // Sort
  const dir = props.sortAsc ? 1 : -1
  switch (props.sortMode) {
    case 'level':
      foods.sort((a, b) => dir * ((a.gourmand_req ?? 0) - (b.gourmand_req ?? 0)) || a.name.localeCompare(b.name))
      break
    case 'food-level':
      foods.sort((a, b) => dir * (a.food_level - b.food_level) || a.name.localeCompare(b.name))
      break
    case 'alpha':
      foods.sort((a, b) => dir * a.name.localeCompare(b.name))
      break
    case 'source':
      foods.sort((a, b) => dir * (foodSourceRank(a) - foodSourceRank(b)) || a.name.localeCompare(b.name))
      break
  }

  return foods
})

function isEaten(food: FoodItem): boolean {
  return props.eatenFoods.has(food.name)
}

function isManuallyMarked(food: FoodItem): boolean {
  return props.manuallyMarkedFoods.has(food.name)
}

function getCount(food: FoodItem): number {
  return props.eatenFoods.get(food.name) ?? 0
}

function isSelected(food: FoodItem): boolean {
  return props.selectedFood?.item_id === food.item_id
}

function canEatFood(food: FoodItem): boolean {
  if (food.gourmand_req === null || props.gourmandLevel === null) return true
  return props.gourmandLevel >= food.gourmand_req
}
</script>
