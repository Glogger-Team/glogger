<template>
  <div class="flex flex-col gap-3">
    <div v-if="vendorItems.length === 0 && sourceItems.length === 0" class="text-text-dim text-xs italic">
      You have everything you need!
    </div>

    <!-- Vendor-purchasable items -->
    <template v-if="vendorItems.length > 0">
      <div v-if="!bare" class="flex items-center justify-between">
        <h4 class="text-text-secondary text-xs font-semibold uppercase tracking-wide m-0">
          Shopping / Gathering List
        </h4>
        <span class="text-text-muted text-[10px]">
          Est. cost: <span class="text-accent-red font-semibold">{{ totalCost.toLocaleString() }}g</span>
        </span>
      </div>

      <DataTable
        :columns="vendorColumns"
        :row-key="(row) => String(row.item_id)"
        compact
        :rows="(vendorItems as unknown as Record<string, unknown>[])">
        <template #cell-item_name="{ value }">
          <ItemInline :reference="(value as string)" />
        </template>
        <template #cell-shortfall="{ value }">
          <span class="text-text-primary">{{ value }}</span>
        </template>
        <template #cell-cost="{ value }">
          <span class="text-accent-red">{{ Math.round(value as number).toLocaleString() }}g</span>
        </template>
      </DataTable>
    </template>

    <!-- Items that need to be sourced elsewhere -->
    <template v-if="sourceItems.length > 0">
      <h4 class="text-text-secondary text-xs font-semibold uppercase tracking-wide m-0">
        Source Elsewhere
      </h4>
      <p class="text-text-dim text-[10px] -mt-2 m-0">
        No price data available — set a market price or find these in-game.
      </p>

      <DataTable
        :columns="sourceColumns"
        :row-key="(row) => String(row.item_id)"
        compact
        :rows="(sourceItems as unknown as Record<string, unknown>[])">
        <template #cell-item_name="{ row }">
          <template v-if="row.is_dynamic">
            <span class="text-accent-gold/60 text-[10px] mr-1">&#9670;</span>
            <span class="text-text-secondary">{{ row.item_name }}</span>
          </template>
          <ItemInline v-else :reference="(row.item_name as string)" />
          <span v-if="row.is_craftable" class="text-accent-gold/70 text-[10px] ml-1" title="Can be crafted">craftable</span>
        </template>
        <template #cell-shortfall="{ value }">
          <span class="text-text-primary">{{ value }}</span>
        </template>
      </DataTable>
    </template>
  </div>
</template>

<script setup lang="ts">
import { computed } from "vue";
import type { MaterialNeed } from "../../types/crafting";
import DataTable, { type ColumnDef } from "../Shared/DataTable.vue";
import ItemInline from "../Shared/Item/ItemInline.vue";

const props = withDefaults(defineProps<{
  needs: MaterialNeed[]
  bare?: boolean
}>(), {
  bare: false,
});

const vendorColumns: ColumnDef[] = [
  { key: "item_name", label: "Item", align: "left" },
  { key: "shortfall", label: "Need", align: "right", width: "4rem", numeric: true },
  { key: "cost", label: "Est. Cost", align: "right", width: "6rem" },
];

const sourceColumns: ColumnDef[] = [
  { key: "item_name", label: "Item", align: "left" },
  { key: "shortfall", label: "Need", align: "right", width: "4rem", numeric: true },
];

interface ShoppingItem {
  item_id: number
  item_name: string
  shortfall: number
  cost: number | null
  is_craftable: boolean
  is_dynamic: boolean
}

const allShortfalls = computed((): ShoppingItem[] => {
  return props.needs
    .filter((m) => m.shortfall > 0)
    .map((m) => ({
      item_id: m.item_id,
      item_name: m.item_name,
      shortfall: m.shortfall,
      cost: m.unit_price ? m.unit_price * m.shortfall : null,
      is_craftable: m.is_craftable,
      is_dynamic: m.is_dynamic ?? false,
    }))
    .sort((a, b) => a.item_name.localeCompare(b.item_name));
});

const vendorItems = computed(() =>
  allShortfalls.value.filter((item) => item.cost !== null)
);

const sourceItems = computed(() =>
  allShortfalls.value.filter((item) => item.cost === null)
);

const totalCost = computed(() =>
  vendorItems.value.reduce((sum, item) => sum + (item.cost ?? 0), 0)
);
</script>
