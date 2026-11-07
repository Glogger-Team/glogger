<template>
  <div class="flex flex-col gap-2 w-80 max-w-[22rem]">
    <!-- Header -->
    <div class="flex items-center justify-between border-b border-border-default pb-1.5">
      <ItemInline :reference="itemName" />
      <span class="text-[0.6rem] text-text-muted uppercase tracking-wide shrink-0">
        {{ verb }} <span class="text-value-positive font-bold">{{ totalLooted }}</span> this session
      </span>
    </div>

    <div v-if="rows.length === 0" class="text-xs text-text-dim italic py-1">
      No source data for this item yet.
    </div>

    <!-- Per-enemy breakdown -->
    <div v-else class="flex flex-col gap-2">
      <div
        v-for="row in rows"
        :key="row.enemyName"
        class="flex flex-col gap-1">
        <!-- Source (enemy for loot; node/survey for gathered) + session tally -->
        <div class="flex items-center justify-between text-xs">
          <EnemyInline v-if="!isExtract" :reference="row.enemyName" />
          <span v-else class="text-text-secondary truncate">{{ row.enemyName }}</span>
          <span class="text-text-dim shrink-0">
            <span class="text-value-positive font-mono font-bold">x{{ row.sessionQuantity }}</span>
            <span v-if="isExtract" class="text-[0.6rem]"> · {{ row.sessionDrops }} {{ row.sessionDrops === 1 ? 'pull' : 'pulls' }}</span>
            <span v-else-if="row.sessionKills > 0" class="text-[0.6rem]"> · {{ row.sessionDrops }}/{{ row.sessionKills }} kills</span>
            <span v-else class="text-[0.6rem]"> · {{ row.sessionDrops }} {{ row.sessionDrops === 1 ? 'drop' : 'drops' }} (kill not tracked)</span>
          </span>
        </div>

        <!-- Harvest conditions (skinning/butchering only): skill level at
             harvest, equipment bonus, and anatomy skill for the monster type. -->
        <div
          v-if="props.mode === 'extract' && hasHarvestDetail(row)"
          class="flex flex-wrap gap-1 text-[10px] leading-tight">
          <span v-if="row.skillLevel != null" class="px-1 py-0.5 rounded bg-[#5a4a2a]/40 text-[#d6b87e]">
            {{ row.skill ?? 'Skill' }} {{ row.skillLevel }}
          </span>
          <span v-if="row.equipmentBonus != null" class="px-1 py-0.5 rounded bg-[#2a3a5a]/40 text-[#7ea4c8]">
            +{{ row.equipmentBonus }} equip
          </span>
          <span v-if="row.anatomyFamily" class="px-1 py-0.5 rounded bg-[#3a5a3a]/40 text-[#9ec89e]">
            Anatomy: {{ row.anatomyFamily }}<template v-if="row.anatomyLevel != null"> {{ row.anatomyLevel }}</template>
          </span>
        </div>

        <!-- Bar: all-time drop rate (loot) or session share (extract) -->
        <div class="relative h-4 rounded bg-black/40 border border-border-default overflow-hidden">
          <div
            class="absolute inset-y-0 left-0 transition-[width] duration-300"
            :class="isExtract ? 'bg-[#5a4a2a]' : 'bg-[#3a5a3a]'"
            :style="{ width: barWidth(row) }" />
          <div class="relative z-10 flex items-center justify-between px-1.5 h-full text-[0.6rem] font-mono">
            <span v-if="isExtract" class="text-text-secondary">
              {{ sharePct(row) }} of {{ itemName }} this session
            </span>
            <template v-else>
              <span class="text-text-secondary">
                {{ row.allTimeDropRate === null ? '…' : fmtPct(row.allTimeDropRate) }} drop rate
                <span class="text-text-dim">(all-time)</span>
              </span>
              <span v-if="row.lootTableSharePct !== null" class="text-text-dim">
                {{ row.lootTableSharePct.toFixed(1) }}% of table
              </span>
            </template>
          </div>
        </div>
      </div>

      <p class="text-[10px] text-text-dim leading-tight pt-0.5">
        <template v-if="props.mode === 'gathered'">
          Mining/survey yield — not a loot-table drop, so there’s no lifetime drop rate.
        </template>
        <template v-else-if="props.mode === 'extract'">
          Skinning/butchering yield — not a loot-table drop, so there’s no lifetime drop rate.
        </template>
        <template v-else>
          Drop rate = kills that dropped this item ÷ all kills of that enemy (lifetime).
          “% of table” = this item’s share of everything that enemy drops, by quantity.
        </template>
      </p>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, watch } from "vue";
import { useFarmingStore } from "../../stores/farmingStore";
import type { ItemDropBreakdownRow } from "../../types/farming";
import ItemInline from "../Shared/Item/ItemInline.vue";
import EnemyInline from "../Shared/Enemy/EnemyInline.vue";

const props = withDefaults(
  defineProps<{
    itemName: string;
    totalLooted: number;
    mode?: "loot" | "extract" | "gathered";
  }>(),
  { mode: "loot" }
);

const store = useFarmingStore();
const rows = ref<ItemDropBreakdownRow[]>([]);

// "extract" (skinning/butchering) and "gathered" (mining/survey) are both
// session-only categories with a plain-text source label (no enemy entity,
// no lifetime DB data) — they render identically, just from different store
// accessors.
const isExtract = computed(() => props.mode !== "loot");
const verb = computed(() => (props.mode === "gathered" ? "Gathered" : props.mode === "extract" ? "Extracted" : "Looted"));

function hasHarvestDetail(row: ItemDropBreakdownRow): boolean {
  return (
    row.skillLevel != null ||
    row.equipmentBonus != null ||
    row.anatomyFamily != null
  );
}

async function build() {
  // Seed rows with this-session data immediately.
  const sessionRows =
    props.mode === "gathered"
      ? store.sessionSourcesForGathered(props.itemName)
      : props.mode === "extract"
        ? store.sessionEnemiesForExtract(props.itemName)
        : store.sessionEnemiesForItem(props.itemName);
  rows.value = sessionRows.map((r) => ({
    ...r,
    allTimeKills: null,
    allTimeDropRate: null,
    lootTableSharePct: null,
  }));

  // Extracts/gathered have no loot-table / all-time data — session view only.
  if (isExtract.value) return;

  // Layer all-time figures from the DB onto each enemy row.
  await Promise.all(
    rows.value.map(async (row) => {
      const stats = await store.fetchEnemyStats(row.enemyName);
      if (!stats) return;
      const lootStat = stats.loot.find((l) => l.item_name === props.itemName);
      const tableTotalQty = stats.loot.reduce((sum, l) => sum + l.total_quantity, 0);
      row.allTimeKills = stats.total_kills;
      row.allTimeDropRate = lootStat ? lootStat.drop_rate : 0;
      row.lootTableSharePct =
        lootStat && tableTotalQty > 0
          ? (lootStat.total_quantity / tableTotalQty) * 100
          : 0;
    })
  );
}

function fmtPct(rate: number): string {
  const pct = rate * 100;
  return pct >= 10 ? `${pct.toFixed(0)}%` : `${pct.toFixed(1)}%`;
}

function sharePct(row: ItemDropBreakdownRow): string {
  if (props.totalLooted <= 0) return "0%";
  const pct = (row.sessionQuantity / props.totalLooted) * 100;
  return pct >= 10 ? `${pct.toFixed(0)}%` : `${pct.toFixed(1)}%`;
}

function barWidth(row: ItemDropBreakdownRow): string {
  if (isExtract.value) {
    if (props.totalLooted <= 0) return "0%";
    return `${Math.min(100, Math.max(2, (row.sessionQuantity / props.totalLooted) * 100))}%`;
  }
  if (row.allTimeDropRate === null) return "0%";
  return `${Math.min(100, Math.max(2, row.allTimeDropRate * 100))}%`;
}

watch(() => [props.itemName, props.mode], build, { immediate: true });
</script>
