<template>
  <div class="flex flex-col gap-3 px-2 py-1 text-center">
    <!-- Stat Bonuses (Health, Armor, Power only) -->
    <div v-if="statBonuses.length > 0">
      <h4 class="micro-label mb-1">Stat Bonuses</h4>
      <div class="space-y-0.5">
        <div
          v-for="attr in statBonuses"
          :key="attr.label"
          class="text-xs">
          <span :class="attr.value > 0 ? 'text-green-400' : 'text-red-400'">
            {{ attr.formattedValue }}
          </span>
          <span class="text-text-muted ml-1">{{ attr.label }}</span>
        </div>
      </div>
    </div>

    <!-- Combat Refresh -->
    <div v-if="combatRefreshBonuses.length > 0">
      <h4 class="micro-label mb-1">Combat Refresh</h4>
      <div class="space-y-0.5">
        <div
          v-for="attr in combatRefreshBonuses"
          :key="attr.label"
          class="text-xs">
          <span :class="attr.value > 0 ? 'text-green-400' : 'text-red-400'">
            {{ attr.formattedValue }}
          </span>
          <span class="text-text-muted ml-1">{{ shortLabel(attr.label) }}</span>
        </div>
      </div>
    </div>

    <!-- Armor Set Bonus -->
    <div v-if="armorSetEntries.length > 0">
      <h4 class="micro-label mb-1">Set Bonus</h4>
      <div class="space-y-0.5">
        <div
          v-for="entry in armorSetEntries"
          :key="entry.type"
          class="text-xs"
          :class="entry.hasBonus ? 'text-accent-gold font-medium' : 'text-text-dim'">
          {{ entry.count }}x {{ entry.type }}
          <span v-if="entry.hasBonus" class="text-[10px] leading-none">(3pc)</span>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import { useBuildPlannerStore } from '../../../stores/buildPlannerStore'
import { useBuildStats } from '../../../composables/useBuildStats'

const store = useBuildPlannerStore()
const { itemAttributes } = useBuildStats()

/** Labels for the core stat bonuses section */
const STAT_LABELS = ['Max Health', 'Max Armor', 'Max Power']

/** Match combat refresh labels like "Health from Combat Refresh Abilities" */
function isCombatRefresh(label: string): boolean {
  return label.toLowerCase().includes('combat refresh')
}

const statBonuses = computed(() =>
  itemAttributes.value.filter(a => STAT_LABELS.includes(a.label))
)

const combatRefreshBonuses = computed(() =>
  itemAttributes.value.filter(a => isCombatRefresh(a.label))
)

/** Shorten "Health from Combat Refresh Abilities" to "Health" */
function shortLabel(label: string): string {
  return label.replace(/\s+from\s+Combat\s+Refresh\s+Abilities/i, '')
}

const armorSetEntries = computed(() => {
  const counts = store.armorTypeCounts
  return Object.entries(counts).map(([type, count]) => ({
    type,
    count,
    hasBonus: count >= 3,
  }))
})
</script>
