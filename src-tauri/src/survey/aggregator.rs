//! Survey session aggregator — Phase 5 brains.
//!
//! Two feeds: chat-gain attribution (the coordinator calls
//! [`SurveySessionAggregator::attribute_chat_gain`] for every chat loot
//! gain) and manual sessions (the Survey tab's start/stop/record commands).
//! Chat gains attribute to a recent Basic use, the adopted survey node, or
//! — by item identity — the active session's most recent use; manual uses
//! seed the windows the chat path reads.
//!
//! See `docs/plans/survey-tracker-rewrite.md` for the full design and
//! `docs/architecture/survey-mechanics.md` for the kind-specific game
//! mechanics this implements.

use crate::cdn_commands::GameDataState;
use crate::game_data::SurveyKind;
use crate::survey::types::{SessionStartTrigger, SurveyUseKind, SurveyUseStatus};
use crate::survey::persistence;
use rusqlite::Connection;
use std::collections::VecDeque;
/// Chat-loot gate, Basic surveys: a `[Status]` gain counts as survey loot
/// only within this many seconds of a Basic survey use's `used_at` (the loot
/// lands in the same game tick as the map's consumption; the slack covers
/// second-truncation).
const CHAT_BASIC_WINDOW_SECS: i64 = 5;
/// Cap for the chat-side recent-basic-uses queue.
const CHAT_STATE_CAP: usize = 16;

/// What [`SurveySessionAggregator::record_manual_use`] needs beyond the
/// connection: the owning character/server plus the hand-framed use's
/// fields. Grouped so the manual-use entry point stays under the
/// argument-count lint without changing what it carries.
#[derive(Debug, Clone, Copy)]
pub struct ManualUse<'a> {
    pub character: &'a str,
    pub server: &'a str,
    pub area: Option<&'a str>,
    pub map_internal_name: &'a str,
    pub kind: SurveyUseKind,
    pub now_iso: &'a str,
}

/// What [`SurveySessionAggregator::attribute_chat_gain`] resolves before the
/// shared tail runs: the owning character/server plus the gain's ledger
/// fields. Grouped so the attribution tail stays under the argument-count
/// lint without changing what it carries.
#[derive(Debug, Clone, Copy)]
pub struct ChatAttribution<'a> {
    pub character: &'a str,
    pub server: &'a str,
    pub use_id: i64,
    pub quantity: u32,
    pub timestamp: &'a str,
    pub item_name: Option<&'a str>,
    pub mark_completed: bool,
}

/// Shared aggregator. The coordinator owns one instance for ALL
/// characters/servers, so every cache below is keyed by identity —
/// a cache entry for one character must never serve another.
pub struct SurveySessionAggregator {
    game_data: GameDataState,

    /// Recently-consumed Basic survey uses `(use_id, used_at UTC,
    /// character, server)`. Chat gains attribute to one of these only
    /// within [`CHAT_BASIC_WINDOW_SECS`] of its `used_at` (Basic loot
    /// lands in the same game tick as the map's consumption) AND for the
    /// same identity. Bounded FIFO.
    recent_basic_uses: VecDeque<(i64, chrono::NaiveDateTime, String, String)>,

    /// Cached active session id (DB is the source of truth, but we cache to
    /// avoid a query per event). `None` means "unknown — go check the DB".
    /// The identity is part of the key: a cached id for (A, server) never
    /// answers for (B, server). Refreshed when a session is started/ended.
    /// Lookup failures are never cached — a transient DB error must not
    /// pin "no session" until the next invalidation.
    cached_active_session: Option<(String, String, Option<i64>)>,
}

impl SurveySessionAggregator {
    pub fn new(game_data: GameDataState) -> Self {
        Self {
            game_data,
            recent_basic_uses: VecDeque::new(),
            cached_active_session: None,
        }
    }

    // ============================================================
    // Public API — session lifecycle
    // ============================================================

    /// Start a manual session. Errors if one is already active for this
    /// character/server. Returns the new session id.
    pub fn start_manual_session(
        &mut self,
        conn: &Connection,
        character: &str,
        server: &str,
        now_iso: &str,
    ) -> Result<i64, String> {
        if let Some(s) =
            persistence::active_session(conn, character, server).map_err(|e| e.to_string())?
        {
            return Err(format!("session {} already active", s.id));
        }
        let id = persistence::insert_session(
            conn,
            character,
            server,
            now_iso,
            SessionStartTrigger::Manual,
            None,
        )
        .map_err(|e| e.to_string())?;
        self.cached_active_session = Some((character.to_string(), server.to_string(), Some(id)));
        Ok(id)
    }

    /// End the active session (manual close). No-op if no session is active.
    /// Returns the ended session id, if any.
    ///
    /// The caller's `now_iso` is used as a fallback end timestamp, but the
    /// session's actual `started_at` / `ended_at` get recomputed from the
    /// first and last attributed event timestamps (see
    /// [`persistence::recompute_session_bounds_and_end`]). That gives more
    /// accurate bounds for live sessions and correct bounds for replayed /
    /// old-log sessions where wall-clock is meaningless.
    pub fn end_active_session(
        &mut self,
        conn: &Connection,
        character: &str,
        server: &str,
        now_iso: &str,
    ) -> Result<Option<i64>, String> {
        let active =
            persistence::active_session(conn, character, server).map_err(|e| e.to_string())?;
        let Some(s) = active else {
            return Ok(None);
        };
        persistence::end_session(conn, s.id, now_iso).map_err(|e| e.to_string())?;
        // The session is closed in the DB at this point, so the cache must
        // already say "no active" even if the bounds recompute below fails —
        // otherwise a stale id would keep routing new uses into the closed
        // session until the next invalidation.
        self.cached_active_session = Some((character.to_string(), server.to_string(), None));
        persistence::recompute_session_bounds_and_end(conn, s.id, now_iso)
            .map_err(|e| e.to_string())?;
        Ok(Some(s.id))
    }

    /// Record a hand-framed survey-map use from the manual "Record use" UI.
    ///
    /// Same tail the retired Player.log path used, but driven by an explicit
    /// user action instead of a log line: `now_iso` is wall-clock time (not
    /// a log timestamp), and a session is always created when none is
    /// active. Returns
    /// `(use_id, session_id)`.
    pub fn record_manual_use(
        &mut self,
        conn: &Connection,
        ManualUse {
            character,
            server,
            area,
            map_internal_name,
            kind,
            now_iso,
        }: ManualUse<'_>,
    ) -> Result<(i64, Option<i64>), String> {
        // Explicit user action — always create a session when none is active.
        let session_id = match self.fetch_active_session(conn, character, server) {
            Some(id) => Some(id),
            None => {
                let id = persistence::insert_session(
                    conn,
                    character,
                    server,
                    now_iso,
                    SessionStartTrigger::FirstUse,
                    None,
                )
                .map_err(|e| e.to_string())?;
                self.cached_active_session = Some((character.to_string(), server.to_string(), Some(id)));
                Some(id)
            }
        };

        // Resolve display name for this map via CDN.
        let display_name = self
            .game_data
            .try_read()
            .ok()
            .and_then(|gd| gd.resolve_item(map_internal_name).map(|i| i.name.clone()))
            .unwrap_or_else(|| map_internal_name.to_string());

        // Resolve area: prefer the live-tracked area, fall back to the
        // survey_types.zone column (populated from CDN at import time).
        let resolved_area: Option<String> = area.map(|s| s.to_string()).or_else(|| {
            conn.query_row(
                "SELECT zone FROM survey_types WHERE internal_name = ?1",
                rusqlite::params![map_internal_name],
                |r| r.get(0),
            )
            .ok()
            .flatten()
        });
        // Single write transaction per attempt: a failed count update rolls
        // back the use row instead of returning success with a stale session
        // count. No outer transaction is open here (fresh pool checkout in
        // the command), so new_unchecked is safe.
        // Transparent stale-session retry: the cached active-session id can
        // point at a session deleted out from under us (History UI deletes
        // directly on the DB), which trips the survey_uses → survey_sessions
        // FK. Drop the cache, re-resolve (starting a fresh session),
        // re-insert once.
        let mut session_id = session_id;
        let mut retried = false;
        let use_id = loop {
            let tx = rusqlite::Transaction::new_unchecked(
                conn,
                rusqlite::TransactionBehavior::Immediate,
            )
            .map_err(|e| e.to_string())?;

            match persistence::insert_use(
                &tx,
                session_id,
                character,
                server,
                now_iso,
                map_internal_name,
                &display_name,
                kind,
                resolved_area.as_deref(),
            ) {
                Ok(id) => {
                    if let Some(s) = session_id {
                        persistence::increment_consumed_count(&tx, s)
                            .map_err(|e| format!("survey count update failed: {e}"))?;
                    }
                    tx.commit()
                        .map_err(|e| format!("survey use transaction failed: {e}"))?;
                    break id;
                }
                Err(_e) if session_id.is_some() && !retried => {
                    retried = true;
                    drop(tx);
                    self.cached_active_session = None;
                    session_id = match self.fetch_active_session(conn, character, server) {
                        Some(id) => Some(id),
                        None => {
                            let id = persistence::insert_session(
                                conn,
                                character,
                                server,
                                now_iso,
                                SessionStartTrigger::FirstUse,
                                None,
                            )
                            .map_err(|e| e.to_string())?;
                            self.cached_active_session = Some((character.to_string(), server.to_string(), Some(id)));
                            Some(id)
                        }
                    };
                    continue;
                }
                Err(e) => {
                    eprintln!("[survey-aggregator] insert_use failed: {e}");
                    self.cached_active_session = None;
                    return Err(format!(
                        "survey use insert failed (the session may have been deleted; try again): {e}"
                    ));
                }
            }
        };

        if let Some(s) = session_id {
            // Tighten started_at so the session header reflects the actual
            // first survey use, not the session-creation moment.
            // Best-effort cosmetic bump (post-commit): log, never propagate.
            if let Err(e) = persistence::tighten_started_at(conn, s, now_iso) {
                eprintln!("[survey-aggregator] manual tighten_started_at failed: {e}");
            }
        }

        // Chat-loot gating for Basic surveys: claim rows already in the
        // ledger within the same-tick window (chat loot lands in the same
        // game tick as the map's consumption).
        if kind == SurveyUseKind::Basic {
            if let Some(used_at) = parse_utc_datetime(now_iso) {
                remember_bounded(
                    &mut self.recent_basic_uses,
                    (use_id, used_at, character.to_string(), server.to_string()),
                );
            }
            // One write transaction for the retro-claim: the row links and
            // the quantity/status updates commit together. If any step
            // fails, the rollback leaves the rows untagged so a later
            // pass can reclaim them — never linked rows with a wrong
            // `loot_qty`. Same unchecked-transaction shape as the use
            // insert above: no outer transaction is open here.
            let claim_outcome = (|| -> Result<u32, String> {
                let tx = rusqlite::Transaction::new_unchecked(
                    conn,
                    rusqlite::TransactionBehavior::Immediate,
                )
                .map_err(|e| format!("claim txn begin failed: {e}"))?;
                let claimed = persistence::claim_unlinked_chat_loot_near(
                    &tx,
                    use_id,
                    character,
                    server,
                    now_iso,
                    CHAT_BASIC_WINDOW_SECS as u32,
                )
                .map_err(|e| format!("claim rows failed: {e}"))?;
                if claimed > 0 {
                    persistence::add_loot_qty(&tx, use_id, claimed)
                        .map_err(|e| format!("claim qty update failed: {e}"))?;
                    persistence::set_use_status(&tx, use_id, SurveyUseStatus::Completed)
                        .map_err(|e| format!("claim status update failed: {e}"))?;
                }
                tx.commit().map_err(|e| format!("claim txn commit failed: {e}"))?;
                Ok(claimed)
            })();
            match claim_outcome {
                Ok(0) => {}
                Ok(_) => {
                    update_session_loot_timestamps(conn, use_id, now_iso);
                }
                Err(e) => {
                    eprintln!("[survey-aggregator] retro-claim rolled back: {e}");
                }
            }
        }

        Ok((use_id, session_id))
    }

    /// Attribute a Chat.log `[Status]` "added to inventory" loot gain to the
    /// survey use it was collected from, independent of the Player.log
    /// pipeline.
    ///
    /// This is the **chat-authoritative** loot path — the only loot path
    /// since the Player.log pipeline was deleted. Project Gorgon always
    /// writes survey loot to the Status chat channel, so chat gains drive
    /// attribution directly (this is what Kaeus' GorgonSurveyTracker reads,
    /// and why its summary works where the old log path did not).
    ///
    /// Chat lines carry no node/map identity, so the gain is gated two ways
    /// (see docs/architecture/survey-mechanics.md):
    ///
    /// Attribution is two-tiered: a precise window for **Basic** uses
    /// (loot lands in the same game tick as the map's consumption →
    /// attribute within [`CHAT_BASIC_WINDOW_SECS`] of a recently-consumed
    /// Basic use), then the **item-identity fallback** below for
    /// everything else — including Motherlode/Multihit loot, which has no
    /// chat-visible swing signal to gate on. The loot summary counts
    /// pending-loot uses, so Motherlode/Multihit attribution needs no
    /// completion story beyond the manual session end.
    ///
    /// **Item-identity fallback** (robust — needs no Player.log timing
    /// signals, which don't reach every user's log): items identifiable as
    /// survey yield ([`Self::is_survey_loot_item`]: `Ore`/`MetalSlab`
    /// keywords, Semi-Real Hassium, Magic Sand) attribute to the active
    /// session's most recent use whenever a session is running. Regular
    /// mining during a session counts too — accepted trade-off; the item
    /// identity is the strongest signal chat offers.
    ///
    /// Anything else gained mid-session — kill loot, foraging, quest items,
    /// crafted survey maps — is **not** survey loot and returns `None`.
    ///
    /// Returns the `survey_use_id` the caller should stamp into the chat
    /// row's `source_details` (so the loot summary query can join it).
    ///
    /// `internal_name` is the CDN internal name of the gained item when
    /// resolvable; `timestamp` is the UTC `YYYY-MM-DD HH:MM:SS` timestamp of
    /// the chat line.
#[allow(clippy::too_many_arguments)]
    pub fn attribute_chat_gain(
        &mut self,
        conn: &Connection,
        character: &str,
        server: &str,
        internal_name: Option<&str>,
        quantity: u32,
        timestamp: &str,
        display_name: Option<&str>,
    ) -> Option<i64> {
        // A survey map entering inventory is a craft, never survey loot.
        // Exclusion when (lookup says map) OR (game data unavailable AND
        // the display name carries the verified "survey" signal).
        let data_available = self.game_data.try_read().is_ok_and(|gd| !gd.items.is_empty());
        let lookup_map = internal_name
            .and_then(|i| self.lookup_survey_kind(i))
            .is_some();
        let display_signal = display_name
            .map(|n| n.to_ascii_lowercase().contains("survey"))
            .unwrap_or(false);
        if lookup_map || (!data_available && display_signal) {
            return None;
        }
        let chat_ts = parse_utc_datetime(timestamp)?;

        // Basic window: same-tick loot around a recent Basic consumption
        // (newest first — back-to-back speed-bonus chains resolve to the
        // most recent map). Identity-scoped: a use recorded for another
        // character/server never matches, even inside the time window.
        // Newest use at-or-before the gain wins first: a delayed
        // older-stamped gain arriving after a newer use must not land on
        // that newer use. Only when nothing precedes the gain, allow a
        // newer use within the window (pipeline clock skew can stamp the
        // gain just before the recorded use).
        let basic_uses_in_window = |after_gain: bool| {
            self.recent_basic_uses
                .iter()
                .rev()
                .filter(move |(_, used_at, c, s)| {
                    c == character
                        && s == server
                        && (*used_at <= chat_ts) != after_gain
                        && (chat_ts - *used_at).num_seconds().abs() <= CHAT_BASIC_WINDOW_SECS
                })
                .map(|(use_id, _, _, _)| *use_id)
        };
        let basic_hit = basic_uses_in_window(false)
            .next()
            .or_else(|| basic_uses_in_window(true).next());
        if let Some(use_id) = basic_hit {
            return self.record_chat_attribution(
                conn,
                ChatAttribution {
                    character,
                    server,
                    use_id,
                    quantity,
                    timestamp,
                    item_name: internal_name,
                    mark_completed: true,
                },
            );
        }

        // Identity fallback: survey-yield items count while a session is
        // active, attributed to the most recent use at-or-before the gain
        // (a delayed older-stamped gain must not land on a newer use).
        // Basic uses only attract same-tick gains: without the window, an
        // ordinary mining gain minutes after a Basic use would land on it,
        // bypassing the 5s rule above. Other kinds have no window mechanism,
        // so the fallback is their only attribution path — preserve it.
        if internal_name.is_some_and(|internal| self.is_survey_loot_item(internal))
            && let Some(session_id) = self.fetch_active_session(conn, character, server)
            && let Ok(Some((use_id, kind, used_at))) =
                persistence::latest_use_for_session(conn, session_id, timestamp)
            && (kind != SurveyUseKind::Basic || {
                parse_utc_datetime(&used_at).is_some_and(|used| {
                    (chat_ts - used).num_seconds().abs() <= CHAT_BASIC_WINDOW_SECS
                })
            })
        {
            return self.record_chat_attribution(
                conn,
                ChatAttribution {
                    character,
                    server,
                    use_id,
                    quantity,
                    timestamp,
                    item_name: internal_name,
                    mark_completed: false,
                },
            );
        }

        None
    }

    /// Is this item identifiable as survey yield by its CDN identity alone?
    /// - `Ore` keyword: metal-survey loot (Copper/Silver/Gold Ore, Rhodium,
    ///   Stibnite, Paladium, Iridium, Pyrite, Cinnabar, Tungsten,
    ///   Orichalcum, Molybdenum, Gold Nugget, …).
    /// - `MetalSlab` keyword: every "… Metal Slab" tier.
    /// - Color-crystal keywords: the base crystal families the color-named
    ///   mineral surveys yield (Blue/Green/White/Orange/Yellow Mineral +
    ///   Rubywall, whose internal name IS `RedCrystal`), covering
    ///   Massive/Maximized variants too. Deliberately NOT the bare
    ///   `Crystal` keyword — that would drag in emotion pearls, mind gems,
    ///   gem fragments, and other non-survey items.
    /// - Semi-Real Hassium (own keyword); Magic Sand, Sulfur, Saltpeter by
    ///   internal name (no distinctive keyword).
    fn is_survey_loot_item(&self, internal_name: &str) -> bool {
        const YIELD_KEYWORDS: [&str; 9] = [
            "Ore",
            "MetalSlab",
            "SemiRealHassium",
            "BlueCrystal",
            "GreenCrystal",
            "WhiteCrystal",
            "OrangeCrystal",
            "YellowCrystal",
            "RedCrystal",
        ];
        const YIELD_INTERNAL_NAMES: [&str; 3] = ["MagicSand", "Sulfur", "Saltpeter"];

        let Ok(gd) = self.game_data.try_read() else {
            return false;
        };
        let Some(info) = gd.resolve_item(internal_name) else {
            return false;
        };
        info.keywords
            .iter()
            .any(|k| YIELD_KEYWORDS.iter().any(|y| k == y))
            || info
                .internal_name
                .as_deref()
                .is_some_and(|n| YIELD_INTERNAL_NAMES.contains(&n))
    }

    /// Shared tail of a successful chat attribution: bump the use's
    /// denormalized loot total and the session's loot timestamps. Basic uses
    /// complete on their (same-tick) loot; mining uses complete when their
    /// engagement closes.
    fn record_chat_attribution(
        &mut self,
        conn: &Connection,
        ChatAttribution {
            character,
            server,
            use_id,
            quantity,
            timestamp,
            item_name,
            mark_completed,
        }: ChatAttribution<'_>,
    ) -> Option<i64> {
        // Single loot authority for `loot_qty`: a gain seen by both pipelines
        // bumps once. Pre-cutoff databases hold `player_log` twin rows (the
        // retired Player.log pipeline usually landed first); when a rescan
        // re-attributes the chat twin, the tagged-or-untagged legacy row is
        // already in the ledger — the bump is skipped but timestamps/status
        // and the returned `use_id` (which tags the chat row) are unchanged.
        // Per-item rows stay in both pipelines; the summary's `NOT EXISTS`
        // read guard dedups those.
        let already_counted = persistence::player_gain_already_counted(
            conn,
            use_id,
            quantity,
            timestamp,
            item_name,
            CHAT_BASIC_WINDOW_SECS as u32,
        ) || persistence::untagged_player_gain_exists(
            conn,
            character,
            server,
            quantity,
            timestamp,
            item_name,
            CHAT_BASIC_WINDOW_SECS as u32,
        );
        if !already_counted
            && let Err(e) = persistence::add_loot_qty(conn, use_id, quantity) {
                eprintln!("[survey-aggregator] chat add_loot_qty failed: {e}");
                return None;
            }
        update_session_loot_timestamps(conn, use_id, timestamp);
        if mark_completed {
            // Post-attribution cosmetic mark: log, never propagate.
            if let Err(e) = persistence::set_use_status(conn, use_id, SurveyUseStatus::Completed) {
                eprintln!("[survey-aggregator] chat set_use_status failed: {e}");
            }
        }
        Some(use_id)
    }

    // ============================================================
    // Event ingestion
    // ============================================================


    /// Bump the in-memory current-mining-entity tracking when a non-mining
    /// interaction starts. The coordinator can call this from
    /// `InteractionStarted` events. Mining state must clear when the player
    /// switches to a non-mining interaction (e.g., corpse search) so the
    /// next mining cycle is correctly recognized as a "different entity".
    #[allow(dead_code)] // Reserved hook for future mining-state clearing
    pub fn note_interaction_started(&mut self, _entity_id: u32) {
        // Currently a no-op: a different-entity mining cycle naturally
        // closes the previous one. Reserved as a hook in case we later
        // find we need to clear mining state earlier (e.g., on a confirmed
        // combat interruption).
    }

    // ============================================================
    // Helpers
    // ============================================================

    /// Look up the survey kind for an item by internal name. Returns `None`
    /// if the item isn't a survey map (the common case).
    pub(crate) fn lookup_survey_kind(&self, internal_name: &str) -> Option<SurveyUseKind> {
        let gd = self.game_data.try_read().ok()?;
        gd.resolve_item(internal_name)
            .and_then(|info| info.survey_kind())
            .map(|k: SurveyKind| k.into())
    }

    /// Cached active-session-id lookup.
    /// Drop the cached active-session id (and the deleted session's
    /// Basic-use window entries, keyed by use id) so the next lookup
    /// re-resolves from the DB. Call this when a session is removed by a
    /// path that bypasses the aggregator's own start/end flow — notably
    /// the History UI's `survey_tracker_delete_session`, which deletes
    /// rows directly. Without this, the cache would keep handing out a
    /// now-missing session id and every subsequent `insert_use` would fail
    /// the FK constraint, and a queued gain could still attribute to the
    /// deleted use. Only the deleted session's use ids leave the window
    /// queue: entries belong to every live session, and a global clear
    /// would steal the attribution window of an unrelated open use.
    pub fn invalidate_session_cache(&mut self, deleted_use_ids: &[i64]) {
        self.cached_active_session = None;
        // The Basic-use window is keyed by use id too: a deleted use must
        // not keep attracting gains timestamped inside its old window.
        // Worst case a gain goes unattributed (healable via retro-claim)
        // instead of landing on a use that no longer exists.
        self.recent_basic_uses
            .retain(|(use_id, _, _, _)| !deleted_use_ids.contains(use_id));
    }

    fn fetch_active_session(
        &mut self,
        conn: &Connection,
        character: &str,
        server: &str,
    ) -> Option<i64> {
        // Identity-keyed: a cached id only answers for the identity that
        // stored it. Anything else re-resolves from the DB.
        if let Some((c, s, id)) = &self.cached_active_session
            && c == character
            && s == server
        {
            return *id;
        }
        // A failed lookup is NOT cached: caching `None` on a transient DB
        // error would pin "no session" (and route every later event down
        // the create-new-session path) until the next invalidation.
        match persistence::active_session(conn, character, server) {
            Ok(s) => {
                let id = s.map(|s| s.id);
                self.cached_active_session =
                    Some((character.to_string(), server.to_string(), id));
                id
            }
            Err(e) => {
                eprintln!("[survey-aggregator] active-session lookup failed, not caching: {e}");
                None
            }
        }
    }
}

/// After attributing loot to a survey use, update the session's
/// `first_loot_at` / `last_loot_at` timestamps. Called from every code
/// path that calls `add_loot_qty`. The lookup of `session_id` from the
/// use row is one cheap indexed read that only fires when loot is
/// attributed (not on every event).
fn update_session_loot_timestamps(conn: &Connection, use_id: i64, ts: &str) {
    match persistence::get_use(conn, use_id) {
        Ok(Some(su)) => {
            if let Some(sid) = su.session_id {
                if let Err(e) = persistence::update_first_loot_at(conn, sid, ts) {
                    eprintln!("[survey-aggregator] first-loot stamp failed: {e}");
                }
                if let Err(e) = persistence::update_last_loot_at(conn, sid, ts) {
                    eprintln!("[survey-aggregator] last-loot stamp failed: {e}");
                }
                // Keep started_at tight for open sessions so the header
                // reflects actual activity, not the session-creation moment.
                if let Err(e) = persistence::tighten_started_at(conn, sid, ts) {
                    eprintln!("[survey-aggregator] started-at tighten failed: {e}");
                }
            }
        }
        Ok(None) => {}
        Err(e) => eprintln!("[survey-aggregator] session-use lookup failed, skipping loot stamps: {e}"),
    }
}

// ============================================================
// Free helpers
// ============================================================





/// Parse a UTC `YYYY-MM-DD HH:MM:SS` string (the shape produced by
/// `to_utc_datetime_with_base` and by the chat pipeline) for window math.
fn parse_utc_datetime(ts: &str) -> Option<chrono::NaiveDateTime> {
    chrono::NaiveDateTime::parse_from_str(ts, "%Y-%m-%d %H:%M:%S").ok()
}

/// Push into a small bounded FIFO memory, skipping duplicates.
fn remember_bounded<T: PartialEq>(queue: &mut VecDeque<T>, value: T) {
    if queue.contains(&value) {
        return;
    }
    queue.push_back(value);
    while queue.len() > CHAT_STATE_CAP {
        queue.pop_front();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::migrations::run_migrations;
    use rusqlite::Connection;

    fn fresh_db() -> Connection {
        let conn = Connection::open_in_memory().expect("open in-memory db");
        run_migrations(&conn, None).expect("run migrations");
        conn
    }

    fn empty_aggregator() -> SurveySessionAggregator {
        SurveySessionAggregator::new(std::sync::Arc::new(
            tauri::async_runtime::RwLock::new(crate::game_data::GameData::empty()),
        ))
    }

    /// Aggregator with `IronOre` resolvable as survey yield (`Ore` keyword).
    /// The fallback path is gated on CDN identity, so tests exercising it
    /// must seed game data — with `GameData::empty()` the gate is false
    /// and every gain falls through unattributed.
    fn seeded_aggregator() -> SurveySessionAggregator {
        use crate::game_data::ItemInfo;
        let mut gd = crate::game_data::GameData::empty();
        gd.items.insert(
            1,
            ItemInfo {
                id: 1,
                name: "Iron Ore".to_string(),
                description: None,
                icon_id: None,
                value: None,
                max_stack_size: None,
                keywords: vec!["Ore".to_string()],
                effect_descs: vec![],
                internal_name: Some("IronOre".to_string()),
                food_desc: None,
                equip_slot: None,
                num_uses: None,
                skill_reqs: None,
                behaviors: None,
                bestow_recipes: None,
                bestow_ability: None,
                bestow_quest: None,
                bestow_title: None,
                craft_points: None,
                crafting_target_level: None,
                tsys_profile: None,
                raw_json: serde_json::Value::Null,
            },
        );
        gd.item_internal_name_index
            .insert("IronOre".to_string(), 1);
        SurveySessionAggregator::new(std::sync::Arc::new(
            tauri::async_runtime::RwLock::new(gd),
        ))
    }

    fn record_basic(
        agg: &mut SurveySessionAggregator,
        conn: &Connection,
        now_iso: &str,
    ) -> i64 {
        let (use_id, _) = agg
            .record_manual_use(
                conn,
                ManualUse {
                    character: "Zenith",
                    server: "Dreva",
                    area: None,
                    map_internal_name: "MiningSurveySerbule1",
                    kind: SurveyUseKind::Basic,
                    now_iso,
                },
            )
            .expect("record manual Basic use");
        use_id
    }

    #[test]
    fn delayed_gain_lands_on_preceding_use_not_newer_one() {
        // Use A at 12:00:00, use B at 12:00:04; A's gain arrives after B
        // was recorded. Newest-at-or-before wins (A) — a plain
        // absolute-difference window would hand it to B (4s < 5s window).
        let conn = fresh_db();
        let mut agg = empty_aggregator();
        let use_a = record_basic(&mut agg, &conn, "2026-04-15 12:00:00");
        let _use_b = record_basic(&mut agg, &conn, "2026-04-15 12:00:04");
        let hit = agg.attribute_chat_gain(
            &conn,
            "Zenith",
            "Dreva",
            Some("IronOre"),
            2,
            "2026-04-15 12:00:00",
            None,
        );
        assert_eq!(hit, Some(use_a));
        let stored = persistence::get_use(&conn, use_a)
            .expect("read use")
            .expect("use present");
        assert_eq!(stored.loot_qty, 2);
    }

    #[test]
    fn skewed_gain_before_use_still_matches_when_nothing_precedes() {
        // Pipeline skew can stamp the gain just before the recorded use:
        // with no preceding use in the window, the newer use still matches.
        let conn = fresh_db();
        let mut agg = empty_aggregator();
        let use_id = record_basic(&mut agg, &conn, "2026-04-15 12:00:05");
        let hit = agg.attribute_chat_gain(
            &conn,
            "Zenith",
            "Dreva",
            Some("IronOre"),
            1,
            "2026-04-15 12:00:04",
            None,
        );
        assert_eq!(hit, Some(use_id));
    }

    #[test]
    fn late_gain_does_not_fallback_onto_basic_use() {
        // Basic use at 12:00:00; an ordinary mining gain a minute later
        // misses the 5s window AND the fallback (Basic attracts same-tick
        // gains only). Seeded game data opens the yield-identity gate, so
        // the None comes from the window — never from a closed gate. The
        // gain stays unattributed instead of inflating the use.
        let conn = fresh_db();
        let mut agg = seeded_aggregator();
        let use_id = record_basic(&mut agg, &conn, "2026-04-15 12:00:00");
        let hit = agg.attribute_chat_gain(
            &conn,
            "Zenith",
            "Dreva",
            Some("IronOre"),
            3,
            "2026-04-15 12:01:00",
            None,
        );
        assert_eq!(hit, None);
        let stored = persistence::get_use(&conn, use_id)
            .expect("read use")
            .expect("use present");
        assert_eq!(stored.loot_qty, 0);
    }

    #[test]
    fn delete_invalidate_keeps_other_identities_attribution() {
        // Scoped invalidation drops only the deleted session's use ids.
        // The retained identity's gain is stamped just BEFORE its Basic
        // use (pipeline clock skew): it can only attribute via the
        // tier-1 after_gain queue arm, because the identity fallback's
        // `latest_use_for_session` requires `used_at <= gain` and the
        // only use postdates the gain. Under a global queue clear this
        // gain would return None — so `Some` here discriminates scoped
        // retain from global clear. The deleted use's queued gain no
        // longer resolves (queue dropped) — the identity fallback
        // cannot rescue it because the gain carries no survey-yield item.
        let conn = fresh_db();
        let mut agg = seeded_aggregator();
        let use_a = record_basic(&mut agg, &conn, "2026-04-15 12:00:00");
        let (use_b, _) = agg
            .record_manual_use(
                &conn,
                ManualUse {
                    character: "AltB",
                    server: "Legacy",
                    area: None,
                    map_internal_name: "MiningSurveySerbule1",
                    kind: SurveyUseKind::Basic,
                    now_iso: "2026-04-15 12:00:05",
                },
            )
            .expect("record manual Basic use");
        agg.invalidate_session_cache(&[use_a]);

        // Gain stamped 1s before the retained use: tier-1 after_gain arm
        // only. The fallback cannot rescue it (use_b postdates the gain).
        let hit = agg.attribute_chat_gain(
            &conn,
            "AltB",
            "Legacy",
            None, // unidentified gain: the identity fallback cannot fire, so
            // a match proves the queue entry survived invalidation.
            1,
            "2026-04-15 12:00:04",
            None,
        );
        assert_eq!(hit, Some(use_b));
        // Non-survey-yield item: identity fallback cannot rescue, so a
        // non-None result here would mean the queue entry survived
        // invalidation (the bug this test guards against).
        let hit = agg.attribute_chat_gain(
            &conn,
            "Zenith",
            "Dreva",
            None,
            1,
            "2026-04-15 12:00:01",
            None,
        );
        assert_eq!(hit, None);
    }

    #[test]
    fn empty_data_map_gain_is_none() {
        let conn = fresh_db();
        let mut agg = empty_aggregator();
        let use_id = record_basic(&mut agg, &conn, "2026-04-15 12:00:00");
        let _ = use_id;
        let hit = agg.attribute_chat_gain(
            &conn,
            "Zenith",
            "Dreva",
            None,
            1,
            "2026-04-15 12:00:01",
            Some("Serbule Blue Mineral Survey"),
        );
        assert_eq!(hit, None);
    }

    #[test]
    fn empty_data_ore_basic_use_still_attributes() {
        let conn = fresh_db();
        let mut agg = empty_aggregator();
        let use_id = record_basic(&mut agg, &conn, "2026-04-15 12:00:00");
        let hit = agg.attribute_chat_gain(
            &conn,
            "Zenith",
            "Dreva",
            None,
            1,
            "2026-04-15 12:00:01",
            Some("Iron Ore"),
        );
        // offline users preserved: ore gain still attributes via Basic window
        assert_eq!(hit, Some(use_id));
    }

    #[test]
    fn seeded_real_map_is_none() {
        // seeded game data resolves the map; need a registered survey map item.
        // Use internal name that lookup would resolve; here we rely on display
        // signal + seeded data path. Register a map item in game data.
        let conn = fresh_db();
        let mut agg = seeded_aggregator();
        // add a survey map item to game data
        {
            use crate::game_data::ItemInfo;
            let mut gd = agg.game_data.try_write().unwrap();
            gd.items.insert(
                2,
                ItemInfo {
                    id: 2,
                    name: "Serbule Blue Mineral Survey".to_string(),
                    description: None,
                    icon_id: None,
                    value: None,
                    max_stack_size: None,
                    keywords: vec!["MineralSurvey".to_string(), "Document".to_string()],
                    effect_descs: vec![],
                    internal_name: Some("GeologySurveySerbule1".to_string()),
                    food_desc: None,
                    equip_slot: None,
                    num_uses: None,
                    skill_reqs: None,
                    behaviors: None,
                    bestow_recipes: None,
                    bestow_ability: None,
                    bestow_quest: None,
                    bestow_title: None,
                    craft_points: None,
                    crafting_target_level: None,
                    tsys_profile: None,
                    raw_json: serde_json::Value::Null,
                },
            );
            gd.item_internal_name_index
                .insert("GeologySurveySerbule1".to_string(), 2);
        }
        let _use_id = record_basic(&mut agg, &conn, "2026-04-15 12:00:00");
        let hit = agg.attribute_chat_gain(
            &conn,
            "Zenith",
            "Dreva",
            Some("GeologySurveySerbule1"),
            1,
            "2026-04-15 12:00:01",
            Some("Serbule Blue Mineral Survey"),
        );
        assert_eq!(hit, None);
    }
}
