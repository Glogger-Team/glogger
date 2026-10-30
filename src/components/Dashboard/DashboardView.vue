<template>
  <PaneLayout
    screen-key="dashboard"
    :right-pane="{ title: 'Widgets', defaultWidth: 260, minWidth: 200, maxWidth: 400 }">
  <div class="pt-4 flex flex-col gap-4 h-full overflow-y-auto">
    <draggable
      v-model="orderedWidgets"
      item-key="id"
      handle=".dashboard-card-handle"
      ghost-class="opacity-30"
      :force-fallback="true"
      :animation="200"
      class="grid gap-4"
      style="grid-template-columns: repeat(auto-fill, minmax(280px, 1fr))">
      <template #item="{ element: widget }">
        <DashboardCard
          :title="widget.name"
          :card-id="widget.id"
          :span="prefs.cardSpans?.[widget.id]"
          :class="sizeClass(widget)"
          @resize="(s: number) => setCardSpan(widget.id, s)">
          <component :is="widget.component" />
          <template v-if="widget.configComponent" #config>
            <component :is="widget.configComponent" />
          </template>
        </DashboardCard>
      </template>
    </draggable>
  </div>

  <template #right>
    <DashboardSettingsPane :prefs="prefs" :update="update" />
  </template>
  </PaneLayout>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import draggable from 'vuedraggable'
import PaneLayout from '../Shared/PaneLayout.vue'
import DashboardCard from './DashboardCard.vue'
import DashboardSettingsPane from './DashboardSettingsPane.vue'
import { DASHBOARD_WIDGETS, DEFAULT_CARD_ORDER, SIZE_CLASSES, type DashboardWidget } from './dashboardWidgets'
import { useViewPrefs } from '../../composables/useViewPrefs'

const { prefs, update } = useViewPrefs('dashboard', {
  cardOrder: DEFAULT_CARD_ORDER,
  hiddenCards: [] as string[],
  cardSpans: {} as Record<string, number>,
})
// One-time migration: XpRateWidget was merged into ProdigyTrackerWidget (superset).
// Strip 'xp-rate' from saved prefs; unhide prodigy-tracker if the user loses the widget entirely.
if (prefs.value.cardOrder.includes('xp-rate')) {
  const hidden = new Set(prefs.value.hiddenCards)
  const xpRateWasVisible = !hidden.has('xp-rate')
  hidden.delete('xp-rate')
  if (xpRateWasVisible && hidden.has('prodigy-tracker')) {
    hidden.delete('prodigy-tracker')
  }
  const { ['xp-rate']: _removed, ...spans } = prefs.value.cardSpans ?? {}
  update({
    cardOrder: prefs.value.cardOrder.filter(id => id !== 'xp-rate'),
    hiddenCards: [...hidden],
    cardSpans: spans,
  })
}

function setCardSpan(id: string, span: number) {
  const spans = { ...(prefs.value.cardSpans ?? {}) }
  if (span > 0) {
    spans[id] = span
  } else {
    // 0 = reset to default (use the widget's default size class)
    delete spans[id]
  }
  update({ cardSpans: spans })
}

function getVisibleWidgets(): DashboardWidget[] {
  const hidden = new Set(prefs.value.hiddenCards)

  // Start with saved order, append any new widgets not yet in the list
  const order = [...prefs.value.cardOrder]
  for (const w of DASHBOARD_WIDGETS) {
    if (!order.includes(w.id)) {
      order.push(w.id)
    }
  }

  return order
    .map(id => DASHBOARD_WIDGETS.find(w => w.id === id))
    .filter((w): w is DashboardWidget => w != null && !hidden.has(w.id))
}

const orderedWidgets = computed({
  get: () => getVisibleWidgets(),
  set: (val: DashboardWidget[]) => {
    // Persist visible cards in new order, hidden cards appended at end
    const hiddenIds = prefs.value.hiddenCards
    update({ cardOrder: [...val.map(w => w.id), ...hiddenIds] })
  },
})

function sizeClass(widget: DashboardWidget): string {
  return SIZE_CLASSES[widget.defaultSize]
}
</script>
