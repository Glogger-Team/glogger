//! Survey session aggregator — Phase 5 brains.
//!
//! Subscribes to [`PlayerEvent`]s as they flow through the coordinator and:
//!
//! - Detects survey-map crafting and use, recording rows in `survey_sessions`
//!   and `survey_uses`.
//! - Drives the session lifecycle state machine (manual / crafting / first-use
//!   start; auto-end for crafting sessions when consumed catches crafted).
//! - Performs **A3 stitching**: when a Mining context opens within 60s of a
//!   survey-map use being consumed, gains in that mining cycle get the
//!   originating `survey_use_id` injected into their `ItemProvenance` so
//!   downstream queries can group "all loot from this survey use".
//! - Tracks per-kind windows: Basic (single tick), Motherlode (one mining
//!   cycle), Multihit (different-entity OR 30min via DB-backed
//!   `open_multihit_nodes`).
//!
//! See `docs/plans/survey-tracker-rewrite.md` for the full design and
//! `docs/architecture/survey-mechanics.md` for the kind-specific game
//! mechanics this implements.

use crate::cdn_commands::GameDataState;
use crate::game_data::SurveyKind;
use crate::parsers::LogDateResolver;
use crate::player_event_parser::{ActivitySource, ItemProvenance, PlayerEvent};
use crate::survey::types::{SessionStartTrigger, SurveyUseKind, SurveyUseStatus};
use crate::survey::{multihit_state, persistence};
use rusqlite::Connection;
use std::collections::VecDeque;

/// Window (seconds) within which a Mining context that opens after a survey
/// map is consumed inherits the originating use's `survey_use_id`. Covers
/// the player walking from where they used the map to where the node spawned.
const SURVEY_TO_MINING_GRACE_SECS: u32 = 60;

/// Multihit node timeout: if no mining hit lands on a tracked node for this
/// long, the node is considered gone and the use closed.
/// See docs/architecture/survey-mechanics.md for why this is 30 minutes.
const MULTIHIT_TIMEOUT_SECS: u32 = 30 * 60;

/// Maximum span (in log-derived seconds, from `started_at` to the current
/// event) a single survey session may cover before it's rolled over to a
/// fresh one. Bounds the size of any one session so a marathon — or
/// left-running-and-forgotten — play session can't accumulate into one
/// enormous row that overwhelms (or crashes) the UI. The replacement session
/// auto-starts on the next survey craft/use via the normal path.
const SESSION_MAX_DURATION_SECS: i64 = 2 * 60 * 60;

/// Chat-loot gate, Basic surveys: a `[Status]` gain counts as survey loot
/// only within this many seconds of a Basic survey use's `used_at` (the loot
/// lands in the same game tick as the map's DeleteItem; the slack covers
/// second-truncation and cross-log write skew).
const CHAT_BASIC_WINDOW_SECS: i64 = 5;

/// Chat-loot gate, Motherlode/Multihit: a mining swing's loot window extends
/// this many seconds past the swing's expected completion (loop start +
/// duration). The loot chat lines land at completion; the slack covers
/// second-truncation and cross-log write skew.
const CHAT_SWING_SLACK_SECS: i64 = 5;

/// A mining delay loop is bound to the node of the most recent
/// `StartInteraction` only if that interaction happened at most this many
/// seconds earlier (they fire in the same tick in practice).
const INTERACTION_TO_LOOP_MAX_SECS: u32 = 10;

/// Cap for the small chat-side memory queues (recent basic uses, known
/// survey nodes, chat-adopted uses).
const CHAT_STATE_CAP: usize = 16;

/// Cap for the deferred basic-gain buffer (gains arriving before the
/// consuming DeleteItem creates the pending use). Bounds memory when the
/// delete never comes (missed line, filtered log); oldest entries drop
/// first with an `eprintln!` log line per drop.
const DEFERRED_BASIC_CAP: usize = 128;

/// One survey-map use awaiting its first Mining context. Lives in memory
/// only — the use itself is already persisted in `survey_uses` with
/// `status = pending_loot`. This entry just remembers we expect a Mining
/// context to start within 60s.
#[derive(Debug, Clone)]
struct PendingUse {
    survey_use_id: i64,
    kind: SurveyUseKind,
    /// Seconds-of-day for `used_at`. Kept for ordering/display; window math
    /// uses `used_at` (full datetime) so grace windows survive midnight.
    used_at_secs: u32,
    /// Full UTC `used_at` of the survey use, mirroring the chat path's
    /// `recent_basic_uses` datetimes. `None` when the timestamp didn't
    /// parse (defensive — window checks then fall back to secs-of-day).
    used_at: Option<chrono::NaiveDateTime>,
}

/// A basic-survey loot gain that arrived *before* the survey map's
/// `ProcessDeleteItem` created the pending use. Buffered so it can be
/// attributed once the deletion fires and the pending use exists.
///
/// In the game's log, the order for basic surveys is:
///   1. ProcessAddItem (or ProcessUpdateItemCode) — primary loot
///   2. ProcessDeleteItem — survey map consumed
///   3. ProcessAddItem — speed-bonus loot (if any)
///
/// The pending use is created at step 2, so gains from step 1 arrive
/// before the pending use exists. This struct holds them until step 2.
#[derive(Debug, Clone)]
struct DeferredBasicGain {
    /// Quantity (1 for ItemAdded, delta for ItemStackChanged).
    qty: u32,
    /// Log-internal item name for the single-authority tie-break in
    /// [`crate::survey::persistence::unclaimed_chat_gain_exists`]. Same-tick
    /// multi-item loot shares qty/time (two qty-1 gains); the name keeps a
    /// chat twin of A from suppressing the bump for B.
    item_name: Option<String>,
    /// Display name of the survey map this gain was attributed to
    /// (`SurveyMapUse.survey_map_internal_name`). The manual path drains
    /// only gains matching its own map; a manual use for a different map
    /// must not consume another map's buffered gain.
    map_display_name: Option<String>,
    /// The gain line's own UTC stamp, resolved **in order** when the line was
    /// seen (`event_iso`) — never re-resolved at drain time, which would resolve
    /// an earlier time of day after the consuming map's and roll the session date
    resolved_at: Option<String>,
}

/// Chat-side record of the survey-spawned node currently being mined
/// (Motherlode/Multihit). Opened when a `Mining...` delay loop is bound to a
/// survey use; each swing refreshes the loot window. Chat `[Status]` gains
/// attribute as survey loot only while inside the current swing's window —
/// this is what keeps kill/forage/other loot out of the survey summary.
#[derive(Debug, Clone)]
struct ChatMiningState {
    survey_use_id: i64,
    /// Node entity id from the StartInteraction preceding the adoption
    /// swing, when one was captured. `None` = never learned (attribution
    /// still works; different-node detection is weaker).
    node_entity: Option<u32>,
    /// UTC start of the current swing.
    swing_start: chrono::NaiveDateTime,
    /// Loot window length for the current swing: ceil(duration) + slack.
    swing_window_secs: i64,
}

/// Side-effect events the aggregator returns so the coordinator can emit
/// them to the frontend. Kept narrow on purpose — frontend gets the high-
/// signal moments, not every internal state transition.
#[derive(Debug, Clone)]
pub enum SurveyAggregatorEvent {
    SessionStarted {
        session_id: i64,
        trigger: SessionStartTrigger,
    },
    SessionEnded {
        session_id: i64,
        // 'manual' | 'auto' | 'idle'
        reason: &'static str,
    },
    UseRecorded {
        use_id: i64,
        session_id: Option<i64>,
        map_internal_name: String,
        kind: SurveyUseKind,
    },
    UseCompleted { use_id: i64 },
    MultihitNodeOpened { use_id: i64, node_entity_id: u32 },
    MultihitNodeClosed { use_id: i64, node_entity_id: u32 },
}

/// Per-character aggregator. The coordinator owns one instance and feeds it
/// `PlayerEvent`s as they arrive.
pub struct SurveySessionAggregator {
    game_data: GameDataState,

    /// Recently-used survey maps awaiting their first Mining context.
    /// Sorted by `used_at_secs` ascending (front = oldest); aged out on each
    /// event tick.
    pending_uses: VecDeque<PendingUse>,

    /// The mining node the player is currently swinging at. Only one node
    /// at a time (you can't mine two simultaneously). Updated on
    /// `DelayLoopStarted("Mining...")` and cleared when a different mining
    /// interaction starts or the player stops mining.
    current_mining_node: Option<u32>,

    /// Active Motherlode attribution. When a pending Motherlode use adopts
    /// a mining node, we record `(node_entity_id, survey_use_id,
    /// first_gain)` — the first gain's full UTC datetime plus its
    /// seconds-of-day fallback — so gains arriving within 1 second of the
    /// first also attribute. After 1 second the use auto-completes. This
    /// covers motherlodes that yield 2+ items in the same swing without
    /// leaving the use open indefinitely.
    active_motherlode: Option<(u32, i64, Option<chrono::NaiveDateTime>, u32)>,

    /// The most-recent Basic survey use that just had its loot attributed,
    /// held only long enough to catch a `ProcessScreenText` "(speed bonus!)"
    /// marker that arrives immediately after the gains in the same tick.
    /// Cleared on the next non-ScreenText event.
    last_basic_use_for_bonus: Option<i64>,

    /// Gains with `SurveyMapUse` provenance that arrived before the
    /// survey map's `DeleteItem` created the pending use. Drained in
    /// `handle_survey_consumed` once the pending use exists. Tiny in
    /// practice (1-2 items per survey use, cleared every tick).
    deferred_basic_gains: Vec<DeferredBasicGain>,

    /// Recently-consumed Basic survey uses `(use_id, used_at UTC)`. Chat
    /// gains attribute to one of these only within
    /// [`CHAT_BASIC_WINDOW_SECS`] of its `used_at` (Basic loot lands in the
    /// same game tick as the map's consumption). Bounded FIFO.
    recent_basic_uses: VecDeque<(i64, chrono::NaiveDateTime)>,

    /// The survey node currently being mined, for chat-loot gating on
    /// Motherlode/Multihit surveys. See [`ChatMiningState`].
    chat_mining: Option<ChatMiningState>,

    /// Most recent `StartInteraction`: `(entity_id, name-if-any,
    /// secs-of-day, full UTC datetime)`. Binds the node identity to the next
    /// mining delay loop (the loop line itself doesn't carry the node's
    /// entity id), mirroring the parser's `pending_interaction` approach.
    /// The datetime makes the bind window midnight-safe (mirror the chat
    /// basic path); secs-of-day is the fallback when unparseable.
    last_interaction: Option<(u32, Option<String>, u32, Option<chrono::NaiveDateTime>)>,

    /// Node entities we've bound to a survey use `(entity_id, use_id)`, so
    /// a multihit node resumes attributing after the player mines something
    /// else in between. Bounded FIFO.
    known_survey_nodes: VecDeque<(u32, i64)>,

    /// Uses already claimed by a chat-side mining adoption. Prevents a
    /// later swing on a *regular* node from re-adopting a survey use whose
    /// node was already worked (the grace window alone can't tell them
    /// apart). Bounded FIFO.
    chat_adopted_uses: VecDeque<i64>,

    /// Cached active session id (DB is the source of truth, but we cache to
    /// avoid a query per event). `None` means "unknown — go check the DB".
    /// Refreshed when a session is started/ended.
    cached_active_session_id: Option<Option<i64>>,

    /// Dates for the Player.log `HH:MM:SS` timestamps the aggregator stamps:
    /// an explicit override (replay / old-log reparse), else the session date
    /// from the log's own login line, else the clock. See [`LogDateResolver`].
    log_dates: LogDateResolver,

    /// When false, the aggregator skips auto-creating sessions on crafting
    /// or first-use detection. The user must manually start sessions.
    /// Updated by the coordinator from `AppSettings.auto_start_survey_sessions`.
    pub auto_start_enabled: bool,
}

impl SurveySessionAggregator {
    pub fn new(game_data: GameDataState) -> Self {
        Self {
            game_data,
            pending_uses: VecDeque::new(),
            current_mining_node: None,
            active_motherlode: None,
            last_basic_use_for_bonus: None,
            deferred_basic_gains: Vec::new(),
            recent_basic_uses: VecDeque::new(),
            chat_mining: None,
            last_interaction: None,
            known_survey_nodes: VecDeque::new(),
            chat_adopted_uses: VecDeque::new(),
            cached_active_session_id: None,
            log_dates: LogDateResolver::default(),
            auto_start_enabled: true,
        }
    }

    /// Stamp Player.log times with an explicit UTC date instead of today's.
    /// Live tailing leaves this unset.
    pub fn set_base_date(&mut self, date: chrono::NaiveDate) {
        self.log_dates.set_base_date(date);
    }

    fn to_utc(&mut self, ts: &str) -> String {
        self.log_dates.resolve(ts)
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
        self.cached_active_session_id = Some(Some(id));
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
        persistence::recompute_session_bounds_and_end(conn, s.id, now_iso)
            .map_err(|e| e.to_string())?;
        self.cached_active_session_id = Some(None);
        Ok(Some(s.id))
    }

    /// Record a hand-framed survey-map use from the manual "Record use" UI.
    ///
    /// Same tail as `handle_survey_consumed` but driven by an explicit user
    /// action instead of a log line: `now_iso` is wall-clock time (not a log
    /// timestamp), and a session is always created when none is active (an
    /// explicit action ignores `auto_start_enabled`). Returns
    /// `(use_id, session_id)`.
    pub fn record_manual_use(
        &mut self,
        conn: &Connection,
        character: &str,
        server: &str,
        area: Option<&str>,
        map_internal_name: &str,
        kind: SurveyUseKind,
        now_iso: &str,
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
                self.cached_active_session_id = Some(Some(id));
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
        // Single write transaction: a failed count update rolls back the use
        // row instead of returning success with a stale session count.
        // No outer transaction is open here (fresh pool checkout in the
        // command), so new_unchecked is safe.
        let tx = rusqlite::Transaction::new_unchecked(
            conn,
            rusqlite::TransactionBehavior::Immediate,
        )
        .map_err(|e| e.to_string())?;

        let use_id = match persistence::insert_use(
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
            Ok(id) => id,
            Err(e) => {
                eprintln!("[survey-aggregator] insert_use failed: {e}");
                // A cached active-session id can point at a session that was
                // deleted out from under us (e.g. the user deleted it in the
                // History UI, which operates directly on the DB). Drop the
                // cache so the next call re-resolves instead of failing
                // forever.
                self.cached_active_session_id = None;
                return Err(format!(
                    "survey use insert failed (the session may have been deleted; try again): {e}"
                ));
            }
        };

        if let Some(s) = session_id {
            persistence::increment_consumed_count(&tx, s)
                .map_err(|e| format!("survey count update failed: {e}"))?;
        }
        tx.commit()
            .map_err(|e| format!("survey use transaction failed: {e}"))?;

        if let Some(s) = session_id {
            // Tighten started_at so the session header reflects the actual
            // first survey use, not the session-creation moment.
            let _ = persistence::tighten_started_at(conn, s, now_iso);
        }

        // Wall-clock ISO is "YYYY-MM-DD HH:MM:SS" — the pending-use queue
        // needs the time half for secs-of-day ordering.
        if let Some(now_secs) = now_iso.split(' ').nth(1).and_then(parse_secs_of_day) {
            self.pending_uses.push_back(PendingUse {
                survey_use_id: use_id,
                kind,
                used_at_secs: now_secs,
                used_at: parse_utc_datetime(now_iso),
            });

            // Drain deferred basic gains for THIS map only (same
            // single-loot-authority guard as the log path: a deferred gain
            // whose chat twin is already in the ledger is counted by the
            // claim step below, not here). Gains buffered for a different
            // map stay buffered for their own delete — draining them here
            // would misattribute another map's loot to this use.
            if kind == SurveyUseKind::Basic && !self.deferred_basic_gains.is_empty() {
                let deferred = std::mem::take(&mut self.deferred_basic_gains);
                let mut kept = Vec::with_capacity(deferred.len());
                let mut total_deferred_qty = 0u32;
                let mut saw_matching_gain = false;
                for gain in deferred {
                    // Match by display name or raw internal name: when the
                    // CDN can't resolve the map, display_name falls back to
                    // the internal spelling while the gain carries the log's
                    // display string. A gain with no map at all stays
                    // buffered (conservative — its delete drains blind).
                    let matches_map = gain.map_display_name.as_deref() == Some(display_name.as_str())
                        || gain.map_display_name.as_deref() == Some(map_internal_name);
                    if !matches_map {
                        kept.push(gain);
                        continue;
                    }
                    saw_matching_gain = true;
                    let has_chat_twin = match gain.resolved_at.as_deref() {
                        Some(at) => persistence::unclaimed_chat_gain_exists(
                            conn,
                            character,
                            server,
                            gain.qty,
                            at,
                            gain.item_name.as_deref(),
                            CHAT_BASIC_WINDOW_SECS as u32,
                        ),
                        None => false,
                    };
                    if has_chat_twin {
                        continue;
                    }
                    total_deferred_qty += gain.qty;
                    if let Some(at) = gain.resolved_at.as_deref() {
                        update_session_loot_timestamps(conn, use_id, at);
                    }
                }
                self.deferred_basic_gains = kept;
                // A matching gain with a chat twin skips the bump above but
                // its player.log row still needs the use_id linkage — the
                // claim step below only tags chat_status rows. Tag once when
                // any matching gain existed, twinned or not.
                if saw_matching_gain {
                    retroactively_tag_unlinked_survey_transactions(
                        conn, use_id, character, server, Some(display_name.as_str()),
                    );
                }
                if total_deferred_qty > 0 {
                    let _ = persistence::add_loot_qty(conn, use_id, total_deferred_qty);
                    let _ = persistence::set_use_status(conn, use_id, SurveyUseStatus::Completed);
                }
            }
        }

        // Chat-loot gating for Basic surveys: claim rows already in the
        // ledger within the same-tick window (the chat and player logs are
        // tailed independently, so either side can win the race).
        if kind == SurveyUseKind::Basic {
            if let Some(used_at) = parse_utc_datetime(now_iso) {
                remember_bounded(&mut self.recent_basic_uses, (use_id, used_at));
            }
            match persistence::claim_unlinked_chat_loot_near(
                conn,
                use_id,
                character,
                server,
                now_iso,
                CHAT_BASIC_WINDOW_SECS as u32,
            ) {
                Ok(0) => {}
                Ok(claimed_qty) => {
                    if let Err(e) = persistence::add_loot_qty(conn, use_id, claimed_qty) {
                        eprintln!("[survey-aggregator] claimed add_loot_qty failed: {e}");
                    } else {
                        let _ =
                            persistence::set_use_status(conn, use_id, SurveyUseStatus::Completed);
                    }
                    update_session_loot_timestamps(conn, use_id, now_iso);
                }
                Err(e) => {
                    eprintln!("[survey-aggregator] claim_unlinked_chat_loot_near failed: {e}");
                }
            }
        }

        Ok((use_id, session_id))
    }

    /// Attribute a Chat.log `[Status]` "added to inventory" loot gain to the
    /// survey use it was collected from, independent of the Player.log
    /// pipeline.
    ///
    /// This is the **chat-authoritative** loot path. Project Gorgon always
    /// writes survey loot to the Status chat channel, but only writes the
    /// detailed `ProcessAddItem` lines to Player.log under certain logging
    /// conditions — so the Player.log attribution in `process_event` can come
    /// up empty even while surveying is clearly producing loot (this is what
    /// Kaeus' GorgonSurveyTracker reads, and why its summary works where ours
    /// did not).
    ///
    /// Chat lines carry no node/map identity, so the gain is gated two ways
    /// (see docs/architecture/survey-mechanics.md):
    ///
    /// Per-kind **collection windows** (precise, attributes to the exact
    /// use):
    /// - **Basic**: loot lands in the same game tick as the map's
    ///   consumption → attribute within [`CHAT_BASIC_WINDOW_SECS`] of a
    ///   recently-consumed Basic use.
    /// - **Motherlode/Multihit**: loot lands when a mining swing on the
    ///   survey-spawned node completes → attribute inside the current
    ///   swing's window of the adopted node (see [`ChatMiningState`]).
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
    pub fn attribute_chat_gain(
        &mut self,
        conn: &Connection,
        character: &str,
        server: &str,
        internal_name: Option<&str>,
        quantity: u32,
        timestamp: &str,
    ) -> Option<i64> {
        // A survey map entering inventory is a craft, never survey loot —
        // its chat line is identical to a loot line, and players commonly
        // craft the next map within seconds of using one.
        if let Some(internal) = internal_name {
            if self.lookup_survey_kind(internal).is_some() {
                return None;
            }
        }
        let chat_ts = parse_utc_datetime(timestamp)?;

        // Basic window: same-tick loot around a recent Basic consumption
        // (newest first — back-to-back speed-bonus chains resolve to the
        // most recent map).
        let basic_hit = self
            .recent_basic_uses
            .iter()
            .rev()
            .find(|(_, used_at)| (chat_ts - *used_at).num_seconds().abs() <= CHAT_BASIC_WINDOW_SECS)
            .map(|(use_id, _)| *use_id);
        if let Some(use_id) = basic_hit {
            return self.record_chat_attribution(
                conn, character, server, use_id, quantity, timestamp, internal_name, true,
            );
        }

        // Mining-swing window: loot from the adopted survey node lands at
        // swing completion, inside [swing_start, swing_start + window].
        if let Some(cm) = &self.chat_mining {
            let dt = (chat_ts - cm.swing_start).num_seconds();
            if (0..=cm.swing_window_secs).contains(&dt) {
                let use_id = cm.survey_use_id;
                return self.record_chat_attribution(
                    conn, character, server, use_id, quantity, timestamp, internal_name, false,
                );
            }
        }

        // Identity fallback: survey-yield items count while a session is
        // active, attributed to its most recent use.
        if internal_name.is_some_and(|internal| self.is_survey_loot_item(internal)) {
            if let Some(session_id) = self.fetch_active_session(conn, character, server) {
                if let Ok(Some(use_id)) = persistence::latest_use_id_for_session(conn, session_id) {
                    return self.record_chat_attribution(
                        conn, character, server, use_id, quantity, timestamp, internal_name, false,
                    );
                }
            }
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
        character: &str,
        server: &str,
        use_id: i64,
        quantity: u32,
        timestamp: &str,
        item_name: Option<&str>,
        mark_completed: bool,
    ) -> Option<i64> {
        // Single loot authority for `loot_qty`: a gain seen by both pipelines
        // bumps once. The Player.log side usually lands first, so by the time
        // a chat gain attributes, its twin row (tagged, or untagged when
        // deferred) is already in the ledger — when found, the bump is
        // skipped but timestamps/status and the returned `use_id` (which tags
        // the chat row) are unchanged. Per-item rows stay in both pipelines;
        // the summary's `NOT EXISTS` read guard dedups those.
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
        if !already_counted {
            if let Err(e) = persistence::add_loot_qty(conn, use_id, quantity) {
                eprintln!("[survey-aggregator] chat add_loot_qty failed: {e}");
                return None;
            }
        }
        update_session_loot_timestamps(conn, use_id, timestamp);
        if mark_completed {
            let _ = persistence::set_use_status(conn, use_id, SurveyUseStatus::Completed);
        }
        Some(use_id)
    }

    // ============================================================
    // Event ingestion
    // ============================================================

    /// Process one `PlayerEvent` for the active character. The event's
    /// provenance may be mutated in place to inject `survey_use_id` (A3
    /// stitching) before downstream consumers see it.
    pub fn process_event(
        &mut self,
        event: &mut PlayerEvent,
        conn: &Connection,
        character: &str,
        server: &str,
        current_area: Option<&str>,
    ) -> Vec<SurveyAggregatorEvent> {
        let mut emitted = Vec::new();

        // The log's own login line re-anchors the date for everything after it
        // (see `LogDateResolver`); it is not itself a survey event.
        if let PlayerEvent::LogSessionAnchored { utc } = event {
            self.log_dates.anchor_session(*utc);
            return emitted;
        }

        // UTC timestamp of this event (log-derived, not wall clock). Computed
        // once here and reused by the expiry + crafting auto-end sweep below.
        let event_iso = event_hms_timestamp(event).map(|hms| self.to_utc(hms));

        // Age out pending-use entries whose 60s grace window has expired.
        // Full-datetime comparison so the window survives midnight (mirror
        // the chat basic path); falls back to secs-of-day when unparseable.
        if let Some(now_secs) = event_secs_of_day(event) {
            let now_dt = event_iso.as_deref().and_then(parse_utc_datetime);
            self.expire_pending_uses(now_secs, now_dt);
        }

        // Session length cap: if the active session has run past the max
        // duration, roll it over now — *before* the event is handled, so a
        // triggering survey use lands in the fresh session rather than the one
        // being closed. Only in the auto-start regime; a manually-managed
        // session is the user's to end. See [`SESSION_MAX_DURATION_SECS`].
        if self.auto_start_enabled {
            if let Some(ref iso) = event_iso {
                if let Some(ev) = self.maybe_roll_over_session(conn, character, server, iso) {
                    emitted.push(ev);
                }
            }
        }
        // Speed-bonus pointer lifetime: the "(speed bonus!)" ScreenText
        // arrives in the same tick as the gains. Any other event means the
        // marker isn't coming, so drop the pointer now (per the documented
        // contract on `last_basic_use_for_bonus`). The marker arm below
        // `take()`s it, so this pre-clear never fires for the marker itself.
        let is_speed_bonus_marker = matches!(
            event,
            PlayerEvent::ScreenText { message, .. } if message.contains("(speed bonus!)")
        );
        if !is_speed_bonus_marker {
            self.last_basic_use_for_bonus = None;
        }

        match event {
            // Survey map crafted (item entered inventory, kind is a survey).
            // Detected on ItemAdded with is_new=true AND SurveyMapCraft
            // provenance (Surveying-skill craft). Loot/map moves carrying
            // other sources and Uncertain/UnknownSource gains (e.g.
            // session-load reconciliation) must not inflate crafted_count —
            // the auto-end check compares it against consumed_count.
            PlayerEvent::ItemAdded {
                item_name,
                is_new,
                timestamp,
                provenance,
                ..
            } if *is_new => {
                let is_craft = matches!(
                    provenance,
                    ItemProvenance::Attributed {
                        source: ActivitySource::SurveyMapCraft,
                        ..
                    }
                );
                if is_craft {
                    if let Some(kind) = self.lookup_survey_kind(item_name) {
                        self.handle_survey_crafted(
                            conn,
                            character,
                            server,
                            item_name,
                            kind,
                            timestamp,
                            &mut emitted,
                        );
                    }
                }
            }

            // Survey map consumed → record use, push pending attribution.
            PlayerEvent::ItemDeleted {
                item_name: Some(name),
                timestamp,
                ..
            } => {
                if let Some(kind) = self.lookup_survey_kind(name) {
                    self.handle_survey_consumed(
                        conn,
                        character,
                        server,
                        current_area,
                        name,
                        kind,
                        timestamp,
                        &mut emitted,
                    );
                }
            }

            // Mining started → bind the swing to a survey use (or refresh /
            // close the current engagement) for chat-loot gating.
            PlayerEvent::DelayLoopStarted {
                action_type,
                label,
                timestamp,
                duration,
                ..
            } if is_mining_loop(action_type, label) => {
                let (timestamp, duration) = (timestamp.clone(), *duration);
                self.handle_mining_started(conn, character, server, &timestamp, duration);
            }

            // Remember the interacted entity so the next mining delay loop
            // can be bound to its node (the loop line itself only carries
            // the player's entity id). A foreign interaction (corpse search,
            // container, NPC) while a survey swing's loot window is open
            // also clips that window — chat gains from that point on belong
            // to the new interaction, not the swing (observed: corpse loot
            // collected seconds after a swing completes).
            PlayerEvent::InteractionStarted {
                timestamp,
                entity_id,
                npc_name,
                ..
            } => {
                let name = if npc_name.is_empty() {
                    None
                } else {
                    Some(npc_name.clone())
                };
                let (entity_id, timestamp) = (*entity_id, timestamp.clone());
                let iso = self.to_utc(&timestamp);
                let at = parse_utc_datetime(&iso);
                self.last_interaction =
                    parse_secs_of_day(&timestamp).map(|secs| (entity_id, name, secs, at));
                self.clip_chat_window_on_foreign_interaction(entity_id, &timestamp);
            }

            // Speed-bonus marker. Arrives *after* the bonus gains have been
            // recorded (see aggregator's event ordering notes). We patch the
            // already-written item_transactions rows for this Basic survey
            // use by item name. Only Basic surveys emit this marker; once
            // applied we clear the pointer so a later unrelated ScreenText
            // can't re-mark old rows.
            PlayerEvent::ScreenText { message, .. } if message.contains("(speed bonus!)") => {
                if let Some(use_id) = self.last_basic_use_for_bonus.take() {
                    self.apply_speed_bonus_marker(conn, use_id, message);
                }
            }

            _ => {}
        }

        // A3 stitching: for any *gain* event with a Mining provenance and a
        // currently-mining node we know about, inject survey_use_id from the
        // open_multihit_nodes table or the most recent pending-use that's
        // already been adopted. This runs after the matches above so newly-
        // adopted nodes are visible.
        self.maybe_inject_survey_use_id(event, conn, character, server, &mut emitted);

        // Run sweeps periodically — once per event is fine; the queries are
        // small and indexed. The multihit sweep needs the event's full UTC
        // timestamp (not just seconds-of-day) so the 30-min cutoff is
        // date-aware across midnight; without an event timestamp there is
        // nothing date-aware to compare, so skip the sweep.
        if let (Some(now_secs), Some(now_iso)) = (event_secs_of_day(event), &event_iso) {
            self.run_multihit_sweep(conn, character, server, now_secs, now_iso, &mut emitted);
            self.maybe_auto_end_crafting_session(
                conn,
                character,
                server,
                event_iso.as_deref(),
                &mut emitted,
            );
        }

        emitted
    }

    /// Bump the in-memory current-mining-entity tracking when a non-mining
    /// interaction starts. The coordinator can call this from
    /// `InteractionStarted` events. Mining state must clear when the player
    /// switches to a non-mining interaction (e.g., corpse search) so the
    /// next mining cycle is correctly recognized as a "different entity".
    #[allow(dead_code)] // Reserved hook for future mining-state clearing
    pub fn note_interaction_started(&mut self, _entity_id: u32) {
        // Currently a no-op: `current_mining_node` is set only when a mining
        // delay loop fires, and a different-entity mining cycle naturally
        // closes the previous one. Reserved as a hook in case we later
        // find we need to clear mining state earlier (e.g., on a confirmed
        // combat interruption).
    }

    // ============================================================
    // Internal handlers
    // ============================================================

    /// Called when a survey map enters inventory (crafted). Starts a
    /// `Crafting`-trigger session if none is active, otherwise increments
    /// the existing session's crafted_count.
    fn handle_survey_crafted(
        &mut self,
        conn: &Connection,
        character: &str,
        server: &str,
        _map_internal_name: &str,
        _kind: SurveyUseKind,
        timestamp: &str,
        emitted: &mut Vec<SurveyAggregatorEvent>,
    ) {
        let now_iso = self.to_utc(timestamp);

        let active = match self.fetch_active_session(conn, character, server) {
            Some(id) => id,
            None => {
                if !self.auto_start_enabled {
                    // Auto-start disabled and no active session — skip.
                    // The craft is observed but no session tracks it.
                    return;
                }
                // No active session → start one. Crafting trigger means
                // auto-end will fire once the player consumes everything
                // they crafted.
                let id = match persistence::insert_session(
                    conn,
                    character,
                    server,
                    &now_iso,
                    SessionStartTrigger::Crafting,
                    Some(0),
                ) {
                    Ok(id) => id,
                    Err(e) => {
                        eprintln!("[survey-aggregator] insert_session failed: {e}");
                        return;
                    }
                };
                self.cached_active_session_id = Some(Some(id));
                emitted.push(SurveyAggregatorEvent::SessionStarted {
                    session_id: id,
                    trigger: SessionStartTrigger::Crafting,
                });
                id
            }
        };
        if let Err(e) = persistence::increment_crafted_count(conn, active) {
            eprintln!("[survey-aggregator] increment_crafted_count failed: {e}");
        }
        // Track craft timestamps on the session and tighten started_at so
        // the session header reflects the actual first activity, not the
        // wall-clock (or trigger-event) moment the session was created.
        let _ = persistence::update_first_craft_at(conn, active, &now_iso);
        let _ = persistence::update_last_craft_at(conn, active, &now_iso);
        let _ = persistence::tighten_started_at(conn, active, &now_iso);
    }

    /// Called when a survey map is consumed (deleted). Inserts a
    /// `survey_uses` row, increments session counts, and queues the
    /// `pending_uses` entry for upcoming Mining-context attribution.
    #[allow(clippy::too_many_arguments)]
    fn handle_survey_consumed(
        &mut self,
        conn: &Connection,
        character: &str,
        server: &str,
        current_area: Option<&str>,
        map_internal_name: &str,
        kind: SurveyUseKind,
        timestamp: &str,
        emitted: &mut Vec<SurveyAggregatorEvent>,
    ) {
        let now_iso = self.to_utc(timestamp);

        // Auto-start a session if none active. Trigger = FirstUse since the
        // player didn't manually start and we didn't see a craft.
        let session_id = match self.fetch_active_session(conn, character, server) {
            Some(id) => Some(id),
            None => {
                if !self.auto_start_enabled {
                    // No session + auto-start off → record the use with
                    // no session (session_id = None).
                    None
                } else {
                    let id = match persistence::insert_session(
                        conn,
                        character,
                        server,
                        &now_iso,
                        SessionStartTrigger::FirstUse,
                        None,
                    ) {
                        Ok(id) => id,
                        Err(e) => {
                            eprintln!("[survey-aggregator] insert_session failed: {e}");
                            return;
                        }
                    };
                    self.cached_active_session_id = Some(Some(id));
                    emitted.push(SurveyAggregatorEvent::SessionStarted {
                        session_id: id,
                        trigger: SessionStartTrigger::FirstUse,
                    });
                    Some(id)
                }
            }
        };

        // Resolve display name for this map via CDN.
        let display_name = self
            .game_data
            .try_read()
            .ok()
            .and_then(|gd| gd.resolve_item(map_internal_name).map(|i| i.name.clone()))
            .unwrap_or_else(|| map_internal_name.to_string());

        // Resolve area: prefer the live-tracked area from the coordinator,
        // fall back to the survey_types.zone column (populated from CDN
        // description + areas lookup at import time).
        let resolved_area: Option<String> = current_area.map(|s| s.to_string()).or_else(|| {
            conn.query_row(
                "SELECT zone FROM survey_types WHERE internal_name = ?1",
                rusqlite::params![map_internal_name],
                |r| r.get(0),
            )
            .ok()
            .flatten()
        });

        let use_id = match persistence::insert_use(
            conn,
            session_id,
            character,
            server,
            &now_iso,
            map_internal_name,
            &display_name,
            kind,
            resolved_area.as_deref(),
        ) {
            Ok(id) => id,
            Err(e) => {
                eprintln!("[survey-aggregator] insert_use failed: {e}");
                // A cached active-session id can point at a session that was
                // deleted out from under us (e.g. the user deleted it in the
                // History UI, which operates directly on the DB). That makes
                // the survey_uses → survey_sessions FK fail on every use.
                // Drop the cache so the next use re-resolves and auto-starts a
                // fresh session instead of failing forever.
                self.cached_active_session_id = None;
                return;
            }
        };

        if let Some(s) = session_id {
            if let Err(e) = persistence::increment_consumed_count(conn, s) {
                eprintln!("[survey-aggregator] increment_consumed_count failed: {e}");
            }
            // Tighten started_at so the session header reflects the actual
            // first survey use, not the session-creation moment.
            let _ = persistence::tighten_started_at(conn, s, &now_iso);
        }

        // For Basic surveys the loot drops in the same tick — no Mining
        // context follows. The use can immediately move to the pending-use
        // queue (so any same-tick gains attribute) and the aggregator will
        // close it via the regular sweep once its window passes.
        // For Motherlode/Multihit, the entry sits in pending_uses waiting
        // for the next Mining context to open within 60s.
        if let Some(now_secs) = parse_secs_of_day(timestamp) {
            self.pending_uses.push_back(PendingUse {
                survey_use_id: use_id,
                kind,
                used_at_secs: now_secs,
                used_at: parse_utc_datetime(&now_iso),
            });

            // Drain any deferred basic gains that arrived before this
            // DeleteItem. In the game's log order, primary loot fires
            // before the map is consumed:
            //   1. ProcessAddItem (primary)      ← buffered
            //   2. ProcessDeleteItem (this line)  ← pending use created
            //   3. ProcessAddItem (bonus)         ← attributed normally
            if kind == SurveyUseKind::Basic && !self.deferred_basic_gains.is_empty() {
                let deferred = std::mem::take(&mut self.deferred_basic_gains);
                // Single loot authority: a deferred gain whose chat twin is
                // already in the ledger (untagged row inside the basic
                // window) is counted by the `claim_unlinked_chat_loot_near`
                // step below, not here. Only gains with no chat twin bump
                // here.
                let mut total_deferred_qty = 0u32;
                for gain in &deferred {
                    if gain.resolved_at.as_deref().is_some_and(|at| {
                        persistence::unclaimed_chat_gain_exists(
                            conn,
                            character,
                            server,
                            gain.qty,
                            at,
                            gain.item_name.as_deref(),
                            CHAT_BASIC_WINDOW_SECS as u32,
                        )
                    }) {
                        continue;
                    }
                    total_deferred_qty += gain.qty;
                    // Already resolved, in order, when the gain line was seen —
                    // re-resolving here would use an earlier time of day than the
                    // consuming map's and roll the session date forward.
                    if let Some(ref at) = gain.resolved_at {
                        update_session_loot_timestamps(conn, use_id, at);
                    }
                }
                if total_deferred_qty > 0 {
                    let _ = persistence::add_loot_qty(conn, use_id, total_deferred_qty);
                }
                // Retroactively patch any item_transactions rows that
                // were already written by GameStateManager (or
                // persist_for_test in tests) with source_kind =
                // 'survey_map_use' but no survey_use_id. These are the
                // deferred gains whose provenance couldn't carry the
                // use_id at write time.
                retroactively_tag_unlinked_survey_transactions(conn, use_id, character, server, None);
                // Don't mark completed yet — more gains (speed bonus) may
                // arrive in the same tick. The next call to
                // attribute_basic_gain (for the bonus item) or the
                // pending-use expiry sweep will close it.
            }
        }

        // Chat-loot gating for Basic surveys: remember the use so chat
        // gains within the same-tick window attribute to it, and claim any
        // chat rows already written before this DeleteItem was processed
        // (the two logs are tailed independently, so either side can win
        // the race).
        if kind == SurveyUseKind::Basic {
            if let Some(used_at) = parse_utc_datetime(&now_iso) {
                remember_bounded(&mut self.recent_basic_uses, (use_id, used_at));
            }
            match persistence::claim_unlinked_chat_loot_near(
                conn,
                use_id,
                character,
                server,
                &now_iso,
                CHAT_BASIC_WINDOW_SECS as u32,
            ) {
                Ok(0) => {}
                Ok(claimed_qty) => {
                    if let Err(e) = persistence::add_loot_qty(conn, use_id, claimed_qty) {
                        eprintln!("[survey-aggregator] claimed add_loot_qty failed: {e}");
                    }
                    update_session_loot_timestamps(conn, use_id, &now_iso);
                }
                Err(e) => {
                    eprintln!("[survey-aggregator] claim_unlinked_chat_loot_near failed: {e}");
                }
            }
        }

        emitted.push(SurveyAggregatorEvent::UseRecorded {
            use_id,
            session_id,
            map_internal_name: map_internal_name.to_string(),
            kind,
        });
    }

    /// Called on each Mining delay-loop event. Drives the **chat-side**
    /// survey-node engagement ([`ChatMiningState`]) that gates chat-loot
    /// attribution for Motherlode/Multihit surveys. (The Player.log gain
    /// path binds nodes separately, via gain provenance in
    /// `maybe_inject_survey_use_id` — unchanged.)
    ///
    /// The node's entity id comes from the most recent `StartInteraction`
    /// (it fires immediately before the loop; the loop's own entity_id is
    /// the player). Rules, per docs/architecture/survey-mechanics.md:
    /// - A swing on the adopted node (or one we can't disprove) refreshes
    ///   the loot window.
    /// - A swing on a provably different node closes the engagement — the
    ///   survey node is done or abandoned.
    /// - A new engagement opens when the node was previously bound to a
    ///   survey use, when a Motherlode/Multihit map was consumed within the
    ///   walk-to-node grace window, or when the node's interaction name
    ///   marks it survey-spawned ("…FromSurvey…").
    fn handle_mining_started(
        &mut self,
        conn: &Connection,
        character: &str,
        server: &str,
        timestamp: &str,
        duration: f32,
    ) {
        let Some(now_secs) = parse_secs_of_day(timestamp) else {
            return;
        };
        let now_iso = self.to_utc(timestamp);
        let Some(now_dt) = parse_utc_datetime(&now_iso) else {
            return;
        };
        let window_secs = duration.ceil().max(0.0) as i64 + CHAT_SWING_SLACK_SECS;

        // Full-datetime comparison so the bind window survives midnight
        // (mirror the chat basic path); secs-of-day is the fallback.
        let node = self
            .last_interaction
            .as_ref()
            .filter(|(_, _, secs, at)| {
                within_grace_window(Some(now_dt), *at, now_secs, *secs, INTERACTION_TO_LOOP_MAX_SECS)
            })
            .map(|(entity, name, _, _)| (*entity, name.clone()));
        let node_entity = node.as_ref().map(|(entity, _)| *entity);

        // Active engagement: refresh or close.
        if let Some(cm) = &mut self.chat_mining {
            let same_node = match (node_entity, cm.node_entity) {
                (Some(a), Some(b)) => a == b,
                // Identity unknown on either side — can't disprove; keep
                // attributing rather than dropping real survey loot.
                _ => true,
            };
            let fresh = (now_dt - cm.swing_start).num_seconds() <= MULTIHIT_TIMEOUT_SECS as i64;
            if same_node && fresh {
                cm.swing_start = now_dt;
                cm.swing_window_secs = window_secs;
                if cm.node_entity.is_none() {
                    cm.node_entity = node_entity;
                    if let Some(entity) = node_entity {
                        let use_id = cm.survey_use_id;
                        remember_bounded(&mut self.known_survey_nodes, (entity, use_id));
                    }
                }
                return;
            }
            let closed_use = cm.survey_use_id;
            self.chat_mining = None;
            let _ = persistence::set_use_status(conn, closed_use, SurveyUseStatus::Completed);
        }

        // (Re-)adoption for this swing, in order of confidence:
        // 1. A node already bound to a survey use — a multihit node resumed
        //    after the player mined something else in between.
        if let Some(entity) = node_entity {
            if let Some(use_id) = self
                .known_survey_nodes
                .iter()
                .find(|(known, _)| *known == entity)
                .map(|(_, use_id)| *use_id)
            {
                self.chat_mining = Some(ChatMiningState {
                    survey_use_id: use_id,
                    node_entity: Some(entity),
                    swing_start: now_dt,
                    swing_window_secs: window_secs,
                });
                return;
            }
        }

        // 2. A Motherlode/Multihit map consumed within the grace window
        //    whose loot hasn't already been claimed by an earlier
        //    engagement (a later swing on a *regular* node must not
        //    re-adopt an already-worked use). Full-datetime comparison so
        //    the window survives midnight (mirror the chat basic path).
        let pending_use = self
            .pending_uses
            .iter()
            .find(|p| {
                p.kind != SurveyUseKind::Basic
                    && within_grace_window(
                        Some(now_dt),
                        p.used_at,
                        now_secs,
                        p.used_at_secs,
                        SURVEY_TO_MINING_GRACE_SECS,
                    )
                    && !self.chat_adopted_uses.contains(&p.survey_use_id)
            })
            .map(|p| p.survey_use_id);
        if let Some(use_id) = pending_use {
            self.adopt_for_chat(use_id, node_entity, now_dt, window_secs);
            return;
        }

        // 3. The interaction name marks the node survey-spawned
        //    ("MiningNodeFromSurvey9", "GeologyNodeFromSurveyBlue", …) but
        //    the map use fell outside the grace window (long walk). Bind to
        //    the most recent Motherlode/Multihit use.
        if let Some((_, Some(name))) = &node {
            if name.contains("FromSurvey") {
                if let Ok(Some((use_id, _))) = persistence::latest_recent_nonbasic_use(
                    conn,
                    character,
                    server,
                    &now_iso,
                    MULTIHIT_TIMEOUT_SECS,
                ) {
                    self.adopt_for_chat(use_id, node_entity, now_dt, window_secs);
                }
            }
        }
    }

    /// Clip the current swing's loot window when the player starts
    /// interacting with something other than the adopted survey node.
    /// Swing loot lands exactly at the swing's completion; the window's
    /// trailing slack exists only for log-write skew — once a corpse search
    /// or other interaction begins, any further "added to inventory" lines
    /// are that interaction's loot, not the swing's. The next swing on the
    /// node re-opens a full window.
    fn clip_chat_window_on_foreign_interaction(&mut self, entity_id: u32, timestamp: &str) {
        let node_entity = match &self.chat_mining {
            Some(cm) => cm.node_entity,
            None => return,
        };
        // Unknown node identity — can't tell a foreign interaction from the
        // node's own; leave the window alone.
        let Some(node) = node_entity else { return };
        if node == entity_id {
            return;
        }
        let iso = self.to_utc(timestamp);
        let Some(at) = parse_utc_datetime(&iso) else {
            return;
        };
        if let Some(cm) = &mut self.chat_mining {
            // Gains sharing the interaction's second are ambiguous; treat
            // them as the interaction's (strictly-before survives).
            let clipped = (at - cm.swing_start).num_seconds() - 1;
            if clipped < cm.swing_window_secs {
                cm.swing_window_secs = clipped;
            }
        }
    }

    /// Open a chat-side mining engagement on `use_id` and remember the
    /// binding so the use can't be re-adopted by an unrelated node and the
    /// node can resume the use later.
    fn adopt_for_chat(
        &mut self,
        use_id: i64,
        node_entity: Option<u32>,
        swing_start: chrono::NaiveDateTime,
        swing_window_secs: i64,
    ) {
        self.chat_mining = Some(ChatMiningState {
            survey_use_id: use_id,
            node_entity,
            swing_start,
            swing_window_secs,
        });
        remember_bounded(&mut self.chat_adopted_uses, use_id);
        if let Some(entity) = node_entity {
            remember_bounded(&mut self.known_survey_nodes, (entity, use_id));
        }
    }
    #[cfg(test)]
    pub(crate) fn chat_mining_for_test(&self) -> Option<(u32, i64)> {
        self.chat_mining
            .as_ref()
            .and_then(|cm| cm.node_entity.map(|entity| (entity, cm.survey_use_id)))
    }

    /// A3 stitching. Called for every event after the per-event handlers.
    /// If this event is a *gain* attributed to Mining, decide whether the
    /// gain belongs to a survey use:
    /// - If it does (matching open multihit node OR pending use within 60s),
    ///   inject survey_use_id into its provenance and bump per-use loot_qty.
    /// - Open a multihit node row if the use kind is Multihit and one
    ///   doesn't exist yet.
    /// - Mark Motherlode/Basic uses completed once they receive their loot.
    fn maybe_inject_survey_use_id(
        &mut self,
        event: &mut PlayerEvent,
        conn: &Connection,
        character: &str,
        server: &str,
        emitted: &mut Vec<SurveyAggregatorEvent>,
    ) {
        // Read-only pass first: extract qty, mining node id, and timestamps
        // before taking any mutable borrow. The mutable borrow on provenance
        // happens at the very end, in a single `inject_survey_use_id_into`
        // call after all DB writes have completed. This keeps the borrow
        // checker happy without requiring extra clones of the provenance
        // value (which is meaningfully sized — has source, candidates, etc.).
        let (qty, item_name_owned, mining_node_id_opt, is_basic_survey_gain, gain_map_name) =
            match event {
            PlayerEvent::ItemAdded {
                provenance,
                initial_quantity,
                item_name,
                ..
            } => {
                let kind_info = read_provenance_kind(provenance);
                (
                    *initial_quantity,
                    Some(item_name.clone()),
                    kind_info.0,
                    kind_info.1,
                    kind_info.2,
                )
            }
            PlayerEvent::ItemStackChanged {
                provenance,
                delta,
                item_name,
                ..
            } if *delta > 0 => {
                let kind_info = read_provenance_kind(provenance);
                (*delta as u32, item_name.clone(), kind_info.0, kind_info.1, kind_info.2)
            }
            _ => return,
        };
        // Borrowed tie-break for the single-authority guards below; the
        // owned clone above ends the `event` borrow before the mutable
        // provenance inject at the end of each arm.
        let item_name_opt = item_name_owned.as_deref();

        // Basic survey direct-loot case (no Mining intermediary).
        if is_basic_survey_gain {
            self.attribute_basic_gain(event, conn, qty, gain_map_name);
            return;
        }

        // Mining-attributed gains: need the node entity id to either match
        // an open multihit row or adopt a pending use.
        let Some(node_id) = mining_node_id_opt else {
            return;
        };
        let node_id_i64 = node_id as i64;

        // Read timestamps up front so we don't need event after taking the
        // mutable provenance borrow below.
        let now_iso = event_hms_timestamp(event).map(|hms| self.to_utc(hms));
        let now_secs = event_secs_of_day(event);

        // Case 0: active Motherlode — the node was already adopted from a
        // pending use on the first gain. Attribute additional gains from
        // the same node within 1 second of the first gain (covers
        // motherlodes that yield 2+ items in one swing). Full-datetime
        // comparison so the window survives midnight.
        if let Some((ml_node, ml_use_id, ml_first_dt, ml_first_secs)) = self.active_motherlode {
            let now_dt = now_iso.as_deref().and_then(parse_utc_datetime);
            let within_window = match (now_dt, ml_first_dt) {
                (Some(now), Some(first)) => (now - first).num_seconds() <= 1,
                _ => now_secs
                    .map(|s| s.saturating_sub(ml_first_secs) <= 1)
                    .unwrap_or(false),
            };
            if ml_node == node_id && within_window {
                // Single loot authority: skip the bump when the chat pipeline
                // already counted this gain (tagged `survey_chat` twin). The
                // name tie-breaks same-tick multi-item loot sharing qty/time.
                if !persistence::chat_gain_already_counted(
                    conn,
                    ml_use_id,
                    qty,
                    now_iso.as_deref(),
                    item_name_opt,
                    CHAT_SWING_SLACK_SECS as u32,
                ) {
                    let _ = persistence::add_loot_qty(conn, ml_use_id, qty);
                }
                if let Some(ts) = &now_iso {
                    update_session_loot_timestamps(conn, ml_use_id, ts);
                }
                apply_survey_use_id(event, ml_use_id);
                return;
            }
            // Window expired or different node — close the motherlode.
            let _ = persistence::set_use_status(conn, ml_use_id, SurveyUseStatus::Completed);
            self.active_motherlode = None;
        }

        // Case 1: existing open multihit node — touch, attribute, inject.
        if let Ok(Some(n)) = multihit_state::get_node(conn, character, server, node_id_i64) {
            if let Some(iso) = &now_iso {
                let _ = multihit_state::touch_node(conn, character, server, node_id_i64, iso);
            }
            // Single loot authority: skip the bump when the chat pipeline
            // already counted this gain (tagged `survey_chat` twin). The
            // name tie-breaks same-tick multi-item loot sharing qty/time.
            if !persistence::chat_gain_already_counted(
                conn,
                n.survey_use_id,
                qty,
                now_iso.as_deref(),
                item_name_opt,
                CHAT_SWING_SLACK_SECS as u32,
            ) {
                let _ = persistence::add_loot_qty(conn, n.survey_use_id, qty);
            }
            if let Some(ts) = &now_iso {
                update_session_loot_timestamps(conn, n.survey_use_id, ts);
            }
            apply_survey_use_id(event, n.survey_use_id);
            return;
        }

        // Case 2: this is a new mining-node + we have a pending use within
        // grace window. Pop it and attach. Full-datetime comparison so the
        // window survives midnight (mirror the chat basic path).
        let Some(now_secs) = now_secs else { return };
        let now_dt = now_iso.as_deref().and_then(parse_utc_datetime);
        let pending_idx = self.pending_uses.iter().position(|p| {
            within_grace_window(now_dt, p.used_at, now_secs, p.used_at_secs, SURVEY_TO_MINING_GRACE_SECS)
        });
        let Some(idx) = pending_idx else { return };
        let pending = self.pending_uses.remove(idx).unwrap();
        let mut opened_multihit: Option<(i64, u32)> = None;

        match pending.kind {
            SurveyUseKind::Multihit => {
                if let Some(iso) = &now_iso {
                    let _ = multihit_state::open_node(
                        conn,
                        character,
                        server,
                        node_id_i64,
                        pending.survey_use_id,
                        iso,
                    );
                }
                self.current_mining_node = Some(node_id);
                opened_multihit = Some((pending.survey_use_id, node_id));
            }
            SurveyUseKind::Motherlode => {
                self.current_mining_node = Some(node_id);
                self.active_motherlode =
                    Some((node_id, pending.survey_use_id, now_dt, now_secs));
            }
            SurveyUseKind::Basic => {
                // Basic shouldn't reach this path (no Mining context follows),
                // but defensively still attach so loot isn't dropped.
            }
        }
        // Single loot authority: skip the bump when the chat pipeline already
        // counted this gain (tagged `survey_chat` twin). The name tie-breaks
        // same-tick multi-item loot sharing qty/time.
        if !persistence::chat_gain_already_counted(
            conn,
            pending.survey_use_id,
            qty,
            now_iso.as_deref(),
            item_name_opt,
            CHAT_SWING_SLACK_SECS as u32,
        ) {
            let _ = persistence::add_loot_qty(conn, pending.survey_use_id, qty);
        }
        if let Some(ts) = &now_iso {
            update_session_loot_timestamps(conn, pending.survey_use_id, ts);
        }
        apply_survey_use_id(event, pending.survey_use_id);

        // The sweep already emits MultihitNodeClosed when the node expires;
        // emit the matching Opened here so the coordinator's arm (already
        // wired to refresh the tracker) fires and the status row appears
        // live. `node_id` is the `u32` mining entity from Case 2 above.
        if let Some((open_use_id, open_node_id)) = opened_multihit {
            emitted.push(SurveyAggregatorEvent::MultihitNodeOpened {
                use_id: open_use_id,
                node_entity_id: open_node_id,
            });
        }
        // Motherlode completion is deferred — handled by Case 0 when a
        // different node starts, or by handle_mining_started / grace-window
        // expiry. This lets multi-item Motherlode loot (two UpdateItemCode
        // lines in the same mining cycle) attribute to the same use.
    }

    /// Parse a "X collected! Also found Y, Z (speed bonus!)" ScreenText line
    /// and flag each bonus item's transaction row for `use_id` in
    /// `item_transactions.source_details`. The bonus items' display names
    /// match the names already recorded on the transactions (both come from
    /// CDN-resolved display names), so the `(use_id, item_name)` match is
    /// exact.
    fn apply_speed_bonus_marker(&self, conn: &Connection, use_id: i64, message: &str) {
        let (items, _earned) = crate::parsers::parse_loot_items(message);
        for item in items.iter().filter(|i| i.is_speed_bonus) {
            if let Err(e) =
                persistence::mark_transactions_as_speed_bonus(conn, use_id, &item.item_name)
            {
                eprintln!(
                    "[survey-aggregator] mark_transactions_as_speed_bonus failed for use={} item={}: {}",
                    use_id, item.item_name, e
                );
            }
        }
    }

    /// Attribute a `SurveyMapUse`-provenance gain to the most recent pending
    /// Basic survey use. Called when gains arrive with SurveyMapUse provenance.
    ///
    /// In the game's log, basic survey loot often arrives *before* the survey
    /// map deletion that creates the pending use:
    ///   1. ProcessAddItem (primary loot)      ← gain arrives here
    ///   2. ProcessDeleteItem (map consumed)    ← pending use created here
    ///   3. ProcessAddItem (speed-bonus loot)
    ///
    /// When no pending Basic use exists yet, the gain is buffered in
    /// `deferred_basic_gains` and replayed when `handle_survey_consumed`
    /// creates the pending use.
    fn attribute_basic_gain(
        &mut self,
        event: &mut PlayerEvent,
        conn: &Connection,
        qty: u32,
        map_display_name: Option<String>,
    ) {
        // Resolve this gain's own stamp while it *is* the newest line seen; the
        // deferred drain then never has to re-resolve an earlier time of day.
        let resolved_at = event_hms_timestamp(event).map(|hms| self.to_utc(hms));
        // Log-internal name for the single-authority tie-break (same-tick
        // multi-item loot shares qty/time; the name keeps a chat twin of A
        // from suppressing the bump for B).
        let item_name: Option<String> = match event {
            PlayerEvent::ItemAdded { item_name, .. } => Some(item_name.clone()),
            PlayerEvent::ItemStackChanged { item_name, .. } => item_name.clone(),
            _ => None,
        };

        // Find the most-recent (back of the queue) Basic pending use.
        let Some(idx) = self
            .pending_uses
            .iter()
            .rposition(|p| p.kind == SurveyUseKind::Basic)
        else {
            // No pending Basic use yet — the DeleteItem hasn't fired.
            // Buffer this gain so handle_survey_consumed can attribute it.
            // Bounded: if the delete never comes the buffer can't grow
            // without limit; drop the oldest, keep the count on the log line.
            if self.deferred_basic_gains.len() >= DEFERRED_BASIC_CAP {
                self.deferred_basic_gains.remove(0);
                eprintln!("[survey-aggregator] deferred_basic_gains full; dropped oldest gain");
            }
            self.deferred_basic_gains.push(DeferredBasicGain {
                qty,
                item_name,
                map_display_name,
                resolved_at,
            });
            return;
        };
        let use_id = self.pending_uses[idx].survey_use_id;

        // Single loot authority: skip the bump when the chat pipeline already
        // counted this gain (tagged `survey_chat` twin). The name tie-breaks
        // same-tick multi-item loot sharing qty/time.
        if !persistence::chat_gain_already_counted(
            conn,
            use_id,
            qty,
            resolved_at.as_deref(),
            item_name.as_deref(),
            CHAT_BASIC_WINDOW_SECS as u32,
        ) {
            let _ = persistence::add_loot_qty(conn, use_id, qty);
        }
        if let Some(at) = resolved_at {
            update_session_loot_timestamps(conn, use_id, &at);
        }
        if let PlayerEvent::ItemAdded { provenance, .. } = event {
            inject_survey_use_id_into(provenance, use_id);
        } else if let PlayerEvent::ItemStackChanged { provenance, .. } = event {
            inject_survey_use_id_into(provenance, use_id);
        }

        // Remember this Basic use so an immediately-following
        // ScreenText "(speed bonus!)" marker can patch the right rows.
        self.last_basic_use_for_bonus = Some(use_id);

        // Basic completes after first loot batch in the same tick. Mark
        // completed and remove from pending. (If more loot arrives in the
        // same tick it'll go through the same code-path before completion
        // is observed by callers.)
        let _ = persistence::set_use_status(conn, use_id, SurveyUseStatus::Completed);
        self.pending_uses.remove(idx);
    }

    /// Periodic sweep of expired multihit nodes. Cheap — the expired-set
    /// query is indexed and almost always empty.
    ///
    /// `now_iso` is the current event's full UTC timestamp
    /// (`YYYY-MM-DD HH:MM:SS`, log-derived, not wall clock). Both the 30-min
    /// cutoff and the in-memory grace-queue prune are computed on the full
    /// datetime so the sweep is date-aware across midnight.
    fn run_multihit_sweep(
        &mut self,
        conn: &Connection,
        character: &str,
        server: &str,
        now_secs: u32,
        now_iso: &str,
        emitted: &mut Vec<SurveyAggregatorEvent>,
    ) {
        // In-memory grace queue first: drop pending uses whose 60s window
        // has passed. (DB rows stay pending_loot — attribution may still
        // arrive via the FromSurvey fallback; only the in-memory shortcut
        // expires here.)
        let now_dt_opt = parse_utc_datetime(now_iso);
        self.expire_pending_uses(now_secs, now_dt_opt);

        let Some(now_dt) = now_dt_opt else {
            return;
        };
        let cutoff = now_dt - chrono::Duration::seconds(MULTIHIT_TIMEOUT_SECS as i64);
        let cutoff_s = cutoff.format("%Y-%m-%d %H:%M:%S").to_string();

        let expired = match multihit_state::sweep_expired(conn, character, server, &cutoff_s) {
            Ok(rows) => rows,
            Err(e) => {
                eprintln!("[survey-aggregator] multihit sweep failed: {e}");
                return;
            }
        };
        for node in expired {
            // Convention (matches handle_mining_started's close path):
            // a node that produced loot completes its use; an empty node
            // aborts it. Only transition rows still pending_loot — a use
            // already closed by another path keeps its status.
            let closed = match persistence::get_use(conn, node.survey_use_id) {
                Ok(Some(u)) if u.status == SurveyUseStatus::PendingLoot => {
                    let next = if u.loot_qty > 0 {
                        SurveyUseStatus::Completed
                    } else {
                        SurveyUseStatus::Aborted
                    };
                    persistence::set_use_status(conn, node.survey_use_id, next).is_ok()
                }
                _ => false,
            };
            if closed {
                emitted.push(SurveyAggregatorEvent::UseCompleted {
                    use_id: node.survey_use_id,
                });
            }
            if let Ok(node_id) = u32::try_from(node.node_entity_id) {
                emitted.push(SurveyAggregatorEvent::MultihitNodeClosed {
                    use_id: node.survey_use_id,
                    node_entity_id: node_id,
                });
            }
            // Drop any in-memory chat-mining engagement bound to this use so
            // a later swing can't keep attributing to a closed node.
            if self
                .chat_mining
                .as_ref()
                .is_some_and(|cm| cm.survey_use_id == node.survey_use_id)
            {
                self.chat_mining = None;
            }
        }
    }

    /// Auto-end check for crafting-trigger sessions: when consumed_count
    /// reaches crafted_count AND no pending_loot uses remain, end the
    /// session automatically. `event_iso` is the current event's UTC
    /// timestamp (derived from the log, not wall clock) — used as the
    /// fallback ended_at so replayed sessions don't get today's date.
    fn maybe_auto_end_crafting_session(
        &mut self,
        conn: &Connection,
        character: &str,
        server: &str,
        event_iso: Option<&str>,
        emitted: &mut Vec<SurveyAggregatorEvent>,
    ) {
        let Some(s) = self.fetch_active_session_full(conn, character, server) else {
            return;
        };
        if s.start_trigger != SessionStartTrigger::Crafting {
            return;
        }
        let crafted = s.crafted_count.unwrap_or(0);
        if crafted == 0 || s.consumed_count < crafted {
            return;
        }
        // All crafted maps consumed. Check pending uses.
        match persistence::session_has_pending_uses(conn, s.id) {
            Ok(true) => return, // wait for loot windows to close
            Ok(false) => {}
            Err(e) => {
                eprintln!("[survey-aggregator] session_has_pending_uses failed: {e}");
                return;
            }
        }
        // Prefer the event timestamp over wall clock so replayed/old-log
        // sessions don't get "today" as their ended_at.
        let fallback = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
        let end_iso = event_iso.unwrap_or(&fallback);
        if let Err(e) = persistence::end_session(conn, s.id, end_iso) {
            eprintln!("[survey-aggregator] end_session failed: {e}");
            return;
        }
        // Correct started_at/ended_at from actual event timestamps so the
        // header reflects when the activity really happened, not when the
        // auto-end condition happened to trip.
        if let Err(e) = persistence::recompute_session_bounds_and_end(conn, s.id, end_iso) {
            eprintln!("[survey-aggregator] recompute_session_bounds_and_end failed: {e}");
        }
        self.cached_active_session_id = Some(None);
        emitted.push(SurveyAggregatorEvent::SessionEnded {
            session_id: s.id,
            reason: "auto",
        });
    }

    /// Hard cap on session length. If the active session's span — from its
    /// `started_at` to `event_iso`, both log-derived UTC — has reached
    /// [`SESSION_MAX_DURATION_SECS`], end it. A fresh session auto-starts on
    /// the next survey craft/use via the normal path, so the triggering event
    /// (handled right after this returns) lands in the new session.
    ///
    /// Pending uses are intentionally left untouched: ending the session
    /// doesn't sever a use from its still-open loot window, so any loot that
    /// arrives afterward keeps attributing to the (now-closed) use as before.
    ///
    /// Returns a `SessionEnded { reason: "rollover" }` event when it fires so
    /// the frontend can refresh; `None` otherwise (no active session, span
    /// under the cap, or an unparseable timestamp — all safe no-ops).
    fn maybe_roll_over_session(
        &mut self,
        conn: &Connection,
        character: &str,
        server: &str,
        event_iso: &str,
    ) -> Option<SurveyAggregatorEvent> {
        let s = self.fetch_active_session_full(conn, character, server)?;
        let start = parse_utc_datetime(&s.started_at)?;
        let now = parse_utc_datetime(event_iso)?;
        if (now - start).num_seconds() < SESSION_MAX_DURATION_SECS {
            return None;
        }
        if let Err(e) = persistence::end_session(conn, s.id, event_iso) {
            eprintln!("[survey-aggregator] rollover end_session failed: {e}");
            return None;
        }
        // Correct started_at/ended_at from the session's own event timestamps
        // so its header reflects the real activity span, not the cap moment.
        if let Err(e) = persistence::recompute_session_bounds_and_end(conn, s.id, event_iso) {
            eprintln!("[survey-aggregator] rollover recompute_session_bounds_and_end failed: {e}");
        }
        self.cached_active_session_id = Some(None);
        Some(SurveyAggregatorEvent::SessionEnded {
            session_id: s.id,
            reason: "rollover",
        })
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
    /// Drop the cached active-session id so the next lookup re-resolves from
    /// the DB. Call this when a session is removed by a path that bypasses the
    /// aggregator's own start/end flow — notably the History UI's
    /// `survey_tracker_delete_session`, which deletes rows directly. Without
    /// this, the cache would keep handing out a now-missing session id and
    /// every subsequent `insert_use` would fail the FK constraint.
    pub fn invalidate_session_cache(&mut self) {
        self.cached_active_session_id = None;
    }

    fn fetch_active_session(
        &mut self,
        conn: &Connection,
        character: &str,
        server: &str,
    ) -> Option<i64> {
        if let Some(cached) = self.cached_active_session_id {
            return cached;
        }
        let s = persistence::active_session(conn, character, server)
            .ok()
            .flatten();
        let id = s.map(|s| s.id);
        self.cached_active_session_id = Some(id);
        id
    }

    /// Full active-session fetch (no cache). Used when we need fields beyond
    /// the id (e.g., crafted_count for the auto-end check).
    fn fetch_active_session_full(
        &mut self,
        conn: &Connection,
        character: &str,
        server: &str,
    ) -> Option<crate::survey::types::SurveySession> {
        persistence::active_session(conn, character, server)
            .ok()
            .flatten()
    }

    fn expire_pending_uses(&mut self, now_secs: u32, now_dt: Option<chrono::NaiveDateTime>) {
        // Pop expired entries from the front. The 60s grace covers the
        // walk from "used here" to "node spawned over there"; anything
        // older is gone. Full-datetime comparison so the window survives
        // midnight (mirror the chat basic path); seconds-of-day is only
        // the fallback when a timestamp didn't parse.
        while let Some(front) = self.pending_uses.front() {
            let expired = match (now_dt, front.used_at) {
                (Some(now), Some(used)) => (now - used).num_seconds() > SURVEY_TO_MINING_GRACE_SECS as i64,
                _ => now_secs.saturating_sub(front.used_at_secs) > SURVEY_TO_MINING_GRACE_SECS,
            };
            if expired {
                let expired = self.pending_uses.pop_front().unwrap();
                // Mark as aborted (no loot ever arrived) — only if still
                // pending in the DB. Best-effort; a quick test would race
                // with the DB-side status.
                // For Multihit/Motherlode we don't auto-mark aborted because
                // the player may genuinely still be walking; leave them
                // pending and let the multihit timeout handle it.
                // For Basic, no Mining follows so a 60s expiry without
                // attribution likely means the chat correlation missed —
                // leave the row pending; it can be cleaned up by a future
                // sweep.
                let _ = expired;
            } else {
                break;
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
    if let Ok(Some(su)) = persistence::get_use(conn, use_id) {
        if let Some(sid) = su.session_id {
            let _ = persistence::update_first_loot_at(conn, sid, ts);
            let _ = persistence::update_last_loot_at(conn, sid, ts);
            // Keep started_at tight for open sessions so the header
            // reflects actual activity, not the session-creation moment.
            let _ = persistence::tighten_started_at(conn, sid, ts);
        }
    }
}

// ============================================================
// Free helpers
// ============================================================

fn is_mining_loop(action_type: &str, label: &str) -> bool {
    if action_type != "ChopLumber" {
        return false;
    }
    let normalized = label
        .trim()
        .trim_end_matches(|c: char| c == '.' || c.is_whitespace())
        .to_lowercase();
    normalized == "mining"
}

fn event_secs_of_day(event: &PlayerEvent) -> Option<u32> {
    let ts = event_hms_timestamp(event)?;
    parse_secs_of_day(&ts)
}

fn event_hms_timestamp(event: &PlayerEvent) -> Option<&str> {
    match event {
        PlayerEvent::ItemAdded { timestamp, .. } => Some(timestamp.as_str()),
        PlayerEvent::ItemStackChanged { timestamp, .. } => Some(timestamp.as_str()),
        PlayerEvent::ItemDeleted { timestamp, .. } => Some(timestamp.as_str()),
        PlayerEvent::DelayLoopStarted { timestamp, .. } => Some(timestamp.as_str()),
        PlayerEvent::StorageWithdrawal { timestamp, .. } => Some(timestamp.as_str()),
        PlayerEvent::InteractionStarted { timestamp, .. } => Some(timestamp.as_str()),
        _ => None,
    }
}

fn parse_secs_of_day(hms: &str) -> Option<u32> {
    let mut parts = hms.split(':');
    let h: u32 = parts.next()?.parse().ok()?;
    let m: u32 = parts.next()?.parse().ok()?;
    let s: u32 = parts.next()?.parse().ok()?;
    Some(h * 3600 + m * 60 + s)
}

/// Parse a UTC `YYYY-MM-DD HH:MM:SS` string (the shape produced by
/// `to_utc_datetime_with_base` and by the chat pipeline) for window math.
fn parse_utc_datetime(ts: &str) -> Option<chrono::NaiveDateTime> {
    chrono::NaiveDateTime::parse_from_str(ts, "%Y-%m-%d %H:%M:%S").ok()
}

/// Midnight-safe grace check: true when `now` is within `window_secs` after
/// `used_at`. Both sides are full datetimes (the chat basic path's
/// convention — mirror it here), so a use at 23:59:50 still matches a gain
/// at 00:00:10 where `saturating_sub` on seconds-of-day would see a ~24h
/// gap and miss.
fn within_grace_window(
    now_dt: Option<chrono::NaiveDateTime>,
    used_at: Option<chrono::NaiveDateTime>,
    now_secs: u32,
    used_at_secs: u32,
    window_secs: u32,
) -> bool {
    match (now_dt, used_at) {
        (Some(now), Some(used)) => {
            let dt = (now - used).num_seconds();
            (0..=(window_secs as i64)).contains(&dt)
        }
        // Unparseable timestamp (defensive): fall back to the old
        // seconds-of-day comparison.
        _ => now_secs.saturating_sub(used_at_secs) <= window_secs,
    }
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

/// Helper to mutate an `ItemProvenance::Attributed` in place to set its
/// `survey_use_id`. No-op for other provenance variants — gain-event
/// callers should only invoke this on Attributed values.
fn inject_survey_use_id_into(provenance: &mut ItemProvenance, use_id: i64) {
    if let ItemProvenance::Attributed { survey_use_id, .. } = provenance {
        *survey_use_id = Some(use_id);
    }
}

/// Retroactively patch all recent `item_transactions` rows with
/// `source_kind = 'survey_map_use'` that are missing a `survey_use_id` in
/// their `source_details`. Used for deferred basic survey gains where the
/// transaction was written before the pending use existed (the game emits
/// primary loot before the map deletion).
/// This is a bulk patch: without a map filter it tags every unlinked
/// survey_map_use row for this character/server with the new use_id. In
/// practice only 1-2 rows are unlinked at any time (the primary loot from
/// the current survey), so the blind tag is safe for the log path (the
/// DeleteItem IS the matching event). The manual path passes its own map
/// to avoid stamping another map's kept rows in a mixed buffer.
fn retroactively_tag_unlinked_survey_transactions(
    conn: &Connection,
    survey_use_id: i64,
    character: &str,
    server: &str,
    map_display_name: Option<&str>,
) {
    // Optional map scope: the manual path passes its own map so a mixed
    // buffer can't stamp another map's kept rows with this use_id. The
    // log path passes None (its DeleteItem IS the matching event, so the
    // blind tag is correct there).
    let result = conn.execute(
        "UPDATE item_transactions
         SET source_details = CASE
           WHEN source_details IS NULL OR source_details = ''
             THEN json_object('survey_use_id', ?1)
           ELSE json_set(source_details, '$.survey_use_id', ?1)
         END
         WHERE character_name = ?2
           AND server_name = ?3
           AND source_kind = 'survey_map_use'
           AND quantity > 0
           AND (source_details IS NULL
                OR json_extract(source_details, '$.survey_use_id') IS NULL)
           AND (?4 IS NULL
                OR json_extract(source_details, '$.survey_map_internal_name') = ?4)",
        rusqlite::params![survey_use_id, character, server, map_display_name],
    );
    if let Err(e) = result {
        eprintln!(
            "[survey-aggregator] retroactively_tag_unlinked_survey_transactions failed for use={}: {}",
            survey_use_id, e
        );
    }
}
/// Inspect a provenance for the read-only info the aggregator
/// needs before it can take a mutable borrow:
/// - `Some(node_entity_id)` if this is a Mining-attributed gain (the inner
///   `node_entity_id` may itself be `None` for nameless nodes — passed
///   through as-is so the caller can decide what to do).
/// - `is_basic_survey_gain` true if this is a `SurveyMapUse`-attributed gain.
/// - the survey map's display name for `SurveyMapUse` gains (`None` otherwise),
///   used to correlate deferred gains with the manual use's map.
///
/// `(None, false, None)` for other provenance variants.
fn read_provenance_kind(
    provenance: &ItemProvenance,
) -> (Option<u32>, bool, Option<String>) {
    match provenance {
        ItemProvenance::Attributed {
            source: ActivitySource::Mining { node_entity_id, .. },
            ..
        } => (*node_entity_id, false, None),
        ItemProvenance::Attributed {
            source:
                ActivitySource::SurveyMapUse {
                    survey_map_internal_name,
                },
            ..
        } => (None, true, survey_map_internal_name.clone()),
        _ => (None, false, None),
    }
}

/// Apply a survey_use_id to whichever provenance field a gain event carries.
fn apply_survey_use_id(event: &mut PlayerEvent, use_id: i64) {
    match event {
        PlayerEvent::ItemAdded { provenance, .. } => {
            inject_survey_use_id_into(provenance, use_id);
        }
        PlayerEvent::ItemStackChanged { provenance, .. } => {
            inject_survey_use_id_into(provenance, use_id);
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cdn_commands::GameDataState;
    use crate::db::migrations::run_migrations;
    use crate::game_data::GameData;
    use std::sync::Arc;
    use tokio::sync::RwLock;

    fn fresh_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        run_migrations(&conn, None).unwrap();
        conn
    }

    fn empty_game_data() -> GameDataState {
        Arc::new(RwLock::new(GameData::empty()))
    }

    /// Build a GameDataState pre-loaded with a small set of survey-map items
    /// covering all three kinds. Sufficient for the aggregator's
    /// `lookup_survey_kind` to function.
    fn game_data_with_survey_maps() -> GameDataState {
        use crate::game_data::ItemInfo;
        use serde_json::json;

        fn survey_item(id: u32, internal: &str, display: &str, keywords: &[&str]) -> ItemInfo {
            ItemInfo {
                id,
                name: display.to_string(),
                description: None,
                icon_id: None,
                value: None,
                max_stack_size: None,
                keywords: keywords.iter().map(|s| s.to_string()).collect(),
                effect_descs: vec![],
                internal_name: Some(internal.to_string()),
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
                raw_json: json!({}),
            }
        }

        let mut gd = GameData::empty();
        let entries = [
            (
                100,
                "GeologySurveySerbule1",
                "Serbule Blue Mineral Survey",
                &["Document", "MineralSurvey"][..],
            ),
            (
                200,
                "MiningSurveyKurMountains1X",
                "Kur Mountains Simple Metal Motherlode Map",
                &["Document", "MiningSurvey", "MotherlodeMap"][..],
            ),
            (
                300,
                "MiningSurveyPovus7Y",
                "Povus Astounding Mining Survey",
                &["Document", "MiningSurvey"][..],
            ),
            // Survey-yield identity items (chat identity fallback) + a
            // resolvable non-yield item for the negative case.
            (
                400,
                "Tungsten",
                "Tungsten",
                &["AlchemyIngredient", "Ore", "Tungsten=250"][..],
            ),
            (
                500,
                "MetalSlab9",
                "Marvelous Metal Slab",
                &[
                    "MetalSlab",
                    "MetalSlab9",
                    "SkillIngredient",
                    "ValuableMetalSlab",
                ][..],
            ),
            (
                600,
                "MagicSand",
                "Magic Sand",
                &["KhyrulekLore", "Sand=1000", "VendorTrash"][..],
            ),
            (
                700,
                "SemiRealHassium",
                "Semi-Real Hassium",
                &["Crystal", "IrradiatedCrystal", "SemiRealHassium"][..],
            ),
            (800, "Skull", "Skull", &["VendorTrash"][..]),
            (
                810,
                "Sulfur",
                "Sulfur",
                &["AlchemyIngredient", "MagicDust", "Sulfur"][..],
            ),
            (
                820,
                "Saltpeter",
                "Saltpeter",
                &["AlchemyIngredient", "MagicDust", "Saltpeter"][..],
            ),
            (
                830,
                "Fluorite",
                "Fluorite",
                &["BlueCrystal", "Crystal", "Fluorite=500", "Gem=200"][..],
            ),
            // Crystal-keyword but NOT a color family — must not match the
            // identity gate (emotion pearls, gem fragments, etc.).
            (
                840,
                "FearPearl1",
                "Small Fear Pearl",
                &["Crystal", "EmotionPearl", "GemFragment"][..],
            ),
        ];
        for (id, internal, display, keywords) in entries {
            gd.items
                .insert(id, survey_item(id, internal, display, keywords));
            gd.item_internal_name_index.insert(internal.to_string(), id);
            gd.item_name_index.insert(display.to_string(), id);
        }
        Arc::new(RwLock::new(gd))
    }

    #[test]
    fn test_aggregator_starts_session_on_first_use_when_kind_known() {
        // Without CDN data the aggregator can't recognize survey kinds, so
        // this test verifies the "unknown item" path is a no-op rather than
        // the happy path. The full happy-path test requires loaded GameData
        // and lives in the integration test harness (next).
        let conn = fresh_db();
        let game_data = empty_game_data();
        let mut agg = SurveySessionAggregator::new(game_data);

        let mut event = PlayerEvent::ItemDeleted {
            timestamp: "12:00:00".to_string(),
            instance_id: 1,
            item_name: Some("UnknownItem".to_string()),
            context: crate::player_event_parser::DeleteContext::Consumed,
        };

        let emitted = agg.process_event(&mut event, &conn, "Zenith", "Dreva", None);
        assert!(emitted.is_empty(), "no events for non-survey deletion");
        // No session should have been created
        assert!(persistence::active_session(&conn, "Zenith", "Dreva")
            .unwrap()
            .is_none());
    }

    #[test]
    fn test_manual_session_lifecycle() {
        let conn = fresh_db();
        let mut agg = SurveySessionAggregator::new(empty_game_data());

        let id = agg
            .start_manual_session(&conn, "Zenith", "Dreva", "2026-04-15 12:00:00")
            .unwrap();
        assert!(id > 0);

        // Second start should fail
        let err = agg
            .start_manual_session(&conn, "Zenith", "Dreva", "2026-04-15 12:01:00")
            .unwrap_err();
        assert!(err.contains("already active"));

        // End it
        let ended = agg
            .end_active_session(&conn, "Zenith", "Dreva", "2026-04-15 12:30:00")
            .unwrap();
        assert_eq!(ended, Some(id));

        // Now starts work again
        let id2 = agg
            .start_manual_session(&conn, "Zenith", "Dreva", "2026-04-15 13:00:00")
            .unwrap();
        assert!(id2 > id);
    }

    /// Regression: the History UI's delete_session removes rows directly,
    /// bypassing the aggregator's end flow. If the aggregator keeps the
    /// deleted id in its active-session cache, every subsequent `insert_use`
    /// fails the survey_uses → survey_sessions FK and no new surveys are
    /// captured. Invalidating the cache must make it re-resolve.
    #[test]
    fn test_delete_session_out_from_under_aggregator_recovers() {
        let conn = fresh_db();
        let mut agg = SurveySessionAggregator::new(empty_game_data());

        // Aggregator starts + caches a session.
        let id = agg
            .start_manual_session(&conn, "Zenith", "Dreva", "2026-04-15 12:00:00")
            .unwrap();
        assert_eq!(agg.fetch_active_session(&conn, "Zenith", "Dreva"), Some(id));

        // Simulate survey_tracker_delete_session: rows removed directly.
        conn.execute(
            "DELETE FROM survey_uses WHERE session_id = ?1",
            rusqlite::params![id],
        )
        .unwrap();
        conn.execute(
            "DELETE FROM survey_sessions WHERE id = ?1",
            rusqlite::params![id],
        )
        .unwrap();

        // The stale cached id would fail the FK if used for a new insert.
        let fk = persistence::insert_use(
            &conn,
            Some(id),
            "Zenith",
            "Dreva",
            "2026-04-15 12:05:00",
            "GeologySurveySerbule1",
            "Serbule Blue Mineral Survey",
            crate::survey::types::SurveyUseKind::Basic,
            Some("Serbule"),
        );
        assert!(
            fk.is_err(),
            "inserting a use against a deleted session must fail the FK"
        );

        // Fix: invalidation re-resolves to 'no active session' so the next
        // use auto-starts a fresh one instead of reusing the missing id.
        agg.invalidate_session_cache();
        assert_eq!(agg.fetch_active_session(&conn, "Zenith", "Dreva"), None);
    }

    #[test]
    fn test_session_rolls_over_after_max_duration() {
        // A session that runs past SESSION_MAX_DURATION_SECS is auto-ended and
        // the next survey use starts a fresh session, bounding session size.
        let conn = fresh_db();
        let mut agg = SurveySessionAggregator::new(game_data_with_survey_maps());
        agg.set_base_date(test_base_date());

        // First Basic use at 12:00:00 auto-starts session 1.
        let mut use1 = PlayerEvent::ItemDeleted {
            timestamp: "12:00:00".to_string(),
            instance_id: 1,
            item_name: Some("GeologySurveySerbule1".to_string()),
            context: crate::player_event_parser::DeleteContext::Consumed,
        };
        agg.process_event(&mut use1, &conn, "Zenith", "Dreva", Some("Serbule"));
        let id1 = persistence::active_session(&conn, "Zenith", "Dreva")
            .unwrap()
            .unwrap()
            .id;

        // A use 1h later is under the cap — same session.
        let mut use_mid = PlayerEvent::ItemDeleted {
            timestamp: "13:00:00".to_string(),
            instance_id: 2,
            item_name: Some("GeologySurveySerbule1".to_string()),
            context: crate::player_event_parser::DeleteContext::Consumed,
        };
        agg.process_event(&mut use_mid, &conn, "Zenith", "Dreva", Some("Serbule"));
        assert_eq!(
            persistence::active_session(&conn, "Zenith", "Dreva")
                .unwrap()
                .unwrap()
                .id,
            id1,
            "a use within the cap stays in the same session"
        );

        // A use just past 2h from the start rolls the session over.
        let mut use_late = PlayerEvent::ItemDeleted {
            timestamp: "14:00:01".to_string(),
            instance_id: 3,
            item_name: Some("GeologySurveySerbule1".to_string()),
            context: crate::player_event_parser::DeleteContext::Consumed,
        };
        let emitted = agg.process_event(&mut use_late, &conn, "Zenith", "Dreva", Some("Serbule"));

        // Session 1 is now ended…
        let s1 = persistence::get_session(&conn, id1).unwrap().unwrap();
        assert!(s1.ended_at.is_some(), "over-long session must be ended");
        // …a rollover end event was emitted…
        assert!(
            emitted.iter().any(|e| matches!(
                e,
                SurveyAggregatorEvent::SessionEnded {
                    reason: "rollover",
                    ..
                }
            )),
            "expected a rollover SessionEnded event: {emitted:?}"
        );
        // …and a fresh session now owns the late use.
        let active = persistence::active_session(&conn, "Zenith", "Dreva")
            .unwrap()
            .expect("a new session should be active after rollover");
        assert!(active.id > id1, "rollover should start a new session");
        assert_eq!(
            active.consumed_count, 1,
            "the late use belongs to the new session, not the closed one"
        );
    }

    #[test]
    fn test_no_rollover_when_auto_start_disabled() {
        // Manual regime (auto_start off): the length cap does not touch a
        // user-managed session, even well past the max duration.
        let conn = fresh_db();
        let mut agg = SurveySessionAggregator::new(game_data_with_survey_maps());
        agg.auto_start_enabled = false;
        agg.set_base_date(test_base_date());

        let id = agg
            .start_manual_session(&conn, "Zenith", "Dreva", "2026-04-15 12:00:00")
            .unwrap();

        // An event 3h later must not auto-end the manual session.
        let mut later = PlayerEvent::ItemDeleted {
            timestamp: "15:00:00".to_string(),
            instance_id: 1,
            item_name: Some("UnknownItem".to_string()),
            context: crate::player_event_parser::DeleteContext::Consumed,
        };
        agg.process_event(&mut later, &conn, "Zenith", "Dreva", None);

        let s = persistence::get_session(&conn, id).unwrap().unwrap();
        assert!(s.ended_at.is_none(), "manual session must survive the cap");
    }

    #[test]
    fn test_inject_survey_use_id_helper() {
        let mut prov = ItemProvenance::Attributed {
            source: ActivitySource::Mining {
                node_entity_id: Some(99),
                node_name: None,
            },
            confidence: crate::player_event_parser::AttributionConfidence::Confident,
            survey_use_id: None,
        };
        inject_survey_use_id_into(&mut prov, 42);
        match prov {
            ItemProvenance::Attributed { survey_use_id, .. } => {
                assert_eq!(survey_use_id, Some(42));
            }
            _ => panic!("expected Attributed"),
        }

        // No-op for non-Attributed
        let mut prov2 = ItemProvenance::UnknownSource;
        inject_survey_use_id_into(&mut prov2, 99);
        assert_eq!(prov2, ItemProvenance::UnknownSource);
    }

    #[test]
    fn test_is_mining_loop_normalizes_label_variants() {
        assert!(is_mining_loop("ChopLumber", "Mining..."));
        assert!(is_mining_loop("ChopLumber", "Mining ..."));
        assert!(is_mining_loop("ChopLumber", "Mining"));
        assert!(!is_mining_loop("ChopLumber", "Surveying"));
        assert!(!is_mining_loop("Cook", "Mining..."));
    }

    #[test]
    fn test_basic_survey_consume_creates_use_and_session() {
        // Player consumes a Basic survey map → should auto-start a FirstUse
        // session and create a survey_uses row in pending_loot.
        let conn = fresh_db();
        let mut agg = SurveySessionAggregator::new(game_data_with_survey_maps());

        let mut delete_event = PlayerEvent::ItemDeleted {
            timestamp: "12:00:00".to_string(),
            instance_id: 1,
            item_name: Some("GeologySurveySerbule1".to_string()),
            context: crate::player_event_parser::DeleteContext::Consumed,
        };

        let emitted =
            agg.process_event(&mut delete_event, &conn, "Zenith", "Dreva", Some("Serbule"));

        // Should have emitted SessionStarted + UseRecorded (in that order)
        assert_eq!(emitted.len(), 2, "expected 2 events: {:?}", emitted);
        assert!(matches!(
            emitted[0],
            SurveyAggregatorEvent::SessionStarted { .. }
        ));
        assert!(matches!(
            emitted[1],
            SurveyAggregatorEvent::UseRecorded { .. }
        ));

        // DB state: one session, one use, both correctly populated
        let session = persistence::active_session(&conn, "Zenith", "Dreva")
            .unwrap()
            .expect("session should be active");
        assert_eq!(session.start_trigger, SessionStartTrigger::FirstUse);
        assert_eq!(session.consumed_count, 1);

        let uses = persistence::uses_for_session(&conn, session.id).unwrap();
        assert_eq!(uses.len(), 1);
        assert_eq!(uses[0].kind, SurveyUseKind::Basic);
        assert_eq!(uses[0].area.as_deref(), Some("Serbule"));
        assert_eq!(uses[0].status, SurveyUseStatus::PendingLoot);
        assert_eq!(uses[0].map_display_name, "Serbule Blue Mineral Survey");
    }

    fn test_base_date() -> chrono::NaiveDate {
        chrono::NaiveDate::from_ymd_opt(2026, 4, 15).unwrap()
    }

    #[test]
    fn test_chat_gain_inside_basic_window_attributes() {
        // Consume a Basic survey map, then chat [Status] gains within the
        // same-tick window attribute to that use; a gain outside the window
        // (kill loot, foraging, …) does not.
        let conn = fresh_db();
        let mut agg = SurveySessionAggregator::new(game_data_with_survey_maps());
        agg.set_base_date(test_base_date());

        let mut delete_event = PlayerEvent::ItemDeleted {
            timestamp: "12:00:00".to_string(),
            instance_id: 1,
            item_name: Some("GeologySurveySerbule1".to_string()),
            context: crate::player_event_parser::DeleteContext::Consumed,
        };
        agg.process_event(&mut delete_event, &conn, "Zenith", "Dreva", Some("Serbule"));

        let session = persistence::active_session(&conn, "Zenith", "Dreva")
            .unwrap()
            .unwrap();
        let use_id = persistence::uses_for_session(&conn, session.id).unwrap()[0].id;

        let attributed =
            agg.attribute_chat_gain(&conn, "Zenith", "Dreva", None, 9, "2026-04-15 12:00:01");
        assert_eq!(attributed, Some(use_id));

        let su = persistence::get_use(&conn, use_id).unwrap().unwrap();
        assert_eq!(su.loot_qty, 9, "chat gain should bump loot_qty");
        assert_eq!(
            su.status,
            SurveyUseStatus::Completed,
            "basic use completes on its same-tick chat loot"
        );

        // A second same-tick gain (speed-bonus item) accumulates.
        agg.attribute_chat_gain(&conn, "Zenith", "Dreva", None, 3, "2026-04-15 12:00:02");
        let su = persistence::get_use(&conn, use_id).unwrap().unwrap();
        assert_eq!(su.loot_qty, 12);

        // 30s later the window is closed — incidental loot is NOT survey loot.
        let attributed =
            agg.attribute_chat_gain(&conn, "Zenith", "Dreva", None, 5, "2026-04-15 12:00:30");
        assert_eq!(
            attributed, None,
            "gain outside the basic window must not attribute"
        );
        let su = persistence::get_use(&conn, use_id).unwrap().unwrap();
        assert_eq!(su.loot_qty, 12, "loot_qty unchanged by rejected gain");
    }

    #[test]
    fn test_chat_gain_without_survey_activity_ignored() {
        // No survey activity at all → nothing to attribute to.
        let conn = fresh_db();
        let mut agg = SurveySessionAggregator::new(game_data_with_survey_maps());
        let attributed =
            agg.attribute_chat_gain(&conn, "Zenith", "Dreva", None, 5, "2026-04-15 12:00:00");
        assert_eq!(attributed, None);
    }

    #[test]
    fn test_chat_gain_in_session_without_use_ignored() {
        // A manually-started session with no survey use yet — incidental
        // loot before the first map is ignored.
        let conn = fresh_db();
        let mut agg = SurveySessionAggregator::new(game_data_with_survey_maps());
        agg.start_manual_session(&conn, "Zenith", "Dreva", "2026-04-15 12:00:00")
            .unwrap();
        let attributed =
            agg.attribute_chat_gain(&conn, "Zenith", "Dreva", None, 5, "2026-04-15 12:00:01");
        assert_eq!(attributed, None);
    }

    #[test]
    fn test_chat_gain_of_survey_map_is_never_loot() {
        // Crafting a survey map emits the same "added to inventory" chat
        // line as loot. Even inside a basic window it must not attribute.
        let conn = fresh_db();
        let mut agg = SurveySessionAggregator::new(game_data_with_survey_maps());
        agg.set_base_date(test_base_date());

        let mut delete_event = PlayerEvent::ItemDeleted {
            timestamp: "12:00:00".to_string(),
            instance_id: 1,
            item_name: Some("GeologySurveySerbule1".to_string()),
            context: crate::player_event_parser::DeleteContext::Consumed,
        };
        agg.process_event(&mut delete_event, &conn, "Zenith", "Dreva", Some("Serbule"));

        let attributed = agg.attribute_chat_gain(
            &conn,
            "Zenith",
            "Dreva",
            Some("GeologySurveySerbule1"),
            1,
            "2026-04-15 12:00:01",
        );
        assert_eq!(attributed, None, "a survey map gain is a craft, not loot");
    }

    #[test]
    fn test_multihit_chat_gains_gated_by_swing_windows() {
        // Multihit flow, chat-only user: consume map → mining swings on the
        // survey node open per-swing loot windows. Gains inside a window
        // attribute; gains between swings or after moving to a different
        // node do not. Returning to the survey node resumes attribution.
        let conn = fresh_db();
        let mut agg = SurveySessionAggregator::new(game_data_with_survey_maps());
        agg.set_base_date(test_base_date());

        let mut consume = PlayerEvent::ItemDeleted {
            timestamp: "12:00:00".to_string(),
            instance_id: 1,
            item_name: Some("MiningSurveyPovus7Y".to_string()),
            context: crate::player_event_parser::DeleteContext::Consumed,
        };
        agg.process_event(&mut consume, &conn, "Zenith", "Dreva", Some("Povus"));
        let session_id = persistence::active_session(&conn, "Zenith", "Dreva")
            .unwrap()
            .unwrap()
            .id;
        let use_id = persistence::uses_for_session(&conn, session_id).unwrap()[0].id;

        let interaction = |ts: &str, entity: u32, name: &str| PlayerEvent::InteractionStarted {
            timestamp: ts.to_string(),
            entity_id: entity,
            interaction_type: 7,
            npc_name: name.to_string(),
        };
        let mining_loop = |ts: &str| PlayerEvent::DelayLoopStarted {
            timestamp: ts.to_string(),
            duration: 6.0,
            action_type: "ChopLumber".to_string(),
            label: "Mining...".to_string(),
            entity_id: 0,
            abort_condition: "AbortIfAttacked".to_string(),
        };

        // Swing 1 on the survey node (within the 60s walk grace).
        let mut ev = interaction("12:00:20", 9999, "MiningNodeFromSurvey9");
        agg.process_event(&mut ev, &conn, "Zenith", "Dreva", Some("Povus"));
        let mut ev = mining_loop("12:00:20");
        agg.process_event(&mut ev, &conn, "Zenith", "Dreva", Some("Povus"));

        // Loot at swing completion (start + 6s) attributes.
        let attributed =
            agg.attribute_chat_gain(&conn, "Zenith", "Dreva", None, 2, "2026-04-15 12:00:26");
        assert_eq!(attributed, Some(use_id), "swing-completion loot attributes");

        // Between swings: not survey loot (e.g. corpse loot mid-fight).
        let attributed =
            agg.attribute_chat_gain(&conn, "Zenith", "Dreva", None, 1, "2026-04-15 12:00:40");
        assert_eq!(attributed, None, "gain between swings must not attribute");

        // Swing 2 on the same node refreshes the window.
        let mut ev = interaction("12:00:45", 9999, "MiningNodeFromSurvey9");
        agg.process_event(&mut ev, &conn, "Zenith", "Dreva", Some("Povus"));
        let mut ev = mining_loop("12:00:45");
        agg.process_event(&mut ev, &conn, "Zenith", "Dreva", Some("Povus"));
        let attributed =
            agg.attribute_chat_gain(&conn, "Zenith", "Dreva", None, 3, "2026-04-15 12:00:51");
        assert_eq!(attributed, Some(use_id));

        // A swing on a DIFFERENT node closes the engagement; its loot is
        // regular mining, not survey loot (grace expired + use already
        // claimed, so no re-adoption).
        let mut ev = interaction("12:01:10", 8888, "");
        agg.process_event(&mut ev, &conn, "Zenith", "Dreva", Some("Povus"));
        let mut ev = mining_loop("12:01:10");
        agg.process_event(&mut ev, &conn, "Zenith", "Dreva", Some("Povus"));
        let attributed =
            agg.attribute_chat_gain(&conn, "Zenith", "Dreva", None, 4, "2026-04-15 12:01:16");
        assert_eq!(attributed, None, "regular-node loot must not attribute");

        let su = persistence::get_use(&conn, use_id).unwrap().unwrap();
        assert_eq!(su.loot_qty, 5, "2 + 3 from the two survey swings");
        assert_eq!(
            su.status,
            SurveyUseStatus::Completed,
            "different-node swing closes the survey use"
        );

        // Returning to the survey node resumes attribution (known binding).
        let mut ev = interaction("12:01:30", 9999, "MiningNodeFromSurvey9");
        agg.process_event(&mut ev, &conn, "Zenith", "Dreva", Some("Povus"));
        let mut ev = mining_loop("12:01:30");
        agg.process_event(&mut ev, &conn, "Zenith", "Dreva", Some("Povus"));
        let attributed =
            agg.attribute_chat_gain(&conn, "Zenith", "Dreva", None, 2, "2026-04-15 12:01:36");
        assert_eq!(
            attributed,
            Some(use_id),
            "returning to the survey node resumes"
        );

        // Corpse loot right after a swing completes (observed leak in the
        // 50x-povus capture): a foreign interaction inside the window's
        // trailing slack clips it, so the corpse's loot is NOT survey loot.
        let mut ev = interaction("12:02:00", 9999, "MiningNodeFromSurvey9");
        agg.process_event(&mut ev, &conn, "Zenith", "Dreva", Some("Povus"));
        let mut ev = mining_loop("12:02:00");
        agg.process_event(&mut ev, &conn, "Zenith", "Dreva", Some("Povus"));
        let attributed =
            agg.attribute_chat_gain(&conn, "Zenith", "Dreva", None, 1, "2026-04-15 12:02:06");
        assert_eq!(
            attributed,
            Some(use_id),
            "swing loot at completion attributes"
        );
        let mut ev = interaction("12:02:08", 7777, ""); // corpse search
        agg.process_event(&mut ev, &conn, "Zenith", "Dreva", Some("Povus"));
        let attributed =
            agg.attribute_chat_gain(&conn, "Zenith", "Dreva", None, 1, "2026-04-15 12:02:09");
        assert_eq!(
            attributed, None,
            "corpse loot inside the clipped slack must not attribute"
        );
    }

    #[test]
    fn test_motherlode_nameless_node_chat_attribution() {
        // Motherlode nodes are frequently nameless in StartInteraction —
        // adoption within the walk grace window must still work, and loot
        // outside the swing window must still be rejected.
        let conn = fresh_db();
        let mut agg = SurveySessionAggregator::new(game_data_with_survey_maps());
        agg.set_base_date(test_base_date());

        let mut consume = PlayerEvent::ItemDeleted {
            timestamp: "12:00:00".to_string(),
            instance_id: 1,
            item_name: Some("MiningSurveyKurMountains1X".to_string()),
            context: crate::player_event_parser::DeleteContext::Consumed,
        };
        agg.process_event(&mut consume, &conn, "Zenith", "Dreva", Some("KurMountains"));
        let session_id = persistence::active_session(&conn, "Zenith", "Dreva")
            .unwrap()
            .unwrap()
            .id;
        let use_id = persistence::uses_for_session(&conn, session_id).unwrap()[0].id;

        let mut ev = PlayerEvent::InteractionStarted {
            timestamp: "12:00:30".to_string(),
            entity_id: 5555,
            interaction_type: 7,
            npc_name: String::new(), // nameless, the common motherlode case
        };
        agg.process_event(&mut ev, &conn, "Zenith", "Dreva", Some("KurMountains"));
        let mut ev = PlayerEvent::DelayLoopStarted {
            timestamp: "12:00:30".to_string(),
            duration: 6.0,
            action_type: "ChopLumber".to_string(),
            label: "Mining...".to_string(),
            entity_id: 0,
            abort_condition: "AbortIfAttacked".to_string(),
        };
        agg.process_event(&mut ev, &conn, "Zenith", "Dreva", Some("KurMountains"));

        let attributed =
            agg.attribute_chat_gain(&conn, "Zenith", "Dreva", None, 2, "2026-04-15 12:00:36");
        assert_eq!(attributed, Some(use_id), "swing loot attributes");

        let attributed =
            agg.attribute_chat_gain(&conn, "Zenith", "Dreva", None, 1, "2026-04-15 12:00:50");
        assert_eq!(
            attributed, None,
            "loot after the swing window must not attribute"
        );

        let su = persistence::get_use(&conn, use_id).unwrap().unwrap();
        assert_eq!(su.loot_qty, 2);
    }

    #[test]
    fn test_ore_identity_attributes_to_latest_use_mid_session() {
        // Item-identity fallback: Ore/MetalSlab/Hassium/Magic Sand gains
        // count as survey loot whenever a session is active — no collection
        // window required (Player.log timing signals don't reach every
        // user's log). Non-yield items still need a window; nothing counts
        // after the session ends.
        let conn = fresh_db();
        let mut agg = SurveySessionAggregator::new(game_data_with_survey_maps());
        agg.set_base_date(test_base_date());

        let mut consume = PlayerEvent::ItemDeleted {
            timestamp: "12:00:00".to_string(),
            instance_id: 1,
            item_name: Some("MiningSurveyPovus7Y".to_string()),
            context: crate::player_event_parser::DeleteContext::Consumed,
        };
        agg.process_event(&mut consume, &conn, "Zenith", "Dreva", Some("Povus"));
        let session_id = persistence::active_session(&conn, "Zenith", "Dreva")
            .unwrap()
            .unwrap()
            .id;
        let use_id = persistence::uses_for_session(&conn, session_id).unwrap()[0].id;

        // Minutes after any window: identity still attributes.
        for (internal, qty, ts) in [
            ("Tungsten", 3, "2026-04-15 12:05:00"),
            ("MetalSlab9", 2, "2026-04-15 12:06:00"),
            ("MagicSand", 4, "2026-04-15 12:07:00"),
            ("SemiRealHassium", 1, "2026-04-15 12:08:00"),
            ("Sulfur", 2, "2026-04-15 12:08:10"),
            ("Saltpeter", 1, "2026-04-15 12:08:20"),
            ("Fluorite", 3, "2026-04-15 12:08:30"), // color-crystal family
        ] {
            let attributed =
                agg.attribute_chat_gain(&conn, "Zenith", "Dreva", Some(internal), qty, ts);
            assert_eq!(
                attributed,
                Some(use_id),
                "{internal} should attribute by identity"
            );
        }
        let su = persistence::get_use(&conn, use_id).unwrap().unwrap();
        assert_eq!(su.loot_qty, 16, "3+2+4+1+2+1+3 identity gains");

        // Resolvable non-yield items outside every window: not survey loot.
        // A bare-`Crystal` item (emotion pearl) must not match the color
        // families either.
        for internal in ["Skull", "FearPearl1"] {
            let attributed = agg.attribute_chat_gain(
                &conn,
                "Zenith",
                "Dreva",
                Some(internal),
                1,
                "2026-04-15 12:09:00",
            );
            assert_eq!(attributed, None, "{internal} needs a collection window");
        }

        // After the session ends, ore is regular mining again.
        agg.end_active_session(&conn, "Zenith", "Dreva", "2026-04-15 12:10:00")
            .unwrap();
        let attributed = agg.attribute_chat_gain(
            &conn,
            "Zenith",
            "Dreva",
            Some("Tungsten"),
            1,
            "2026-04-15 12:11:00",
        );
        assert_eq!(attributed, None, "no attribution without an active session");
    }

    #[test]
    fn test_basic_retro_claim_tags_preexisting_chat_rows() {
        // The chat watcher can process the loot lines BEFORE the player
        // watcher processes the map's DeleteItem. Those rows are inserted
        // untagged; consuming the Basic map must claim the ones in the
        // same-tick window (and only those), bumping loot_qty.
        let conn = fresh_db();
        let mut agg = SurveySessionAggregator::new(game_data_with_survey_maps());
        agg.set_base_date(test_base_date());

        conn.execute(
            "INSERT INTO item_transactions
                (timestamp, character_name, server_name, item_name, internal_name,
                 quantity, context, source)
             VALUES
                ('2026-04-15 12:00:00','Zenith','Dreva','Fluorite','Fluorite',
                 9,'loot','chat_status'),
                ('2026-04-15 11:58:00','Zenith','Dreva','Skull','Skull',
                 1,'loot','chat_status'),
                ('2026-04-15 12:00:00','Zenith','Dreva','Serbule Blue Mineral Survey',
                 'GeologySurveySerbule1',1,'loot','chat_status')",
            [],
        )
        .unwrap();

        let mut delete_event = PlayerEvent::ItemDeleted {
            timestamp: "12:00:00".to_string(),
            instance_id: 1,
            item_name: Some("GeologySurveySerbule1".to_string()),
            context: crate::player_event_parser::DeleteContext::Consumed,
        };
        agg.process_event(&mut delete_event, &conn, "Zenith", "Dreva", Some("Serbule"));

        let session_id = persistence::active_session(&conn, "Zenith", "Dreva")
            .unwrap()
            .unwrap()
            .id;
        let use_id = persistence::uses_for_session(&conn, session_id).unwrap()[0].id;

        let tagged: Vec<(String, Option<String>, Option<i64>)> = conn
            .prepare(
                "SELECT item_name, source_kind,
                        CAST(json_extract(source_details, '$.survey_use_id') AS INTEGER)
                 FROM item_transactions ORDER BY id",
            )
            .unwrap()
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap();
        assert_eq!(
            tagged[0].1.as_deref(),
            Some("survey_chat"),
            "in-window row claimed"
        );
        assert_eq!(tagged[0].2, Some(use_id));
        assert_eq!(tagged[1].1, None, "out-of-window row untouched");
        assert_eq!(tagged[2].1, None, "survey-map craft row untouched");

        let su = persistence::get_use(&conn, use_id).unwrap().unwrap();
        assert_eq!(su.loot_qty, 9, "claimed quantity lands on the use");
    }

    #[test]
    fn test_dual_pipeline_gain_bumps_loot_qty_once() {
        // One game gain seen by BOTH logs: the Player.log path (direct
        // SurveyMapUse gain -> tagged row) bumps first, then the chat twin
        // arrives in the same-tick Basic window. loot_qty must bump once;
        // the chat row is still tagged (per-row authority) and the summary
        // guard still counts the item once.
        let conn = fresh_db();
        let mut agg = SurveySessionAggregator::new(game_data_with_survey_maps());
        agg.set_base_date(test_base_date());

        // 1. Consume the Basic map -> use + pending.
        let mut delete_event = PlayerEvent::ItemDeleted {
            timestamp: "12:00:00".to_string(),
            instance_id: 1,
            item_name: Some("GeologySurveySerbule1".to_string()),
            context: crate::player_event_parser::DeleteContext::Consumed,
        };
        agg.process_event(&mut delete_event, &conn, "Zenith", "Dreva", Some("Serbule"));
        let session_id = persistence::active_session(&conn, "Zenith", "Dreva")
            .unwrap()
            .unwrap()
            .id;
        let use_id = persistence::uses_for_session(&conn, session_id).unwrap()[0].id;

        // 2. Player.log gain (primary loot in the same tick): bumps loot_qty
        //    and writes the tagged Player.log row the live ledger would.
        let mut gain_event = PlayerEvent::ItemAdded {
            timestamp: "12:00:00".to_string(),
            item_name: "Fluorite".to_string(),
            instance_id: 999,
            slot_index: -1,
            is_new: true,
            initial_quantity: 3,
            provenance: ItemProvenance::Attributed {
                source: ActivitySource::SurveyMapUse {
                    survey_map_internal_name: Some("Serbule Blue Mineral Survey".to_string()),
                },
                confidence: crate::player_event_parser::AttributionConfidence::Confident,
                survey_use_id: None,
            },
        };
        agg.process_event(&mut gain_event, &conn, "Zenith", "Dreva", Some("Serbule"));
        let details = format!(r#"{{"survey_use_id":{use_id}}}"#);
        conn.execute(
            "INSERT INTO item_transactions (timestamp, character_name, server_name, item_name, internal_name, quantity, context, source, source_kind, source_details)
             VALUES ('2026-04-15 12:00:01','Zenith','Dreva','Fluorite','Fluorite',3,'loot','player_log','survey_map_use',?1)",
            rusqlite::params![details],
        )
        .unwrap();
        let su = persistence::get_use(&conn, use_id).unwrap().unwrap();
        assert_eq!(su.loot_qty, 3, "Player.log gain bumps loot_qty");

        // 3. Chat twin of the same gain arrives in the same-tick window.
        let attributed = agg.attribute_chat_gain(
            &conn,
            "Zenith",
            "Dreva",
            Some("Fluorite"),
            3,
            "2026-04-15 12:00:01",
        );
        assert_eq!(attributed, Some(use_id), "chat twin still attributes");
        conn.execute(
            "INSERT INTO item_transactions (timestamp, character_name, server_name, item_name, internal_name, quantity, context, source, source_kind, source_details)
             VALUES ('2026-04-15 12:00:01','Zenith','Dreva','Fluorite','Fluorite',3,'loot','chat_status','survey_chat',?1)",
            rusqlite::params![details],
        )
        .unwrap();

        let su = persistence::get_use(&conn, use_id).unwrap().unwrap();
        assert_eq!(su.loot_qty, 3, "dual-pipeline gain bumps loot_qty once");

        // 4. Summary counts the item once via the chat-authoritative guard.
        let total: i64 = conn
            .query_row(
                "SELECT COALESCE(SUM(t.quantity), 0) FROM item_transactions t
                 JOIN survey_uses u ON u.session_id = ?1 AND u.id = t.survey_use_id
                 WHERE t.quantity > 0 AND t.item_name = 'Fluorite'
                   AND (t.source_kind = 'survey_chat'
                        OR NOT EXISTS (SELECT 1 FROM item_transactions c
                                       WHERE c.source_kind = 'survey_chat'
                                         AND c.survey_use_id = u.id))",
                rusqlite::params![session_id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(total, 3, "summary counts the gain once");
    }

    #[test]
    fn test_player_side_guard_tiebreaks_multi_item_same_tick() {
        // Regression: two different items with the same qty in the same tick
        // must each bump loot_qty once, even when the chat pipeline saw one
        // of them first. Without the item tie-break, the second Player.log
        // gain matches the first gain's chat twin (same use/qty/time) and
        // its bump is skipped → systematic undercount of Motherlode loot.
        // Motherlode path: the node stays adopted across gains (no Basic
        // completion orphaning the second gain), matching the real trigger.
        let conn = fresh_db();
        let mut agg = SurveySessionAggregator::new(game_data_with_survey_maps());
        agg.set_base_date(test_base_date());

        // 1. Consume a Motherlode map -> pending use.
        let mut delete_event = PlayerEvent::ItemDeleted {
            timestamp: "12:00:00".to_string(),
            instance_id: 1,
            item_name: Some("MiningSurveyKurMountains1X".to_string()),
            context: crate::player_event_parser::DeleteContext::Consumed,
        };
        agg.process_event(
            &mut delete_event,
            &conn,
            "Zenith",
            "Dreva",
            Some("KurMountains"),
        );
        let session_id = persistence::active_session(&conn, "Zenith", "Dreva")
            .unwrap()
            .unwrap()
            .id;
        let use_id = persistence::uses_for_session(&conn, session_id).unwrap()[0].id;

        let mining_prov = |node_id: u32| ItemProvenance::Attributed {
            source: ActivitySource::Mining {
                node_entity_id: Some(node_id),
                node_name: Some("MiningNodeFromSurvey9".to_string()),
            },
            confidence: crate::player_event_parser::AttributionConfidence::Confident,
            survey_use_id: None,
        };
        let mut gain = |name: &str| PlayerEvent::ItemAdded {
            timestamp: "12:00:29".to_string(),
            item_name: name.to_string(),
            instance_id: 999,
            slot_index: -1,
            is_new: true,
            initial_quantity: 1,
            provenance: mining_prov(435138),
        };
        let details = format!(r#"{{"survey_use_id":{use_id}}}"#);
        let tag_chat_row = |conn: &Connection, name: &str| {
            conn.execute(
                "INSERT INTO item_transactions (timestamp, character_name, server_name, item_name, internal_name, quantity, context, source, source_kind, source_details)
                 VALUES ('2026-04-15 12:00:29','Zenith','Dreva',?1,?1,1,'loot','chat_status','survey_chat',?2)",
                rusqlite::params![name, details],
            )
            .unwrap();
        };

        // Interleave Player-A, Chat-A, Player-B as SEQ-ORDERED pipelines would:
        // the chat twin of A must not suppress the Player.log bump for B.
        let mut gain_a = gain("Fluorite");
        agg.process_event(&mut gain_a, &conn, "Zenith", "Dreva", Some("KurMountains"));
        tag_chat_row(&conn, "Fluorite");
        let mut gain_b = gain("Azurite");
        agg.process_event(&mut gain_b, &conn, "Zenith", "Dreva", Some("KurMountains"));

        let su = persistence::get_use(&conn, use_id).unwrap().unwrap();
        assert_eq!(
            su.loot_qty, 2,
            "each same-tick Player.log gain bumps once despite the other item's chat twin"
        );
    }

    #[test]
    fn test_basic_survey_attribution_marks_use_completed() {
        // Basic survey: same-tick gain attributed via SurveyMapUse provenance
        // should land the survey_use_id link AND mark the use completed.
        let conn = fresh_db();
        let mut agg = SurveySessionAggregator::new(game_data_with_survey_maps());

        // 1. Consume the survey map
        let mut delete_event = PlayerEvent::ItemDeleted {
            timestamp: "12:00:00".to_string(),
            instance_id: 1,
            item_name: Some("GeologySurveySerbule1".to_string()),
            context: crate::player_event_parser::DeleteContext::Consumed,
        };
        agg.process_event(&mut delete_event, &conn, "Zenith", "Dreva", Some("Serbule"));

        // 2. Loot arrives in same tick as a SurveyMapUse-attributed gain
        let mut gain_event = PlayerEvent::ItemAdded {
            timestamp: "12:00:00".to_string(),
            item_name: "Fluorite".to_string(),
            instance_id: 999,
            slot_index: -1,
            is_new: true,
            initial_quantity: 1,
            provenance: ItemProvenance::Attributed {
                source: ActivitySource::SurveyMapUse {
                    survey_map_internal_name: Some("Serbule Blue Mineral Survey".to_string()),
                },
                confidence: crate::player_event_parser::AttributionConfidence::Confident,
                survey_use_id: None,
            },
        };
        agg.process_event(&mut gain_event, &conn, "Zenith", "Dreva", Some("Serbule"));

        // Provenance should now carry the survey_use_id
        if let PlayerEvent::ItemAdded { provenance, .. } = &gain_event {
            match provenance {
                ItemProvenance::Attributed {
                    survey_use_id: Some(_),
                    ..
                } => {}
                _ => panic!("expected survey_use_id to be set; got {:?}", provenance),
            }
        }

        // Use should be marked completed
        let uses = persistence::uses_for_session(
            &conn,
            persistence::active_session(&conn, "Zenith", "Dreva")
                .unwrap()
                .unwrap()
                .id,
        )
        .unwrap();
        assert_eq!(uses[0].status, SurveyUseStatus::Completed);
        assert_eq!(uses[0].loot_qty, 1);
    }

    #[test]
    fn test_basic_survey_deferred_gain_before_delete() {
        // Real game log ordering: primary loot arrives BEFORE the survey
        // map deletion. The aggregator must buffer the gain and attribute
        // it once the DeleteItem fires and creates the pending use.
        let conn = fresh_db();
        let mut agg = SurveySessionAggregator::new(game_data_with_survey_maps());

        // 1. Primary loot arrives first — no pending use exists yet.
        let mut gain = PlayerEvent::ItemAdded {
            timestamp: "12:00:00".to_string(),
            item_name: "Fluorite".to_string(),
            instance_id: 999,
            slot_index: -1,
            is_new: true,
            initial_quantity: 1,
            provenance: ItemProvenance::Attributed {
                source: ActivitySource::SurveyMapUse {
                    survey_map_internal_name: Some("Serbule Blue Mineral Survey".to_string()),
                },
                confidence: crate::player_event_parser::AttributionConfidence::Confident,
                survey_use_id: None,
            },
        };
        agg.process_event(&mut gain, &conn, "Zenith", "Dreva", Some("Serbule"));

        // No session or use yet — the gain is buffered.
        assert!(
            persistence::active_session(&conn, "Zenith", "Dreva")
                .unwrap()
                .is_none(),
            "no session before DeleteItem"
        );
        assert_eq!(agg.deferred_basic_gains.len(), 1);

        // 2. Survey map consumed — creates the pending use and drains buffer.
        let mut delete = PlayerEvent::ItemDeleted {
            timestamp: "12:00:00".to_string(),
            instance_id: 1,
            item_name: Some("GeologySurveySerbule1".to_string()),
            context: crate::player_event_parser::DeleteContext::Consumed,
        };
        agg.process_event(&mut delete, &conn, "Zenith", "Dreva", Some("Serbule"));

        // Buffer should be drained.
        assert_eq!(agg.deferred_basic_gains.len(), 0);

        // The use should have loot_qty = 1 from the deferred gain.
        let session = persistence::active_session(&conn, "Zenith", "Dreva")
            .unwrap()
            .expect("session after DeleteItem");
        let uses = persistence::uses_for_session(&conn, session.id).unwrap();
        assert_eq!(uses.len(), 1);
        assert_eq!(uses[0].loot_qty, 1, "deferred gain should be counted");

        // 3. Bonus loot arrives after delete — should attribute normally
        // and mark use completed.
        let mut bonus = PlayerEvent::ItemAdded {
            timestamp: "12:00:00".to_string(),
            item_name: "Azurite".to_string(),
            instance_id: 1000,
            slot_index: -1,
            is_new: true,
            initial_quantity: 1,
            provenance: ItemProvenance::Attributed {
                source: ActivitySource::SurveyMapUse {
                    survey_map_internal_name: Some("Serbule Blue Mineral Survey".to_string()),
                },
                confidence: crate::player_event_parser::AttributionConfidence::Confident,
                survey_use_id: None,
            },
        };
        agg.process_event(&mut bonus, &conn, "Zenith", "Dreva", Some("Serbule"));

        let uses = persistence::uses_for_session(&conn, session.id).unwrap();
        assert_eq!(uses[0].loot_qty, 2, "deferred + normal gain");
        assert_eq!(uses[0].status, SurveyUseStatus::Completed);
    }

    #[test]
    fn test_deferred_drain_tiebreaks_multi_item_same_tick() {
        // Regression: two buffered gains sharing qty/time but with different
        // items must each count, even when the chat pipeline saw only one of
        // them. Without the item tie-break, both deferred gains match the
        // single untagged chat row and both skips fire → the claim step below
        // bumps only once (the chat row), undercounting the ledger twin.
        let conn = fresh_db();
        let mut agg = SurveySessionAggregator::new(game_data_with_survey_maps());
        agg.set_base_date(test_base_date());

        let mut gain = |name: &str| PlayerEvent::ItemAdded {
            timestamp: "12:00:00".to_string(),
            item_name: name.to_string(),
            instance_id: 999,
            slot_index: -1,
            is_new: true,
            initial_quantity: 1,
            provenance: ItemProvenance::Attributed {
                source: ActivitySource::SurveyMapUse {
                    survey_map_internal_name: Some("Serbule Blue Mineral Survey".to_string()),
                },
                confidence: crate::player_event_parser::AttributionConfidence::Confident,
                survey_use_id: None,
            },
        };
        // Both gains arrive before the DeleteItem → buffered.
        let mut gain_a = gain("Fluorite");
        agg.process_event(&mut gain_a, &conn, "Zenith", "Dreva", Some("Serbule"));
        let mut gain_b = gain("Azurite");
        agg.process_event(&mut gain_b, &conn, "Zenith", "Dreva", Some("Serbule"));
        assert_eq!(agg.deferred_basic_gains.len(), 2);

        // Chat saw only the Fluorite twin (untagged row, same tick/qty).
        conn.execute(
            "INSERT INTO item_transactions (timestamp, character_name, server_name, item_name, internal_name, quantity, context, source)
             VALUES ('2026-04-15 12:00:00','Zenith','Dreva','Fluorite','Fluorite',1,'loot','chat_status')",
            [],
        )
        .unwrap();

        // DeleteItem drains the buffer: Fluorite skips (chat claims it),
        // Azurite bumps here → total 1 + 1 = 2.
        let mut delete = PlayerEvent::ItemDeleted {
            timestamp: "12:00:00".to_string(),
            instance_id: 1,
            item_name: Some("GeologySurveySerbule1".to_string()),
            context: crate::player_event_parser::DeleteContext::Consumed,
        };
        agg.process_event(&mut delete, &conn, "Zenith", "Dreva", Some("Serbule"));
        let session = persistence::active_session(&conn, "Zenith", "Dreva")
            .unwrap()
            .expect("session after DeleteItem");
        let uses = persistence::uses_for_session(&conn, session.id).unwrap();
        assert_eq!(uses.len(), 1);
        assert_eq!(
            uses[0].loot_qty, 2,
            "chat-twin gain claimed (1) + untwinned gain bumped (1)"
        );
    }

    #[test]
    fn test_manual_use_drains_only_matching_map_gains() {
        // Major regression: a manual Basic use must not consume deferred
        // gains buffered for a different map. Buffer a Serbule gain (no
        // delete yet), record a manual use for an Ilmari map, and assert
        // the Serbule gain stays buffered while the manual use gets none.
        let conn = fresh_db();
        let mut agg = SurveySessionAggregator::new(game_data_with_survey_maps());
        agg.set_base_date(test_base_date());

        // 1. Serbule primary loot arrives before its DeleteItem → buffered.
        let mut gain = PlayerEvent::ItemAdded {
            timestamp: "12:00:00".to_string(),
            item_name: "Fluorite".to_string(),
            instance_id: 999,
            slot_index: -1,
            is_new: true,
            initial_quantity: 1,
            provenance: ItemProvenance::Attributed {
                source: ActivitySource::SurveyMapUse {
                    survey_map_internal_name: Some("Serbule Blue Mineral Survey".to_string()),
                },
                confidence: crate::player_event_parser::AttributionConfidence::Confident,
                survey_use_id: None,
            },
        };
        agg.process_event(&mut gain, &conn, "Zenith", "Dreva", Some("Serbule"));
        assert_eq!(agg.deferred_basic_gains.len(), 1);

        // 2. Manual use for a DIFFERENT map — must not drain the buffer.
        let (use_id, _) = agg
            .record_manual_use(
                &conn,
                "Zenith",
                "Dreva",
                Some("Ilmari"),
                "GeologySurveyIlmari1",
                crate::survey::types::SurveyUseKind::Basic,
                "2026-04-15 12:00:05",
            )
            .expect("manual use records");
        assert_eq!(
            agg.deferred_basic_gains.len(),
            1,
            "other-map gain stays buffered for its own delete"
        );
        let uses = persistence::uses_for_session(
            &conn,
            persistence::active_session(&conn, "Zenith", "Dreva")
                .unwrap()
                .unwrap()
                .id,
        )
        .unwrap();
        let manual_use = uses.iter().find(|u| u.id == use_id).unwrap();
        assert_eq!(manual_use.loot_qty, 0, "manual use gets no other-map loot");
        assert_eq!(manual_use.status, SurveyUseStatus::PendingLoot);

        // 3. Serbule's own DeleteItem still drains its gain onto the
        // Serbule use — and leaves the Ilmari manual use untouched.
        let mut delete = PlayerEvent::ItemDeleted {
            timestamp: "12:00:06".to_string(),
            instance_id: 1,
            item_name: Some("GeologySurveySerbule1".to_string()),
            context: crate::player_event_parser::DeleteContext::Consumed,
        };
        agg.process_event(&mut delete, &conn, "Zenith", "Dreva", Some("Serbule"));
        assert_eq!(agg.deferred_basic_gains.len(), 0);
        let session_id = persistence::active_session(&conn, "Zenith", "Dreva")
            .unwrap()
            .unwrap()
            .id;
        let uses = persistence::uses_for_session(&conn, session_id).unwrap();
        assert_eq!(uses.len(), 2);
        let serbule_use = uses.iter().find(|u| u.id != use_id).unwrap();
        assert_eq!(serbule_use.loot_qty, 1, "Serbule gain lands on Serbule use");
        let manual_after = uses.iter().find(|u| u.id == use_id).unwrap();
        assert_eq!(manual_after.loot_qty, 0, "Ilmari manual still has none");
        assert_eq!(manual_after.status, SurveyUseStatus::PendingLoot);
    }

    #[test]
    fn test_manual_use_drains_matching_gain_untwinned() {
        // Same-map manual use with a buffered player.log-only gain: the
        // drain must flush qty to the use and mark it Completed (the
        // deferred buffer exists exactly for this log-ordering case).
        let conn = fresh_db();
        let mut agg = SurveySessionAggregator::new(game_data_with_survey_maps());
        agg.set_base_date(test_base_date());

        let mut gain = PlayerEvent::ItemAdded {
            timestamp: "12:00:00".to_string(),
            item_name: "Fluorite".to_string(),
            instance_id: 999,
            slot_index: -1,
            is_new: true,
            initial_quantity: 1,
            provenance: ItemProvenance::Attributed {
                source: ActivitySource::SurveyMapUse {
                    survey_map_internal_name: Some("Serbule Blue Mineral Survey".to_string()),
                },
                confidence: crate::player_event_parser::AttributionConfidence::Confident,
                survey_use_id: None,
            },
        };
        agg.process_event(&mut gain, &conn, "Zenith", "Dreva", Some("Serbule"));
        assert_eq!(agg.deferred_basic_gains.len(), 1);

        let (use_id, _) = agg
            .record_manual_use(
                &conn,
                "Zenith",
                "Dreva",
                Some("Serbule"),
                "GeologySurveySerbule1",
                crate::survey::types::SurveyUseKind::Basic,
                "2026-04-15 12:00:05",
            )
            .expect("manual use records");
        assert_eq!(agg.deferred_basic_gains.len(), 0, "matching gain drained");
        let uses = persistence::uses_for_session(
            &conn,
            persistence::active_session(&conn, "Zenith", "Dreva")
                .unwrap()
                .unwrap()
                .id,
        )
        .unwrap();
        let manual_use = uses.iter().find(|u| u.id == use_id).unwrap();
        assert_eq!(manual_use.loot_qty, 1, "buffered gain flushed to manual use");
        assert_eq!(manual_use.status, SurveyUseStatus::Completed);
    }

    #[test]
    fn test_manual_use_matching_gain_with_chat_twin_links_player_row() {
        // Same-map manual use where the buffered gain has an unclaimed chat
        // twin: qty comes via the claim step, and the player.log row must
        // still be tagged with this use_id (map-scoped retro-tag).
        let conn = fresh_db();
        let mut agg = SurveySessionAggregator::new(game_data_with_survey_maps());
        agg.set_base_date(test_base_date());

        // Seed the untagged chat twin the drain checks for, plus the
        // player.log row GameStateManager would have written for the gain.
        conn.execute(
            "INSERT INTO item_transactions (timestamp, character_name, server_name, item_name, quantity, context, source, source_kind, source_details)
             VALUES ('2026-04-15 12:00:00','Zenith','Dreva','Fluorite',1,'loot','chat_status','survey_chat',NULL)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO item_transactions (timestamp, character_name, server_name, item_name, quantity, context, source, source_kind, source_details)
             VALUES ('2026-04-15 12:00:00','Zenith','Dreva','Fluorite',1,'loot','player_log','survey_map_use',
                     json_object('survey_map_internal_name', 'Serbule Blue Mineral Survey'))",
            [],
        )
        .unwrap();
        let mut gain = PlayerEvent::ItemAdded {
            timestamp: "12:00:00".to_string(),
            item_name: "Fluorite".to_string(),
            instance_id: 999,
            slot_index: -1,
            is_new: true,
            initial_quantity: 1,
            provenance: ItemProvenance::Attributed {
                source: ActivitySource::SurveyMapUse {
                    survey_map_internal_name: Some("Serbule Blue Mineral Survey".to_string()),
                },
                confidence: crate::player_event_parser::AttributionConfidence::Confident,
                survey_use_id: None,
            },
        };
        agg.process_event(&mut gain, &conn, "Zenith", "Dreva", Some("Serbule"));
        assert_eq!(agg.deferred_basic_gains.len(), 1);

        let (use_id, _) = agg
            .record_manual_use(
                &conn,
                "Zenith",
                "Dreva",
                Some("Serbule"),
                "GeologySurveySerbule1",
                crate::survey::types::SurveyUseKind::Basic,
                "2026-04-15 12:00:05",
            )
            .expect("manual use records");
        assert_eq!(agg.deferred_basic_gains.len(), 0, "twinned gain consumed");
        let uses = persistence::uses_for_session(
            &conn,
            persistence::active_session(&conn, "Zenith", "Dreva")
                .unwrap()
                .unwrap()
                .id,
        )
        .unwrap();
        let manual_use = uses.iter().find(|u| u.id == use_id).unwrap();
        assert_eq!(manual_use.loot_qty, 1, "twinned gain counted via claim");
        assert_eq!(manual_use.status, SurveyUseStatus::Completed);
        // The player.log row was tagged by the map-scoped retro-tag even
        // though its qty came via the chat claim.
        let player_row_use: Option<i64> = conn
            .query_row(
                "SELECT json_extract(source_details, '$.survey_use_id') FROM item_transactions
                 WHERE source = 'player_log' AND source_kind = 'survey_map_use'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(player_row_use, Some(use_id), "player.log row linked to manual use");
    }

    #[test]
    fn test_basic_survey_speed_bonus_marks_bonus_transaction() {
        // Speed-bonus ScreenText arrives *after* the bonus gain has been
        // recorded. The aggregator should patch the bonus row's
        // source_details to include is_speed_bonus: true, leaving the
        // primary row untouched.
        let conn = fresh_db();
        let mut agg = SurveySessionAggregator::new(game_data_with_survey_maps());

        // 1. Consume the Basic survey map.
        let mut delete_event = PlayerEvent::ItemDeleted {
            timestamp: "12:00:00".to_string(),
            instance_id: 1,
            item_name: Some("GeologySurveySerbule1".to_string()),
            context: crate::player_event_parser::DeleteContext::Consumed,
        };
        agg.process_event(&mut delete_event, &conn, "Zenith", "Dreva", Some("Serbule"));

        // Pull the allocated use_id so we can seed transactions that mirror
        // what game_state would have written.
        let session_id = persistence::active_session(&conn, "Zenith", "Dreva")
            .unwrap()
            .unwrap()
            .id;
        let uses = persistence::uses_for_session(&conn, session_id).unwrap();
        let use_id = uses[0].id;

        // 2. Primary gain — provenance gets survey_use_id injected.
        let mut primary = PlayerEvent::ItemAdded {
            timestamp: "12:00:00".to_string(),
            item_name: "Blue Spinel".to_string(),
            instance_id: 900,
            slot_index: -1,
            is_new: true,
            initial_quantity: 1,
            provenance: ItemProvenance::Attributed {
                source: ActivitySource::SurveyMapUse {
                    survey_map_internal_name: Some("Serbule Blue Mineral Survey".to_string()),
                },
                confidence: crate::player_event_parser::AttributionConfidence::Confident,
                survey_use_id: None,
            },
        };
        agg.process_event(&mut primary, &conn, "Zenith", "Dreva", Some("Serbule"));

        // Seed transactions that game_state would have written for both items
        // — this test doesn't run game_state, so we fake the rows directly.
        let details = format!(r#"{{"survey_use_id":{}}}"#, use_id);
        conn.execute(
            "INSERT INTO item_transactions (timestamp, character_name, server_name, item_name, quantity, context, source, source_kind, source_details)
             VALUES ('2026-04-15 12:00:00','Zenith','Dreva','Blue Spinel',1,'loot','player_log','survey_map_use',?1),
                    ('2026-04-15 12:00:00','Zenith','Dreva','Rubywall Crystal',2,'loot','player_log','survey_map_use',?1)",
            rusqlite::params![details],
        ).unwrap();

        // 3. ScreenText with "(speed bonus!)" arrives last in the tick.
        let mut screen = PlayerEvent::ScreenText {
            timestamp: "12:00:00".to_string(),
            category: "ImportantInfo".to_string(),
            message: "Blue Spinel collected! Also found Rubywall Crystal x2 (speed bonus!)"
                .to_string(),
        };
        agg.process_event(&mut screen, &conn, "Zenith", "Dreva", Some("Serbule"));

        // Verify: Rubywall flagged, Blue Spinel not.
        let flags: Vec<(String, Option<i64>)> = conn
            .prepare("SELECT item_name, json_extract(source_details, '$.is_speed_bonus') FROM item_transactions ORDER BY id")
            .unwrap()
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap();
        assert_eq!(flags[0].0, "Blue Spinel");
        assert_eq!(flags[0].1, None, "primary should not be flagged");
        assert_eq!(flags[1].0, "Rubywall Crystal");
        assert_eq!(flags[1].1, Some(1), "bonus should be flagged");
    }

    #[test]
    fn test_motherlode_survey_attributes_first_mining_gain() {
        // Motherlode: consume map → wait for Mining context → gains from
        // the same mining cycle all attribute to the same use. Completion
        // is deferred until a different mining node starts.
        let conn = fresh_db();
        let mut agg = SurveySessionAggregator::new(game_data_with_survey_maps());

        // 1. Consume motherlode map
        let mut consume = PlayerEvent::ItemDeleted {
            timestamp: "18:04:23".to_string(),
            instance_id: 1,
            item_name: Some("MiningSurveyKurMountains1X".to_string()),
            context: crate::player_event_parser::DeleteContext::Consumed,
        };
        agg.process_event(&mut consume, &conn, "Zenith", "Dreva", Some("KurMountains"));

        let session_id = persistence::active_session(&conn, "Zenith", "Dreva")
            .unwrap()
            .unwrap()
            .id;
        let use_id = persistence::uses_for_session(&conn, session_id).unwrap()[0].id;

        // 2. Mining starts (no-op for the aggregator; node id flows in via gain)
        let mut mining = PlayerEvent::DelayLoopStarted {
            timestamp: "18:04:23".to_string(),
            duration: 6.0,
            action_type: "ChopLumber".to_string(),
            label: "Mining...".to_string(),
            entity_id: 0,
            abort_condition: "AbortIfAttacked".to_string(),
        };
        agg.process_event(&mut mining, &conn, "Zenith", "Dreva", Some("KurMountains"));

        let mining_prov = |node_id: u32| ItemProvenance::Attributed {
            source: ActivitySource::Mining {
                node_entity_id: Some(node_id),
                node_name: Some("MiningNodeFromSurvey9".to_string()),
            },
            confidence: crate::player_event_parser::AttributionConfidence::Confident,
            survey_use_id: None,
        };

        // 3. First mining gain — should adopt the pending use but NOT mark
        //    it completed yet (Motherlode can yield multiple items in one swing).
        let mut gain1 = PlayerEvent::ItemAdded {
            timestamp: "18:04:29".to_string(),
            item_name: "Orichalcum".to_string(),
            instance_id: 1234,
            slot_index: -1,
            is_new: true,
            initial_quantity: 1,
            provenance: mining_prov(435138),
        };
        agg.process_event(&mut gain1, &conn, "Zenith", "Dreva", Some("KurMountains"));

        // survey_use_id should be injected on the first gain
        if let PlayerEvent::ItemAdded { provenance, .. } = &gain1 {
            if let ItemProvenance::Attributed { survey_use_id, .. } = provenance {
                assert_eq!(*survey_use_id, Some(use_id));
            } else {
                panic!("expected Attributed");
            }
        }

        // Use should still be pending_loot (deferred completion)
        let u = persistence::get_use(&conn, use_id).unwrap().unwrap();
        assert_eq!(u.status, SurveyUseStatus::PendingLoot);
        assert_eq!(u.loot_qty, 1);

        // 4. Second gain in the same mining cycle — same node, same timestamp.
        //    Should also attribute to the same use.
        let mut gain2 = PlayerEvent::ItemStackChanged {
            timestamp: "18:04:29".to_string(),
            instance_id: 5678,
            item_name: Some("PebbleMix".to_string()),
            item_type_id: 99,
            old_stack_size: 5,
            new_stack_size: 8,
            delta: 3,
            from_server: true,
            provenance: mining_prov(435138),
        };
        agg.process_event(&mut gain2, &conn, "Zenith", "Dreva", Some("KurMountains"));

        if let PlayerEvent::ItemStackChanged { provenance, .. } = &gain2 {
            if let ItemProvenance::Attributed { survey_use_id, .. } = provenance {
                assert_eq!(
                    *survey_use_id,
                    Some(use_id),
                    "second gain should use same use_id"
                );
            } else {
                panic!("expected Attributed on second gain");
            }
        }

        let u2 = persistence::get_use(&conn, use_id).unwrap().unwrap();
        assert_eq!(u2.loot_qty, 4, "1 + 3 from the two gains");
        assert_eq!(u2.status, SurveyUseStatus::PendingLoot);

        // 5. Gain on the same node but >1 second later — window expired,
        //    should NOT attribute to the motherlode use.
        let mut gain_late = PlayerEvent::ItemAdded {
            timestamp: "18:04:31".to_string(), // 2 seconds after first gain
            item_name: "Pebbles".to_string(),
            instance_id: 9999,
            slot_index: -1,
            is_new: true,
            initial_quantity: 1,
            provenance: mining_prov(435138), // same node
        };
        agg.process_event(
            &mut gain_late,
            &conn,
            "Zenith",
            "Dreva",
            Some("KurMountains"),
        );

        let u3 = persistence::get_use(&conn, use_id).unwrap().unwrap();
        assert_eq!(
            u3.status,
            SurveyUseStatus::Completed,
            "window expiry closes the motherlode"
        );
        assert_eq!(u3.loot_qty, 4, "late gain not counted");
    }

    #[test]
    fn test_multihit_survey_opens_node_and_chains_subsequent_hits() {
        // Multihit: consume map → first mining gain opens a row in
        // open_multihit_nodes → subsequent gains on the same entity are
        // attributed to the same use (loot accumulates).
        let conn = fresh_db();
        let mut agg = SurveySessionAggregator::new(game_data_with_survey_maps());

        // 1. Consume Multihit map
        let mut consume = PlayerEvent::ItemDeleted {
            timestamp: "12:00:00".to_string(),
            instance_id: 1,
            item_name: Some("MiningSurveyPovus7Y".to_string()),
            context: crate::player_event_parser::DeleteContext::Consumed,
        };
        agg.process_event(&mut consume, &conn, "Zenith", "Dreva", Some("Povus"));

        let session_id = persistence::active_session(&conn, "Zenith", "Dreva")
            .unwrap()
            .unwrap()
            .id;
        let use_id = persistence::uses_for_session(&conn, session_id).unwrap()[0].id;

        // 2. First mining gain on node 9999 — opens the open_multihit_nodes row
        let mut hit1 = PlayerEvent::ItemAdded {
            timestamp: "12:00:10".to_string(),
            item_name: "Marvelous Metal Slab".to_string(),
            instance_id: 1001,
            slot_index: -1,
            is_new: true,
            initial_quantity: 1,
            provenance: ItemProvenance::Attributed {
                source: ActivitySource::Mining {
                    node_entity_id: Some(9999),
                    node_name: None,
                },
                confidence: crate::player_event_parser::AttributionConfidence::Confident,
                survey_use_id: None,
            },
        };
        agg.process_event(&mut hit1, &conn, "Zenith", "Dreva", Some("Povus"));

        // open_multihit_nodes should now have a row
        let node = multihit_state::get_node(&conn, "Zenith", "Dreva", 9999)
            .unwrap()
            .expect("node row should exist");
        assert_eq!(node.survey_use_id, use_id);

        // 3. Second hit on same node — should NOT pop another pending use,
        // should chain via the existing open_multihit_nodes row
        let mut hit2 = PlayerEvent::ItemAdded {
            timestamp: "12:00:16".to_string(),
            item_name: "Pebbles".to_string(),
            instance_id: 1002,
            slot_index: -1,
            is_new: true,
            initial_quantity: 1,
            provenance: ItemProvenance::Attributed {
                source: ActivitySource::Mining {
                    node_entity_id: Some(9999),
                    node_name: None,
                },
                confidence: crate::player_event_parser::AttributionConfidence::Confident,
                survey_use_id: None,
            },
        };
        agg.process_event(&mut hit2, &conn, "Zenith", "Dreva", Some("Povus"));

        // Both gains should have the same survey_use_id and loot_qty=2
        let u = persistence::get_use(&conn, use_id).unwrap().unwrap();
        assert_eq!(u.loot_qty, 2);
        // Status still pending — multihit doesn't auto-complete
        assert_eq!(u.status, SurveyUseStatus::PendingLoot);

        // last_hit_at should have advanced past the original opened_at
        let updated_node = multihit_state::get_node(&conn, "Zenith", "Dreva", 9999)
            .unwrap()
            .unwrap();
        assert!(
            updated_node.last_hit_at > updated_node.opened_at,
            "last_hit_at ({}) should be later than opened_at ({})",
            updated_node.last_hit_at,
            updated_node.opened_at
        );
    }

    #[test]
    fn test_fromsurvey_name_binds_after_grace_expiry() {
        // Long-walk case: map use falls outside the 60 s pending window,
        // but the node's StartInteraction name marks it survey-spawned
        // ("MiningNodeFromSurvey9", x443 in 50x-povusmarvelous fixture).
        // Branch 3 of handle_mining_started must recover the use via
        // latest_recent_nonbasic_use.
        let conn = fresh_db();
        let mut agg = SurveySessionAggregator::new(game_data_with_survey_maps());

        let mut consume = PlayerEvent::ItemDeleted {
            timestamp: "12:00:00".to_string(),
            instance_id: 1,
            item_name: Some("MiningSurveyKurMountains1X".to_string()),
            context: crate::player_event_parser::DeleteContext::Consumed,
        };
        agg.process_event(&mut consume, &conn, "Zenith", "Dreva", Some("KurMountains"));
        let use_id = persistence::uses_for_session(
            &conn,
            persistence::active_session(&conn, "Zenith", "Dreva")
                .unwrap()
                .unwrap()
                .id,
        )
        .unwrap()[0]
            .id;

        // 70 s later the pending queue has aged out, but the use row is
        // still within MULTIHIT_TIMEOUT_SECS for the fallback lookup.
        let interaction = |ts: &str, entity: u32, name: &str| PlayerEvent::InteractionStarted {
            timestamp: ts.to_string(),
            entity_id: entity,
            interaction_type: 7,
            npc_name: name.to_string(),
        };
        let mining_loop = |ts: &str| PlayerEvent::DelayLoopStarted {
            timestamp: ts.to_string(),
            duration: 6.0,
            action_type: "ChopLumber".to_string(),
            label: "Mining...".to_string(),
            entity_id: 0,
            abort_condition: "AbortIfAttacked".to_string(),
        };
        let mut interact = interaction("12:01:10", 435138, "MiningNodeFromSurvey9");
        agg.process_event(&mut interact, &conn, "Zenith", "Dreva", Some("KurMountains"));
        let mut mining = mining_loop("12:01:10");
        agg.process_event(&mut mining, &conn, "Zenith", "Dreva", Some("KurMountains"));
        assert!(
            agg.pending_uses.is_empty(),
            "grace window must have expired so only the FromSurvey fallback can bind"
        );
        let bound = agg.chat_mining_for_test();
        assert_eq!(
            bound,
            Some((435138, use_id)),
            "FromSurvey fallback should adopt the node for the expired use"
        );
    }

    #[test]
    fn test_pending_use_grace_window_expires() {
        // If no Mining context fires within 60 seconds of a Motherlode
        // consume, the pending use is dropped from the in-memory queue.
        let conn = fresh_db();
        let mut agg = SurveySessionAggregator::new(game_data_with_survey_maps());

        let mut consume = PlayerEvent::ItemDeleted {
            timestamp: "12:00:00".to_string(),
            instance_id: 1,
            item_name: Some("MiningSurveyKurMountains1X".to_string()),
            context: crate::player_event_parser::DeleteContext::Consumed,
        };
        agg.process_event(&mut consume, &conn, "Zenith", "Dreva", Some("KurMountains"));
        assert_eq!(agg.pending_uses.len(), 1);

        // 2 minutes later, an unrelated event ticks the expiry
        let mut later = PlayerEvent::DelayLoopStarted {
            timestamp: "12:02:00".to_string(),
            duration: 0.5,
            action_type: "Unset".to_string(),
            label: "Using Something Else".to_string(),
            entity_id: 0,
            abort_condition: "AbortIfAttacked".to_string(),
        };
        agg.process_event(&mut later, &conn, "Zenith", "Dreva", None);

        assert_eq!(
            agg.pending_uses.len(),
            0,
            "pending use should have aged out after 60s grace"
        );
    }

    #[test]
    fn test_parse_secs_of_day() {
        assert_eq!(parse_secs_of_day("00:00:00"), Some(0));
        assert_eq!(parse_secs_of_day("01:02:03"), Some(3723));
        assert_eq!(parse_secs_of_day("23:59:59"), Some(86399));
        assert_eq!(parse_secs_of_day("garbage"), None);
        assert_eq!(parse_secs_of_day("12:30"), None);
    }

    #[test]
    fn test_deferred_basic_gains_bounded_drops_oldest() {
        // Fill the buffer past the cap without a DeleteItem: the oldest
        // gains drop and the buffer stays bounded.
        let conn = fresh_db();
        let mut agg = SurveySessionAggregator::new(game_data_with_survey_maps());
        let gain = |i: usize| PlayerEvent::ItemAdded {
            timestamp: "12:00:00".to_string(),
            // Distinct names per push so the identity check below can tell
            // a dropped oldest from a dropped newest.
            item_name: format!("Fluorite{i}"),
            instance_id: 999 + i as u64,
            slot_index: -1,
            is_new: true,
            initial_quantity: 1,
            provenance: ItemProvenance::Attributed {
                source: ActivitySource::SurveyMapUse {
                    survey_map_internal_name: Some("Serbule Blue Mineral Survey".to_string()),
                },
                confidence: crate::player_event_parser::AttributionConfidence::Confident,
                survey_use_id: None,
            },
        };
        for i in 0..(DEFERRED_BASIC_CAP + 1) {
            let mut g = gain(i);
            agg.process_event(&mut g, &conn, "Zenith", "Dreva", Some("Serbule"));
        }
        assert_eq!(
            agg.deferred_basic_gains.len(),
            DEFERRED_BASIC_CAP,
            "129th push must drop the oldest, keeping the buffer at cap"
        );
        assert_eq!(
            agg.deferred_basic_gains.first().and_then(|g| g.item_name.as_deref()),
            Some("Fluorite1"),
            "oldest entry (Fluorite0) must be the one dropped, not the newest"
        );
    }

    #[test]
    fn test_speed_bonus_pointer_clears_on_next_non_screentext() {
        // The bonus pointer must not survive past the same-tick ScreenText:
        // an intervening non-ScreenText event clears it, so a later marker
        // can't patch stale rows.
        let conn = fresh_db();
        let mut agg = SurveySessionAggregator::new(game_data_with_survey_maps());
        let mut delete_event = PlayerEvent::ItemDeleted {
            timestamp: "12:00:00".to_string(),
            instance_id: 1,
            item_name: Some("GeologySurveySerbule1".to_string()),
            context: crate::player_event_parser::DeleteContext::Consumed,
        };
        agg.process_event(&mut delete_event, &conn, "Zenith", "Dreva", Some("Serbule"));
        let mut primary = PlayerEvent::ItemAdded {
            timestamp: "12:00:00".to_string(),
            item_name: "Blue Spinel".to_string(),
            instance_id: 900,
            slot_index: -1,
            is_new: true,
            initial_quantity: 1,
            provenance: ItemProvenance::Attributed {
                source: ActivitySource::SurveyMapUse {
                    survey_map_internal_name: Some("Serbule Blue Mineral Survey".to_string()),
                },
                confidence: crate::player_event_parser::AttributionConfidence::Confident,
                survey_use_id: None,
            },
        };
        agg.process_event(&mut primary, &conn, "Zenith", "Dreva", Some("Serbule"));
        assert!(
            agg.last_basic_use_for_bonus.is_some(),
            "gain should arm the bonus pointer"
        );
        // Unrelated event clears the pointer.
        let mut other = PlayerEvent::DelayLoopStarted {
            timestamp: "12:00:01".to_string(),
            duration: 0.5,
            action_type: "Unset".to_string(),
            label: "Using Something Else".to_string(),
            entity_id: 0,
            abort_condition: "AbortIfAttacked".to_string(),
        };
        agg.process_event(&mut other, &conn, "Zenith", "Dreva", None);
        assert!(
            agg.last_basic_use_for_bonus.is_none(),
            "non-ScreenText event must clear the bonus pointer"
        );
    }

    #[test]
    fn test_crafted_counts_only_survey_map_craft_provenance() {
        // Non-craft gains of a survey-map item (loot echoes, session-load
        // reconciliation) must not increment crafted_count.
        let conn = fresh_db();
        let mut agg = SurveySessionAggregator::new(game_data_with_survey_maps());
        let mut craft = PlayerEvent::ItemAdded {
            timestamp: "12:00:00".to_string(),
            item_name: "GeologySurveySerbule1".to_string(),
            instance_id: 1,
            slot_index: -1,
            is_new: true,
            initial_quantity: 1,
            provenance: ItemProvenance::Attributed {
                source: ActivitySource::SurveyMapCraft,
                confidence: crate::player_event_parser::AttributionConfidence::Confident,
                survey_use_id: None,
            },
        };
        agg.process_event(&mut craft, &conn, "Zenith", "Dreva", Some("Serbule"));
        let session = persistence::active_session(&conn, "Zenith", "Dreva")
            .unwrap()
            .expect("craft should start a session");
        let full = persistence::get_session(&conn, session.id).unwrap().unwrap();
        assert_eq!(full.crafted_count, Some(1));
        // Same item arriving as plain SurveyMapUse loot must not count.
        let mut echo = PlayerEvent::ItemAdded {
            timestamp: "12:00:01".to_string(),
            item_name: "GeologySurveySerbule1".to_string(),
            instance_id: 2,
            slot_index: -1,
            is_new: true,
            initial_quantity: 1,
            provenance: ItemProvenance::Attributed {
                source: ActivitySource::SurveyMapUse {
                    survey_map_internal_name: None,
                },
                confidence: crate::player_event_parser::AttributionConfidence::Confident,
                survey_use_id: None,
            },
        };
        agg.process_event(&mut echo, &conn, "Zenith", "Dreva", Some("Serbule"));
        let full = persistence::get_session(&conn, session.id).unwrap().unwrap();
        assert_eq!(full.crafted_count, Some(1), "loot echo must not craft-count");
    }

    #[test]
    fn test_multihit_open_emits_opened_event() {
        // Adopting a pending Multihit use must emit MultihitNodeOpened so
        // the coordinator's already-wired arm can refresh the tracker live.
        let conn = fresh_db();
        let mut agg = SurveySessionAggregator::new(game_data_with_survey_maps());
        let mut consume = PlayerEvent::ItemDeleted {
            timestamp: "12:00:00".to_string(),
            instance_id: 1,
            item_name: Some("MiningSurveyPovus7Y".to_string()),
            context: crate::player_event_parser::DeleteContext::Consumed,
        };
        agg.process_event(&mut consume, &conn, "Zenith", "Dreva", Some("Povus"));
        let mining_prov = |node: u32| ItemProvenance::Attributed {
            source: ActivitySource::Mining {
                node_entity_id: Some(node),
                node_name: None,
            },
            confidence: crate::player_event_parser::AttributionConfidence::Confident,
            survey_use_id: None,
        };
        let mut hit = PlayerEvent::ItemAdded {
            timestamp: "12:00:10".to_string(),
            item_name: "Marvelous Metal Slab".to_string(),
            instance_id: 1001,
            slot_index: -1,
            is_new: true,
            initial_quantity: 1,
            provenance: mining_prov(435138),
        };
        let emitted = agg.process_event(&mut hit, &conn, "Zenith", "Dreva", Some("Povus"));
        assert!(
            emitted.iter().any(|e| matches!(
                e,
                SurveyAggregatorEvent::MultihitNodeOpened { node_entity_id: 435138, .. }
            )),
            "expected MultihitNodeOpened for the adopted node: {emitted:?}"
        );
    }

    #[test]
    fn test_record_manual_use_claims_seeded_chat_loot() {
        // Manual "Record use" with a Basic map: untagged chat-loot rows
        // already in the ledger (same tick) attach to the use via
        // claim_unlinked_chat_loot_near, loot_qty bumps, and a second claim
        // returns 0 (idempotent single-loot authority).
        let conn = fresh_db();
        let mut agg = SurveySessionAggregator::new(game_data_with_survey_maps());
        let now = "2026-04-15 12:00:00";
        // Seed untagged chat-status loot rows inside the basic window.
        conn.execute(
            "INSERT INTO item_transactions (timestamp, character_name, server_name, item_name, quantity, context, source, source_kind, source_details)
             VALUES (?1, 'Zenith', 'Dreva', 'Tungsten', 7, 'loot', 'chat_status', NULL, NULL)",
            rusqlite::params![now],
        )
        .unwrap();
        let (use_id, session_id) = agg
            .record_manual_use(
                &conn,
                "Zenith",
                "Dreva",
                Some("Serbule"),
                "GeologySurveySerbule1",
                crate::survey::types::SurveyUseKind::Basic,
                now,
            )
            .expect("record_manual_use must succeed");
        assert!(session_id.is_some(), "manual use always creates a session");
        let su = persistence::get_use(&conn, use_id).unwrap().unwrap();
        assert_eq!(su.map_internal_name, "GeologySurveySerbule1");
        assert_eq!(su.area.as_deref(), Some("Serbule"));
        assert_eq!(su.loot_qty, 7, "seeded chat loot must be claimed");
        let session = persistence::active_session(&conn, "Zenith", "Dreva")
            .unwrap()
            .unwrap();
        assert_eq!(session.id, session_id.unwrap());
        assert_eq!(session.consumed_count, 1);
        // Second claim is a no-op (rows already tagged).
        let again = persistence::claim_unlinked_chat_loot_near(
            &conn,
            use_id,
            "Zenith",
            "Dreva",
            now,
            CHAT_BASIC_WINDOW_SECS as u32,
        )
        .unwrap();
        assert_eq!(again, 0, "re-claim must return 0");
    }

    #[test]
    fn test_record_manual_use_ignores_auto_start_flag() {
        // Explicit user action creates a session even with auto-start off.
        let conn = fresh_db();
        let mut agg = SurveySessionAggregator::new(game_data_with_survey_maps());
        agg.auto_start_enabled = false;
        let (use_id, session_id) = agg
            .record_manual_use(
                &conn,
                "Zenith",
                "Dreva",
                None,
                "MiningSurveyKurMountains1X",
                crate::survey::types::SurveyUseKind::Motherlode,
                "2026-04-15 12:00:00",
            )
            .expect("record_manual_use must succeed");
        let sid = session_id.expect("session must be created despite flag");
        assert!(use_id > 0);
        let su = persistence::get_use(&conn, use_id).unwrap().unwrap();
        assert_eq!(su.session_id, Some(sid));
        assert_eq!(su.area, None, "no live area and empty survey_types.zone → NULL");
    }
}
