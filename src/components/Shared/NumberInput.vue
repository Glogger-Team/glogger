<template>
  <div class="inline-flex items-center rounded border border-border-default bg-surface-elevated overflow-hidden">
    <button
      type="button"
      class="flex items-center justify-center shrink-0 cursor-pointer transition-colors text-text-muted hover:text-text-primary hover:bg-surface-hover border-r border-border-default disabled:opacity-30 disabled:cursor-not-allowed"
      :class="buttonClasses"
      :disabled="disabled || atMin"
      @click="decrement">
      <span :class="iconClasses" class="font-bold leading-none select-none">&minus;</span>
    </button>

    <input
      type="number"
      class="bg-transparent text-text-primary text-center border-none outline-none appearance-none [&::-webkit-inner-spin-button]:appearance-none [&::-webkit-outer-spin-button]:appearance-none [-moz-appearance:textfield] min-w-0"
      :class="inputClasses"
      :value="modelValue"
      :min="min"
      :max="max"
      :step="step"
      :disabled="disabled"
      :placeholder="placeholder"
      @input="onInput"
      @blur="onBlur"
      @keydown.up.prevent="increment"
      @keydown.down.prevent="decrement" />

    <button
      type="button"
      class="flex items-center justify-center shrink-0 cursor-pointer transition-colors text-text-muted hover:text-text-primary hover:bg-surface-hover border-l border-border-default disabled:opacity-30 disabled:cursor-not-allowed"
      :class="buttonClasses"
      :disabled="disabled || atMax"
      @click="increment">
      <span :class="iconClasses" class="font-bold leading-none select-none">+</span>
    </button>
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue'

const props = withDefaults(defineProps<{
  modelValue: number
  min?: number
  max?: number
  step?: number
  size?: 'sm' | 'md' | 'lg'
  disabled?: boolean
  placeholder?: string
}>(), {
  min: 0,
  max: Infinity,
  step: 1,
  size: 'md',
  disabled: false,
  placeholder: '',
})

const emit = defineEmits<{
  'update:modelValue': [value: number]
  'change': [value: number]
}>()

const atMin = computed(() => props.modelValue <= props.min)
const atMax = computed(() => props.modelValue >= props.max)

const buttonClasses = computed(() => {
  switch (props.size) {
    case 'sm': return 'w-6 h-6'
    case 'lg': return 'w-10 h-10'
    default: return 'w-8 h-8'
  }
})

const iconClasses = computed(() => {
  switch (props.size) {
    case 'sm': return 'text-sm'
    case 'lg': return 'text-xl'
    default: return 'text-base'
  }
})

const inputClasses = computed(() => {
  switch (props.size) {
    case 'sm': return 'w-10 text-xs py-0.5'
    case 'lg': return 'w-20 text-sm py-1.5'
    default: return 'w-14 text-xs py-1'
  }
})

function clamp(value: number): number {
  return Math.min(props.max, Math.max(props.min, value))
}

function setValue(value: number) {
  const clamped = clamp(value)
  emit('update:modelValue', clamped)
  emit('change', clamped)
}

function increment() {
  if (!props.disabled && !atMax.value) {
    setValue(props.modelValue + props.step)
  }
}

function decrement() {
  if (!props.disabled && !atMin.value) {
    setValue(props.modelValue - props.step)
  }
}

function onInput(event: Event) {
  const target = event.target as HTMLInputElement
  const raw = parseFloat(target.value)
  if (!isNaN(raw)) {
    emit('update:modelValue', raw)
  }
}

function onBlur() {
  // Clamp and emit on blur to allow free typing
  setValue(props.modelValue)
}
</script>
