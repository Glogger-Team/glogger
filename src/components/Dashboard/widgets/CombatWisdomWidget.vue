<template>
  <div class="flex flex-col gap-3 text-sm h-full">
    <!-- Header: session total -->
    <div class="flex items-baseline justify-between gap-2">
      <span class="text-xs text-text-dim uppercase tracking-wide">Wisdom this session</span>
      <span class="text-base font-semibold text-accent-gold tabular-nums">
        {{ session.total.toLocaleString() }}
      </span>
    </div>

    <div class="h-px bg-border-default" />

    <!-- Earned this session -->
    <div class="flex flex-col gap-1.5 min-h-0">
      <span class="text-xs text-text-dim uppercase tracking-wide">Earned this session</span>
      <div v-if="session.earns.length === 0" class="text-xs text-text-dim italic">
        No Combat Wisdom earned yet this session.
      </div>
      <div
        v-else
        class="flex flex-col gap-1 overflow-y-auto pr-1 min-h-0"
        :style="{ height: earnedHeight + 'px' }">
        <div
          v-for="(e, i) in session.earns"
          :key="i"
          class="flex items-baseline justify-between gap-2">
          <span class="text-text-primary truncate">
            {{ e.name ?? 'Prodigy Level' }}
            <span v-if="e.zone" class="text-text-dim text-[11px]">({{ e.zone }})</span>
          </span>
          <span class="text-accent-gold tabular-nums whitespace-nowrap">+{{ e.amount }}</span>
        </div>
      </div>
    </div>

    <!-- Draggable divider: shrinks Earned to give Monster cooldowns more room -->
    <div
      class="group relative h-px bg-border-default cursor-row-resize select-none -my-1 py-1"
      title="Drag to resize"
      @pointerdown="startDrag">
      <div
        class="absolute inset-x-0 top-1/2 -translate-y-1/2 h-0.5 rounded transition-colors"
        :class="dragging ? 'bg-accent-gold' : 'bg-transparent group-hover:bg-accent-gold/50'" />
    </div>

    <!-- Per-monster cooldowns -->
    <div class="flex flex-col gap-1.5 min-h-0 flex-1">
      <span class="text-xs text-text-dim uppercase tracking-wide">Monster cooldowns</span>
      <div v-if="cooldowns.length === 0" class="text-xs text-text-dim italic">
        No monster history yet.
      </div>
      <div v-else class="flex flex-col gap-1 overflow-y-auto pr-1">
        <div v-for="m in cooldowns" :key="m.name" class="flex items-baseline justify-between gap-2">
          <span class="text-text-primary truncate" :title="cooldownTitle(m)">
            <span class="mr-1.5 rounded border px-1 py-px text-[10px] font-medium uppercase tracking-wide" :class="m.kind === 'elite' ? 'border-accent-gold/40 text-accent-gold' : m.kind === 'named' ? 'border-accent-blue/40 text-accent-blue' : 'border-border-default text-text-dim'">{{ m.kind }}</span>
            {{ m.name }}
          </span>
          <span class="tabular-nums whitespace-nowrap text-xs" :class="m.ready ? 'text-emerald-400' : 'text-text-dim'">
            {{ m.ready ? 'Ready' : m.label }}
          </span>
        </div>
      </div>
    </div>

    <p class="text-[10px] text-text-dim/70 leading-tight mt-auto">
      Cooldowns are learned from your own kills (shortest gap seen), with a wiki default until then.
    </p>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted } from 'vue'
import {
  useGameStateStore,
  type CombatWisdomMonster,
} from '../../../stores/gameStateStore'

const store = useGameStateStore()

// Live clock so countdowns tick.
const now = ref(Date.now())
let timer: ReturnType<typeof setInterval> | undefined

// --- Resizable split between "Earned this session" and "Monster cooldowns" ---
const EARNED_MIN = 48
const EARNED_MAX = 400
const EARNED_KEY = 'combatWisdom.earnedHeight'
const earnedHeight = ref(clampEarned(Number(localStorage.getItem(EARNED_KEY)) || 160))
const dragging = ref(false)
let dragStartY = 0
let dragStartHeight = 0

function clampEarned(h: number): number {
  return Math.min(EARNED_MAX, Math.max(EARNED_MIN, Math.round(h)))
}

function startDrag(e: PointerEvent) {
  dragging.value = true
  dragStartY = e.clientY
  dragStartHeight = earnedHeight.value
  ;(e.target as HTMLElement).setPointerCapture?.(e.pointerId)
  window.addEventListener('pointermove', onDrag)
  window.addEventListener('pointerup', endDrag)
  e.preventDefault()
}
function onDrag(e: PointerEvent) {
  earnedHeight.value = clampEarned(dragStartHeight + (e.clientY - dragStartY))
}
function endDrag() {
  dragging.value = false
  localStorage.setItem(EARNED_KEY, String(earnedHeight.value))
  window.removeEventListener('pointermove', onDrag)
  window.removeEventListener('pointerup', endDrag)
}

onMounted(() => {
  store.fetchCombatWisdomMonsters()
  timer = setInterval(() => {
    now.value = Date.now()
  }, 1000)
})
onUnmounted(() => {
  if (timer) clearInterval(timer)
  window.removeEventListener('pointermove', onDrag)
  window.removeEventListener('pointerup', endDrag)
})

const session = computed(() => store.combatWisdomSession)

/** Named/boss reuse timer cap (wiki: named soloable = once per 24h). */
const WIKI_MAX_COOLDOWN_SECS = 24 * 3600

/** Elite check shared by the badge and the timer: "Defeated" is the elite verb,
 * and the name rule covers Elite-named monsters reported via a "Killed" line. */
function isElite(m: { verb: string; name: string }): boolean {
  // "Defeated" = elite: chance-based each kill, no real cooldown.
  return m.verb === 'Defeated' || /^Elite\b/i.test(m.name)
}

/** Cooldown in seconds for a monster: observed minimum, else wiki default. */
function cooldownSecs(m: CombatWisdomMonster): number {
  if (isElite(m)) return 0
  // Named/boss: the observed shortest gap is an *upper bound* on the true
  // cooldown, so cap it at the wiki max (24h). This lets a real boss timer
  // shorten below 24h (e.g. ~3h) while preventing a rarely-killed mob — whose
  // only observed gap is weeks — from showing an absurd multi-week countdown.
  if (m.min_gap_secs != null) return Math.min(m.min_gap_secs, WIKI_MAX_COOLDOWN_SECS)
  return WIKI_MAX_COOLDOWN_SECS
}

function formatDuration(secs: number): string {
  if (secs <= 0) return 'Ready'
  const h = Math.floor(secs / 3600)
  const min = Math.floor((secs % 3600) / 60)
  const s = Math.floor(secs % 60)
  if (h > 0) return `${h}h ${min}m`
  if (min > 0) return `${min}m ${s}s`
  return `${s}s`
}

const cooldowns = computed(() => {
  return store.combatWisdomMonsters
    .map((m) => {
      const readyAt = m.last_earned_ms + cooldownSecs(m) * 1000
      const remainMs = readyAt - now.value
      const ready = remainMs <= 0
      const kind = isElite(m) ? 'elite' : m.min_gap_secs != null ? 'named' : 'unknown'
      return {
        name: m.name,
        verb: m.verb,
        min_gap_secs: m.min_gap_secs,
        kind,
        ready,
        remainMs,
        label: formatDuration(Math.ceil(remainMs / 1000)),
      }
    })
    .sort((a, b) => {
      // Not-ready first (soonest-ready first), then ready ones.
      if (a.ready !== b.ready) return a.ready ? 1 : -1
      return a.remainMs - b.remainMs
    })
})

function cooldownTitle(m: { verb: string; min_gap_secs: number | null; kind?: string }): string {
  if (m.kind === 'elite' || m.verb === 'Defeated') return 'Elite — chance each kill'
  if (m.min_gap_secs != null) {
    return `Shortest gap seen ${formatDuration(m.min_gap_secs)} — timer capped at 24h wiki default`
  }
  return 'Likely named — 24h wiki default until observed'
}
</script>
