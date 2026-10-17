<template>
  <PaneLayout screen-key="character">
  <div class="flex flex-col gap-4 h-full min-h-0">
    <!-- Skills: unified skill view (manages its own scroll) -->
    <template v-if="activeTab === 'skills'">
      <SkillsScreen />
    </template>

    <!-- Stats: character report data (skills, stats, currencies) -->
    <div v-else-if="activeTab === 'stats'" class="flex-1 flex flex-col gap-3 min-h-0">
      <!-- Header row -->
      <div class="flex items-center justify-between shrink-0">
        <div class="flex items-center gap-4">
          <h2 class="text-lg font-semibold text-text-primary">
            {{ store.selectedCharacter?.character_name ?? 'Character' }}
          </h2>
          <span v-if="store.selectedCharacter" class="text-sm text-text-muted">
            {{ store.selectedCharacter.server_name }}
          </span>
        </div>
        <div class="flex items-center gap-4">
          <template v-if="store.selectedCharacter">
            <select
              class="bg-surface-elevated border border-border-default rounded px-3 py-1.5 text-sm text-text-primary cursor-pointer min-w-70"
              :value="store.selectedSnapshot?.id"
              @change="onSnapshotChange">
              <option v-for="snap in store.snapshots" :key="snap.id" :value="snap.id">
                {{ formatTimestamp(snap.snapshot_timestamp) }} — {{ snap.race }} · {{ snap.skill_count }} skills
              </option>
            </select>
          </template>
          <button
            class="px-4 py-2 bg-accent-gold/20 border border-accent-gold/40 text-accent-gold rounded cursor-pointer text-sm font-medium transition-all hover:bg-accent-gold/30"
            :disabled="store.loading"
            @click="handleImport">
            Import Report
          </button>
        </div>
      </div>

      <!-- Import feedback -->
      <div v-if="store.lastImport && !store.lastImport.was_duplicate" class="p-3 bg-green-900/20 border border-green-700/40 rounded text-sm text-green-300 shrink-0">
        Imported {{ store.lastImport.character_name }} ({{ store.lastImport.server_name }}) —
        {{ store.lastImport.skills_imported }} skills,
        {{ store.lastImport.npcs_imported }} NPCs,
        {{ store.lastImport.recipes_imported }} recipes
      </div>

      <div v-if="store.lastImport?.was_duplicate" class="p-3 bg-yellow-900/20 border border-yellow-700/40 rounded text-sm text-yellow-300 shrink-0">
        Snapshot already imported (duplicate).
      </div>

      <div v-if="store.error" class="p-3 bg-red-900/20 border border-red-700/40 rounded text-sm text-red-300 shrink-0">
        {{ store.error }}
      </div>

      <!-- 3-column card grid -->
      <div class="flex-1 grid grid-cols-3 gap-3 min-h-0">
        <!-- Column 1: Combat Stats + Player Attributes -->
        <div class="flex flex-col gap-3 min-h-0">
          <div class="bg-surface-elevated border border-border-default rounded-lg p-3 flex flex-col min-h-0 flex-1 overflow-hidden">
            <template v-if="store.selectedSnapshot">
              <StatsTable :stats="store.stats" />
            </template>
            <EmptyState v-else-if="!store.selectedCharacter && !store.loading"
              primary="No character data."
              secondary="Import a character report to see combat stats." />
            <div v-else class="text-xs text-text-dim italic">No snapshot selected.</div>
          </div>
          <div class="bg-surface-elevated border border-border-default rounded-lg p-3 flex flex-col min-h-0 flex-1 overflow-hidden">
            <PlayerAttributesCard />
          </div>
        </div>

        <!-- Column 2: Skills + Currencies -->
        <div class="flex flex-col gap-3 min-h-0">
          <div class="bg-surface-elevated border border-border-default rounded-lg p-3 flex flex-col min-h-0 flex-1 overflow-hidden">
            <template v-if="store.selectedSnapshot">
              <SkillTable :skills="store.skills" />
            </template>
            <EmptyState v-else-if="!store.selectedCharacter && !store.loading"
              primary="No character data."
              secondary="Import a character report to see skills." />
            <div v-else class="text-xs text-text-dim italic">No snapshot selected.</div>
          </div>
          <div v-if="store.selectedSnapshot" class="bg-surface-elevated border border-border-default rounded-lg p-3 flex flex-col min-h-0 shrink-0 max-h-[30%] overflow-hidden">
            <CurrencyTable :currencies="store.currencies" />
          </div>
        </div>

        <!-- Column 3: Computed Stats + Character Report Stats -->
        <div class="flex flex-col gap-3 min-h-0">
          <div class="bg-surface-elevated border border-border-default rounded-lg p-3 flex flex-col min-h-0 flex-1 overflow-hidden">
            <ComputedStatsCard />
          </div>
          <div class="bg-surface-elevated border border-border-default rounded-lg p-3 flex flex-col min-h-0 flex-1 overflow-hidden">
            <ReportStatsSection />
          </div>
        </div>
      </div>
    </div>

    <!-- NPCs -->
    <template v-else-if="activeTab === 'npcs'">
      <NpcsScreen />
    </template>

    <!-- Quests -->
    <template v-else-if="activeTab === 'quests'">
      <QuestsScreen />
    </template>

    <!-- Deaths -->
    <template v-else-if="activeTab === 'deaths'">
      <DeathsView />
    </template>

    <!-- Gourmand -->
    <template v-else-if="activeTab === 'gourmand'">
      <GourmandView />
    </template>

    <!-- Statehelm -->
    <template v-else-if="activeTab === 'statehelm'">
      <StatehelmView />
    </template>

    <!-- Build Planner -->
    <template v-else-if="activeTab === 'build-planner'">
      <BuildPlannerScreen />
    </template>

    <!-- Account Overview -->
    <template v-else-if="activeTab === 'account'">
      <!-- The pane itself is `overflow-hidden`, so AggregateView's grids need this to be
           the nearest scrollport: without it the content is clipped and the sticky
           headers cannot pin (DataTable never owns scrolling). -->
      <div class="flex-1 min-h-0 overflow-auto">
        <AggregateView />
      </div>
    </template>
  </div>
  </PaneLayout>
</template>

<script setup lang="ts">
import { onMounted } from 'vue'
import { useCharacterStore } from '../../stores/characterStore'
import { formatDateTimeFull } from '../../composables/useTimestamp'
import PaneLayout from '../Shared/PaneLayout.vue'
import EmptyState from '../Shared/EmptyState.vue'
import SkillsScreen from './SkillsScreen.vue'
import SkillTable from './SkillTable.vue'
import NpcsScreen from './NpcsScreen.vue'
import QuestsScreen from './QuestsScreen.vue'
import StatsTable from './StatsTable.vue'
import CurrencyTable from './CurrencyTable.vue'
import GourmandView from '../Gourmand/GourmandView.vue'
import BuildPlannerScreen from './BuildPlanner/BuildPlannerScreen.vue'
import DeathsView from './DeathsView.vue'
import StatehelmView from './StatehelmView.vue'
import AggregateView from '../Dashboard/AggregateView.vue'
import ReportStatsSection from './ReportStatsSection.vue'
import ComputedStatsCard from './ComputedStatsCard.vue'
import PlayerAttributesCard from './PlayerAttributesCard.vue'

defineProps<{
  activeTab: string;
}>();

const store = useCharacterStore()

function onSnapshotChange(event: Event) {
  const id = Number((event.target as HTMLSelectElement).value)
  const snap = store.snapshots.find(s => s.id === id)
  if (snap) {
    store.selectSnapshot(snap)
  }
}

async function handleImport() {
  await store.importCharacterReport()
  if (store.lastImport && !store.lastImport.was_duplicate) {
    await store.initForActiveCharacter()
  }
}

function formatTimestamp(ts: string): string {
  return formatDateTimeFull(ts)
}

onMounted(() => {
  if (!store.selectedCharacter) {
    store.initForActiveCharacter()
  }
})
</script>
