<template>
  <div class="flex flex-col gap-3 text-sm">
    <!-- Recent deaths -->
    <div>
      <h3 class="section-heading mb-1.5">Recent Deaths</h3>
      <div v-if="deathStore.deaths.length === 0" class="text-xs text-text-dim italic">
        No deaths recorded yet.
      </div>
      <div v-else class="flex flex-col gap-1">
        <div
          v-for="death in recentDeaths"
          :key="death.id"
          class="flex items-center justify-between gap-2 py-0.5">
          <div class="flex items-center gap-1.5 min-w-0">
            <span class="text-value-negative text-xs shrink-0">&#x2620;</span>
            <span class="text-text-primary truncate">
              <EnemyInline :reference="death.killer_name" />
            </span>
          </div>
          <span class="text-text-dim text-xs font-mono whitespace-nowrap shrink-0">
            {{ formatTs(death.died_at) }}
          </span>
        </div>
      </div>
    </div>

    <!-- Last rezzed by -->
    <div v-if="rezStore.lastRezzedBy">
      <h3 class="section-heading mb-1.5">Last Rezzed By</h3>
      <div class="flex items-center justify-between gap-2 py-0.5">
        <span class="text-value-positive">{{ rezStore.lastRezzedBy.caster_name }}</span>
        <span class="text-text-dim text-xs font-mono whitespace-nowrap">
          {{ formatTs(rezStore.lastRezzedBy.occurred_at) }}
        </span>
      </div>
    </div>

    <!-- Top rezzers (who rezzed you most) -->
    <div v-if="rezStore.topRezzers.length > 0">
      <h3 class="section-heading mb-1.5">Top Rezzers</h3>
      <div class="flex flex-col gap-0.5">
        <div
          v-for="entry in rezStore.topRezzers.slice(0, 5)"
          :key="entry.name"
          class="flex items-center justify-between gap-2 py-0.5">
          <span class="text-text-primary truncate">{{ entry.name }}</span>
          <span class="text-text-muted text-xs shrink-0">{{ entry.count }}</span>
        </div>
      </div>
    </div>

    <!-- Top rezzed (who you rezzed most) -->
    <div v-if="rezStore.topRezzed.length > 0">
      <h3 class="section-heading mb-1.5">You Rezzed</h3>
      <div class="flex flex-col gap-0.5">
        <div
          v-for="entry in rezStore.topRezzed.slice(0, 5)"
          :key="entry.name"
          class="flex items-center justify-between gap-2 py-0.5">
          <span class="text-text-primary truncate">{{ entry.name }}</span>
          <span class="text-text-muted text-xs shrink-0">{{ entry.count }}</span>
        </div>
      </div>
    </div>

    <!-- Summary line -->
    <div class="text-xs text-text-dim border-t border-border-default pt-2 flex gap-3">
      <span>{{ deathStore.totalDeaths }} deaths</span>
      <span>{{ rezStore.rezzedByOthers.length }} rezzes received</span>
      <span>{{ rezStore.rezzedOthers.length }} rezzes given</span>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed, onMounted } from 'vue'
import { useDeathStore } from '../../../stores/deathStore'
import { useResuscitateStore } from '../../../stores/resuscitateStore'
import { formatAnyTimestamp as formatTs } from '../../../composables/useTimestamp'
import EnemyInline from '../../Shared/Enemy/EnemyInline.vue'

const deathStore = useDeathStore()
const rezStore = useResuscitateStore()

const recentDeaths = computed(() => deathStore.deaths.slice(0, 5))

onMounted(() => {
  if (!deathStore.loaded) deathStore.loadDeaths()
  if (!rezStore.loaded) rezStore.loadResuscitations()
})
</script>
