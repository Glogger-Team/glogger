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
/// Speed-bonus chain window, Basic surveys: a Basic use whose `used_at`
/// lands within this many seconds after another Basic use for the same
/// identity earned the game's back-to-back speed bonus — its attributed rows
/// stamp `$.is_speed_bonus` at claim time (issue #211). Same 5s value as the
/// loot-tick window above: the game appends bonus rewards on fast
/// re-collection and chat carries no per-item marker since the Player.log
/// `"(speed bonus!)"` signal went away, so chained-use timing is the only
/// non-quantity signal available. NEVER infer bonus from quantity patterns.
const SPEED_BONUS_CHAIN_WINDOW_SECS: u32 = 5;
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
/// lint without changing what it carries. `item_name` is the CDN internal
/// name (loose-matched for twin dedup); `display_name` is the chat display
/// name the ledger row carries — the live speed-bonus stamp matches on it.
#[derive(Debug, Clone, Copy)]
pub struct ChatAttribution<'a> {
    pub character: &'a str,
    pub server: &'a str,
    pub use_id: i64,
    pub quantity: u32,
    pub timestamp: &'a str,
    pub item_name: Option<&'a str>,
    pub display_name: Option<&'a str>,
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
        if let Err(e) = persistence::recompute_session_bounds_and_end(conn, s.id, now_iso) {
            // The caller runs this inside its own transaction: on error it
            // exits before COMMIT, so the close rolls back and the session is
            // open again in the DB. Forget the key entirely (outer None)
            // rather than "no active" — the next lookup must re-read the
            // database after either commit or rollback, never trust the
            // cached absence of a rolled-back close.
            self.cached_active_session = None;
            return Err(e.to_string());
        }
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
        // The creation joins the use-insert transaction below (not a
        // pre-commit here): a failed insert rolls the session back with it
        // instead of orphaning a zero-use session row. A lookup failure
        // propagates (`?`) instead of falling through to `insert_session`:
        // treating "unknown" as "absent" would fragment sessions with a
        // second active row on a transient DB/decode error.
        let session_id = self.fetch_active_session(conn, character, server)?;

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
        // Single write transaction per attempt: the FirstUse session row
        // and the use row commit together — a failed use insert rolls the
        // session back with it instead of orphaning a zero-use session, and
        // a failed count update rolls back the use instead of returning
        // success with a stale session count. No outer transaction is open
        // here (fresh pool checkout in the command), so new_unchecked is safe.
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
            if session_id.is_none() {
                session_id = Some(
                    persistence::insert_session(
                        &tx,
                        character,
                        server,
                        now_iso,
                        SessionStartTrigger::FirstUse,
                        None,
                    )
                    .map_err(|e| e.to_string())?,
                );
            }
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
                    // Cache only the committed session: caching a rolled-back
                    // id would route the next use at a missing row.
                    self.cached_active_session =
                        Some((character.to_string(), server.to_string(), session_id));
                    break id;
                }
                Err(_e) if session_id.is_some() && !retried => {
                    retried = true;
                    drop(tx);
                    self.cached_active_session = None;
                    // Re-resolve only: the loop head creates the
                    // replacement session inside the next attempt's
                    // transaction so it rolls back with a failed retry.
                    session_id = self.fetch_active_session(conn, character, server)?;
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

        // Chat-loot retro-claim for every kind: claim untagged rows already
        // in the ledger within the same-tick window. Live ordering (loot
        // after the use) attributes through `attribute_chat_gain`; this
        // covers the cross order — rows landed before the use was recorded
        // (scan/rescan, or a first yield the user was slow to frame).
        // Design (TODO backlog): extend the claim to non-Basic kinds rather
        // than relaxing the `used_at <= gain` fallback bound — that bound is
        // load-bearing against delayed-gain misattribution
        // (`delayed_gain_lands_on_preceding_use_not_newer_one`). The claim
        // window stays the tight Basic ±5s for every kind: it covers the
        // first-yield-before-framing case without vacuuming unrelated
        // mining/kill loot into Motherlode/Multihit uses. Basic uses queue
        // for the live same-tick window AND complete on the claim;
        // Motherlode/Multihit stay pending — their loot drips across swings
        // and the live identity fallback (windowless) keeps attributing
        // after this. Speed bonus is Basic-only (see survey-mechanics.md).
        if kind == SurveyUseKind::Basic
            && let Some(used_at) = parse_utc_datetime(now_iso)
        {
            remember_bounded(
                &mut self.recent_basic_uses,
                (use_id, used_at, character.to_string(), server.to_string()),
            );
        }
        // One write transaction for the retro-claim: the row links and the
        // quantity/status updates commit together. If any step fails, the
        // rollback leaves the rows untagged so a later pass can reclaim
        // them — never linked rows with a wrong `loot_qty`. Same
        // unchecked-transaction shape as the use insert above: no outer
        // transaction is open here.
        // CDN-down crafted maps carry NULL internal_name on their ledger
        // rows: the retro-claim must apply the display-name arm of the live
        // gate or it vacuums them in as loot. Same shape as
        // `attribute_chat_gain`'s data-available check.
        let cdn_available = match self.game_data.try_read() {
            Ok(gd) => !gd.items.is_empty(),
            Err(e) => {
                eprintln!(
                    "[survey-aggregator] game-data read failed, treating CDN as unavailable for retro-claim: {e}"
                );
                false
            }
        };
        let claim_outcome = (|| -> Result<u32, String> {
            let tx = rusqlite::Transaction::new_unchecked(
                conn,
                rusqlite::TransactionBehavior::Immediate,
            )
            .map_err(|e| format!("claim txn begin failed: {e}"))?;
            // Claim-time speed-bonus (issue #211): a Basic use recorded
            // back-to-back after a prior Basic use earned the game's speed
            // bonus — stamp the claimed rows. Non-Basic kinds never chain
            // (Motherlode/Multihit have no speed bonus). Same transaction:
            // a failure rolls back to untagged rows a later pass can
            // reclaim, never half-tagged rows.
            let is_bonus = kind == SurveyUseKind::Basic
                && persistence::basic_use_is_chained(
                    &tx,
                    character,
                    server,
                    use_id,
                    now_iso,
                    SPEED_BONUS_CHAIN_WINDOW_SECS,
                )
                .map_err(|e| format!("bonus chain check failed: {e}"))?;
            let claimed = persistence::claim_unlinked_chat_loot_near(
                &tx,
                use_id,
                character,
                server,
                now_iso,
                CHAT_BASIC_WINDOW_SECS as u32,
                is_bonus,
                cdn_available,
            )
            .map_err(|e| format!("claim rows failed: {e}"))?;
            if claimed > 0 {
                persistence::add_loot_qty(&tx, use_id, claimed)
                    .map_err(|e| format!("claim qty update failed: {e}"))?;
                // Basic completes on its same-tick loot; mining uses stay
                // pending — completing on the first yield would close the
                // node cycle the live fallback still feeds.
                if kind == SurveyUseKind::Basic {
                    persistence::set_use_status(&tx, use_id, SurveyUseStatus::Completed)
                        .map_err(|e| format!("claim status update failed: {e}"))?;
                }
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
        let data_available = match self.game_data.try_read() {
            Ok(gd) => !gd.items.is_empty(),
            Err(e) => {
                eprintln!(
                    "[survey-aggregator] game-data lock failed, treating as unavailable for {character}@{server}: {e}"
                );
                false
            }
        };
        let lookup_map = internal_name
            .and_then(|i| self.lookup_survey_kind(i))
            .is_some();
        let display_signal = display_name
            .map(|n| n.to_ascii_lowercase().contains("survey"))
            .unwrap_or(false);
        if lookup_map || (!data_available && display_signal) {
            return None;
        }
        let Some(chat_ts) = parse_utc_datetime(timestamp) else {
            eprintln!(
                "[survey-aggregator] chat timestamp parse failed, skipping attribution for {character}@{server}: {timestamp}"
            );
            return None;
        };

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
                    display_name,
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
        // A failed lookup skips attribution (healable via retro-claim);
        // it must never read as "no session" anywhere near an insert — the
        // chat path inserts nothing, so failing closed is the safe shape
        // here. Every error collapse below logs with identity context;
        // session-lookup errors are already logged at the source
        // (`fetch_active_session`).
        if internal_name.is_some_and(|internal| self.is_survey_loot_item(internal)) {
            let Ok(Some(session_id)) = self.fetch_active_session(conn, character, server) else {
                return None;
            };
            let latest = match persistence::latest_use_for_session(conn, session_id, timestamp) {
                Ok(latest) => latest,
                Err(e) => {
                    eprintln!(
                        "[survey-aggregator] latest-use lookup failed, skipping chat attribution for {character}@{server} item {}: {e}",
                        internal_name.unwrap_or("?")
                    );
                    return None;
                }
            };
            if let Some((use_id, kind, used_at)) = latest {
                let in_window = kind != SurveyUseKind::Basic || match parse_utc_datetime(&used_at)
                {
                    Some(used) => (chat_ts - used).num_seconds().abs() <= CHAT_BASIC_WINDOW_SECS,
                    None => {
                        eprintln!(
                            "[survey-aggregator] use timestamp parse failed, skipping chat attribution for {character}@{server} use {use_id}: {used_at}"
                        );
                        false
                    }
                };
                if in_window {
                    return self.record_chat_attribution(
                        conn,
                        ChatAttribution {
                            character,
                            server,
                            use_id,
                            quantity,
                            timestamp,
                            item_name: internal_name,
                            display_name,
                            mark_completed: false,
                        },
                    );
                }
            }
        }

        None
    }

    /// Attribute a live chat loot gain AND tag its ledger row in one write
    /// transaction: the qty bump plus the `source_details` tag commit
    /// together. If the tag fails, the bump rolls back with it, leaving the
    /// row untagged with no qty behind it (only a later use recorded within
    /// the claim window may still reclaim it via retro-claim) —
    /// never a bumped qty on an untagged row that a later claim would
    /// double-count. Same unchecked-transaction shape as the dispatch arms
    /// and the retro-claim tail: fresh pool checkout, no outer transaction
    /// open here.
    ///
    /// Returns `Ok(None)` when the gain is not survey loot (nothing to
    /// retry — the caller moves on), `Ok(Some(use_id))` when the row is
    /// tagged exactly once, and `Err` when the unit rolled back. `Err` is
    /// retry-safe: the rollback removed the bump, so re-running this whole
    /// method re-bumps exactly once and a later pass never double-counts.
    /// The caller stages the inputs on `Err` for a bounded retry at batch
    /// end (TODO 53).
#[allow(clippy::too_many_arguments)]
    pub fn attribute_and_tag_chat_gain(
        &mut self,
        conn: &Connection,
        character: &str,
        server: &str,
        internal_name: Option<&str>,
        quantity: u32,
        timestamp: &str,
        display_name: Option<&str>,
    ) -> Result<Option<i64>, String> {
        let tx = rusqlite::Transaction::new_unchecked(
            conn,
            rusqlite::TransactionBehavior::Immediate,
        )
        .map_err(|e| format!("survey attr txn begin failed: {e}"))?;
        let use_id = match self.attribute_chat_gain(
            &tx,
            character,
            server,
            internal_name,
            quantity,
            timestamp,
            display_name,
        ) {
            Some(use_id) => use_id,
            // Not survey loot: no writes happened, so dropping the untouched
            // transaction is a no-op (never staged, never retried).
            None => return Ok(None),
        };
        // Merge into source_details (never overwrite):
        // JSON receipts carry a `json_twin` provenance
        // marker that the twin-claim guard relies on —
        // replacing the blob would drop the marker and
        // re-open the unsourced-first duplicate hole.
        let tag_rows = tx
            .execute(
                "UPDATE item_transactions
                                        SET source_kind = 'survey_chat',
                                            source_details = json_set(
                                                COALESCE(source_details, '{}'),
                                                '$.survey_use_id', ?1)
                                      WHERE rowid = (
                                        SELECT rowid FROM item_transactions
                                         WHERE timestamp = ?2
                                           AND character_name = ?3
                                           AND server_name = ?4
                                           AND source = 'chat_status'
                                           AND source_kind IS NULL
                                           -- staged survey gains are loot only
                                           -- (summoned rows share source='chat_status'
                                           -- / source_kind NULL); exclude them.
                                           AND context = 'loot'
                                           AND item_name = ?5
                                           AND quantity = ?6
                                         ORDER BY rowid DESC LIMIT 1
                                      )",
                rusqlite::params![
                    use_id,
                    timestamp,
                    character,
                    server,
                    display_name,
                    quantity as i64,
                ],
            )
            .map_err(|e| format!("survey row tag failed: {e}"))?;
        // The tag must land on exactly one untagged
        // ledger row: zero means the row vanished (or
        // never matched) between checkouts — commit
        // would bump a use with no provenance, so roll
        // back and let a later pass reclaim the gain.
        if tag_rows != 1 {
            return Err(format!(
                "survey row tag matched {tag_rows} rows, rolled back"
            ));
        }
        tx.commit()
            .map_err(|e| format!("survey attr txn commit failed: {e}"))?;
        Ok(Some(use_id))
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
            display_name,
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
        // Fail-dangerous guards: a busy/locked DB error is "unknown", never
        // "not counted". Bumping blind would double-count loot_qty with no
        // error and no log; skipping leaves the row untagged with no qty
        // behind it (only a later use recorded within the claim window may
        // still reclaim it via retro-claim) instead of a bumped qty a later
        // claim would double-count.
        let tagged = match persistence::player_gain_already_counted(
            conn,
            use_id,
            quantity,
            timestamp,
            item_name,
            CHAT_BASIC_WINDOW_SECS as u32,
        ) {
            Ok(counted) => counted,
            Err(e) => {
                eprintln!("[survey-aggregator] twin-guard query failed, skipping attribution: {e}");
                return None;
            }
        };
        let already_counted = tagged
            || match persistence::untagged_player_gain_exists(
                conn,
                character,
                server,
                quantity,
                timestamp,
                item_name,
                CHAT_BASIC_WINDOW_SECS as u32,
            ) {
                Ok(exists) => exists,
                Err(e) => {
                    eprintln!(
                        "[survey-aggregator] untagged-twin guard query failed, skipping attribution: {e}"
                    );
                    return None;
                }
            };
        if !already_counted
            && let Err(e) = persistence::add_loot_qty(conn, use_id, quantity) {
                eprintln!("[survey-aggregator] chat add_loot_qty failed: {e}");
                return None;
            }
        // Claim-time speed-bonus stamp (issue #211): a Basic use chained
        // back-to-back after a prior Basic use earned the game's speed
        // bonus, so its rows carry `$.is_speed_bonus`. Chat has no per-item
        // marker since the Player.log "(speed bonus!)" signal went away —
        // chained-use timing is the only signal, NEVER quantity patterns.
        // Motherlode/Multihit have no speed bonus. The stamp lands before
        // the coordinator's tag UPDATE in the same transaction and its
        // json_set merge preserves the flag; best-effort (log, never
        // propagate) so a vanished row can't fail the attribution.
        let bonus_eligible = persistence::get_use(conn, use_id)
            .ok()
            .flatten()
            .is_some_and(|u| {
                u.kind == SurveyUseKind::Basic
                    && persistence::basic_use_is_chained(
                        conn,
                        character,
                        server,
                        use_id,
                        &u.used_at,
                        SPEED_BONUS_CHAIN_WINDOW_SECS,
                    )
                    .unwrap_or(false)
            });
        if bonus_eligible
            && let Some(name) = display_name
            && let Err(e) = persistence::mark_live_chat_gain_as_speed_bonus(
                conn, character, server, timestamp, name, quantity,
            )
        {
            eprintln!("[survey-aggregator] speed-bonus stamp failed: {e}");
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
    ) -> Result<Option<i64>, String> {
        // Identity-keyed: a cached id only answers for the identity that
        // stored it. Anything else re-resolves from the DB.
        if let Some((c, s, id)) = &self.cached_active_session
            && c == character
            && s == server
        {
            return Ok(*id);
        }
        // A failed lookup is NOT cached and NOT treated as "no session":
        // returning `None` here would route the caller down the
        // create-new-session path and fragment sessions on a transient
        // DB/decode failure. Propagate so `record_manual_use` fails the
        // attempt instead of inserting a second active session.
        match persistence::active_session(conn, character, server) {
            Ok(s) => {
                let id = s.map(|s| s.id);
                self.cached_active_session =
                    Some((character.to_string(), server.to_string(), id));
                Ok(id)
            }
            Err(e) => {
                eprintln!("[survey-aggregator] active-session lookup failed, not caching: {e}");
                Err(e.to_string())
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
    fn twin_guard_error_skips_bump_and_retro_claim_heals() {
        // Fail-dangerous guard regression: a guard query error is
        // "unknown", never "not counted". With the ledger table renamed
        // away, the twin-guard query fails → attribution returns None
        // and loot_qty stays 0 (no blind bump). Restoring the table and
        // running the retro-claim sweep then heals the same Basic-window
        // gain onto the use.
        let conn = fresh_db();
        let mut agg = empty_aggregator();
        let use_id = record_basic(&mut agg, &conn, "2026-04-15 12:00:00");
        conn.execute(
            "ALTER TABLE item_transactions RENAME TO item_transactions_hidden",
            [],
        )
        .expect("hide ledger table");
        let hit = agg.attribute_chat_gain(
            &conn,
            "Zenith",
            "Dreva",
            Some("IronOre"),
            2,
            "2026-04-15 12:00:00",
            None,
        );
        assert_eq!(hit, None);
        let stored = persistence::get_use(&conn, use_id)
            .expect("read use")
            .expect("use present");
        assert_eq!(stored.loot_qty, 0);
        conn.execute(
            "ALTER TABLE item_transactions_hidden RENAME TO item_transactions",
            [],
        )
        .expect("restore ledger table");
        // The skipped gain's ledger row, still untagged.
        conn.execute(
            "INSERT INTO item_transactions
                (timestamp, character_name, server_name, item_name, quantity, context, source)
             VALUES ('2026-04-15 12:00:00', 'Zenith', 'Dreva', 'Iron Ore', 2, 'loot', 'chat_status')",
            [],
        )
        .expect("insert skipped gain");
        // Retro-claim sweep: the same calls record_manual_use makes for
        // Basic uses (claim rows, then bump the use).
        let claimed = persistence::claim_unlinked_chat_loot_near(
            &conn,
            use_id,
            "Zenith",
            "Dreva",
            "2026-04-15 12:00:00",
            CHAT_BASIC_WINDOW_SECS as u32,
            false,
            true,
        )
        .expect("retro-claim");
        assert_eq!(claimed, 2);
        persistence::add_loot_qty(&conn, use_id, claimed).expect("heal bump");
        let stored = persistence::get_use(&conn, use_id)
            .expect("read use")
            .expect("use present");
        assert_eq!(stored.loot_qty, 2);
    }

    #[test]
    fn latest_use_error_skips_attribution_and_retro_claim_heals() {
        // Identity-fallback collapse regression: with `survey_uses`
        // hidden, `latest_use_for_session` fails → attribution returns
        // None and loot_qty stays 0 (fail-closed, no blind bump; the
        // collapse point logs with identity context). Restoring the
        // table and running the retro-claim sweep heals the same gain
        // onto the use. A Motherlode use keeps the tier-1 Basic window
        // out of the picture (it never queues), so `None` proves the
        // fallback failed closed.
        let conn = fresh_db();
        let mut agg = seeded_aggregator();
        let (use_id, _) = agg
            .record_manual_use(
                &conn,
                ManualUse {
                    character: "Zenith",
                    server: "Dreva",
                    area: None,
                    map_internal_name: "MiningSurveySerbule1",
                    kind: SurveyUseKind::Motherlode,
                    now_iso: "2026-04-15 12:00:00",
                },
            )
            .expect("record manual Motherlode use");
        conn.execute(
            "ALTER TABLE survey_uses RENAME TO survey_uses_hidden",
            [],
        )
        .expect("hide uses table");
        let hit = agg.attribute_chat_gain(
            &conn,
            "Zenith",
            "Dreva",
            Some("IronOre"),
            2,
            "2026-04-15 12:00:00",
            None,
        );
        assert_eq!(hit, None);
        conn.execute(
            "ALTER TABLE survey_uses_hidden RENAME TO survey_uses",
            [],
        )
        .expect("restore uses table");
        let stored = persistence::get_use(&conn, use_id)
            .expect("read use")
            .expect("use present");
        assert_eq!(stored.loot_qty, 0);
        // The skipped gain's ledger row, still untagged.
        conn.execute(
            "INSERT INTO item_transactions
                (timestamp, character_name, server_name, item_name, quantity, context, source)
             VALUES ('2026-04-15 12:00:00', 'Zenith', 'Dreva', 'Iron Ore', 2, 'loot', 'chat_status')",
            [],
        )
        .expect("insert skipped gain");
        let claimed = persistence::claim_unlinked_chat_loot_near(
            &conn,
            use_id,
            "Zenith",
            "Dreva",
            "2026-04-15 12:00:00",
            CHAT_BASIC_WINDOW_SECS as u32,
            false,
            true,
        )
        .expect("retro-claim");
        assert_eq!(claimed, 2);
        persistence::add_loot_qty(&conn, use_id, claimed).expect("heal bump");
        let stored = persistence::get_use(&conn, use_id)
            .expect("read use")
            .expect("use present");
        assert_eq!(stored.loot_qty, 2);
    }

    #[test]
    fn first_use_session_rolls_back_with_failed_use_insert() {
        // The FirstUse session row commits in the same transaction as the
        // use: sabotaging the use insert must leave zero sessions, not an
        // orphaned zero-use row. Pre-fix the session pre-committed outside
        // the use tx (and the retry created a second one) — this fails
        // with 2 rows there, passes with 0 here.
        let conn = fresh_db();
        conn.execute_batch(
            "CREATE TRIGGER sabotage_use_inserts BEFORE INSERT ON survey_uses
             BEGIN SELECT RAISE(ABORT, 'sabotaged use insert'); END;",
        )
        .expect("install sabotage trigger");
        let mut agg = empty_aggregator();
        let err = agg
            .record_manual_use(
                &conn,
                ManualUse {
                    character: "Zenith",
                    server: "Dreva",
                    area: None,
                    map_internal_name: "MiningSurveySerbule1",
                    kind: SurveyUseKind::Basic,
                    now_iso: "2026-04-15 12:00:00",
                },
            )
            .expect_err("sabotaged use insert must fail");
        assert!(err.contains("sabotaged"), "unexpected error: {err}");
        let sessions: i64 = conn
            .query_row("SELECT COUNT(*) FROM survey_sessions", [], |r| r.get(0))
            .unwrap();
        assert_eq!(sessions, 0, "failed use left an orphan FirstUse session");
        let uses: i64 = conn
            .query_row("SELECT COUNT(*) FROM survey_uses", [], |r| r.get(0))
            .unwrap();
        assert_eq!(uses, 0);
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

    fn insert_chat_loot(conn: &Connection, ts: &str, item: &str, qty: u32) {
        conn.execute(
            "INSERT INTO item_transactions
                (timestamp, character_name, server_name, item_name, quantity, context, source)
             VALUES (?1, 'Zenith', 'Dreva', ?2, ?3, 'loot', 'chat_status')",
            rusqlite::params![ts, item, qty],
        )
        .unwrap();
    }

    fn speed_bonus_flag(conn: &Connection, item: &str) -> Option<i64> {
        conn.query_row(
            "SELECT json_extract(source_details, '$.is_speed_bonus')
             FROM item_transactions WHERE item_name = ?1",
            rusqlite::params![item],
            |row| row.get(0),
        )
        .unwrap()
    }

    #[test]
    fn live_attribution_stamps_bonus_for_chained_use() {
        // Loot lands AFTER the use is recorded (live ordering): the
        // retro-claim inside record_basic finds nothing, so the flag below
        // comes from the live attribution stamp, not the claim path.
        let conn = fresh_db();
        let mut agg = empty_aggregator();
        let _a = record_basic(&mut agg, &conn, "2026-04-15 12:00:00");
        let b = record_basic(&mut agg, &conn, "2026-04-15 12:00:04");
        insert_chat_loot(&conn, "2026-04-15 12:00:04", "Iron Ore", 2);
        let hit = agg.attribute_chat_gain(
            &conn,
            "Zenith",
            "Dreva",
            None,
            2,
            "2026-04-15 12:00:04",
            Some("Iron Ore"),
        );
        assert_eq!(hit, Some(b));
        assert_eq!(speed_bonus_flag(&conn, "Iron Ore"), Some(1));
        // The unit stamps the flag only — tagging the row `survey_chat` is
        // the coordinator's job (its json_set merge preserves the flag).
        let kind: Option<String> = conn
            .query_row(
                "SELECT source_kind FROM item_transactions WHERE item_name = 'Iron Ore'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(kind, None);
        // Single loot authority still holds: the gain bumps exactly once.
        assert_eq!(
            persistence::get_use(&conn, b).unwrap().unwrap().loot_qty,
            2
        );
    }

    #[test]
    fn live_attribution_leaves_flag_null_without_chain() {
        let conn = fresh_db();
        let mut agg = empty_aggregator();
        let a = record_basic(&mut agg, &conn, "2026-04-15 12:00:00");
        insert_chat_loot(&conn, "2026-04-15 12:00:00", "Iron Ore", 3);
        let hit = agg.attribute_chat_gain(
            &conn,
            "Zenith",
            "Dreva",
            None,
            3,
            "2026-04-15 12:00:00",
            Some("Iron Ore"),
        );
        assert_eq!(hit, Some(a));
        assert_eq!(speed_bonus_flag(&conn, "Iron Ore"), None);
    }

    #[test]
    fn live_attribution_never_flags_non_basic_use() {
        // Motherlode M recorded back-to-back after Basic A; a survey-yield
        // gain past A's 5s window falls back to M and stays unflagged —
        // Motherlode has no speed bonus even when chained.
        let conn = fresh_db();
        let mut agg = seeded_aggregator();
        let _a = record_basic(&mut agg, &conn, "2026-04-15 12:00:00");
        let (m, _) = agg
            .record_manual_use(
                &conn,
                ManualUse {
                    character: "Zenith",
                    server: "Dreva",
                    area: None,
                    map_internal_name: "SimpleMetalMotherlodeKur1",
                    kind: SurveyUseKind::Motherlode,
                    now_iso: "2026-04-15 12:00:03",
                },
            )
            .expect("record manual Motherlode use");
        insert_chat_loot(&conn, "2026-04-15 12:00:10", "Iron Ore", 2);
        let hit = agg.attribute_chat_gain(
            &conn,
            "Zenith",
            "Dreva",
            Some("IronOre"),
            2,
            "2026-04-15 12:00:10",
            Some("Iron Ore"),
        );
        assert_eq!(hit, Some(m));
        assert_eq!(speed_bonus_flag(&conn, "Iron Ore"), None);
    }

    #[test]
    fn retro_claim_stamps_bonus_for_chained_use() {
        // Loot lands BEFORE the use is recorded (scan ordering): the
        // retro-claim inside record_basic stamps the chained use's rows.
        let conn = fresh_db();
        let mut agg = empty_aggregator();
        let _a = record_basic(&mut agg, &conn, "2026-04-15 12:00:00");
        insert_chat_loot(&conn, "2026-04-15 12:00:04", "Iron Ore", 2);
        insert_chat_loot(&conn, "2026-04-15 12:00:05", "Copper Ore", 1);
        let b = record_basic(&mut agg, &conn, "2026-04-15 12:00:04");
        assert_eq!(speed_bonus_flag(&conn, "Iron Ore"), Some(1));
        assert_eq!(speed_bonus_flag(&conn, "Copper Ore"), Some(1));
        let use_id: Option<i64> = conn
            .query_row(
                "SELECT survey_use_id FROM item_transactions WHERE item_name = 'Iron Ore'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(use_id, Some(b));
        let stored = persistence::get_use(&conn, b).unwrap().unwrap();
        assert_eq!(stored.loot_qty, 3);
    }

    #[test]
    fn retro_claim_leaves_flag_null_for_first_use() {
        let conn = fresh_db();
        let mut agg = empty_aggregator();
        insert_chat_loot(&conn, "2026-04-15 12:00:00", "Iron Ore", 3);
        let a = record_basic(&mut agg, &conn, "2026-04-15 12:00:00");
        assert_eq!(speed_bonus_flag(&conn, "Iron Ore"), None);
        assert_eq!(
            persistence::get_use(&conn, a).unwrap().unwrap().loot_qty,
            3
        );
    }

    fn record_mining_use(
        agg: &mut SurveySessionAggregator,
        conn: &Connection,
        kind: SurveyUseKind,
        map: &str,
        now_iso: &str,
    ) -> i64 {
        let (use_id, _) = agg
            .record_manual_use(
                conn,
                ManualUse {
                    character: "Zenith",
                    server: "Dreva",
                    area: None,
                    map_internal_name: map,
                    kind,
                    now_iso,
                },
            )
            .expect("record manual mining use");
        use_id
    }

    #[test]
    fn retro_claim_covers_motherlode_first_yield() {
        // Scan order: the first yield lands in the ledger before the user
        // frames the Motherlode use. The all-kinds retro-claim tags it and
        // bumps loot_qty — but the use stays pending: Motherlode loot
        // arrives across the node cycle and must not complete on the first
        // ore, and Motherlode never earns a speed bonus.
        let conn = fresh_db();
        let mut agg = empty_aggregator();
        insert_chat_loot(&conn, "2026-04-15 12:00:10", "Iron Ore", 2);
        let m = record_mining_use(
            &mut agg,
            &conn,
            SurveyUseKind::Motherlode,
            "SimpleMetalMotherlodeKur1",
            "2026-04-15 12:00:12",
        );
        let stored = persistence::get_use(&conn, m).unwrap().unwrap();
        assert_eq!(stored.loot_qty, 2);
        assert_eq!(stored.status, SurveyUseStatus::PendingLoot);
        assert_eq!(speed_bonus_flag(&conn, "Iron Ore"), None);
        let tag: Option<String> = conn
            .query_row(
                "SELECT source_kind FROM item_transactions WHERE item_name = 'Iron Ore'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(tag.as_deref(), Some("survey_chat"));
    }

    #[test]
    fn retro_claim_covers_multihit_first_yield() {
        // Same cross-order shape for Multihit: pre-stamped first yield is
        // claimed, the use stays pending for the remaining swings.
        let conn = fresh_db();
        let mut agg = empty_aggregator();
        insert_chat_loot(&conn, "2026-04-15 12:00:10", "Iron Ore", 1);
        let m = record_mining_use(
            &mut agg,
            &conn,
            SurveyUseKind::Multihit,
            "MiningSurveyPovus7Y",
            "2026-04-15 12:00:13",
        );
        let stored = persistence::get_use(&conn, m).unwrap().unwrap();
        assert_eq!(stored.loot_qty, 1);
        assert_eq!(stored.status, SurveyUseStatus::PendingLoot);
    }

    #[test]
    fn retro_claim_ignores_far_rows_for_non_basic() {
        // The tight ±5s window binds non-Basic claims too: an ordinary
        // mining gain a minute before the framed use is not vacuumed in.
        let conn = fresh_db();
        let mut agg = empty_aggregator();
        insert_chat_loot(&conn, "2026-04-15 12:00:00", "Iron Ore", 2);
        let m = record_mining_use(
            &mut agg,
            &conn,
            SurveyUseKind::Motherlode,
            "SimpleMetalMotherlodeKur1",
            "2026-04-15 12:01:00",
        );
        let stored = persistence::get_use(&conn, m).unwrap().unwrap();
        assert_eq!(stored.loot_qty, 0);
        assert_eq!(stored.status, SurveyUseStatus::PendingLoot);
        let tag: Option<String> = conn
            .query_row(
                "SELECT source_kind FROM item_transactions WHERE item_name = 'Iron Ore'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(tag, None);
    }

    #[test]
    fn survey_tag_failure_rolls_back_and_retry_heals_both_twin_orders() {
        // TODO 53: the tag UPDATE is DB-error-only, so force it with a
        // failing UPDATE trigger. The first attempt must roll the whole
        // unit back (loot_qty untouched, row untagged); dropping the
        // trigger and re-running the unit — what the coordinator's
        // batch-end retry does — heals exactly once. Both twin orders: a
        // JSON-first row carries the `json_twin` marker the merge must
        // preserve, an English-first row starts from NULL details.
        for json_first in [false, true] {
            let conn = fresh_db();
            let mut agg = empty_aggregator();
            let use_id = record_basic(&mut agg, &conn, "2026-04-15 12:00:00");
            conn.execute(
                "INSERT INTO item_transactions
                    (timestamp, character_name, server_name, item_name, quantity,
                     context, source, source_details)
                 VALUES ('2026-04-15 12:00:00', 'Zenith', 'Dreva', 'Iron Ore', 2,
                         'loot', 'chat_status', ?1)",
                rusqlite::params![json_first.then_some("{\"json_twin\":true}")],
            )
            .expect("insert engrafted gain");
            conn.execute(
                "CREATE TEMP TRIGGER fail_tag BEFORE UPDATE OF source_kind
                 ON item_transactions
                 BEGIN SELECT RAISE(ABORT, 'injected tag failure'); END",
                [],
            )
            .expect("install failing tag trigger");
            let err = agg
                .attribute_and_tag_chat_gain(
                    &conn,
                    "Zenith",
                    "Dreva",
                    Some("IronOre"),
                    2,
                    "2026-04-15 12:00:00",
                    Some("Iron Ore"),
                )
                .expect_err("tag UPDATE failure must roll back");
            assert!(
                err.contains("survey row tag failed"),
                "unexpected error (json_first={json_first}): {err}"
            );
            let stored = persistence::get_use(&conn, use_id)
                .expect("read use")
                .expect("use present");
            assert_eq!(stored.loot_qty, 0, "json_first={json_first}");
            let tag: Option<String> = conn
                .query_row(
                    "SELECT source_kind FROM item_transactions WHERE item_name = 'Iron Ore'",
                    [],
                    |row| row.get(0),
                )
                .expect("read tag");
            assert_eq!(tag, None, "json_first={json_first}");
            conn.execute("DROP TRIGGER fail_tag", [])
                .expect("drop failing tag trigger");
            // Batch-end retry: same inputs, whole unit re-runs.
            assert_eq!(
                agg.attribute_and_tag_chat_gain(
                    &conn,
                    "Zenith",
                    "Dreva",
                    Some("IronOre"),
                    2,
                    "2026-04-15 12:00:00",
                    Some("Iron Ore"),
                ),
                Ok(Some(use_id)),
                "json_first={json_first}"
            );
            let stored = persistence::get_use(&conn, use_id)
                .expect("read use")
                .expect("use present");
            assert_eq!(stored.loot_qty, 2, "json_first={json_first}");
            let (kind, linked, twin): (Option<String>, Option<i64>, Option<i64>) = conn
                .query_row(
                    "SELECT source_kind,
                            json_extract(source_details, '$.survey_use_id'),
                            json_extract(source_details, '$.json_twin')
                     FROM item_transactions WHERE item_name = 'Iron Ore'",
                    [],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                )
                .expect("read tagged row");
            assert_eq!(kind.as_deref(), Some("survey_chat"));
            assert_eq!(linked, Some(use_id));
            assert_eq!(twin, json_first.then_some(1));
            // A later pass over the now-tagged row matches nothing and rolls
            // its own bump back: loot_qty must not move (never double-count).
            assert!(
                agg.attribute_and_tag_chat_gain(
                    &conn,
                    "Zenith",
                    "Dreva",
                    Some("IronOre"),
                    2,
                    "2026-04-15 12:00:00",
                    Some("Iron Ore"),
                )
                .is_err()
            );
            let stored = persistence::get_use(&conn, use_id)
                .expect("read use")
                .expect("use present");
            assert_eq!(stored.loot_qty, 2, "json_first={json_first}");
        }
    }

    #[test]
    fn survey_tag_zero_match_rolls_back_and_retry_heals() {
        // The tag matches no row (gain staged before its ledger row
        // landed): committing would bump a use with no provenance, so the
        // unit rolls back instead. The row arriving later plus a re-run —
        // what the batch-end retry and a later rescan both do — heals
        // exactly once.
        let conn = fresh_db();
        let mut agg = empty_aggregator();
        let use_id = record_basic(&mut agg, &conn, "2026-04-15 12:00:00");
        let err = agg
            .attribute_and_tag_chat_gain(
                &conn,
                "Zenith",
                "Dreva",
                Some("IronOre"),
                2,
                "2026-04-15 12:00:00",
                Some("Iron Ore"),
            )
            .expect_err("zero-match tag must roll back");
        assert!(err.contains("matched 0 rows"), "unexpected error: {err}");
        let stored = persistence::get_use(&conn, use_id)
            .expect("read use")
            .expect("use present");
        assert_eq!(stored.loot_qty, 0);
        insert_chat_loot(&conn, "2026-04-15 12:00:00", "Iron Ore", 2);
        assert_eq!(
            agg.attribute_and_tag_chat_gain(
                &conn,
                "Zenith",
                "Dreva",
                Some("IronOre"),
                2,
                "2026-04-15 12:00:00",
                Some("Iron Ore"),
            ),
            Ok(Some(use_id))
        );
        let stored = persistence::get_use(&conn, use_id)
            .expect("read use")
            .expect("use present");
        assert_eq!(stored.loot_qty, 2);
    }

}
