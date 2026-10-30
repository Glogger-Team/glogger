<template>
  <div class="flex flex-col gap-2 text-sm">
    <!-- Identity section: Character / Server / Zone -->
    <div v-if="showIdentityRow" class="flex flex-col gap-0.5">
      <div v-if="(prefs.showCharacter && characterName) || (prefs.showServer && serverName)" class="flex items-center gap-2">
        <span v-if="prefs.showCharacter && characterName" class="text-text-primary text-xs font-medium">{{ characterName }}</span>
        <span v-if="(prefs.showCharacter && characterName) && (prefs.showServer && serverName)" class="text-text-muted text-xs uppercase ">on</span>
        <span v-if="prefs.showServer && serverName" class="text-text-primary text-xs font-medium">{{ serverName }}</span>
      </div>
      <div v-if="prefs.showZone" class="flex items-center gap-2">
        <span class="text-text-muted text-xs uppercase tracking-wide ">In</span>
        <span v-if="zoneName" class="text-text-primary text-xs font-medium">{{ zoneName }}</span>
        <span v-else class="text-text-dim italic text-xs">Unknown</span>
      </div>
    </div>

    <!-- Divider -->
    <div
      v-if="showIdentityRow && (showAnyTime || prefs.showMoon)"
      class="border-t border-border-default" />

    <!-- Times + Moon side by side -->
    <div v-if="showAnyTime || (prefs.showMoon && moonReady)" class="flex gap-4">
      <!-- Left: Times stacked -->
      <div v-if="showAnyTime" class="flex flex-col gap-0.5">
        <div v-if="prefs.showGameTime" class="flex items-center gap-2">
          <span class="text-text-muted text-xs uppercase tracking-wide w-14">Game</span>
          <span class="text-text-primary text-xs">{{ formattedGameTime }}</span>
        </div>
        <div v-if="prefs.showServerTime" class="flex items-center gap-2">
          <span class="text-text-muted text-xs uppercase tracking-wide w-14">Server</span>
          <span class="text-text-primary text-xs">{{ formattedServerTime }}</span>
        </div>
        <div v-if="prefs.showLocalTime" class="flex items-center gap-2">
          <span class="text-text-muted text-xs uppercase tracking-wide w-14">Local</span>
          <span class="text-text-primary text-xs">{{ formattedLocalTime }}</span>
        </div>
      </div>

      <!-- Right: Moon phase -->
      <template v-if="prefs.showMoon && moonReady && phase">
        <div v-if="showAnyTime" class="w-px bg-border-default" />
        <EntityTooltipWrapper :delay="300" :interactive="true" border-class="border-border-default">
          <div class="flex flex-col items-center gap-0.5 min-w-20 cursor-help">
            <span class="text-3xl leading-none">{{ phase.emoji }}</span>
            <span class="text-text-primary text-xs font-medium">{{ phase.label }}</span>
            <span v-if="nextPhaseText" class="text-text-dim text-[10px]">{{ nextPhaseText }}</span>
            <span v-if="fullMoonText" class="text-text-dim text-[10px]">{{ fullMoonText }}</span>
          </div>
          <template #tooltip>
            <div class="flex flex-col gap-2 text-xs">
              <!-- Casino Portal -->
              <div>
                <div class="text-text-primary font-medium mb-1">Casino Portal</div>
                <div class="text-text-secondary">{{ casinoPortalLocation }}</div>
              </div>
              <!-- Dark Chapel Statue -->
              <div class="border-t border-border-default pt-2">
                <div class="text-text-primary font-medium mb-1">Dark Chapel Statue</div>
                <div class="text-text-secondary">{{ darkChapelDirection }}</div>
              </div>
              <!-- Upcoming phases -->
              <div v-if="daysUntil.length > 0" class="border-t border-border-default pt-2">
                <div class="text-text-primary font-medium mb-1">Upcoming</div>
                <div v-for="d in daysUntil" :key="d.game_phase" class="flex justify-between gap-4">
                  <span class="text-text-secondary">{{ d.label }}</span>
                  <span class="text-text-muted">{{ d.days }}d</span>
                </div>
              </div>
            </div>
          </template>
        </EntityTooltipWrapper>
      </template>
      <div v-else-if="prefs.showMoon && !moonReady" class="flex flex-col items-center gap-0.5 min-w-20">
        <span class="text-3xl leading-none">🌙</span>
        <span class="text-text-dim text-xs italic">Loading…</span>
      </div>
    </div>

    <!-- Divider -->
    <div
      v-if="(showAnyTime || prefs.showMoon) && prefs.showWeather"
      class="border-t border-border-default" />

    <!-- Weather row -->
    <div v-if="prefs.showWeather" class="flex items-center gap-4">
      <div class="flex items-center gap-1.5">
        <span v-if="weather" class="text-text-primary text-xs">{{ weather }}</span>
        <span v-else class="text-text-dim italic text-xs">Unknown</span>
      </div>
    </div>

    <!-- Divider -->
    <div
      v-if="prefs.showWeather && displayedCurrencies.length > 0"
      class="border-t border-border-default" />

    <!-- Currencies -->
    <div
      v-if="displayedCurrencies.length > 0"
      class="flex flex-col gap-0.5">
      <div
        v-for="c in displayedCurrencies"
        :key="c.currency_name"
        class="flex items-center justify-between gap-2 text-xs">
        <span class="text-text-muted truncate">{{ formatCurrencyName(c.currency_name) }}</span>
        <span class="text-accent-gold font-bold">{{ Math.floor(c.amount).toLocaleString() }}</span>
      </div>
    </div>

    <!-- Empty state -->
    <div
      v-if="!showIdentityRow && !showAnyTime && !prefs.showMoon && !prefs.showWeather && displayedCurrencies.length === 0"
      class="text-text-dim text-xs italic">
      All sections hidden — use the gear icon to configure.
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed, ref, watch, onMounted, onUnmounted } from 'vue'
import { useGameStateStore } from '../../stores/gameStateStore'
import { useGameDataStore } from '../../stores/gameDataStore'
import { useSettingsStore } from '../../stores/settingsStore'
import { useMoonPhase, CASINO_PORTAL_BY_PHASE, DARK_CHAPEL_DIRECTION_BY_PHASE } from '../../composables/useMoonPhase'
import EntityTooltipWrapper from '../Shared/EntityTooltipWrapper.vue'
import { useViewPrefs } from '../../composables/useViewPrefs'
import { CURRENCY_DISPLAY_ORDER, CONTEXT_BAR_DEFAULTS, formatCurrencyDisplayName, type ContextBarPrefs } from './contextBarPrefs'

const store = useGameStateStore()
const gameData = useGameDataStore()
const settings = useSettingsStore()
const { phase, daysUntil, loading: moonLoading } = useMoonPhase()
const moonReady = computed(() => !moonLoading.value)
const { prefs } = useViewPrefs<ContextBarPrefs>('widget.context-bar', CONTEXT_BAR_DEFAULTS)

// --- Identity helpers ---

const characterName = computed(() => settings.settings.activeCharacterName)
const serverName = computed(() => settings.settings.activeServerName)

const areaKey = computed(() => store.world.area?.area_name ?? null)
const zoneName = ref<string | null>(null)

watch(areaKey, async (key) => {
  if (!key) { zoneName.value = null; return }
  const area = await gameData.resolveArea(key)
  zoneName.value = area?.short_friendly_name ?? area?.friendly_name ?? key
}, { immediate: true })

/** The identity block renders a character/server row only when a name is present. */
const showIdentityRow = computed(() =>
  ((prefs.value.showCharacter && !!characterName.value) || (prefs.value.showServer && !!serverName.value))
  || prefs.value.showZone
)

// --- Time helpers ---

const showAnyTime = computed(() =>
  prefs.value.showGameTime || prefs.value.showServerTime || prefs.value.showLocalTime
)

function formatTo12h(time24: string): string {
  if (time24 === '--:--') return time24
  const [hStr, mStr] = time24.split(':')
  const h = parseInt(hStr, 10)
  const period = h >= 12 ? 'PM' : 'AM'
  const h12 = h === 0 ? 12 : h > 12 ? h - 12 : h
  return `${h12}:${mStr} ${period}`
}

const formattedServerTime = computed(() =>
  prefs.value.use24h ? store.serverTime : formatTo12h(store.serverTime)
)

const formattedGameTime = computed(() =>
  prefs.value.use24h ? store.gameTime : formatTo12h(store.gameTime)
)

// --- Local time: minute-aligned ticking ---
// Display is minute-granular, so schedule the first tick on the next minute
// boundary (not "1s after mount") and re-arm each minute — no per-second wakeups.
const localTime24 = ref(formatLocalTime24())
let clockTimeout: ReturnType<typeof setTimeout> | null = null

function formatLocalTime24(): string {
  return new Date().toLocaleTimeString([], { hour: '2-digit', minute: '2-digit', hour12: false })
}

function scheduleLocalTick() {
  const msToNextMinute = 60_000 - (Date.now() % 60_000)
  clockTimeout = setTimeout(() => {
    localTime24.value = formatLocalTime24()
    scheduleLocalTick()
  }, msToNextMinute)
}
onMounted(() => {
  scheduleLocalTick()
})

const formattedLocalTime = computed(() => {
  if (prefs.value.use24h) return localTime24.value
  return formatTo12h(localTime24.value)
})

onUnmounted(() => {
  if (clockTimeout) clearTimeout(clockTimeout)
})
// --- Moon: shared lookup tables live in useMoonPhase.ts ---
const nextPhaseText = computed(() => {
  if (daysUntil.value.length === 0) return null
  const next = daysUntil.value[0]
  return `${next.label} in ${next.days}d`
})

const casinoPortalLocation = computed(() => {
  if (!phase.value) return ''
  return CASINO_PORTAL_BY_PHASE[phase.value.name]
})

const darkChapelDirection = computed(() => {
  if (!phase.value) return ''
  return DARK_CHAPEL_DIRECTION_BY_PHASE[phase.value.name]
})

const fullMoonText = computed(() => {
  if (!phase.value) return null
  if (phase.value.name === 'FullMoon') return null
  const fullMoonEntry = daysUntil.value.find(d => d.game_phase === 'FullMoon')
  if (!fullMoonEntry) return null
  return `Full Moon in ${fullMoonEntry.days}d`
})

// --- Weather (live via AreaConfig lines; is_active always true) ---
const weather = computed(() => {
  const w = store.world.weather
  if (!w || !w.is_active) return null
  return w.weather_name
})

// --- Currencies ---

/** Currencies filtered by per-currency toggle, sorted in display order */
const displayedCurrencies = computed(() => {
  const visible = store.currencies.filter(c => {
    if (c.amount <= 0) return false
    const prefKey = currencyPrefKey(c.currency_name)
    // Default to true for unknown currencies (unless explicitly hidden)
    return prefs.value[prefKey] !== false
  })

  // Sort by CURRENCY_DISPLAY_ORDER, unknowns go to end
  return visible.sort((a, b) => {
    const aIdx = CURRENCY_DISPLAY_ORDER.indexOf(a.currency_name.toLowerCase())
    const bIdx = CURRENCY_DISPLAY_ORDER.indexOf(b.currency_name.toLowerCase())
    const aOrder = aIdx === -1 ? 999 : aIdx
    const bOrder = bIdx === -1 ? 999 : bIdx
    return aOrder - bOrder
  })
})

function currencyPrefKey(name: string): string {
  return `currency_${name.toLowerCase()}`
}

function formatCurrencyName(name: string): string {
  return formatCurrencyDisplayName(name)
}
</script>
