<template>
  <DataTable
    :columns="columns"
    :rows="displayRows"
    :row-key="(row) => String(row._key)"
    compact
    :hoverable="false"
    :row-class="displayRowClass"
    @row-click="onRowClick">
    <template #cell-item_name="{ row }">
      <template v-if="row._kind === 'breakdown'">
        <!-- The full 1.5rem cue of the base cell's `pl-6`: the parent row's own
             text now starts at `compact`'s `px-2` (0.5rem), so the span carries
             the whole indent instead of sharing it with the cell. -->
        <span class="inline-block pl-6">
          <ItemInline :reference="String(row.item_id)" />
        </span>
      </template>
      <template v-else>
        <template v-if="row.is_dynamic">
          <span
            v-if="(row.dynamic_breakdown as unknown[] | undefined)?.length"
            class="text-text-muted text-[0.6rem] mr-1 inline-block transition-transform"
            :class="expandedKeys.has(row.item_name as string) ? 'rotate-90' : ''">&#9656;</span>
          <span class="text-accent-gold/60 text-[0.65rem] mr-1">&#9670;</span>
          <span class="text-text-secondary">{{ row.item_name }}</span>
        </template>
        <ItemInline v-else :reference="(row.item_name as string)" />
      </template>
    </template>
    <template #cell-quantity_needed="{ row }">
      <span v-if="row._kind === 'material'" class="font-mono text-text-primary">
        {{ row.quantity_needed }}
      </span>
    </template>
    <template #cell-customer_provides="{ row }">
      <input
        v-if="row._kind === 'material'"
        :value="getCustomerProvides(row as unknown as MaterialNeed)"
        type="number"
        min="0"
        :max="Math.ceil(row.quantity_needed as number)"
        class="input w-14 text-xs text-right py-0"
        @change="onCustomerProvidesChange(getMaterialKey(row as unknown as MaterialNeed), row.quantity_needed as number, $event)" />
    </template>
    <template #cell-inventory_have="{ row }">
      <span
        v-if="row._kind === 'breakdown'"
        class="font-mono text-[0.65rem]"
        :class="(row.inventory_qty as number) > 0 ? 'text-green-400/70' : 'text-text-dim'">
        {{ row.inventory_qty }}
      </span>
      <span
        v-else
        class="font-mono"
        :class="(row.inventory_have as number) > 0 ? 'text-green-400' : 'text-text-muted'">
        {{ row.inventory_have }}
      </span>
    </template>
    <template #cell-storage_have="{ row }">
      <span
        v-if="row._kind === 'breakdown'"
        class="font-mono text-[0.65rem]"
        :class="(row.storage_qty as number) > 0 ? 'text-green-400/70' : 'text-text-dim'">
        {{ row.storage_qty }}
      </span>
      <span
        v-else
        class="font-mono"
        :class="(row.storage_have as number) > 0 ? 'text-green-400' : 'text-text-muted'">
        {{ row.storage_have }}
      </span>
    </template>
    <template #cell-shortfall="{ row }">
      <span
        v-if="row._kind === 'material'"
        class="font-mono font-semibold"
        :class="(row.shortfall as number) > 0 ? 'text-accent-red' : 'text-green-400'">
        {{ (row.shortfall as number) > 0 ? row.shortfall : '&#10003;' }}
      </span>
    </template>
  </DataTable>
</template>

<script setup lang="ts">
import { computed } from "vue";
import type { MaterialNeed } from "../../types/crafting";
import DataTable, { type ColumnDef } from "../Shared/DataTable.vue";
import ItemInline from "../Shared/Item/ItemInline.vue";

const props = defineProps<{
  items: MaterialNeed[]
  pricingMode?: boolean
  customerProvides?: Record<string, number>
  expandedKeys: Set<string>
}>();

const emit = defineEmits<{
  'toggle-expanded': [key: string]
  'update-customer-provides': [key: string, quantity: number]
}>();

const columns = computed<ColumnDef[]>(() => {
  const cols: ColumnDef[] = [
    { key: "item_name", label: "Item", align: "left" },
    { key: "quantity_needed", label: "Need", align: "right", width: "4rem", numeric: true },
  ];
  if (props.pricingMode) {
    cols.push({ key: "customer_provides", label: "They Give", align: "right", width: "5rem" });
  }
  cols.push({ key: "inventory_have", label: "Inv", align: "right", width: "4rem", numeric: true });
  cols.push({ key: "storage_have", label: "Storage", align: "right", width: "4rem", numeric: true });
  cols.push({ key: "shortfall", label: "Shortfall", align: "right", width: "5rem", numeric: true });
  return cols;
});

/**
 * One row per material plus one row per expanded dynamic breakdown entry. DataTable renders
 * exactly one `<tr>` per row, so the breakdown children are flattened into the row list and
 * tagged `_kind: "breakdown"`, which the cell slots and `displayRowClass` switch on.
 */
const displayRows = computed<Record<string, unknown>[]>(() => {
  const rows: Record<string, unknown>[] = [];
  for (const mat of props.items) {
    // Parent identity is baked in as `_key`: a breakdown row keyed by its position in the
    // flattened list would change whenever *another* parent expands or collapses, and each
    // rekey remounts the row's `ItemInline` (which re-resolves its reference over IPC).
    const key = getMaterialKey(mat);
    rows.push(asRow(mat, { _kind: "material", _key: key }));
    if (props.expandedKeys.has(mat.item_name)) {
      for (const sub of mat.dynamic_breakdown ?? []) {
        rows.push(asRow(sub, { _kind: "breakdown", _key: `${key}:${sub.item_id}` }));
      }
    }
  }
  return rows;
});

function asRow(source: object, extra: Record<string, unknown>): Record<string, unknown> {
  return { ...source, ...extra } as unknown as Record<string, unknown>;
}

function getCustomerProvides(mat: MaterialNeed): number {
  if (!props.customerProvides) return 0;
  const key = mat.is_dynamic ? `kw:${mat.item_name}` : String(mat.item_id);
  return props.customerProvides[key] ?? 0;
}

function getMaterialKey(mat: MaterialNeed): string {
  return mat.is_dynamic ? `kw:${mat.item_name}` : String(mat.item_id);
}

function onCustomerProvidesChange(key: string, maxQty: number, event: Event) {
  const target = event.target as HTMLInputElement;
  const qty = Math.max(0, Math.min(Math.ceil(maxQty), parseFloat(target.value) || 0));
  emit('update-customer-provides', key, qty);
}

/** Preserves the original row classes: dimmed once covered, pointer when expandable. */
function displayRowClass(row: Record<string, unknown>): string {
  if (row._kind === "breakdown") return "bg-surface-dark/20";
  const mat = row as unknown as MaterialNeed;
  const classes: string[] = [];
  if (mat.shortfall === 0) classes.push("opacity-60");
  if (mat.is_dynamic && mat.dynamic_breakdown?.length) classes.push("cursor-pointer");
  return classes.join(" ");
}

/** Whole-row click toggles the breakdown, as the original row `@click` did. */
function onRowClick({ row }: { row: Record<string, unknown> }) {
  const mat = row as unknown as MaterialNeed;
  if (row._kind === "material" && mat.is_dynamic && mat.dynamic_breakdown?.length) {
    emit('toggle-expanded', mat.item_name);
  }
}
</script>
