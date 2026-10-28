<template>
  <PaneLayout
    screen-key="survey-tracker"
    :left-pane="{ title: 'Sessions', defaultWidth: 300, minWidth: 240, maxWidth: 440 }"
    :right-pane="{
      title: 'Economics',
      defaultWidth: 380,
      minWidth: 280,
      maxWidth: 600,
      defaultCollapsed: false,
    }"
  >
    <!-- ═══ LEFT: controls + unified session list ═══ -->
    <template #left>
      <div class="flex flex-col h-full overflow-hidden">
        <!-- Sticky controls at top -->
        <div class="shrink-0 p-2 border-b border-border-default flex flex-col gap-2">
          <button
            class="w-full text-xs px-3 py-1.5 rounded border transition-colors"
            :class="
              store.hasActiveSession
                ? 'border-border-default bg-surface-elevated text-text-dim cursor-not-allowed'
                : 'border-accent-gold/60 text-accent-gold hover:bg-accent-gold/10'
            "
            :disabled="store.hasActiveSession || store.isBusy"
            @click="onStart"
          >
            Start New Session
          </button>

          <label class="flex items-center gap-2 text-xs text-text-secondary cursor-pointer px-1"
            title="Automatic detection needs Player.log survey events, which the current game client no longer writes. Manual start always works.">
            <input
              type="checkbox"
              class="accent-accent-gold"
              :checked="settingsStore.settings.autoStartSurveySessions"
              @change="toggleAutoStart"
            />
            Auto-start sessions
          </label>

          <div class="border-t border-border-default pt-2 flex flex-col gap-1.5">
            <div
              class="text-[10px] uppercase tracking-wider text-text-secondary font-semibold px-1"
              title="Automatic detection needs Player.log survey events, which the current game client no longer writes. Record each map you use by hand."
            >
              Record survey use
            </div>
            <SearchableSelect
              :model-value="recordMapLabel"
              :options="surveyTypeNames"
              all-label="Select a survey map…"
              :full-width="true"
              @update:model-value="recordMapLabel = $event"
            />
            <div class="flex items-center gap-1.5">
              <select
                v-model="recordKindOverride"
                class="flex-1 min-w-0 bg-surface-elevated border border-border-default rounded px-1.5 py-1 text-xs text-text-primary"
                title="Auto derives the kind from CDN data; override when the map is unknown to the CDN."
              >
                <option value="auto">Kind: Auto</option>
                <option value="basic">Kind: Basic</option>
                <option value="motherlode">Kind: Motherlode</option>
                <option value="multihit">Kind: Multihit</option>
              </select>
              <button
                class="flex-1 text-xs px-2 py-1 rounded border border-accent-gold/60 text-accent-gold hover:bg-accent-gold/10 transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
                :disabled="store.isBusy || recordMapInternalName === null"
                @click="onRecordUse"
              >
                Record use
              </button>
            </div>
          </div>

          <div class="border-t border-border-default pt-2">
            <GstLauncher />
          </div>
        </div>

        <!-- Active session card (pinned) -->
        <div v-if="activeRow" class="shrink-0 p-2 pb-0">
          <SessionCard
            :row="activeRow"
            :is-active="true"
            :is-selected="selectedSessionId === activeRow.session.id"
            @select="selectSession(activeRow!.session.id)"
          />
        </div>

        <!-- Filter/sort controls -->
        <div v-if="store.historical.length > 0" class="shrink-0 px-2 pt-2 flex flex-col gap-1.5">
          <div class="flex items-center gap-1.5">
            <SearchableSelect
              :model-value="prefs.filterZone"
              :options="availableZones"
              all-label="All zones"
              :full-width="true"
              @update:model-value="update({ filterZone: $event })"
            />
            <select
              class="bg-surface-elevated border border-border-default rounded px-1.5 py-1 text-xs text-text-primary min-w-0"
              :value="prefs.sortBy"
              @change="update({ sortBy: ($event.target as HTMLSelectElement).value as any })"
            >
              <option value="date">Date</option>
              <option value="profit">Profit</option>
              <option value="duration">Duration</option>
              <option value="surveys">Surveys</option>
            </select>
            <button
              class="text-xs text-text-secondary hover:text-text-primary px-1"
              @click="update({ sortDir: prefs.sortDir === 'desc' ? 'asc' : 'desc' })"
              :title="`Sort ${prefs.sortDir === 'desc' ? 'ascending' : 'descending'}`"
            >
              {{ prefs.sortDir === 'desc' ? '▼' : '▲' }}
            </button>
          </div>
        </div>

        <!-- Historical sessions list -->
        <div class="flex-1 min-h-0 overflow-y-auto p-2 flex flex-col gap-1">
          <div v-if="filteredSessions.length === 0" class="text-text-dim text-xs italic px-1 py-4 text-center">
            {{ store.historical.length === 0 ? 'No sessions yet.' : 'No sessions match filters.' }}
          </div>
          <SessionCard
            v-for="row in filteredSessions"
            :key="row.session.id"
            :row="row"
            :is-active="false"
            :is-selected="selectedSessionId === row.session.id"
            @select="selectSession(row.session.id)"
          />
        </div>
      </div>
    </template>

    <!-- ═══ CENTER: selected session detail ═══ -->
    <div class="h-full overflow-y-auto p-3 flex flex-col gap-3">
      <!-- Nothing selected -->
      <div
        v-if="!selectedDetail"
        class="h-full flex flex-col items-center justify-center gap-3 text-text-dim text-xs"
      >
        <template v-if="store.historical.length === 0 && !store.hasActiveSession">
          <p class="italic">No sessions yet.</p>
          <p>Start a session manually to begin tracking — the current game client
            writes no survey events to Player.log, so automatic use detection is
            unavailable. Chat-loot attribution needs a survey use recorded with
            the "Record use" control above, so no loot attributes until you
            record one.</p>
        </template>
        <template v-else>
          <p class="italic">Select a session from the list to view its detail.</p>
        </template>
      </div>

      <!-- Selected session detail -->
      <template v-else>
        <!-- Header with end-session button for active -->
        <div class="flex items-center justify-between shrink-0">
          <div class="flex items-center gap-2">
            <h2 class="text-sm text-text-primary font-semibold">
              {{ selectedDetail.session.name ?? `Session #${selectedDetail.session.id}` }}
            </h2>
            <span
              v-if="isSelectedActive"
              class="text-[10px] px-1.5 py-0.5 rounded bg-accent-green/20 text-accent-green uppercase tracking-wider font-semibold"
            >
              Active
            </span>
            <span class="text-[10px] text-text-dim">
              {{ selectedDetail.session.start_trigger }}
            </span>
          </div>
          <button
            v-if="isSelectedActive"
            class="text-xs px-2.5 py-1 rounded border border-accent-red/50 text-accent-red hover:bg-accent-red/10 transition-colors"
            :disabled="store.isBusy"
            @click="onEnd"
          >
            End Session
          </button>
        </div>

        <!-- Top row: Loot table (2/5) | Donut chart (2/5) | Time breakdown (1/5) -->
        <div class="grid grid-cols-5 gap-3 shrink-0" style="height: 600px">
          <div class="col-span-2 min-h-0 overflow-hidden">
            <LootOverviewPanel :rows="liveRows" />
          </div>
          <div class="col-span-2 min-h-0 overflow-hidden">
            <LootDonutChart :rows="liveRows" />
          </div>
          <div class="col-span-1 min-h-0 overflow-hidden">
            <TimeBreakdownPanel
              :session="selectedDetail.session"
              :is-active="isSelectedActive"
            />
          </div>
        </div>

        <!-- Second row: breakdown by survey type -->
        <LootByTypePanel :detail="selectedDetail" />

      </template>
    </div>

    <!-- ═══ RIGHT: notes + economics ═══ -->
    <template #right>
      <div class="h-full overflow-y-auto p-3 flex flex-col gap-3">
        <div v-if="!selectedDetail" class="text-text-dim text-xs italic">
          Select a session to view economics and notes.
        </div>

        <template v-else>
          <!-- Session name (editable) -->
          <div>
            <div class="text-[10px] uppercase tracking-wider text-text-secondary font-semibold mb-1">
              Session Name
            </div>
            <input
              :value="selectedDetail.session.name ?? `Session #${selectedDetail.session.id}`"
              class="w-full text-xs px-2 py-1.5 rounded border border-border-default bg-surface-elevated text-text-primary placeholder:text-text-dim focus:outline-none focus:border-accent-gold/50"
              @blur="onNameBlur($event, selectedDetail!.session.id)"
              @keydown.enter="($event.target as HTMLInputElement).blur()"
            />
          </div>

          <!-- Notes (editable) -->
          <div>
            <div class="text-[10px] uppercase tracking-wider text-text-secondary font-semibold mb-1">
              Notes
            </div>
            <textarea
              :value="selectedDetail.session.notes ?? ''"
              class="w-full text-xs px-2 py-1.5 rounded border border-border-default bg-surface-elevated text-text-primary placeholder:text-text-dim focus:outline-none focus:border-accent-gold/50"
              rows="3"
              placeholder="Optional notes about this session…"
              @blur="onNotesBlur($event, selectedDetail!.session.id)"
            />
          </div>

          <!-- Crafting cost breakdown -->
          <div v-if="selectedDetail.craft_materials.length > 0" class="flex flex-col gap-2">
            <div class="text-[10px] uppercase tracking-wider text-text-secondary font-semibold">
              Crafting Cost
            </div>
            <div class="flex flex-col gap-1">
              <div
                v-for="mat in selectedDetail.craft_materials"
                :key="mat.item_name"
                class="flex items-center justify-between text-xs"
              >
                <div class="flex items-center gap-1.5 min-w-0">
                  <ItemInline :reference="mat.item_name" :show-icon="true" />
                  <span class="text-text-dim">&times;{{ mat.total_quantity }}</span>
                </div>
                <span class="text-text-secondary tabular-nums shrink-0">
                  {{ mat.total_cost !== null ? formatGold(mat.total_cost) : '—' }}
                </span>
              </div>
              <div class="flex justify-between text-xs border-t border-border-default pt-1 mt-1">
                <span class="text-text-secondary font-semibold">Total cost</span>
                <span class="text-text-primary font-semibold tabular-nums">
                  {{ formatGold(selectedDetail.economics.cost_total) }}
                </span>
              </div>
            </div>
          </div>

          <!-- Revenue + Profit -->
          <div class="flex flex-col gap-2">
            <div class="text-[10px] uppercase tracking-wider text-text-secondary font-semibold">
              Revenue &amp; Profit
            </div>
            <div class="flex flex-col gap-1.5 text-xs tabular-nums">
              <div class="flex justify-between">
                <span class="text-text-secondary">Revenue</span>
                <span class="text-accent-gold font-semibold">{{ formatGold(liveRev) }}</span>
              </div>
              <div v-if="revenuePerHour !== null" class="flex justify-between">
                <span class="text-text-dim">Revenue / hr</span>
                <span class="text-text-muted">{{ formatGold(revenuePerHour) }}/hr</span>
              </div>
              <div class="flex justify-between border-t border-border-default pt-1.5">
                <span class="text-text-secondary">Profit</span>
                <span
                  class="font-semibold"
                  :class="liveProf >= 0 ? 'text-accent-green' : 'text-accent-red'"
                >
                  {{ liveProf >= 0 ? '+' : '' }}{{ formatGold(liveProf) }}
                </span>
              </div>
              <div v-if="profitPerHour !== null" class="flex justify-between">
                <span class="text-text-dim">Profit / hr (total)</span>
                <span
                  class="font-semibold"
                  :class="profitPerHour >= 0 ? 'text-accent-green' : 'text-accent-red'"
                >
                  {{ profitPerHour >= 0 ? '+' : '' }}{{ formatGold(profitPerHour) }}/hr
                </span>
              </div>
              <div v-if="profitPerHourSurveyOnly !== null" class="flex justify-between">
                <span class="text-text-dim">Profit / hr (survey only)</span>
                <span
                  class="font-semibold"
                  :class="profitPerHourSurveyOnly >= 0 ? 'text-accent-green' : 'text-accent-red'"
                >
                  {{ profitPerHourSurveyOnly >= 0 ? '+' : '' }}{{ formatGold(profitPerHourSurveyOnly) }}/hr
                </span>
              </div>
            </div>
          </div>

          <!-- Delete -->
          <div class="flex justify-end mt-4">
            <button
              class="text-xs px-2.5 py-1 rounded border border-accent-red/50 text-accent-red hover:bg-accent-red/10 transition-colors"
              @click="pendingDeleteId = selectedDetail!.session.id; showDeleteConfirm = true"
            >
              Delete session
            </button>
          </div>
        </template>
      </div>
    </template>
  </PaneLayout>

  <ModalDialog
    :show="showDeleteConfirm"
    title="Delete Session"
    type="confirm"
    :message="`Delete session #${pendingDeleteId}? This will remove the session and its survey uses. The underlying item transactions are preserved.\n\nThis cannot be undone.`"
    confirm-label="Delete"
    :danger="true"
    @update:show="showDeleteConfirm = $event"
    @confirm="handleDeleteConfirmed"
  />
</template>

<script setup lang="ts">
// Unified survey session view — merges the previous separate Session and
// History tabs into one PaneLayout:
//   Left:   controls (start, auto-start) + unified session list with
//           active pinned at top, filter/sort for historical.
//   Center: detail for the selected session (SessionSummary today,
//           richer panels in Phase C).
//   Right:  editable name + notes + economics for the selected session.
//
// State lives in surveyTrackerStore. Historical rows carry loot_summary
// with item_type_id, enabling reactive revenue/profit via useLiveValuation.

import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import PaneLayout from '../Shared/PaneLayout.vue'
import ModalDialog from '../Shared/ModalDialog.vue'
import SearchableSelect from '../Shared/SearchableSelect.vue'
import SessionCard from './SessionCard.vue'
import GstLauncher from './GstLauncher.vue'
import LootOverviewPanel from './LootOverviewPanel.vue'
import LootDonutChart from './LootDonutChart.vue'
import TimeBreakdownPanel from './TimeBreakdownPanel.vue'
import LootByTypePanel from './LootByTypePanel.vue'
import ItemInline from '../Shared/Item/ItemInline.vue'
import {
  useSurveyTrackerStore,
  type HistoricalSessionRow,
  type SurveySessionDetail,
} from '../../stores/surveyTrackerStore'
import { useSettingsStore } from '../../stores/settingsStore'
import { useViewPrefs } from '../../composables/useViewPrefs'
import { formatGold } from '../../composables/useRecipeCost'
import { liveEnrichedRows, liveRevenue, liveProfit as computeLiveProfit } from '../../composables/useLiveValuation'

const store = useSurveyTrackerStore()
const settingsStore = useSettingsStore()

interface SurveyTypeEntry {
  internal_name: string
  name: string
  zone: string | null
}

// ── Manual "Record use" control ──────────────────────────────────────
// Map picker options come from the get_all_survey_types catalog query
// (value = internal_name for the invoke, label = name + zone for display).
const surveyTypes = ref<SurveyTypeEntry[]>([])
const recordMapLabel = ref<string | null>(null)
const recordKindOverride = ref<'auto' | 'basic' | 'motherlode' | 'multihit'>('auto')
const recordMapInternalName = computed<string | null>(() => {
  if (recordMapLabel.value === null) return null
  const hit = surveyTypes.value.find(
    (t) => `${t.name}${t.zone ? ` — ${t.zone}` : ''}` === recordMapLabel.value,
  )
  return hit ? hit.internal_name : null
})
const surveyTypeNames = computed(() =>
  surveyTypes.value.map((t) => `${t.name}${t.zone ? ` — ${t.zone}` : ''}`).sort(),
)

async function loadSurveyTypes() {
  try {
    const rows = await invoke<SurveyTypeEntry[]>('get_all_survey_types')
    surveyTypes.value = rows
  } catch (err) {
    console.error('[SurveyTrackerView] get_all_survey_types failed:', err)
    surveyTypes.value = []
  }
}

async function onRecordUse() {
  const internalName = recordMapInternalName.value
  if (internalName === null) return
  const kind = recordKindOverride.value === 'auto' ? null : recordKindOverride.value
  const res = await store.recordUse(internalName, kind)
  if (res !== null && res.session_id !== null) {
    selectedSessionId.value = res.session_id
    await selectSession(res.session_id)
  }
}

// ── View prefs (filter/sort, persisted) ─────────────────────────────
const { prefs, update } = useViewPrefs('survey-session-list', {
  filterZone: null as string | null,
  sortBy: 'date' as 'date' | 'profit' | 'duration' | 'surveys',
  sortDir: 'desc' as 'asc' | 'desc',
})

// ── Session selection ───────────────────────────────────────────────
const selectedSessionId = ref<number | null>(null)
const selectedDetail = ref<SurveySessionDetail | null>(null)

const isSelectedActive = computed(() =>
  store.activeSession !== null &&
  selectedSessionId.value === store.activeSession.id,
)

async function selectSession(id: number) {
  selectedSessionId.value = id
  try {
    selectedDetail.value = await invoke<SurveySessionDetail | null>(
      'survey_tracker_session_detail',
      { sessionId: id },
    )
  } catch (err) {
    console.error('[SurveyTrackerView] selectSession failed:', err)
    selectedDetail.value = null
  }
}

// Auto-refresh selected detail when it's the active session and events arrive.
watch(
  () => store.status,
  () => {
    if (isSelectedActive.value && selectedSessionId.value !== null) {
      void selectSession(selectedSessionId.value)
    }
  },
  { deep: true },
)

// Safety-net poll for active session detail.
let pollId: number | null = null
onMounted(() => {
  pollId = window.setInterval(() => {
    if (isSelectedActive.value && selectedSessionId.value !== null) {
      void selectSession(selectedSessionId.value)
    }
  }, 5000)
})
onBeforeUnmount(() => {
  if (pollId !== null) window.clearInterval(pollId)
})

// Auto-select the active session on load if there is one.
watch(
  () => store.activeSession?.id,
  (id) => {
    if (id !== undefined && id !== null && selectedSessionId.value === null) {
      void selectSession(id)
    }
  },
  { immediate: true },
)

// ── Reactive economics for the right panel ──────────────────────────
const liveRows = computed(() =>
  selectedDetail.value ? liveEnrichedRows(selectedDetail.value.loot_summary) : [],
)
const liveRev = computed(() => liveRevenue(liveRows.value))
const liveProf = computed(() =>
  selectedDetail.value
    ? computeLiveProfit(liveRows.value, selectedDetail.value.economics.cost_total)
    : 0,
)
function totalDurationSecs(): number | null {
  if (!selectedDetail.value) return null
  const s = selectedDetail.value.session
  const startTs = s.user_started_at ?? s.started_at
  const endTs = s.user_ended_at ?? s.ended_at
  if (!endTs) return null
  const secs = (Date.parse(endTs.replace(' ', 'T') + 'Z') - Date.parse(startTs.replace(' ', 'T') + 'Z')) / 1000
  return secs >= 60 ? secs : null
}

function surveyOnlySecs(): number | null {
  if (!selectedDetail.value) return null
  const s = selectedDetail.value.session
  if (!s.first_loot_at || !s.last_loot_at) return null
  const secs = (Date.parse(s.last_loot_at.replace(' ', 'T') + 'Z') - Date.parse(s.first_loot_at.replace(' ', 'T') + 'Z')) / 1000
  return secs >= 60 ? secs : null
}

const revenuePerHour = computed<number | null>(() => {
  const secs = totalDurationSecs()
  if (secs === null) return null
  return (liveRev.value / secs) * 3600
})
const profitPerHour = computed<number | null>(() => {
  const secs = totalDurationSecs()
  if (secs === null) return null
  return (liveProf.value / secs) * 3600
})
const profitPerHourSurveyOnly = computed<number | null>(() => {
  const secs = surveyOnlySecs()
  if (secs === null) return null
  return (liveProf.value / secs) * 3600
})

// ── Filtered + sorted historical sessions ───────────────────────────
// Active session is shown separately (pinned at top), so filter it out
// of the historical list.
const activeRow = computed<HistoricalSessionRow | null>(() => {
  if (!store.activeSession) return null
  return store.historical.find(r => r.session.id === store.activeSession!.id) ?? null
})

const availableZones = computed(() => {
  const set = new Set<string>()
  for (const row of store.historical) {
    for (const z of row.zones) set.add(z)
  }
  return [...set].sort()
})

const filteredSessions = computed(() => {
  let list = store.historical.filter(r => {
    // Exclude the active session (shown pinned separately).
    if (store.activeSession && r.session.id === store.activeSession.id) return false
    // Zone filter.
    if (prefs.value.filterZone && !r.zones.includes(prefs.value.filterZone)) return false
    return true
  })

  // Sort.
  const dir = prefs.value.sortDir === 'asc' ? 1 : -1
  list = [...list].sort((a, b) => {
    switch (prefs.value.sortBy) {
      case 'profit': {
        const ap = liveRowProfit(a)
        const bp = liveRowProfit(b)
        return (ap - bp) * dir
      }
      case 'duration':
        return ((a.duration_seconds ?? 0) - (b.duration_seconds ?? 0)) * dir
      case 'surveys':
        return (a.total_uses - b.total_uses) * dir
      case 'date':
      default:
        return (a.session.id - b.session.id) * dir
    }
  })

  return list
})

function liveRowProfit(row: HistoricalSessionRow): number {
  const enriched = liveEnrichedRows(row.loot_summary)
  return computeLiveProfit(enriched, row.economics.cost_total)
}

// ── Actions ─────────────────────────────────────────────────────────
onMounted(async () => {
  await Promise.all([store.init(), loadSurveyTypes()])
})

async function onStart() {
  const id = await store.startSession()
  if (id !== null) void selectSession(id)
}

async function onEnd() {
  await store.endSession()
  // Refresh the detail if still selected.
  if (selectedSessionId.value !== null) {
    void selectSession(selectedSessionId.value)
  }
}

function toggleAutoStart(e: Event) {
  const checked = (e.target as HTMLInputElement).checked
  settingsStore.updateSettings({ autoStartSurveySessions: checked })
}

function onNameBlur(e: FocusEvent, sessionId: number) {
  const value = (e.target as HTMLInputElement).value.trim()
  const current = selectedDetail.value?.session.name
  // Don't save if the user typed the default placeholder text.
  const defaultName = `Session #${sessionId}`
  const effectiveValue = value === defaultName || value === '' ? null : value
  if (effectiveValue !== current) {
    store.updateSessionName(sessionId, effectiveValue ?? '')
  }
}

function onNotesBlur(e: FocusEvent, sessionId: number) {
  const value = (e.target as HTMLTextAreaElement).value
  const current = selectedDetail.value?.session.notes ?? ''
  if (value !== current) {
    store.updateSessionNotes(sessionId, value)
  }
}

const showDeleteConfirm = ref(false)
const pendingDeleteId = ref<number | null>(null)

async function handleDeleteConfirmed() {
  const id = pendingDeleteId.value
  if (id === null) return
  pendingDeleteId.value = null
  const ok = await store.deleteSession(id)
  if (ok && selectedSessionId.value === id) {
    selectedSessionId.value = null
    selectedDetail.value = null
  }
}
</script>
