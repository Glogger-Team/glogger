<template>
  <div class="w-full overflow-auto">
    <table class="w-full border-collapse" :class="compact ? 'text-[11px]' : 'text-xs'">
      <thead v-if="!loading || rows.length > 0" :class="stickyHeader ? 'sticky top-0 z-10' : ''" class="bg-surface-base border-b border-border-default">
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
      <tbody v-else-if="rows.length === 0 && !loading">
        <tr>
          <td :colspan="columns.length" class="py-8">
            <EmptyState :primary="emptyText" :secondary="emptyHint" />
          </td>
        </tr>
      </tbody>

      <!-- Data rows -->
      <tbody v-else>
        <tr
          v-for="(row, index) in rows"
          :key="index"
          class="border-b border-border-default/50"
          :class="[
            hoverable ? 'hover:bg-surface-row-hover' : '',
            rowClickable ? 'cursor-pointer' : '',
            rowClassFn ? rowClassFn(row, index) : '',
          ]"
          @click="emit('rowClick', { row, index })">
          <td
            v-for="col in columns"
            :key="col.key"
            :class="[
              cellPadding,
              alignClass(col),
              col.numeric ? 'tabular-nums' : '',
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
  hoverable?: boolean
  stickyHeader?: boolean
  skeletonRows?: number
  rowClickable?: boolean
  rowClass?: (row: Record<string, unknown>, index: number) => string
}>(), {
  sortDir: 'asc',
  loading: false,
  emptyText: 'No data',
  compact: false,
  hoverable: true,
  stickyHeader: true,
  skeletonRows: 5,
  rowClickable: false,
})

const emit = defineEmits<{
  sort: [payload: { key: string; dir: 'asc' | 'desc' }]
  rowClick: [payload: { row: Record<string, unknown>; index: number }]
}>()

const rowClassFn = props.rowClass

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
