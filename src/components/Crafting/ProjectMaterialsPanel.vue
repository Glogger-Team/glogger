<template>
  <div class="h-full overflow-y-auto p-4 flex flex-col gap-3">
    <!-- Empty state -->
    <template v-if="!activeProject && !activeGroupName">
      <EmptyState variant="compact" primary="No project selected" secondary="Select a project to see materials." />
    </template>

    <template v-else>
      <!-- Header: group or project -->
      <div v-if="activeGroupName" class="flex flex-col gap-1">
        <h3 class="text-accent-gold text-base font-semibold m-0">{{ activeGroupName }}</h3>
        <div class="text-text-muted text-[0.65rem]">
          Group summary &mdash; {{ groupProjectNames.length }} project{{ groupProjectNames.length !== 1 ? 's' : '' }}
        </div>
      </div>

      <!-- Buffer control + recheck button (visible when there's content) -->
      <div v-if="recipeEntries.length > 0 && hasContent" class="flex items-center justify-end gap-3">
        <div
          class="flex items-center gap-1 text-[0.65rem] text-text-muted cursor-help"
          title="Safety buffer added to ingredients with a variable (<100%) chance to be consumed per craft. Their actual usage is random, so this pads the plan to avoid coming up short. Always-consumed materials are unaffected.">
          <span>Consume buffer</span>
          <!-- Stepper: −/+ buttons sit outside the input box so they never overlap the value -->
          <div class="flex items-center">
            <button
              type="button"
              class="w-4 h-[1.15rem] flex items-center justify-center leading-none text-text-muted hover:text-accent-gold bg-surface-card border border-border-default border-r-0 rounded-l cursor-pointer disabled:opacity-40 disabled:cursor-default"
              :disabled="consumeBufferPct <= 0"
              title="Decrease buffer by 5%"
              @click="stepBuffer(-5)">&minus;</button>
            <input
              type="number"
              min="0"
              max="100"
              step="5"
              :value="consumeBufferPct"
              class="input no-spinner w-7 text-center text-[0.65rem] py-0 px-0 rounded-none"
              @change="onBufferInput" />
            <button
              type="button"
              class="w-4 h-[1.15rem] flex items-center justify-center leading-none text-text-muted hover:text-accent-gold bg-surface-card border border-border-default border-l-0 rounded-r cursor-pointer disabled:opacity-40 disabled:cursor-default"
              :disabled="consumeBufferPct >= 100"
              title="Increase buffer by 5%"
              @click="stepBuffer(5)">+</button>
          </div>
          <span>%</span>
        </div>
        <button
          class="text-[0.65rem] text-text-muted hover:text-text-primary cursor-pointer bg-transparent border border-border-light rounded px-1.5 py-0.5 shrink-0 transition-colors"
          :disabled="resolving"
          @click="$emit('resolve')">
          {{ resolving ? 'Refreshing...' : 'Recheck Inventory' }}
        </button>
      </div>

      <!-- Recipe summary: what you're crafting (collapsible) -->
      <AccordionSection v-if="recipeEntries.length > 0" :default-open="!hasContent">
        <template #title>Crafting</template>
        <template #badge>
          <span class="text-text-muted text-[0.65rem]">{{ recipeEntries.length }} recipe{{ recipeEntries.length !== 1 ? 's' : '' }}</span>
        </template>
        <div class="flex flex-wrap gap-x-4 gap-y-0.5 text-xs">
          <div
            v-for="entry in recipeEntries"
            :key="entry.id"
            class="flex items-center gap-1.5 text-text-muted">
            <RecipeInline :reference="entry.recipe_name" />
            <template v-if="entry.target_stock !== null">
              <span
                v-if="getStockTarget(entry.id)"
                class="text-[0.65rem]"
                :class="getStockTarget(entry.id)!.effectiveQty <= 0 ? 'text-green-400' : 'text-accent-gold'">
                {{ getStockTarget(entry.id)!.effectiveQty <= 0 ? 'met' : `×${getStockTarget(entry.id)!.effectiveQty}` }}
              </span>
              <span v-else class="font-mono text-[0.65rem] text-accent-gold">target {{ entry.target_stock }}</span>
            </template>
            <span v-else class="font-mono text-text-primary/70">&times;{{ entry.quantity }}</span>
          </div>
        </div>
      </AccordionSection>

      <!-- Skeleton loading state when resolving with no content yet -->
      <div v-if="resolving && !hasContent" class="flex flex-col gap-3">
        <!-- Skeleton for intermediates section -->
        <AccordionSection :default-open="true">
          <template #title>Intermediates</template>
          <div class="flex flex-col gap-1">
            <div
              v-for="i in 3"
              :key="`skel-inter-${i}`"
              class="flex items-center gap-2 px-2 py-1.5 rounded bg-surface-dark/30 border border-surface-elevated/50">
              <SkeletonLoader variant="rect" width="w-24" height="h-4" />
              <SkeletonLoader variant="rect" width="w-16" height="h-3" />
            </div>
          </div>
        </AccordionSection>

        <!-- Skeleton for raw materials table -->
        <AccordionSection :default-open="true">
          <template #title>Raw Materials</template>
          <DataTableSkeleton :columns="2" :rows="5" :show-header="true" />
        </AccordionSection>
      </div>

      <!-- Two-column layout for materials + actionable lists -->
      <div v-if="hasContent" class="grid grid-cols-1 lg:grid-cols-2 gap-3 items-start">
        <!-- Left: Materials overview -->
        <div class="flex flex-col gap-3">
          <!-- Intermediates: craft-or-buy decisions -->
          <AccordionSection v-if="craftableItems.length > 0">
            <template #title>Intermediates</template>
            <template #badge>
              <div class="flex items-center gap-2">
                <span class="text-text-muted text-[0.65rem]">
                  {{ expandedItemIds.size }} crafting, {{ craftableButNotExpanded.length }} buying
                </span>
                <template v-if="!activeGroupName">
                  <button
                    v-if="craftableButNotExpanded.length > 0"
                    class="text-[0.6rem] text-text-muted hover:text-accent-gold cursor-pointer bg-transparent border border-border-light hover:border-accent-gold/30 rounded px-1 py-0 transition-colors"
                    @click.stop="emit('set-all-intermediates', craftableItems.map(i => i.item_id), true)">
                    craft all
                  </button>
                  <button
                    v-if="expandedItemIds.size > 0"
                    class="text-[0.6rem] text-text-muted hover:text-accent-gold cursor-pointer bg-transparent border border-border-light hover:border-accent-gold/30 rounded px-1 py-0 transition-colors"
                    @click.stop="emit('set-all-intermediates', craftableItems.map(i => i.item_id), false)">
                    buy all
                  </button>
                </template>
              </div>
            </template>
            <div class="flex flex-col gap-1">
              <div
                v-for="item in craftableItems"
                :key="item.item_id"
                class="flex items-center gap-2 px-2 py-1.5 rounded text-xs"
                :class="item.isExpanded
                  ? 'bg-accent-gold/5 border border-accent-gold/20'
                  : 'bg-surface-dark/30 border border-surface-elevated/50'">
                <ItemInline :reference="item.item_name" />
                <div class="flex flex-col text-[0.65rem] text-text-muted leading-tight">
                  <span>need {{ item.quantity }}</span>
                  <template v-if="item.have > 0">
                    <span v-if="item.toCraft > 0">
                      have <span class="text-green-400">{{ item.have }}</span>
                      → {{ item.isExpanded ? 'craft' : 'buy' }} <span class="text-text-primary">{{ item.toCraft }}</span>
                    </span>
                    <span v-else class="text-green-400">have {{ item.have }} ✓</span>
                  </template>
                </div>
                <button
                  v-if="!activeGroupName"
                  class="ml-auto text-[0.65rem] cursor-pointer bg-transparent border rounded px-1.5 py-0.5 transition-colors shrink-0"
                  :class="item.isExpanded
                    ? 'text-accent-gold border-accent-gold/40 bg-accent-gold/10 hover:bg-transparent hover:text-text-muted'
                    : 'text-text-muted border-border-light hover:text-accent-gold hover:border-accent-gold/30'"
                  @click="$emit('toggle-intermediate', item.item_id)">
                  {{ item.isExpanded ? '✓ crafting' : '+ craft' }}
                </button>
              </div>
            </div>
          </AccordionSection>

          <!-- Raw materials table (with availability) -->
          <AccordionSection v-if="materialNeeds.length > 0">
            <template #title>Raw Materials</template>
            <template #badge>
              <div class="flex gap-3 text-[0.65rem] text-text-muted">
                <span>{{ materialNeeds.length }}</span>
                <span><span class="text-green-400">{{ coveredCount }}</span> ok</span>
                <span v-if="partialCount > 0"><span class="text-yellow-400">{{ partialCount }}</span> partial</span>
                <span v-if="missingCount > 0"><span class="text-accent-red">{{ missingCount }}</span> missing</span>
                <span v-if="checkingAvailability" class="inline-flex items-center gap-1 text-accent-gold/60">
                  <LoadingSpinner size="xs" />
                  checking
                </span>
              </div>
            </template>
            <MaterialSummary
              :needs="materialNeeds"
              :bare="true"
              :pricing-mode="pricingMode"
              :customer-provides="customerProvides"
              @update-customer-provides="(key, qty) => $emit('update-customer-provides', key, qty)" />
          </AccordionSection>

          <!-- Fallback: raw materials table when availability hasn't been checked yet -->
          <AccordionSection v-else-if="materials.size > 0">
            <template #title>Raw Materials</template>
            <template #badge>
              <div class="flex items-center gap-2 text-text-muted text-[0.65rem]">
                <span>{{ materials.size }} items</span>
                <span v-if="checkingAvailability" class="inline-flex items-center gap-1 text-accent-gold/60">
                  <LoadingSpinner size="xs" />
                  checking availability
                </span>
              </div>
            </template>
            <DataTable
              :columns="materialColumns"
              :rows="(sortedMaterials as unknown as Record<string, unknown>[])"
              :row-key="(row) => String(row.key)"
              compact
              :hoverable="false"
              :row-class="materialRowClass">
              <template #cell-item_name="{ row }">
                <template v-if="row.is_dynamic">
                  <span class="text-accent-gold/60 text-[0.65rem] mr-1">&#9670;</span>
                  <span class="text-text-secondary">{{ row.item_name }}</span>
                </template>
                <ItemInline v-else-if="row.item_id !== null" :reference="(row.item_name as string)" />
                <span v-else class="text-text-muted italic">{{ row.item_name }}</span>
              </template>
              <template #cell-expected_quantity="{ row }">
                <span class="text-text-primary font-mono whitespace-nowrap">
                  {{ row.expected_quantity }}
                  <span
                    v-if="(row.chance_to_consume as number) < 1"
                    class="text-accent-gold cursor-help"
                    :title="`~${Math.round((row.chance_to_consume as number) * 100)}% chance to consume per use${consumeBufferPct > 0 ? ` (incl. +${consumeBufferPct}% safety buffer)` : ''}. Raw quantity: ${row.quantity}`">
                    *
                  </span>
                </span>
              </template>
              <template #cell-customer_provides="{ row }">
                <input
                  :value="customerProvides[row.key as string] ?? 0"
                  type="number"
                  min="0"
                  :max="Math.ceil(row.expected_quantity as number)"
                  class="input w-14 text-xs text-right py-0"
                  @change="onCustomerProvidesChange(row.key as string, row.expected_quantity as number, $event)" />
              </template>
            </DataTable>
          </AccordionSection>
        </div>

        <!-- Right: Actionable lists -->
        <div class="flex flex-col gap-3">
          <!-- Shopping / gathering list -->
          <AccordionSection v-if="materialNeeds.length > 0 && hasShortfalls">
            <template #title>Shopping / Gathering</template>
            <template #badge>
              <span v-if="vendorCost > 0" class="text-text-muted text-[0.65rem]">
                ~<span class="text-accent-red font-semibold">{{ vendorCost.toLocaleString() }}g</span>
              </span>
            </template>
            <ShoppingList :needs="materialNeeds" :bare="true" />
          </AccordionSection>

          <!-- Pickup list -->
          <AccordionSection v-if="materialNeeds.length > 0 && hasPickupItems">
            <template #title>Pickup List</template>
            <template #badge>
              <span class="text-text-muted text-[0.65rem]">{{ pickupAreaCount }} area{{ pickupAreaCount !== 1 ? 's' : '' }}</span>
            </template>
            <PickupList :needs="materialNeeds" :bare="true" />
          </AccordionSection>

          <!-- "All covered" state when nothing to shop or pick up -->
          <div
            v-if="materialNeeds.length > 0 && !hasShortfalls && !hasPickupItems"
            class="border border-surface-elevated rounded px-3 py-4 text-center text-xs text-green-400">
            All materials in inventory — ready to craft!
          </div>
        </div>
      </div>

      <!-- Footer actions -->
      <div v-if="materialNeeds.length > 0 || (activeProject && !activeGroupName)" class="flex gap-2">
        <button
          v-if="displayName && materialNeeds.length > 0"
          class="btn-secondary text-xs py-1.5 flex-1"
          @click="craftingStore.exportMaterialList(displayName, materialNeeds)">
          Export
        </button>
        <button
          v-if="activeProject && !activeGroupName && !craftingStore.tracker?.active"
          class="btn-secondary text-xs py-1.5 flex-1"
          @click="craftingStore.startProjectTracking()">
          Start Tracking
        </button>
      </div>

      <!-- Pricing Summary (pricing mode only) -->
      <div v-if="pricingMode && pricingCalculation && pricingCalculation.materials.length > 0" class="flex flex-col gap-2 border-t border-surface-elevated pt-3">
        <h4 class="text-text-secondary text-xs font-semibold uppercase tracking-wide m-0">Pricing Summary</h4>

        <div class="flex flex-col gap-1 text-xs">
          <div class="flex justify-between">
            <span class="text-text-muted">Your material cost</span>
            <span class="text-text-primary">{{ formatGold(pricingCalculation.yourMaterialCost) }}</span>
          </div>
          <div v-if="pricingCalculation.theirMaterialValue > 0" class="flex justify-between">
            <span class="text-text-muted">Customer-supplied value</span>
            <span class="text-text-dim">{{ formatGold(pricingCalculation.theirMaterialValue) }}</span>
          </div>

          <div v-if="pricingCalculation.perCraftTotal > 0" class="flex justify-between">
            <span class="text-text-muted">Per-craft fee ({{ pricingCalculation.totalCrafts }}x)</span>
            <span class="text-accent-gold">{{ formatGold(pricingCalculation.perCraftTotal) }}</span>
          </div>
          <div v-if="pricingCalculation.materialPctFee > 0" class="flex justify-between">
            <span class="text-text-muted">Material % fee</span>
            <span class="text-accent-gold">{{ formatGold(pricingCalculation.materialPctFee) }}</span>
          </div>
          <div v-if="pricingCalculation.flatFee > 0" class="flex justify-between">
            <span class="text-text-muted">Flat fee</span>
            <span class="text-accent-gold">{{ formatGold(pricingCalculation.flatFee) }}</span>
          </div>

          <div v-if="pricingCalculation.totalFee > 0" class="flex justify-between border-t border-surface-dark pt-1">
            <span class="text-text-muted">Total fee</span>
            <span class="text-accent-gold font-medium">{{ formatGold(pricingCalculation.totalFee) }}</span>
          </div>
        </div>

        <!-- Bottom line -->
        <div class="flex justify-between items-baseline bg-surface-dark/60 rounded px-3 py-2 mt-1">
          <span class="text-text-secondary text-sm font-semibold">Charge Customer</span>
          <span class="text-accent-gold text-lg font-bold">{{ formatGold(pricingCalculation.chargeCustomer) }}</span>
        </div>

        <div v-if="pricingCalculation.hasUnknownPrices" class="text-text-dim text-[0.65rem] italic">
          * Some material prices are unknown — total may be incomplete.
        </div>
      </div>

      <!-- Live crafting panel (project only) -->
      <LiveCraftingPanel v-if="activeProject && !activeGroupName" />
    </template>
  </div>
</template>

<script setup lang="ts">
import { computed } from "vue";
import { useCraftingStore } from "../../stores/craftingStore";
import type { CraftingProject, CraftingProjectEntry, FlattenedMaterial, IntermediateCraft, MaterialNeed } from "../../types/crafting";
import type { PriceCalculation } from "../../composables/usePriceCalculator";
import { formatGold } from "../../composables/useRecipeCost";
import DataTable, { type ColumnDef } from "../Shared/DataTable.vue";
import EmptyState from "../Shared/EmptyState.vue";
import AccordionSection from "../Shared/AccordionSection.vue";
import LoadingSpinner from "../Shared/LoadingSpinner.vue";
import SkeletonLoader from "../Shared/SkeletonLoader.vue";
import DataTableSkeleton from "../Shared/DataTableSkeleton.vue";
import ItemInline from "../Shared/Item/ItemInline.vue";
import RecipeInline from "../Shared/Recipe/RecipeInline.vue";
import MaterialSummary from "./MaterialSummary.vue";
import PickupList from "./PickupList.vue";
import ShoppingList from "./ShoppingList.vue";
import LiveCraftingPanel from "./LiveCraftingPanel.vue";

const props = defineProps<{
  activeProject: CraftingProject | null
  activeGroupName: string | null
  groupProjectNames: string[]
  groupEntries: CraftingProjectEntry[]
  stockTargets: Map<number, { effectiveQty: number; currentStock: number }>
  materials: Map<string, FlattenedMaterial>
  intermediates: IntermediateCraft[]
  expandedItemIds: Set<number>
  intermediateStock: Map<number, number>
  materialNeeds: MaterialNeed[]
  resolving: boolean
  checkingAvailability: boolean
  pricingMode: boolean
  customerProvides: Record<string, number>
  pricingCalculation: PriceCalculation | null
  consumeBufferPct: number
}>();

const emit = defineEmits<{
  'resolve': []
  'toggle-intermediate': [itemId: number]
  'set-all-intermediates': [itemIds: number[], expand: boolean]
  'update-customer-provides': [key: string, quantity: number]
  'update-buffer': [pct: number]
}>();

function onBufferInput(event: Event) {
  const target = event.target as HTMLInputElement;
  emit('update-buffer', Math.max(0, Math.min(100, parseInt(target.value, 10) || 0)));
}

function stepBuffer(delta: number) {
  emit('update-buffer', Math.max(0, Math.min(100, props.consumeBufferPct + delta)));
}

function onCustomerProvidesChange(key: string, maxQty: number, event: Event) {
  const target = event.target as HTMLInputElement;
  const qty = Math.max(0, Math.min(Math.ceil(maxQty), parseFloat(target.value) || 0));
  emit('update-customer-provides', key, qty);
}

const craftingStore = useCraftingStore();
const displayName = computed(() =>
  props.activeGroupName ?? props.activeProject?.name ?? null,
);

/** The recipe entries to display in the summary */
const recipeEntries = computed(() => {
  if (props.activeGroupName) return props.groupEntries;
  return props.activeProject?.entries ?? [];
});

const hasContent = computed(() => props.materials.size > 0 || props.materialNeeds.length > 0 || props.intermediates.length > 0);

// Shorthands for template readability
const materials = computed(() => props.materials);

function getStockTarget(entryId: number) {
  return props.stockTargets.get(entryId) ?? null;
}

const sortedMaterials = computed(() => {
  return Array.from(props.materials.values())
    .sort((a, b) => {
      if (a.is_dynamic !== b.is_dynamic) return a.is_dynamic ? 1 : -1;
      return a.item_name.localeCompare(b.item_name);
    });
});

/** "They Give" column only exists while pricing mode is on, mirroring the original header `v-if`. */
const materialColumns = computed<ColumnDef[]>(() => {
  const cols: ColumnDef[] = [
    { key: "item_name", label: "Item", align: "left" },
    { key: "expected_quantity", label: "Qty", align: "right", width: "5rem", numeric: true },
  ];
  if (props.pricingMode) {
    cols.push({ key: "customer_provides", label: "They Give", align: "right", width: "5rem" });
  }
  return cols;
});

/** Highlight materials the customer is providing. */
function materialRowClass(row: Record<string, unknown>): string {
  return props.pricingMode && (props.customerProvides[row.key as string] ?? 0) > 0
    ? "bg-accent-green/5"
    : "";
}

interface CraftableItem {
  item_id: number
  item_name: string
  quantity: number
  have: number
  toCraft: number
  isExpanded: boolean
  intermediate: IntermediateCraft | null
}

/**
 * All craftable items across the project — both currently expanded (being crafted)
 * and not expanded (being bought). Includes availability info so users can see
 * "need 10, have 5, craft 5".
 */
const craftableItems = computed((): CraftableItem[] => {
  const items = new Map<number, CraftableItem>();

  // Build availability lookup from materialNeeds
  const availLookup = new Map<number, { have: number }>();
  for (const need of props.materialNeeds) {
    availLookup.set(need.item_id, { have: need.inventory_have + need.storage_have });
  }

  // Craftable items from flat materials (not currently expanded)
  for (const mat of props.materials.values()) {
    if (mat.item_id !== null && mat.is_craftable && !props.expandedItemIds.has(mat.item_id)) {
      const avail = availLookup.get(mat.item_id);
      const have = avail?.have ?? 0;
      items.set(mat.item_id, {
        item_id: mat.item_id,
        item_name: mat.item_name,
        quantity: mat.expected_quantity,
        have,
        toCraft: Math.max(0, mat.expected_quantity - have),
        isExpanded: false,
        intermediate: null,
      });
    }
  }

  // Currently expanded intermediates — intermediates are now deduplicated
  // with correct cross-entry totals for quantity_produced.
  // quantity_produced is the total needed (before stock subtraction);
  // the actual craft shortfall is computed here from stock on hand.
  for (const inter of props.intermediates) {
    if (items.has(inter.item_id)) continue;
    const onHand = props.intermediateStock.get(inter.item_id) ?? 0;
    items.set(inter.item_id, {
      item_id: inter.item_id,
      item_name: inter.item_name,
      quantity: inter.quantity_produced,
      have: onHand,
      toCraft: Math.max(0, inter.quantity_produced - onHand),
      isExpanded: true,
      intermediate: inter,
    });
  }

  // Sort: expanded first, then alphabetical
  return Array.from(items.values()).sort((a, b) => {
    if (a.isExpanded !== b.isExpanded) return a.isExpanded ? -1 : 1;
    return a.item_name.localeCompare(b.item_name);
  });
});

const craftableButNotExpanded = computed(() =>
  craftableItems.value.filter((i) => !i.isExpanded),
);

const coveredCount = computed(() => props.materialNeeds.filter((m) => m.shortfall === 0).length);
const partialCount = computed(() =>
  props.materialNeeds.filter((m) => m.shortfall > 0 && (m.inventory_have > 0 || m.storage_have > 0)).length
);
const missingCount = computed(() =>
  props.materialNeeds.filter((m) => m.shortfall > 0 && m.inventory_have === 0 && m.storage_have === 0).length
);

const hasPickupItems = computed(() =>
  props.materialNeeds.some((m) => m.storage_have > 0)
);

const pickupAreaCount = computed(() => {
  const areas = new Set<string>();
  for (const mat of props.materialNeeds) {
    if (mat.storage_have === 0) continue;
    for (const vs of mat.vault_breakdown) {
      if (vs.quantity > 0) areas.add(vs.vault_name);
    }
  }
  return areas.size;
});

const hasShortfalls = computed(() =>
  props.materialNeeds.some((m) => m.shortfall > 0)
);

const vendorCost = computed(() =>
  props.materialNeeds
    .filter((m) => m.shortfall > 0 && m.unit_price)
    .reduce((sum, m) => sum + (m.unit_price! * m.shortfall), 0)
);
</script>
