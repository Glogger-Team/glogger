<template>
  <div
    ref="containerRef"
    class="relative inline-flex"
    :class="sizeClasses.container">
    <button
      type="button"
      class="flex items-center gap-1.5 rounded border cursor-pointer transition-colors text-left min-w-0"
      :class="[sizeClasses.button, buttonColorClass, fullWidth ? 'w-full' : '']"
      @click="toggle"
      @keydown.escape="close"
      @keydown.enter.prevent="toggle"
      @keydown.space.prevent="toggle"
      @keydown.up.prevent="selectPrev"
      @keydown.down.prevent="selectNext">
      <span class="truncate flex-1">{{ displayLabel }}</span>
      <svg
        class="shrink-0 transition-transform"
        :class="[open ? 'rotate-180' : '', sizeClasses.chevron]"
        viewBox="0 0 12 12"
        fill="currentColor">
        <path d="M2 4l4 4 4-4z" />
      </svg>
    </button>

    <Teleport to="body">
      <div
        v-if="open"
        class="fixed inset-0 z-50"
        @click="close" />
      <div
        v-if="open"
        ref="dropdownRef"
        class="fixed z-50 rounded border border-border-default bg-surface-elevated shadow-lg flex flex-col"
        :style="dropdownStyle"
        :class="sizeClasses.dropdown">
        <input
          v-if="showSearch"
          ref="searchRef"
          v-model="searchQuery"
          type="text"
          :placeholder="searchPlaceholder"
          class="shrink-0 w-full bg-surface border-b border-border-default text-text-primary placeholder:text-text-muted focus:outline-none"
          :class="sizeClasses.option"
          @keydown.enter.prevent="selectFirstFiltered"
          @keydown.escape.prevent="close"
          @keydown.down.prevent="focusFirstOption" />
        <div class="overflow-y-auto min-h-0">
          <button
            v-for="option in filteredOptions"
            :key="option.value"
            type="button"
            class="w-full text-left cursor-pointer transition-colors truncate"
            :class="[
              sizeClasses.option,
              option.value === modelValue
                ? 'bg-accent-gold/15 text-accent-gold font-medium'
                : 'text-text-primary hover:bg-surface-hover',
            ]"
            @click="select(option.value)">
            {{ option.label }}
          </button>
          <div
            v-if="showSearch && filteredOptions.length === 0"
            class="text-text-muted"
            :class="sizeClasses.option">
            No matches
          </div>
        </div>
      </div>
    </Teleport>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, nextTick, onBeforeUnmount } from 'vue'

export interface SelectOption {
  value: string
  label: string
}

const props = withDefaults(defineProps<{
  options: SelectOption[]
  modelValue: string
  placeholder?: string
  size?: 'xs' | 'sm' | 'md'
  colorClass?: string
  fullWidth?: boolean
  /**
   * Type-to-search filter box atop the dropdown.
   * - 'auto' (default): shown automatically once the list is long enough
   *   (see AUTO_SEARCH_THRESHOLD) so every big dropdown app-wide gets it for free.
   * - true / false: force it on or off regardless of list length.
   */
  searchable?: boolean | 'auto'
  searchPlaceholder?: string
}>(), {
  placeholder: 'Select...',
  size: 'sm',
  colorClass: '',
  fullWidth: false,
  searchable: 'auto',
  searchPlaceholder: 'Type to search...',
})

const emit = defineEmits<{
  'update:modelValue': [value: string]
}>()

// Lists at or above this many options auto-gain a search box when searchable === 'auto'.
const AUTO_SEARCH_THRESHOLD = 8

const containerRef = ref<HTMLElement>()
const dropdownRef = ref<HTMLElement>()
const searchRef = ref<HTMLInputElement>()
const open = ref(false)
const searchQuery = ref('')
const dropdownStyle = ref<Record<string, string>>({})

const showSearch = computed(() => {
  if (props.searchable === 'auto') return props.options.length >= AUTO_SEARCH_THRESHOLD
  return props.searchable
})

const filteredOptions = computed(() => {
  if (!showSearch.value) return props.options
  const q = searchQuery.value.trim().toLowerCase()
  if (!q) return props.options
  return props.options.filter(o => o.label.toLowerCase().includes(q))
})

const sizeClasses = computed(() => {
  switch (props.size) {
    case 'xs': return {
      container: '',
      button: 'px-1.5 py-0.5 text-xs',
      chevron: 'w-2.5 h-2.5',
      dropdown: 'max-h-48',
      option: 'px-2 py-1 text-xs',
    }
    case 'md': return {
      container: '',
      button: 'px-3 py-1.5 text-sm',
      chevron: 'w-3 h-3',
      dropdown: 'max-h-64',
      option: 'px-3 py-1.5 text-sm',
    }
    default: return {
      container: '',
      button: 'px-2 py-1 text-xs',
      chevron: 'w-2.5 h-2.5',
      dropdown: 'max-h-56',
      option: 'px-2.5 py-1 text-xs',
    }
  }
})

const buttonColorClass = computed(() => {
  if (props.colorClass) {
    return `bg-surface-elevated border-border-default ${props.colorClass}`
  }
  return 'bg-surface-elevated border-border-default text-text-primary hover:border-border-default/80'
})

const displayLabel = computed(() => {
  const selected = props.options.find(o => o.value === props.modelValue)
  return selected?.label ?? props.placeholder
})

function positionDropdown() {
  if (!containerRef.value) return
  const rect = containerRef.value.getBoundingClientRect()
  const spaceBelow = window.innerHeight - rect.bottom
  const openAbove = spaceBelow < 200 && rect.top > spaceBelow

  dropdownStyle.value = {
    left: `${Math.max(4, Math.min(rect.left, window.innerWidth - Math.max(rect.width, 160) - 4))}px`,
    width: `${Math.max(rect.width, 160)}px`,
    ...(openAbove
      ? { bottom: `${window.innerHeight - rect.top + 2}px` }
      : { top: `${rect.bottom + 2}px` }),
  }
}

function toggle() {
  if (open.value) {
    close()
  } else {
    open.value = true
    searchQuery.value = ''
    nextTick(() => {
      positionDropdown()
      if (showSearch.value) searchRef.value?.focus()
    })
  }
}

function close() {
  open.value = false
}

function selectFirstFiltered() {
  const first = filteredOptions.value[0]
  if (first) select(first.value)
}

function focusFirstOption() {
  const btn = dropdownRef.value?.querySelector<HTMLButtonElement>('button')
  btn?.focus()
}

function select(value: string) {
  emit('update:modelValue', value)
  close()
}

function selectPrev() {
  const idx = props.options.findIndex(o => o.value === props.modelValue)
  if (idx > 0) {
    emit('update:modelValue', props.options[idx - 1].value)
  }
}

function selectNext() {
  const idx = props.options.findIndex(o => o.value === props.modelValue)
  if (idx < props.options.length - 1) {
    emit('update:modelValue', props.options[idx + 1].value)
  }
}

onBeforeUnmount(close)
</script>
