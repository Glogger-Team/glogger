<template>
  <div class="flex flex-col gap-3 text-sm h-full">
    <div class="text-[11px] text-text-dim italic"
      title="Personal bets are read from Player.log talk screens, which the current game client no longer writes. Match results still track via chat.">
      No new bets can be recorded on the current game client — historic bets below, if any.
    </div>
    <div v-if="stats.total_matches === 0" class="text-xs text-text-dim italic">
      No arena fights recorded yet. Stand in the casino arena while Kuzavek
      narrates — matchups and winners are read from the chat log's
      <span class="font-mono">[NPC Chatter] Kuzavek</span> lines. No fight tips to
      enter; rankings build themselves from what actually happened on your server.
    </div>

    <template v-else>
      <!-- Header: sample size + personal betting rate -->
      <div class="flex items-baseline justify-between gap-2 flex-wrap">
        <span class="text-xs text-text-dim uppercase tracking-wide">Fighter rankings</span>
        <span class="flex items-baseline gap-3 text-xs text-text-dim tabular-nums">
          <span
            v-if="stats.betting.total > 0"
            class="cursor-help"
            :title="bettingTooltip">
            <span class="text-sm font-semibold" :style="{ color: rateColor(stats.betting.win_pct) }">
              {{ stats.betting.win_pct.toFixed(0) }}%
            </span>
            Betting Rate
          </span>
          <span
            v-else
            class="cursor-help italic"
            title="Personal bets are read from Player.log talk screens, which the current game client no longer writes. Match results above still track via chat.">
            No bets tracked
          </span>
          <span>
            <span class="text-sm font-semibold text-accent-gold">{{ stats.total_matches.toLocaleString() }}</span>
            fights tracked
          </span>
        </span>
      </div>

      <!-- Matchup predictor: pick any two fighters → most likely winner. -->
      <div class="flex flex-col gap-1.5 rounded border border-border-default p-2">
        <span class="text-[11px] text-text-dim uppercase tracking-wide">Who would win?</span>
        <div class="flex items-center gap-2">
          <StyledSelect
            v-model="pickA"
            :options="fighterOptions"
            size="xs"
            full-width
            class="flex-1 min-w-0" />
          <span class="text-text-dim text-xs shrink-0">vs</span>
          <StyledSelect
            v-model="pickB"
            :options="fighterOptions"
            size="xs"
            full-width
            class="flex-1 min-w-0" />
        </div>
        <div v-if="prediction" class="flex items-baseline justify-between gap-2">
          <span class="text-xs">
            Favored:
            <span class="font-semibold" :style="{ color: winColor }">{{ prediction.favored }}</span>
          </span>
          <span class="text-[11px] text-text-dim tabular-nums">
            {{ prediction.pct }}% · {{ prediction.basisLabel }}
          </span>
        </div>
        <div v-else class="text-[11px] text-text-dim italic">
          Pick two different fighters.
        </div>
      </div>

      <!-- Rankings table -->
      <div class="flex flex-col gap-1 min-w-0">
        <div
          v-for="(f, i) in stats.fighters"
          :key="f.name"
          class="flex items-center gap-2">
          <span class="text-text-dim tabular-nums w-4 text-right text-xs">{{ i + 1 }}</span>
          <span class="text-text-primary truncate flex-1 min-w-0">{{ f.name }}</span>
          <span class="text-[11px] text-text-dim tabular-nums whitespace-nowrap">
            {{ f.wins }}–{{ f.losses }}
          </span>
          <!-- win% bar -->
          <div class="h-2 w-20 rounded-full bg-bg-secondary overflow-hidden shrink-0">
            <div
              class="h-full rounded-full"
              :style="{ width: f.win_pct + '%', backgroundColor: rateColor(f.win_pct) }" />
          </div>
          <span class="text-xs tabular-nums w-10 text-right" :style="{ color: rateColor(f.win_pct) }">
            {{ f.win_pct.toFixed(0) }}%
          </span>
        </div>
      </div>

      <div class="h-px bg-border-default" />

      <!-- Head-to-head matrix: row fighter's win% vs column fighter. -->
      <div class="flex flex-col gap-1 min-h-0">
        <div class="flex items-baseline justify-between gap-2">
          <span class="text-xs text-text-dim uppercase tracking-wide">Head-to-head</span>
          <span class="text-[10px] text-text-dim/80">row beats column</span>
        </div>
        <div class="overflow-x-auto">
          <table class="border-collapse text-[11px]">
            <thead>
              <tr>
                <th class="p-1"></th>
                <th
                  v-for="c in fighterNames"
                  :key="c"
                  class="p-1 font-medium text-text-dim text-center whitespace-nowrap"
                  :title="c">
                  {{ abbr(c) }}
                </th>
              </tr>
            </thead>
            <tbody>
              <tr v-for="r in fighterNames" :key="r">
                <td class="p-1 pr-2 text-text-dim text-right whitespace-nowrap font-medium">
                  {{ abbr(r) }}
                </td>
                <td
                  v-for="c in fighterNames"
                  :key="c"
                  class="p-0.5 text-center tabular-nums"
                  :style="cellStyle(r, c)"
                  :title="cellTitle(r, c)">
                  {{ cellLabel(r, c) }}
                </td>
              </tr>
            </tbody>
          </table>
        </div>
      </div>
    </template>
  </div>
</template>

<script setup lang="ts">
import { computed, onMounted, ref, watch } from 'vue'
import { useGameStateStore } from '../../../stores/gameStateStore'
import StyledSelect from '../../Shared/StyledSelect.vue'

const store = useGameStateStore()

onMounted(() => {
  store.fetchArenaStats()
})

const stats = computed(() => store.arenaStats)

/** Fighter names, ranked (highest win% first) — the display/matrix order. */
const fighterNames = computed(() => stats.value.fighters.map((f) => f.name))

/** Fighter names as StyledSelect options for the matchup predictor. */
const fighterOptions = computed(() =>
  fighterNames.value.map((n) => ({ value: n, label: n })),
)

/** P&L tooltip for the personal betting rate in the header. */
const bettingTooltip = computed(() => {
  const b = stats.value.betting
  const net = b.net_profit
  const sign = net >= 0 ? '+' : '−'
  const netStr = `${sign}${Math.abs(net).toLocaleString()}`
  return (
    `${b.won}/${b.total} bets won\n` +
    `Wagered: ${b.total_wagered.toLocaleString()} councils\n` +
    `Won back: ${b.total_won.toLocaleString()} councils\n` +
    `Net: ${netStr} councils`
  )
})

// ── Matchup predictor state ───────────────────────────────────────────────
const pickA = ref('')
const pickB = ref('')

// Seed the two picks with the top two fighters once data arrives / changes.
watch(
  fighterNames,
  (names) => {
    if (names.length >= 2) {
      if (!names.includes(pickA.value)) pickA.value = names[0]
      if (!names.includes(pickB.value) || pickB.value === pickA.value)
        pickB.value = names[1]
    }
  },
  { immediate: true },
)

// ── Lookup helpers ─────────────────────────────────────────────────────────
/** Overall win pct by fighter name. */
const overallPct = computed(
  () => new Map(stats.value.fighters.map((f) => [f.name, f.win_pct])),
)

/** Directed H2H record: "A|B" → { wins, losses } (A's record vs B). */
const h2hMap = computed(() => {
  const m = new Map<string, { wins: number; losses: number }>()
  for (const c of stats.value.head_to_head) {
    m.set(`${c.fighter}|${c.opponent}`, { wins: c.wins, losses: c.losses })
  }
  return m
})

function h2h(a: string, b: string) {
  return h2hMap.value.get(`${a}|${b}`) ?? { wins: 0, losses: 0 }
}

/** Minimum head-to-head samples before we trust it over overall win rates. */
const H2H_MIN_SAMPLES = 3

/**
 * Predict the favored fighter for a matchup. Uses the direct head-to-head
 * record when enough games exist; otherwise falls back to overall win rates.
 */
const prediction = computed(() => {
  const a = pickA.value
  const b = pickB.value
  if (!a || !b || a === b) return null

  const rec = h2h(a, b)
  const played = rec.wins + rec.losses
  if (played >= H2H_MIN_SAMPLES) {
    const aRate = (rec.wins / played) * 100
    const favored = aRate >= 50 ? a : b
    const pct = Math.round(aRate >= 50 ? aRate : 100 - aRate)
    return { favored, pct, basisLabel: `head-to-head (${played})` }
  }

  // Fallback: overall win rates.
  const aPct = overallPct.value.get(a) ?? 0
  const bPct = overallPct.value.get(b) ?? 0
  const favored = aPct >= bPct ? a : b
  // Express confidence as the favorite's share of the two overall rates.
  const sum = aPct + bPct
  const pct = sum > 0 ? Math.round((Math.max(aPct, bPct) / sum) * 100) : 50
  const basis = played > 0 ? `overall (only ${played} h2h)` : 'overall win rate'
  return { favored, pct, basisLabel: basis }
})

// ── Colors: matte / pastel diverging scale (loss → win) ────────────────────
const C_LOW = '#c97b7b' // pastel red  (low win rate)
const C_MID = '#cbb56b' // pastel amber (even)
const C_HIGH = '#7bab86' // pastel green (high win rate)

function lerp(a: number, b: number, t: number) {
  return Math.round(a + (b - a) * t)
}
function hexToRgb(h: string) {
  return [
    parseInt(h.slice(1, 3), 16),
    parseInt(h.slice(3, 5), 16),
    parseInt(h.slice(5, 7), 16),
  ] as const
}
/** Map a 0..100 win rate onto the red→amber→green pastel ramp. */
function rateColor(pct: number): string {
  const t = Math.max(0, Math.min(100, pct)) / 100
  const [c1, c2] = t < 0.5 ? [C_LOW, C_MID] : [C_MID, C_HIGH]
  const tt = t < 0.5 ? t / 0.5 : (t - 0.5) / 0.5
  const a = hexToRgb(c1)
  const b = hexToRgb(c2)
  return `rgb(${lerp(a[0], b[0], tt)}, ${lerp(a[1], b[1], tt)}, ${lerp(a[2], b[2], tt)})`
}

const winColor = computed(() => rateColor(prediction.value?.pct ?? 50))

// ── Matrix cell rendering ──────────────────────────────────────────────────
function abbr(name: string): string {
  return name.length <= 4 ? name : name.slice(0, 4)
}

/** Row fighter's win% vs column fighter, or null when they've never met. */
function cellRate(row: string, col: string): number | null {
  const rec = h2h(row, col)
  const total = rec.wins + rec.losses
  return total === 0 ? null : (rec.wins / total) * 100
}

function cellStyle(row: string, col: string) {
  if (row === col) return { backgroundColor: 'var(--color-bg-secondary)' }
  const rate = cellRate(row, col)
  if (rate === null) return { color: 'var(--color-text-dim)', opacity: 0.4 }
  return {
    backgroundColor: rateColor(rate),
    color: '#1a1a1a',
    fontWeight: '600',
  }
}

function cellLabel(row: string, col: string): string {
  if (row === col) return '·'
  const rate = cellRate(row, col)
  return rate === null ? '–' : `${Math.round(rate)}`
}

function cellTitle(row: string, col: string): string {
  if (row === col) return row
  const rec = h2h(row, col)
  const total = rec.wins + rec.losses
  if (total === 0) return `${row} vs ${col}: never met`
  return `${row} beats ${col}: ${rec.wins}–${rec.losses} (${Math.round((rec.wins / total) * 100)}%)`
}
</script>
