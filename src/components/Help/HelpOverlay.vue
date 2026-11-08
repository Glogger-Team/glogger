<template>
  <Teleport to="body">
    <Transition name="help-overlay">
      <div v-if="show" class="fixed inset-0 z-50 flex items-center justify-center">
        <!-- Backdrop -->
        <div class="absolute inset-0 bg-black/60 backdrop-blur-sm" @click="close" />

        <!-- Modal -->
        <div class="relative bg-surface-dark border border-border-default rounded-xl shadow-2xl w-[85vw] max-w-275 h-[80vh] flex flex-col overflow-hidden">
          <!-- Header -->
          <div class="flex items-center justify-between px-6 py-4 border-b border-border-default bg-surface-base/50 shrink-0">
            <div class="flex items-center gap-3">
              <img src="/glogger.png" alt="" class="size-7 rounded-lg" />
              <h2 class="text-accent-gold m-0 text-lg font-semibold tracking-wide">Glogger</h2>
            </div>
            <button
              class="text-text-dim hover:text-text-primary bg-surface-base hover:bg-surface-elevated border border-border-default rounded-lg cursor-pointer size-8 flex items-center justify-center text-sm transition-colors"
              @click="close"
              title="Close (Esc)">
              ✕
            </button>
          </div>

          <!-- Body: side tabs + content -->
          <div class="flex flex-col md:flex-row flex-1 min-h-0">
            <nav class="flex flex-row md:flex-col gap-0.5 w-full md:w-auto md:min-w-48 border-b md:border-b-0 md:border-r border-border-default p-3 bg-surface-dark overflow-x-auto">
              <button
                v-for="tab in tabs"
                :key="tab.id"
                @click="activeTab = tab.id"
                class="help-nav-btn relative flex items-center gap-2.5 px-3 py-2.5 bg-transparent border-none rounded-lg text-text-muted cursor-pointer text-sm text-left transition-all whitespace-nowrap hover:text-text-secondary hover:bg-surface-base/50"
                :class="{ 'help-nav-active': activeTab === tab.id }">
                <span class="help-nav-icon text-xs w-5 text-center" v-html="tab.icon" />
                {{ tab.label }}
                <span
                  v-if="tab.id === 'changelog' && updateStore.updateAvailable"
                  class="absolute top-2 right-2 w-2 h-2 rounded-full bg-accent-blue animate-pulse"
                />
              </button>
            </nav>

            <div class="flex-1 min-w-0 overflow-y-auto p-6 bg-surface-base/30">
              <div class="max-w-2xl">
                <AboutTab v-if="activeTab === 'about'" />
                <HelpSetupTab v-else-if="activeTab === 'help'" @navigate="$emit('navigate', $event)" />
                <ChangelogTab v-else-if="activeTab === 'changelog'" />
                <KnownIssuesTab v-else-if="activeTab === 'known-issues'" />
                <PgNewsTab v-else-if="activeTab === 'pg-news'" />
              </div>
            </div>
          </div>
        </div>
      </div>
    </Transition>
  </Teleport>
</template>

<script setup lang="ts">
import { ref, watch } from 'vue'
import { useOverlayEscape } from '../../composables/useOverlayEscape'
import { useUpdateStore } from '../../stores/updateStore'
import AboutTab from './AboutTab.vue'
import HelpSetupTab from './HelpSetupTab.vue'
import ChangelogTab from './ChangelogTab.vue'
import KnownIssuesTab from './KnownIssuesTab.vue'
import PgNewsTab from './PgNewsTab.vue'

const updateStore = useUpdateStore()

const props = defineProps<{
  show: boolean
}>()

const emit = defineEmits<{
  'update:show': [value: boolean]
  navigate: [view: string]
}>()

type TabId = 'about' | 'help' | 'changelog' | 'known-issues' | 'pg-news'

const tabs: { id: TabId; label: string; icon: string }[] = [
  { id: 'about', label: 'About', icon: '&#9830;' },
  { id: 'help', label: 'Help & Setup', icon: '?' },
  { id: 'changelog', label: 'Changelog', icon: '&#8227;' },
  { id: 'known-issues', label: 'Known Issues', icon: '!' },
  { id: 'pg-news', label: 'PG News', icon: '&#9734;' },
]

const activeTab = ref<TabId>('about')

function close() {
  emit('update:show', false)
}

// Esc scoping: HelpOverlay registers on the shared overlay Esc stack while
// open, so one Esc with stacked overlays closes only the topmost. The stack
// owns the capture-phase window listener (tab buttons may own focus, so a
// bubble-phase listener can miss the key).
useOverlayEscape(() => props.show, close);

watch(() => props.show, (open) => {
  if (open) {
    if (updateStore.updateAvailable) {
      activeTab.value = 'changelog'
    }
  }
})
</script>

<style scoped>
.help-overlay-enter-active,
.help-overlay-leave-active {
  transition: opacity 0.15s ease;
}
.help-overlay-enter-from,
.help-overlay-leave-to {
  opacity: 0;
}

.help-nav-active {
  background: var(--color-surface-base) !important;
  color: var(--color-accent-gold) !important;
  box-shadow: inset 3px 0 0 var(--color-accent-gold);
}

.help-nav-active .help-nav-icon {
  color: var(--color-accent-gold);
}
</style>
