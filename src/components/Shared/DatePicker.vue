<template>
  <div
    ref="containerRef"
    class="relative inline-flex">
    <button
      type="button"
      class="flex items-center gap-1.5 rounded border bg-surface-elevated border-border-default text-text-primary hover:border-border-default/80 cursor-pointer transition-colors text-left min-w-0 px-2 py-1 text-xs"
      @click="toggle"
      @keydown.escape="close">
      <span
        class="truncate flex-1"
        :class="modelValue ? '' : 'text-text-secondary'">
        {{ displayLabel }}
      </span>
      <svg
        class="shrink-0 w-2.5 h-2.5 text-text-secondary"
        viewBox="0 0 16 16"
        fill="currentColor">
        <path
          d="M5 1v2H3a1 1 0 00-1 1v9a1 1 0 001 1h10a1 1 0 001-1V4a1 1 0 00-1-1h-2V1h-1v2H6V1H5zm-2 5h10v7H3V6z" />
      </svg>
    </button>

    <Teleport to="body">
      <div
        v-if="open"
        class="fixed inset-0 z-[70]"
        @click="close" />
      <div
        v-if="open"
        ref="popoverRef"
        tabindex="-1"
        class="fixed z-[70] w-[260px] rounded border border-border-default bg-surface-elevated shadow-lg p-2 outline-none"
        :style="popoverStyle"
        @keydown="onPopoverKeydown">
        <!-- Month / year header -->
        <div class="flex items-center justify-between mb-2">
          <button
            type="button"
            class="px-2 py-0.5 text-text-secondary hover:text-text-primary cursor-pointer"
            aria-label="Previous month"
            @click="navigateMonth(-1)">
            ‹
          </button>
          <div class="text-text-primary text-xs font-medium tabular-nums">
            {{ monthLabel }}
          </div>
          <button
            type="button"
            class="px-2 py-0.5 text-text-secondary hover:text-text-primary cursor-pointer"
            aria-label="Next month"
            @click="navigateMonth(1)">
            ›
          </button>
        </div>

        <!-- Day-of-week header -->
        <div class="grid grid-cols-7 gap-0.5 mb-1">
          <div
            v-for="dow in dowLabels"
            :key="dow"
            class="text-[10px] text-text-secondary text-center uppercase">
            {{ dow }}
          </div>
        </div>

        <!-- Day grid -->
        <div class="grid grid-cols-7 gap-0.5">
          <button
            v-for="cell in cells"
            :key="cell.key"
            type="button"
            class="text-xs tabular-nums rounded h-7 cursor-pointer transition-colors"
            :class="dayCellClass(cell)"
            @click="selectCell(cell)">
            {{ cell.day }}
          </button>
        </div>

        <!-- Footer shortcuts -->
        <div class="flex items-center justify-between mt-2 pt-2 border-t border-border-default/60">
          <button
            type="button"
            class="text-xs text-accent-gold hover:underline cursor-pointer"
            @click="selectToday">
            Today
          </button>
          <button
            v-if="modelValue"
            type="button"
            class="text-xs text-text-secondary hover:text-text-primary cursor-pointer"
            @click="clearAndClose">
            Clear
          </button>
        </div>
      </div>
    </Teleport>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, nextTick, onBeforeUnmount, watch } from 'vue'

const props = withDefaults(
  defineProps<{
    /** ISO date string `"YYYY-MM-DD"` or empty for unset. */
    modelValue: string
    placeholder?: string
  }>(),
  {
    placeholder: 'Select date',
  },
)

const emit = defineEmits<{
  'update:modelValue': [value: string]
}>()

const containerRef = ref<HTMLElement>()
const popoverRef = ref<HTMLElement>()
const open = ref(false)
const popoverStyle = ref<Record<string, string>>({})

/** The month currently shown in the calendar grid. Stored as a Date set to
 * the first of the month. Drives the day-cell rendering. */
const currentMonth = ref<Date>(monthStart(new Date()))

const dowLabels = ['Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat', 'Sun']

interface DayCell {
  key: string
  day: number
  iso: string
  inMonth: boolean
  isToday: boolean
  isSelected: boolean
}

function monthStart(d: Date): Date {
  return new Date(d.getFullYear(), d.getMonth(), 1)
}

function pad2(n: number): string {
  return n.toString().padStart(2, '0')
}

function toIso(d: Date): string {
  return `${d.getFullYear()}-${pad2(d.getMonth() + 1)}-${pad2(d.getDate())}`
}

function fromIso(s: string): Date | null {
  if (!s || s.length !== 10) return null
  const [y, m, d] = s.split('-').map(Number)
  if (!y || !m || !d) return null
  const date = new Date(y, m - 1, d)
  if (Number.isNaN(date.getTime())) return null
  return date
}

const displayLabel = computed(() => {
  const d = fromIso(props.modelValue)
  if (!d) return props.placeholder
  return d.toLocaleDateString(undefined, { month: 'short', day: 'numeric', year: 'numeric' })
})

const monthLabel = computed(() => {
  return currentMonth.value.toLocaleDateString(undefined, { month: 'long', year: 'numeric' })
})

/** Build the 6×7 day grid. Leading cells come from the previous month, trailing
 * from the next, so every grid is exactly 42 cells regardless of month length. */
const cells = computed<DayCell[]>(() => {
  const first = currentMonth.value
  const year = first.getFullYear()
  const month = first.getMonth()
  // JS Sunday=0, but we display Monday-first. Convert.
  const jsDow = first.getDay()
  const leadingBlanks = (jsDow + 6) % 7
  const daysInMonth = new Date(year, month + 1, 0).getDate()

  const todayIso = toIso(new Date())
  const selectedIso = props.modelValue

  const out: DayCell[] = []
  // Leading days from previous month
  const prevMonthLastDay = new Date(year, month, 0).getDate()
  for (let i = leadingBlanks - 1; i >= 0; i--) {
    const day = prevMonthLastDay - i
    const d = new Date(year, month - 1, day)
    out.push({
      key: `p${day}`,
      day,
      iso: toIso(d),
      inMonth: false,
      isToday: toIso(d) === todayIso,
      isSelected: toIso(d) === selectedIso,
    })
  }
  // Current month
  for (let day = 1; day <= daysInMonth; day++) {
    const d = new Date(year, month, day)
    const iso = toIso(d)
    out.push({
      key: `c${day}`,
      day,
      iso,
      inMonth: true,
      isToday: iso === todayIso,
      isSelected: iso === selectedIso,
    })
  }
  // Trailing days to fill 42
  let trailing = 1
  while (out.length < 42) {
    const d = new Date(year, month + 1, trailing)
    const iso = toIso(d)
    out.push({
      key: `n${trailing}`,
      day: trailing,
      iso,
      inMonth: false,
      isToday: iso === todayIso,
      isSelected: iso === selectedIso,
    })
    trailing++
  }
  return out
})

function dayCellClass(cell: DayCell): string {
  const classes: string[] = []
  if (!cell.inMonth) classes.push('text-text-dim')
  else classes.push('text-text-primary hover:bg-surface-hover')
  if (cell.isSelected) classes.push('bg-accent-gold/20 text-accent-gold font-medium')
  else if (cell.isToday) classes.push('ring-1 ring-accent-gold/40')
  return classes.join(' ')
}

function navigateMonth(delta: number) {
  const d = currentMonth.value
  currentMonth.value = new Date(d.getFullYear(), d.getMonth() + delta, 1)
}

function selectCell(cell: DayCell) {
  emit('update:modelValue', cell.iso)
  // If user picked a day from a neighbor month, advance the visible month
  // so the next time they open the picker it starts on the picked month.
  const picked = fromIso(cell.iso)
  if (picked) currentMonth.value = monthStart(picked)
  close()
}

function selectToday() {
  const today = toIso(new Date())
  emit('update:modelValue', today)
  currentMonth.value = monthStart(new Date())
  close()
}

function clearAndClose() {
  emit('update:modelValue', '')
  close()
}

function positionPopover() {
  if (!containerRef.value) return
  const rect = containerRef.value.getBoundingClientRect()
  const popoverWidth = 260
  const popoverHeight = 280
  const spaceBelow = window.innerHeight - rect.bottom
  const openAbove = spaceBelow < popoverHeight && rect.top > spaceBelow

  popoverStyle.value = {
    left: `${Math.max(4, Math.min(rect.left, window.innerWidth - popoverWidth - 4))}px`,
    ...(openAbove
      ? { bottom: `${window.innerHeight - rect.top + 2}px` }
      : { top: `${rect.bottom + 2}px` }),
  }
}

function toggle() {
  if (open.value) {
    close()
  } else {
    // Sync the visible month with the current value when opening.
    const d = fromIso(props.modelValue)
    if (d) currentMonth.value = monthStart(d)
    open.value = true
    nextTick(() => {
      positionPopover()
      // Focus the popover so its keydown handler can receive arrow keys
      // and Escape. Plain divs aren't focusable without tabindex=-1.
      popoverRef.value?.focus()
    })
  }
}

function close() {
  open.value = false
}

function onPopoverKeydown(e: KeyboardEvent) {
  if (e.key === 'Escape') {
    e.stopPropagation()
    close()
    return
  }
  let delta = 0
  if (e.key === 'ArrowLeft') delta = -1
  else if (e.key === 'ArrowRight') delta = 1
  else if (e.key === 'ArrowUp') delta = -7
  else if (e.key === 'ArrowDown') delta = 7
  if (delta === 0) return
  e.preventDefault()
  // Anchor: existing selection if set, otherwise today. Lets arrow keys
  // work from a fresh-opened picker without requiring a click first.
  const current = fromIso(props.modelValue) ?? new Date()
  const next = new Date(current.getFullYear(), current.getMonth(), current.getDate() + delta)
  emit('update:modelValue', toIso(next))
  currentMonth.value = monthStart(next)
}

// Reposition on scroll/resize while open so the popover follows its trigger.
function onWindowEvent() {
  if (open.value) positionPopover()
}
window.addEventListener('resize', onWindowEvent)
window.addEventListener('scroll', onWindowEvent, true)

// Re-sync the visible month if the parent mutates `modelValue` while the
// popover is open (e.g., a character switch fires `clearFilters()` mid-
// interaction). Without this, the popover would still show whatever month
// the user was browsing — no longer aligned with the now-empty selection.
// When closed, we don't bother syncing because `toggle()` re-syncs on next
// open from the current modelValue.
watch(
  () => props.modelValue,
  (val) => {
    if (!open.value) return
    const d = fromIso(val)
    if (d) currentMonth.value = monthStart(d)
    else currentMonth.value = monthStart(new Date())
  },
)

onBeforeUnmount(() => {
  close()
  window.removeEventListener('resize', onWindowEvent)
  window.removeEventListener('scroll', onWindowEvent, true)
})

</script>
