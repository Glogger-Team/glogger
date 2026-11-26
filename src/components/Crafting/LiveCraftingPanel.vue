<template>
  <div v-if="store.tracker" class="flex flex-col gap-3 bg-surface-elevated rounded p-3 border border-border-light">
    <!-- Header -->
    <div class="flex items-center justify-between">
      <div class="flex items-center gap-2">
        <span
          class="w-2 h-2 rounded-full"
          :class="store.tracker.active ? 'bg-status-active animate-pulse' : 'bg-text-muted'" />
        <h4 class="panel-label m-0">
          Live Tracking
        </h4>
      </div>
      <div class="flex gap-2">
        <button
          v-if="store.tracker.active"
          class="text-text-muted text-[0.65rem] cursor-pointer bg-transparent border border-border-light rounded px-2 py-0.5 hover:text-text-primary"
          @click="store.stopTracking()">
          Pause
        </button>
        <button
          v-else
          class="text-accent-gold text-[0.65rem] cursor-pointer bg-transparent border border-accent-gold/30 rounded px-2 py-0.5 hover:bg-accent-gold/10"
          @click="store.tracker.active = true">
          Resume
        </button>
        <button
          class="text-accent-red/60 text-[0.65rem] cursor-pointer bg-transparent border-none hover:text-accent-red"
          @click="store.clearTracking()">
          Clear
        </button>
      </div>
    </div>

    <!-- Per-recipe progress -->
    <div class="flex flex-col gap-1.5">
      <div
        v-for="entry in store.tracker.entries"
        :key="entry.recipe_id"
        class="flex items-center gap-2 text-xs">
        <RecipeInline :reference="entry.recipe_name" class="shrink-0" />

        <!-- Progress bar -->
        <div class="flex-1 flex items-center gap-2">
          <div class="flex-1 bg-surface-dark rounded-full h-1.5 overflow-hidden">
            <div
              class="h-full rounded-full transition-all"
              :class="progressColor(entry)"
              :style="{ width: `${progressPercent(entry)}%` }" />
          </div>
        </div>

        <!-- Count -->
        <span class="text-text-primary font-mono text-[0.65rem] shrink-0">
          {{ effectiveOutput(entry) }} / {{ entry.target_quantity }}
        </span>

        <!-- Manual +/- buttons -->
        <div class="flex items-center gap-0.5 shrink-0">
          <button
            class="text-text-dim text-[0.6rem] w-4 h-4 flex items-center justify-center rounded hover:bg-surface-dark hover:text-text-primary cursor-pointer bg-transparent border-none"
            title="Subtract one craft"
            @click="store.adjustTrackedOutput(entry.recipe_id, -entry.output_per_craft)">
            -
          </button>
          <button
            class="text-text-dim text-[0.6rem] w-4 h-4 flex items-center justify-center rounded hover:bg-surface-dark hover:text-text-primary cursor-pointer bg-transparent border-none"
            title="Add one craft"
            @click="store.adjustTrackedOutput(entry.recipe_id, entry.output_per_craft)">
            +
          </button>
        </div>

        <!-- Complete badge -->
        <span
          v-if="effectiveOutput(entry) >= entry.target_quantity"
          class="text-value-positive text-[0.6rem] font-semibold">
          DONE
        </span>
      </div>
    </div>

    <!-- Recent detections log -->
    <div v-if="store.craftLog.length > 0" class="flex flex-col gap-0.5 max-h-24 overflow-y-auto">
      <div class="text-text-muted text-[0.6rem] uppercase tracking-wide">Recent</div>
      <div
        v-for="(evt, idx) in store.craftLog.slice(0, 10)"
        :key="idx"
        class="flex items-center gap-2 text-[0.65rem] text-text-dim">
        <span class="text-text-muted shrink-0">{{ formatTime(evt.timestamp) }}</span>
        <ItemInline :reference="evt.item_name" />
        <span v-if="evt.quantity > 0" class="text-accent-gold">+{{ evt.quantity }}</span>
      </div>
    </div>

    <!-- All complete notification -->
    <div
      v-if="allComplete"
      class="bg-green-900/20 border border-green-600/30 rounded px-3 py-2 text-xs text-green-300/80 text-center">
      All targets reached!
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed } from "vue";
import { useCraftingStore } from "../../stores/craftingStore";
import { formatAnyTimestamp } from "../../composables/useTimestamp";
import type { TrackedRecipeEntry } from "../../types/crafting";
import RecipeInline from "../Shared/Recipe/RecipeInline.vue";
import ItemInline from "../Shared/Item/ItemInline.vue";

const store = useCraftingStore();

function effectiveOutput(entry: TrackedRecipeEntry): number {
  return Math.max(0, entry.detected_output + entry.manual_adjustment);
}

const allComplete = computed(() => {
  if (!store.tracker) return false;
  return store.tracker.entries.every((e) => effectiveOutput(e) >= e.target_quantity);
});

function progressPercent(entry: TrackedRecipeEntry): number {
  if (entry.target_quantity === 0) return 0;
  return Math.min(100, Math.round((effectiveOutput(entry) / entry.target_quantity) * 100));
}

function progressColor(entry: TrackedRecipeEntry): string {
  const pct = progressPercent(entry);
  if (pct >= 100) return "bg-green-500";
  if (pct >= 50) return "bg-accent-gold";
  return "bg-blue-500";
}

function formatTime(timestamp: string): string {
  // Strip brackets from player event timestamps like "[16:17:48]"
  const bare = timestamp.startsWith("[") ? timestamp.slice(1, -1) : timestamp;
  return formatAnyTimestamp(bare);
}
</script>
