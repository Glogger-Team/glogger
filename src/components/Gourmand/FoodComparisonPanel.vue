<template>
  <div v-if="selectedMeal || selectedSnack" class="bg-surface-card border border-border-default rounded p-3">
    <div class="flex items-center gap-3 mb-2">
      <h3 class="display-title">Food Buff</h3>
      <div class="flex items-center gap-2 text-xs text-text-muted">
        <span v-if="selectedMeal" class="text-accent-green">{{ selectedMeal.name }}</span>
        <span v-if="selectedMeal && selectedSnack">+</span>
        <span v-if="selectedSnack" class="text-accent-blue">{{ selectedSnack.name }}</span>
      </div>
      <button
        class="ml-auto px-2 py-0.5 text-xs bg-surface-elevated border border-border-default rounded text-text-secondary hover:text-text-primary hover:border-border-hover transition-all"
        @click="$emit('clear')"
      >
        clear
      </button>
    </div>

    <div class="grid grid-cols-2 gap-x-4 gap-y-0.5 text-xs">
      <div
        v-for="effect in combinedEffects"
        :key="effect.label"
        class="flex justify-between"
      >
        <span class="text-text-secondary">{{ effect.label }}</span>
        <span class="text-accent-green font-medium">{{ effect.display }}</span>
      </div>
      <div
        v-for="text in textEffects"
        :key="text"
        class="text-text-muted italic"
      >
        {{ text }}
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import type { FoodItem } from '../../types/gourmand'

interface ResolvedEffect {
  label: string
  value: string
  display_type: string
  formatted: string
  icon_id: number | null
}

const props = defineProps<{
  selectedMeal: FoodItem | null
  selectedSnack: FoodItem | null
}>()

defineEmits<{
  clear: []
}>()

const resolvedEffects = ref<ResolvedEffect[]>([])

const allDescs = computed(() => [
  ...(props.selectedMeal?.effect_descs ?? []),
  ...(props.selectedSnack?.effect_descs ?? []),
])

watch(allDescs, async (descs) => {
  if (!descs.length) {
    resolvedEffects.value = []
    return
  }
  try {
    resolvedEffects.value = await invoke<ResolvedEffect[]>('resolve_effect_descs', { descs })
  } catch {
    resolvedEffects.value = descs.map(d => ({
      label: d, value: '', display_type: '', formatted: d, icon_id: null,
    }))
  }
}, { immediate: true })

const combinedEffects = computed(() => {
  const merged = new Map<string, { value: number; formatted: string }>()
  const textList: string[] = []

  for (const e of resolvedEffects.value) {
    if (e.value) {
      const numVal = parseFloat(e.value)
      const existing = merged.get(e.label)
      if (existing) {
        existing.value += numVal
      } else {
        merged.set(e.label, { value: numVal, formatted: e.formatted })
      }
    } else {
      textList.push(e.formatted)
    }
  }

  return Array.from(merged.entries()).map(([label, { value }]) => ({
    label,
    display: value > 0
      ? `+${Number.isInteger(value) ? value : value.toFixed(1)}`
      : Number.isInteger(value) ? value.toString() : value.toFixed(1),
  }))
})

const textEffects = computed(() => {
  const texts: string[] = []
  for (const e of resolvedEffects.value) {
    if (!e.value) texts.push(e.formatted)
  }
  return [...new Set(texts)]
})
</script>
