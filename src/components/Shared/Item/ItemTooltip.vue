<template>
  <div class="flex gap-2 items-start mb-2">
    <img
      v-if="iconSrc"
      :src="iconSrc"
      :alt="item.name"
      class="w-8 h-8 object-contain bg-black/30 border border-border-light rounded shrink-0" />
    <div class="flex-1">
      <div class="font-bold text-entity-item text-sm mb-0.5">{{ item.name }}</div>
      <div class="flex flex-wrap gap-x-3 gap-y-0.5">
        <div v-if="item.value" class="text-accent-gold text-xs">
          Vendor: {{ item.value }}g
        </div>
        <div v-if="item.value" class="text-text-muted text-xs">
          Buy Used: {{ Math.round(item.value * 2) }}g
        </div>
        <div v-if="marketValue !== null" class="text-accent-green text-xs">
          Market: {{ marketValue.toLocaleString() }}g
          <span class="text-text-muted ml-0.5">({{ marketStaleness }})</span>
        </div>
      </div>
      <div v-if="effectiveValue !== null && showEffectiveValue" class="text-text-secondary text-xs mt-0.5">
        Effective: {{ effectiveValue.toLocaleString() }}g
        <span class="text-text-dim">({{ valuationModeLabel }})</span>
      </div>
      <div v-if="stallObservation" class="text-text-secondary text-xs mt-0.5">
        Last seen: {{ stallObservation.quantity }}x {{ stallObservation.price_unit === 0 ? 'price?' : stallObservation.price_unit.toLocaleString() + 'g' }} at {{ stallObservation.stall_label }}
        <span class="text-text-dim">({{ formatRelative(stallObservation.observed_at) }})</span>
      </div>
    </div>
  </div>

  <div v-if="item.description" class="text-text-secondary text-xs leading-relaxed mb-2 italic">
    {{ item.description }}
  </div>

  <div v-if="item.keywords?.length" class="flex flex-wrap gap-1 mb-2">
    <span
      v-for="keyword in item.keywords"
      :key="keyword"
      class="bg-entity-item/10 text-entity-item px-1.5 py-0.5 rounded-sm text-[0.65rem] uppercase tracking-wide"
    >
      {{ keyword }}
    </span>
  </div>

  <div v-if="resolvedEffects.length" class="mb-2">
    <div
      v-for="(effect, i) in resolvedEffects"
      :key="i"
      class="text-accent-green text-xs leading-relaxed pl-2 relative before:content-['•'] before:absolute before:left-0"
    >
      {{ effect.formatted }}
    </div>
  </div>

  <div v-if="vendorNpcs.length" class="mt-2 pt-2 border-t border-[#2a2a3e]">
    <div class="text-text-muted text-[0.65rem] uppercase tracking-wide mb-1">Sold by</div>
    <div class="flex flex-wrap gap-x-0.5 gap-y-0 text-xs">
      <template v-for="(npc, idx) in vendorNpcs" :key="npc.npc_key">
        <NpcInline :reference="npc.npc_key" /><span v-if="idx < vendorNpcs.length - 1" class="text-text-dim">,</span>
      </template>
    </div>
  </div>

  <div v-if="interestedNpcs.length" class="mt-2 pt-2 border-t border-[#2a2a3e]">
    <div class="text-text-muted text-[0.65rem] uppercase tracking-wide mb-1">Wanted as gift by</div>
    <div v-for="entry in interestedNpcs" :key="entry.npc.key" class="flex items-center gap-1.5 text-xs leading-relaxed">
      <NpcInline :reference="entry.npc.key" />
      <span
        class="text-[0.6rem] px-1 rounded-sm font-semibold"
        :class="entry.bestPref.desire === 'Love' ? 'bg-emerald-500/20 text-emerald-400' : 'bg-sky-500/20 text-sky-400'"
      >
        {{ entry.bestPref.desire }}
      </span>
      <span class="text-text-dim text-[0.6rem]">+{{ entry.bestPref.pref }}</span>
    </div>
  </div>

  <div v-if="matchingVendors.length" class="mt-2 pt-2 border-t border-[#2a2a3e]">
    <div class="text-text-muted text-[0.65rem] uppercase tracking-wide mb-1">Vendors buying this</div>
    <div v-for="v in matchingVendors" :key="v.npc.key" class="flex items-center gap-1.5 text-xs leading-relaxed">
      <NpcInline :reference="v.npc.key" />
      <span class="text-text-dim text-[0.6rem]">
        <template v-if="v.currentGold !== null">{{ v.currentGold.toLocaleString() }} / {{ v.maxGold.toLocaleString() }}g</template>
        <template v-else>{{ v.maxGold.toLocaleString() }}g cap</template>
      </span>
    </div>
  </div>

  <div v-if="(item.max_stack_size && item.max_stack_size > 1) || ownedCount > 0 || woState" class="text-text-muted text-[0.7rem] mt-2 pt-2 border-t border-[#2a2a3e] flex justify-between">
    <span v-if="item.max_stack_size && item.max_stack_size > 1">Max Stack: {{ item.max_stack_size }}</span>
    <span v-if="ownedCount > 0" class="text-accent-gold">Owned: {{ ownedCount.toLocaleString() }}</span>
    <span v-if="woState" class="text-text-secondary"><template v-if="woState.completedBy.length">Done: {{ woState.completedBy.join(', ') }}</template><template v-if="woState.completedBy.length && woState.activeBy.length"> • </template><template v-if="woState.activeBy.length">Active: {{ woState.activeBy.join(', ') }}</template></span>
  </div>

  <!-- Set Market Value -->
  <div class="text-[0.7rem] mt-1.5 pt-1.5 border-t border-[#2a2a3e]">
    <div v-if="!editingMarket" class="flex items-center gap-2">
      <button
        class="text-text-muted hover:text-text-primary bg-transparent border-none cursor-pointer text-[0.65rem] p-0 underline"
        @click.stop="startEditMarket">
        {{ marketValue !== null ? 'Edit market value' : 'Set market value' }}
      </button>
      <button
        v-if="marketValue !== null"
        class="text-text-muted hover:text-red-400 bg-transparent border-none cursor-pointer text-[0.65rem] p-0 underline"
        @click.stop="removeMarketValue">
        Remove
      </button>
    </div>
    <div v-else class="flex items-center gap-1" @click.stop>
      <input
        ref="marketInput"
        v-model="marketEditValue"
        type="number"
        min="0"
        class="w-20 bg-surface-dark border border-border-default rounded px-1 py-0.5 text-[0.65rem] text-text-primary"
        placeholder="Price"
        @keydown.enter="saveMarketValue"
        @keydown.escape="editingMarket = false" />
      <span class="text-text-muted">g</span>
      <button
        class="text-accent-green hover:text-green-400 bg-transparent border-none cursor-pointer text-[0.65rem] px-1"
        @click.stop="saveMarketValue">
        Save
      </button>
      <button
        class="text-text-muted hover:text-text-primary bg-transparent border-none cursor-pointer text-[0.65rem] px-1"
        @click.stop="editingMarket = false">
        Cancel
      </button>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, watch, nextTick } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { useGameDataStore } from "../../../stores/gameDataStore";
import { useGameStateStore } from "../../../stores/gameStateStore";
import { useMarketStore } from "../../../stores/marketStore";
import { useStallPriceStore } from "../../../stores/stallPriceStore";
import { useSettingsStore } from "../../../stores/settingsStore";
import { useCharacterStore } from "../../../stores/characterStore";
import { formatStaleness, formatRelative } from "../../../composables/useTimestamp";
import { findInterestedNpcs } from "../../../composables/useNpcGiftMatching";
import { getStoreService, maxGoldCap } from "../../../composables/useNpcServices";
import type { ItemInfo } from "../../../types/gameData";
import type { NpcInfo } from "../../../types/gameData/npcs";
import NpcInline from "../NPC/NpcInline.vue";

interface VendorNpc {
  npc_key: string;
  name: string;
}

interface ResolvedEffect {
  label: string
  value: string
  display_type: string
  formatted: string
  icon_id: number | null
}

const props = defineProps<{
  item: ItemInfo;
  iconSrc: string | null;
}>();

const gameDataStore = useGameDataStore();
const gameStateStore = useGameStateStore();
const marketStore = useMarketStore();
const stallPriceStore = useStallPriceStore();
const settingsStore = useSettingsStore();
const characterStore = useCharacterStore();
const ownedCount = computed(() => gameStateStore.ownedItemCounts[props.item.name] ?? 0);
const woKey = computed(() => props.item.bestow_quest);
const woState = computed(() => characterStore.workOrderStateFor(woKey.value));

// NPC gift matching — find NPCs interested in this item's keywords
const allNpcs = computed<NpcInfo[]>(() => Object.values(gameDataStore.npcsByKey))

const interestedNpcs = computed(() => {
  const keywords = props.item.keywords
  if (!keywords?.length) return []
  return findInterestedNpcs(keywords, allNpcs.value).slice(0, 5)
})

// Vendor matching — find vendors whose store accepts this item's keywords
const matchingVendors = computed(() => {
  const keywords = props.item.keywords
  if (!keywords?.length) return []
  const keywordsLower = new Set(keywords.map(k => k.toLowerCase()))

  const results: { npc: NpcInfo; currentGold: number | null; maxGold: number }[] = []

  for (const npc of allNpcs.value) {
    const store = getStoreService(npc)
    if (!store) continue

    // Check if any CapIncrease itemType matches an item keyword
    const accepts = store.capIncreases.some(cap =>
      cap.itemTypes.some(t => keywordsLower.has(t.toLowerCase()))
    )
    if (!accepts) continue

    const bestCap = maxGoldCap(npc)
    if (!bestCap) continue

    const vendorState = gameStateStore.vendorByNpc[npc.key]
    results.push({
      npc,
      currentGold: vendorState?.vendor_gold_available ?? null,
      maxGold: bestCap.maxGold,
    })
  }

  // Sort by max gold descending, show top 5
  return results.sort((a, b) => b.maxGold - a.maxGold).slice(0, 5)
})

// Market value
const marketEntry = computed(() => marketStore.valuesByItemId[props.item.id] ?? null);
const marketValue = computed(() => marketEntry.value?.market_value ?? null);

// Most recent stall observation (other players' stalls) for this item.
const stallObservation = computed(() => stallPriceStore.observationsByItemId[props.item.id] ?? null);
const marketStaleness = computed(() => {
  if (!marketEntry.value) return ''
  return formatStaleness(marketEntry.value.updated_at)
})

// Effective value based on valuation mode
const effectiveValue = computed(() => {
  const vendor = props.item.value ?? 0
  const market = marketValue.value ?? 0
  const mode = settingsStore.settings.itemValuationMode
  switch (mode) {
    case 'highest_market_vendor': return Math.max(market, vendor)
    case 'highest_market_buy_used': return Math.max(market, vendor * 2)
    case 'vendor_only': return vendor
    case 'buy_used_only': return vendor * 2
    case 'market_only': return market
    default: return Math.max(market, vendor)
  }
})

const showEffectiveValue = computed(() => {
  // Only show if there's at least one source value and the effective differs from the obvious single value
  const vendor = props.item.value ?? 0
  const market = marketValue.value ?? 0
  if (vendor === 0 && market === 0) return false
  // Always show when both values exist, or when mode transforms vendor (buy_used)
  const mode = settingsStore.settings.itemValuationMode
  if (market > 0 && vendor > 0) return true
  if (mode === 'buy_used_only' || mode === 'highest_market_buy_used') return vendor > 0
  return false
})

const valuationModeLabel = computed(() => {
  const labels: Record<string, string> = {
    'highest_market_vendor': 'highest of market/vendor',
    'highest_market_buy_used': 'highest of market/buy-used',
    'vendor_only': 'vendor only',
    'buy_used_only': 'buy-used only',
    'market_only': 'market only',
  }
  return labels[settingsStore.settings.itemValuationMode] ?? 'default'
})

const editingMarket = ref(false)
const marketEditValue = ref('')
const marketInput = ref<HTMLInputElement | null>(null)

function startEditMarket() {
  marketEditValue.value = marketValue.value?.toString() ?? ''
  editingMarket.value = true
  nextTick(() => marketInput.value?.focus())
}

async function saveMarketValue() {
  const val = parseInt(marketEditValue.value)
  if (isNaN(val) || val < 0) return
  await marketStore.setValue(props.item.id, props.item.name, val)
  editingMarket.value = false
}

async function removeMarketValue() {
  await marketStore.deleteValue(props.item.id)
}

// Effects
const resolvedEffects = ref<ResolvedEffect[]>([]);

async function resolveEffects() {
  if (!props.item.effect_descs?.length) {
    resolvedEffects.value = [];
    return;
  }
  try {
    resolvedEffects.value = await invoke<ResolvedEffect[]>('resolve_effect_descs', {
      descs: props.item.effect_descs,
    });
  } catch {
    // Fallback to raw strings
    resolvedEffects.value = props.item.effect_descs.map(d => ({
      label: d, value: '', display_type: '', formatted: d, icon_id: null,
    }));
  }
}

// Vendor NPCs
const vendorNpcs = ref<VendorNpc[]>([]);

async function loadVendorNpcs() {
  try {
    vendorNpcs.value = await invoke<VendorNpc[]>('get_vendors_for_item', {
      itemId: props.item.id,
    });
  } catch {
    vendorNpcs.value = [];
  }
}

watch(() => props.item, () => {
  resolveEffects();
  loadVendorNpcs();
  void characterStore.loadAllCharacterWorkOrders();
}, { immediate: true });
</script>
