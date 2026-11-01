<template>
  <EntityTooltipWrapper
    border-class="border-entity-item/50"
    :interactive="true"
    @hover="loadData"
  >
    <div
      class="flex items-center gap-2 cursor-pointer transition-all duration-150 hover:bg-black/30 rounded-md px-2 py-1"
      :class="[widthClass, bordered ? 'bg-black/15 border border-border-default hover:border-border-hover' : '']"
      @click="handleClick"
    >
      <GameIcon
        v-if="showIcon"
        :icon-id="itemData?.icon_id"
        :alt="itemData?.name ?? reference"
        size="md"
        class="shrink-0"
      />
      <div class="flex-1 min-w-0">
        <div class="text-text-primary text-xs font-medium truncate">
          {{ itemData?.name ?? reference }}
        </div>
        <div class="flex items-center gap-2 text-[10px] leading-tight mt-0.5">
          <span v-if="vendorPrice !== null" class="text-accent-gold">{{ vendorPrice }}g</span>
          <span
            v-if="marketPrice !== null"
            class="text-accent-green"
          >{{ marketPrice.toLocaleString() }}g</span>
          <button
            v-else
            class="text-text-dim hover:text-accent-green bg-transparent border-none cursor-pointer p-0 text-[10px] underline"
            @click.stop="startSetMarket"
            title="Set market price"
          >set price</button>
          <span v-if="ownedCount > 0" class="text-text-secondary">×{{ ownedCount.toLocaleString() }}</span>
        </div>
      </div>
    </div>

    <!-- Inline market value editor (shown above the card) -->
    <Teleport to="body">
      <div
        v-if="editingMarket"
        class="fixed inset-0 z-[10000]"
        @click="editingMarket = false"
      >
        <div
          class="absolute bg-surface-dark border border-border-default rounded-md shadow-lg p-2 flex items-center gap-1 z-[10001]"
          :style="editPopupStyle"
          @click.stop
        >
          <input
            ref="marketInput"
            v-model="marketEditValue"
            type="number"
            min="0"
            class="w-20 bg-black/30 border border-border-default rounded px-1.5 py-0.5 text-xs text-text-primary"
            placeholder="Price"
            @keydown.enter="saveMarketValue"
            @keydown.escape="editingMarket = false"
          />
          <span class="text-text-muted text-xs">g</span>
          <button
            class="text-accent-green hover:text-green-400 bg-transparent border-none cursor-pointer text-xs px-1"
            @click="saveMarketValue"
          >Save</button>
          <button
            class="text-text-muted hover:text-text-primary bg-transparent border-none cursor-pointer text-xs px-1"
            @click="editingMarket = false"
          >Cancel</button>
        </div>
      </div>
    </Teleport>

    <template #tooltip>
      <ItemTooltip v-if="itemData" :item="itemData" :icon-src="iconSrc" />
    </template>
  </EntityTooltipWrapper>
</template>

<script setup lang="ts">
import { ref, computed, onMounted, watch, nextTick } from "vue";
import { convertFileSrc } from "@tauri-apps/api/core";
import { useGameDataStore } from "../../../stores/gameDataStore";
import { useGameStateStore } from "../../../stores/gameStateStore";
import { useMarketStore } from "../../../stores/marketStore";
import { useEntityNavigation } from "../../../composables/useEntityNavigation";
import type { ItemInfo } from "../../../types/gameData";
import EntityTooltipWrapper from "../EntityTooltipWrapper.vue";
import GameIcon from "../GameIcon.vue";
import ItemTooltip from "./ItemTooltip.vue";

const props = withDefaults(defineProps<{
  reference: string;
  showIcon?: boolean;
  bordered?: boolean;
  /** Card width behavior: "fixed" = standard 11rem, "min" = at least 11rem, "max" = at most 11rem */
  width?: "fixed" | "min" | "max";
}>(), {
  showIcon: true,
  bordered: true,
  width: "fixed",
});

const widthClass = computed(() => {
  switch (props.width) {
    case "min": return "min-w-44";
    case "max": return "max-w-44";
    default: return "w-44";
  }
});

const store = useGameDataStore();
const gameStateStore = useGameStateStore();
const marketStore = useMarketStore();
const { navigateToEntity } = useEntityNavigation();

const itemData = ref<ItemInfo | null>(null);
const iconSrc = ref<string | null>(null);

// Pricing
const vendorPrice = computed(() => itemData.value?.value ?? null);
const marketEntry = computed(() =>
  itemData.value ? (marketStore.valuesByItemId[itemData.value.id] ?? null) : null
);
const marketPrice = computed(() => marketEntry.value?.market_value ?? null);
const ownedCount = computed(() =>
  itemData.value ? (gameStateStore.ownedItemCounts[itemData.value.name] ?? 0) : 0
);

// Market editing
const editingMarket = ref(false);
const marketEditValue = ref("");
const marketInput = ref<HTMLInputElement | null>(null);
const editPopupStyle = ref<Record<string, string>>({});

function startSetMarket(event: MouseEvent) {
  const target = event.target as HTMLElement;
  const rect = target.getBoundingClientRect();
  const popupWidth = 200;
  const popupHeight = 60;
  const maxLeft = window.innerWidth - popupWidth - 4;
  const maxTop = window.innerHeight - popupHeight - 4;
  editPopupStyle.value = {
    top: `${Math.max(4, Math.min(rect.bottom + 4, maxTop))}px`,
    left: `${Math.max(4, Math.min(rect.left, maxLeft))}px`,
  };
  marketEditValue.value = marketPrice.value?.toString() ?? "";
  editingMarket.value = true;
  nextTick(() => marketInput.value?.focus());
}

async function saveMarketValue() {
  if (!itemData.value) return;
  const val = parseInt(marketEditValue.value);
  if (isNaN(val) || val < 0) return;
  await marketStore.setValue(itemData.value.id, itemData.value.name, val);
  editingMarket.value = false;
}

async function loadData() {
  if (itemData.value) return;
  try {
    const item = await store.resolveItem(props.reference);
    if (!item) return;
    itemData.value = item;
    if (item.icon_id) {
      const path = await store.getIconPath(item.icon_id);
      iconSrc.value = convertFileSrc(path);
    }
  } catch (e) {
    console.warn(`Failed to resolve item: ${props.reference}`, e);
  }
}

onMounted(loadData);

watch(() => props.reference, () => {
  itemData.value = null;
  iconSrc.value = null;
  loadData();
});

function handleClick() {
  const id = itemData.value?.name ?? props.reference;
  navigateToEntity({ type: "item", id });
}
</script>
