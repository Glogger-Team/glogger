<template>
  <div class="w-full">
    <!-- No `overflow` on this wrapper: a box with `overflow != visible` becomes the nearest
         scrollport for the table's `position: sticky` header, and because this wrapper's height
         is content-driven it never scrolls — the header would scroll away with the rows instead
         of pinning to the parent scroller. Only the outermost scroller must stay the scrollport.
         (Keep this comment out of the SFC's root position: a comment before the root element
         makes the root a Fragment, which silently drops every fallthrough attribute —
         `class`/`style`/`ref`/native listeners.) -->
    <table class="w-full border-collapse" :class="size === 'sm' ? 'text-sm' : 'text-xs'">
      <thead v-if="rows.length > 0 || loading" :class="stickyHeader ? 'sticky top-0 z-10' : ''" class="bg-surface-base border-b border-border-default">
        <tr>
          <th
            v-for="col in columns"
            :key="col.key"
            class="text-[10px] uppercase tracking-wider text-text-muted font-semibold"
            :class="[
              headerPadding,
              alignClass(col),
              col.sortable ? 'cursor-pointer hover:text-text-primary select-none' : '',
            ]"
            :style="col.width ? { width: col.width } : undefined"
            @click="col.sortable ? handleSort(col.key) : undefined">
            <slot :name="`header-${col.key}`" :column="col">
              {{ col.label }}
              <span v-if="col.sortable && sortKey === col.key" class="ml-0.5">
                {{ sortDir === 'asc' ? '\u25B2' : '\u25BC' }}
              </span>
            </slot>
          </th>
        </tr>
      </thead>

      <!-- Loading skeleton -->
      <tbody v-if="loading && rows.length === 0">
        <tr v-for="i in skeletonRows" :key="`skel-${i}`" class="border-b border-border-default/50">
          <td
            v-for="col in columns"
            :key="col.key"
            :class="cellPadding">
            <div class="h-3 rounded animate-pulse bg-surface-elevated" :class="col.numeric ? 'ml-auto w-12' : 'w-3/4'" />
          </td>
        </tr>
      </tbody>

      <!-- Empty state -->
      <tbody v-else-if="rows.length === 0">
        <tr>
          <td :colspan="columns.length" :class="emptyVariant === 'compact' ? 'py-2' : 'py-8'">
            <EmptyState :primary="emptyText" :secondary="emptyHint" :variant="emptyVariant" />
          </td>
        </tr>
      </tbody>

      <!-- Data rows -->
      <tbody v-else>
        <tr
          v-for="(row, index) in rows"
          :key="rowKey ? rowKey(row, index) : index"
          class="border-b border-border-default/50"
          :class="[
            hoverable ? 'hover:bg-surface-row-hover' : '',
            rowClickable ? 'cursor-pointer' : '',
            rowClass ? rowClass(row, index) : '',
          ]"
          :tabindex="rowClickable ? 0 : undefined"
          @click="emit('rowClick', { row, index })"
          @keydown.enter="onRowKeydown($event, row, index)"
          @keydown.space="onRowKeydown($event, row, index)">
          <td
            v-for="col in columns"
            :key="col.key"
            :class="[
              cellPadding,
              alignClass(col),
              col.numeric ? 'tabular-nums' : '',
              col.cellClass ?? '',
            ]">
            <slot :name="`cell-${col.key}`" :row="row" :value="getCellValue(row, col.key)">
              {{ getCellValue(row, col.key) ?? '' }}
            </slot>
          </td>
        </tr>
      </tbody>
    </table>
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import EmptyState from './EmptyState.vue'

export interface ColumnDef {
  key: string
  label: string
  sortable?: boolean
  align?: 'left' | 'center' | 'right'
  width?: string
  numeric?: boolean
  /**
   * Extra classes for this column's `<td>` (e.g. `align-top`, `max-w-0` for truncation).
   * Appended after the alignment/numeric classes, but Tailwind resolves equal-specificity
   * conflicts by stylesheet order, not attribute order — so it cannot reliably override
   * `align`/`numeric`'s `text-right`. Use the column's own `align` for that.
   */
  cellClass?: string
}

const props = withDefaults(defineProps<{
  columns: ColumnDef[]
  rows: Record<string, unknown>[]
  sortKey?: string
  sortDir?: 'asc' | 'desc'
  loading?: boolean
  emptyText?: string
  emptyHint?: string
  compact?: boolean
  size?: 'sm' | 'xs'
  /** `EmptyState` variant for the empty branch. `compact` is a single inline line for
   * dense widgets/panels whose old markup was an italic one-liner. */
  emptyVariant?: 'panel' | 'compact'
  hoverable?: boolean
  stickyHeader?: boolean
  skeletonRows?: number
  rowClickable?: boolean
  rowKey?: (row: Record<string, unknown>, index: number) => string | number
  rowClass?: (row: Record<string, unknown>, index: number) => string
}>(), {
  sortDir: 'asc',
  loading: false,
  emptyText: 'No data',
  compact: false,
  size: 'xs',
  emptyVariant: 'panel',
  hoverable: true,
  stickyHeader: true,
  skeletonRows: 5,
  rowClickable: false,
})

const emit = defineEmits<{
  sort: [payload: { key: string; dir: 'asc' | 'desc' }]
  rowClick: [payload: { row: Record<string, unknown>; index: number }]
}>()

const headerPadding = computed(() => props.compact ? 'px-2 py-1' : 'px-3 py-1.5')
const cellPadding = computed(() => props.compact ? 'px-2 py-1' : 'px-3 py-2')

function alignClass(col: ColumnDef): string {
  if (col.align === 'right' || col.numeric) return 'text-right'
  if (col.align === 'center') return 'text-center'
  return 'text-left'
}

function getCellValue(row: Record<string, unknown>, key: string): unknown {
  return row[key]
}

/**
 * `Enter`/`Space` on a focused row emit the same `rowClick` payload — but only for a
 * `rowClickable` table and only when the event comes from the row itself. A keydown that
 * bubbles out of a control inside a cell belongs to that control: emitting `rowClick` for
 * it would fire the row action while the user is operating the control, and calling
 * `preventDefault()` would cancel the control's own activation (Space on a checkbox, Space
 * or Enter on a button). `.prevent` cannot be used as a modifier here — Vue applies guard
 * modifiers before the handler runs, so it would cancel the key for every row regardless.
 */
function onRowKeydown(event: KeyboardEvent, row: Record<string, unknown>, index: number) {
  if (!props.rowClickable || event.target !== event.currentTarget) return
  event.preventDefault()
  emit('rowClick', { row, index })
}

function handleSort(key: string) {
  let dir: 'asc' | 'desc'
  if (props.sortKey === key) {
    dir = props.sortDir === 'asc' ? 'desc' : 'asc'
  } else {
    dir = 'asc'
  }
  emit('sort', { key, dir })
}
</script>
