/// Shared structured dispatch for Chat.log messages — the single home of the
/// Status/Combat structured arms that `coordinator.rs::process_chat_events`
/// runs on the live drain.
///
/// Manual scan paths (`scan_chat_logs`, `scan_chat_log_file`, `tail_chat_log`
/// in `chat_commands.rs`) previously stored chat rows but never ran any of
/// this, so a `/exportcharacter`-era manual scan silently dropped milk,
/// level-ups, roulette, Words of Power, combat wisdom, arena matches, deaths,
/// rezzes, hoplology, currency deltas, and watch-rule evaluation. Those paths
/// now call [`dispatch_chat_message`] per parsed message with the file's own
/// session identity and merge the returned domains into one
/// `game-state-updated` emit.
///
/// # Per-file identity
///
/// The live drain uses the global active character/server. Scan paths pass the
/// producing session instead: the file's own login lines
/// (`parse_chat_login_line`) name the character/server; when the scan resumes
/// mid-session (no login line yet seen) the caller falls back to
/// `session_server_for` with the `user_characters` table and skips structured
/// dispatch for the line when that also yields nothing — never attributing
/// one session's gains to whoever happens to be active.
///
/// # Replay safety
///
/// Every arm here is idempotent or recency-guarded, so re-scanning a file
/// inserts nothing new — except currency, which is live-only by design
/// (see below):
/// - item ledger: `INSERT` is guarded by an existence check on the full
///   `(timestamp, character, server, item, quantity, context, source)` key —
///   a re-scan's second write finds the row and skips it.
/// - cow/player milk, poop: `ON CONFLICT … DO NOTHING` / MAX-newer upserts.
/// - combat wisdom, roulette, arena: `INSERT OR IGNORE` unique indices.
/// - level-up: newer-wins recency guard in `apply_chat_level_up`.
/// - Words of Power: `DELETE` of an already-gone word removes 0 rows.
/// - deaths: deduped against `character_deaths` on
///   `(character, server, died_at, killer)` before insert.
/// - rezzes: rolling 30-second per-target window, same as the live path.
/// - hoplology: `insert_hoplology_study_from_chat` only touches the row when
///   the study is new or upgrades a report-only row to chat.
/// - currency: live-only (see arm below) — scans skip it, because a
///   scan cannot know which of its lines the live drain already counted
///   (the accumulator has no per-line memory), while replaying
///   pre-export chat would double-count.
///
/// # Survey attribution (documented exclusion)
///
/// Chat gains feed the survey tracker through the coordinator's *live*
/// `SurveySessionAggregator` (`attribute_chat_gain`), whose gating state
/// (recent Basic uses, mining-swing windows) exists only in the running
/// coordinator. The scan path cannot feed that state — it is not available
/// in `chat_commands.rs` and replaying history into it would corrupt live
/// gating. Scan-inserted loot rows instead land untagged
/// (`source_kind IS NULL`), which is exactly the shape the tracker's
/// `claim_unlinked_chat_loot_near` retro-tag step consumes: the next
/// Player.log DeleteItem for a Basic survey within ±5s of the chat row
/// claims it, bumps `loot_qty` once, and stamps `source_kind =
/// 'survey_chat'`. Mining-swing-window and identity-fallback attribution do
/// not apply retroactively — accepted gap, same as any late-arriving log.
///
/// # ReportSaved (documented exclusion)
///
/// `ReportSaved` lines point at VIP export files by absolute path. Outside
/// the coordinator the referenced file may not exist (different machine, or
/// the user cleaned up `Reports/`), so failed imports would spam errors on
/// every re-scan and — worse — import a stale local path over newer state.
/// Paths are therefore reported back in
/// [`DispatchOutcome::report_saved_paths`] (NOT auto-imported): the live
/// drain imports each path; scan paths log-and-skip (see
/// `run_scan_dispatch`). Importing stays a live-path privilege where the
/// file was just written by the game on this machine.
use crate::arena_parser::ArenaTracker;
use crate::cdn_commands::GameDataState;
use crate::chat_combat_parser::{self, ChatCombatEvent};
use crate::chat_parser::ChatMessage;
use crate::chat_resuscitate_parser::{self, ChatResuscitateEvent};
use crate::chat_status_parser::{parse_status_message, ChatStatusEvent};
use crate::db::DbPool;
use crate::survey::{persistence as survey_persistence, SurveyUseStatus};
use crate::watch_rules::WatchRuleTriggered;
use crate::settings::WatchRule;

/// Who a scanned/observed line belongs to. The live drain uses the global
/// active identity; scan paths resolve this per file (login line, else
/// `user_characters` fallback) and skip dispatch when it is `None`.
#[derive(Debug, Clone)]
pub struct DispatchIdentity {
    pub character: String,
    pub server: String,
}

/// Mutable per-process state the live coordinator owns and the dispatch
/// needs: the arena narration tracker and the rez-dedup map.
pub struct DispatchState<'a> {
    pub arena_tracker: &'a mut ArenaTracker,
    /// Last persisted successful-rez timestamp per target (rolling 30s
    /// dedup window, same as the coordinator's `last_rez_dedup`).
    pub last_rez_dedup: &'a mut std::collections::HashMap<String, String>,
    /// Rolling buffer of recent damage-on-player events (max 10),
    /// snapshotted into the DB when a death persists.
    pub recent_damage: &'a mut Vec<ChatCombatEvent>,
    /// Set when a cow interaction is in progress (internal `Cow_*` name);
    /// consumed when a Bottle of Milk gain lands.
    pub pending_cow_interaction: &'a mut Option<String>,
    /// Current area, attached to death rows.
    pub current_area: Option<String>,
    /// Whether wallet-delta arms (`CoinsLooted`/`CouncilsChanged`) may
    /// accrue into `currency_estimate.delta_since`. The accumulator has no
    /// per-line memory, so a scan cannot know which of its lines the live
    /// drain already counted — re-scanning double-counts whenever the live
    /// path also saw the lines. The live coordinator sets this; scan paths
    /// (`run_scan_dispatch`) leave the default `false` and skip accrual.
    pub count_currency: bool,
}

/// What one dispatched message changed, for the caller to emit.
#[derive(Debug, Default)]
pub struct DispatchOutcome {
    /// Domains touched (dedup + sort before emitting `game-state-updated`).
    pub domains: Vec<String>,
    /// Live-only structured events the frontend subscribes to. The scan
    /// path forwards these too (same names as the live path) so the UI
    /// refreshes identically.
    pub chat_status_event: Option<ChatStatusEvent>,
    pub enemy_killed: Option<ChatCombatEvent>,
    pub character_death: Option<ChatCombatEvent>,
    pub character_resuscitated: Option<ChatResuscitateEvent>,
    /// Watch rules that fired for this message.
    pub watch_triggered: Vec<WatchRuleTriggered>,
    /// ReportSaved paths seen (live drain imports them; scan paths
    /// log-and-skip — see module docs).
    pub report_saved_paths: Vec<String>,
    /// Skill names that did not resolve against game data (live path keeps
    /// a monotonic counter and logs; scan paths log-and-skip).
    pub unknown_skills: Vec<String>,
    /// Item gains the caller treats as chat-sourced stack corrections
    /// (display name, quantity). The live coordinator feeds the
    /// player-watcher correlation buffer and corrects inline (live) or
    /// stages session-keyed gains (catch-up); the scan path applies them
    /// session-keyed via `correct_stack_for_session`.
    pub staged_gains: Vec<(String, u32)>,
    /// Set when an arena match resolved — the caller runs the (idempotent)
    /// bet rescan and emits `arena` when it changes anything.
    pub arena_resolved: bool,
}

impl DispatchOutcome {
    fn domain(&mut self, d: &str) {
        if !self.domains.iter().any(|x| x == d) {
            self.domains.push(d.to_string());
        }
    }
}

/// Run every idempotent structured arm for one chat message.
///
/// - `msg`: message with its **UTC** timestamp (`chat_local_to_utc` already
///   applied) — this is what `parse_status_message` stamps into rows and what
///   the DB compares against anchors.
/// - `local_ts`: the log's local wall-clock `"%Y-%m-%d %H:%M:%S"` timestamp —
///   the dedup key for Combat Wisdom / arena / poop (matches the backfill).
/// - `identity`: `None` skips identity-scoped arms (ledger, skills, milk,
///   currency, hoplology, deaths) instead of misattributing them.
/// - `survey_hook`: reserved hook into the coordinator's
///   `SurveySessionAggregator::attribute_chat_gain`. Currently always `None`
///   in production: both the live drain and scan paths pass `None` (rows
///   stay untagged for `claim_unlinked_chat_loot_near` — see module docs),
///   and the live drain runs attribution post-dispatch per staged gain
///   (same timestamps the dispatch just wrote). Stack correction and the
///   player-watcher correlation feed stay caller-side: live/catch-up need
///   coordinator state the scan path cannot (and must not) touch, so gains
///   are reported in [`DispatchOutcome::staged_gains`] for the caller to
///   apply session-keyed.
pub fn dispatch_chat_message(
    db: &DbPool,
    game_data: &GameDataState,
    identity: Option<&DispatchIdentity>,
    msg: &ChatMessage,
    local_ts: &str,
    survey_hook: Option<&mut dyn FnMut(Option<&str>, u32, &str) -> Option<i64>>,
    state: &mut DispatchState<'_>,
    watch_rules: &[WatchRule],
) -> DispatchOutcome {
    let mut out = DispatchOutcome::default();

    // Arena narration resolves on the local wall clock so live records
    // dedup against the chat backfill.
    if let Some(m) = state.arena_tracker.observe(
        msg.channel.as_deref(),
        msg.sender.as_deref(),
        &msg.message,
        local_ts,
    ) {
        if let Ok(conn) = db.get_write() {
            if let Ok(1) = crate::db::arena_commands::record_arena_match(
                &conn,
                &m.fought_at,
                &m.fighter_a,
                &m.fighter_b,
                &m.winner,
            ) {
                out.domain("arena");
                out.arena_resolved = true;
            }
        }
    }
    // Watch rules fire on the raw message (same as the live drain's
    // post-insert evaluation). Rules match on channel/sender/text, none of
    // which are identity-scoped, so scans evaluate them too.
    if !watch_rules.is_empty() {
        for hit in crate::watch_rules::evaluate_rules(msg, watch_rules) {
            out.watch_triggered.push(hit);
        }
    }

    if let Some(status_event) = parse_status_message(msg) {
        // Combat Wisdom keys on the LOCAL wall clock (live + backfill dedup),
        // so it bypasses the UTC-stamped generic arm.
        if let ChatStatusEvent::CombatWisdomEarned { amount, source_name, verb, zone, .. } = &status_event {
            if let Ok(conn) = db.get_write() {
                crate::db::combat_wisdom_commands::record_combat_wisdom_earn(
                    &conn, local_ts, *amount, source_name.as_deref(), verb, zone.as_deref(),
                )
                .ok();
            }
        }
        dispatch_status_event(db, game_data, identity, &status_event, &mut out, survey_hook, state, local_ts);
        out.chat_status_event = Some(status_event);
    }

    if let Some(id) = identity {
        if let Some(combat_event) = chat_combat_parser::parse_combat_message(msg, &id.character) {
            match &combat_event {
                ChatCombatEvent::PlayerDeath { .. } => {
                    if persist_death_event(db, game_data, id, &combat_event, state) {
                        out.domain("deaths");
                    }
                    out.character_death = Some(combat_event);
                    state.recent_damage.clear();
                }
                ChatCombatEvent::EnemyKilled { .. } => {
                    // Drop-rate kills come from Player.log corpse searches;
                    // chat only feeds the live farming-session UI.
                    out.enemy_killed = Some(combat_event);
                }
                ChatCombatEvent::DamageOnPlayer { .. } => {
                    if state.recent_damage.len() >= 10 {
                        state.recent_damage.remove(0);
                    }
                    state.recent_damage.push(combat_event);
                }
            }
        }

        if msg.channel.as_deref() == Some("Combat") {
            if chat_combat_parser::parse_self_ability_line(
                msg.sender.as_deref(),
                &msg.message,
                &id.character,
                "Collect Milk",
            )
            .as_deref()
                == Some(id.character.as_str())
            {
                // UTC basis (like the MilkedBy arm and the backfill): `msg`
                // arrives UTC-converted, while `local_ts` is the wall clock
                // the converter would misread as UTC (off by the tz offset,
                // defeating the v72 UNIQUE key against backfill rows).
                let utc_ts = msg.timestamp.format("%Y-%m-%d %H:%M:%S").to_string();
                record_player_milk(db, &id.character, &id.server, &id.character, "self_milked", &utc_ts);
                out.domain("milking");
            }
            if let Some(target) = chat_combat_parser::parse_self_poop_line(
                msg.sender.as_deref(),
                &msg.message,
                &id.character,
            ) {
                if record_poop(db, &id.character, &id.server, &target, local_ts) {
                    out.domain("report_stats");
                }
            }
        }

        if let Some(rez_event) = chat_resuscitate_parser::parse_resuscitate_message(msg) {
            if persist_rez_event(db, id, &rez_event, state.last_rez_dedup, &state.current_area) {
                out.character_resuscitated = Some(rez_event);
            }
        }
    }

    out
}

fn dispatch_status_event(
    db: &DbPool,
    game_data: &GameDataState,
    identity: Option<&DispatchIdentity>,
    event: &ChatStatusEvent,
    out: &mut DispatchOutcome,
    mut survey_hook: Option<&mut dyn FnMut(Option<&str>, u32, &str) -> Option<i64>>,
    state: &mut DispatchState<'_>,
    local_ts: &str,
) {
    match event {
        ChatStatusEvent::ItemGained {
            item_name,
            quantity,
            timestamp,
        }
        | ChatStatusEvent::Summoned {
            item_name,
            quantity,
            timestamp,
        } => {
            let context = match event {
                ChatStatusEvent::Summoned { .. } => "summoned",
                _ => "loot",
            };
            let (internal_name, item_type_id): (Option<String>, Option<i64>) = game_data
                .try_read()
                .ok()
                .and_then(|gd| gd.resolve_item(item_name).map(|info| (info.internal_name.clone(), Some(info.id as i64))))
                .unwrap_or((None, None));

            if let Some(id) = identity {
                // Milking timer: a Bottle of Milk gain while a cow
                // interaction is pending attributes the milk to that cow
                // (live path only — scans have no interaction state).
                if item_name == "Bottle of Milk" || internal_name.as_deref() == Some("BottleOfMilk") {
                    if let Some(cow_internal) = state.pending_cow_interaction.take() {
                        let cow_display = cow_internal
                            .strip_prefix("Cow_")
                            .unwrap_or(&cow_internal)
                            .to_string();
                        if record_cow_milk_session(
                            db, &id.character, &id.server, &cow_display, "", timestamp,
                        ) {
                            out.domain("milking");
                        }
                    }
                }
                out.staged_gains.push((item_name.clone(), *quantity));
                let survey_use_id = if context == "loot" {
                    match survey_hook.as_deref_mut() {
                        Some(hook) => hook(internal_name.as_deref(), *quantity, timestamp),
                        None => None,
                    }
                } else {
                    None
                };
                insert_item_transaction_guarded(
                    db,
                    timestamp,
                    &id.character,
                    &id.server,
                    item_name,
                    internal_name.as_deref(),
                    item_type_id,
                    *quantity,
                    context,
                    survey_use_id,
                );
            }
        }
        ChatStatusEvent::ItemStudied { timestamp, item_name } => {
            let base_name = game_data
                .try_read()
                .ok()
                .and_then(|gd| gd.find_equipment_base_name(item_name))
                .unwrap_or_else(|| item_name.to_string());
            if let Some(id) = identity {
                if let Ok(conn) = db.get_write() {
                    match crate::db::hoplology_commands::insert_hoplology_study_from_chat(
                        &conn, &id.character, &id.server, &base_name, timestamp,
                    ) {
                        Ok(Some(_)) => out.domain("hoplology"),
                        Ok(None) => {}
                        Err(e) => eprintln!("Failed to persist hoplology study: {e}"),
                    }
                }
            }
        }
        ChatStatusEvent::CombatWisdomEarned { .. } => {
            // Already recorded with the local-ts key before this fn —
            // nothing to do here (arm exists so the match stays exhaustive).
        }
        // Live-only (gated on `state.count_currency`): the accumulator has
        // no per-line memory, so a scan cannot know which of its lines the
        // live drain already counted. Scan paths leave the flag `false`.
        ChatStatusEvent::CoinsLooted { amount, timestamp } => {
            if state.count_currency {
                if let Some(id) = identity {
                    accrue_currency_delta(db, &id.character, &id.server, *amount as i64, timestamp);
                }
            }
        }
        ChatStatusEvent::CouncilsChanged { amount, timestamp } => {
            if state.count_currency {
                if let Some(id) = identity {
                    accrue_currency_delta(db, &id.character, &id.server, *amount, timestamp);
                }
            }
        }
        // Local wall-clock key, like Combat Wisdom and arena: the chat
        // backfill parses raw log lines (local time, no UTC conversion),
        // so the UTC event stamp would never dedup against it.
        ChatStatusEvent::RouletteResult { number, .. } => {
            if let Ok(conn) = db.get_write() {
                crate::db::roulette_commands::record_roulette_result(&conn, local_ts, *number).ok();
            }
        }
        ChatStatusEvent::WordOfPowerUsed { word, .. } => {
            if let Ok(conn) = db.get_write() {
                match crate::db::words_of_power_commands::delete_words_by_word(&conn, word) {
                    Ok(0) => {}
                    Ok(_) => out.domain("words_of_power"),
                    Err(e) => eprintln!("[chat_status_dispatch] Failed to remove used word of power: {e}"),
                }
            }
        }
        ChatStatusEvent::MilkedBy { milker, timestamp } => {
            if let Some(id) = identity {
                if record_player_milk(db, &id.character, &id.server, milker, "milked_by", timestamp) {
                    out.domain("milking");
                }
            }
        }
        ChatStatusEvent::LevelUp { skill, level, xp, timestamp } => {
            if let Some(id) = identity {
                let resolved = game_data
                    .try_read()
                    .ok()
                    .and_then(|gd| gd.resolve_skill(skill).map(|info| (info.id as i64, info.name.clone())));
                match resolved {
                    Some((skill_id, display_name)) => {
                        if let Ok(conn) = db.get_write() {
                            match crate::db::game_state_commands::apply_chat_level_up(
                                &conn, &id.character, &id.server, skill_id, &display_name,
                                *level as i64, *xp as i64, timestamp,
                            ) {
                                Ok(true) => out.domain("skills"),
                                Ok(false) => {}
                                Err(e) => eprintln!(
                                    "[chat_status_dispatch] Chat level-up apply failed for {skill} L{level} ({}@{}): {e}",
                                    id.character, id.server
                                ),
                            }
                        }
                    }
                    None => out.unknown_skills.push(skill.clone()),
                }
            }
        }
        ChatStatusEvent::ReportSaved { file_path, .. } => {
            // Documented exclusion: scan paths must not auto-import (see
            // module docs). Report the path so the caller can log it.
            out.report_saved_paths.push(file_path.clone());
        }
        _ => {}
    }
}


/// Insert a chat-sourced item ledger row, skipping when the identical row
/// already exists (replay-safe re-scan). Returns `true` when a row was
/// written. Survey attribution stamps `survey_chat` rows when the live hook
/// supplied a use id; scan paths leave rows untagged for the tracker's
/// retro-claim step.
fn insert_item_transaction_guarded(
    db: &DbPool,
    timestamp: &str,
    character: &str,
    server: &str,
    item_name: &str,
    internal_name: Option<&str>,
    item_type_id: Option<i64>,
    quantity: u32,
    context: &str,
    survey_use_id: Option<i64>,
) -> bool {
    let Ok(conn) = db.get_write() else {
        return false;
    };
    let (source_kind, source_details): (Option<&str>, Option<String>) = match survey_use_id {
        Some(id) => (Some("survey_chat"), Some(format!("{{\"survey_use_id\":{id}}}"))),
        None => (None, None),
    };
    // Replay guard: the full natural key of a chat ledger row. No UNIQUE
    // index exists (adding one would reindex every install), so the check
    // is an explicit SELECT — same-connection, no race on the single writer.
    let exists: bool = conn
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM item_transactions
             WHERE timestamp = ?1 AND character_name = ?2 AND server_name = ?3
               AND item_name = ?4 AND quantity = ?5 AND context = ?6 AND source = 'chat_status')",
            rusqlite::params![timestamp, character, server, item_name, quantity as i32, context],
            |r| r.get(0),
        )
        .unwrap_or(false);
    if exists {
        return false;
    }
    conn.execute(
        "INSERT INTO item_transactions
            (timestamp, character_name, server_name, item_name,
             internal_name, item_type_id, quantity, context,
             source, source_kind, source_details)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 'chat_status', ?9, ?10)",
        rusqlite::params![
            timestamp, character, server, item_name, internal_name,
            item_type_id, quantity as i32, context, source_kind, source_details,
        ],
    )
    .is_ok()
}

/// Signed wallet delta with the same guards as the coordinator: no-op
/// without an export anchor, and only events newer than the anchor count.
fn accrue_currency_delta(db: &DbPool, character: &str, server: &str, amount: i64, event_ts: &str) {
    if amount == 0 {
        return;
    }
    if let Ok(conn) = db.get_write() {
        conn.execute(
            "UPDATE currency_estimate
                SET delta_since = delta_since + ?1, updated_at = datetime('now')
              WHERE character_name = ?2 AND server_name = ?3
                AND currency_name = 'GOLD'
                AND anchor_at IS NOT NULL
                AND ?4 > anchor_at",
            rusqlite::params![amount, character, server, event_ts],
        )
        .ok();
    }
}

fn milking_event_timestamp(event_ts: &str) -> String {
    match chrono::NaiveDateTime::parse_from_str(event_ts.trim(), "%Y-%m-%d %H:%M:%S") {
        Ok(dt) => chrono::DateTime::<chrono::Utc>::from_naive_utc_and_offset(dt, chrono::Utc).to_rfc3339(),
        Err(_) => chrono::Utc::now().to_rfc3339(),
    }
}

/// Returns `true` when a row was written (caller emits `milking`).
fn record_player_milk(
    db: &DbPool,
    character: &str,
    server: &str,
    other_player: &str,
    direction: &str,
    timestamp: &str,
) -> bool {
    let milked_at = milking_event_timestamp(timestamp);
    let Ok(conn) = db.get_write() else {
        return false;
    };
    match crate::db::game_state_commands::record_milking_event(
        &conn, character, server, other_player, direction, &milked_at,
    ) {
        Ok(true) => true,
        Ok(false) => false,
        Err(e) => {
            eprintln!("[chat_status_dispatch] Failed to record milking event: {e}");
            false
        }
    }
}

/// Returns `true` when a row was written (caller emits `report_stats`).
fn record_poop(db: &DbPool, character: &str, server: &str, target: &str, local_ts: &str) -> bool {
    let Ok(conn) = db.get_write() else {
        return false;
    };
    match crate::db::game_state_commands::record_poop(&conn, character, server, target, local_ts) {
        Ok(true) => true,
        Ok(false) => false,
        Err(e) => {
            eprintln!("[chat_status_dispatch] Failed to record poop: {e}");
            false
        }
    }
}

/// Persist a combat death with a replay guard (same killer/timestamp row
/// already present → skip) plus the recent-damage context. Returns `true`
/// when a row was written.
fn persist_death_event(
    db: &DbPool,
    game_data: &GameDataState,
    id: &DispatchIdentity,
    event: &ChatCombatEvent,
    state: &DispatchState<'_>,
) -> bool {
    let ChatCombatEvent::PlayerDeath {
        timestamp,
        killer_name,
        killer_entity_id,
        killing_ability,
        health_damage,
        armor_damage,
    } = event
    else {
        return false;
    };
    let damage_type: Option<String> = game_data
        .try_read()
        .ok()
        .and_then(|gd| gd.resolve_ability(killing_ability).map(|a| a.damage_type.clone()))
        .flatten();
    let Ok(mut conn) = db.get_write() else {
        return false;
    };
    // Replay guard: one death row per (character, server, time, killer).
    let exists: bool = conn
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM character_deaths
             WHERE character_name = ?1 AND server_name = ?2 AND died_at = ?3 AND killer_name = ?4)",
            rusqlite::params![&id.character, &id.server, timestamp, killer_name],
            |r| r.get(0),
        )
        .unwrap_or(false);
    if exists {
        return false;
    }
    let tx = match conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate) {
        Ok(tx) => tx,
        Err(e) => {
            eprintln!("Failed to begin death transaction: {e}");
            return false;
        }
    };
    if tx
        .execute(
            "INSERT INTO character_deaths
                (character_name, server_name, died_at, killer_name, killer_entity_id,
                 killing_ability, health_damage, armor_damage, area, damage_type)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            rusqlite::params![
                &id.character, &id.server, timestamp, killer_name, killer_entity_id,
                killing_ability, health_damage, armor_damage, state.current_area.as_deref(), damage_type,
            ],
        )
        .is_err()
    {
        return false;
    }
    let death_id = tx.last_insert_rowid();
    for (order, dmg_event) in state.recent_damage.iter().enumerate() {
        if let ChatCombatEvent::DamageOnPlayer {
            timestamp: dmg_ts,
            attacker_name,
            attacker_entity_id,
            ability_name,
            health_damage: hp,
            armor_damage: ap,
            is_crit,
        } = dmg_event
        {
            if tx
                .execute(
                    "INSERT INTO death_damage_sources
                        (death_id, event_order, timestamp, attacker_name, attacker_entity_id,
                         ability_name, health_damage, armor_damage, is_crit)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                    rusqlite::params![
                        death_id, order as i64, dmg_ts, attacker_name, attacker_entity_id,
                        ability_name, hp, ap, *is_crit,
                    ],
                )
                .is_err()
            {
                return false;
            }
        }
    }
    tx.commit().is_ok()
}


/// Persist a resuscitation with the rolling 30-second per-target dedup
/// window. Returns `true` when a row was written (caller emits the event).
fn persist_rez_event(
    db: &DbPool,
    id: &DispatchIdentity,
    event: &ChatResuscitateEvent,
    last_rez_dedup: &mut std::collections::HashMap<String, String>,
    current_area: &Option<String>,
) -> bool {
    let (timestamp, caster_name, target_name, success) = match event {
        ChatResuscitateEvent::Resuscitated { timestamp, caster_name, target_name } => {
            (timestamp, caster_name, target_name, true)
        }
        ChatResuscitateEvent::ResuscitateFailed { timestamp, caster_name, target_name } => {
            (timestamp, caster_name, target_name, false)
        }
    };
    if success {
        if let Some(prev) = last_rez_dedup.get(target_name) {
            if crate::coordinator::rez_within_dedup_window(prev, timestamp) {
                return false;
            }
        }
        last_rez_dedup.retain(|_, prev| crate::coordinator::rez_within_dedup_window(prev, timestamp));
    }
    let Ok(conn) = db.get_write() else {
        return false;
    };
    // Replay guard: same (character, server, caster, target, time) row.
    let exists: bool = conn
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM character_resuscitations
             WHERE character_name = ?1 AND server_name = ?2 AND occurred_at = ?3
               AND caster_name = ?4 AND target_name = ?5)",
            rusqlite::params![&id.character, &id.server, timestamp, caster_name, target_name],
            |r| r.get(0),
        )
        .unwrap_or(false);
    if exists {
        // Cross-run duplicate: a previous run persisted this row, but this
        // run's in-memory dedup map is fresh. Prime it so the paired
        // phrasing (same target within 30s, different caster/stamp) dedups
        // against this run's memory instead of inserting a second row.
        if success {
            last_rez_dedup.insert(target_name.clone(), timestamp.clone());
        }
        return false;
    }
    let ok = conn
        .execute(
            "INSERT INTO character_resuscitations
                (character_name, server_name, occurred_at, caster_name, target_name, success, area)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            rusqlite::params![
                &id.character, &id.server, timestamp, caster_name, target_name,
                success, current_area.as_deref(),
            ],
        )
        .is_ok();
    if ok && success {
        last_rez_dedup.insert(target_name.clone(), timestamp.clone());
    }
    ok
}

/// Record a cow milking with the newer-wins upsert. Returns `true` on
/// success (caller emits `milking`).
pub fn record_cow_milk_session(
    db: &DbPool,
    character: &str,
    server: &str,
    cow_name: &str,
    zone: &str,
    timestamp: &str,
) -> bool {
    let milked_at = milking_event_timestamp(timestamp);
    let Ok(conn) = db.get_write() else {
        return false;
    };
    crate::db::game_state_commands::record_cow_milk(&conn, character, server, cow_name, zone, &milked_at).is_ok()
}

/// Backfill path for a missed survey-use claim: tag untagged chat loot rows
/// near `used_at` and bump the use's loot total, mirroring the live
/// aggregator's `handle_survey_consumed` step. Idempotent — already-claimed
/// rows carry `source_kind = 'survey_chat'` and the predicate skips them.
#[allow(dead_code)] // Staged for survey retro-claim wiring (scan rows stay untagged until then).
pub fn claim_survey_loot_near(
    db: &DbPool,
    survey_use_id: i64,
    character: &str,
    server: &str,
    used_at_iso: &str,
) -> u32 {
    let Ok(conn) = db.get_write() else {
        return 0;
    };
    match survey_persistence::claim_unlinked_chat_loot_near(&conn, survey_use_id, character, server, used_at_iso, 5) {
        Ok(0) => 0,
        Ok(n) => {
            if survey_persistence::add_loot_qty(&conn, survey_use_id, n).is_err() {
                return 0;
            }
            let _ = survey_persistence::set_use_status(&conn, survey_use_id, SurveyUseStatus::Completed);
            if let Ok(Some(su)) = survey_persistence::get_use(&conn, survey_use_id) {
                if let Some(sid) = su.session_id {
                    let _ = survey_persistence::update_first_loot_at(&conn, sid, used_at_iso);
                    let _ = survey_persistence::update_last_loot_at(&conn, sid, used_at_iso);
                }
            }
            n
        }
        Err(_) => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_pool() -> (DbPool, std::path::PathBuf) {
        // Named temp file so the read and write pools share one database.
        // Unique per call: SystemTime nanos repeat when parallel tests start
        // in the same tick, so a pid + atomic counter carry uniqueness.
        use std::sync::atomic::{AtomicU64, Ordering};
        static SEQ: AtomicU64 = AtomicU64::new(0);
        let n = SEQ.fetch_add(1, Ordering::Relaxed);
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let path = std::env::temp_dir().join(format!(
            "glogger_dispatch_test_{}_{nanos}_{n}.db",
            std::process::id()
        ));
        let pool = crate::db::init_pool(path.clone(), Some(0)).expect("init pool");
        (pool, path)
    }

    fn game_data() -> GameDataState {
        std::sync::Arc::new(tauri::async_runtime::RwLock::new(crate::game_data::GameData::empty()))
    }

    fn state<'a>(
        arena: &'a mut ArenaTracker,
        rez: &'a mut std::collections::HashMap<String, String>,
        dmg: &'a mut Vec<ChatCombatEvent>,
        cow: &'a mut Option<String>,
    ) -> DispatchState<'a> {
        DispatchState {
            arena_tracker: arena,
            last_rez_dedup: rez,
            recent_damage: dmg,
            pending_cow_interaction: cow,
            current_area: None,
            // Tests exercise arms directly (live-equivalent coverage).
            count_currency: true,
        }
    }

    /// Scan-equivalent state: currency accrual off, like `run_scan_dispatch`.
    fn state_scan<'a>(
        arena: &'a mut ArenaTracker,
        rez: &'a mut std::collections::HashMap<String, String>,
        dmg: &'a mut Vec<ChatCombatEvent>,
        cow: &'a mut Option<String>,
    ) -> DispatchState<'a> {
        DispatchState {
            arena_tracker: arena,
            last_rez_dedup: rez,
            recent_damage: dmg,
            pending_cow_interaction: cow,
            current_area: None,
            count_currency: false,
        }
    }

    fn status_msg(text: &str, ts: &str) -> ChatMessage {
        let line = format!("{ts}\t[Status] {text}");
        crate::chat_parser::parse_chat_line(&line).unwrap()
    }

    fn combat_msg(text: &str, ts: &str) -> ChatMessage {
        let line = format!("{ts}\t[Combat] {text}");
        crate::chat_parser::parse_chat_line(&line).unwrap()
    }

    fn emote_msg(text: &str, ts: &str) -> ChatMessage {
        let line = format!("{ts}\t[Action Emotes] {text}");
        crate::chat_parser::parse_chat_line(&line).unwrap()
    }

    fn id() -> DispatchIdentity {
        DispatchIdentity { character: "Alice".to_string(), server: "Dreva".to_string() }
    }

    fn seed_skill(gd: &GameDataState, name: &str, sid: u32) {
        let mut g = gd.blocking_write();
        g.skills.insert(
            sid,
            crate::game_data::SkillInfo {
                id: sid,
                name: name.to_string(),
                internal_name: name.to_string(),
                description: None,
                icon_id: None,
                xp_table: None,
                keywords: vec![],
                combat: None,
                max_bonus_levels: None,
                parents: vec![],
                advancement_table: None,
                guest_level_cap: None,
                hide_when_zero: None,
                advancement_hints: None,
                rewards: None,
                reports: None,
                raw_json: serde_json::Value::Null,
            },
        );
        g.skill_name_index.insert(name.to_string(), sid);
    }

    fn insert_use(
        conn: &rusqlite::Connection,
        character: &str,
        server: &str,
        used_at: &str,
    ) -> i64 {
        conn.execute(
            "INSERT INTO survey_sessions (character_name, server_name, started_at, start_trigger)
             VALUES (?1, ?2, ?3, 'manual')",
            rusqlite::params![character, server, used_at],
        )
        .unwrap();
        let sid = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO survey_uses (session_id, character_name, server_name, used_at,
                map_internal_name, map_display_name, kind, status, loot_qty)
             VALUES (?1, ?2, ?3, ?4, 'BasicMap', 'Basic Map', 'basic', 'pending_loot', 0)",
            rusqlite::params![sid, character, server, used_at],
        )
        .unwrap();
        conn.last_insert_rowid()
    }

    #[test]
    fn scan_persists_status_lines_like_live_drain() {
        let (pool, _guard) = test_pool();
        let gd = game_data();
        seed_skill(&gd, "Sword", 7);
        let mut arena = ArenaTracker::new();
        let mut rez = std::collections::HashMap::new();
        let mut dmg = Vec::new();
        let mut cow = None;

        let ts = "26-04-01 05:30:53";
        let msgs = vec![
            status_msg("You earned 62 XP and reached level 5 in Sword!", ts),
            status_msg("Tiny Brain Bug Meat added to inventory.", ts),
            status_msg("You received 10 Councils.", ts),
            status_msg("Roulette ball ended on 7!", ts),
            status_msg("Alice has milked you.", ts),
            status_msg("Poop on Somebody!", ts), // not a status line — ignored here
        ];
        for m in &msgs {
            let mut st = state(&mut arena, &mut rez, &mut dmg, &mut cow);
            dispatch_chat_message(&pool, &gd, Some(&id()), m, ts, None, &mut st, &[]);
        }
        // Cow-curse poop comes over Combat, attributed to the file identity.
        let poop = combat_msg("Alice: Poop on Bob!", ts);
        let mut st = state(&mut arena, &mut rez, &mut dmg, &mut cow);
        let o = dispatch_chat_message(&pool, &gd, Some(&id()), &poop, ts, None, &mut st, &[]);
        assert!(o.domains.contains(&"report_stats".to_string()));

        let conn = pool.get_write().unwrap();
        let skills: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM game_state_skills WHERE character_name = 'Alice'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(skills, 1);
        let loot: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM item_transactions WHERE source = 'chat_status'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(loot, 1);
        let spins: i64 = conn.query_row("SELECT COUNT(*) FROM roulette_results", [], |r| r.get(0)).unwrap();
        assert_eq!(spins, 1);
        let milks: i64 = conn.query_row("SELECT COUNT(*) FROM player_milking_log", [], |r| r.get(0)).unwrap();
        assert_eq!(milks, 1);
        // Live-equivalent dispatch (count_currency: true) accrues the
        // wallet delta exactly once. The anchor is required first.
        conn.execute(
            "INSERT INTO currency_estimate (character_name, server_name, currency_name, anchor_amount, anchor_at, delta_since)
             VALUES ('Alice','Dreva','GOLD',1000,'2026-04-01 05:00:00',0)",
            [],
        )
        .unwrap();
        drop(conn);
        let m = status_msg("You received 10 Councils.", ts);
        let mut st = state(&mut arena, &mut rez, &mut dmg, &mut cow);
        dispatch_chat_message(&pool, &gd, Some(&id()), &m, ts, None, &mut st, &[]);
        let conn = pool.get_write().unwrap();
        let delta: i64 = conn
            .query_row(
                "SELECT delta_since FROM currency_estimate WHERE character_name = 'Alice'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(delta, 10);
        // Scan-equivalent dispatch (count_currency: false) must not accrue.
        drop(conn);
        let mut st = state_scan(&mut arena, &mut rez, &mut dmg, &mut cow);
        dispatch_chat_message(&pool, &gd, Some(&id()), &m, ts, None, &mut st, &[]);
        let conn = pool.get_write().unwrap();
        let delta2: i64 = conn
            .query_row(
                "SELECT delta_since FROM currency_estimate WHERE character_name = 'Alice'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(delta2, 10);
        // Re-scan inserts nothing new.
        for m in &msgs {
            let mut st = state(&mut arena, &mut rez, &mut dmg, &mut cow);
            dispatch_chat_message(&pool, &gd, Some(&id()), m, ts, None, &mut st, &[]);
        }
        let mut st = state(&mut arena, &mut rez, &mut dmg, &mut cow);
        dispatch_chat_message(&pool, &gd, Some(&id()), &poop, ts, None, &mut st, &[]);
        let loot2: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM item_transactions WHERE source = 'chat_status'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(loot2, 1);
        let spins2: i64 = conn.query_row("SELECT COUNT(*) FROM roulette_results", [], |r| r.get(0)).unwrap();
        assert_eq!(spins2, 1);
        let milks2: i64 = conn.query_row("SELECT COUNT(*) FROM player_milking_log", [], |r| r.get(0)).unwrap();
        assert_eq!(milks2, 1);
    }

    #[test]
    fn per_file_identity_two_sessions() {
        let (pool, _guard) = test_pool();
        let gd = game_data();
        seed_skill(&gd, "Sword", 7);
        let mut arena = ArenaTracker::new();
        let mut rez = std::collections::HashMap::new();
        let mut dmg = Vec::new();
        let mut cow = None;
        let ts = "26-04-01 05:30:53";

        let alice = DispatchIdentity { character: "Alice".to_string(), server: "Dreva".to_string() };
        let bob = DispatchIdentity { character: "Bob".to_string(), server: "Legacy".to_string() };
        let m = status_msg("You earned 62 XP and reached level 5 in Sword!", ts);
        let mut st = state(&mut arena, &mut rez, &mut dmg, &mut cow);
        dispatch_chat_message(&pool, &gd, Some(&alice), &m, ts, None, &mut st, &[]);
        let mut st = state(&mut arena, &mut rez, &mut dmg, &mut cow);
        dispatch_chat_message(&pool, &gd, Some(&bob), &m, ts, None, &mut st, &[]);

        let conn = pool.get_write().unwrap();
        let rows: Vec<(String, String)> = conn
            .prepare("SELECT character_name, server_name FROM game_state_skills ORDER BY character_name")
            .unwrap()
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap()
            .map(|r| r.unwrap())
            .collect();
        assert_eq!(
            rows,
            vec![
                ("Alice".to_string(), "Dreva".to_string()),
                ("Bob".to_string(), "Legacy".to_string()),
            ]
        );
    }

    #[test]
    fn skips_dispatch_without_identity() {
        let (pool, _guard) = test_pool();
        let gd = game_data();
        let mut arena = ArenaTracker::new();
        let mut rez = std::collections::HashMap::new();
        let mut dmg = Vec::new();
        let mut cow = None;
        let ts = "26-04-01 05:30:53";
        let m = status_msg("Tiny Brain Bug Meat added to inventory.", ts);
        let mut st = state(&mut arena, &mut rez, &mut dmg, &mut cow);
        let out = dispatch_chat_message(&pool, &gd, None, &m, ts, None, &mut st, &[]);
        assert!(out.staged_gains.is_empty());
        let conn = pool.get_write().unwrap();
        let n: i64 = conn.query_row("SELECT COUNT(*) FROM item_transactions", [], |r| r.get(0)).unwrap();
        assert_eq!(n, 0);
    }

    #[test]
    fn death_persists_once_and_rez_dedups_pair() {
        let (pool, _guard) = test_pool();
        let gd = game_data();
        let mut arena = ArenaTracker::new();
        let mut rez = std::collections::HashMap::new();
        let mut dmg = Vec::new();
        let mut cow = None;
        let ts = "26-04-01 05:30:53";

        let death = combat_msg(
            "Demon Scout #7059135: Demon Bolt on Alice! Dmg: 179 health, 178 armor. (FATALITY!)",
            ts,
        );
        let mut st = state(&mut arena, &mut rez, &mut dmg, &mut cow);
        dispatch_chat_message(&pool, &gd, Some(&id()), &death, ts, None, &mut st, &[]);
        let mut st = state(&mut arena, &mut rez, &mut dmg, &mut cow);
        dispatch_chat_message(&pool, &gd, Some(&id()), &death, ts, None, &mut st, &[]);
        let n: i64 = pool.get().unwrap().query_row("SELECT COUNT(*) FROM character_deaths", [], |r| r.get(0)).unwrap();
        assert_eq!(n, 1);

        // Paired rez phrasings within 30s collapse to one row.
        let r1 = emote_msg("Goblin resuscitates Alice", "26-04-01 05:31:00");
        let r2 = emote_msg("Alice comes back to life!", "26-04-01 05:31:10");
        let mut st = state(&mut arena, &mut rez, &mut dmg, &mut cow);
        dispatch_chat_message(&pool, &gd, Some(&id()), &r1, "26-04-01 05:31:00", None, &mut st, &[]);
        let mut st = state(&mut arena, &mut rez, &mut dmg, &mut cow);
        let out = dispatch_chat_message(&pool, &gd, Some(&id()), &r2, "26-04-01 05:31:10", None, &mut st, &[]);
        assert!(out.character_resuscitated.is_none());
        let n: i64 = pool.get().unwrap().query_row("SELECT COUNT(*) FROM character_resuscitations", [], |r| r.get(0)).unwrap();
        assert_eq!(n, 1);
    }

    #[test]
    fn scan_loot_claimable_by_survey_use() {
        let (pool, _guard) = test_pool();
        let gd = game_data();
        let mut arena = ArenaTracker::new();
        let mut rez = std::collections::HashMap::new();
        let mut dmg = Vec::new();
        let mut cow = None;
        let ts = "26-04-01 05:30:53";

        let m = status_msg("Copper Ore x3 added to inventory.", ts);
        let mut st = state(&mut arena, &mut rez, &mut dmg, &mut cow);
        dispatch_chat_message(&pool, &gd, Some(&id()), &m, ts, None, &mut st, &[]);

        // Untagged until a Basic use claims it.
        let conn = pool.get_write().unwrap();
        let kind: Option<String> = conn
            .query_row("SELECT source_kind FROM item_transactions", [], |r| r.get(0))
            .unwrap();
        assert_eq!(kind, None);
        let use_id = insert_use(&conn, "Alice", "Dreva", ts);
        drop(conn);
        // Rows carry the normalized 4-digit stamp (`msg.timestamp.format`), so
        // the claim window needs the same shape — the 2-digit log form is not
        // parseable by SQLite `datetime()`.
        assert_eq!(claim_survey_loot_near(&pool, use_id, "Alice", "Dreva", "2026-04-01 05:30:53"), 3);
        let conn = pool.get_write().unwrap();
        let kind: Option<String> = conn
            .query_row("SELECT source_kind FROM item_transactions", [], |r| r.get(0))
            .unwrap();
        assert_eq!(kind.as_deref(), Some("survey_chat"));
        // Second claim finds nothing (rows already tagged).
        drop(conn);
        assert_eq!(claim_survey_loot_near(&pool, use_id, "Alice", "Dreva", "2026-04-01 05:30:53"), 0);
    }

    #[test]
    fn wisdom_records_once_with_local_key() {
        // Regression: CombatWisdomEarned used to hit `unreachable!` in the
        // shared arm (live catch-up with real wisdom lines aborted the app).
        let (pool, _guard) = test_pool();
        let gd = game_data();
        let mut arena = ArenaTracker::new();
        let mut rez = std::collections::HashMap::new();
        let mut dmg = Vec::new();
        let mut cow = None;
        let ts = "26-04-01 05:30:53";
        let m = status_msg("You earned 64 Combat Wisdom: Killed the Aktaari Queen", ts);
        let mut st = state(&mut arena, &mut rez, &mut dmg, &mut cow);
        let out = dispatch_chat_message(&pool, &gd, Some(&id()), &m, ts, None, &mut st, &[]);
        assert!(out.chat_status_event.is_some());
        let mut st = state(&mut arena, &mut rez, &mut dmg, &mut cow);
        dispatch_chat_message(&pool, &gd, Some(&id()), &m, ts, None, &mut st, &[]);
        let n: i64 = pool.get().unwrap().query_row("SELECT COUNT(*) FROM combat_wisdom_earns", [], |r| r.get(0)).unwrap();
        assert_eq!(n, 1);
    }

    #[test]
    fn report_saved_is_reported_not_imported() {
        let (pool, _guard) = test_pool();
        let gd = game_data();
        let mut arena = ArenaTracker::new();
        let mut rez = std::collections::HashMap::new();
        let mut dmg = Vec::new();
        let mut cow = None;
        let ts = "26-04-01 05:30:53";
        let m = status_msg("Saved report to C:/Games/Reports/Character_Alice.json", ts);
        let mut st = state(&mut arena, &mut rez, &mut dmg, &mut cow);
        let out = dispatch_chat_message(&pool, &gd, Some(&id()), &m, ts, None, &mut st, &[]);
        assert_eq!(out.report_saved_paths.len(), 1);
    }
}
