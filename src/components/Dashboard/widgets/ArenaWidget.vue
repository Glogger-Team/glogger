<template>
  <div class="flex flex-col gap-3 text-sm h-full">
    <div class="flex gap-1" role="tablist" aria-label="Arena views">
      <button
        v-for="tab in tabs"
        :key="tab"
        role="tab"
        :aria-selected="activeTab === tab"
        class="flex-1 cursor-pointer rounded border px-2 py-1 text-xs transition-colors"
        :class="activeTab === tab
          ? 'border-border-default bg-bg-secondary font-semibold text-text-primary'
          : 'border-transparent text-text-dim hover:text-text-secondary'"
        @click="activeTab = tab">
        {{ tab }}
      </button>
    </div>

    <template v-if="activeTab === 'Today'">
    <div class="flex flex-col gap-1.5">
      <span class="text-[11px] text-text-dim uppercase tracking-wide">Today's fight tips</span>
      <DataTable
        :columns="tipColumns"
        :rows="tipsTableRows"
        :row-key="(row) => row.tipster as string"
        :row-class="tipRowClass"
        :compact="true"
        :hoverable="false">
        <template #cell-tip="{ value, row }">
          <span :title="(row.fullBody as string) || undefined">{{ value }}</span>
        </template>
      </DataTable>
    </div>

    <div class="flex flex-col gap-1.5">
      <span class="text-[11px] text-text-dim uppercase tracking-wide">Today's record</span>
      <DataTable
        :columns="recordColumns"
        :rows="todayRecordRows"
        :row-key="(row) => row.fighter as string"
        :row-class="recordRowClass"
        :compact="true"
        :hoverable="false"
        empty-variant="compact"
        empty-text="No fights today yet"
        empty-hint="Records appear as Kuzavek's results land." />
    </div>
    </template>

    <template v-else-if='activeTab === "Full Day&apos;s Results"'>
    <div class="flex flex-col gap-1.5">
      <span class="text-[11px] text-text-dim uppercase tracking-wide">Today's results</span>
      <div v-if="todayMatches.length > 0" class="flex flex-col gap-1">
        <div
          v-for="m in todayMatches"
          :key="m.fought_at + '|' + m.fighter_a + '|' + m.fighter_b"
          class="flex items-baseline justify-between gap-2 text-xs"
          :title="m.fought_at">
          <span class="truncate">
            {{ m.fighter_a }} vs {{ m.fighter_b }} —
            <span class="font-semibold">{{ m.winner }}</span>
          </span>
          <span class="text-text-dim tabular-nums shrink-0">{{ shortTime(m.fought_at) }}</span>
        </div>
      </div>
      <div v-else class="text-[11px] text-text-dim italic">
        No arena fights recorded today yet. Stand in the casino arena while Kuzavek
        narrates — today's winners appear here.
      </div>
    </div>
    </template>

    <template v-else>
    <div class="flex flex-col gap-1.5">
      <span class="text-[11px] text-text-dim uppercase tracking-wide">Lifetime</span>
      <DataTable
        :columns="recordColumns"
        :rows="lifetimeRows"
        :row-key="(row) => row.fighter as string"
        :row-class="recordRowClass"
        :compact="true"
        :hoverable="false"
        empty-variant="compact"
        empty-text="No fights recorded yet"
        empty-hint="Lifetime records build from Kuzavek's results." />
    </div>
    </template>
  </div>
</template>

<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { useGameStateStore } from '../../../stores/gameStateStore'
import DataTable from '../../Shared/DataTable.vue'

const store = useGameStateStore()

onMounted(() => {
  store.fetchArenaStats()
  store.fetchArenaTips()
})

const stats = computed(() => store.arenaStats)
const tips = computed(() => store.arenaTips)

/** Arena views: Today first (default), then the fight log, then lifetime. */
const tabs = ['Today', "Full Day's Results", 'Lifetime Results'] as const
type ArenaTab = (typeof tabs)[number]
const activeTab = ref<ArenaTab>('Today')

/** Tip bodies carry chat markup (`<i>…</i>`); render plain text.
 *  Loops tag-removal to a fixpoint (one pass lets nested-tag smuggling
 *  through: `<scr<script>ipt>` collapses toward `<script>`), then strips
 *  any leftover angle brackets so smuggled residue (`ipt>`) cannot survive
 *  either. Bare `<`/`>` in prose are not preserved — chat tips need none. */
function stripTags(s: string): string {
  let prev: string
  let out = s
  do {
    prev = out
    out = prev.replace(/<[^>]*>/g, '')
  } while (out !== prev)
  return out.replace(/[<>]/g, '')
}

interface ParsedTip {
  favored: string
  other: string | null
  edge: number
}

/** Best-effort parse of Eveline's daily tip after tag-stripping.
 *  Returns null when the body doesn't match — the caller falls back to
 *  plain-text display so an unrecognized tip is never hidden. */
function parseTip(body: string): ParsedTip | null {
  const m =
    /If (.+?) and (.+?) fight today.*?that ([A-Z][\w '.\-]*?) will have the upper hand\.\s*\(\+(\d+)%\)/.exec(
      stripTags(body),
    )
  if (!m) return null
  const a = m[1].trim()
  const b = m[2].trim()
  const favored = m[3].trim()
  const edge = Number(m[4])
  return { favored, other: favored === a ? b : favored === b ? a : null, edge }
}

/** Closed arena roster (per the Gorgon wiki's casino arena page): the only
 *  fighters a tip can name. Single-fighter tips ("…Leo… (+5%)", applies to
 *  every fight that day) name exactly one of these; anything else falls
 *  through to first-sentence display. */
const ROSTER = ['Otis', 'Leo', 'Corrrak', 'Dura', 'Ushug', 'Vizlark', 'Gloz']

/** Best-effort parse of a single-fighter tip: the edge plus the one roster
 *  name in the body. Returns null when the edge is absent or when zero or
 *  2+ distinct roster names appear — the caller falls back to first-sentence
 *  display so the tip is never hidden or misattributed. */
function parseSingleTip(body: string): { favored: string; edge: number } | null {
  const text = stripTags(body)
  const edge = /\(\+(\d+)%\)/.exec(text)
  if (!edge) return null
  const present = ROSTER.filter((name) => new RegExp(`\\b${name}\\b`).test(text))
  if (present.length !== 1) return null
  return { favored: present[0], edge: Number(edge[1]) }
}

/** Lossless fallback: plain text minus the boilerplate opener, clamped to
 *  the first sentence / 140 chars. The full body stays in the row tooltip. */
function fallbackTip(body: string): string {
  let text = stripTags(body).trim().replace(/^(Here's a hot tip|Gotta hot tip for ya):\s*/i, '')
  const first = /[^.!?]*[.!?]/.exec(text)
  if (first) text = first[0].trim()
  if (text.length > 140) text = `${text.slice(0, 137).trimEnd()}…`
  return text
}

/** Naive-UTC "YYYY-MM-DD HH:MM:SS" → "HH:MM" for display. */
function shortTime(ts: string): string {
  return ts.length >= 16 ? ts.slice(11, 16) : ts
}

/** Fixed tipster roster in stable display order (exact other_player values).
 *  The tips table left-joins today's tips against this roster, frontend-only. */
const TIPSTER_ROSTER = [
  'Arianna Fangblade',
  'Eveline Rastin',
  'Irkima',
  'Mandibles',
  'Qatik',
]

/** Tips-table columns: who said it, the matchup, and the edge. */
const tipColumns = [
  { key: 'tipster', label: 'Tipster' },
  { key: 'tip', label: 'Tip' },
  { key: 'edge', label: '+/-', numeric: true },
]

/** Compact tip text + edge for a tip row (time + boilerplate dropped; the full
 *  stripped body stays in the row tooltip). The edge lives in its own column,
 *  so the Tip text carries the matchup without the embedded percentage.
 *  The backend parse wins (it covers (-N%) shapes the client regexes miss);
 *  unparseable rows fall back to the client parse, then first-sentence text
 *  with a blank edge so a tip is never hidden. */
function formatEdge(pct: number): string {
  return `${pct >= 0 ? '+' : ''}${pct}%`
}
function tipText(tip: {
  body: string
  edge_kind: string | null
  favored: string | null
  opponent: string | null
  edge_pct: number | null
}): { tip: string; edge: string } {
  if (tip.edge_kind === 'edge' && tip.favored && tip.opponent && tip.edge_pct !== null)
    return { tip: `${tip.favored} over ${tip.opponent}`, edge: formatEdge(tip.edge_pct) }
  if (tip.edge_kind === 'solo' && tip.favored && tip.edge_pct !== null)
    return { tip: `${tip.favored} all fights`, edge: formatEdge(tip.edge_pct) }
  const parsed = parseTip(tip.body)
  if (parsed && parsed.other)
    return { tip: `${parsed.favored} over ${parsed.other}`, edge: `+${parsed.edge}%` }
  if (parsed) return { tip: parsed.favored, edge: `+${parsed.edge}%` }
  const single = parseSingleTip(tip.body)
  if (single) return { tip: `${single.favored} all fights`, edge: `+${single.edge}%` }
  return { tip: fallbackTip(tip.body), edge: '' }
}

/** One row per roster tipster: today's tip, or a muted placeholder naming
 *  who to visit. Placeholders carry no edge text and are dim italic so they
 *  can never be mistaken for a tip. */
const tipsTableRows = computed(() =>
  TIPSTER_ROSTER.map((tipster) => {
    const tip = tips.value.find((t) => t.tipster === tipster)
    if (!tip) {
      const first = tipster.split(' ')[0]
      return { tipster, tip: `No tip yet — talk to ${first}`, edge: '', fullBody: '', placeholder: true }
    }
    const { tip: text, edge } = tipText(tip)
    return { tipster, tip: text, edge, fullBody: stripTags(tip.body), placeholder: false }
  }),
)
/** Placeholder rows render dim italic; real tips keep default text. */
function tipRowClass(row: Record<string, unknown>): string {
  return row.placeholder ? 'italic text-text-dim' : ''
}

/** Eastern calendar day of a date ("YYYY-MM-DD"), machine-tz independent. */
function easternDay(d: Date): string {
  return new Intl.DateTimeFormat('en-CA', {
    timeZone: 'America/New_York',
    year: 'numeric',
    month: '2-digit',
    day: '2-digit',
  }).format(d)
}

/** Today's Kuzavek-narrated results. fought_at is log-local wall clock
 *  ("YYYY-MM-DD HH:MM:SS"), so parse it as machine-local and compare its
 *  Eastern date — correct on any machine tz (mirrors the store's
 *  America/New_York server-time anchor). */
const todayMatches = computed(() => {
  const today = easternDay(new Date())
  return stats.value.recent.filter(
    (m) => easternDay(new Date(m.fought_at.replace(' ', 'T'))) === today,
  )
})

/** Today's per-fighter W-L, tallied from today's Eastern-day matches. Only
 *  fighters who fought today appear. */
const todayRecords = computed(() => {
  const map = new Map<string, { wins: number; losses: number }>()
  for (const m of todayMatches.value) {
    for (const name of [m.fighter_a, m.fighter_b]) {
      if (!map.has(name)) map.set(name, { wins: 0, losses: 0 })
    }
    const w = map.get(m.winner)
    if (w) w.wins += 1
    else map.set(m.winner, { wins: 1, losses: 0 })
    for (const name of [m.fighter_a, m.fighter_b]) {
      if (name !== m.winner) map.get(name)!.losses += 1
    }
  }
  return [...map]
    .map(([name, r]) => ({ name, ...r, ratio: r.wins / (r.wins + r.losses) }))
    .sort((a, b) => b.wins - a.wins || a.losses - b.losses || a.name.localeCompare(b.name))
})

/** Shared record-table columns: Fighter | W | L | Win%. */
const recordColumns = [
  { key: 'fighter', label: 'Fighter' },
  { key: 'w', label: 'W', numeric: true },
  { key: 'l', label: 'L', numeric: true },
  { key: 'winPct', label: 'Win%', numeric: true },
]

/** Today's rows for the record table. */
const todayRecordRows = computed(() =>
  todayRecords.value.map((r) => ({
    fighter: r.name,
    w: r.wins,
    l: r.losses,
    winPct: `${Math.round(r.ratio * 100)}%`,
    ratio: r.ratio,
  })),
)

/** Win-ratio row tint from the theme's value semantics (no painted
 *  backgrounds): undefeated strongest green, winless red, .500 default text.
 *  (--success/--error do not exist in the theme; value-positive/negative are
 *  its gain/loss equivalents.) */
function recordRowClass(row: Record<string, unknown>): string {
  const ratio = row.ratio as number
  if (ratio >= 1) return 'text-value-positive'
  if (ratio <= 0) return 'text-value-negative'
  return ''
}

/** Lifetime standings: all known fighters, most wins first. */
const standings = computed(() =>
  [...stats.value.fighters].sort(
    (a, b) => b.wins - a.wins || b.win_pct - a.win_pct || a.name.localeCompare(b.name),
  ),
)

/** Lifetime rows for the record table. */
const lifetimeRows = computed(() =>
  standings.value.map((f) => ({
    fighter: f.name,
    w: f.wins,
    l: f.losses,
    winPct: `${f.win_pct.toFixed(0)}%`,
    ratio: f.win_pct / 100,
  })),
)
</script>
