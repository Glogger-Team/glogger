<template>
  <div
    class="card dashboard-card-bg flex flex-col h-100 relative"
    ref="cardRef"
    :style="effectiveSpan ? { gridColumn: `span ${effectiveSpan}` } : undefined">
    <!-- Title bar — drag handle -->
    <div class="dashboard-card-handle flex items-center gap-2 px-3 py-1 border-b border-border-default cursor-grab active:cursor-grabbing bg-surface-base/30 select-none">
      <span class="section-heading truncate">{{ title }}</span>
      <div v-if="hasConfig" class="ml-auto relative">
        <button
          class="p-0.5 text-text-dim hover:text-text-secondary transition-colors"
          title="Widget options"
          @click.stop="configOpen = !configOpen">
          <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 20 20" fill="currentColor" class="w-3.5 h-3.5">
            <path fill-rule="evenodd" d="M7.84 1.804A1 1 0 0 1 8.82 1h2.36a1 1 0 0 1 .98.804l.331 1.652a6.993 6.993 0 0 1 1.929 1.115l1.598-.54a1 1 0 0 1 1.186.447l1.18 2.044a1 1 0 0 1-.205 1.251l-1.267 1.113a7.047 7.047 0 0 1 0 2.228l1.267 1.113a1 1 0 0 1 .206 1.25l-1.18 2.045a1 1 0 0 1-1.187.447l-1.598-.54a6.993 6.993 0 0 1-1.929 1.115l-.33 1.652a1 1 0 0 1-.98.804H8.82a1 1 0 0 1-.98-.804l-.331-1.652a6.993 6.993 0 0 1-1.929-1.115l-1.598.54a1 1 0 0 1-1.186-.447l-1.18-2.044a1 1 0 0 1 .205-1.251l1.267-1.114a7.05 7.05 0 0 1 0-2.227L1.821 7.773a1 1 0 0 1-.206-1.25l1.18-2.045a1 1 0 0 1 1.187-.447l1.598.54A6.992 6.992 0 0 1 7.51 3.456l.33-1.652ZM10 13a3 3 0 1 0 0-6 3 3 0 0 0 0 6Z" clip-rule="evenodd" />
          </svg>
        </button>
        <!-- Config popover -->
        <div
          v-if="configOpen"
          ref="popoverRef"
          class="absolute top-full mt-1 z-50 min-w-48 bg-surface-elevated border border-border-default rounded-lg shadow-lg p-3 text-xs text-text-secondary"
          :class="popoverAlignClass">
          <slot name="config" />
        </div>
      </div>
    </div>

    <!-- Card content -->
    <div class="p-4 flex-1 min-h-0 overflow-scroll">
      <slot />
    </div>

    <!-- Right-edge resize handle — drags the X axis (snaps to grid columns) -->
    <div
      class="dashboard-card-resize absolute top-0 right-0 h-full w-1.5 cursor-ew-resize select-none z-10 hover:bg-accent/40"
      :class="{ 'bg-accent/40': dragSpan != null }"
      title="Drag to resize width — double-click to reset"
      @pointerdown="onResizeStart"
      @dblclick="onResizeReset"></div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, watch, nextTick, onMounted, onBeforeUnmount, useSlots } from 'vue'

const props = defineProps<{
  title: string
  cardId?: string
  /** Persisted explicit grid-column span; undefined = use the default size class. */
  span?: number
}>()

const emit = defineEmits<{
  /** Emitted on resize release; the new column span. 0 means "reset to default". */
  (e: 'resize', span: number): void
}>()

const slots = useSlots()
const hasConfig = computed(() => !!slots.config)
const configOpen = ref(false)
const cardRef = ref<HTMLElement | null>(null)
const popoverRef = ref<HTMLElement | null>(null)
const popoverAlignClass = ref('right-0')

// --- Width resize (X axis, snapped to grid columns) ----------------------
// Dragging the right edge changes how many grid columns the card spans, rather
// than setting a free pixel width. Snapping to columns is what lets the grid
// free up tracks so neighbouring widgets reflow into the space — a pixel width
// left the card's track full-size, stranding an unusable gap.
//
// While dragging, dragSpan holds the live span for instant feedback; the
// persisted prop only updates once on release.
const dragSpan = ref<number | null>(null)
const effectiveSpan = computed(() => dragSpan.value ?? props.span)

let startX = 0
let startSpan = 1
let trackPlusGap = 1
let gridGap = 0
let maxSpan = 1

// Measure the grid's column geometry from the card's parent (the grid element).
function measureGrid(el: HTMLElement) {
  const grid = el.parentElement
  if (!grid) return { trackPlusGap: el.offsetWidth, gap: 0, cols: 1 }
  const cs = getComputedStyle(grid)
  const tracks = cs.gridTemplateColumns.split(' ').filter(Boolean)
  const cols = Math.max(1, tracks.length)
  const trackW = parseFloat(tracks[0]) || el.offsetWidth
  const gap = parseFloat(cs.columnGap || cs.gap || '0') || 0
  return { trackPlusGap: trackW + gap, gap, cols }
}

// width of N spans = N*track + (N-1)*gap = N*(track+gap) - gap
function spanForWidth(width: number): number {
  return Math.round((width + gridGap) / trackPlusGap)
}

function widthForSpan(span: number): number {
  return span * trackPlusGap - gridGap
}

function onResizeMove(e: PointerEvent) {
  const newW = widthForSpan(startSpan) + (e.clientX - startX)
  dragSpan.value = Math.max(1, Math.min(spanForWidth(newW), maxSpan))
}

function onResizeEnd() {
  window.removeEventListener('pointermove', onResizeMove)
  window.removeEventListener('pointerup', onResizeEnd)
  // Only persist if the span actually changed — a stray click (no drag) on the
  // handle must not pin the widget at its current span.
  if (dragSpan.value != null && dragSpan.value !== startSpan) {
    emit('resize', dragSpan.value)
  }
  dragSpan.value = null
}

function onResizeStart(e: PointerEvent) {
  e.preventDefault()
  e.stopPropagation()
  const el = cardRef.value
  if (!el) return
  const geom = measureGrid(el)
  trackPlusGap = geom.trackPlusGap
  gridGap = geom.gap
  maxSpan = geom.cols
  startX = e.clientX
  // Derive the current span from the rendered width so it works whether the
  // span comes from the default size class or a saved override.
  startSpan = Math.max(1, Math.min(spanForWidth(el.offsetWidth), maxSpan))
  dragSpan.value = startSpan
  window.addEventListener('pointermove', onResizeMove)
  window.addEventListener('pointerup', onResizeEnd)
}

function onResizeReset(e: MouseEvent) {
  e.preventDefault()
  e.stopPropagation()
  dragSpan.value = null
  emit('resize', 0)
}

// Position popover so it doesn't overflow the viewport
watch(configOpen, async (open) => {
  if (!open) return
  popoverAlignClass.value = 'right-0' // default
  await nextTick()
  if (!popoverRef.value) return
  const rect = popoverRef.value.getBoundingClientRect()
  // If overflowing right, anchor to right edge
  if (rect.right > window.innerWidth - 8) {
    popoverAlignClass.value = 'right-0'
  }
  // If overflowing left, anchor to left edge
  else if (rect.left < 8) {
    popoverAlignClass.value = 'left-0'
  }
  // If overflowing bottom, cap max-height via style
  if (rect.bottom > window.innerHeight - 8) {
    const maxH = window.innerHeight - rect.top - 16
    popoverRef.value.style.maxHeight = `${maxH}px`
    popoverRef.value.style.overflowY = 'auto'
  }
})

function handleClickOutside(e: MouseEvent) {
  if (configOpen.value && cardRef.value && !cardRef.value.contains(e.target as Node)) {
    configOpen.value = false
  }
}

onMounted(() => {
  document.addEventListener('click', handleClickOutside)
})

onBeforeUnmount(() => {
  document.removeEventListener('click', handleClickOutside)
  window.removeEventListener('pointermove', onResizeMove)
  window.removeEventListener('pointerup', onResizeEnd)
})
</script>

<style scoped>
/* Override the opaque .card background with a user-adjustable opacity so the
   page background shows through. --dashboard-widget-bg-opacity (0..1) is set at
   runtime from the App Settings "Widget background opacity" control; it
   defaults to 1 (fully opaque) when unset. The scoped selector outranks the
   layered .card utility, so this wins. */
.dashboard-card-bg {
  background-color: color-mix(
    in srgb,
    var(--color-surface-card) calc(var(--dashboard-widget-bg-opacity, 1) * 100%),
    transparent
  );
}
</style>
