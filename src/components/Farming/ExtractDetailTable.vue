<template>
  <!-- Renders nothing unless this item was harvested via skinning/butchering. -->
  <div v-if="details.length > 0" class="flex flex-col gap-1.5">
    <div class="text-[0.6rem] uppercase tracking-widest text-[#c8a47e] font-bold">
      Harvested
    </div>
    <div v-for="(row, i) in details" :key="i" class="flex flex-col gap-0.5">
      <div class="flex items-center justify-between text-xs">
        <span class="text-text-secondary truncate">
          {{ row.corpse_name ?? 'Unknown corpse' }}
        </span>
        <span class="text-text-dim shrink-0">
          <span class="text-value-positive font-mono font-bold">x{{ row.total_quantity }}</span>
          <span class="text-[0.6rem]"> · {{ row.times }} {{ row.times === 1 ? 'pull' : 'pulls' }}</span>
        </span>
      </div>
      <div class="flex flex-wrap gap-1 text-[10px] leading-tight">
        <span v-if="row.skill_level != null" class="px-1 py-0.5 rounded bg-[#5a4a2a]/40 text-[#d6b87e]">
          {{ row.skill }} {{ row.skill_level }}
        </span>
        <span v-else class="px-1 py-0.5 rounded bg-[#5a4a2a]/40 text-[#d6b87e]">{{ row.skill }}</span>
        <span v-if="row.equipment_bonus != null" class="px-1 py-0.5 rounded bg-[#2a3a5a]/40 text-[#7ea4c8]">
          +{{ row.equipment_bonus }} equip
        </span>
        <span v-if="row.anatomy_family" class="px-1 py-0.5 rounded bg-[#3a5a3a]/40 text-[#9ec89e]">
          Anatomy: {{ row.anatomy_family }}<template v-if="row.anatomy_level != null"> {{ row.anatomy_level }}</template>
        </span>
      </div>
    </div>
    <p class="text-[10px] text-text-dim leading-tight">
      Skill level &amp; anatomy shown are your current (highest observed); equipment is the harvest bonus.
    </p>
  </div>
</template>

<script setup lang="ts">
import { ref, watch } from "vue";
import { invoke } from "@tauri-apps/api/core";
import type { ExtractDetail } from "../../types/farming";

const props = defineProps<{ itemName: string }>();

const details = ref<ExtractDetail[]>([]);

async function load() {
  try {
    details.value = await invoke<ExtractDetail[]>("get_corpse_extract_details", {
      itemName: props.itemName,
    });
  } catch (e) {
    console.error("[extract-detail-table] Failed to load:", e);
    details.value = [];
  }
}

watch(() => props.itemName, load, { immediate: true });
</script>
