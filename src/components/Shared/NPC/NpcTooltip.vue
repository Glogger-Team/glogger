<template>
  <div class="mb-2">
    <div class="font-bold text-entity-npc text-sm mb-0.5">{{ npc.name }}</div>
    <div v-if="npc.area_friendly_name" class="text-entity-area text-xs">{{ npc.area_friendly_name }}</div>
  </div>

  <div v-if="npc.desc" class="text-text-secondary text-xs leading-relaxed mb-2 italic">
    {{ npc.desc }}
  </div>

  <div v-if="npc.trains_skills?.length" class="mb-2">
    <div class="text-text-muted text-[10px] uppercase tracking-wide mb-1">Trains</div>
    <div class="flex flex-wrap gap-1">
      <span
        v-for="skill in npc.trains_skills"
        :key="skill"
        class="bg-entity-skill/10 text-entity-skill px-1.5 py-0.5 rounded-sm text-[10px]"
      >
        {{ skill }}
      </span>
    </div>
  </div>

  <!-- Player-specific data -->
  <div v-if="hasPlayerData" class="border-t border-border-default/50 pt-1.5 mb-2 mt-1 space-y-1">
    <!-- Favor tier -->
    <div v-if="favorTier" class="flex items-center gap-1.5 text-xs">
      <span class="text-text-muted">Favor:</span>
      <span
        class="px-1.5 py-0.5 rounded-sm text-[10px] border"
        :class="favorBadgeClasses(favorTier)"
      >
        {{ tierDisplayName(favorTier) }}
      </span>
      <span v-if="snapshotFavor?.favor != null" class="text-text-dim text-[10px]">
        ({{ snapshotFavor.favor.toLocaleString() }})
      </span>
    </div>

    <!-- Favor progress -->
    <div v-if="favorTier && favorTier !== 'SoulMates' && favorProgressPercent != null && nextTierName" class="flex items-center gap-1.5">
      <div class="flex-1 h-1 bg-border-default rounded-sm overflow-hidden">
        <div class="h-full bg-accent-gold/60 rounded-sm" :style="{ width: favorProgressPercent + '%' }"></div>
      </div>
      <span class="text-[10px] text-text-dim shrink-0">&rarr; {{ tierDisplayName(nextTierName) }}: {{ favorProgressPercent }}%</span>
    </div>

    <!-- Vendor gold -->
    <div v-if="showVendor" class="flex items-center gap-1.5 text-xs flex-wrap">
      <span class="text-text-muted">Gold:</span>
      <span v-if="vendorData?.vendor_gold_available != null" class="text-text-primary">
        {{ vendorData.vendor_gold_available.toLocaleString() }}
        <span v-if="vendorData.vendor_gold_max != null" class="text-text-dim">
          / {{ vendorData.vendor_gold_max.toLocaleString() }}
        </span>
      </span>
      <span v-else-if="snapshotFavor?.councils_spent_at_vendor != null" class="text-text-dim">
        spent {{ snapshotFavor.councils_spent_at_vendor.toLocaleString() }} (snapshot)
      </span>
      <span v-else class="text-text-dim">unknown</span>
      <span v-if="timerRemaining" class="text-text-dim text-[10px]">
        (resets in {{ timerRemaining }})
      </span>
    </div>

    <!-- Vendor item types -->
    <div v-if="showVendor && vendorItemTypes.length" class="text-[10px] text-text-dim truncate">
      Buys: {{ vendorItemTypes.join(', ') }}
    </div>

    <!-- Storage usage -->
    <div v-if="showStorage && storageUsed != null" class="flex items-center gap-1.5 text-xs">
      <span class="text-text-muted">Storage:</span>
      <span class="text-text-primary">
        {{ storageUsed }} <span v-if="storageTotal != null" class="text-text-dim">/ {{ storageTotal }}</span> slots
      </span>
    </div>
  </div>

  <div v-if="topPrefs.length" class="mb-1">
    <div class="text-text-muted text-[10px] uppercase tracking-wide mb-1">Preferences</div>
    <div v-for="pref in topPrefs" :key="pref.name ?? pref.desire" class="text-xs flex gap-1.5 py-0.5">
      <span :class="desireColor(pref.desire)">{{ pref.desire }}</span>
      <span class="text-text-secondary">{{ pref.name ?? pref.keywords?.join(', ') }}</span>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed } from "vue";
import type { NpcInfo } from "../../../types/gameData";
import { useGameStateStore } from "../../../stores/gameStateStore";
import { useCharacterStore } from "../../../stores/characterStore";
import { FAVOR_TIERS, favorBadgeClasses, tierDisplayName, pointsToNextTier } from "../../../composables/useFavorTiers";
import { hasVendor, hasStorage, goldCapAtTier, getStoreService } from "../../../composables/useNpcServices";

const props = defineProps<{
  npc: NpcInfo;
}>();

const gameState = useGameStateStore();
const characterStore = useCharacterStore();

const topPrefs = computed(() =>
  props.npc.preferences?.slice(0, 5) ?? []
);

// ── Favor ──────────────────────────────────────────────────────────
const favorTier = computed<string | null>(() => {
  const gsf = gameState.favorByNpc[props.npc.key];
  if (gsf?.favor_tier) return gsf.favor_tier;
  const snap = characterStore.npcFavor.find(f => f.npc_key === props.npc.key);
  return snap?.favor_level ?? null;
});

/** Snapshot fallback values (parenthetical, live data keeps precedence). */
const snapshotFavor = computed(() =>
  characterStore.npcFavor.find(f => f.npc_key === props.npc.key) ?? null,
);

// ── Favor progress ────────────────────────────────────────────────
const gamestateFavor = computed(() => gameState.favorByNpc[props.npc.key] ?? null);

const nextTierName = computed(() => {
  if (!favorTier.value || favorTier.value === 'SoulMates') return null;
  const idx = FAVOR_TIERS.indexOf(favorTier.value as any);
  if (idx <= 0) return null;
  return FAVOR_TIERS[idx - 1];
});

const favorProgressPercent = computed(() => {
  if (!favorTier.value || favorTier.value === 'SoulMates') return null;
  const needed = pointsToNextTier(favorTier.value);
  if (!needed) return null;
  const delta = gamestateFavor.value?.cumulative_delta ?? 0;
  if (delta <= 0) return 0;
  return Math.min(100, Math.round((delta / needed) * 100));
});

// ── Vendor ─────────────────────────────────────────────────────────
const showVendor = computed(() => hasVendor(props.npc));
const vendorData = computed(() => gameState.vendorByNpc[props.npc.key] ?? null);

const vendorItemTypes = computed<string[]>(() => {
  if (!showVendor.value) return [];
  if (favorTier.value) {
    const cap = goldCapAtTier(props.npc, favorTier.value);
    if (cap) return cap.itemTypes;
  }
  const store = getStoreService(props.npc);
  if (store && store.capIncreases.length) return store.capIncreases[0].itemTypes;
  return [];
});

const timerRemaining = computed(() => {
  if (!vendorData.value?.vendor_gold_timer_start) return null;
  const start = new Date(vendorData.value.vendor_gold_timer_start + 'Z');
  const resetAt = new Date(start.getTime() + 168 * 60 * 60 * 1000); // 168 hours
  const now = new Date();
  const remaining = resetAt.getTime() - now.getTime();
  if (remaining <= 0) return null;
  const hours = Math.floor(remaining / (1000 * 60 * 60));
  const days = Math.floor(hours / 24);
  const remainingHours = hours % 24;
  return days > 0 ? `~${days}d ${remainingHours}h` : `~${hours}h`;
});

// ── Storage ────────────────────────────────────────────────────────
const showStorage = computed(() => hasStorage(props.npc));

const storageUsed = computed<number | null>(() => {
  const items = gameState.storageByVault[props.npc.key];
  return items ? items.length : null;
});

const storageTotal = computed<number | null>(() => {
  const vault = gameState.storageVaultsByKey[props.npc.key];
  if (!vault) return null;
  return gameState.getVaultUnlockedSlots(vault);
});

// ── Show separator only when there is player data ──────────────────
const hasPlayerData = computed(() =>
  favorTier.value != null ||
  (showVendor.value && vendorData.value != null) ||
  (showStorage.value && storageUsed.value != null)
);

function desireColor(desire: string): string {
  switch (desire.toLowerCase()) {
    case "love": return "text-accent-red";
    case "like": return "text-accent-green";
    case "hate": return "text-accent-red";
    default: return "text-text-muted";
  }
}
</script>
