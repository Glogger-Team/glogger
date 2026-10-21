<template>
  <div
    ref="containerRef"
    class="relative inline-flex"
    :class="fullWidth ? 'w-full' : ''">
    <button
      type="button"
      class="flex items-center gap-1.5 rounded border bg-surface-elevated border-border-default text-text-primary hover:border-border-default/80 cursor-pointer transition-colors text-left min-w-0 px-2 py-1 text-xs"
      :class="fullWidth ? 'w-full' : ''"
      @click="toggle"
      @keydown.escape="close">
      <span class="truncate flex-1">{{ displayLabel }}</span>
      <svg
        class="shrink-0 transition-transform w-2.5 h-2.5"
        :class="open ? 'rotate-180' : ''"
        viewBox="0 0 12 12"
        fill="currentColor">
        <path d="M2 4l4 4 4-4z" />
      </svg>
    </button>

    <Teleport to="body">
      <!-- z-[70] so the dropdown layers above the Stall Tracker Shop Log
           modal (z-[60]) and any future modal at z-[60]. The default z-50
           used to occlude the dropdown when this component was rendered
           inside such a modal. -->
      <div
        v-if="open"
        class="fixed inset-0 z-[70]"
        @click="close" />
      <div
        v-if="open"
        class="fixed z-[70] rounded border border-border-default bg-surface-elevated shadow-lg flex flex-col"
        :style="dropdownStyle">
        <div class="p-1.5 border-b border-border-default/60">
          <input
            ref="searchInputRef"
            v-model="searchQuery"
            type="text"
            placeholder="Search..."
            class="w-full bg-surface-base border border-border-default rounded px-2 py-1 text-xs text-text-primary focus:outline-none focus:border-accent-gold/50"
            @keydown.escape.stop="close"
            @keydown.enter.prevent="selectFirstFiltered" />
        </div>
        <div class="overflow-y-auto max-h-56">
          <button
            type="button"
            class="w-full text-left cursor-pointer transition-colors truncate px-2.5 py-1 text-xs"
            :class="
              modelValue === null
                ? 'bg-accent-gold/15 text-accent-gold font-medium'
                : 'text-text-primary hover:bg-surface-hover'
            "
            @click="select(null)">
            {{ allLabel }}
          </button>
          <button
            v-for="option in filteredOptions"
            :key="option"
            type="button"
            class="w-full text-left cursor-pointer transition-colors truncate px-2.5 py-1 text-xs"
            :class="
              option === modelValue
                ? 'bg-accent-gold/15 text-accent-gold font-medium'
                : 'text-text-primary hover:bg-surface-hover'
            "
            @click="select(option)">
            {{ option }}
          </button>
          <div
            v-if="filteredOptions.length === 0"
            class="px-2.5 py-1.5 text-xs text-text-secondary italic">
            No matches
          </div>
        </div>
      </div>
    </Teleport>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, nextTick, onBeforeUnmount } from 'vue'

const props = withDefaults(
  defineProps<{
    options: string[]
    /** Selected value, or `null` for "all". */
    modelValue: string | null
    /** Label shown when nothing is selected. */
    allLabel?: string
    fullWidth?: boolean
  }>(),
  {
    allLabel: 'All',
    fullWidth: false,
  },
)

const emit = defineEmits<{
  'update:modelValue': [value: string | null]
}>()

const containerRef = ref<HTMLElement>()
const searchInputRef = ref<HTMLInputElement>()
const open = ref(false)
const searchQuery = ref('')
const dropdownStyle = ref<Record<string, string>>({})

const displayLabel = computed(() => props.modelValue ?? props.allLabel)

const filteredOptions = computed(() => {
  const q = searchQuery.value.trim().toLowerCase()
  if (!q) return props.options
  return props.options.filter((o) => o.toLowerCase().includes(q))
})

function positionDropdown() {
  if (!containerRef.value) return
  const rect = containerRef.value.getBoundingClientRect()
  const spaceBelow = window.innerHeight - rect.bottom
  const openAbove = spaceBelow < 280 && rect.top > spaceBelow

  dropdownStyle.value = {
    left: `${rect.left}px`,
    width: `${Math.max(rect.width, 200)}px`,
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
      searchInputRef.value?.focus()
    })
  }
}

function close() {
  open.value = false
}

function select(value: string | null) {
  emit('update:modelValue', value)
  close()
}

function selectFirstFiltered() {
  if (filteredOptions.value.length > 0) {
    select(filteredOptions.value[0])
  }
}

onBeforeUnmount(close)
</script>
