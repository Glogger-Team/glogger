<template>
  <div class="flex flex-col gap-3 min-w-80 max-w-md max-h-[70vh] overflow-y-auto">
    <div class="text-[11px] text-text-dim">
      Same list <span class="font-mono">/events</span> shows in-game. Edits save to this machine only —
      to ship a schedule fix to everyone, update
      <span class="font-mono">public/data/server-events.json</span> and release.
    </div>

    <!-- Weekly jams -->
    <div>
      <div class="text-xs font-bold text-text-secondary uppercase tracking-wide mb-1">Weekly jams</div>
      <div v-if="allRecurring.length === 0" class="text-xs text-text-dim italic">None — add one below.</div>
      <div v-for="e in allRecurring" :key="e.id" class="py-1 border-b border-border-default/30 last:border-0">
        <div class="flex items-center gap-1.5">
          <span class="text-xs text-text-primary font-semibold truncate flex-1 min-w-0">{{ e.name }}</span>
          <button class="text-xs text-red-400 hover:text-red-300 cursor-pointer shrink-0" @click="removeRecurring(e.id)">Remove</button>
        </div>
        <div class="text-[11px] text-text-muted">{{ describeRecurring(e) }}</div>
        <div class="text-[11px] text-text-dim">{{ e.location }} · {{ serverList(e.servers) }}</div>
      </div>
    </div>

    <!-- Add weekly jam -->
    <div class="rounded border border-border-default/50 p-2 flex flex-col gap-1.5">
      <div class="text-xs font-bold text-text-secondary uppercase tracking-wide">Add weekly jam</div>
      <input v-model="rjName" type="text" placeholder="Name (e.g. Poetry Jam)"
        class="w-full px-2 py-1 bg-surface-base border border-border-default rounded text-xs text-text-primary placeholder-text-muted focus:outline-none focus:border-accent-gold/50" />
      <div class="flex gap-1.5">
        <select v-model="rjWeekday"
          class="flex-1 px-1 py-1 bg-surface-base border border-border-default rounded text-xs text-text-primary focus:outline-none focus:border-accent-gold/50">
          <option v-for="(d, i) in WEEKDAYS" :key="d" :value="i">{{ d }}</option>
        </select>
        <input v-model="rjTime" type="time"
          class="w-24 px-1 py-1 bg-surface-base border border-border-default rounded text-xs text-text-primary focus:outline-none focus:border-accent-gold/50" />
      </div>
      <input v-model="rjLocation" type="text" placeholder="Location (e.g. Serbule Inn)"
        class="w-full px-2 py-1 bg-surface-base border border-border-default rounded text-xs text-text-primary placeholder-text-muted focus:outline-none focus:border-accent-gold/50" />
      <input v-model="rjServers" type="text" placeholder="Servers, comma-separated (blank = all)"
        title="Which servers this event runs on, e.g. Dreva, Arisetsu. Blank = every server."
        class="w-full px-2 py-1 bg-surface-base border border-border-default rounded text-xs text-text-primary placeholder-text-muted focus:outline-none focus:border-accent-gold/50" />
      <button :disabled="!canAddRecurring" :class="addBtnClass" @click="handleAddRecurring">Add jam</button>
    </div>

    <!-- Seasonal events -->
    <div>
      <div class="text-xs font-bold text-text-secondary uppercase tracking-wide mb-1">Seasonal events</div>
      <div v-if="allCustom.length === 0" class="text-xs text-text-dim italic">None.</div>
      <div v-for="e in allCustom" :key="e.id" class="py-1 border-b border-border-default/30 last:border-0">
        <div class="flex items-center gap-1.5">
          <span class="text-xs text-text-primary font-semibold truncate flex-1 min-w-0">{{ e.name }}</span>
          <button class="text-xs text-red-400 hover:text-red-300 cursor-pointer shrink-0" @click="removeCustom(e.id)">Remove</button>
        </div>
        <div class="text-[11px] text-text-muted">{{ e.startsAt.slice(0, 10) }} → {{ e.endsAt ? e.endsAt.slice(0, 10) : 'open-ended' }} · {{ e.location }}</div>
        <div v-if="e.note" class="text-[11px] text-text-dim">{{ e.note }}</div>
      </div>
    </div>

    <!-- Add seasonal event -->
    <div class="rounded border border-border-default/50 p-2 flex flex-col gap-1.5">
      <div class="text-xs font-bold text-text-secondary uppercase tracking-wide">Add seasonal event</div>
      <input v-model="cName" type="text" placeholder="Name (e.g. Zhia Lian Celebration)"
        class="w-full px-2 py-1 bg-surface-base border border-border-default rounded text-xs text-text-primary placeholder-text-muted focus:outline-none focus:border-accent-gold/50" />
      <div class="flex gap-1.5 items-center">
        <label class="text-[11px] text-text-dim w-10 shrink-0">From</label>
        <input v-model="cStart" type="date"
          class="flex-1 px-1 py-1 bg-surface-base border border-border-default rounded text-xs text-text-primary focus:outline-none focus:border-accent-gold/50" />
      </div>
      <div class="flex gap-1.5 items-center">
        <label class="text-[11px] text-text-dim w-10 shrink-0">To</label>
        <input v-model="cEnd" type="date" placeholder="optional"
          title="Optional end date — the event drops off the list after this day."
          class="flex-1 px-1 py-1 bg-surface-base border border-border-default rounded text-xs text-text-primary focus:outline-none focus:border-accent-gold/50" />
      </div>
      <input v-model="cLocation" type="text" placeholder="Location"
        class="w-full px-2 py-1 bg-surface-base border border-border-default rounded text-xs text-text-primary placeholder-text-muted focus:outline-none focus:border-accent-gold/50" />
      <input v-model="cServers" type="text" placeholder="Servers, comma-separated (blank = all)"
        class="w-full px-2 py-1 bg-surface-base border border-border-default rounded text-xs text-text-primary placeholder-text-muted focus:outline-none focus:border-accent-gold/50" />
      <button :disabled="!canAddCustom" :class="addBtnClass" @click="handleAddCustom">Add event</button>
    </div>

    <button class="self-start text-xs text-text-dim hover:text-text-secondary cursor-pointer" @click="resetAll">
      Restore shipped schedule
    </button>
  </div>
</template>

<script setup lang="ts">
import { ref, computed } from 'vue'
import {
  useServerEvents,
  type RecurringServerEvent,
} from '../../../composables/useServerEvents'

const {
  allRecurring, allCustom, addRecurring, removeRecurring,
  addCustom, removeCustom, resetRecurring, resetCustom,
} = useServerEvents()

const WEEKDAYS = ['Sunday', 'Monday', 'Tuesday', 'Wednesday', 'Thursday', 'Friday', 'Saturday']

function describeRecurring(e: RecurringServerEvent): string {
  const h12 = e.hour % 12 === 0 ? 12 : e.hour % 12
  const ampm = e.hour < 12 ? 'AM' : 'PM'
  const mm = String(e.minute).padStart(2, '0')
  return `Every ${WEEKDAYS[e.weekday]} at ${h12}:${mm} ${ampm} US Eastern`
}

function serverList(servers: string[]): string {
  return servers.length === 0 ? 'all servers' : servers.join(', ')
}

const addBtnClass =
  'px-2 py-1 bg-accent-gold/20 text-accent-gold border border-accent-gold/40 rounded text-xs hover:bg-accent-gold/30 transition-colors cursor-pointer disabled:opacity-40 disabled:cursor-not-allowed self-start'

// ── Add-weekly form ──
const rjName = ref('')
const rjWeekday = ref(0)
const rjTime = ref('14:00')
const rjLocation = ref('')
const rjServers = ref('')

function parseServers(input: string): string[] {
  return input.split(',').map((s) => s.trim()).filter((s) => s.length > 0)
}

const canAddRecurring = computed(
  () => rjName.value.trim().length > 0 && rjLocation.value.trim().length > 0 && /^(\d{2}):(\d{2})$/.test(rjTime.value),
)

function handleAddRecurring() {
  if (!canAddRecurring.value) return
  const [h, m] = rjTime.value.split(':').map(Number)
  addRecurring({
    name: rjName.value.trim(),
    weekday: rjWeekday.value,
    hour: h,
    minute: m,
    location: rjLocation.value.trim(),
    servers: parseServers(rjServers.value),
  })
  rjName.value = ''
  rjLocation.value = ''
  rjServers.value = ''
}

// ── Add-seasonal form ──
const cName = ref('')
const cStart = ref('')
const cEnd = ref('')
const cLocation = ref('')
const cServers = ref('')

const canAddCustom = computed(() => cName.value.trim().length > 0 && cStart.value.length > 0)

function handleAddCustom() {
  if (!canAddCustom.value) return
  addCustom({
    name: cName.value.trim(),
    startsAt: new Date(cStart.value + 'T00:00:00Z').toISOString(),
    endsAt: cEnd.value ? new Date(cEnd.value + 'T23:59:59Z').toISOString() : null,
    location: cLocation.value.trim() || 'See /events',
    note: '',
    servers: parseServers(cServers.value),
  })
  cName.value = ''
  cStart.value = ''
  cEnd.value = ''
  cLocation.value = ''
  cServers.value = ''
}

function resetAll() {
  resetRecurring()
  resetCustom()
}
</script>
