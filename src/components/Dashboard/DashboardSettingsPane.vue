<template>
  <div class="p-3 flex flex-col gap-3">
    <h3 class="section-heading">Widgets</h3>

    <div class="flex flex-col gap-1">
      <label
        v-for="widget in DASHBOARD_WIDGETS"
        :key="widget.id"
        class="flex items-center gap-2 px-2 py-1.5 rounded text-sm hover:bg-surface-elevated/50 cursor-pointer">
        <input
          type="checkbox"
          :checked="!hiddenSet.has(widget.id)"
          class="accent-accent-gold shrink-0 cursor-pointer"
          @change="toggleWidget(widget.id)" />
        <span class="text-text-primary truncate">{{ widget.name }}</span>
        <span class="ml-auto text-text-dim text-xs shrink-0">{{ widget.defaultSize }}</span>
      </label>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import { DASHBOARD_WIDGETS } from './dashboardWidgets'

const props = defineProps<{
  prefs: { cardOrder: string[]; hiddenCards: string[] }
  update: (partial: Partial<{ cardOrder: string[]; hiddenCards: string[] }>) => void
}>()

const hiddenSet = computed(() => new Set(props.prefs.hiddenCards))

function toggleWidget(id: string) {
  const hidden = [...props.prefs.hiddenCards]
  const idx = hidden.indexOf(id)

  if (idx >= 0) {
    // Show: remove from hidden, append to card order if not already there
    hidden.splice(idx, 1)
    const order = [...props.prefs.cardOrder]
    if (!order.includes(id)) {
      order.push(id)
    }
    props.update({ hiddenCards: hidden, cardOrder: order })
  } else {
    // Hide: add to hidden (keep in cardOrder so position is remembered)
    hidden.push(id)
    props.update({ hiddenCards: hidden })
  }
}
</script>
