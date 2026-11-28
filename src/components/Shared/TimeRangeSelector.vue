<template>
  <div class="flex flex-col gap-2">
    <div class="flex items-center gap-1 flex-wrap">
      <button
        v-for="opt in presetOptions"
        :key="opt.id"
        @click="applyPreset(opt.id)"
        :class="['px-2 py-1 text-xs rounded font-medium cursor-pointer', activePreset === opt.id ? 'bg-surface-elevated text-accent-gold' : 'text-text-secondary hover:text-text-primary']">
        {{ opt.label }}
      </button>
      <button
        @click="showCustom = !showCustom"
        :class="['px-2 py-1 text-xs rounded font-medium cursor-pointer', activePreset === 'custom' ? 'bg-surface-elevated text-accent-gold' : 'text-text-secondary hover:text-text-primary']">
        Custom
      </button>
    </div>

    <div v-if="showCustom || activePreset === 'custom'" class="flex items-center gap-2 flex-wrap text-xs text-text-secondary">
      <label class="flex items-center gap-1">From
        <input v-model="customStart" type="datetime-local" @change="applyCustom"
          class="bg-surface-dark border border-border-default rounded px-2 py-1 text-xs text-text-primary" />
      </label>
      <label class="flex items-center gap-1">To
        <input v-model="customEnd" type="datetime-local" @change="applyCustom"
          class="bg-surface-dark border border-border-default rounded px-2 py-1 text-xs text-text-primary" />
      </label>
    </div>

    <div class="flex items-center gap-2 flex-wrap text-xs">
      <span class="text-text-dim">Window: {{ formatAnyTimestamp(range.start) }} → {{ formatAnyTimestamp(range.end) }} ({{ rangeLabel }})</span>
      <div class="flex items-center gap-1">
        <input v-model="newLabel" placeholder="Save this window as…"
          class="bg-surface-dark border border-border-default rounded px-2 py-1 text-xs text-text-primary w-40" />
        <button @click="saveWindow"
          class="px-2 py-1 text-xs rounded border border-border-default text-text-secondary cursor-pointer hover:bg-surface-elevated hover:text-text-primary">
          Save
        </button>
      </div>
    </div>

    <div v-if="savedWindows.length > 0" class="flex items-center gap-1 flex-wrap">
      <button
        v-for="w in savedWindows"
        :key="w.label"
        @click="applySaved(w)"
        :title="`${w.start_ts} → ${w.end_ts}`"
        class="px-2 py-1 text-xs rounded border border-border-default text-text-secondary cursor-pointer hover:bg-surface-elevated hover:text-text-primary">
        {{ w.label }}
      </button>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, onMounted } from 'vue'
import { useViewPrefs } from '../../composables/useViewPrefs'
import { formatAnyTimestamp } from '../../composables/useTimestamp'
import { easternOffsetMinutes } from '../../composables/useStatehelmTracker'

export interface TimeRange {
  start: string
  end: string
}

export interface SavedWindow {
  label: string
  start_ts: string
  end_ts: string
}

const emit = defineEmits<{
  (e: 'change', range: TimeRange): void
}>()

/** Backend UTC contract: space-format naive UTC ("YYYY-MM-DD HH:MM:SS"). */
function toSpaceUTC(d: Date): string {
  return d.toISOString().slice(0, 19).replace('T', ' ')
}
/** Eastern-midnight-as-UTC start of the current game day, machine-tz
 *  independent. The game day resets at midnight America/New_York (server
 *  time — gameStateStore server-time anchor; mirrors the arena
 *  `get_today_tips` Eastern-midnight UTC range and ArenaWidget's
 *  machine-tz-independent Eastern day). Offset is re-resolved at the
 *  midnight itself so a DST switch inside today doesn't shift the start. */
function easternDayStartUtc(now: Date): Date {
  const offsetNow = easternOffsetMinutes(now)
  const easternNow = new Date(now.getTime() + offsetNow * 60000)
  const wallMidnight = Date.UTC(
    easternNow.getUTCFullYear(), easternNow.getUTCMonth(), easternNow.getUTCDate(), 0, 0, 0, 0)
  const offsetMidnight = easternOffsetMinutes(new Date(wallMidnight - offsetNow * 60000))
  return new Date(wallMidnight - offsetMidnight * 60000)
}

type PresetId = 'today' | 'd1' | 'd7' | 'm1' | 'm3' | 'm6' | 'y1' | 'custom' | 'saved'
const presetOptions: Array<{ id: PresetId; label: string; hours?: number; months?: number; range?: string }> = [
  { id: 'today', label: 'Today', range: 'since Eastern midnight (game day)' },
  { id: 'd1', label: '1D', hours: 24, range: 'last 24h' },
  { id: 'd7', label: '7D', hours: 24 * 7, range: 'last 7 days' },
  { id: 'm1', label: '1M', months: 1, range: 'last 1 calendar month' },
  { id: 'm3', label: '3M', months: 3, range: 'last 3 calendar months' },
  { id: 'm6', label: '6M', months: 6, range: 'last 6 calendar months' },
  { id: 'y1', label: '1Y', months: 12, range: 'last 12 calendar months' },
]

/** Calendar-aware month subtraction in UTC: Mar 31 minus 1 month lands on
 *  Feb 28/29, not Mar 3 (which naive Date.UTC month underflow would give).
 *  Day clamps to the target month's length; wall-clock time is preserved. */
function subtractMonthsUtc(now: Date, months: number): Date {
  const target = now.getUTCMonth() - months
  const year = now.getUTCFullYear() + Math.floor(target / 12)
  const month = ((target % 12) + 12) % 12
  const dim = new Date(Date.UTC(year, month + 1, 0)).getUTCDate()
  const day = Math.min(now.getUTCDate(), dim)
  return new Date(Date.UTC(year, month, day,
    now.getUTCHours(), now.getUTCMinutes(), now.getUTCSeconds()))
}

const activePreset = ref<PresetId>('today')
const showCustom = ref(false)
const customStart = ref('')
const customEnd = ref('')
const newLabel = ref('')
const range = ref<TimeRange>({ start: '', end: '' })

const { prefs, update } = useViewPrefs('timeboxed', { savedWindows: [] as SavedWindow[] })
const savedWindows = computed(() => (prefs.value.savedWindows ?? []) as SavedWindow[])

const rangeLabel = computed(() => {
  if (activePreset.value === 'today') return 'since Eastern midnight (game day)'
  const opt = presetOptions.find((o) => o.id === activePreset.value)
  if (opt?.range) return opt.range
  const saved = savedWindows.value.find((w) => w.start_ts === range.value.start && w.end_ts === range.value.end)
  if (saved) return saved.label
  return 'custom'
})

function setRange(start: string, end: string) {
  range.value = { start, end }
  emit('change', range.value)
}

function applyPreset(id: PresetId) {
  activePreset.value = id
  const now = new Date()
  if (id === 'today') {
    setRange(toSpaceUTC(easternDayStartUtc(now)), toSpaceUTC(now))
    return
  }
  const opt = presetOptions.find((o) => o.id === id)
  if (opt?.months) {
    setRange(toSpaceUTC(subtractMonthsUtc(now, opt.months)), toSpaceUTC(now))
  } else if (opt?.hours) {
    setRange(toSpaceUTC(new Date(now.getTime() - opt.hours * 3600_000)), toSpaceUTC(now))
  }
}

function applyCustom() {
  if (!customStart.value || !customEnd.value) return
  activePreset.value = 'custom'
  // datetime-local parses as local time; the backend contract is UTC.
  setRange(toSpaceUTC(new Date(customStart.value)), toSpaceUTC(new Date(customEnd.value)))
}

function saveWindow() {
  const label = newLabel.value.trim()
  if (!label || !range.value.start) return
  const next = savedWindows.value.filter((w) => w.label !== label)
  next.push({ label, start_ts: range.value.start, end_ts: range.value.end })
  update({ savedWindows: next })
  newLabel.value = ''
}

function applySaved(w: SavedWindow) {
  activePreset.value = 'saved'
  showCustom.value = false
  setRange(w.start_ts, w.end_ts)
}

onMounted(() => {
  applyPreset('today')
})
</script>
