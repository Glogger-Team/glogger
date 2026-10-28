// New survey tracker store (Phase 5 of item-provenance overhaul).
//
// Thin Pinia store over the backend SurveySessionAggregator. The backend
// owns all tracker state (per the backend-owned-state rule in CLAUDE.md);
// this store is a read-through cache plus a thin command dispatcher.
//
// Replaces the legacy `surveyStore.ts` once the new survey screen ships.
//
// Backend surface this store consumes:
//   Commands:
//     survey_tracker_status              -> { active_session, open_multihit_nodes }
//     survey_tracker_start_session       -> session_id (errors if active)
//     survey_tracker_end_session         -> Option<session_id>
//     survey_tracker_recent_sessions     -> Vec<SurveySession>
//     survey_tracker_session_detail(id)  -> { session, uses, loot_summary }
//   Events (Tauri emit):
//     survey-tracker-session-started   { session_id, trigger }
//     survey-tracker-session-ended     { session_id, reason }
//     survey-tracker-use-recorded      { use_id, session_id, map_internal_name, kind }
//     survey-tracker-use-completed     use_id (number)
//     survey-tracker-multihit-opened   { use_id, node_entity_id }
//     survey-tracker-multihit-closed   { use_id, node_entity_id }

import { defineStore } from "pinia";
import { computed, ref } from "vue";
import { invoke } from "@tauri-apps/api/core";

// ── Types matching the Rust shapes ──────────────────────────────────────

export type SessionStartTrigger = "manual" | "crafting" | "first_use";
export type SurveyUseKind = "basic" | "motherlode" | "multihit";
export type SurveyUseStatus =
  | "pending_loot"
  | "completed"
  | "aborted"
  | "unknown";

export interface SurveySession {
  id: number;
  character_name: string;
  server_name: string;
  started_at: string;
  ended_at: string | null;
  start_trigger: SessionStartTrigger;
  crafted_count: number | null;
  consumed_count: number;
  notes: string | null;
  name: string | null;
  user_started_at: string | null;
  user_ended_at: string | null;
  first_craft_at: string | null;
  last_craft_at: string | null;
  first_loot_at: string | null;
  last_loot_at: string | null;
}

export interface SurveyUse {
  id: number;
  session_id: number | null;
  character_name: string;
  server_name: string;
  used_at: string;
  map_internal_name: string;
  map_display_name: string;
  kind: SurveyUseKind;
  area: string | null;
  status: SurveyUseStatus;
  loot_qty: number;
}

export interface MultihitSummary {
  node_entity_id: number;
  survey_use_id: number;
  map_display_name: string;
  opened_at: string;
  last_hit_at: string;
  loot_qty: number;
}

export interface LootSummaryRow {
  item_name: string;
  item_type_id: number | null;
  total_qty: number;
  primary_qty: number;
  bonus_qty: number;
  /** Snapshot of unit value at fetch time (backend-computed using the
   *  user's valuation mode). The frontend may recompute this reactively
   *  from marketStore for live updates within the same view. */
  unit_value: number | null;
  total_value: number | null;
}

export interface SessionEconomics {
  cost_total: number;
  revenue_total: number;
  bonus_revenue_total: number;
  profit_total: number;
  items_priced: number;
  items_unpriced: number;
}

export interface SurveyTrackerStatus {
  active_session: SurveySession | null;
  open_multihit_nodes: MultihitSummary[];
}

export interface CraftMaterialRow {
  item_name: string;
  item_type_id: number | null;
  total_quantity: number;
  unit_cost: number | null;
  total_cost: number | null;
}

export interface SurveySessionDetail {
  session: SurveySession;
  uses: SurveyUse[];
  loot_summary: LootSummaryRow[];
  economics: SessionEconomics;
  craft_materials: CraftMaterialRow[];
}

// ── Historical / Analytics types ────────────────────────────────────────

export interface HistoricalSessionRow {
  session: SurveySession;
  total_uses: number;
  basic_uses: number;
  motherlode_uses: number;
  multihit_uses: number;
  total_loot_qty: number;
  duration_seconds: number | null;
  economics: SessionEconomics;
  /** Per-item loot with item_type_id so the frontend can reactively
   *  re-price when market values change. */
  loot_summary: LootSummaryRow[];
  /** Distinct area keys from this session's survey uses. */
  zones: string[];
}

export interface ItemSummary {
  item_name: string;
  total_qty: number;
  primary_qty: number;
  bonus_qty: number;
  times_received: number;
}

export interface ZoneSummary {
  area: string;
  total_uses: number;
  basic_uses: number;
  motherlode_uses: number;
  multihit_uses: number;
  total_loot_qty: number;
  basic_uses_with_bonus: number;
  bonus_items_total: number;
  items: ItemSummary[];
}

export interface SurveyTypeSummary {
  map_internal_name: string;
  map_display_name: string;
  area: string | null;
  kind: SurveyUseKind;
  total_uses: number;
  total_loot_qty: number;
  avg_loot_per_use: number | null;
  uses_with_bonus: number;
  bonus_items_total: number;
  items: ItemSummary[];
}

export interface SurveyAnalytics {
  zones: ZoneSummary[];
  survey_types: SurveyTypeSummary[];
  items: ItemSummary[];
  total_sessions: number;
  total_uses: number;
  total_basic_uses: number;
  basic_uses_with_bonus: number;
  bonus_items_total: number;
}

export interface ItemSourceAnalysis {
  item_name: string;
  survey_type: string;
  map_internal_name: string;
  zone: string | null;
  category: string;
  crafting_cost: number;
  total_completions: number;
  primary_total_qty: number;
  primary_times_seen: number;
  bonus_total_qty: number;
  bonus_times_seen: number;
  bonus_avg_per_proc: number;
  speed_bonus_rate: number;
  avg_seconds_per_survey: number;
}

// ── Live event payloads (the slim shapes the backend emits) ─────────────

interface SessionStartedPayload {
  session_id: number;
  trigger: SessionStartTrigger;
}

interface SessionEndedPayload {
  session_id: number;
  reason: string;
}

interface UseRecordedPayload {
  use_id: number;
  session_id: number | null;
  map_internal_name: string;
  kind: SurveyUseKind;
}

export interface RecordUseResult {
  use_id: number;
  session_id: number | null;
}

interface MultihitNodePayload {
  use_id: number;
  node_entity_id: number;
}

// ── Store ───────────────────────────────────────────────────────────────

export const useSurveyTrackerStore = defineStore("surveyTracker", () => {
  // Live status — refetched from the backend on every event we receive that
  // could change it. Cheap on the backend (one indexed query per call) and
  // keeps the frontend honest about not duplicating state derivation.
  const status = ref<SurveyTrackerStatus | null>(null);

  // Recent sessions list, refreshed when sessions start/end.
  const recentSessions = ref<SurveySession[]>([]);

  // Cached session detail for the currently-open detail panel, if any.
  // null = nothing open. Refreshed on relevant tracker events.
  const openDetailSessionId = ref<number | null>(null);
  const openDetail = ref<SurveySessionDetail | null>(null);

  // History tab cache. Refetched lazily — the History view calls
  // refreshHistorical() on mount and on relevant tracker events.
  const historical = ref<HistoricalSessionRow[]>([]);

  // Analytics tab cache. Same lazy-refetch pattern.
  const analytics = ref<SurveyAnalytics | null>(null);

  // True while a backend command is in flight — the UI uses this to disable
  // buttons. Kept simple (single boolean) since calls are cheap and we don't
  // overlap them in normal use.
  const isBusy = ref(false);

  // Revenue and profit are computed reactively on the frontend from
  // loot_summary rows + marketStore, so they update instantly when the
  // user changes a market price — no re-fetch needed. The backend's
  // economics snapshot is used only for cost_total (recipe-based, not
  // market-dependent). See SessionSummary.vue's liveRows / liveProfit.

  // ── Computed ──────────────────────────────────────────────────────────

  const activeSession = computed(() => status.value?.active_session ?? null);
  const hasActiveSession = computed(() => activeSession.value !== null);
  const openMultihitNodes = computed(
    () => status.value?.open_multihit_nodes ?? []
  );

  // ── Backend command wrappers ──────────────────────────────────────────

  async function refreshStatus(): Promise<void> {
    try {
      status.value = await invoke<SurveyTrackerStatus>(
        "survey_tracker_status"
      );
    } catch (err) {
      console.error("[surveyTracker] status refresh failed:", err);
    }
  }

  async function refreshRecentSessions(limit = 20): Promise<void> {
    try {
      recentSessions.value = await invoke<SurveySession[]>(
        "survey_tracker_recent_sessions",
        { limit }
      );
    } catch (err) {
      console.error("[surveyTracker] recent sessions refresh failed:", err);
    }
  }

  async function loadSessionDetail(
    sessionId: number
  ): Promise<SurveySessionDetail | null> {
    try {
      const detail = await invoke<SurveySessionDetail | null>(
        "survey_tracker_session_detail",
        { sessionId }
      );
      openDetailSessionId.value = sessionId;
      openDetail.value = detail;
      return detail;
    } catch (err) {
      console.error("[surveyTracker] session detail load failed:", err);
      openDetailSessionId.value = null;
      openDetail.value = null;
      return null;
    }
  }

  function clearOpenDetail() {
    openDetailSessionId.value = null;
    openDetail.value = null;
  }

  async function refreshHistorical(limit = 50): Promise<void> {
    try {
      historical.value = await invoke<HistoricalSessionRow[]>(
        "survey_tracker_historical_sessions",
        { limit }
      );
    } catch (err) {
      console.error("[surveyTracker] historical refresh failed:", err);
    }
  }

  async function refreshAnalytics(): Promise<void> {
    try {
      analytics.value = await invoke<SurveyAnalytics>("survey_tracker_analytics");
    } catch (err) {
      console.error("[surveyTracker] analytics refresh failed:", err);
    }
  }

  async function updateSessionNotes(
    sessionId: number,
    notes: string
  ): Promise<boolean> {
    try {
      await invoke("survey_tracker_update_session_notes", { sessionId, notes });
      // Refresh affected views so the user sees the update immediately.
      await refreshHistorical();
      if (openDetailSessionId.value === sessionId) {
        await loadSessionDetail(sessionId);
      }
      return true;
    } catch (err) {
      console.error("[surveyTracker] update_session_notes failed:", err);
      return false;
    }
  }

  async function updateSessionName(
    sessionId: number,
    name: string,
  ): Promise<boolean> {
    try {
      await invoke("survey_tracker_update_session_name", { sessionId, name });
      await refreshHistorical();
      if (openDetailSessionId.value === sessionId) {
        await loadSessionDetail(sessionId);
      }
      return true;
    } catch (err) {
      console.error("[surveyTracker] update_session_name failed:", err);
      return false;
    }
  }

  async function updateSessionTimes(
    sessionId: number,
    userStartedAt: string | null,
    userEndedAt: string | null,
  ): Promise<boolean> {
    try {
      await invoke("survey_tracker_update_session_times", {
        sessionId,
        userStartedAt,
        userEndedAt,
      });
      await refreshHistorical();
      if (openDetailSessionId.value === sessionId) {
        await loadSessionDetail(sessionId);
      }
      return true;
    } catch (err) {
      console.error("[surveyTracker] update_session_times failed:", err);
      return false;
    }
  }

  async function deleteSession(sessionId: number): Promise<boolean> {
    try {
      await invoke("survey_tracker_delete_session", { sessionId });
      // Clear the open detail if it was pointing at the now-gone session.
      if (openDetailSessionId.value === sessionId) {
        clearOpenDetail();
      }
      await Promise.all([
        refreshStatus(),
        refreshRecentSessions(),
        refreshHistorical(),
        refreshAnalytics(),
      ]);
      return true;
    } catch (err) {
      console.error("[surveyTracker] delete_session failed:", err);
      return false;
    }
  }

  async function startSession(): Promise<number | null> {
    isBusy.value = true;
    try {
      const id = await invoke<number>("survey_tracker_start_session");
      await Promise.all([refreshStatus(), refreshRecentSessions(), refreshHistorical()]);
      return id;
    } catch (err) {
      console.error("[surveyTracker] start_session failed:", err);
      return null;
    } finally {
      isBusy.value = false;
    }
  }

  async function endSession(): Promise<number | null> {
    isBusy.value = true;
    try {
      const id = await invoke<number | null>("survey_tracker_end_session");
      await Promise.all([refreshStatus(), refreshRecentSessions(), refreshHistorical()]);
      // If the ended session is the one currently open in the detail panel,
      // refresh its detail too so the user sees the final state.
      if (openDetailSessionId.value !== null) {
        await loadSessionDetail(openDetailSessionId.value);
      }
      return id;
    } catch (err) {
      console.error("[surveyTracker] end_session failed:", err);
      return null;
    } finally {
      isBusy.value = false;
    }
  }

  const recordError = ref<string | null>(null);

  async function recordUse(
    mapInternalName: string,
    kind?: SurveyUseKind | null
  ): Promise<RecordUseResult | null> {
    isBusy.value = true;
    recordError.value = null;
    try {
      const res = await invoke<RecordUseResult>("survey_tracker_record_use", {
        mapInternalName,
        kind: kind ?? null,
      });
      await Promise.all([refreshStatus(), refreshRecentSessions(), refreshHistorical()]);
      // If the recorded use landed in the open detail's session, refresh it
      // so the new use shows up (mirrors endSession).
      if (
        openDetailSessionId.value !== null &&
        res.session_id === openDetailSessionId.value
      ) {
        await loadSessionDetail(openDetailSessionId.value);
      }
      return res;
    } catch (err) {
      console.error("[surveyTracker] record_use failed:", err);
      recordError.value = String(err);
      // record_manual_use may have persisted a session before insert_use
      // failed — refresh so status/recent don't hide the orphaned session.
      await Promise.all([refreshStatus(), refreshRecentSessions(), refreshHistorical()]);
      return null;
    } finally {
      isBusy.value = false;
    }
  }

  // ── Event handlers ────────────────────────────────────────────────────
  // Wired into startupStore alongside other Tauri event listeners. Each
  // handler refreshes only what could have changed — cheap queries, easy
  // to reason about.

  async function handleSessionStarted(_p: SessionStartedPayload) {
    await Promise.all([refreshStatus(), refreshRecentSessions(), refreshHistorical()]);
  }

  async function handleSessionEnded(_p: SessionEndedPayload) {
    await Promise.all([
      refreshStatus(),
      refreshRecentSessions(),
      refreshHistorical(),
      analytics.value !== null ? refreshAnalytics() : Promise.resolve(),
    ]);
    if (openDetailSessionId.value !== null) {
      await loadSessionDetail(openDetailSessionId.value);
    }
  }

  async function handleUseRecorded(p: UseRecordedPayload) {
    // A new use likely belongs to the active session; if its session_id
    // matches our open detail, refresh the detail too so the new use shows up.
    await refreshStatus();
    if (
      openDetailSessionId.value !== null &&
      p.session_id === openDetailSessionId.value
    ) {
      await loadSessionDetail(openDetailSessionId.value);
    }
    await refreshHistorical();
    if (analytics.value !== null) {
      await refreshAnalytics();
    }
  }

  async function handleUseCompleted(_useId: number) {
    // Status doesn't change (it's the open-nodes summary), but the open
    // detail's per-use status does.
    if (openDetailSessionId.value !== null) {
      await loadSessionDetail(openDetailSessionId.value);
    }
    await refreshHistorical();
    if (analytics.value !== null) {
      await refreshAnalytics();
    }
  }

  async function handleMultihitOpened(_p: MultihitNodePayload) {
    await refreshStatus();
  }

  async function handleMultihitClosed(_p: MultihitNodePayload) {
    await refreshStatus();
    if (openDetailSessionId.value !== null) {
      await loadSessionDetail(openDetailSessionId.value);
    }
  }

  // Initial fetch — call once at startup or when the user navigates to the
  // survey screen for the first time. Idempotent. Loads historical sessions
  // alongside status so the unified left panel has data immediately.
  async function init() {
    await Promise.all([
      refreshStatus(),
      refreshRecentSessions(),
      refreshHistorical(),
    ]);
  }

  return {
    // state
    status,
    recentSessions,
    openDetail,
    openDetailSessionId,
    historical,
    analytics,
    isBusy,
    // computed
    activeSession,
    hasActiveSession,
    openMultihitNodes,
    // commands
    init,
    refreshStatus,
    refreshRecentSessions,
    refreshHistorical,
    refreshAnalytics,
    loadSessionDetail,
    clearOpenDetail,
    startSession,
    endSession,
    recordError,
    recordUse,
    updateSessionNotes,
    updateSessionName,
    updateSessionTimes,
    deleteSession,
    // event handlers
    handleSessionStarted,
    handleSessionEnded,
    handleUseRecorded,
    handleUseCompleted,
    handleMultihitOpened,
    handleMultihitClosed,
  };
});
