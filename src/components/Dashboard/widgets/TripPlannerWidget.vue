<template>
  <div class="flex flex-col gap-2">
    <!-- Zone selection row -->
    <div class="flex items-center gap-2">
      <div class="flex-1">
        <label class="text-[0.6rem] text-text-dim uppercase tracking-wide">From</label>
        <div class="flex items-center gap-1">
          <select
            v-model="startZone"
            class="flex-1 px-2 py-1 rounded bg-surface-elevated border border-border-light text-xs text-text-primary">
            <option v-for="z in ZONES" :key="z.key" :value="z.key">{{ z.name }}</option>
          </select>
          <button
            class="px-1.5 py-1 rounded text-[0.6rem] bg-surface-elevated border border-border-light text-text-muted hover:text-text-primary shrink-0"
            title="Use current zone"
            @click="useCurrentZone">
            Current
          </button>
          <button
            v-if="homeZone"
            class="px-1.5 py-1 rounded text-[0.6rem] bg-surface-elevated border border-border-light text-text-muted hover:text-text-primary shrink-0"
            title="Use home zone"
            @click="useHomeZone">
            Home
          </button>
        </div>
      </div>

      <span class="text-text-muted text-xs mt-3.5">&rarr;</span>

      <div class="flex-1">
        <label class="text-[0.6rem] text-text-dim uppercase tracking-wide">To</label>
        <select
          v-model="endZone"
          class="w-full px-2 py-1 rounded bg-surface-elevated border border-border-light text-xs text-text-primary">
          <option v-for="z in ZONES" :key="z.key" :value="z.key">{{ z.name }}</option>
        </select>
      </div>
    </div>

    <!-- Travel options -->
    <div class="flex items-center gap-3 text-[0.6rem] text-text-dim">
      <label class="flex items-center gap-1 cursor-pointer">
        <input v-model="useTeleports" type="checkbox" class="accent-accent-gold" />
        Use teleports
      </label>
      <label v-if="useTeleports" class="flex items-center gap-1 cursor-pointer">
        <input v-model="useTpMachine" type="checkbox" class="accent-accent-gold" />
        TP Machine
      </label>
    </div>

    <!-- Bind summary (compact) -->
    <div v-if="useTeleports" class="flex flex-wrap gap-x-3 gap-y-0.5 text-[0.6rem] text-text-dim">
      <span v-if="config.primaryBind">
        Bind 1: <span class="text-text-secondary">{{ config.primaryBind }}</span>
      </span>
      <span v-if="config.secondaryBind">
        Bind 2: <span class="text-text-secondary">{{ config.secondaryBind }}</span>
      </span>
      <span v-if="config.mushroomCircle">
        Circle: <span class="text-text-secondary">{{ config.mushroomCircle }}</span>
      </span>
      <span
        v-if="!config.primaryBind && !config.secondaryBind && !config.mushroomCircle"
        class="italic">
        No binds configured — use settings gear
      </span>
    </div>

    <!-- Home zone indicator -->
    <div v-if="homeZone" class="flex items-center gap-1 text-[0.6rem] text-text-dim">
      <span>Home:</span>
      <span class="text-text-secondary">{{ zoneName(homeZone) }}</span>
    </div>

    <!-- Plan button -->
    <button
      class="w-full px-2 py-1.5 rounded text-xs font-medium bg-accent-gold/15 border border-accent-gold/30 text-accent-gold hover:bg-accent-gold/25 cursor-pointer transition-colors"
      :disabled="!startZone || !endZone || startZone === endZone || planning"
      @click="planRoute">
      {{ planning ? 'Planning...' : 'Plan Route' }}
    </button>

    <!-- Route display -->
    <div v-if="route" class="flex flex-col gap-0.5 overflow-y-auto max-h-64 pr-1">
      <div class="flex items-center justify-between text-[0.6rem] text-text-dim mb-1">
        <span>{{ route.steps.length }} steps</span>
        <span>{{ route.total_hops }} hop{{ route.total_hops !== 1 ? 's' : '' }}</span>
      </div>
      <div
        v-for="(step, i) in route.steps"
        :key="i"
        class="flex items-start gap-2 py-1 px-1.5 rounded text-xs"
        :class="stepClass(step)">
        <span class="shrink-0 w-4 text-text-muted text-[0.6rem] text-right mt-0.5">{{ i + 1 }}</span>
        <span :class="step.action === 'travel' ? 'text-text-dim italic' : 'text-text-primary'">
          {{ step.details }}
        </span>
      </div>
    </div>

    <div v-if="error" class="text-accent-red text-xs">{{ error }}</div>
  </div>
</template>

<script setup lang="ts">
import { ref, reactive, computed, watch, onMounted } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { useGameStateStore } from '../../../stores/gameStateStore'
import { useSettingsStore } from '../../../stores/settingsStore'
import { CASINO_PORTAL_BY_PHASE, type MoonPhaseName } from '../../../composables/useMoonPhase'

const gameState = useGameStateStore()
const settingsStore = useSettingsStore()

// ── Zone list (matches zone_graph.rs overworld zones) ───────────────────────

const ZONES = [
  { key: 'AreaNewbieIsland', name: 'Anagoge Island' },
  { key: 'AreaSerbule', name: 'Serbule' },
  { key: 'AreaSerbule2', name: 'Serbule Hills' },
  { key: 'AreaEltibule', name: 'Eltibule' },
  { key: 'AreaSunVale', name: 'Sun Vale' },
  { key: 'AreaKurMountains', name: 'Kur Mountains' },
  { key: 'AreaCasino', name: 'Red Wing Casino' },
  { key: 'AreaDesert1', name: 'Ilmari' },
  { key: 'AreaRahu', name: 'Rahu' },
  { key: 'AreaGazluk', name: 'Gazluk' },
  { key: 'AreaFaeRealm1', name: 'Fae Realm' },
  { key: 'AreaPovus', name: 'Povus' },
  { key: 'AreaVidaria', name: 'Vidaria' },
  { key: 'AreaStatehelm', name: 'Statehelm' },
  { key: 'AreaPlanes', name: 'Winter Nexus' },
]

// Map friendly area names (from game state) to CDN keys
const FRIENDLY_TO_KEY: Record<string, string> = {}
for (const z of ZONES) {
  FRIENDLY_TO_KEY[z.name] = z.key
}
// Extra mappings for game state names that differ
FRIENDLY_TO_KEY['Ilmari Desert'] = 'AreaDesert1'
FRIENDLY_TO_KEY['Gazluk Plateau'] = 'AreaGazluk'
FRIENDLY_TO_KEY['Existential Planes'] = 'AreaPlanes'

// ── Casino portal: Rahu vs Statehelm edge comes from the moon phase ──────────

const casinoPortal = ref<string | null>(null)

async function loadCasinoPortal() {
  try {
    const phase = await invoke<{ game_phase: string }>('get_current_moon_phase')
    const dest = CASINO_PORTAL_BY_PHASE[phase.game_phase as MoonPhaseName]
    casinoPortal.value = dest ? dest.toLowerCase() : null
  } catch {
    // Moon phase unavailable, solver will use both edges
  }
}

// ── State ───────────────────────────────────────────────────────────────────

const startZone = ref(ZONES[1].key) // default: Serbule
const endZone = ref(ZONES[8].key)   // default: Rahu
const useTeleports = ref(false)
const useTpMachine = ref(false)
const planning = ref(false)
const error = ref('')

interface PlannedRoute {
  steps: { zone: string; action: string; details: string }[]
  total_hops: number
}
const route = ref<PlannedRoute | null>(null)

// ── Home Zone (per-character) ──────────────────────────────────────────────

const homeZoneKey = computed(() => {
  const char = settingsStore.settings.activeCharacterName
  const server = settingsStore.settings.activeServerName
  if (!char || !server) return null
  return `tripPlanner.homeZone.${char}.${server}`
})

const homeZone = ref<string | null>(null)

function loadHomeZone() {
  if (homeZoneKey.value) {
    homeZone.value = localStorage.getItem(homeZoneKey.value) || null
  } else {
    homeZone.value = null
  }
}

function useHomeZone() {
  if (homeZone.value) {
    startZone.value = homeZone.value
  }
}

// ── Config (persisted to localStorage, editable via config panel) ───────────

const CONFIG_KEY = 'tripPlannerWidget.config'

interface TripPlannerConfig {
  primaryBind: string | null
  secondaryBind: string | null
  mushroomCircle: string | null
}

const defaultConfig: TripPlannerConfig = {
  primaryBind: null,
  secondaryBind: null,
  mushroomCircle: null,
}

function loadConfig(): TripPlannerConfig {
  try {
    const raw = localStorage.getItem(CONFIG_KEY)
    if (raw) return { ...defaultConfig, ...JSON.parse(raw) }
  } catch { /* ignore */ }
  return { ...defaultConfig }
}

const config = reactive<TripPlannerConfig>(loadConfig())

// Listen for config changes from config panel
function onConfigChanged() {
  Object.assign(config, loadConfig())
  loadHomeZone()
}
onMounted(() => {
  window.addEventListener('tripPlannerConfigChanged', onConfigChanged)
  loadBindsFromDb()
  loadCasinoPortal()
  loadHomeZone()
})

// ── Load binds from database (auto-parsed from game) ────────────────────────

async function loadBindsFromDb() {
  const character = settingsStore.settings.activeCharacterName
  const server = settingsStore.settings.activeServerName
  if (!character || !server) return

  try {
    interface TeleportBindsResponse {
      primary_bind: string | null
      secondary_bind: string | null
      mushroom_circle_1: string | null
      mushroom_circle_2: string | null
      last_updated: string | null
    }
    const binds = await invoke<TeleportBindsResponse>(
      'get_teleportation_binds',
      { character, server }
    )
    // Only overwrite if we got data from the DB and local config is empty
    let changed = false
    if (binds.primary_bind && !config.primaryBind) {
      config.primaryBind = binds.primary_bind
      changed = true
    }
    if (binds.secondary_bind && !config.secondaryBind) {
      config.secondaryBind = binds.secondary_bind
      changed = true
    }
    if (binds.mushroom_circle_1 && !config.mushroomCircle) {
      config.mushroomCircle = binds.mushroom_circle_1
      changed = true
    }
    if (changed) {
      localStorage.setItem(CONFIG_KEY, JSON.stringify(config))
    }
  } catch {
    // DB not ready yet, that's fine
  }
}

// ── Actions ─────────────────────────────────────────────────────────────────

function useCurrentZone() {
  const areaName = gameState.world.area?.area_name
  if (!areaName) return
  const key = FRIENDLY_TO_KEY[areaName]
  if (key) {
    startZone.value = key
  }
}

async function planRoute() {
  if (!startZone.value || !endZone.value) return
  planning.value = true
  error.value = ''
  route.value = null

  try {
    const travelConfig = {
      primaryBind: useTeleports.value ? config.primaryBind : null,
      secondaryBind: useTeleports.value ? config.secondaryBind : null,
      mushroomCircle1: useTeleports.value ? config.mushroomCircle : null,
      mushroomCircle2: null,
      useTpMachine: useTeleports.value && useTpMachine.value,
      casinoPortal: casinoPortal.value,
    }

    route.value = await invoke<PlannedRoute>('plan_trip', {
      startZone: startZone.value,
      stops: [{ zone: endZone.value, purpose: 'pickup', details: `Arrive at ${zoneName(endZone.value)}` }],
      travelConfig,
    })
  } catch (e) {
    error.value = String(e)
  } finally {
    planning.value = false
  }
}

function zoneName(key: string): string {
  return ZONES.find(z => z.key === key)?.name ?? key
}

function stepClass(step: { action: string }): string {
  if (step.action === 'travel') return 'bg-surface-elevated/50'
  return 'bg-accent-gold/5'
}

// Clear route when zones change
watch([startZone, endZone], () => {
  route.value = null
})
</script>
