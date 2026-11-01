<template>
  <Teleport to="body">
    <Transition name="modal">
      <div v-if="show" ref="dialogRef" tabindex="-1" class="fixed inset-0 z-50 flex items-center justify-center p-4 outline-none" @keydown.escape="onEscape">
        <!-- Backdrop (no click-to-dismiss — this is intentionally blocking) -->
        <div class="absolute inset-0 bg-black/60" />

        <!-- Dialog -->
        <div class="relative bg-surface-base border border-accent-red/40 rounded-lg shadow-xl w-[28rem] max-w-full max-h-[85vh] overflow-y-auto">
          <!-- Header -->
          <div class="px-5 pt-5 pb-2 flex items-center gap-2">
            <span class="text-accent-red text-lg">!</span>
            <h3 class="text-sm font-semibold text-text-primary">Game Data Update Available</h3>
          </div>

          <!-- Content -->
          <div class="px-5 pb-4">
            <p class="text-sm text-text-secondary leading-relaxed">
              Game data has been updated
              <span class="text-text-primary font-medium">
                (v{{ currentVersion }} &#8594; v{{ remoteVersion }})
              </span>.
              Glogger needs to restart to load the new data.
            </p>
            <p class="mt-2 text-xs text-accent-red/80 leading-relaxed">
              Running with outdated game data may cause incorrect item names,
              broken recipes, or missing information.
            </p>
          </div>

          <!-- Footer -->
          <div class="flex items-center justify-end gap-2 px-5 pb-5">
            <button
              class="px-3 py-1.5 text-xs font-medium rounded cursor-pointer bg-surface-elevated border border-border-default text-text-secondary hover:bg-surface-hover"
              :disabled="restarting"
              @click="$emit('dismiss')">
              Remind Me Later
            </button>
            <button
              class="px-3 py-1.5 text-xs font-medium rounded cursor-pointer bg-accent-red/20 border border-accent-red/40 text-accent-red hover:bg-accent-red/30"
              :disabled="restarting"
              @click="$emit('restart')">
              {{ restarting ? 'Restarting...' : 'Restart Now' }}
            </button>
          </div>
        </div>
      </div>
    </Transition>
  </Teleport>
</template>

<script setup lang="ts">
import { ref, watch, nextTick } from 'vue'

const props = defineProps<{
  show: boolean
  currentVersion: number
  remoteVersion: number
  restarting: boolean
}>()

const emit = defineEmits<{
  dismiss: []
  restart: []
}>()

// Esc maps to `dismiss` (same as Remind Me Later) — but never while
// `restarting` is true (buttons disabled; the modal must stay until done).
function onEscape() {
  if (!props.restarting) emit('dismiss')
}

const dialogRef = ref<HTMLElement | null>(null)
let previouslyFocused: HTMLElement | null = null

// Blocking modal keeps no-click backdrop, but keyboard users still need an
// out: Esc maps to `dismiss` (same as Remind Me Later), not restart.
watch(() => props.show, (open) => {
  if (open) {
    previouslyFocused = document.activeElement instanceof HTMLElement ? document.activeElement : null
    nextTick(() => dialogRef.value?.focus())
  } else {
    previouslyFocused?.focus()
    previouslyFocused = null
  }
})
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
