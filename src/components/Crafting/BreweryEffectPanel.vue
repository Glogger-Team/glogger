<template>
  <div class="flex flex-col h-full min-h-0">
    <!-- Search -->
    <div class="px-2 pt-1 pb-1">
      <input
        v-model="store.effectSearchQuery"
        type="text"
        placeholder="Search by effect..."
        class="input text-xs w-full" />
    </div>

    <!-- No discoveries state -->
    <div v-if="store.effectEntries.length === 0" class="px-2 py-4 text-center">
      <div class="text-text-dim text-xs italic">
        No effects discovered yet.
      </div>
      <div class="text-text-dim text-xs mt-1">
        Scan Storage reports or import a CSV to populate.
      </div>
    </div>

    <!-- Effect list -->
    <div v-else class="flex-1 min-h-0 overflow-y-auto">
      <button
        v-for="effect in store.filteredEffects"
        :key="effect.power"
        class="flex flex-col gap-0.5 px-2 py-1.5 text-left cursor-pointer border-none w-full border-b border-surface-card"
        :class="store.selectedEffect === effect.power
          ? 'bg-accent-gold/15'
          : 'bg-transparent hover:bg-surface-base'"
        @click="store.selectEffect(effect.power)">
        <!-- Effect label + count -->
        <div class="flex items-center justify-between w-full">
          <span class="text-xs truncate"
            :class="store.selectedEffect === effect.power
              ? 'text-accent-gold'
              : effect.raceRestriction
                ? 'text-accent-red/70'
                : 'text-text-secondary'">
            {{ effect.effectLabel ?? effect.power }}
            <span v-if="effect.raceRestriction" class="text-xs opacity-70 ml-0.5">
              ({{ effect.raceRestriction }})
            </span>
          </span>
          <span class="text-text-dim text-xs shrink-0 ml-1">
            {{ effect.discoveryCount }}
          </span>
        </div>
        <!-- Resolved effect descriptions -->
        <div v-if="effect.descriptions.length > 0" class="flex flex-col gap-0">
          <span
            v-for="(desc, i) in effect.descriptions"
            :key="i"
            class="text-xs text-text-dim leading-tight truncate">
            {{ desc }}
          </span>
        </div>
        <!-- Required skill -->
        <span v-if="effect.skill" class="text-xs text-text-dim">
          Requires {{ effect.skill }}
        </span>
      </button>

      <div v-if="store.filteredEffects.length === 0" class="text-text-dim text-xs italic px-2 py-2 text-center">
        No matching effects.
      </div>
    </div>

    <!-- Footer -->
    <div class="text-xs text-text-muted px-2 py-1 border-t border-surface-card">
      {{ store.effectEntries.length }} effects
    </div>
  </div>
</template>

<script setup lang="ts">
import { useBreweryStore } from "../../stores/breweryStore";

const store = useBreweryStore();
</script>
