<template>
  <div class="flex flex-col gap-4 h-full min-h-0 overflow-y-auto">
    <div class="text-[11px] text-text-dim italic">Kills, deaths, XP, and Combat Wisdom from Chat.log. Item loot drops are no longer tracked by the game client — historic drop rates live under Economics → Farming → Database.</div>

    <EmptyState
      v-if="!hasCombat && combat.baselineLoaded"
      variant="panel"
      primary="No combat recorded yet."
      secondary="Turn on chat-log tailing and fight something — kills, deaths, and XP land here automatically." />

    <template v-else>
      <!-- Header: live-since, elapsed, rates, reset -->
      <div class="flex items-center justify-between gap-3 flex-wrap">
        <div class="flex items-center gap-4 text-xs text-text-secondary">
          <span v-if="combat.startedAt">Live since {{ formatTs(combat.startedAt) }}</span>
          <span v-else class="italic text-text-dim">Waiting for first combat event…</span>
          <span v-if="combat.startedAt">Elapsed {{ elapsed }}</span>
          <span>{{ combat.killsPerHour }}/hr kills</span>
          <span>{{ combat.xpPerHour }}/hr XP</span>
        </div>
        <button
          @click="combat.reset()"
          class="px-3 py-1.5 text-xs bg-transparent border border-border-default rounded text-text-secondary cursor-pointer hover:bg-surface-elevated hover:text-text-primary">
          Reset live
        </button>
      </div>

      <!-- Summary cards -->
      <div class="grid grid-cols-4 gap-3">
        <div class="bg-surface-elevated rounded border border-border-default p-3">
          <h3 class="text-xs font-semibold text-text-secondary uppercase tracking-wider mb-2">Kills</h3>
          <div class="text-lg font-bold text-text-primary">{{ combat.liveKills }} <span class="text-xs font-normal text-text-muted">live / {{ combat.lifetimeKills }} lifetime</span></div>
        </div>
        <div class="bg-surface-elevated rounded border border-border-default p-3">
          <h3 class="text-xs font-semibold text-text-secondary uppercase tracking-wider mb-2">Deaths</h3>
          <div class="text-lg font-bold text-text-primary">{{ deathStore.totalDeaths }}</div>
        </div>
        <div class="bg-surface-elevated rounded border border-border-default p-3">
          <h3 class="text-xs font-semibold text-text-secondary uppercase tracking-wider mb-2">XP (live)</h3>
          <div class="text-lg font-bold text-[#7ec87e]">{{ combat.liveXp.toLocaleString() }}</div>
        </div>
        <div class="bg-surface-elevated rounded border border-border-default p-3">
          <h3 class="text-xs font-semibold text-text-secondary uppercase tracking-wider mb-2">Wisdom (session)</h3>
          <div class="text-lg font-bold text-accent-gold">{{ gameState.combatWisdomSession.total.toLocaleString() }}</div>
        </div>
      </div>

      <!-- Kills by enemy -->
      <div v-if="combat.killRows.length > 0" class="flex flex-col gap-1">
        <h3 class="text-xs font-semibold text-text-secondary uppercase tracking-wider">Kills by enemy</h3>
        <div class="overflow-auto max-h-64">
          <table class="w-full text-sm border-collapse">
            <thead class="sticky top-0 bg-surface-base">
              <tr class="text-left text-text-secondary border-b border-border-default">
                <th class="py-1.5 px-2">Enemy</th>
                <th class="py-1.5 px-2 text-right">Live</th>
                <th class="py-1.5 px-2 text-right">Lifetime</th>
                <th class="py-1.5 px-2">Top ability</th>
                <th class="py-1.5 px-2">Last seen</th>
              </tr>
            </thead>
            <tbody>
              <tr v-for="row in combat.killRows" :key="row.name" class="border-b border-border-default/50">
                <td class="py-1 px-2"><EnemyInline :reference="row.name" /></td>
                <td class="py-1 px-2 text-right">{{ row.live }}</td>
                <td class="py-1 px-2 text-right text-text-muted">{{ row.lifetime }}</td>
                <td class="py-1 px-2"><AbilityInline v-if="row.topAbility" :reference="row.topAbility" :show-icon="false" /><span v-else class="text-text-dim">--</span></td>
                <td class="py-1 px-2 text-text-muted text-xs whitespace-nowrap">{{ formatTs(row.lastSeen) }}</td>
              </tr>
            </tbody>
          </table>
        </div>
      </div>

      <!-- XP by skill -->
      <div v-if="combat.xpRows.length > 0" class="flex flex-col gap-1">
        <h3 class="text-xs font-semibold text-text-secondary uppercase tracking-wider">XP by skill</h3>
        <div class="overflow-auto max-h-64">
          <table class="w-full text-sm border-collapse">
            <thead class="sticky top-0 bg-surface-base">
              <tr class="text-left text-text-secondary border-b border-border-default">
                <th class="py-1.5 px-2">Skill</th>
                <th class="py-1.5 px-2 text-right">Gained</th>
                <th class="py-1.5 px-2 text-right">Prodigy</th>
                <th class="py-1.5 px-2 text-right">Levels</th>
              </tr>
            </thead>
            <tbody>
              <tr v-for="row in combat.xpRows" :key="row.skill" class="border-b border-border-default/50">
                <td class="py-1 px-2"><SkillInline :reference="row.skill" /></td>
                <td class="py-1 px-2 text-right text-[#7ec87e]">{{ row.gained.toLocaleString() }}</td>
                <td class="py-1 px-2 text-right text-text-muted">{{ row.prodigy.toLocaleString() }}</td>
                <td class="py-1 px-2 text-right text-text-muted">{{ row.levels }}{{ row.lastLevel != null ? ` (→ ${row.lastLevel})` : '' }}</td>
              </tr>
            </tbody>
          </table>
        </div>
      </div>

      <!-- Recent deaths -->
      <div v-if="deathStore.deaths.length > 0" class="flex flex-col gap-1">
        <h3 class="text-xs font-semibold text-text-secondary uppercase tracking-wider">Recent deaths</h3>
        <div class="overflow-auto max-h-64">
          <table class="w-full text-sm border-collapse">
            <thead class="sticky top-0 bg-surface-base">
              <tr class="text-left text-text-secondary border-b border-border-default">
                <th class="py-1.5 px-2">Time</th>
                <th class="py-1.5 px-2">Killed by</th>
                <th class="py-1.5 px-2">Ability</th>
              </tr>
            </thead>
            <tbody>
              <tr v-for="death in deathStore.deaths.slice(0, 20)" :key="death.id" class="border-b border-border-default/50">
                <td class="py-1 px-2 text-text-muted text-xs whitespace-nowrap">{{ formatTs(death.died_at) }}</td>
                <td class="py-1 px-2"><EnemyInline :reference="death.killer_name" /></td>
                <td class="py-1 px-2"><AbilityInline :reference="death.killing_ability" :show-icon="false" /></td>
              </tr>
            </tbody>
          </table>
        </div>
      </div>

      <!-- Wisdom earns -->
      <div v-if="gameState.combatWisdomSession.earns.length > 0" class="flex flex-col gap-1">
        <h3 class="text-xs font-semibold text-text-secondary uppercase tracking-wider">Wisdom earns this session</h3>
        <div class="flex flex-col gap-1">
          <div v-for="(e, i) in gameState.combatWisdomSession.earns" :key="i" class="flex items-baseline justify-between gap-2 text-sm">
            <span class="text-text-primary truncate">{{ e.name ?? 'Prodigy Level' }}<span v-if="e.zone" class="text-text-dim text-[11px]"> ({{ e.zone }})</span></span>
            <span class="text-accent-gold tabular-nums whitespace-nowrap">+{{ e.amount }}</span>
          </div>
        </div>
      </div>

      <!-- Event log -->
      <div class="flex flex-col gap-1">
        <div class="flex items-center justify-between">
          <h3 class="text-xs font-semibold text-text-secondary uppercase tracking-wider">Event log</h3>
          <div class="flex items-center gap-1">
            <button
              v-for="opt in filterOptions"
              :key="opt.value"
              @click="logFilter = opt.value"
              :class="['px-2 py-1 text-xs rounded font-medium cursor-pointer', logFilter === opt.value ? 'bg-surface-elevated text-accent-gold' : 'text-text-secondary hover:text-text-primary']">
              {{ opt.label }}
            </button>
          </div>
        </div>
        <div v-if="filteredLog.length === 0" class="text-xs text-text-dim italic">No events yet — fight something with chat-log tailing on.</div>
        <div v-else class="flex flex-col gap-0.5 max-h-64 overflow-y-auto">
          <div v-for="(entry, i) in filteredLog" :key="i" class="flex items-baseline gap-2 text-xs">
            <span class="text-text-dim whitespace-nowrap">{{ formatTs(entry.timestamp) }}</span>
            <span :class="kindClass(entry.kind)">{{ entry.label }}</span>
            <span v-if="entry.detail" class="text-text-dim">{{ entry.detail }}</span>
          </div>
        </div>
      </div>
    </template>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted } from 'vue'
import { useCombatStore } from '../../stores/combatStore'
import { useDeathStore } from '../../stores/deathStore'
import { useGameStateStore } from '../../stores/gameStateStore'
import { formatAnyTimestamp as formatTs } from '../../composables/useTimestamp'
import type { CombatLogKind } from '../../types/combat'
import EmptyState from '../Shared/EmptyState.vue'
import EnemyInline from '../Shared/Enemy/EnemyInline.vue'
import AbilityInline from '../Shared/Ability/AbilityInline.vue'
import SkillInline from '../Shared/Skill/SkillInline.vue'

const combat = useCombatStore()
const deathStore = useDeathStore()
const gameState = useGameStateStore()

type LogFilter = 'all' | CombatLogKind
const logFilter = ref<LogFilter>('all')
const filterOptions: Array<{ value: LogFilter; label: string }> = [
  { value: 'all', label: 'All' },
  { value: 'kill', label: 'Kills' },
  { value: 'death', label: 'Deaths' },
  { value: 'level', label: 'Levels' },
  { value: 'wisdom', label: 'Wisdom' },
]

const hasCombat = computed(() =>
  combat.liveKills > 0 ||
  combat.liveXp > 0 ||
  combat.log.length > 0 ||
  deathStore.totalDeaths > 0 ||
  gameState.combatWisdomSession.total > 0,
)

const filteredLog = computed(() =>
  logFilter.value === 'all' ? combat.log : combat.log.filter((e) => e.kind === logFilter.value),
)

function kindClass(kind: CombatLogKind): string {
  switch (kind) {
    case 'kill': return 'text-text-primary'
    case 'death': return 'text-[#c87e7e]'
    case 'level': return 'text-[#7ec87e]'
    case 'wisdom': return 'text-accent-gold'
  }
}

// Live elapsed clock (1s tick, same pattern as the frozen farming timer).
const now = ref(Date.now())
let timer: ReturnType<typeof setInterval> | undefined
const elapsed = computed(() => {
  if (!combat.startedAt) return '—'
  const secs = Math.max(0, Math.floor((now.value - Date.parse(combat.startedAt)) / 1000))
  const h = Math.floor(secs / 3600)
  const m = Math.floor((secs % 3600) / 60)
  const s = secs % 60
  return h > 0 ? `${h}h ${m}m` : m > 0 ? `${m}m ${s}s` : `${s}s`
})

onMounted(() => {
  void combat.loadHistoricBaseline()
  if (!deathStore.loaded) void deathStore.loadDeaths()
  void gameState.fetchCombatWisdomMonsters()
  timer = setInterval(() => { now.value = Date.now() }, 1000)
})

onUnmounted(() => {
  if (timer) clearInterval(timer)
})
</script>
