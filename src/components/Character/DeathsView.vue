<template>
  <div class="flex flex-col gap-4 h-full min-h-0 overflow-y-auto">
    <div class="flex items-center justify-between">
      <h2 class="screen-title">Deaths</h2>
      <span class="text-sm text-text-muted">{{ store.totalDeaths }} total</span>
    </div>

    <EmptyState
      v-if="store.totalDeaths === 0 && store.loaded"
      primary="No deaths recorded yet."
      secondary="Deaths are tracked from the [Combat] channel in your chat log." />

    <!-- Summary cards -->
    <div v-if="store.totalDeaths > 0" class="grid grid-cols-4 gap-3">
      <!-- Top killers -->
      <div class="bg-surface-elevated rounded border border-border-default p-3">
        <h3 class="section-heading mb-2">Top Killers</h3>
        <div class="flex flex-col gap-1">
          <div
            v-for="entry in store.deathsByKiller.slice(0, 5)"
            :key="entry.name"
            class="flex items-center justify-between text-sm">
            <EnemyInline :reference="entry.name" />
            <span class="text-text-muted ml-2 shrink-0">{{ entry.count }}</span>
          </div>
        </div>
      </div>

      <!-- Top abilities -->
      <div class="bg-surface-elevated rounded border border-border-default p-3">
        <h3 class="section-heading mb-2">Deadliest Abilities</h3>
        <div class="flex flex-col gap-1">
          <div
            v-for="entry in store.deathsByAbility.slice(0, 5)"
            :key="entry.name"
            class="flex items-center justify-between text-sm">
            <AbilityInline :reference="entry.name" :show-icon="false" />
            <span class="text-text-muted ml-2 shrink-0">{{ entry.count }}</span>
          </div>
        </div>
      </div>

      <!-- Top areas -->
      <div class="bg-surface-elevated rounded border border-border-default p-3">
        <h3 class="section-heading mb-2">Deadliest Areas</h3>
        <div class="flex flex-col gap-1">
          <div
            v-for="entry in store.deathsByArea.slice(0, 5)"
            :key="entry.name"
            class="flex items-center justify-between text-sm">
            <AreaInline :reference="entry.name" />
            <span class="text-text-muted ml-2 shrink-0">{{ entry.count }}</span>
          </div>
        </div>
      </div>

      <!-- Top damage types -->
      <div class="bg-surface-elevated rounded border border-border-default p-3">
        <h3 class="section-heading mb-2">Damage Types</h3>
        <div class="flex flex-col gap-1">
          <div
            v-for="entry in store.deathsByDamageType.slice(0, 5)"
            :key="entry.name"
            class="flex items-center justify-between text-sm">
            <span class="text-text-primary truncate">{{ entry.name }}</span>
            <span class="text-text-muted ml-2 shrink-0">{{ entry.count }}</span>
          </div>
        </div>
      </div>
    </div>

    <!-- Death log table -->
    <div v-if="store.totalDeaths > 0" class="flex-1 min-h-0">
      <div class="overflow-auto max-h-full">
        <table class="w-full text-sm border-collapse">
          <thead class="sticky top-0 bg-surface-base">
            <tr class="text-left text-text-secondary border-b border-border-default">
              <th class="py-1.5 px-2 w-6"></th>
              <th class="py-1.5 px-2">Time</th>
              <th class="py-1.5 px-2">Killed By</th>
              <th class="py-1.5 px-2">Ability</th>
              <th class="py-1.5 px-2">Damage Type</th>
              <th class="py-1.5 px-2">Area</th>
              <th class="py-1.5 px-2 text-right">Damage</th>
            </tr>
          </thead>
          <tbody>
            <template v-for="death in store.deaths" :key="death.id">
              <tr
                class="border-b border-border-default/50 hover:bg-surface-elevated/50 cursor-pointer"
                @click="toggleExpand(death.id)">
                <td class="py-1 px-2 text-text-dim text-xs">
                  <span class="inline-block transition-transform" :class="expandedDeathId === death.id ? 'rotate-90' : ''">&#9654;</span>
                </td>
                <td class="py-1 px-2 text-text-muted text-xs whitespace-nowrap">
                  {{ formatTs(death.died_at) }}
                </td>
                <td class="py-1 px-2">
                  <EnemyInline :reference="death.killer_name" />
                </td>
                <td class="py-1 px-2">
                  <AbilityInline :reference="death.killing_ability" :show-icon="false" />
                </td>
                <td class="py-1 px-2 text-text-secondary">
                  <span v-if="death.damage_type">{{ death.damage_type }}</span>
                  <span v-else class="text-text-dim">--</span>
                </td>
                <td class="py-1 px-2 text-text-secondary">
                  <AreaInline v-if="death.area" :reference="death.area" />
                  <span v-else class="text-text-dim">--</span>
                </td>
                <td class="py-1 px-2 text-right">
                  <span class="text-vital-health">{{ death.health_damage }}</span>
                  <span v-if="death.armor_damage > 0" class="text-vital-armor">
                    / {{ death.armor_damage }}
                  </span>
                </td>
              </tr>
              <!-- Expanded damage sources -->
              <tr v-if="expandedDeathId === death.id">
                <td :colspan="7" class="p-0">
                  <div class="bg-surface-dark/50 border-b border-border-default px-6 py-2">
                    <div v-if="loadingSources" class="text-text-dim text-xs py-1">Loading...</div>
                    <div v-else-if="!expandedSources.length" class="text-text-dim text-xs py-1">
                      No prior damage recorded for this death.
                    </div>
                    <table v-else class="w-full text-xs">
                      <thead>
                        <tr class="text-text-dim">
                          <th class="py-0.5 pr-3 text-left font-normal">Time</th>
                          <th class="py-0.5 pr-3 text-left font-normal">Attacker</th>
                          <th class="py-0.5 pr-3 text-left font-normal">Ability</th>
                          <th class="py-0.5 text-right font-normal">Damage</th>
                        </tr>
                      </thead>
                      <tbody>
                        <tr v-for="src in expandedSources" :key="src.id" class="text-text-secondary">
                          <td class="py-0.5 pr-3 text-text-dim whitespace-nowrap">{{ formatTs(src.timestamp) }}</td>
                          <td class="py-0.5 pr-3">
                            <EnemyInline :reference="src.attacker_name" />
                          </td>
                          <td class="py-0.5 pr-3">
                            <AbilityInline :reference="src.ability_name" :show-icon="false" />
                            <span v-if="src.is_crit" class="text-yellow-400 ml-1 text-[10px]">CRIT</span>
                          </td>
                          <td class="py-0.5 text-right">
                            <span class="text-vital-health">{{ src.health_damage }}</span>
                            <span v-if="src.armor_damage > 0" class="text-vital-armor">
                              / {{ src.armor_damage }}
                            </span>
                          </td>
                        </tr>
                      </tbody>
                    </table>
                  </div>
                </td>
              </tr>
            </template>
          </tbody>
        </table>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, onMounted } from 'vue'
import { useDeathStore, type DeathDamageSource } from '../../stores/deathStore'
import { formatAnyTimestamp as formatTs } from '../../composables/useTimestamp'
import EmptyState from '../Shared/EmptyState.vue'
import AbilityInline from '../Shared/Ability/AbilityInline.vue'
import EnemyInline from '../Shared/Enemy/EnemyInline.vue'
import AreaInline from '../Shared/Area/AreaInline.vue'

const store = useDeathStore()

const expandedDeathId = ref<number | null>(null)
const expandedSources = ref<DeathDamageSource[]>([])
const loadingSources = ref(false)

async function toggleExpand(deathId: number) {
  if (expandedDeathId.value === deathId) {
    expandedDeathId.value = null
    expandedSources.value = []
    return
  }
  expandedDeathId.value = deathId
  loadingSources.value = true
  expandedSources.value = await store.loadDamageSources(deathId)
  loadingSources.value = false
}

onMounted(() => {
  if (!store.loaded) {
    store.loadDeaths()
  }
})
</script>
