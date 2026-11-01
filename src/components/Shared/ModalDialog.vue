<template>
  <Teleport to="body">
    <Transition name="modal">
      <div v-if="show" ref="dialogRef" tabindex="-1" class="fixed inset-0 z-50 flex items-center justify-center outline-none" @keydown.escape="handleCancel">
        <!-- Backdrop -->
        <div class="absolute inset-0 bg-black/50" @click="handleCancel" />

        <!-- Dialog -->
        <div class="relative bg-surface-base border border-border-default rounded-lg shadow-xl w-96 max-w-[90vw]">
          <!-- Header -->
          <div class="px-4 pt-4 pb-2">
            <h3 class="text-sm font-semibold text-text-primary">{{ title }}</h3>
          </div>

          <!-- Content -->
          <div class="px-4 pb-4">
            <slot>
              <input
                v-if="type === 'prompt'"
                ref="inputRef"
                v-model="inputValue"
                type="text"
                :placeholder="placeholder"
                class="w-full bg-surface-elevated border border-border-default rounded px-3 py-1.5 text-sm text-text-primary focus:border-accent-gold/50 focus:outline-none"
                @keydown.enter="handleConfirm" />
              <p v-else-if="type === 'confirm'" class="text-sm text-text-secondary">
                {{ message }}
              </p>
            </slot>
          </div>

          <!-- Footer -->
          <div class="flex items-center justify-end gap-2 px-4 pb-4">
            <button
              class="px-3 py-1.5 text-xs font-medium rounded cursor-pointer bg-surface-elevated border border-border-default text-text-secondary hover:bg-surface-hover"
              @click="handleCancel">
              Cancel
            </button>
            <button
              class="px-3 py-1.5 text-xs font-medium rounded cursor-pointer"
              :class="confirmClass"
              @click="handleConfirm">
              {{ confirmLabel }}
            </button>
          </div>
        </div>
      </div>
    </Transition>
  </Teleport>
</template>

<script setup lang="ts">
import { ref, computed, watch, nextTick } from 'vue'

const props = withDefaults(defineProps<{
  show: boolean
  title: string
  type?: 'prompt' | 'confirm'
  message?: string
  placeholder?: string
  initialValue?: string
  confirmLabel?: string
  danger?: boolean
}>(), {
  type: 'prompt',
  message: '',
  placeholder: '',
  initialValue: '',
  confirmLabel: 'OK',
  danger: false,
})

const emit = defineEmits<{
  'update:show': [value: boolean]
  confirm: [value: string]
  cancel: []
}>()

const dialogRef = ref<HTMLElement>()
const inputRef = ref<HTMLInputElement>()
const inputValue = ref('')

const confirmClass = computed(() =>
  props.danger
    ? 'bg-red-900/30 border border-red-700/40 text-red-400 hover:bg-red-900/50'
    : 'bg-accent-gold/20 border border-accent-gold/40 text-accent-gold hover:bg-accent-gold/30'
)

watch(() => props.show, async (open) => {
  if (open) {
    inputValue.value = props.initialValue
    await nextTick()
    if (props.type === 'prompt') {
      inputRef.value?.focus()
      inputRef.value?.select()
    } else {
      dialogRef.value?.focus()
    }
  }
})

function handleConfirm() {
  if (props.type === 'prompt' && !inputValue.value.trim()) return
  emit('confirm', inputValue.value.trim())
  emit('update:show', false)
}

function handleCancel() {
  emit('cancel')
  emit('update:show', false)
}
</script>

<style scoped>
.modal-enter-active,
.modal-leave-active {
  transition: opacity 0.15s ease;
}
.modal-enter-from,
.modal-leave-to {
  opacity: 0;
}
</style>
