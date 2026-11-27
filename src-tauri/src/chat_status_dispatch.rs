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
/// - chat XP (level-up + within-level): v101 per-award ledger identity on
///   (character, server, skill, timestamp, xp, level) — a replay or twin
///   finds its row and skips; same-second / out-of-order awards with a
///   distinct identity apply. Same-level awards at or before the snapshot
///   anchor (`xp_snapshot_at`) drop as already-counted.
/// - Words of Power: `DELETE` of an already-gone word removes 0 rows.
/// - deaths: deduped against `character_deaths` on
///   `(character, server, died_at, killer)` before insert.
/// - rezzes: deduped against `character_resuscitations` on the full row key
///   before insert — the cross-phrasing window is gone (pair never observed).
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
/// recorded survey use of any kind within ±5s of the chat row claims it,
/// bumps `loot_qty` once, and stamps `source_kind =
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
/// needs: the arena narration tracker.
pub struct DispatchState<'a> {
    pub arena_tracker: &'a mut ArenaTracker,
    /// Rolling buffer of recent damage-on-player events (max 10),
    /// snapshotted into the DB when a death persists.
    pub recent_damage: &'a mut Vec<ChatCombatEvent>,
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
    /// `[JSON:Type]` type strings whose body failed to parse (live path keeps
    /// a monotonic counter and logs; the raw chat row is still stored).
    pub malformed_json: Vec<String>,
    /// `[JSON:Type]` type strings with no frozen schema yet (kept as raw rows
    /// only). Logged like malformed lines so a new game-side Type surfaces
    /// instead of vanishing silently.
    pub unknown_json: Vec<String>,
    /// A `[JSON:Book]` line's (book_type, title, body, event timestamp) for
    /// triggers (gourmand/hoplology/teleports/report-stats need `AppHandle`
    /// for their emits, which dispatch never holds). The books upsert itself
    /// already ran here; the scan path ignores this (persistence is shared,
    /// triggers stay live-only like the old `BookOpened` arm).
    pub json_book: Option<(String, String, String, String)>,
    /// New item gains for caller-side attribution (display name, quantity).
    /// Exactly-once: pushed only when this line wrote the ledger row, so the
    /// live coordinator's survey attribution and catch-up staging never
    /// double-count twins or replays.
    pub staged_gains: Vec<(String, u32)>,
}

impl DispatchOutcome {
    fn domain(&mut self, d: &str) {
        if !self.domains.iter().any(|x| x == d) {
            self.domains.push(d.to_string());
        }
    }
}

/// Identity-scoped twin slot for unresolvable-skill sightings.
///
/// `note_unknown_skill`'s `Option<String>` slot keys on skill name only, so
/// one character's half arms a slot that another character's same-skill half
/// then collapses (cross-character collision undercounts one skip). This slot
/// keys on the full identity (character, server, skill): only the same
/// character's same-skill consecutive sighting collapses. Consecutive genuine
/// repeats on one identity still collapse by adjacency (indistinguishable
/// from a twin at this layer); the coordinator counts those suppressions in
/// its observability-only counter rather than re-bumping the skip total.
/// The scan path keeps the skill-only slot (batch-local, out of slice scope).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct UnknownSkillSlot {
    pub character: String,
    pub server: String,
    pub skill: String,
}

/// Identity-scoped twin-collapse for unresolvable-skill sightings. Same
/// adjacency contract as [`note_unknown_skill`], but the slot carries the
/// sighting identity: a same-skill sighting from another character/server
/// counts as a new skip instead of collapsing into the armed slot. A twin
/// consumes the slot (`None` after the pair). Returns `true` when the caller
/// should count/log the skip; `false` is a suppression (twin second or a
/// genuine repeat — observability only, never a recount, no ledger rows).
pub(crate) fn note_unknown_skill_for(
    last: &mut Option<UnknownSkillSlot>,
    identity: Option<&DispatchIdentity>,
    skill: String,
) -> bool {
    let slot = UnknownSkillSlot {
        character: identity
            .map(|i| i.character.as_str())
            .unwrap_or_default()
            .to_string(),
        server: identity
            .map(|i| i.server.as_str())
            .unwrap_or_default()
            .to_string(),
        skill,
    };
    if last.as_ref() == Some(&slot) {
        // Twin second sighting (or a consecutive genuine repeat on the same
        // identity — indistinguishable here): same event shape, not a second
        // skip. Consume the slot so the next same-skill event counts again.
        *last = None;
        false
    } else {
        *last = Some(slot);
        true
    }
}

/// Twin-collapse for unresolvable-skill sightings. An English `LevelUp` and
/// its JSON `Xp` twin arrive as consecutive messages, so the second sighting
/// of the same skill is the same event, not a second skip. Callers keep the
/// `last` slot across poll batches (the coordinator owns it as a field) and
/// only count/log on `true`. A twin consumes the slot (`None` after the
/// pair), so a later same-skill event counts again — `E,J,E,J` is two skips,
/// split across batches or not. An unpaired half arms the slot only until a
/// poll that drains chat lines sees no unknown skill (the coordinator
/// expires it there); empty polls preserve it.
pub(crate) fn note_unknown_skill(last: &mut Option<String>, skill: String) -> bool {
    if last.as_deref() == Some(skill.as_str()) {
        // Twin second sighting: same event, not a second skip. Consume the
        // slot so the next same-skill event counts as a new skip.
        *last = None;
        false
    } else {
        *last = Some(skill);
        true
    }
}

/// Callback into the coordinator's survey attribution (`attribute_chat_gain`):
/// resolved internal item name, quantity, UTC timestamp → linked loot row id.
pub type SurveyHook<'a> = Option<&'a mut dyn FnMut(Option<&str>, u32, &str) -> Option<i64>>;

/// Immutable per-message dispatch inputs, threaded alongside the mutable
/// [`DispatchState`]. Bundles the over-long parameter lists of
/// [`dispatch_chat_message`] and `dispatch_status_event` so neither trips
/// `clippy::too_many_arguments`; the `&mut` borrows (`DispatchState`,
/// survey hook, outcome) stay separate parameters to keep lifetimes simple.
pub struct DispatchCtx<'a> {
    pub db: &'a DbPool,
    pub game_data: &'a GameDataState,
    pub identity: Option<&'a DispatchIdentity>,
    pub msg: &'a ChatMessage,
    pub local_ts: &'a str,
    pub watch_rules: &'a [WatchRule],
    /// Ephemeral session set (in-memory auto-watch skillbook rules, never
    /// persisted). Evaluated by the same matcher right after `watch_rules`;
    /// per-hit backstop below drops auto hits a manual rule also covers.
    pub auto_watch_rules: &'a [WatchRule],
    /// True on scan/tail paths (rescan-safe skips apply); false on the live
    /// drain, where every line is fresh and provisional rows are wanted.
    pub is_scan: bool,
    /// True only on the live tail, where `DispatchState::current_area` tracks
    /// the area the line was logged in. False on startup catch-up and scans,
    /// which drain older lines under a newer area — arms that stamp location
    /// must record NULL instead of the current value.
    pub is_live: bool,
    /// Opaque source-line identity for the ledger replay guards, threaded
    /// into [`ItemLedgerCtx`] and the JSON receipt guard. `None` (failed
    /// raw store) keeps legacy match-any collapse: any same-key row
    /// suppresses the insert. `Some("file:offset")` separates a true
    /// replay (the matching row carries this line's id) from a genuine
    /// second receipt (different offset inserts) and from the cross-twin
    /// (a stamped/JSON-marked row still suppresses). Per-OCCURRENCE, not
    /// per-row: two byte-identical lines share one raw row but carry
    /// different offsets. Callers must pass STABLE ids — the byte offset
    /// of the line in its file (`INSERT OR IGNORE` raw storage never
    /// moves a stored line) — never a per-pass fresh id, which would
    /// duplicate the ledger on every rescan.
    pub source_line_id: Option<String>,
}

/// One ledger row pending the replay guard.
pub struct ItemLedgerCtx<'a> {
    pub timestamp: &'a str,
    pub character: &'a str,
    pub server: &'a str,
    pub item_name: &'a str,
    pub internal_name: Option<&'a str>,
    pub item_type_id: Option<i64>,
    pub quantity: u32,
    pub context: &'a str,
    pub survey_use_id: Option<i64>,
    /// Looted-from entity when the caller knows it. The English `[Status]`
    /// line carries no entity, so the dispatch arm passes `None` — and
    /// `None` matches any existing row (see the guard): an English twin
    /// arriving after the JSON receipt stamped the row still skips, and a
    /// resend still collapses. Pass `Some` only when the gain is keyed by
    /// entity; then only the exact-entity row suppresses the insert.
    pub from_entity_id: Option<i64>,
    /// Mirrors `DispatchCtx::is_scan`: scans skip provisional rows that
    /// are already past the link-up window instead of writing-then-sweeping.
    pub is_scan: bool,
    /// This line's stable identity (`DispatchCtx::source_line_id`): `None`
    /// keeps legacy match-any collapse; `Some("file:offset")` collapses only
    /// on a true replay (a row carrying this line's id) or a
    /// stamped/JSON-marked twin row, and inserts a genuine second receipt
    /// as its own row.
    pub source_line_id: Option<&'a str>,
}

/// Run every idempotent structured arm for one chat message.
///
/// - `ctx.msg`: message with its **UTC** timestamp (`chat_local_to_utc`
///   already applied) — this is what `parse_status_message` stamps into rows
///   and what the DB compares against anchors.
/// - `ctx.local_ts`: the log's local wall-clock `"%Y-%m-%d %H:%M:%S"`
///   timestamp — the dedup key for Combat Wisdom / arena / poop (matches the
///   backfill).
/// - `ctx.identity`: `None` skips identity-scoped arms (ledger, skills,
///   milk, currency, hoplology, deaths) instead of misattributing them.
/// - `survey_hook`: reserved hook into the coordinator's
///   `SurveySessionAggregator::attribute_chat_gain`. Currently always `None`
///   in production: both the live drain and scan paths pass `None` (rows
///   stay untagged for `claim_unlinked_chat_loot_near` — see module docs),
///   (same timestamps the dispatch just wrote). The player-watcher
///   correlation feed stays caller-side: live/catch-up need coordinator
///   state the scan path cannot (and must not) touch, so new gains are
///   reported in [`DispatchOutcome::staged_gains`] (exactly-once attribution).
///   Inventory quantities come from snapshot imports, not from chat lines.
pub fn dispatch_chat_message(
    ctx: DispatchCtx<'_>,
    survey_hook: SurveyHook<'_>,
    state: &mut DispatchState<'_>,
) -> DispatchOutcome {
    let mut out = DispatchOutcome::default();

    // Arena narration resolves on the local wall clock so live records
    // dedup against the chat backfill.
    if let Some(m) = state.arena_tracker.observe(
        ctx.msg.channel.as_deref(),
        ctx.msg.sender.as_deref(),
        &ctx.msg.message,
        ctx.local_ts,
    )
        && let Ok(conn) = ctx.db.get_write()
            && let Ok(1) = crate::db::arena_commands::record_arena_match(
                &conn,
                &m.fought_at,
                &m.fighter_a,
                &m.fighter_b,
                &m.winner,
            ) {
                out.domain("arena");
            }
    // Watch rules fire on the raw message (same as the live drain's
    // post-insert evaluation). Rules match on channel/sender/text, none of
    // which are identity-scoped, so scans evaluate them too.
    if !ctx.watch_rules.is_empty() {
        for hit in crate::watch_rules::evaluate_rules(ctx.msg, ctx.watch_rules) {
            out.watch_triggered.push(hit);
        }
    }
    // Ephemeral session set: same matcher, evaluated after manual rules.
    // Backstop dedupe: an auto hit is dropped when a manual rule covers the
    // same book, so the same book never double-fires even if a manual rule
    // was added after the last session-set rebuild.
    if !ctx.auto_watch_rules.is_empty() {
        for hit in crate::watch_rules::evaluate_rules(ctx.msg, ctx.auto_watch_rules) {
            let dominated = crate::skillbook_watch::auto_hit_book_name(&hit.rule_name)
                .map(|book| crate::skillbook_watch::manual_covers_book(ctx.watch_rules, book))
                .unwrap_or(false);
            if !dominated {
                out.watch_triggered.push(hit);
            }
        }
    }

    if let Some(status_event) = parse_status_message(ctx.msg) {
        // Gate the frontend event on a fresh persist like the death/kill
        // arms: a replayed line's write is a no-op, so re-scans stay
        // silent instead of double-accruing session XP/wisdom.
        if dispatch_status_event(&ctx, &status_event, &mut out, survey_hook, state) {
            out.chat_status_event = Some(status_event);
        }
    }

    // VIP JSON logging: `[JSON:Type] {json}` lines twin the English `[Status]`
    // text (English first, same second) and carry the exact values. The twin
    // guard lives in each arm (enrich-instead-of-skip), so live, scan, and
    // replay all converge on one row — order-independent.
    if ctx.msg.json_type.is_some() {
        match crate::chat_json_parser::parse_json_message(ctx.msg) {
            Some(event) => dispatch_json_event(&ctx, &event, &mut out, state),
            None => out.malformed_json.push(
                ctx.msg.json_type.clone().unwrap_or_default(),
            ),
        }
    }


    if let Some(id) = ctx.identity {
        if let Some(combat_event) = chat_combat_parser::parse_combat_message(ctx.msg, &id.character) {
            match &combat_event {
                ChatCombatEvent::PlayerDeath { .. } => {
                    // Gate the event like the domain: a replayed death row
                    // already exists, so only a fresh persist notifies.
                    if persist_death_event(ctx.db, ctx.game_data, id, &combat_event, state) {
                        out.domain("deaths");
                        out.character_death = Some(combat_event);
                    }
                    state.recent_damage.clear();
                }
                ChatCombatEvent::EnemyKilled {
                    timestamp,
                    enemy_name,
                    enemy_entity_id,
                    killing_ability,
                    health_damage,
                    armor_damage,
                } => {
                    // FATALITY lines carry the entity ID, which matches receipt
                    // `FromEntityId` (paired live 2026-11-17: slimes #488966 /
                    // #488976, shark #19868) — the loot↔kill join key. Group
                    // kills where another lands the blow emit no FATALITY.
                    // Gate both the domain and the event on a fresh insert:
                    // replays and write failures stay silent.
                    // Non-live kills (catch-up, scans) record NULL zone: the
                    // drain's `current_area` is newer than where the kill
                    // happened, and a confident-wrong area is unrecoverable.
                    // Reads bucket NULL as Unknown.
                    let zone = if ctx.is_live {
                        state.current_area.as_deref()
                    } else {
                        None
                    };
                    if let Ok(conn) = ctx.db.get_write()
                        && let Ok(Some(_)) =
                            crate::db::kill_tracking_commands::record_enemy_kill(
                                &conn,
                                &id.character,
                                &id.server,
                                enemy_name,
                                enemy_entity_id,
                                killing_ability,
                                *health_damage as i64,
                                *armor_damage as i64,
                                zone,
                                timestamp,
                            )
                    {
                        out.enemy_killed = Some(combat_event);
                    }
                }
                ChatCombatEvent::DamageOnPlayer { .. } => {
                    if state.recent_damage.len() >= 10 {
                        state.recent_damage.remove(0);
                    }
                    state.recent_damage.push(combat_event);
                }
            }
        }

        if ctx.msg.channel.as_deref() == Some("Combat") {
            if let Some(target) = chat_combat_parser::parse_self_ability_line(
                ctx.msg.sender.as_deref(),
                &ctx.msg.message,
                &id.character,
                "Collect Milk",
            ) {
                // UTC basis (like the MilkedBy arm and the backfill): `msg`
                // arrives UTC-converted, while `local_ts` is the wall clock
                // the converter would misread as UTC (off by the tz offset,
                // defeating the v72 UNIQUE key against backfill rows).
                let utc_ts = ctx.msg.timestamp.format("%Y-%m-%d %H:%M:%S").to_string();
                if target == id.character
                    && record_player_milk(
                        ctx.db,
                        &id.character,
                        &id.server,
                        &id.character,
                        "self_milked",
                        &utc_ts,
                    )
                {
                    out.domain("milking");
                }
                // Gate the domain on a write that advanced the row (like the
                // MilkedBy arm and EnemyKilled): replays and write failures
                // stay silent.
                if record_cow_milk_session(
                    ctx.db,
                    &id.character,
                    &id.server,
                    &target,
                    state.current_area.as_deref().unwrap_or(""),
                    &utc_ts,
                ) {
                    out.domain("milking");
                }
            }
            if let Some(target) = chat_combat_parser::parse_self_poop_line(
                ctx.msg.sender.as_deref(),
                &ctx.msg.message,
                &id.character,
            ) && record_poop(ctx.db, &id.character, &id.server, &target, ctx.local_ts) {
                out.domain("report_stats");
            }
        }

        if let Some(rez_event) = chat_resuscitate_parser::parse_resuscitate_message(ctx.msg)
            && persist_rez_event(ctx.db, id, &rez_event, &state.current_area) {
                out.character_resuscitated = Some(rez_event);
            }
    }

    out
}

fn dispatch_status_event(
    ctx: &DispatchCtx<'_>,
    event: &ChatStatusEvent,
    out: &mut DispatchOutcome,
    survey_hook: SurveyHook<'_>,
    state: &mut DispatchState<'_>,
) -> bool {
    // True when the line advanced a persist (or has no persist key and is
    // live): the caller gates `chat_status_event` on this, so replays and
    // re-scans stay silent like the death/kill arms.
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
            let (internal_name, item_type_id): (Option<String>, Option<i64>) = ctx.game_data
                .try_read()
                .ok()
                .and_then(|gd| gd.resolve_item(item_name).map(|info| (info.internal_name.clone(), Some(info.id as i64))))
                .unwrap_or((None, None));

            if let Some(id) = ctx.identity {
                // Attribution stages only when this line wrote the row: in
                // JSON-first order the JSON twin already staged it, and the
                // English twin's guard skips the insert — staging again would
                // double-count downstream (previously masked only by the
                // survey `already_counted` guard).
                let survey_use_id = if context == "loot" {
                    match survey_hook {
                        Some(hook) => hook(internal_name.as_deref(), *quantity, timestamp),
                        None => None,
                    }
                } else {
                    None
                };
                if insert_item_transaction_guarded(
                    ctx.db,
                    ItemLedgerCtx {
                        timestamp,
                        character: &id.character,
                        server: &id.server,
                        item_name,
                        internal_name: internal_name.as_deref(),
                        item_type_id,
                        quantity: *quantity,
                        context,
                        survey_use_id,
                        // English lines carry no looted-from entity: the
                        // guard treats `None` as "match any row" on the
                        // entity arm, while the line arm keys on this line's
                        // identity. A JSON-first twin-second consumes its
                        // pending JSON row (no stage — the receipt staged);
                        // a genuine second line inserts its own row. Legacy
                        // `None` line ids keep the match-any collapse.
                        from_entity_id: None,
                        source_line_id: ctx.source_line_id.as_deref(),
                        is_scan: ctx.is_scan,
                    },
                ) {
                    out.staged_gains.push((item_name.clone(), *quantity));
                    true
                } else {
                    // Replay (or twin-second): the ledger row already exists,
                    // so nothing stages. A missing row (retention skip, failed
                    // insert write) stages nothing either: the next rescan
                    // retries the insert first.
                    false
                }
            } else {
                false
            }
        }
        ChatStatusEvent::ItemStudied { timestamp, item_name } => {
            let base_name = ctx.game_data
                .try_read()
                .ok()
                .and_then(|gd| gd.find_equipment_base_name(item_name))
                .unwrap_or_else(|| item_name.to_string());
            if let Some(id) = ctx.identity
                && let Ok(conn) = ctx.db.get_write() {
                    match crate::db::hoplology_commands::insert_hoplology_study_from_chat(
                        &conn, &id.character, &id.server, &base_name, timestamp,
                    ) {
                        Ok(Some(_)) => {
                            out.domain("hoplology");
                            true
                        }
                        Ok(None) => false,
                        Err(e) => {
                            eprintln!("Failed to persist hoplology study: {e}");
                            false
                        }
                    }
                } else {
                false
            }
        }
        ChatStatusEvent::CombatWisdomEarned { amount, source_name, verb, zone, .. } => {
            // Recorded here on the LOCAL wall clock (live + backfill dedup),
            // not the UTC-stamped generic path. `INSERT OR IGNORE` hides
            // duplicates, so the emit follows `changes()` on the single
            // held write checkout — a replayed line's 0-row write stays
            // silent, with no check-then-act race.
            let Some(name) = source_name.as_deref() else {
                // Prodigy-level awards name no monster (no row to write):
                // no persist key, so scan stays silent and live emits.
                return !ctx.is_scan;
            };
            let Ok(conn) = ctx.db.get_write() else {
                return false;
            };
            crate::db::combat_wisdom_commands::record_combat_wisdom_earn(
                &conn, ctx.local_ts, *amount, Some(name), verb, zone.as_deref(),
            )
            .unwrap_or_default()
        }
        // Live-only (gated on `state.count_currency`): the accumulator has
        // no per-line memory, so a scan cannot know which of its lines the
        // live drain already counted. Scan paths leave the flag `false`.
        ChatStatusEvent::CoinsLooted { amount, timestamp } => {
            if state.count_currency {
                if ctx.is_scan {
                    if let Some(id) = ctx.identity {
                        accrue_currency_delta(ctx.db, &id.character, &id.server, *amount as i64, timestamp, "coins_looted", ctx.source_line_id.as_deref())
                    } else {
                        false
                    }
                } else {
                    if let Some(id) = ctx.identity {
                        accrue_currency_delta(ctx.db, &id.character, &id.server, *amount as i64, timestamp, "coins_looted", ctx.source_line_id.as_deref());
                    }
                    true
                }
            } else {
                false
            }
        }
        ChatStatusEvent::CouncilsChanged { amount, timestamp } => {
            if state.count_currency {
                if ctx.is_scan {
                    if let Some(id) = ctx.identity {
                        accrue_currency_delta(ctx.db, &id.character, &id.server, *amount, timestamp, "councils_changed", ctx.source_line_id.as_deref())
                    } else {
                        false
                    }
                } else {
                    if let Some(id) = ctx.identity {
                        accrue_currency_delta(ctx.db, &id.character, &id.server, *amount, timestamp, "councils_changed", ctx.source_line_id.as_deref());
                    }
                    true
                }
            } else {
                false
            }
        }
        // Local wall-clock key, like Combat Wisdom and arena: the chat
        // backfill parses raw log lines (local time, no UTC conversion),
        // so the UTC event stamp would never dedup against it.
        ChatStatusEvent::RouletteResult { number, .. } => {
            let Ok(conn) = ctx.db.get_write() else {
                return false;
            };
            // `INSERT OR IGNORE` hides duplicates, so the emit follows
            // `changes()` on the single held write checkout — a replayed
            // line's 0-row write stays silent, with no check-then-act
            // race. Out-of-range numbers still error out of the helper.
            match crate::db::roulette_commands::record_roulette_result(&conn, ctx.local_ts, *number) {
                Ok(()) => conn.changes() == 1,
                Err(_) => false,
            }
        }
        ChatStatusEvent::WordOfPowerUsed { word, .. } => {
            if let Ok(conn) = ctx.db.get_write() {
                match crate::db::words_of_power_commands::delete_words_by_word(&conn, word) {
                    Ok(0) => false,
                    Ok(_) => {
                        out.domain("words_of_power");
                        true
                    }
                    Err(e) => {
                        eprintln!("[chat_status_dispatch] Failed to remove used word of power: {e}");
                        false
                    }
                }
            } else {
                false
            }
        }
        ChatStatusEvent::MilkedBy { milker, timestamp } => {
            if let Some(id) = ctx.identity
                && record_player_milk(ctx.db, &id.character, &id.server, milker, "milked_by", timestamp) {
                    out.domain("milking");
                    true
                } else {
                false
            }
        }
        ChatStatusEvent::LevelUp { skill, level, xp, timestamp } => {
            if let Some(id) = ctx.identity {
                let resolved = ctx.game_data
                    .try_read()
                    .ok()
                    .and_then(|gd| gd.resolve_skill(skill).map(|info| (info.id as i64, info.name.clone())));
                match resolved {
                    Some((skill_id, display_name)) => {
                        if let Ok(conn) = ctx.db.get_write() {
                            match crate::db::game_state_commands::apply_chat_level_up(
                                &crate::db::game_state_commands::ChatLevelUp {
                                    conn: &conn,
                                    character: &id.character,
                                    server: &id.server,
                                    skill_id,
                                    skill_name: &display_name,
                                    level: *level as i64,
                                    xp: *xp as i64,
                                    confirmed_at: timestamp,
                                },
                            ) {
                                Ok(true) => {
                                    out.domain("skills");
                                    true
                                }
                                Ok(false) => false,
                                Err(e) => {
                                    eprintln!(
                                        "[chat_status_dispatch] Chat level-up apply failed for {skill} L{level} ({}@{}): {e}",
                                        id.character, id.server
                                    );
                                    false
                                }
                            }
                        } else {
                            false
                        }
                    }
                    None => {
                        out.unknown_skills.push(skill.clone());
                        false
                    }
                }
            } else {
                false
            }
        }
        ChatStatusEvent::ReportSaved { file_path, .. } => {
            // Documented exclusion: scan paths must not auto-import (see
            // module docs). Report the path so the caller can log it.
            out.report_saved_paths.push(file_path.clone());
            // No persist key in the dispatch (import is caller-side and
            // live-only), so scan stays silent and live emits.
            !ctx.is_scan
        }
        // XpGained, ProdigyXpGained, TreasureDistance, AnatomyResult: no
        // persist key, so gate on live like the currency arms — scan stays
        // silent, live unchanged.
        _ => !ctx.is_scan,
    }
}

/// True when a UTC `"%Y-%m-%d %H:%M:%S"` timestamp is older than the
/// unlinked-gain retention window. Fail-open: malformed timestamps never
/// skip (the sweeper owns those). Lexicographic compare is chronological
/// in this fixed format.
fn is_older_than_retention(timestamp: &str) -> bool {
    if timestamp.len() != 19 {
        return false;
    }
    let cutoff = chrono::Utc::now() - chrono::Duration::hours(crate::db::unknown_source_commands::UNLINKED_GAIN_RETENTION_HOURS);
    timestamp < cutoff.format("%Y-%m-%d %H:%M:%S").to_string().as_str()
}

/// Insert a chat-sourced item ledger row, skipping when the identical row
/// already exists (replay-safe re-scan). Returns `true` when a row was
/// written. Survey attribution stamps `survey_chat` rows when the live hook
/// supplied a use id; scan paths leave rows untagged for the tracker's
/// retro-claim step.
fn insert_item_transaction_guarded(db: &DbPool, item: ItemLedgerCtx<'_>) -> bool {
    // Kill-only ledger: scans never write provisional rows that are already
    // past the link-up window — the end-of-scan sweep would delete them in
    // the same pass. Sourced rows (survey tag) always write; live lines are
    // always fresh so the live drain never skips.
    if item.is_scan && item.survey_use_id.is_none() && is_older_than_retention(item.timestamp) {
        return false;
    }
    let Ok(conn) = db.get_write() else {
        return false;
    };
    let (source_kind, source_details): (Option<&str>, Option<String>) = match item.survey_use_id {
        Some(id) => (Some("survey_chat"), Some(format!("{{\"survey_use_id\":{id}}}"))),
        None => (None, None),
    };
    // Replay guard: the natural key of a chat ledger row plus the looted-from
    // entity when the caller knows it, plus this line's stable identity.
    // `None` entity (every English line — the line carries no entity) matches
    // any row's entity, and a `None` line id (legacy callers) matches any
    // row's identity: legacy match-any collapse, preserving replay collapse.
    // `Some` line id collapses only across representations or onto this
    // line's own trace: a true replay (a row carrying this line's id) or a
    // row this line already consumed (`english_claimed_by`, set below) still
    // skips. The two line-blind disjuncts (`from_entity_id IS NOT NULL`,
    // `json_twin IS NOT NULL`) are gone: a stamped/JSON row from another
    // line no longer collapses a genuine second receipt — that receipt falls
    // through to the consume step and inserts its own row.
    // Pre-identity (`NULL`) rows keep match-any collapse so the first
    // post-upgrade rescan cannot duplicate history. No UNIQUE index exists
    // (adding one would reindex every install), so the check is an explicit
    // SELECT — same-connection, no race on the single writer.
    let exists: bool = conn
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM item_transactions
             WHERE timestamp = ?1 AND character_name = ?2 AND server_name = ?3
               AND item_name = ?4 AND quantity = ?5 AND context = ?6 AND source = 'chat_status'
               AND (?7 IS NULL OR from_entity_id IS ?7)
               AND (?8 IS NULL OR source_line_id IS NULL OR source_line_id IS ?8
                    OR json_extract(source_details, '$.english_claimed_by') IS ?8))",
            rusqlite::params![item.timestamp, item.character, item.server, item.item_name, item.quantity as i32, item.context, item.from_entity_id, item.source_line_id],
            |r| r.get(0),
        )
        .unwrap_or(false);
    if exists {
        return false;
    }
    // Consume marker (twin-second): on a replay-check miss, claim one
    // pending JSON-twin row for this line instead of inserting a twin —
    // twin-second stages nothing because the JSON receipt already staged the
    // gain. Pending = carries the `json_twin` marker and is not already
    // consumed by another line (`english_claimed_by` NULL-or-self).
    // Completed pairs are never consumable: a JSON-first completed row
    // carries an `english_claimed_by` trace, and an English-first completed
    // row carries the JSON claim (`claimed_by`) with no `json_twin` marker,
    // so neither is a consume candidate. The claim is entity-scoped exactly
    // like the replay check (`None` matches any row), so an
    // entity-knowing caller only consumes its own entity's row — sourced-JSON
    // twin-second convergence is unchanged (`None` match-any). A genuine
    // second receipt finds no pending twin and falls through to the INSERT
    // below. Legacy
    // `None` line ids skip the marker (match-any already hit above). A
    // failed UPDATE reports and stops: falling through to the INSERT on a
    // broken connection would duplicate the gain (same fail-closed shape as
    // the JSON twin claim).
    if let Some(line_id) = item.source_line_id {
        let claimed = conn.execute(
            "UPDATE item_transactions
                SET source_details = json_set(COALESCE(source_details, '{}'), '$.english_claimed_by', ?8)
             WHERE rowid = (SELECT MIN(rowid) FROM item_transactions
                 WHERE timestamp = ?1 AND character_name = ?2 AND server_name = ?3
                   AND item_name = ?4 AND quantity = ?5 AND context = ?6 AND source = 'chat_status'
                   AND (?7 IS NULL OR from_entity_id IS ?7)
                   AND json_extract(source_details, '$.json_twin') IS NOT NULL
                   AND (json_extract(source_details, '$.english_claimed_by') IS NULL
                        OR json_extract(source_details, '$.english_claimed_by') IS ?8))",
            rusqlite::params![
                item.timestamp,
                item.character,
                item.server,
                item.item_name,
                item.quantity as i32,
                item.context,
                item.from_entity_id,
                line_id,
            ],
        );
        match claimed {
            Ok(n) if n > 0 => return false,
            Ok(_) => {}
            Err(e) => {
                eprintln!("[dispatch] english twin-consume update failed, receipt left for retry: {e}");
                return false;
            }
        }
    }
    conn.execute(
        "INSERT INTO item_transactions
            (timestamp, character_name, server_name, item_name,
             internal_name, item_type_id, quantity, context,
             source, source_kind, source_details, from_entity_id, source_line_id)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 'chat_status', ?9, ?10, ?11, ?12)",
        rusqlite::params![
            item.timestamp, item.character, item.server, item.item_name, item.internal_name,
            item.item_type_id, item.quantity as i32, item.context, source_kind, source_details,
            item.from_entity_id, item.source_line_id,
        ],
    )
    .is_ok()
}


/// Twin-aware insert for a VIP JSON `AddItem` receipt. The English `[Status]`
/// twin (same timestamp, item, quantity, `context = 'loot'`,
/// `source = 'chat_status'`) may already exist — live order is English-first,
/// rescan order varies. When it does, stamp the looted-from entity onto that
/// row instead of inserting a second one, and report "already present" so the
/// caller stages the gain exactly once. The claim targets ENGLISH rows only:
/// JSON-originated rows carry `{"json_twin":true}` in source_details (set at
/// insert) and are never stamped — an unsourced JSON row is not an English
/// twin, and stamping it would break its own replay's exact-entity hit.
/// Survey columns are never touched: the row keeps `source_kind IS NULL`
/// (or its live survey tag), so the existing live attribution and scan
/// retro-claim paths work unchanged.
/// Returns `true` when the caller should treat this as a new gain (stage it).
#[allow(clippy::too_many_arguments)]
fn insert_json_receipt_guarded(
    db: &DbPool,
    timestamp: &str,
    character: &str,
    server: &str,
    item_name: &str,
    internal_name: Option<&str>,
    item_type_id: Option<i64>,
    quantity: u32,
    from_name: Option<&str>,
    from_entity_id: Option<i64>,
    // This receipt's stable line identity (`DispatchCtx::source_line_id`):
    // `None` (failed raw store) keeps the legacy content-key collapse.
    // `Some("file:offset")` narrows both existence checks to a same-line
    // replay or this line's own prior claim, so a genuine second receipt
    // (different offset) inserts instead of collapsing onto the first row.
    source_line_id: Option<&str>,
    // Mirrors `DispatchCtx::is_scan`: scans skip bare provisional receipts
    // already past the link-up window; sourced rows (entity present) always
    // write so the twin claim and kill link converge on rescans.
    is_scan: bool,
) -> bool {
    // Kill-only ledger, JSON side: a bare old receipt could never attribute,
    // and the end-of-scan sweep would delete it in the same pass.
    if is_scan && from_entity_id.is_none() && is_older_than_retention(timestamp) {
        return false;
    }
    let Ok(conn) = db.get_write() else {
        return false;
    };
    // 1. Narrowed replay check: the content key + exact entity, and this
    // line's own trace — a row carrying this line's id (same-line replay)
    // or a row this line already claimed — or a pre-identity (`NULL`) row,
    // which keeps match-any collapse so the first post-upgrade rescan
    // cannot duplicate history. A `None` line id (every production caller
    // today) matches any row: the legacy exact shape, unchanged. A sourced
    // replay hits its own row here and never stamps an unsourced twin
    // (T44); a genuine second receipt (different line, same entity) misses
    // and falls through to the claim instead of collapsing.
    let exact_exists: bool = conn
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM item_transactions
             WHERE timestamp = ?1 AND character_name = ?2 AND server_name = ?3
               AND item_name = ?4 AND quantity = ?5 AND context = 'loot' AND source = 'chat_status'
               AND (from_entity_id = ?6 OR (?6 IS NULL AND from_entity_id IS NULL))
               AND (?7 IS NULL OR source_line_id IS ?7
                    OR json_extract(source_details, '$.claimed_by') IS ?7
                    OR source_line_id IS NULL))",
            rusqlite::params![timestamp, character, server, item_name, quantity as i32, from_entity_id, source_line_id],
            |r| r.get(0),
        )
        .unwrap_or(false);
    // 2. Only when that misses, claim at most one genuinely unstamped
    // ENGLISH twin. Presence of FromEntityId locates the twin; the display
    // name stays optional. Single-row scoping via MIN(rowid). Provenance:
    // JSON-originated rows carry a `json_twin` marker in source_details
    // (set at insert below) and are never claimed — an unsourced JSON row
    // is not an English twin, and stamping it would destroy the NULL-entity
    // marker its own replay needs for the exact hit (unsourced-first order
    // would then insert a duplicate row on rescan). Pre-marker rows have
    // NULL details and stay claimable (they predate JSON ingestion).
    // Rows already claimed by another line are excluded, so a genuine
    // second receipt claims the second English twin row instead of
    // collapsing onto the first receipt's stamped row. The claim records
    // this line as the claimant (`claimed_by`, alongside the entity stamp
    // — never overwriting `source_line_id`, which stays the English line's
    // so its own rescan still same-line hits). A `None` line id keeps the
    // legacy UPDATE verbatim (no details touch): byte-identical legacy path.
    // A failed UPDATE reports and stops: falling through to the insert on a
    // broken connection would record a duplicate of the English gain.
    if !exact_exists && from_entity_id.is_some() {
        let claimed = if let Some(line_id) = source_line_id {
            conn.execute(
                "UPDATE item_transactions SET from_name = ?1, from_entity_id = ?2,
                    source_details = json_set(COALESCE(source_details, '{}'), '$.claimed_by', ?8)
                 WHERE rowid = (SELECT MIN(rowid) FROM item_transactions
                     WHERE timestamp = ?3 AND character_name = ?4 AND server_name = ?5
                       AND item_name = ?6 AND quantity = ?7 AND context = 'loot'
                       AND source = 'chat_status'
                       AND from_name IS NULL AND from_entity_id IS NULL
                       AND (source_details IS NULL
                            OR json_extract(source_details, '$.json_twin') IS NULL)
                       AND (json_extract(source_details, '$.claimed_by') IS NULL
                            OR json_extract(source_details, '$.claimed_by') = ?8))",
                rusqlite::params![
                    from_name,
                    from_entity_id,
                    timestamp,
                    character,
                    server,
                    item_name,
                    quantity as i32,
                    line_id
                ],
            )
        } else {
            conn.execute(
                "UPDATE item_transactions SET from_name = ?1, from_entity_id = ?2
                 WHERE rowid = (SELECT MIN(rowid) FROM item_transactions
                     WHERE timestamp = ?3 AND character_name = ?4 AND server_name = ?5
                       AND item_name = ?6 AND quantity = ?7 AND context = 'loot'
                       AND source = 'chat_status'
                       AND from_name IS NULL AND from_entity_id IS NULL
                       AND (source_details IS NULL
                            OR json_extract(source_details, '$.json_twin') IS NULL))",
                rusqlite::params![
                    from_name,
                    from_entity_id,
                    timestamp,
                    character,
                    server,
                    item_name,
                    quantity as i32
                ],
            )
        };
        if let Err(e) = claimed {
            eprintln!("[dispatch] twin-claim update failed, receipt left for retry: {e}");
            return false;
        }
    }

    // Re-check after a possible twin claim, with the same narrowed shape
    // as step 1 (this is the correction: the old content-key-only shape
    // re-collapsed a genuine second receipt onto the first receipt's row).
    // Hit (own insert replayed, or own claim replayed) → no insert; the
    // caller stages nothing because the English twin already staged the gain.
    let exists_after: bool = conn
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM item_transactions
             WHERE timestamp = ?1 AND character_name = ?2 AND server_name = ?3
               AND item_name = ?4 AND quantity = ?5 AND context = 'loot' AND source = 'chat_status'
               AND (from_entity_id = ?6 OR (?6 IS NULL AND from_entity_id IS NULL))
               AND (?7 IS NULL OR source_line_id IS ?7
                    OR json_extract(source_details, '$.claimed_by') IS ?7
                    OR source_line_id IS NULL))",
            rusqlite::params![timestamp, character, server, item_name, quantity as i32, from_entity_id, source_line_id],
            |r| r.get(0),
        )
        .unwrap_or(false);

    let wrote = if exists_after {
        false
    } else {
        conn.execute(
            "INSERT INTO item_transactions
                (timestamp, character_name, server_name, item_name,
                 internal_name, item_type_id, quantity, context,
                 source, from_name, from_entity_id, source_details, source_line_id)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'loot', 'chat_status', ?8, ?9, ?10, ?11)",
            rusqlite::params![
                timestamp,
                character,
                server,
                item_name,
                internal_name,
                item_type_id,
                quantity as i32,
                from_name,
                from_entity_id,
                // Provenance: this row came from a JSON receipt, never an
                // English Status line, so the twin claim above must not stamp
                // it. Readers use json_extract (survey_use_id et al.) and
                // ignore unknown keys; the survey tagger merges via json_set
                // (coordinator live-attribution block), preserving the marker.
                "{\"json_twin\":true}",
                source_line_id,
            ],
        )
        .is_ok()
    };
    // Both twin orders converge here, and the receipt's entity stamps onto
    // the ledger row above (twin-claim stamping for English-first pairs,
    // direct insert for JSON-first). That stamping is now the SOLE entity
    // source the kill↔loot read-time join depends on — the retired
    // `link_kill_loot` call used to re-derive it here, so the stamp path
    // must stay exactly as is. No link call: mine-scope readers join
    // `item_transactions` to `enemy_kills` on next query.
    wrote
}

/// Chat XP without a level-up: same row shape as the level-up writer, minus
/// the level gate (XP within a level carries no level signal). No writer
/// for this existed — `[Status]` XP lines were parsed but never persisted,
/// and `Player.log` skill snapshots are gone.
/// `xp` here is an AWARDED INCREMENT, not a total (proven by the live
/// fixture: Riding 250 → 350 → 300 across three lines — totals cannot fall).
/// Identity comes from the v101 per-award ledger (`apply_chat_xp_award`,
/// shared with the level-up writer): a replayed award finds its identity
/// row and skips, while a same-second or out-of-order award with a distinct
/// identity applies. Same-timestamp same-xp same-level pairs still
/// collapse — second-granularity logs give no key separating a genuine
/// duplicate from its own replay (accepted limit).
#[allow(clippy::too_many_arguments)]
fn upsert_chat_xp(
    conn: &rusqlite::Connection,
    character: &str,
    server: &str,
    skill_id: i64,
    skill_name: &str,
    level: i64,
    xp: i64,
    confirmed_at: &str,
) -> bool {
    crate::db::game_state_commands::apply_chat_xp_award(
        &crate::db::game_state_commands::ChatXpAward {
            conn,
            character,
            server,
            skill_id,
            skill_name,
            level,
            xp,
            confirmed_at,
        },
    )
    .unwrap_or(false)
}

/// Classify a `[JSON:Book]` title into the `game_state_books.book_type` the
/// old `ProcessBook` flow would have carried. Frozen from the first live
/// lines (three distinct titles); anything else stores as `"JSON"` (upsert
/// only, no content triggers) until its schema is observed.
fn classify_json_book_title(title: &str) -> &'static str {
    if title == "Skill Info" {
        "SkillReport"
    } else if title.starts_with("Behavior Report") {
        "HelpScreen"
    } else if title.ends_with("'s Age") {
        "PlayerAge"
    } else {
        "JSON"
    }
}

/// Structured dispatch for one parsed `[JSON:Type]` line. Runs inside the
/// shared [`dispatch_chat_message`] so live, scan, and replay converge.
/// Twin rule (see `docs/features/vip-json-logging.md`): the English
/// `[Status]` twin carries no new information — JSON is authoritative when
/// present, and every arm is idempotent across both orders via the same
/// guards the English arms use.
fn dispatch_json_event(
    ctx: &DispatchCtx<'_>,
    event: &crate::chat_json_parser::ChatJsonEvent,
    out: &mut DispatchOutcome,
    _state: &mut DispatchState<'_>,
) {
    use crate::chat_json_parser::ChatJsonEvent as E;
    match event {
        E::ItemReceipt {
            timestamp,
            item_name,
            quantity,
            from_name,
            from_entity_id,
            ..
        } => {
            if let Some(id) = ctx.identity {
                // Same CDN resolution as the English `ItemGained` arm so the
                // twin writes identical `internal_name`/`item_type_id`
                // (the line's own `TypeId` is not stored — twin-identical wins).
                let (internal_name, item_type_id): (Option<String>, Option<i64>) = ctx
                    .game_data
                    .try_read()
                    .ok()
                    .and_then(|gd| {
                        gd.resolve_item(item_name)
                            .map(|info| (info.internal_name.clone(), Some(info.id as i64)))
                    })
                    .unwrap_or((None, None));
                if insert_json_receipt_guarded(
                    ctx.db,
                    timestamp,
                    &id.character,
                    &id.server,
                    item_name,
                    internal_name.as_deref(),
                    item_type_id,
                    *quantity,
                    from_name.as_deref(),
                    (*from_entity_id).map(|n| n as i64),
                    ctx.source_line_id.as_deref(),
                    ctx.is_scan,
                ) {
                    out.staged_gains.push((item_name.clone(), *quantity));
                    // Replay, twin-second, or missing row: nothing further
                    // stages. A missing row (retention skip, failed receipt
                    // write) is retried by the next rescan's receipt insert.
                }
            }
        }
        E::Xp {
            timestamp,
            skill,
            xp,
            old_level,
            new_level,
        } => {
            if let Some(id) = ctx.identity {
                let resolved = ctx.game_data.try_read().ok().and_then(|gd| {
                    gd.resolve_skill(skill)
                        .map(|info| (info.id as i64, info.name.clone()))
                });
                match resolved {
                    Some((skill_id, display_name)) => {
                        if let Ok(conn) = ctx.db.get_write() {
                            if new_level > old_level {
                                // Level-up: the existing chat writer (same call as the `LevelUp` arm).
                                match crate::db::game_state_commands::apply_chat_level_up(
                                    &crate::db::game_state_commands::ChatLevelUp {
                                        conn: &conn,
                                        character: &id.character,
                                        server: &id.server,
                                        skill_id,
                                        skill_name: &display_name,
                                        level: *new_level as i64,
                                        xp: *xp as i64,
                                        confirmed_at: timestamp,
                                    },
                                ) {
                                    Ok(true) => out.domain("skills"),
                                    Ok(false) => {}
                                    Err(e) => eprintln!(
                                        "[chat_status_dispatch] JSON XP level-up apply failed for {skill} ({}@{}): {e}",
                                        id.character, id.server
                                    ),
                                }
                            } else if upsert_chat_xp(
                                &conn,
                                &id.character,
                                &id.server,
                                skill_id,
                                &display_name,
                                *new_level as i64,
                                *xp as i64,
                                timestamp,
                            ) {
                                out.domain("skills");
                            }
                        }
                    }
                    None => out.unknown_skills.push(skill.clone()),
                }
            }
        }
        E::Book { timestamp, title, body } => {
            if let Some(id) = ctx.identity {
                let book_type = classify_json_book_title(title);
                // `current` tracks whether the row is known to hold this
                // body: identical content (rescan no-op) or a successful
                // write. A failed write leaves it false so caller-side
                // imports never derive content tables from a row that
                // never landed.
                let mut current = false;
                if let Ok(conn) = ctx.db.get_write() {
                    // Same upsert as `persist_book_content` (no emit here —
                    // the caller owns emits). No `books` domain: nothing in
                    // the FE consumes it, so writes stay silent.
                    // Gate on content: identical re-reads skip the write, so
                    // rescans stay silent like every other arm.
                    let same: bool = conn
                        .query_row(
                            "SELECT content FROM game_state_books
                             WHERE character_name = ?1 AND server_name = ?2
                               AND book_type = ?3 AND title = ?4",
                            rusqlite::params![id.character, id.server, book_type, title],
                            |r| r.get::<_, String>(0),
                        )
                        .map(|c| c == *body)
                        .unwrap_or(false);
                    current = same;
                    // The book's own observation time, normalized to the
                    // RFC3339 shape `persist_book_content` writes: a naive
                    // stamp sorts before any RFC3339 row, so without this an
                    // older watcher row would beat every newer live book.
                    let observed = utc_naive_to_rfc3339(timestamp);
                    if same {
                        // Identical re-read: advance the observation time so
                        // this row stays the newest observation. No domain,
                        // no imports — rescan stays silent.
                        if let Err(e) = conn.execute(
                            "UPDATE game_state_books SET captured_at = ?5
                             WHERE character_name = ?1 AND server_name = ?2
                               AND book_type = ?3 AND title = ?4
                               AND captured_at < ?5",
                            rusqlite::params![id.character, id.server, book_type, title, observed],
                        ) {
                            // Recency bump failed: the write path is broken, so
                            // fail closed (report + return) rather than falling
                            // through to the INSERT — the return discards
                            // `current` with the function, so caller-side
                            // imports never derive from this row.
                            eprintln!("[dispatch] JSON book captured_at bump failed: {e}");
                            return;
                        }
                    }
                    if !same
                        && conn
                            .execute(
                                "INSERT INTO game_state_books (character_name, server_name, book_type, title, content, captured_at)
                                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                                 ON CONFLICT(character_name, server_name, book_type, title) DO UPDATE SET
                                    content = excluded.content,
                                    captured_at = excluded.captured_at
                                 WHERE captured_at <= excluded.captured_at",
                                rusqlite::params![id.character, id.server, book_type, title, body, observed],
                            )
                            .map(|n| n > 0)
                            .unwrap_or(false)
                    {
                        current = true;
                    }
                }
                // Content triggers (gourmand/hoplology/teleports/report-stats)
                // run caller-side: they need `AppHandle` for their emits.
                // Bodies are serde_json-decoded, so newlines are already real —
                // no `normalize_book_content` step.
                if current {
                    out.json_book = Some((book_type.to_string(), title.clone(), body.clone(), timestamp.clone()));
                }
            }
        }
        E::FavorDelta {
            timestamp,
            npc,
            delta,
        } => {
            if let Some(id) = ctx.identity {
                // JSON carries the CDN key already ("NPC_Elune"); resolve the
                // display name, falling back to the key like the old arms.
                let (display_name, giftable): (String, bool) = ctx
                    .game_data
                    .try_read()
                    .ok()
                    .and_then(|gd| {
                        gd.resolve_npc(npc).map(|info| {
                            let giftable = !crate::game_data::is_gift_capped_area(
                                info.area_name.as_deref(),
                            ) && crate::game_data::has_gift_tastes(info);
                            (info.name.clone(), giftable)
                        })
                    })
                    // Unknown NPC (CDN gap) defaults to giftable: a positive
                    // delta is more likely a real gift than quest noise, and a
                    // missed real gift is worse than a spurious gift row.
                    .unwrap_or_else(|| (npc.clone(), true));
                if let Ok(conn) = ctx.db.get_write() {
                    // Event log: one row per observed line. Rescans and scan
                    // replays collapse onto the row carrying their own line
                    // id; session sums and gift detection derive from this
                    // table by query, so there is no running total to keep
                    // consistent. Two genuinely distinct same-second
                    // same-delta gifts carry different `file:offset` ids and
                    // insert as separate rows (post-#352 line identity); the
                    // old (npc, second, delta) key could not separate them.
                    // Single write transaction for event + touch-up + gift:
                    // a gift-log failure rolls back the event, so a later
                    // replay retries the whole unit instead of leaving a
                    // gift-less event behind (INSERT OR IGNORE keeps
                    // successful replays no-ops). Same unchecked-transaction
                    // shape as the survey aggregator: the pool checkout is
                    // exclusive and no outer transaction is open here.
                    let tx = match rusqlite::Transaction::new_unchecked(
                        &conn,
                        rusqlite::TransactionBehavior::Immediate,
                    ) {
                        Ok(tx) => tx,
                        Err(e) => {
                            eprintln!("[chat_status_dispatch] favor txn begin failed: {e}");
                            return;
                        }
                    };
                    // Upgrade-boundary replay guard (ledger-guard shape): a
                    // stamped line matching a pre-identity (`NULL`) row is a
                    // rescan replay, not a genuine second gift — the COALESCE
                    // index would see key+id vs key+'' as distinct and
                    // duplicate it. `None` (failed raw store) keeps legacy
                    // match-any collapse; `Some` collapses only onto its own
                    // line's row or a pre-identity row, never onto another
                    // line's stamped row. Same-transaction SELECT on the
                    // pool's exclusive write checkout: no race, and the
                    // event + touch-up + gift unit stays atomic.
                    let source_line_id = ctx.source_line_id.as_deref();
                    let seen: bool = tx
                        .query_row(
                            "SELECT EXISTS(SELECT 1 FROM favor_events
                             WHERE character_name = ?1 AND server_name = ?2 AND npc_key = ?3
                               AND timestamp = ?4 AND delta = ?5
                               AND (?6 IS NULL OR source_line_id IS NULL OR source_line_id IS ?6))",
                            rusqlite::params![id.character, id.server, npc, timestamp, *delta, source_line_id],
                            |r| r.get(0),
                        )
                        .unwrap_or(false);
                    if seen {
                        return;
                    }
                    let fresh = tx
                        .execute(
                            "INSERT OR IGNORE INTO favor_events
                                (character_name, server_name, npc_key, timestamp, delta, source_line_id)
                             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                            rusqlite::params![id.character, id.server, npc, timestamp, *delta, source_line_id],
                        )
                        .map(|n| n > 0)
                        .unwrap_or(false);
                    if !fresh {
                        return;
                    }
                    // Tier/name touch-up so log-only NPCs (no report row yet)
                    // still render. Snapshot tiers are never overwritten with
                    // NULL — the upsert only fills name and recency.
                    // Recency-guarded like the snapshot seeder: an unseen
                    // older-stamped delta (catch-up scan of a rotated log)
                    // must not rewind `last_confirmed_at`, or a later report
                    // with a middle timestamp would replace a newer tier.
                    if let Err(e) = tx.execute(
                        "INSERT INTO game_state_favor (character_name, server_name, npc_key, npc_name, favor_tier, last_confirmed_at, source)
                         VALUES (?1, ?2, ?3, ?4, NULL, ?5, 'log')
                         ON CONFLICT(character_name, server_name, npc_key) DO UPDATE SET
                            npc_name = excluded.npc_name,
                            last_confirmed_at = excluded.last_confirmed_at
                         WHERE excluded.last_confirmed_at > game_state_favor.last_confirmed_at",
                        rusqlite::params![id.character, id.server, npc, display_name, timestamp],
                    ) {
                        eprintln!("[chat_status_dispatch] favor touch-up failed, rolled back: {e}");
                        return;
                    }
                    // capped-area counts are manual-only (the Statehelm +/- UI;
                    // the user reads the game's cap note): no report field feeds
                    // game_state_gift_log, so there is no report reconcile to
                    // restore — only the chat path (uncapped) and manual gifts
                    // write this table. Quest-payout noise for uncapped NPCs is
                    // accepted and documented — JSON carries no is_gift flag.
                    // Favor losses are never gifts, whatever the NPC's tastes.
                    if giftable
                        && *delta > 0.0
                        && let Err(e) = tx.execute(
                            "INSERT INTO game_state_gift_log (character_name, server_name, npc_key, npc_name, gifted_at, favor_delta)
                             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                            rusqlite::params![
                                id.character, id.server, npc, display_name, timestamp, *delta
                            ],
                        )
                    {
                        eprintln!("[chat_status_dispatch] gift-log insert failed, rolled back: {e}");
                        return;
                    }
                    if let Err(e) = tx.commit() {
                        eprintln!("[chat_status_dispatch] favor txn commit failed: {e}");
                        return;
                    }
                    out.domain("favor");
                }
            }
        }
        E::RecipeUsed {
            timestamp,
            recipe_id,
            completions,
            ..
        } => {
            if let Some(id) = ctx.identity {
                // Same absolute-count upsert as the old `RecipeUpdated` arm,
                // with one refinement: an equal-timestamp snapshot carrying a
                // GREATER total still applies (two crafts in one second).
                // Identical resends (same ts, same total) stay no-ops, and
                // older or lesser snapshots never move the total backward.
                if let Ok(conn) = ctx.db.get_write() {
                    let rows = conn.execute(
                        "INSERT INTO game_state_recipes (character_name, server_name, recipe_id, completion_count, last_confirmed_at, source)
                         VALUES (?1, ?2, ?3, ?4, ?5, 'log')
                         ON CONFLICT(character_name, server_name, recipe_id) DO UPDATE SET
                            completion_count = excluded.completion_count,
                            last_confirmed_at = excluded.last_confirmed_at,
                            source = excluded.source
                         WHERE excluded.last_confirmed_at > game_state_recipes.last_confirmed_at
                            OR (excluded.last_confirmed_at = game_state_recipes.last_confirmed_at
                                AND excluded.completion_count > game_state_recipes.completion_count)",
                        rusqlite::params![
                            id.character, id.server, *recipe_id as i64, *completions as i64, timestamp
                        ],
                    )
                    .unwrap_or(0);
                    if rows > 0 {
                        out.domain("recipes");
                    }
                }
            }
        }
        E::PigeonReceived {
            timestamp,
            title,
            body,
        } => {
            if let Some(id) = ctx.identity {
                // Receive direction only — the game emits no line for sent
                // pigeons (verified 2026-11-17), so `direction` is always in.
                if let Some(sender) = parse_pigeon_sender(title) {
                    let item = parse_pigeon_item(body);
                    if record_player_message(
                        ctx.db,
                        &id.character,
                        &id.server,
                        timestamp,
                        "pigeon",
                        "in",
                        &sender,
                        body,
                        item.as_deref(),
                    ) {
                        out.domain("player_messages");
                    }
                } else {
                    // Counting, not dropping: a title that is not
                    // "Pigeon From <name>" (a future send line, format drift)
                    // must stay visible in malformed counters, never vanish.
                    out.malformed_json.push("Pigeon".to_string());
                }
            }
        }
        E::ParagonXp { .. } => {
            // Deliberate no-op: per-kill prodigy flow has no DB home (the
            // prodigy tally is in-memory from the English Status twin, which
            // already covers live). Typed so the shape is pinned and
            // `unknown_json` stays clean for genuinely unobserved Types.
        }
        E::EntityReport { timestamp, title, body } => {
            if let Some(id) = ctx.identity
                && record_player_message(
                    ctx.db,
                    &id.character,
                    &id.server,
                    timestamp,
                    "entity_report",
                    "in",
                    title,
                    body,
                    None,
                )
            {
                out.domain("player_messages");
            }
        }
        E::WordDiscovered {
            timestamp,
            word,
            power_name,
            description,
        } => {
            if let Some(id) = ctx.identity
                && let Ok(conn) = ctx.db.get_write()
            {
                // Words are consumed game-wide when spoken, so the word
                // itself is the rescan key: rediscovery never duplicates.
                let rows = conn
                    .execute(
                        "INSERT INTO words_of_power (character_name, server_name, word, power_name, description, discovered_at, source)
                         SELECT ?1, ?2, ?3, ?4, ?5, ?6, 'chat'
                         WHERE NOT EXISTS (SELECT 1 FROM words_of_power
                                            WHERE character_name = ?1 AND server_name = ?2 AND word = ?3)",
                        rusqlite::params![id.character, id.server, word, power_name, description, timestamp],
                    )
                    .unwrap_or(0);
                if rows > 0 {
                    out.domain("words_of_power");
                }
            }
        }
        E::QuestAccepted { timestamp, quest_id } => {
            if let Some(id) = ctx.identity
                && let Ok(conn) = ctx.db.get_write()
            {
                // Name best-effort at write time: CDN `quest_<id>` → `Name`,
                // else the daily fallback maps; NULL stays id-only.
                let quest_name: Option<String> = ctx
                    .game_data
                    .try_read()
                    .ok()
                    .and_then(|gd| crate::quest_names::quest_display_name(&gd, *quest_id));
                // Ledger-guard shape (favor/council precedent): a stamped
                // line replayed collapses onto its own row; a `None` line id
                // keeps legacy match-any collapse; a genuinely distinct
                // same-second accept carries a different line id and inserts.
                // Single statement, single table: atomic without a txn (the
                // BEGIN IMMEDIATE rule covers multi-table arms only).
                let source_line_id = ctx.source_line_id.as_deref();
                let seen: bool = conn
                    .query_row(
                        "SELECT EXISTS(SELECT 1 FROM quest_accepts
                         WHERE character_name = ?1 AND server_name = ?2
                           AND accepted_at = ?3 AND quest_id = ?4
                           AND (?5 IS NULL OR source_line_id IS NULL OR source_line_id IS ?5))",
                        rusqlite::params![id.character, id.server, timestamp, *quest_id as i64, source_line_id],
                        |r| r.get(0),
                    )
                    .unwrap_or(false);
                if seen {
                    return;
                }
                if conn
                    .execute(
                        "INSERT OR IGNORE INTO quest_accepts
                            (character_name, server_name, accepted_at, quest_id, quest_name, source_line_id)
                         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                        rusqlite::params![id.character, id.server, timestamp, *quest_id as i64, quest_name, source_line_id],
                    )
                    .map(|n| n > 0)
                    .unwrap_or(false)
                {
                    out.domain("quests");
                }
            }
        }
        E::Unknown { json_type } => {
            out.unknown_json.push(json_type.clone());
        }
    }
}

/// Signed wallet delta with the same guards as the coordinator: no-op
/// without an export anchor, and only events newer than the anchor count.
/// The per-event ledger row (`council_events`, v103) lands regardless of the
/// anchor — pre-anchor and anchor-less events still happened, and the anchor
/// filter applies at read time — but only on accumulating paths:
/// `count_currency = false` scan paths record neither accrual nor ledger
/// (live owns accumulation, as today). Returns true only when a row actually
/// accrued.
#[allow(clippy::too_many_arguments)]
fn accrue_currency_delta(db: &DbPool, character: &str, server: &str, amount: i64, event_ts: &str, kind: &str, source_line_id: Option<&str>) -> bool {
    if amount == 0 {
        return false;
    }
    let Ok(conn) = db.get_write() else {
        return false;
    };
    // Single write transaction for accrual + ledger (favor-arm precedent, same
    // file): a ledger failure rolls back the delta, so a later replay retries
    // the whole unit instead of leaving an unledgered delta behind (INSERT OR
    // IGNORE keeps successful replays no-ops). Same unchecked-transaction
    // shape as the survey aggregator: the pool checkout is exclusive and no
    // outer transaction is open here.
    let tx = match rusqlite::Transaction::new_unchecked(
        &conn,
        rusqlite::TransactionBehavior::Immediate,
    ) {
        Ok(tx) => tx,
        Err(e) => {
            eprintln!("[chat_status_dispatch] council txn begin failed: {e}");
            return false;
        }
    };
    let event_ts = crate::db::game_state_commands::normalize_contract_ts(event_ts);
    // Upgrade-boundary replay guard (favor-ledger shape): a stamped line
    // matching a pre-identity (`NULL`) row is a rescan replay, not a genuine
    // second receipt — the COALESCE index would see key+id vs key+'' as
    // distinct and duplicate it. `None` (failed raw store) keeps legacy
    // match-any collapse; `Some` collapses only onto its own line's row or a
    // pre-identity row, never onto another line's stamped row.
    // Same-transaction SELECT on the pool's exclusive write checkout: no
    // race, and the accrual + ledger unit stays atomic.
    let seen: bool = tx
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM council_events
             WHERE character_name = ?1 AND server_name = ?2
               AND event_ts = ?3 AND delta = ?4 AND kind = ?5
               AND (?6 IS NULL OR source_line_id IS NULL OR source_line_id IS ?6))",
            rusqlite::params![character, server, event_ts, amount, kind, source_line_id],
            |r| r.get(0),
        )
        .unwrap_or(false);
    if !seen
        && let Err(e) = tx.execute(
            "INSERT OR IGNORE INTO council_events
                (character_name, server_name, event_ts, delta, kind, source_line_id)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            rusqlite::params![character, server, event_ts, amount, kind, source_line_id],
        )
    {
        eprintln!("[chat_status_dispatch] council ledger insert failed, rolled back: {e}");
        return false;
    }
    let updated = match tx.execute(
        "UPDATE currency_estimate
        SET delta_since = delta_since + ?1, updated_at = datetime('now')
      WHERE character_name = ?2 AND server_name = ?3
        AND currency_name = 'GOLD'
        AND anchor_at IS NOT NULL
        AND ?4 > anchor_at",
        rusqlite::params![amount, character, server, event_ts],
    ) {
        Ok(n) => n,
        Err(e) => {
            eprintln!("[chat_status_dispatch] currency accrual failed, rolled back: {e}");
            return false;
        }
    };
    if let Err(e) = tx.commit() {
        eprintln!("[chat_status_dispatch] council txn commit failed: {e}");
        return false;
    }
    updated == 1
}

/// Naive UTC (`%Y-%m-%d %H:%M:%S`) → RFC3339 for `*_at` columns shared with
/// wall-clock writers. Falls back to now() on unparseable input so a bad
/// stamp never blocks the write. Unreachable by construction today: every
/// caller passes `NaiveDateTime::format` output in this same shape (probe
/// 2026-11-25 — books `observed`, both milking arms), so the fallback is
/// belt-and-suspenders, not a live path. Do not "fix" the fallback without
/// a malformed-stamp sighting; the phantom-row class it would feed (books
/// anchor, milking key) has no observed trigger.
fn utc_naive_to_rfc3339(event_ts: &str) -> String {
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
    let milked_at = utc_naive_to_rfc3339(timestamp);
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
/// Record a cow milking with the newer-wins upsert. Returns `true` when the
/// write advanced the row (caller emits `milking`); replays that move
/// nothing and write failures report `false`. (The upsert itself always
/// succeeds on an existing row, so `.is_ok()` would report success for a
/// pure replay - the change check lives in `record_cow_milk`.)
pub fn record_cow_milk_session(
    db: &DbPool,
    character: &str,
    server: &str,
    cow_name: &str,
    zone: &str,
    timestamp: &str,
) -> bool {
    let milked_at = utc_naive_to_rfc3339(timestamp);
    let Ok(conn) = db.get_write() else {
        return false;
    };
    match crate::db::game_state_commands::record_cow_milk(&conn, character, server, cow_name, zone, &milked_at) {
        Ok(advanced) => advanced,
        Err(e) => {
            eprintln!("[chat_status_dispatch] Failed to record cow milk: {e}");
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

/// Persist a player/entity message (pigeon mail, entity reports) with a
/// rescan guard (same character/server/timestamp/type/sender/body → skip).
/// The body is in the key: two mails in the same second from one sender
/// must not collapse (live fixture: text-only pigeon and the item-bearing
/// one share a timestamp). Byte-identical resends still collapse, which is
/// the point. Returns `true` when a row was written (caller emits
/// `player_messages`).
#[allow(clippy::too_many_arguments)]
fn record_player_message(
    db: &DbPool,
    character: &str,
    server: &str,
    timestamp: &str,
    message_type: &str,
    direction: &str,
    other_player: &str,
    body: &str,
    item_name: Option<&str>,
) -> bool {
    let Ok(conn) = db.get_write() else {
        return false;
    };
    match conn.execute(
        "INSERT INTO player_messages
            (character_name, server_name, timestamp, message_type, direction, other_player, body, item_name)
         SELECT ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8
         WHERE NOT EXISTS (
            SELECT 1 FROM player_messages
             WHERE character_name = ?1 AND server_name = ?2 AND timestamp = ?3
               AND message_type = ?4 AND direction = ?5 AND other_player = ?6 AND body = ?7
        )",
        rusqlite::params![
            character, server, timestamp, message_type, direction, other_player, body, item_name
        ],
    ) {
        Ok(rows) => rows > 0,
        Err(e) => {
            eprintln!("[chat_status_dispatch] Failed to record player message: {e}");
            false
        }
    }
}

/// Friend name from a `"Pigeon From <name>"` title.
fn parse_pigeon_sender(title: &str) -> Option<String> {
    title
        .strip_prefix("Pigeon From ")
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// Item name from a pigeon body (`<i>You received:\n<item> xN</i>`).
fn parse_pigeon_item(body: &str) -> Option<String> {
    let start = body.find("You received:")? + "You received:".len();
    let rest = &body[start..];
    // Strip `<...>` markup spans (keeps item-name letters intact), then the
    // first text line is `<item> xN`.
    let mut text = String::with_capacity(rest.len());
    let mut in_tag = false;
    for c in rest.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => text.push(c),
            _ => {}
        }
    }
    let line = text.split_whitespace().collect::<Vec<_>>().join(" ");
    // `Mulberry Salad x10` → item is everything before a trailing `xN`.
    let mut parts: Vec<&str> = line.split_whitespace().collect();
    if let Some(last) = parts.last()
        && last.len() > 1
        && last.starts_with('x')
        && last[1..].chars().all(|c| c.is_ascii_digit())
    {
        parts.pop();
    }
    let item = parts.join(" ").trim().to_string();
    (!item.is_empty()).then_some(item)
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
            && tx
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
    tx.commit().is_ok()
}


/// Persist a resuscitation. Returns `true` when a row was written (caller emits the event).
fn persist_rez_event(
    db: &DbPool,
    id: &DispatchIdentity,
    event: &ChatResuscitateEvent,
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
        return false;
    }
    conn.execute(
        "INSERT INTO character_resuscitations
            (character_name, server_name, occurred_at, caster_name, target_name, success, area)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        rusqlite::params![
            &id.character, &id.server, timestamp, caster_name, target_name,
            success, current_area.as_deref(),
        ],
    )
    .is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TempDb {
        path: std::path::PathBuf,
    }

    impl Drop for TempDb {
        fn drop(&mut self) {
            // Call sites bind as `let (_guard, pool)`: pool drops first,
            // releasing the Windows file handle, then this removes db + journals.
            let _ = std::fs::remove_file(&self.path);
            let wal = std::path::PathBuf::from(format!("{}-wal", self.path.display()));
            let shm = std::path::PathBuf::from(format!("{}-shm", self.path.display()));
            let _ = std::fs::remove_file(wal);
            let _ = std::fs::remove_file(shm);
        }
    }

    fn test_pool() -> (TempDb, DbPool) {
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
        (TempDb { path }, pool)
    }

    fn game_data() -> GameDataState {
        std::sync::Arc::new(tauri::async_runtime::RwLock::new(crate::game_data::GameData::empty()))
    }

    fn state<'a>(
        arena: &'a mut ArenaTracker,
        dmg: &'a mut Vec<ChatCombatEvent>,
    ) -> DispatchState<'a> {
        DispatchState {
            arena_tracker: arena,
            recent_damage: dmg,
            current_area: None,
            // Tests exercise arms directly (live-equivalent coverage).
            count_currency: true,
        }
    }

    /// Scan-equivalent state: currency accrual off, like `run_scan_dispatch`.
    fn state_scan<'a>(
        arena: &'a mut ArenaTracker,
        dmg: &'a mut Vec<ChatCombatEvent>,
    ) -> DispatchState<'a> {
        DispatchState {
            arena_tracker: arena,
            recent_damage: dmg,
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
    fn dispatch(
        pool: &DbPool,
        gd: &GameDataState,
        identity: Option<&DispatchIdentity>,
        msg: &ChatMessage,
        local_ts: &str,
        st: &mut DispatchState<'_>,
    ) -> DispatchOutcome {
        dispatch_with_line(pool, gd, identity, msg, local_ts, st, None)
    }
    /// Dispatch with an explicit source-line id: `None` exercises the legacy
    /// match-any path (all pre-existing tests), `Some` the line-keyed guards.
    fn dispatch_with_line(
        pool: &DbPool,
        gd: &GameDataState,
        identity: Option<&DispatchIdentity>,
        msg: &ChatMessage,
        local_ts: &str,
        st: &mut DispatchState<'_>,
        source_line_id: Option<&str>,
    ) -> DispatchOutcome {
        dispatch_chat_message(
            DispatchCtx {
                db: pool,
                game_data: gd,
                identity,
                msg,
                local_ts,
                watch_rules: &[],
                auto_watch_rules: &[],
                source_line_id: source_line_id.map(str::to_string),
                // Live path: old pinned timestamps must still write.
                is_scan: false,
                is_live: true,
            },
            None,
            st,
        )
    }
    fn dispatch_scan(
        pool: &DbPool,
        gd: &GameDataState,
        identity: Option<&DispatchIdentity>,
        msg: &ChatMessage,
        local_ts: &str,
        st: &mut DispatchState<'_>,
    ) -> DispatchOutcome {
        dispatch_chat_message(
            DispatchCtx {
                db: pool,
                game_data: gd,
                identity,
                msg,
                local_ts,
                watch_rules: &[],
                auto_watch_rules: &[],
                source_line_id: None,
                is_scan: true,
                is_live: false,
            },
            None,
            st,
        )
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

    #[test]
    fn scan_persists_status_lines_like_live_drain() {
        let (_guard, pool) = test_pool();
        let gd = game_data();
        seed_skill(&gd, "Sword", 7);
        let mut arena = ArenaTracker::new();
        let mut dmg = Vec::new();
        

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
            let mut st = state(&mut arena, &mut dmg);
            dispatch(&pool, &gd, Some(&id()), m, ts, &mut st);
        }
        // Cow-curse poop comes over Combat, attributed to the file identity.
        let poop = combat_msg("Alice: Poop on Bob!", ts);
        let mut st = state(&mut arena, &mut dmg);
        let o = dispatch(&pool, &gd, Some(&id()), &poop, ts, &mut st);
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
        let mut st = state(&mut arena, &mut dmg);
        dispatch(&pool, &gd, Some(&id()), &m, ts, &mut st);
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
        let mut st = state_scan(&mut arena, &mut dmg);
        dispatch(&pool, &gd, Some(&id()), &m, ts, &mut st);
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
            let mut st = state(&mut arena, &mut dmg);
            dispatch(&pool, &gd, Some(&id()), m, ts, &mut st);
        }
        let mut st = state(&mut arena, &mut dmg);
        dispatch(&pool, &gd, Some(&id()), &poop, ts, &mut st);
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
    fn council_ledger_rows_land_per_event_and_replays_converge() {
        // Plan reconciliation pins (timeseries-history-plan section 2): ledger
        // rows land per event with timestamp + delta + kind + identity, and a
        // rescan/replay converges to one row per line.
        let (_guard, pool) = test_pool();
        let gd = game_data();
        seed_skill(&gd, "Sword", 7);
        let mut arena = ArenaTracker::new();
        let mut dmg = Vec::new();
        pool.get_write()
            .unwrap()
            .execute(
                "INSERT INTO currency_estimate (character_name, server_name, currency_name, anchor_amount, anchor_at, delta_since)
                 VALUES ('Alice','Dreva','GOLD',1000,'2026-04-01 05:00:00',0)",
                [],
            )
            .unwrap();
        let councils = status_msg("You received 10 Councils.", "26-04-01 05:30:53");
        let loot = status_msg("You searched the corpse and found 5 coins.", "26-04-01 05:31:53");
        let mut st = state(&mut arena, &mut dmg);
        dispatch_with_line(&pool, &gd, Some(&id()), &councils, "26-04-01 05:30:53", &mut st, Some("chat.log:100"));
        let mut st = state(&mut arena, &mut dmg);
        dispatch_with_line(&pool, &gd, Some(&id()), &loot, "26-04-01 05:31:53", &mut st, Some("chat.log:200"));
        let conn = pool.get_write().unwrap();
        let rows: Vec<(String, i64, String, Option<String>)> = conn
            .prepare("SELECT event_ts, delta, kind, source_line_id FROM council_events ORDER BY event_ts")
            .unwrap()
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
            .unwrap()
            .map(|r| r.unwrap())
            .collect();
        assert_eq!(
            rows,
            vec![
                ("2026-04-01 05:30:53".to_string(), 10, "councils_changed".to_string(), Some("chat.log:100".to_string())),
                ("2026-04-01 05:31:53".to_string(), 5, "coins_looted".to_string(), Some("chat.log:200".to_string())),
            ],
            "one ledger row per event, timestamp + delta + kind + identity"
        );
        // Anchor-reconciliation contract: delta_since equals the post-anchor
        // ledger sum while every post-anchor event accrued exactly once.
        let delta: i64 = conn
            .query_row("SELECT delta_since FROM currency_estimate WHERE character_name = 'Alice'", [], |r| r.get(0))
            .unwrap();
        let ledger_sum: i64 = conn
            .query_row("SELECT COALESCE(SUM(delta), 0) FROM council_events WHERE event_ts > '2026-04-01 05:00:00'", [], |r| r.get(0))
            .unwrap();
        assert_eq!((delta, ledger_sum), (15, 15), "accumulator reconciles with the ledger");
        drop(conn);
        // Same lines replayed with the same ids: ledger converges to one row
        // per line. The accumulator has no per-line memory, so it accrues
        // again (documented double-accrue limit — live owns accumulation).
        let mut st = state(&mut arena, &mut dmg);
        dispatch_with_line(&pool, &gd, Some(&id()), &councils, "26-04-01 05:30:53", &mut st, Some("chat.log:100"));
        let mut st = state(&mut arena, &mut dmg);
        dispatch_with_line(&pool, &gd, Some(&id()), &loot, "26-04-01 05:31:53", &mut st, Some("chat.log:200"));
        let conn = pool.get_write().unwrap();
        let n: i64 = conn.query_row("SELECT COUNT(*) FROM council_events", [], |r| r.get(0)).unwrap();
        assert_eq!(n, 2, "replay converges to one row per line");
        // A genuinely distinct second receipt (different offset, same second
        // and content) inserts its own row.
        drop(conn);
        let mut st = state(&mut arena, &mut dmg);
        dispatch_with_line(&pool, &gd, Some(&id()), &loot, "26-04-01 05:31:53", &mut st, Some("chat.log:300"));
        let conn = pool.get_write().unwrap();
        let n: i64 = conn.query_row("SELECT COUNT(*) FROM council_events", [], |r| r.get(0)).unwrap();
        assert_eq!(n, 3, "distinct line id inserts a genuine second receipt");
    }

    #[test]
    fn council_ledger_legacy_null_ids_collapse_and_scan_records_nothing() {
        // Legacy match-any path (`None` id): two same-second same-content
        // lines collapse onto one row, and scan paths (`count_currency =
        // false`) record neither accrual nor ledger.
        let (_guard, pool) = test_pool();
        let gd = game_data();
        seed_skill(&gd, "Sword", 7);
        let mut arena = ArenaTracker::new();
        let mut dmg = Vec::new();
        pool.get_write()
            .unwrap()
            .execute(
                "INSERT INTO currency_estimate (character_name, server_name, currency_name, anchor_amount, anchor_at, delta_since)
                 VALUES ('Alice','Dreva','GOLD',1000,'2026-04-01 05:00:00',0)",
                [],
            )
            .unwrap();
        let councils = status_msg("You received 10 Councils.", "26-04-01 05:30:53");
        let mut st = state(&mut arena, &mut dmg);
        dispatch(&pool, &gd, Some(&id()), &councils, "26-04-01 05:30:53", &mut st);
        let mut st = state(&mut arena, &mut dmg);
        dispatch(&pool, &gd, Some(&id()), &councils, "26-04-01 05:30:53", &mut st);
        let conn = pool.get_write().unwrap();
        let n: i64 = conn.query_row("SELECT COUNT(*) FROM council_events", [], |r| r.get(0)).unwrap();
        assert_eq!(n, 1, "legacy NULL ids collapse match-any");
        drop(conn);
        // Scan path records nothing (live owns accumulation, as today).
        let mut st = state_scan(&mut arena, &mut dmg);
        dispatch(&pool, &gd, Some(&id()), &councils, "26-04-01 05:30:53", &mut st);
        let conn = pool.get_write().unwrap();
        let n: i64 = conn.query_row("SELECT COUNT(*) FROM council_events", [], |r| r.get(0)).unwrap();
        assert_eq!(n, 1, "scan records no ledger row");
        let delta: i64 = conn
            .query_row("SELECT delta_since FROM currency_estimate WHERE character_name = 'Alice'", [], |r| r.get(0))
            .unwrap();
        // Two live accrues (10 + 10); the scan added nothing.
        assert_eq!(delta, 20, "scan accrues nothing");
    }

    #[test]
    fn per_file_identity_two_sessions() {
        let (_guard, pool) = test_pool();
        let gd = game_data();
        seed_skill(&gd, "Sword", 7);
        let mut arena = ArenaTracker::new();
        let mut dmg = Vec::new();
        
        let ts = "26-04-01 05:30:53";

        let alice = DispatchIdentity { character: "Alice".to_string(), server: "Dreva".to_string() };
        let bob = DispatchIdentity { character: "Bob".to_string(), server: "Legacy".to_string() };
        let m = status_msg("You earned 62 XP and reached level 5 in Sword!", ts);
        let mut st = state(&mut arena, &mut dmg);
        dispatch(&pool, &gd, Some(&alice), &m, ts, &mut st);
        let mut st = state(&mut arena, &mut dmg);
        dispatch(&pool, &gd, Some(&bob), &m, ts, &mut st);

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
        let (_guard, pool) = test_pool();
        let gd = game_data();
        let mut arena = ArenaTracker::new();
        let mut dmg = Vec::new();
        
        let ts = "26-04-01 05:30:53";
        let m = status_msg("Tiny Brain Bug Meat added to inventory.", ts);
        let mut st = state(&mut arena, &mut dmg);
        let out = dispatch(&pool, &gd, None, &m, ts, &mut st);
        assert!(out.staged_gains.is_empty());
        let conn = pool.get_write().unwrap();
        let n: i64 = conn.query_row("SELECT COUNT(*) FROM item_transactions", [], |r| r.get(0)).unwrap();
        assert_eq!(n, 0);
    }

    #[test]
    fn death_persists_once() {
        let (_guard, pool) = test_pool();
        let gd = game_data();
        let mut arena = ArenaTracker::new();
        let mut dmg = Vec::new();
        
        let ts = "26-04-01 05:30:53";

        let death = combat_msg(
            "Demon Scout #7059135: Demon Bolt on Alice! Dmg: 179 health, 178 armor. (FATALITY!)",
            ts,
        );
        let mut st = state(&mut arena, &mut dmg);
        dispatch(&pool, &gd, Some(&id()), &death, ts, &mut st);
        let mut st = state(&mut arena, &mut dmg);
        dispatch(&pool, &gd, Some(&id()), &death, ts, &mut st);
        let n: i64 = pool.get().unwrap().query_row("SELECT COUNT(*) FROM character_deaths", [], |r| r.get(0)).unwrap();
        assert_eq!(n, 1);
    }

    #[test]
    fn rez_same_phrasing_repeat_records() {
        // Live shape (Chat-26-06-11, Bumblebutt 8s apart): a pet rezzed,
        // alive mid-fight, dead again. Repeats are genuine re-rezzes and
        // must both persist — no cross-phrasing suppression exists any more
        // (pair never observed live; guard removed 2026-11-24).
        let (_guard, pool) = test_pool();
        let gd = game_data();
        let mut arena = ArenaTracker::new();
        let mut dmg = Vec::new();
        

        let r1 = emote_msg("Zabael resuscitates Bumblebutt", "26-06-11 22:19:33");
        let r2 = emote_msg("Zabael resuscitates Bumblebutt", "26-06-11 22:19:41");
        let mut st = state(&mut arena, &mut dmg);
        dispatch(&pool, &gd, Some(&id()), &r1, "26-06-11 22:19:33", &mut st);
        let mut st = state(&mut arena, &mut dmg);
        let out = dispatch(&pool, &gd, Some(&id()), &r2, "26-06-11 22:19:41", &mut st);
        assert!(out.character_resuscitated.is_some());
        let n: i64 = pool.get().unwrap().query_row("SELECT COUNT(*) FROM character_resuscitations", [], |r| r.get(0)).unwrap();
        assert_eq!(n, 2);
    }

    #[test]
    fn wisdom_records_once_with_local_key() {
        // Regression: CombatWisdomEarned used to hit `unreachable!` in the
        // shared arm (live catch-up with real wisdom lines aborted the app).
        let (_guard, pool) = test_pool();
        let gd = game_data();
        let mut arena = ArenaTracker::new();
        let mut dmg = Vec::new();
        
        let ts = "26-04-01 05:30:53";
        let m = status_msg("You earned 64 Combat Wisdom: Killed the Aktaari Queen", ts);
        let mut st = state(&mut arena, &mut dmg);
        let out = dispatch(&pool, &gd, Some(&id()), &m, ts, &mut st);
        assert!(out.chat_status_event.is_some());
        let mut st = state(&mut arena, &mut dmg);
        dispatch(&pool, &gd, Some(&id()), &m, ts, &mut st);
        let n: i64 = pool.get().unwrap().query_row("SELECT COUNT(*) FROM combat_wisdom_earns", [], |r| r.get(0)).unwrap();
        assert_eq!(n, 1);
    }

    #[test]
    fn rescan_stays_silent_for_chat_status_event() {
        // Rescan dedup gate: the frontend event fires only when the line's
        // persist advanced, so a re-scan never double-accrues session XP.
        let (_guard, pool) = test_pool();
        let gd = game_data();
        seed_skill(&gd, "Sword", 7);
        let mut arena = ArenaTracker::new();
        let mut dmg = Vec::new();
        let ts = "26-04-01 05:30:53";

        // Persist-backed arm: fresh live line emits, identical replay is silent.
        let m = status_msg("Tiny Brain Bug Meat added to inventory.", ts);
        let mut st = state(&mut arena, &mut dmg);
        let out = dispatch(&pool, &gd, Some(&id()), &m, ts, &mut st);
        assert!(out.chat_status_event.is_some());
        let mut st = state(&mut arena, &mut dmg);
        let out = dispatch(&pool, &gd, Some(&id()), &m, ts, &mut st);
        assert!(out.chat_status_event.is_none());

        // Pure-XP arm (no persist key): live emits, scan stays silent.
        let x = status_msg("You earned 25 XP in Sword.", ts);
        let mut st = state(&mut arena, &mut dmg);
        let out = dispatch(&pool, &gd, Some(&id()), &x, ts, &mut st);
        assert!(out.chat_status_event.is_some());
        let mut st = state_scan(&mut arena, &mut dmg);
        let out = dispatch_scan(&pool, &gd, Some(&id()), &x, ts, &mut st);
        assert!(out.chat_status_event.is_none());

        // Wisdom dedups on the local-ts key: replay of the same line is silent.
        let w = status_msg("You earned 64 Combat Wisdom: Killed the Aktaari Queen", ts);
        let mut st = state(&mut arena, &mut dmg);
        let out = dispatch(&pool, &gd, Some(&id()), &w, ts, &mut st);
        assert!(out.chat_status_event.is_some());
        let mut st = state(&mut arena, &mut dmg);
        let out = dispatch(&pool, &gd, Some(&id()), &w, ts, &mut st);
        assert!(out.chat_status_event.is_none());
    }

    #[test]
    fn report_saved_is_reported_not_imported() {
        let (_guard, pool) = test_pool();
        let gd = game_data();
        let mut arena = ArenaTracker::new();
        let mut dmg = Vec::new();
        
        let ts = "26-04-01 05:30:53";
        let m = status_msg("Saved report to C:/Games/Reports/Character_Alice.json", ts);
        let mut st = state(&mut arena, &mut dmg);
        let out = dispatch(&pool, &gd, Some(&id()), &m, ts, &mut st);
        assert_eq!(out.report_saved_paths.len(), 1);
    }

    fn json_msg(json_type: &str, body: &str, ts: &str) -> ChatMessage {
        let line = format!("{ts}\t[JSON:{json_type}] {body}");
        crate::chat_parser::parse_chat_line(&line).unwrap()
    }

    fn seed_quest(gd: &GameDataState, key: &str, name: &str) {
        gd.blocking_write().quests.insert(
            key.to_string(),
            crate::game_data::QuestInfo {
                internal_name: name.to_string(),
                raw: serde_json::json!({ "Name": name }),
            },
        );
    }

    /// One `quest_accepts` row as read back by tests.
    #[allow(clippy::type_complexity)]
    fn quest_rows(conn: &rusqlite::Connection) -> Vec<(String, String, String, i64, Option<String>, Option<String>)> {
        let mut stmt = conn
            .prepare(
                "SELECT character_name, server_name, accepted_at, quest_id, quest_name, source_line_id
                 FROM quest_accepts ORDER BY accepted_at, quest_id",
            )
            .unwrap();
        stmt.query_map([], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?))
        })
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap()
    }

    #[test]
    fn start_quest_accept_persists_with_identity_and_cdn_name() {
        let (_guard, pool) = test_pool();
        let gd = game_data();
        seed_quest(&gd, "quest_25521", "Aurest Mission: Labyrinth Chests");
        let mut arena = ArenaTracker::new();
        let mut dmg = Vec::new();
        let ts = "26-11-18 10:22:45";
        let m = json_msg("StartQuest", r#"{"QuestId":25521}"#, ts);
        let mut st = state(&mut arena, &mut dmg);
        let out = dispatch_with_line(&pool, &gd, Some(&id()), &m, ts, &mut st, Some("chat.log:42"));
        assert!(out.domains.contains(&"quests".to_string()));
        let conn = pool.get_write().unwrap();
        let rows = quest_rows(&conn);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].0, "Alice");
        assert_eq!(rows[0].1, "Dreva");
        assert_eq!(rows[0].2, "2026-11-18 10:22:45");
        assert_eq!(rows[0].3, 25521);
        assert_eq!(rows[0].4.as_deref(), Some("Aurest Mission: Labyrinth Chests"));
        assert_eq!(rows[0].5.as_deref(), Some("chat.log:42"));
    }

    #[test]
    fn start_quest_dailies_resolve_by_name_without_cdn() {
        // Empty GameData (offline/stale CDN): the Agrashab + Queen's dailies
        // resolve through the monitor fallback maps; Queen's 25541-44 are
        // absent from the checked-in CDN snapshot, so this is the live path.
        let (_guard, pool) = test_pool();
        let gd = game_data();
        let mut arena = ArenaTracker::new();
        let mut dmg = Vec::new();
        let ts = "26-11-18 10:22:45";
        for (qid, name) in [
            (25521u32, "Aurest Mission: Labyrinth Chests"),
            (25104u32, "Agrashab's Bounty: Phoenices"),
            (25542u32, "Queen's Request: Mope's Mantis Infestation"),
        ] {
            let m = json_msg("StartQuest", &format!("{{\"QuestId\":{qid}}}"), ts);
            let mut st = state(&mut arena, &mut dmg);
            let out = dispatch(&pool, &gd, Some(&id()), &m, ts, &mut st);
            assert!(out.domains.contains(&"quests".to_string()), "quest {qid}");
            let conn = pool.get_write().unwrap();
            let stored: Option<String> = conn
                .query_row(
                    "SELECT quest_name FROM quest_accepts WHERE quest_id = ?1",
                    rusqlite::params![qid as i64],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(stored.as_deref(), Some(name), "quest {qid}");
        }
    }

    #[test]
    fn start_quest_unknown_id_stores_id_only() {
        let (_guard, pool) = test_pool();
        let gd = game_data();
        let mut arena = ArenaTracker::new();
        let mut dmg = Vec::new();
        let ts = "26-11-18 10:22:45";
        let m = json_msg("StartQuest", r#"{"QuestId":999999}"#, ts);
        let mut st = state(&mut arena, &mut dmg);
        let out = dispatch(&pool, &gd, Some(&id()), &m, ts, &mut st);
        assert!(out.domains.contains(&"quests".to_string()));
        let conn = pool.get_write().unwrap();
        let rows = quest_rows(&conn);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].3, 999999);
        assert_eq!(rows[0].4, None);
    }

    #[test]
    fn start_quest_malformed_stays_malformed_never_unknown() {
        let (_guard, pool) = test_pool();
        let gd = game_data();
        let mut arena = ArenaTracker::new();
        let mut dmg = Vec::new();
        let ts = "26-11-18 10:22:45";
        let m = json_msg("StartQuest", r#"{"Nope":1}"#, ts);
        let mut st = state(&mut arena, &mut dmg);
        let out = dispatch(&pool, &gd, Some(&id()), &m, ts, &mut st);
        assert_eq!(out.malformed_json, vec!["StartQuest".to_string()]);
        assert!(out.unknown_json.is_empty());
        assert!(!out.domains.contains(&"quests".to_string()));
    }

    #[test]
    fn start_quest_replay_converges_on_line_identity() {
        // Dedupe key: (character, server, accepted_at, quest_id, line id) —
        // same favor/council ledger discipline: a stamped replay collapses,
        // an unstamped rescan collapses, a distinct line inserts.
        let (_guard, pool) = test_pool();
        let gd = game_data();
        let mut arena = ArenaTracker::new();
        let mut dmg = Vec::new();
        let ts = "26-11-18 10:22:45";
        let m = json_msg("StartQuest", r#"{"QuestId":25542}"#, ts);
        let mut st = state(&mut arena, &mut dmg);
        let out = dispatch_with_line(&pool, &gd, Some(&id()), &m, ts, &mut st, Some("chat.log:42"));
        assert!(out.domains.contains(&"quests".to_string()));
        // Same line re-scanned: silent no-op.
        let mut st = state(&mut arena, &mut dmg);
        let out = dispatch_with_line(&pool, &gd, Some(&id()), &m, ts, &mut st, Some("chat.log:42"));
        assert!(!out.domains.contains(&"quests".to_string()));
        // Legacy unstamped rescan collapses too.
        let mut st = state(&mut arena, &mut dmg);
        let out = dispatch(&pool, &gd, Some(&id()), &m, ts, &mut st);
        assert!(!out.domains.contains(&"quests".to_string()));
        let conn = pool.get_write().unwrap();
        assert_eq!(quest_rows(&conn).len(), 1);
        drop(conn);
        // Genuinely distinct same-second line: its own row.
        let mut st = state(&mut arena, &mut dmg);
        let out = dispatch_with_line(&pool, &gd, Some(&id()), &m, ts, &mut st, Some("chat.log:97"));
        assert!(out.domains.contains(&"quests".to_string()));
        let conn = pool.get_write().unwrap();
        assert_eq!(quest_rows(&conn).len(), 2);
    }

    #[test]
    fn note_unknown_skill_collapses_adjacent_twins_only() {
        let mut last: Option<String> = None;
        // English `LevelUp` + JSON `Xp` twin: one event, one skip.
        assert!(note_unknown_skill(&mut last, "Fungiculture".to_string()));
        assert!(!note_unknown_skill(&mut last, "Fungiculture".to_string()));
        // A different skill right after is a separate event.
        assert!(note_unknown_skill(&mut last, "Mycology".to_string()));
        // A non-adjacent repeat (later poll) counts again.
        assert!(note_unknown_skill(&mut last, "Fungiculture".to_string()));
    }

    #[test]
    fn unknown_skill_twin_english_first_counts_once() {
        // Cross-batch twin, English first: the coordinator owns the slot
        // across polls, so the JSON twin collapsing needs no batch locality.
        let (_guard, pool) = test_pool();
        let gd = game_data();
        let mut arena = ArenaTracker::new();
        let mut dmg = Vec::new();
        let ts = "26-04-01 05:30:53";
        // `Fungiculture` stays unseeded: both sightings land in
        // `unknown_skills` like a new game skill on stale CDN data.
        let english = status_msg("You earned 62 XP and reached level 5 in Fungiculture!", ts);
        let json = json_msg(
            "EarnedXP",
            r#"{"Skill":"Fungiculture","XP":62,"OldLevel":4,"NewLevel":5}"#,
            ts,
        );
        let mut last: Option<String> = None;
        let mut count = 0;
        let mut st = state(&mut arena, &mut dmg);
        let out = dispatch(&pool, &gd, Some(&id()), &english, ts, &mut st);
        assert_eq!(out.unknown_skills.len(), 1);
        for skill in out.unknown_skills {
            if note_unknown_skill(&mut last, skill) {
                count += 1;
            }
        }
        let mut st = state(&mut arena, &mut dmg);
        let out = dispatch(&pool, &gd, Some(&id()), &json, ts, &mut st);
        assert_eq!(out.unknown_skills.len(), 1);
        for skill in out.unknown_skills {
            if note_unknown_skill(&mut last, skill) {
                count += 1;
            }
        }
        assert_eq!(count, 1);
    }

    #[test]
    fn unknown_skill_twin_json_first_counts_once() {
        // Mirror twin, JSON first: collapse is order-independent.
        let (_guard, pool) = test_pool();
        let gd = game_data();
        let mut arena = ArenaTracker::new();
        let mut dmg = Vec::new();
        let ts = "26-04-01 05:30:53";
        let english = status_msg("You earned 62 XP and reached level 5 in Fungiculture!", ts);
        let json = json_msg(
            "EarnedXP",
            r#"{"Skill":"Fungiculture","XP":62,"OldLevel":4,"NewLevel":5}"#,
            ts,
        );
        let mut last: Option<String> = None;
        let mut count = 0;
        let mut st = state(&mut arena, &mut dmg);
        let out = dispatch(&pool, &gd, Some(&id()), &json, ts, &mut st);
        assert_eq!(out.unknown_skills.len(), 1);
        for skill in out.unknown_skills {
            if note_unknown_skill(&mut last, skill) {
                count += 1;
            }
        }
        let mut st = state(&mut arena, &mut dmg);
        let out = dispatch(&pool, &gd, Some(&id()), &english, ts, &mut st);
        assert_eq!(out.unknown_skills.len(), 1);
        for skill in out.unknown_skills {
            if note_unknown_skill(&mut last, skill) {
                count += 1;
            }
        }
        assert_eq!(count, 1);
    }

    #[test]
    fn unknown_skill_second_twin_pair_counts_again() {
        // Pair consumption: `E,J,E,J` is two events, two skips — a blind
        // never-recount collapse would undercount the second event.
        let (_guard, pool) = test_pool();
        let gd = game_data();
        let mut arena = ArenaTracker::new();
        let mut dmg = Vec::new();
        let ts = "26-04-01 05:30:53";
        let ts2 = "26-04-01 06:30:53";
        let english = status_msg("You earned 62 XP and reached level 5 in Fungiculture!", ts);
        let json = json_msg(
            "EarnedXP",
            r#"{"Skill":"Fungiculture","XP":62,"OldLevel":4,"NewLevel":5}"#,
            ts,
        );
        let english2 = status_msg("You earned 70 XP and reached level 6 in Fungiculture!", ts2);
        let json2 = json_msg(
            "EarnedXP",
            r#"{"Skill":"Fungiculture","XP":70,"OldLevel":5,"NewLevel":6}"#,
            ts2,
        );
        let mut last: Option<String> = None;
        let mut count = 0;
        for (m, t) in [(&english, ts), (&json, ts), (&english2, ts2), (&json2, ts2)] {
            let mut st = state(&mut arena, &mut dmg);
            let out = dispatch(&pool, &gd, Some(&id()), m, t, &mut st);
            assert_eq!(out.unknown_skills.len(), 1);
            for skill in out.unknown_skills {
                if note_unknown_skill(&mut last, skill) {
                    count += 1;
                }
            }
        }
        assert_eq!(count, 2);
    }

    fn id_bob() -> DispatchIdentity {
        DispatchIdentity {
            character: "Bob".to_string(),
            server: "Dreva".to_string(),
        }
    }

    #[test]
    fn unknown_skill_slot_twins_both_orders_same_identity() {
        // Identity-scoped slot through real dispatch, both arrival orders:
        // English-first and JSON-first each collapse to one skip with one
        // observability-only suppression (no recount, no ledger rows — the
        // function returns bool only and writes nothing).
        let (_guard, pool) = test_pool();
        let gd = game_data();
        let mut arena = ArenaTracker::new();
        let mut dmg = Vec::new();
        let ts = "26-04-01 05:30:53";
        let alice = id();
        let english = status_msg("You earned 62 XP and reached level 5 in Fungiculture!", ts);
        let json = json_msg(
            "EarnedXP",
            r#"{"Skill":"Fungiculture","XP":62,"OldLevel":4,"NewLevel":5}"#,
            ts,
        );
        for (first, second) in [(&english, &json), (&json, &english)] {
            let mut last: Option<UnknownSkillSlot> = None;
            let mut skipped = 0;
            let mut suppressed = 0;
            for m in [first, second] {
                let mut st = state(&mut arena, &mut dmg);
                let out = dispatch(&pool, &gd, Some(&alice), m, ts, &mut st);
                assert_eq!(out.unknown_skills.len(), 1);
                for skill in out.unknown_skills {
                    if note_unknown_skill_for(&mut last, Some(&alice), skill) {
                        skipped += 1;
                    } else {
                        suppressed += 1;
                    }
                }
            }
            assert_eq!(skipped, 1);
            assert_eq!(suppressed, 1);
        }
    }

    #[test]
    fn unknown_skill_slot_cross_identity_counts_twice() {
        // Cross-character same-skill collision: Alice's half must not swallow
        // Bob's same-skill sighting. Bob's own twin still collapses after.
        let (_guard, pool) = test_pool();
        let gd = game_data();
        let mut arena = ArenaTracker::new();
        let mut dmg = Vec::new();
        let ts = "26-04-01 05:30:53";
        let alice = id();
        let bob = id_bob();
        let english = status_msg("You earned 62 XP and reached level 5 in Fungiculture!", ts);
        let json = json_msg(
            "EarnedXP",
            r#"{"Skill":"Fungiculture","XP":62,"OldLevel":4,"NewLevel":5}"#,
            ts,
        );
        let mut last: Option<UnknownSkillSlot> = None;
        let mut skipped = 0;
        let mut suppressed = 0;
        // Alice English arms the slot.
        let mut st = state(&mut arena, &mut dmg);
        let out = dispatch(&pool, &gd, Some(&alice), &english, ts, &mut st);
        assert_eq!(out.unknown_skills.len(), 1);
        for skill in out.unknown_skills {
            if note_unknown_skill_for(&mut last, Some(&alice), skill) {
                skipped += 1;
            } else {
                suppressed += 1;
            }
        }
        // Bob same-skill English is a new skip, not a twin collapse.
        let mut st = state(&mut arena, &mut dmg);
        let out = dispatch(&pool, &gd, Some(&bob), &english, ts, &mut st);
        assert_eq!(out.unknown_skills.len(), 1);
        for skill in out.unknown_skills {
            if note_unknown_skill_for(&mut last, Some(&bob), skill) {
                skipped += 1;
            } else {
                suppressed += 1;
            }
        }
        // Bob's JSON twin collapses into Bob's arming.
        let mut st = state(&mut arena, &mut dmg);
        let out = dispatch(&pool, &gd, Some(&bob), &json, ts, &mut st);
        assert_eq!(out.unknown_skills.len(), 1);
        for skill in out.unknown_skills {
            if note_unknown_skill_for(&mut last, Some(&bob), skill) {
                skipped += 1;
            } else {
                suppressed += 1;
            }
        }
        assert_eq!(skipped, 2);
        assert_eq!(suppressed, 1);
    }

    #[test]
    fn unknown_skill_slot_cross_server_counts_twice() {
        // Same character on two servers is two identities: no collapse.
        let mut last: Option<UnknownSkillSlot> = None;
        let dreva = id();
        let legacy = DispatchIdentity {
            character: "Alice".to_string(),
            server: "Legacy".to_string(),
        };
        assert!(note_unknown_skill_for(
            &mut last,
            Some(&dreva),
            "Fungiculture".to_string()
        ));
        assert!(note_unknown_skill_for(
            &mut last,
            Some(&legacy),
            "Fungiculture".to_string()
        ));
    }

    #[test]
    fn unknown_skill_slot_genuine_repeat_suppresses_without_recount() {
        // Same identity, two consecutive genuine sightings (no twin between):
        // adjacency still collapses — the suppression is observability-only
        // (suppressed bumps, skipped does not, no ledger rows written).
        let (_guard, pool) = test_pool();
        let gd = game_data();
        let mut arena = ArenaTracker::new();
        let mut dmg = Vec::new();
        let ts = "26-04-01 05:30:53";
        let alice = id();
        let english = status_msg("You earned 62 XP and reached level 5 in Fungiculture!", ts);
        let mut last: Option<UnknownSkillSlot> = None;
        let mut skipped = 0;
        let mut suppressed = 0;
        for _ in 0..2 {
            let mut st = state(&mut arena, &mut dmg);
            let out = dispatch(&pool, &gd, Some(&alice), &english, ts, &mut st);
            assert_eq!(out.unknown_skills.len(), 1);
            for skill in out.unknown_skills {
                if note_unknown_skill_for(&mut last, Some(&alice), skill) {
                    skipped += 1;
                } else {
                    suppressed += 1;
                }
            }
        }
        assert_eq!(skipped, 1);
        assert_eq!(suppressed, 1);
    }

    fn seed_npc(gd: &GameDataState, key: &str, name: &str, area: Option<&str>, tastes: bool) {
        let mut g = gd.blocking_write();
        g.npcs.insert(
            key.to_string(),
            crate::game_data::NpcInfo {
                key: key.to_string(),
                name: name.to_string(),
                desc: None,
                area_name: area.map(str::to_string),
                area_friendly_name: None,
                trains_skills: vec![],
                preferences: vec![],
                gift_favor_tiers: if tastes { vec!["T1".to_string()] } else { vec![] },
                pos: None,
                services: None,
                raw_json: serde_json::Value::Null,
            },
        );
    }

    fn item_rows(pool: &DbPool) -> Vec<(String, i64, Option<String>, Option<i64>)> {
        let conn = pool.get().unwrap();
        let mut stmt = conn
            .prepare(
                "SELECT item_name, quantity, from_name, from_entity_id
                 FROM item_transactions ORDER BY id",
            )
            .unwrap();
        stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
            .unwrap()
            .map(|r| r.unwrap())
            .collect()
    }

    const LOOT_BODY: &str = "{\"Name\": \"Toxic Sludge\", \"TypeId\": 1137, \"Stack\": 1, \"Flags\": [], \"FromName\": \"Olugax The Ever-Pudding\", \"FromEntityId\": 257400}";

    #[test]
    fn json_additem_twins_converge_english_first() {
        let (_guard, pool) = test_pool();
        let gd = game_data();
        let mut arena = ArenaTracker::new();
        let mut dmg = Vec::new();
        
        let ts = "26-11-17 08:50:27";
        let english = status_msg("Toxic Sludge added to inventory.", ts);
        let json = json_msg("AddItem", LOOT_BODY, ts);
        let mut st = state(&mut arena, &mut dmg);
        let out1 = dispatch(&pool, &gd, Some(&id()), &english, ts, &mut st);
        let mut st = state(&mut arena, &mut dmg);
        let out2 = dispatch(&pool, &gd, Some(&id()), &json, ts, &mut st);
        // Exactly one staging across the pair: no double attribution.
        assert_eq!(out1.staged_gains.len(), 1);
        assert!(out2.staged_gains.is_empty());
        let rows = item_rows(&pool);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].0, "Toxic Sludge");
        assert_eq!(rows[0].1, 1);
        // The JSON twin enriched the English row with the entity.
        assert_eq!(rows[0].2.as_deref(), Some("Olugax The Ever-Pudding"));
        assert_eq!(rows[0].3, Some(257400));
    }

    #[test]
    fn json_receipt_enrichment_stamps_single_row() {
        // Same-second identical receipts from different entities share the
        // whole enrichment key. Live English lines dedupe, so two NULL rows
        // can only arise from out-of-band writes — but the UPDATE must still
        // stamp exactly one (earliest) and leave the other for its own twin.
        let (_guard, pool) = test_pool();
        {
            // Scope the seed writes: the pool has a single writer, and the
            // function under test checks out its own.
            let conn = pool.get_write().unwrap();
            for _ in 0..2 {
                conn.execute(
                    "INSERT INTO item_transactions
                        (timestamp, character_name, server_name, item_name, quantity,
                         context, source, from_name, from_entity_id)
                     VALUES ('26-11-17 08:50:27', 'Alice', 'Dreva', 'Toxic Sludge', 1,
                         'loot', 'chat_status', NULL, NULL)",
                    [],
                )
                .unwrap();
            }
        }
        insert_json_receipt_guarded(
            &pool,
            "26-11-17 08:50:27",
            "Alice",
            "Dreva",
            "Toxic Sludge",
            None,
            None,
            1,
            Some("Olugax The Ever-Pudding"),
            Some(257400),
            None,
            false,
        );
        let conn = pool.get().unwrap();
        let stamped: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM item_transactions WHERE from_entity_id = 257400",
                [],
                |r| r.get(0),
            )
            .unwrap();
        let unstamped: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM item_transactions WHERE from_name IS NULL",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!((stamped, unstamped), (1, 1));
    }
    #[test]
    fn json_additem_enriches_twin_on_entity_id_without_name() {
        // T43: a receipt carrying FromEntityId but no FromName must still
        // enrich the English twin — not write a second row.
        let (_guard, pool) = test_pool();
        let gd = game_data();
        let mut arena = ArenaTracker::new();
        let mut dmg = Vec::new();
        let ts = "26-11-17 08:50:27";
        let english = status_msg("Toxic Sludge added to inventory.", ts);
        let entity_only = "{\"Name\": \"Toxic Sludge\", \"TypeId\": 1137, \"Stack\": 1, \"Flags\": [], \"FromEntityId\": 257400}";
        let json = json_msg("AddItem", entity_only, ts);
        let mut st = state(&mut arena, &mut dmg);
        dispatch(&pool, &gd, Some(&id()), &english, ts, &mut st);
        let mut st = state(&mut arena, &mut dmg);
        let out = dispatch(&pool, &gd, Some(&id()), &json, ts, &mut st);
        assert!(out.staged_gains.is_empty());
        let rows = item_rows(&pool);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].3, Some(257400));
    }

    #[test]
    fn json_sourced_replay_does_not_claim_unsourced_twin() {
        // T44: sourced + unsourced rows coexist; replaying the sourced
        // receipt must hit the exact-entity guard first and leave the
        // unsourced row unstamped (no third row on the next replay).
        let (_guard, pool) = test_pool();
        let gd = game_data();
        let mut arena = ArenaTracker::new();
        let mut dmg = Vec::new();
        let ts = "26-11-17 08:50:27";
        let sourced = json_msg("AddItem", LOOT_BODY, ts);
        let unsourced = json_msg(
            "AddItem",
            "{\"Name\": \"Toxic Sludge\", \"TypeId\": 1137, \"Stack\": 1, \"Flags\": []}",
            ts,
        );
        let mut st = state(&mut arena, &mut dmg);
        dispatch(&pool, &gd, Some(&id()), &sourced, ts, &mut st);
        let mut st = state(&mut arena, &mut dmg);
        dispatch(&pool, &gd, Some(&id()), &unsourced, ts, &mut st);
        assert_eq!(item_rows(&pool).len(), 2);
        // Replay the sourced receipt: exact-entity hit, no stamping.
        let mut st = state(&mut arena, &mut dmg);
        dispatch(&pool, &gd, Some(&id()), &sourced, ts, &mut st);
        let rows = item_rows(&pool);
        assert_eq!(rows.len(), 2);
        assert!(rows.iter().any(|r| r.3.is_none()));
    }
    #[test]
    fn json_unsourced_first_sourced_does_not_claim_json_row() {
        // Reverse of T44: an unsourced JSON receipt is NOT an English twin,
        // so the sourced receipt must insert its own row instead of stamping
        // the JSON row — stamping would destroy the NULL-entity marker the
        // unsourced replay needs, inserting a duplicate third row.
        let (_guard, pool) = test_pool();
        let gd = game_data();
        let mut arena = ArenaTracker::new();
        let mut dmg = Vec::new();
        let ts = "26-11-17 08:50:27";
        let sourced = json_msg("AddItem", LOOT_BODY, ts);
        let unsourced = json_msg(
            "AddItem",
            "{\"Name\": \"Toxic Sludge\", \"TypeId\": 1137, \"Stack\": 1, \"Flags\": []}",
            ts,
        );
        let mut st = state(&mut arena, &mut dmg);
        dispatch(&pool, &gd, Some(&id()), &unsourced, ts, &mut st);
        let mut st = state(&mut arena, &mut dmg);
        dispatch(&pool, &gd, Some(&id()), &sourced, ts, &mut st);
        assert_eq!(item_rows(&pool).len(), 2);
        // Replay the unsourced receipt: exact-NULL hit on its own row.
        let mut st = state(&mut arena, &mut dmg);
        dispatch(&pool, &gd, Some(&id()), &unsourced, ts, &mut st);
        let rows = item_rows(&pool);
        assert_eq!(rows.len(), 2);
        // Replay the sourced receipt: exact-entity hit, no stamping.
        let mut st = state(&mut arena, &mut dmg);
        dispatch(&pool, &gd, Some(&id()), &sourced, ts, &mut st);
        let rows = item_rows(&pool);
        assert_eq!(rows.len(), 2);
        assert!(rows.iter().any(|r| r.3.is_none()));
    }
    #[test]
    fn json_additem_twins_converge_json_first() {
        let (_guard, pool) = test_pool();
        let gd = game_data();
        let mut arena = ArenaTracker::new();
        let mut dmg = Vec::new();
        
        let ts = "26-11-17 08:50:27";
        let english = status_msg("Toxic Sludge added to inventory.", ts);
        let json = json_msg("AddItem", LOOT_BODY, ts);
        let mut st = state(&mut arena, &mut dmg);
        let out1 = dispatch(&pool, &gd, Some(&id()), &json, ts, &mut st);
        let mut st = state(&mut arena, &mut dmg);
        let out2 = dispatch(&pool, &gd, Some(&id()), &english, ts, &mut st);
        assert_eq!(out1.staged_gains.len(), 1);
        // English-second finds the JSON twin's row and stages nothing:
        // conditional staging holds in both twin orders.
        assert!(out2.staged_gains.is_empty());
        let rows = item_rows(&pool);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].2.as_deref(), Some("Olugax The Ever-Pudding"));
    }

    const LOOT_BODY_B: &str = "{\"Name\": \"Toxic Sludge\", \"TypeId\": 1137, \"Stack\": 1, \"Flags\": [], \"FromName\": \"Slime B\", \"FromEntityId\": 257401}";

    #[test]
    fn twins_two_entities_converge_english_first() {
        // Two slimes, one Toxic Sludge each, same second, English-first
        // interleave. Both gains must survive with their entities, and each
        // gain stages exactly once across its twin pair.
        let (_guard, pool) = test_pool();
        let gd = game_data();
        let mut arena = ArenaTracker::new();
        let mut dmg = Vec::new();
        let ts = "26-11-17 08:50:27";
        let english = status_msg("Toxic Sludge added to inventory.", ts);
        let json_a = json_msg("AddItem", LOOT_BODY, ts);
        let json_b = json_msg("AddItem", LOOT_BODY_B, ts);
        let mut staged = 0;
        for m in [&english, &json_a, &english, &json_b] {
            let mut st = state(&mut arena, &mut dmg);
            staged += dispatch(&pool, &gd, Some(&id()), m, ts, &mut st).staged_gains.len();
        }
        assert_eq!(staged, 2, "each entity's gain stages exactly once");
        let rows = item_rows(&pool);
        assert_eq!(rows.len(), 2, "second entity's gain survives its English twin's skip");
        let mut entities: Vec<Option<i64>> = rows.iter().map(|r| r.3).collect();
        entities.sort_unstable();
        assert_eq!(entities, vec![Some(257400), Some(257401)]);
    }

    #[test]
    fn twins_two_entities_converge_json_first() {
        // Mirror order: both receipts land before either English twin. The
        // English twins must not insert a third row, and staging stays at
        // one per entity.
        let (_guard, pool) = test_pool();
        let gd = game_data();
        let mut arena = ArenaTracker::new();
        let mut dmg = Vec::new();
        let ts = "26-11-17 08:50:27";
        let english = status_msg("Toxic Sludge added to inventory.", ts);
        let json_a = json_msg("AddItem", LOOT_BODY, ts);
        let json_b = json_msg("AddItem", LOOT_BODY_B, ts);
        let mut staged = 0;
        for m in [&json_a, &json_b, &english, &english] {
            let mut st = state(&mut arena, &mut dmg);
            staged += dispatch(&pool, &gd, Some(&id()), m, ts, &mut st).staged_gains.len();
        }
        assert_eq!(staged, 2, "each entity's gain stages exactly once");
        let rows = item_rows(&pool);
        assert_eq!(rows.len(), 2, "English twins add no rows past the receipts");
        let mut entities: Vec<Option<i64>> = rows.iter().map(|r| r.3).collect();
        entities.sort_unstable();
        assert_eq!(entities, vec![Some(257400), Some(257401)]);
    }

    fn english_ctx(entity: Option<i64>, source_line_id: Option<&'static str>) -> ItemLedgerCtx<'static> {
        ItemLedgerCtx {
            timestamp: "26-11-17 08:50:27",
            character: "Alice",
            server: "Dreva",
            item_name: "Toxic Sludge",
            internal_name: None,
            item_type_id: None,
            quantity: 1,
            context: "loot",
            survey_use_id: None,
            from_entity_id: entity,
            source_line_id,
            is_scan: false,
        }
    }

    #[test]
    fn english_guard_collapses_identical_second_gain() {
        // Same line id twice is a true replay: an identical resend
        // collapses, exactly as before the line-identity key.
        let (_guard, pool) = test_pool();
        assert!(insert_item_transaction_guarded(&pool, english_ctx(None, Some("Chat.log:11"))));
        assert!(
            !insert_item_transaction_guarded(&pool, english_ctx(None, Some("Chat.log:11"))),
            "identical English resend collapses"
        );
        assert_eq!(item_rows(&pool).len(), 1);
    }

    #[test]
    fn english_guard_records_second_entity_when_known() {
        // Entity-aware key: two same-second same-item+qty gains from
        // different entities (different lines) both record; a same-line
        // resend of one still collapses. Production English passes `None`
        // (no entity on the line), so this exercises the key for
        // entity-knowing callers.
        let (_guard, pool) = test_pool();
        assert!(insert_item_transaction_guarded(&pool, english_ctx(Some(1001), Some("Chat.log:11"))));
        assert!(
            insert_item_transaction_guarded(&pool, english_ctx(Some(1002), Some("Chat.log:12"))),
            "second entity's gain is a distinct gain"
        );
        assert!(
            !insert_item_transaction_guarded(&pool, english_ctx(Some(1001), Some("Chat.log:11"))),
            "exact-entity resend collapses"
        );
        let rows = item_rows(&pool);
        assert_eq!(rows.len(), 2);
        let mut entities: Vec<Option<i64>> = rows.iter().map(|r| r.3).collect();
        entities.sort_unstable();
        assert_eq!(entities, vec![Some(1001), Some(1002)]);
    }

    #[test]
    fn english_twin_pairs_converge_interleaved_both_orders() {
        // Interleaved twin pairs (E1,J1,E2,J2 and J1,E1,J2,E2), one line id
        // per line: exactly 2 rows, both stamped with the entity, exactly 2
        // staged gains. Preservation lock: the consume marker must keep the
        // twin-second convergence the line-blind guard provided when every
        // twin arrives — passes pre- and post-fix.
        for json_first in [false, true] {
            let (_guard, pool) = test_pool();
            let gd = game_data();
            let mut arena = ArenaTracker::new();
            let mut dmg = Vec::new();
            let ts = "26-11-17 08:50:27";
            let english = status_msg("Toxic Sludge added to inventory.", ts);
            let json = json_msg("AddItem", LOOT_BODY, ts);
            let seq: [(bool, &str); 4] = if json_first {
                [
                    (true, "Chat.log:21"),
                    (false, "Chat.log:11"),
                    (true, "Chat.log:22"),
                    (false, "Chat.log:12"),
                ]
            } else {
                [
                    (false, "Chat.log:11"),
                    (true, "Chat.log:21"),
                    (false, "Chat.log:12"),
                    (true, "Chat.log:22"),
                ]
            };
            let mut staged = 0;
            for (is_json, line) in seq {
                let m = if is_json { &json } else { &english };
                let mut st = state(&mut arena, &mut dmg);
                staged += dispatch_with_line(&pool, &gd, Some(&id()), m, ts, &mut st, Some(line))
                    .staged_gains
                    .len();
            }
            assert_eq!(staged, 2, "each event stages exactly once (json_first={json_first})");
            let rows = item_rows(&pool);
            assert_eq!(rows.len(), 2, "two events keep two rows (json_first={json_first})");
            assert!(
                rows.iter().all(|r| r.3 == Some(257400)),
                "both rows stamped (json_first={json_first})"
            );
        }
    }

    #[test]
    fn english_genuine_second_survives_malformed_twin() {
        // Mutant-killing (fails pre-fix): E1 + J1 complete the first pair;
        // E2's twin arrives malformed (`skipped_json_malformed`: counted, no
        // ledger row). E2 must still insert and stage its own gain — the
        // line-blind guard collapsed it onto the stamped row and the gain
        // was silently lost.
        let (_guard, pool) = test_pool();
        let gd = game_data();
        let mut arena = ArenaTracker::new();
        let mut dmg = Vec::new();
        let ts = "26-11-17 08:50:27";
        let english = status_msg("Toxic Sludge added to inventory.", ts);
        let json = json_msg("AddItem", LOOT_BODY, ts);
        let mut st = state(&mut arena, &mut dmg);
        let out1 = dispatch_with_line(&pool, &gd, Some(&id()), &english, ts, &mut st, Some("Chat.log:11"));
        assert_eq!(out1.staged_gains.len(), 1);
        let mut st = state(&mut arena, &mut dmg);
        let out2 = dispatch_with_line(&pool, &gd, Some(&id()), &json, ts, &mut st, Some("Chat.log:21"));
        assert!(out2.staged_gains.is_empty());
        assert_eq!(item_rows(&pool).len(), 1);
        // E2's twin never lands: malformed JSON is counted, never stored.
        let bad = json_msg("AddItem", "not-json", ts);
        let mut st = state(&mut arena, &mut dmg);
        let bad_out = dispatch_with_line(&pool, &gd, Some(&id()), &bad, ts, &mut st, Some("Chat.log:22"));
        assert_eq!(bad_out.malformed_json, vec!["AddItem".to_string()]);
        assert_eq!(item_rows(&pool).len(), 1);
        // The genuine second receipt inserts and stages despite the stamp.
        let mut st = state(&mut arena, &mut dmg);
        let out3 = dispatch_with_line(&pool, &gd, Some(&id()), &english, ts, &mut st, Some("Chat.log:12"));
        assert_eq!(out3.staged_gains.len(), 1, "malformed-twin second gain must survive");
        assert_eq!(item_rows(&pool).len(), 2);
    }

    #[test]
    fn english_inserts_after_completed_pair_json_first() {
        // Mutant-killing (fails pre-fix): JSON-first completed pair (J1
        // inserts the `json_twin` row, E1 consumes it via
        // `english_claimed_by`) then a genuine second English gain — E2 must
        // insert its own row, never consume E1's completed row. Replaces
        // `english_guard_null_skips_stamped_twin`, whose traceless raw-UPDATE
        // stamp no real writer produces post-#352 (stamps always carry
        // `claimed_by` + line identity).
        let (_guard, pool) = test_pool();
        let gd = game_data();
        let mut arena = ArenaTracker::new();
        let mut dmg = Vec::new();
        let ts = "26-11-17 08:50:27";
        let english = status_msg("Toxic Sludge added to inventory.", ts);
        let json = json_msg("AddItem", LOOT_BODY, ts);
        let mut st = state(&mut arena, &mut dmg);
        let out1 = dispatch_with_line(&pool, &gd, Some(&id()), &json, ts, &mut st, Some("Chat.log:21"));
        assert_eq!(out1.staged_gains.len(), 1);
        let mut st = state(&mut arena, &mut dmg);
        let out2 = dispatch_with_line(&pool, &gd, Some(&id()), &english, ts, &mut st, Some("Chat.log:11"));
        assert!(out2.staged_gains.is_empty(), "twin-second consumes, stages nothing");
        assert_eq!(item_rows(&pool).len(), 1);
        let claimed: i64 = pool
            .get()
            .unwrap()
            .query_row(
                "SELECT COUNT(*) FROM item_transactions
                 WHERE json_extract(source_details, '$.english_claimed_by') = 'Chat.log:11'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(claimed, 1, "consume leaves a per-line trace");
        let mut st = state(&mut arena, &mut dmg);
        let out3 = dispatch_with_line(&pool, &gd, Some(&id()), &english, ts, &mut st, Some("Chat.log:12"));
        assert_eq!(out3.staged_gains.len(), 1, "post-completed-pair English inserts");
        assert_eq!(item_rows(&pool).len(), 2);
    }
    #[test]
    fn english_consume_respects_entity_when_known() {
        // Mutant-killing for the entity predicate: an entity-aware English
        // line must not consume another entity's pending `json_twin` row —
        // that would complete the wrong pair and leave its own twin pending.
        // Production English always passes `None` (match-any, unchanged).
        let (_guard, pool) = test_pool();
        pool.get_write()
            .unwrap()
            .execute(
                "INSERT INTO item_transactions
                    (timestamp, character_name, server_name, item_name, quantity,
                     context, source, from_name, from_entity_id, source_details, source_line_id)
                 VALUES ('26-11-17 08:50:27', 'Alice', 'Dreva', 'Toxic Sludge', 1,
                     'loot', 'chat_status', 'Olugax', 1002, '{\"json_twin\":true}', 'Chat.log:29')",
                [],
            )
            .unwrap();
        assert!(
            insert_item_transaction_guarded(&pool, english_ctx(Some(1001), Some("Chat.log:31"))),
            "different-entity English inserts instead of consuming"
        );
        assert_eq!(item_rows(&pool).len(), 2);
        let marked: i64 = pool
            .get()
            .unwrap()
            .query_row(
                "SELECT COUNT(*) FROM item_transactions
                 WHERE json_extract(source_details, '$.english_claimed_by') IS NOT NULL",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(marked, 0, "no row consumed by the wrong-entity line");
    }

    #[test]
    fn english_consume_replay_is_stable() {
        // Preservation lock (stable pre- and post-fix): replay the full
        // consume interleave with original line ids — nothing restages,
        // nothing duplicates. Each replay hits its own trace (insert line,
        // JSON `claimed_by`, or English `english_claimed_by`).
        let (_guard, pool) = test_pool();
        let gd = game_data();
        let mut arena = ArenaTracker::new();
        let mut dmg = Vec::new();
        let ts = "26-11-17 08:50:27";
        let english = status_msg("Toxic Sludge added to inventory.", ts);
        let json = json_msg("AddItem", LOOT_BODY, ts);
        let seq: [(&ChatMessage, &str); 4] = [
            (&json, "Chat.log:21"),
            (&english, "Chat.log:11"),
            (&json, "Chat.log:22"),
            (&english, "Chat.log:12"),
        ];
        let mut staged_first = 0;
        for (m, line) in seq {
            let mut st = state(&mut arena, &mut dmg);
            staged_first += dispatch_with_line(&pool, &gd, Some(&id()), m, ts, &mut st, Some(line))
                .staged_gains
                .len();
        }
        assert_eq!(staged_first, 2);
        assert_eq!(item_rows(&pool).len(), 2);
        let mut staged_second = 0;
        for (m, line) in seq {
            let mut st = state(&mut arena, &mut dmg);
            staged_second += dispatch_with_line(&pool, &gd, Some(&id()), m, ts, &mut st, Some(line))
                .staged_gains
                .len();
        }
        assert_eq!(staged_second, 0, "replays must not restage");
        assert_eq!(item_rows(&pool).len(), 2, "replays must not duplicate");
    }

    #[test]
    fn english_genuine_duplicate_keeps_two_rows() {
        // Same key, two English lines, different line ids: the genuine
        // second stack inserts its own row instead of collapsing.
        let (_guard, pool) = test_pool();
        let gd = game_data();
        let mut arena = ArenaTracker::new();
        let mut dmg = Vec::new();
        let ts = "26-11-17 08:50:27";
        let english = status_msg("Toxic Sludge added to inventory.", ts);
        let mut st = state(&mut arena, &mut dmg);
        let out1 = dispatch_with_line(&pool, &gd, Some(&id()), &english, ts, &mut st, Some("Chat.log:11"));
        let mut st = state(&mut arena, &mut dmg);
        let out2 = dispatch_with_line(&pool, &gd, Some(&id()), &english, ts, &mut st, Some("Chat.log:12"));
        assert_eq!(out1.staged_gains.len(), 1);
        assert_eq!(out2.staged_gains.len(), 1, "genuine second stack stages its own gain");
        assert_eq!(item_rows(&pool).len(), 2);
    }

    #[test]
    fn json_genuine_duplicate_same_entity_keeps_two_rows() {
        // Same key+entity, two JSON receipts, different line ids, no
        // English rows: each inserts its own row instead of collapsing.
        let (_guard, pool) = test_pool();
        let gd = game_data();
        let mut arena = ArenaTracker::new();
        let mut dmg = Vec::new();
        let ts = "26-11-17 08:50:27";
        let json = json_msg("AddItem", LOOT_BODY, ts);
        let mut st = state(&mut arena, &mut dmg);
        let out1 = dispatch_with_line(&pool, &gd, Some(&id()), &json, ts, &mut st, Some("Chat.log:21"));
        let mut st = state(&mut arena, &mut dmg);
        let out2 = dispatch_with_line(&pool, &gd, Some(&id()), &json, ts, &mut st, Some("Chat.log:22"));
        assert_eq!(out1.staged_gains.len(), 1);
        assert_eq!(out2.staged_gains.len(), 1, "genuine second receipt stages its own gain");
        let rows = item_rows(&pool);
        assert_eq!(rows.len(), 2);
        assert!(rows.iter().all(|r| r.3 == Some(257400)));
    }

    #[test]
    fn twin_pair_times_two_events_both_orders() {
        // Two genuine twin pairs (English + JSON each), same key+entity,
        // four line ids: exactly 2 rows, both stamped with the entity, and
        // exactly 2 staged gains — in both arrival orders.
        for json_first in [false, true] {
            let (_guard, pool) = test_pool();
            let gd = game_data();
            let mut arena = ArenaTracker::new();
            let mut dmg = Vec::new();
            let ts = "26-11-17 08:50:27";
            let english = status_msg("Toxic Sludge added to inventory.", ts);
            let json = json_msg("AddItem", LOOT_BODY, ts);
            // Line ids stay with their lines across both orders.
            let seq: [(bool, &str); 4] = if json_first {
                [(true, "Chat.log:21"), (true, "Chat.log:22"), (false, "Chat.log:11"), (false, "Chat.log:12")]
            } else {
                [(false, "Chat.log:11"), (false, "Chat.log:12"), (true, "Chat.log:21"), (true, "Chat.log:22")]
            };
            let mut staged = 0;
            for (is_json, line) in seq {
                let m = if is_json { &json } else { &english };
                let mut st = state(&mut arena, &mut dmg);
                staged += dispatch_with_line(&pool, &gd, Some(&id()), m, ts, &mut st, Some(line))
                    .staged_gains
                    .len();
            }
            assert_eq!(staged, 2, "each event stages exactly once (json_first={json_first})");
            let rows = item_rows(&pool);
            assert_eq!(rows.len(), 2, "two events keep two rows (json_first={json_first})");
            assert!(
                rows.iter().all(|r| r.3 == Some(257400)),
                "both rows stamped (json_first={json_first})"
            );
        }
    }

    #[test]
    fn full_redispatch_is_stable() {
        // Re-dispatch all four lines with their original ids: still 2 rows,
        // 0 newly staged. Guards the INSERT-happy rules against rescan
        // duplication — every replay must hit its own trace.
        let (_guard, pool) = test_pool();
        let gd = game_data();
        let mut arena = ArenaTracker::new();
        let mut dmg = Vec::new();
        let ts = "26-11-17 08:50:27";
        let english = status_msg("Toxic Sludge added to inventory.", ts);
        let json = json_msg("AddItem", LOOT_BODY, ts);
        let seq: [(&ChatMessage, &str); 4] = [
            (&english, "Chat.log:11"),
            (&english, "Chat.log:12"),
            (&json, "Chat.log:21"),
            (&json, "Chat.log:22"),
        ];
        let mut staged_first = 0;
        for (m, line) in seq {
            let mut st = state(&mut arena, &mut dmg);
            staged_first += dispatch_with_line(&pool, &gd, Some(&id()), m, ts, &mut st, Some(line))
                .staged_gains
                .len();
        }
        assert_eq!(staged_first, 2);
        assert_eq!(item_rows(&pool).len(), 2);
        let mut staged_second = 0;
        for (m, line) in seq {
            let mut st = state(&mut arena, &mut dmg);
            staged_second += dispatch_with_line(&pool, &gd, Some(&id()), m, ts, &mut st, Some(line))
                .staged_gains
                .len();
        }
        assert_eq!(staged_second, 0, "replays must not restage");
        assert_eq!(item_rows(&pool).len(), 2, "replays must not duplicate");
    }

    #[test]
    fn json_additem_rescan_applies_once() {
        let (_guard, pool) = test_pool();
        let gd = game_data();
        let mut arena = ArenaTracker::new();
        let mut dmg = Vec::new();
        
        let ts = "26-11-17 08:50:27";
        let json = json_msg("AddItem", LOOT_BODY, ts);
        let mut st = state(&mut arena, &mut dmg);
        let out1 = dispatch(&pool, &gd, Some(&id()), &json, ts, &mut st);
        let mut st = state(&mut arena, &mut dmg);
        let out2 = dispatch(&pool, &gd, Some(&id()), &json, ts, &mut st);
        assert_eq!(out1.staged_gains.len(), 1);
        assert!(out2.staged_gains.is_empty());
        assert_eq!(item_rows(&pool).len(), 1);
    }

    #[test]
    fn replayed_gain_does_not_restage_attribution() {
        // A replayed English gain must not re-attribute: the ledger row
        // already exists, so no channel stages. Inventory quantities come
        // from snapshot imports, not from chat lines.
        let (_guard, pool) = test_pool();
        let gd = game_data();
        let mut arena = ArenaTracker::new();
        let mut dmg = Vec::new();
        let ts = "26-04-01 05:30:53";

        let m = status_msg("Mandrake Root x5 added to inventory.", ts);
        // First dispatch: new ledger row, gain stages exactly once.
        let mut st = state(&mut arena, &mut dmg);
        let out1 = dispatch(&pool, &gd, Some(&id()), &m, ts, &mut st);
        assert_eq!(out1.staged_gains.len(), 1);
        // Replay finds the ledger row: attribution stays silent.
        let mut st = state(&mut arena, &mut dmg);
        let out2 = dispatch(&pool, &gd, Some(&id()), &m, ts, &mut st);
        assert!(out2.staged_gains.is_empty(), "replay must not re-attribute");
        // A further replay converges without doubling.
        let mut st = state(&mut arena, &mut dmg);
        let out3 = dispatch(&pool, &gd, Some(&id()), &m, ts, &mut st);
        assert!(out3.staged_gains.is_empty());
        let ledger: i64 = pool
            .get()
            .unwrap()
            .query_row("SELECT COUNT(*) FROM item_transactions", [], |r| r.get(0))
            .unwrap();
        assert_eq!(ledger, 1, "exactly one ledger row");
    }

    #[test]
    fn replayed_json_receipt_does_not_restage_attribution() {
        // JSON mirror of replayed_gain_does_not_restage_attribution: a
        // replayed AddItem receipt hits the exact-entity row, so attribution
        // stays silent and the ledger keeps exactly one row.
        let (_guard, pool) = test_pool();
        let gd = game_data();
        let mut arena = ArenaTracker::new();
        let mut dmg = Vec::new();
        let ts = "26-04-01 05:30:53";

        let body = "{\"Name\": \"Mandrake Root\", \"TypeId\": 1137, \"Stack\": 5, \"Flags\": [], \"FromName\": \"Olugax The Ever-Pudding\", \"FromEntityId\": 257400}";
        let json = json_msg("AddItem", body, ts);
        // First dispatch: new receipt row, gain stages exactly once.
        let mut st = state(&mut arena, &mut dmg);
        let out1 = dispatch(&pool, &gd, Some(&id()), &json, ts, &mut st);
        assert_eq!(out1.staged_gains.len(), 1);
        // Replay hits the exact-entity row: attribution stays silent.
        let mut st = state(&mut arena, &mut dmg);
        let out2 = dispatch(&pool, &gd, Some(&id()), &json, ts, &mut st);
        assert!(out2.staged_gains.is_empty(), "replay must not re-attribute");
        // A further replay converges without doubling.
        let mut st = state(&mut arena, &mut dmg);
        let out3 = dispatch(&pool, &gd, Some(&id()), &json, ts, &mut st);
        assert!(out3.staged_gains.is_empty());
        let rows = item_rows(&pool);
        assert_eq!(rows.len(), 1, "exactly one ledger row");
        assert_eq!(rows[0].1, 5);
        assert_eq!(rows[0].3, Some(257400));
    }

    #[test]
    fn json_receipt_same_second_second_entity_records() {
        // Two slimes, one Coin Sack each, same second: entity-scoped EXISTS
        // keeps both gains; a byte-identical resend still collapses.
        let (_guard, pool) = test_pool();
        let ts = "26-11-17 08:50:27";
        let first = insert_json_receipt_guarded(
            &pool, ts, "Alice", "Dreva", "Coin Sack", None, None, 1,
            Some("Slime A"), Some(1001),
            None,
            false,
        );
        assert!(first);
        let second = insert_json_receipt_guarded(
            &pool, ts, "Alice", "Dreva", "Coin Sack", None, None, 1,
            Some("Slime B"), Some(1002),
            None,
            false,
        );
        assert!(second, "second entity's receipt is a distinct gain");
        let resend = insert_json_receipt_guarded(
            &pool, ts, "Alice", "Dreva", "Coin Sack", None, None, 1,
            Some("Slime A"), Some(1001),
            None,
            false,
        );
        assert!(!resend, "byte-identical resend collapses");
        assert_eq!(item_rows(&pool).len(), 2);
    }

    #[test]
    fn scan_skips_old_unsourced_english_gain() {
        // Kill-only ledger: a scan of an old English loot line with no
        // survey tag writes nothing (the end-of-scan sweep would delete it
        // in the same pass). Pinned old date stays old forever.
        let (_guard, pool) = test_pool();
        let gd = game_data();
        let mut arena = ArenaTracker::new();
        let mut dmg = Vec::new();
        let ts = "26-11-17 08:50:27";
        let english = status_msg("Mandrake Root x5 added to inventory.", ts);
        let mut st = state(&mut arena, &mut dmg);
        let out = dispatch_scan(&pool, &gd, Some(&id()), &english, ts, &mut st);
        assert!(out.staged_gains.is_empty());
        assert_eq!(item_rows(&pool).len(), 0);
    }

    #[test]
    fn scan_keeps_old_sourced_json_receipt() {
        // A sourced receipt always writes, even old and on a scan: it
        // carries the kill link the ledger keeps.
        let (_guard, pool) = test_pool();
        let gd = game_data();
        let mut arena = ArenaTracker::new();
        let mut dmg = Vec::new();
        let ts = "26-11-17 08:50:27";
        let json = json_msg("AddItem", LOOT_BODY, ts);
        let mut st = state(&mut arena, &mut dmg);
        let out = dispatch_scan(&pool, &gd, Some(&id()), &json, ts, &mut st);
        assert_eq!(out.staged_gains.len(), 1);
        let rows = item_rows(&pool);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].3, Some(257400));
    }

    #[test]
    fn scan_skips_old_bare_json_receipt() {
        // A bare old receipt could never attribute: skip it.
        let (_guard, pool) = test_pool();
        let gd = game_data();
        let mut arena = ArenaTracker::new();
        let mut dmg = Vec::new();
        let ts = "26-11-17 08:50:27";
        let bare = "{\"Name\": \"Mandrake Root\", \"TypeId\": 1012, \"Stack\": 5, \"Flags\": []}";
        let json = json_msg("AddItem", bare, ts);
        let mut st = state(&mut arena, &mut dmg);
        let out = dispatch_scan(&pool, &gd, Some(&id()), &json, ts, &mut st);
        assert!(out.staged_gains.is_empty());
        assert_eq!(item_rows(&pool).len(), 0);
    }

    #[test]
    fn live_keeps_old_unsourced_english_gain() {
        // The live drain writes provisional rows regardless of age; the
        // sweeper owns aging them out. Pinned old date proves the point.
        let (_guard, pool) = test_pool();
        let gd = game_data();
        let mut arena = ArenaTracker::new();
        let mut dmg = Vec::new();
        let ts = "26-11-17 08:50:27";
        let english = status_msg("Mandrake Root x5 added to inventory.", ts);
        let mut st = state(&mut arena, &mut dmg);
        let out = dispatch(&pool, &gd, Some(&id()), &english, ts, &mut st);
        assert_eq!(out.staged_gains.len(), 1);
        assert_eq!(item_rows(&pool).len(), 1);
    }

    #[test]
    fn json_xp_upserts_without_level_up() {
        let (_guard, pool) = test_pool();
        let gd = game_data();
        seed_skill(&gd, "Teleportation", 4242);
        let mut arena = ArenaTracker::new();
        let mut dmg = Vec::new();
        
        let ts = "26-11-17 08:42:52";
        let json = json_msg(
            "EarnedXP",
            "{\"Skill\":\"Teleportation\", \"XP\": 53, \"OldLevel\": 46, \"NewLevel\": 46}",
            ts,
        );
        let mut st = state(&mut arena, &mut dmg);
        let out = dispatch(&pool, &gd, Some(&id()), &json, ts, &mut st);
        assert!(out.domains.iter().any(|d| d == "skills"));
        let mut st = state(&mut arena, &mut dmg);
        dispatch(&pool, &gd, Some(&id()), &json, ts, &mut st);
        let (xp, level, n): (i64, i64, i64) = pool
            .get()
            .unwrap()
            .query_row(
                "SELECT xp, level, COUNT(*) FROM game_state_skills
                 WHERE character_name = 'Alice' AND server_name = 'Dreva'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .unwrap();
        assert_eq!((xp, level, n), (53, 46, 1));
        // Live-fixture sequence: Riding awards 250 → 350 → 300 at rising
        // timestamps (non-monotonic, so increments, not totals). They
        // accumulate to 900; the byte-identical resend stays a no-op.
        seed_skill(&gd, "Riding", 4244);
        for (ts, amount) in [
            ("26-11-17 08:44:08", 250),
            ("26-11-17 08:44:23", 350),
            ("26-11-17 08:44:38", 300),
        ] {
            let json = json_msg(
                "EarnedXP",
                &format!("{{\"Skill\":\"Riding\", \"XP\": {amount}, \"OldLevel\": 102, \"NewLevel\": 102}}"),
                ts,
            );
            let mut st = state(&mut arena, &mut dmg);
            dispatch(&pool, &gd, Some(&id()), &json, ts, &mut st);
        }
        let xp: i64 = pool
            .get()
            .unwrap()
            .query_row(
                "SELECT xp FROM game_state_skills
                 WHERE character_name = 'Alice' AND server_name = 'Dreva' AND skill_name = 'Riding'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(xp, 900);
    }
    #[test]
    fn json_xp_level_up_uses_chat_writer() {
        let (_guard, pool) = test_pool();
        let gd = game_data();
        seed_skill(&gd, "Sword", 4243);
        let mut arena = ArenaTracker::new();
        let mut dmg = Vec::new();
        
        let ts = "26-11-17 08:42:52";
        let json = json_msg(
            "EarnedXP",
            "{\"Skill\":\"Sword\", \"XP\": 62, \"OldLevel\": 4, \"NewLevel\": 5}",
            ts,
        );
        let mut st = state(&mut arena, &mut dmg);
        let out = dispatch(&pool, &gd, Some(&id()), &json, ts, &mut st);
        assert!(out.domains.iter().any(|d| d == "skills"));
        let level: i64 = pool
            .get()
            .unwrap()
            .query_row(
                "SELECT level FROM game_state_skills
                 WHERE character_name = 'Alice' AND server_name = 'Dreva' AND skill_name = 'Sword'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(level, 5);
    }

    #[test]
    fn xp_same_second_same_level_awards_both_apply() {
        // v101 contract 1: two same-second same-level awards are distinct
        // identities (different XP), so both accumulate — the old WHERE
        // guard matched neither arm and dropped the second.
        let (_guard, pool) = test_pool();
        let gd = game_data();
        seed_skill(&gd, "Riding", 4244);
        let mut arena = ArenaTracker::new();
        let mut dmg = Vec::new();
        let ts = "26-11-17 08:44:08";
        for amount in [250, 350] {
            let json = json_msg(
                "EarnedXP",
                &format!("{{\"Skill\":\"Riding\", \"XP\": {amount}, \"OldLevel\": 102, \"NewLevel\": 102}}"),
                ts,
            );
            let mut st = state(&mut arena, &mut dmg);
            let out = dispatch(&pool, &gd, Some(&id()), &json, ts, &mut st);
            assert!(out.domains.iter().any(|d| d == "skills"));
        }
        let (xp, n): (i64, i64) = pool
            .get()
            .unwrap()
            .query_row(
                "SELECT xp, (SELECT COUNT(*) FROM chat_xp_awards
                 WHERE character_name = 'Alice' AND server_name = 'Dreva' AND skill_id = 4244)
                 FROM game_state_skills
                 WHERE character_name = 'Alice' AND server_name = 'Dreva' AND skill_name = 'Riding'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!((xp, n), (600, 2));
    }

    #[test]
    fn xp_out_of_order_older_award_accumulates() {
        // v101 contract 2: an older award arriving after newer state is a
        // distinct award, not a replay — it accumulates, and the clock
        // stays monotonic.
        let (_guard, pool) = test_pool();
        let gd = game_data();
        seed_skill(&gd, "Riding", 4244);
        let mut arena = ArenaTracker::new();
        let mut dmg = Vec::new();
        let newer = json_msg(
            "EarnedXP",
            "{\"Skill\":\"Riding\", \"XP\": 50, \"OldLevel\": 102, \"NewLevel\": 102}",
            "26-11-17 08:44:38",
        );
        let mut st = state(&mut arena, &mut dmg);
        let out = dispatch(&pool, &gd, Some(&id()), &newer, "26-11-17 08:44:38", &mut st);
        assert!(out.domains.iter().any(|d| d == "skills"));
        let older = json_msg(
            "EarnedXP",
            "{\"Skill\":\"Riding\", \"XP\": 30, \"OldLevel\": 102, \"NewLevel\": 102}",
            "26-11-17 08:44:08",
        );
        let mut st = state(&mut arena, &mut dmg);
        let out = dispatch(&pool, &gd, Some(&id()), &older, "26-11-17 08:44:08", &mut st);
        assert!(out.domains.iter().any(|d| d == "skills"));
        let (xp, ts): (i64, String) = pool
            .get()
            .unwrap()
            .query_row(
                "SELECT xp, last_confirmed_at FROM game_state_skills
                 WHERE character_name = 'Alice' AND server_name = 'Dreva' AND skill_name = 'Riding'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!((xp, ts.as_str()), (80, "2026-11-17 08:44:38"));
    }

    #[test]
    fn xp_replayed_award_does_not_double_count() {
        // v101 contract 3: a byte-identical resend finds its ledger row
        // and stays silent — the total moves once.
        let (_guard, pool) = test_pool();
        let gd = game_data();
        seed_skill(&gd, "Riding", 4244);
        let mut arena = ArenaTracker::new();
        let mut dmg = Vec::new();
        let ts = "26-11-17 08:44:08";
        let json = json_msg(
            "EarnedXP",
            "{\"Skill\":\"Riding\", \"XP\": 250, \"OldLevel\": 102, \"NewLevel\": 102}",
            ts,
        );
        let mut st = state(&mut arena, &mut dmg);
        let out = dispatch(&pool, &gd, Some(&id()), &json, ts, &mut st);
        assert!(out.domains.iter().any(|d| d == "skills"));
        let mut st = state(&mut arena, &mut dmg);
        let out = dispatch(&pool, &gd, Some(&id()), &json, ts, &mut st);
        assert!(out.domains.iter().all(|d| d != "skills"));
        let (xp, n): (i64, i64) = pool
            .get()
            .unwrap()
            .query_row(
                "SELECT xp, (SELECT COUNT(*) FROM chat_xp_awards
                 WHERE character_name = 'Alice' AND server_name = 'Dreva' AND skill_id = 4244)
                 FROM game_state_skills
                 WHERE character_name = 'Alice' AND server_name = 'Dreva' AND skill_name = 'Riding'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!((xp, n), (250, 1));
    }

    #[test]
    fn xp_english_json_level_up_twin_applies_once_both_orders() {
        // Twin convergence under the ledger: English `LevelUp` + JSON
        // `EarnedXP` carry the same award identity, so the second — either
        // order — finds the row and stays silent. The old same-second drop
        // also converged twins, by accident; this pins it on identity.
        for english_first in [true, false] {
            let (_guard, pool) = test_pool();
            let gd = game_data();
            seed_skill(&gd, "Sword", 4243);
            let mut arena = ArenaTracker::new();
            let mut dmg = Vec::new();
            let ts = "26-11-17 08:42:52";
            let english = status_msg("You earned 62 XP and reached level 5 in Sword!", ts);
            let json = json_msg(
                "EarnedXP",
                "{\"Skill\":\"Sword\", \"XP\": 62, \"OldLevel\": 4, \"NewLevel\": 5}",
                ts,
            );
            let (first, second) = if english_first {
                (&english, &json)
            } else {
                (&json, &english)
            };
            let mut st = state(&mut arena, &mut dmg);
            let out = dispatch(&pool, &gd, Some(&id()), first, ts, &mut st);
            assert!(out.domains.iter().any(|d| d == "skills"), "first twin applies (english_first={english_first})");
            let mut st = state(&mut arena, &mut dmg);
            let out = dispatch(&pool, &gd, Some(&id()), second, ts, &mut st);
            assert!(out.domains.iter().all(|d| d != "skills"), "twin second skips (english_first={english_first})");
            let (level, xp): (i64, i64) = pool
                .get()
                .unwrap()
                .query_row(
                    "SELECT level, xp FROM game_state_skills
                     WHERE character_name = 'Alice' AND server_name = 'Dreva' AND skill_name = 'Sword'",
                    [],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .unwrap();
            assert_eq!((level, xp), (5, 62), "twin awards once (english_first={english_first})");
        }
    }

    #[test]
    fn json_favor_delta_accrues_once_with_gift_row() {
        let (_guard, pool) = test_pool();
        let gd = game_data();
        seed_npc(&gd, "NPC_Elune", "Elune", Some("AreaSerbule"), true);
        let mut arena = ArenaTracker::new();
        let mut dmg = Vec::new();
        let ts = "26-11-17 08:32:34";
        let json = json_msg("DeltaFavor", "{\"Npc\":\"NPC_Elune\", \"Delta\": 18.06948}", ts);
        let mut st = state(&mut arena, &mut dmg);
        let out1 = dispatch(&pool, &gd, Some(&id()), &json, ts, &mut st);
        let mut st = state(&mut arena, &mut dmg);
        let out2 = dispatch(&pool, &gd, Some(&id()), &json, ts, &mut st);
        assert!(out1.domains.iter().any(|d| d == "favor"));
        assert!(out2.domains.iter().all(|d| d != "favor"));
        let conn = pool.get().unwrap();
        // One event row for two identical applications — the log, not a total.
        let events: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM favor_events
                 WHERE character_name = 'Alice' AND server_name = 'Dreva' AND npc_key = 'NPC_Elune'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(events, 1);
        let delta: f64 = conn
            .query_row(
                "SELECT delta FROM favor_events
                 WHERE character_name = 'Alice' AND server_name = 'Dreva' AND npc_key = 'NPC_Elune'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert!((delta - 18.06948).abs() < 1e-9);
        let gifts: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM game_state_gift_log
                 WHERE character_name = 'Alice' AND server_name = 'Dreva' AND npc_key = 'NPC_Elune'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(gifts, 1);
    }

    #[test]
    fn favor_same_second_same_delta_distinct_lines_insert_two_rows() {
        // Post-#352 line identity: two genuinely distinct same-second
        // same-delta gifts (different file offsets) are two rows, not one.
        // The old (npc, second, delta) key collapsed them; the line id
        // separates per-occurrence twins.
        let (_guard, pool) = test_pool();
        let gd = game_data();
        seed_npc(&gd, "NPC_Elune", "Elune", Some("AreaSerbule"), true);
        let mut arena = ArenaTracker::new();
        let mut dmg = Vec::new();
        let ts = "26-11-17 08:32:34";
        let json = json_msg("DeltaFavor", "{\"Npc\":\"NPC_Elune\", \"Delta\": 18.06948}", ts);
        let mut st = state(&mut arena, &mut dmg);
        let out1 = dispatch_with_line(&pool, &gd, Some(&id()), &json, ts, &mut st, Some("Chat.log:100"));
        let mut st = state(&mut arena, &mut dmg);
        let out2 = dispatch_with_line(&pool, &gd, Some(&id()), &json, ts, &mut st, Some("Chat.log:200"));
        assert!(out1.domains.iter().any(|d| d == "favor"));
        assert!(out2.domains.iter().any(|d| d == "favor"));
        let conn = pool.get().unwrap();
        let events: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM favor_events
                 WHERE character_name = 'Alice' AND server_name = 'Dreva' AND npc_key = 'NPC_Elune'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(events, 2);
        let gifts: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM game_state_gift_log
                 WHERE character_name = 'Alice' AND server_name = 'Dreva' AND npc_key = 'NPC_Elune'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(gifts, 2);
    }

    #[test]
    fn favor_same_line_replay_collapses_to_one_row() {
        // Same line id twice (rescan replay) stays one row: the second
        // application is a no-op, exactly like the legacy content key.
        let (_guard, pool) = test_pool();
        let gd = game_data();
        seed_npc(&gd, "NPC_Elune", "Elune", Some("AreaSerbule"), true);
        let mut arena = ArenaTracker::new();
        let mut dmg = Vec::new();
        let ts = "26-11-17 08:32:34";
        let json = json_msg("DeltaFavor", "{\"Npc\":\"NPC_Elune\", \"Delta\": 18.06948}", ts);
        let mut st = state(&mut arena, &mut dmg);
        let out1 = dispatch_with_line(&pool, &gd, Some(&id()), &json, ts, &mut st, Some("Chat.log:100"));
        let mut st = state(&mut arena, &mut dmg);
        let out2 = dispatch_with_line(&pool, &gd, Some(&id()), &json, ts, &mut st, Some("Chat.log:100"));
        assert!(out1.domains.iter().any(|d| d == "favor"));
        assert!(out2.domains.iter().all(|d| d != "favor"));
        let conn = pool.get().unwrap();
        let events: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM favor_events
                 WHERE character_name = 'Alice' AND server_name = 'Dreva' AND npc_key = 'NPC_Elune'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(events, 1);
    }

    #[test]
    fn favor_stamped_line_matching_legacy_null_row_collapses() {
        // Upgrade boundary: a pre-identity (`NULL`) row keeps match-any
        // collapse, so the first post-upgrade rescan (which stamps every
        // line) cannot duplicate history. Mirrors the ledger guards.
        let (_guard, pool) = test_pool();
        let gd = game_data();
        seed_npc(&gd, "NPC_Elune", "Elune", Some("AreaSerbule"), true);
        let mut arena = ArenaTracker::new();
        let mut dmg = Vec::new();
        let ts = "26-11-17 08:32:34";
        let json = json_msg("DeltaFavor", "{\"Npc\":\"NPC_Elune\", \"Delta\": 18.06948}", ts);
        let mut st = state(&mut arena, &mut dmg);
        dispatch(&pool, &gd, Some(&id()), &json, ts, &mut st);
        let mut st = state(&mut arena, &mut dmg);
        let out = dispatch_with_line(&pool, &gd, Some(&id()), &json, ts, &mut st, Some("Chat.log:100"));
        assert!(out.domains.iter().all(|d| d != "favor"));
        let conn = pool.get().unwrap();
        let events: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM favor_events
                 WHERE character_name = 'Alice' AND server_name = 'Dreva' AND npc_key = 'NPC_Elune'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(events, 1);
        let gifts: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM game_state_gift_log
                 WHERE character_name = 'Alice' AND server_name = 'Dreva' AND npc_key = 'NPC_Elune'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(gifts, 1);
    }

    #[test]
    fn json_favor_negative_delta_accrues_but_writes_no_gift_row() {
        let (_guard, pool) = test_pool();
        let gd = game_data();
        seed_npc(&gd, "NPC_Elune", "Elune", Some("AreaSerbule"), true);
        let mut arena = ArenaTracker::new();
        let mut dmg = Vec::new();
        let ts = "26-11-17 08:32:34";
        let json = json_msg("DeltaFavor", "{\"Npc\":\"NPC_Elune\", \"Delta\": -52.65}", ts);
        let mut st = state(&mut arena, &mut dmg);
        dispatch(&pool, &gd, Some(&id()), &json, ts, &mut st);
        let conn = pool.get().unwrap();
        // Favor loss is real and kept as an event (session sums go negative).
        let delta: f64 = conn
            .query_row(
                "SELECT delta FROM favor_events
                 WHERE character_name = 'Alice' AND server_name = 'Dreva' AND npc_key = 'NPC_Elune'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert!((delta + 52.65).abs() < 1e-9);
        // But a loss is never a gift, even for a giftable NPC.
        let gifts: i64 = conn
            .query_row("SELECT COUNT(*) FROM game_state_gift_log", [], |r| r.get(0))
            .unwrap();
        assert_eq!(gifts, 0);
    }

    #[test]
    fn json_favor_older_delta_does_not_rewind_confirmed_at() {
        // An unseen older-stamped delta (catch-up scan of a rotated log)
        // must not rewind `last_confirmed_at`: the snapshot seeder's
        // recency guard compares against it, so a rewind would let a later
        // older report replace a newer tier. The event row itself still
        // lands (the ledger is append-only); only the watermark is guarded.
        let (_guard, pool) = test_pool();
        let gd = game_data();
        seed_npc(&gd, "NPC_Elune", "Elune", Some("AreaSerbule"), true);
        let conn = pool.get_write().unwrap();
        conn.execute(
            "INSERT INTO game_state_favor
                (character_name, server_name, npc_key, npc_name, favor_tier, last_confirmed_at, source)
             VALUES ('Alice', 'Dreva', 'NPC_Elune', 'Elune', 'Close Friends', '2026-11-17 10:00:00', 'snapshot')",
            [],
        )
        .unwrap();
        drop(conn);
        let mut arena = ArenaTracker::new();
        let mut dmg = Vec::new();
        let old_ts = "26-11-17 08:00:00";
        let old = json_msg("DeltaFavor", "{\"Npc\":\"NPC_Elune\", \"Delta\": 5.0}", old_ts);
        let mut st = state(&mut arena, &mut dmg);
        dispatch(&pool, &gd, Some(&id()), &old, old_ts, &mut st);
        let conn = pool.get().unwrap();
        let (tier, confirmed): (Option<String>, String) = conn
            .query_row(
                "SELECT favor_tier, last_confirmed_at FROM game_state_favor
                 WHERE character_name = 'Alice' AND server_name = 'Dreva' AND npc_key = 'NPC_Elune'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(tier.as_deref(), Some("Close Friends"));
        assert_eq!(confirmed, "2026-11-17 10:00:00");
        drop(conn);
        let new_ts = "26-11-17 11:00:00";
        let new = json_msg("DeltaFavor", "{\"Npc\":\"NPC_Elune\", \"Delta\": 5.0}", new_ts);
        let mut st = state(&mut arena, &mut dmg);
        dispatch(&pool, &gd, Some(&id()), &new, new_ts, &mut st);
        let conn = pool.get().unwrap();
        let confirmed: String = conn
            .query_row(
                "SELECT last_confirmed_at FROM game_state_favor
                 WHERE character_name = 'Alice' AND server_name = 'Dreva' AND npc_key = 'NPC_Elune'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(confirmed, "2026-11-17 11:00:00");
    }

    #[test]
    fn upsert_chat_xp_newer_lower_level_does_not_downgrade() {
        // Mirrors `apply_chat_level_up`'s guard: a newer-stamped award
        // carrying a lower level (explicit 0/0 first-XP arriving after the
        // skill is known, or any out-of-order line) must not rewind the
        // stored level. The conflicting update is skipped whole, matching
        // the level-up writer's drop semantics.
        let (_guard, pool) = test_pool();
        let conn = pool.get_write().unwrap();
        conn.execute(
            "INSERT INTO game_state_skills
                (character_name, server_name, skill_id, skill_name, level, base_level, bonus_levels, xp, tnl, max_level, last_confirmed_at, source)
             VALUES ('Alice', 'Dreva', 7, 'Sword', 5, 5, 0, 100, 0, 0, '2026-11-17 10:00:00', 'snapshot')",
            [],
        )
        .unwrap();
        let changed = upsert_chat_xp(&conn, "Alice", "Dreva", 7, "Sword", 0, 53, "2026-11-17 11:00:00");
        assert!(!changed);
        let (level, xp): (i64, i64) = conn
            .query_row(
                "SELECT level, xp FROM game_state_skills
                 WHERE character_name = 'Alice' AND server_name = 'Dreva' AND skill_id = 7",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!((level, xp), (5, 100));
        let changed = upsert_chat_xp(&conn, "Alice", "Dreva", 7, "Sword", 6, 10, "2026-11-17 12:00:00");
        assert!(changed);
        let (level, xp): (i64, i64) = conn
            .query_row(
                "SELECT level, xp FROM game_state_skills
                 WHERE character_name = 'Alice' AND server_name = 'Dreva' AND skill_id = 7",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        // Level change stores just the award: the old level-5 progress (100)
        // must not carry into level 6.
        assert_eq!((level, xp), (6, 10));
    }

    #[test]
    fn upsert_chat_xp_same_level_awards_accumulate() {
        let (_guard, pool) = test_pool();
        let conn = pool.get_write().unwrap();
        conn.execute(
            "INSERT INTO game_state_skills
                (character_name, server_name, skill_id, skill_name, level, base_level, bonus_levels, xp, tnl, max_level, last_confirmed_at, source)
             VALUES ('Alice', 'Dreva', 7, 'Sword', 5, 5, 0, 100, 0, 0, '2026-11-17 10:00:00', 'snapshot')",
            [],
        )
        .unwrap();
        let changed = upsert_chat_xp(&conn, "Alice", "Dreva", 7, "Sword", 5, 50, "2026-11-17 11:00:00");
        assert!(changed);
        let (level, xp): (i64, i64) = conn
            .query_row(
                "SELECT level, xp FROM game_state_skills
                 WHERE character_name = 'Alice' AND server_name = 'Dreva' AND skill_id = 7",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!((level, xp), (5, 150));
    }

    #[test]
    fn json_favor_delta_skips_gift_row_when_capped_or_tasteless() {
        let (_guard, pool) = test_pool();
        let gd = game_data();
        seed_npc(&gd, "NPC_Goss", "Goss", Some("AreaStatehelm"), true);
        seed_npc(&gd, "NPC_Card", "Card", Some("AreaSerbule"), false);
        let mut arena = ArenaTracker::new();
        let mut dmg = Vec::new();
        
        let ts = "26-11-17 08:32:34";
        for npc in ["NPC_Goss", "NPC_Card"] {
            let json = json_msg(
                "DeltaFavor",
                &format!("{{\"Npc\":\"{npc}\", \"Delta\": 5.0}}"),
                ts,
            );
            let mut st = state(&mut arena, &mut dmg);
            dispatch(&pool, &gd, Some(&id()), &json, ts, &mut st);
        }
        let conn = pool.get().unwrap();
        // Both deltas are retained as events — including the Statehelm one.
        // Session sums cover capped NPCs; only gift *detection* excludes them.
        let n_events: i64 = conn
            .query_row("SELECT COUNT(*) FROM favor_events", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n_events, 2);
        // Favor rows exist for name/tier display in both cases.
        let n_favor: i64 = conn
            .query_row("SELECT COUNT(*) FROM game_state_favor", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n_favor, 2);
        // No gift rows: capped counts are manual-only (Statehelm +/- UI), and
        // tasteless NPCs cannot receive gifts.
        let n_gifts: i64 = conn
            .query_row("SELECT COUNT(*) FROM game_state_gift_log", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n_gifts, 0);
    }

    #[test]
    fn json_recipe_used_sets_absolute_count() {
        let (_guard, pool) = test_pool();
        let gd = game_data();
        let mut arena = ArenaTracker::new();
        let mut dmg = Vec::new();
        
        let ts = "26-11-17 08:42:52";
        let json = json_msg(
            "RecipeUsage",
            "{\"RecipeName\":\"Recall Heart's Home\", \"RecipeId\": 9008, \"Completions\": 211}",
            ts,
        );
        let mut st = state(&mut arena, &mut dmg);
        let out = dispatch(&pool, &gd, Some(&id()), &json, ts, &mut st);
        assert!(out.domains.iter().any(|d| d == "recipes"));
        let n: i64 = pool
            .get()
            .unwrap()
            .query_row(
                "SELECT completion_count FROM game_state_recipes
                 WHERE character_name = 'Alice' AND server_name = 'Dreva' AND recipe_id = 9008",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(n, 211);
        // Same-second progress (212 crafts in the same second) applies;
        // the byte-identical resend stays a no-op and stays silent.
        let json = json_msg(
            "RecipeUsage",
            "{\"RecipeName\":\"Recall Heart's Home\", \"RecipeId\": 9008, \"Completions\": 212}",
            ts,
        );
        let mut st = state(&mut arena, &mut dmg);
        let out = dispatch(&pool, &gd, Some(&id()), &json, ts, &mut st);
        assert!(out.domains.iter().any(|d| d == "recipes"));
        let mut st = state(&mut arena, &mut dmg);
        let out = dispatch(&pool, &gd, Some(&id()), &json, ts, &mut st);
        assert!(out.domains.iter().all(|d| d != "recipes"));
        let n: i64 = pool
            .get()
            .unwrap()
            .query_row(
                "SELECT completion_count FROM game_state_recipes
                 WHERE character_name = 'Alice' AND server_name = 'Dreva' AND recipe_id = 9008",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(n, 212);
    }
    #[test]
    fn json_book_upserts_and_reports_trigger() {
        let (_guard, pool) = test_pool();
        let gd = game_data();
        let mut arena = ArenaTracker::new();
        let mut dmg = Vec::new();
        
        let ts = "26-11-17 08:31:58";
        let json = json_msg(
            "Book",
            "{\"Title\":\"Skill Info\", \"Body\":\"Foods Consumed:\\n\\n  Bacon: 22\"}",
            ts,
        );
        let mut st = state(&mut arena, &mut dmg);
        let out = dispatch(&pool, &gd, Some(&id()), &json, ts, &mut st);
        assert!(out.domains.is_empty(), "book upserts stay silent — no FE books consumer");
        let (book_type, title): (String, String) = pool
            .get()
            .unwrap()
            .query_row(
                "SELECT book_type, title FROM game_state_books
                 WHERE character_name = 'Alice' AND server_name = 'Dreva'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(book_type, "SkillReport");
        assert_eq!(title, "Skill Info");
        let (t, b, by, at): (String, String, String, String) =
            out.json_book.expect("book trigger payload");
        assert_eq!((t.as_str(), b.as_str(), at.as_str()), ("SkillReport", "Skill Info", "2026-11-17 08:31:58"));
        assert!(by.contains("Bacon"));
        // Rescan of the identical book: no write, no domain — but the
        // trigger payload still flows so caller-side imports stay converged.
        let mut st = state(&mut arena, &mut dmg);
        let out2 = dispatch(&pool, &gd, Some(&id()), &json, ts, &mut st);
        assert!(out2.domains.iter().all(|d| d != "books"));
        assert!(out2.json_book.is_some());
    }

    #[test]
    fn json_book_identical_recapture_advances_observation() {
        let (_guard, pool) = test_pool();
        let gd = game_data();
        let mut arena = ArenaTracker::new();
        let mut dmg = Vec::new();

        let body = "{\"Title\":\"Skill Info\", \"Body\":\"Foods Consumed:\\n\\n  Bacon: 22\"}";
        let ts1 = "26-11-17 08:31:58";
        let mut st = state(&mut arena, &mut dmg);
        let out = dispatch(&pool, &gd, Some(&id()), &json_msg("Book", body, ts1), ts1, &mut st);
        assert!(out.domains.is_empty(), "book upserts stay silent — no FE books consumer");

        // Identical body re-observed later: no domain (rescan silent),
        // but captured_at advances to the newer observation.
        let ts2 = "26-11-17 08:41:58";
        let mut st = state(&mut arena, &mut dmg);
        let out2 = dispatch(&pool, &gd, Some(&id()), &json_msg("Book", body, ts2), ts2, &mut st);
        assert!(out2.domains.iter().all(|d| d != "books"));
        let captured: String = pool
            .get()
            .unwrap()
            .query_row(
                "SELECT captured_at FROM game_state_books
                 WHERE character_name = 'Alice' AND server_name = 'Dreva'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(captured, "2026-11-17T08:41:58+00:00");

        // An older rotated body now loses the freshness check: no write,
        // no domain, stored content untouched.
        let ts0 = "26-11-17 08:21:58";
        let stale = "{\"Title\":\"Skill Info\", \"Body\":\"Foods Consumed:\\n\\n  Eggs: 3\"}";
        let mut st = state(&mut arena, &mut dmg);
        let out3 = dispatch(&pool, &gd, Some(&id()), &json_msg("Book", stale, ts0), ts0, &mut st);
        assert!(out3.domains.iter().all(|d| d != "books"));
        let content: String = pool
            .get()
            .unwrap()
            .query_row(
                "SELECT content FROM game_state_books
                 WHERE character_name = 'Alice' AND server_name = 'Dreva'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert!(content.contains("Bacon"), "older rotated body must not win");
    }

    #[test]
    fn json_unknown_and_malformed_are_accounted() {
        let (_guard, pool) = test_pool();
        let gd = game_data();
        let mut arena = ArenaTracker::new();
        let mut dmg = Vec::new();
        
        let ts = "26-11-17 08:50:27";
        let unknown = json_msg("Report", "{\"Title\":\"Something\"}", ts);
        let mut st = state(&mut arena, &mut dmg);
        let out = dispatch(&pool, &gd, Some(&id()), &unknown, ts, &mut st);
        assert_eq!(out.unknown_json, vec!["Report".to_string()]);
        assert!(out.domains.is_empty());
        let bad = json_msg("AddItem", "not-json", ts);
        let mut st = state(&mut arena, &mut dmg);
        let out = dispatch(&pool, &gd, Some(&id()), &bad, ts, &mut st);
        assert_eq!(out.malformed_json, vec!["AddItem".to_string()]);
    }

    #[test]
    fn json_pigeon_records_message_rescan_safe() {
        let (_guard, pool) = test_pool();
        let gd = game_data();
        let mut arena = ArenaTracker::new();
        let mut dmg = Vec::new();
        let ts = "26-11-17 16:55:59";
        let body = "{\"Title\":\"Pigeon From TestFriend\", \"Body\":\"This one has food!\\n\\n<i>You received:\\nMulberry Salad x10</i>\"}";
        let msg = json_msg("Pigeon", body, ts);
        let mut st = state(&mut arena, &mut dmg);
        let out = dispatch(&pool, &gd, Some(&id()), &msg, ts, &mut st);
        assert!(out.domains.iter().any(|d| d == "player_messages"));
        assert!(out.unknown_json.is_empty());
        let count: i64 = pool
            .get_write()
            .unwrap()
            .query_row(
                "SELECT COUNT(*) FROM player_messages WHERE message_type = 'pigeon' AND other_player = 'TestFriend'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);
        let item: Option<String> = pool
            .get_write()
            .unwrap()
            .query_row(
                "SELECT item_name FROM player_messages WHERE message_type = 'pigeon'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(item.as_deref(), Some("Mulberry Salad"));
        // Same-second pair from the live fixture: the text-only line lands
        // first and must not swallow the item-bearing one (body is in the
        // rescan key, not just the sender + second).
        let plain = json_msg("Pigeon", "{\"Title\":\"Pigeon From TestFriend\", \"Body\":\"This is a pigeon\\n\"}", ts);
        let mut st = state(&mut arena, &mut dmg);
        let out = dispatch(&pool, &gd, Some(&id()), &plain, ts, &mut st);
        assert!(out.domains.iter().any(|d| d == "player_messages"));
        let count: i64 = pool
            .get_write()
            .unwrap()
            .query_row("SELECT COUNT(*) FROM player_messages WHERE message_type = 'pigeon'", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(count, 2);
        // Rescan: both lines replay to the existing rows, domains stay silent.
        let mut st = state(&mut arena, &mut dmg);
        let out = dispatch(&pool, &gd, Some(&id()), &msg, ts, &mut st);
        assert!(out.domains.iter().all(|d| d != "player_messages"));
        let mut st = state(&mut arena, &mut dmg);
        let out = dispatch(&pool, &gd, Some(&id()), &plain, ts, &mut st);
        assert!(out.domains.iter().all(|d| d != "player_messages"));
        let count: i64 = pool
            .get_write()
            .unwrap()
            .query_row("SELECT COUNT(*) FROM player_messages WHERE message_type = 'pigeon'", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(count, 2);
    }

    #[test]
    fn json_pigeon_unparseable_title_is_counted_not_dropped() {
        let (_guard, pool) = test_pool();
        let gd = game_data();
        let mut arena = ArenaTracker::new();
        let mut dmg = Vec::new();
        let ts = "26-11-17 16:55:59";
        let body = "{\"Title\":\"Pigeon To Somebody\", \"Body\":\"...\"}";
        let msg = json_msg("Pigeon", body, ts);
        let mut st = state(&mut arena, &mut dmg);
        let out = dispatch(&pool, &gd, Some(&id()), &msg, ts, &mut st);
        // No message row, but the line stays visible in malformed counters.
        assert!(out.domains.iter().all(|d| d != "player_messages"));
        assert_eq!(out.malformed_json, vec!["Pigeon".to_string()]);
    }

    #[test]
    fn json_entity_report_records_message() {
        let (_guard, pool) = test_pool();
        let gd = game_data();
        let mut arena = ArenaTracker::new();
        let mut dmg = Vec::new();
        let ts = "26-11-17 16:55:02";
        let msgs = crate::chat_parser::parse_chat_lines(include_str!("../../test_data/vip_json/chat-entityreport.log"));
        assert_eq!(msgs.len(), 1);
        let msg = msgs.into_iter().next().unwrap();
        let mut st = state(&mut arena, &mut dmg);
        let out = dispatch(&pool, &gd, Some(&id()), &msg, ts, &mut st);
        assert!(out.domains.iter().any(|d| d == "player_messages"));
        let row: (String, String) = pool
            .get_write()
            .unwrap()
            .query_row(
                "SELECT other_player, body FROM player_messages WHERE message_type = 'entity_report'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(row.0, "Eveline Rastin".to_string());
        assert!(row.1.contains("Ushug"));
    }

    #[test]
    fn json_tip_discovery_records_word_rescan_safe() {
        let (_guard, pool) = test_pool();
        let gd = game_data();
        let mut arena = ArenaTracker::new();
        let mut dmg = Vec::new();
        let ts = "26-11-19 21:48:19";
        let msgs = crate::chat_parser::parse_chat_lines(include_str!("../../test_data/vip_json/chat-tip.log"));
        assert_eq!(msgs.len(), 11);
        // First pass: ten discoveries write, the dementia tip does not.
        let mut hit = false;
        for msg in &msgs {
            let mut st = state(&mut arena, &mut dmg);
            let out = dispatch(&pool, &gd, Some(&id()), msg, ts, &mut st);
            hit |= out.domains.iter().any(|d| d == "words_of_power");
        }
        assert!(hit);
        // Second pass (rescan): rediscovery never duplicates.
        for msg in &msgs {
            let mut st = state(&mut arena, &mut dmg);
            let out = dispatch(&pool, &gd, Some(&id()), msg, ts, &mut st);
            assert!(out.domains.iter().all(|d| d != "words_of_power"));
        }
        let rows: Vec<(String, String, String, String)> = pool
            .get_write()
            .unwrap()
            .prepare("SELECT word, power_name, source, discovered_at FROM words_of_power")
            .unwrap()
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(rows.len(), 10);
        let breath = rows.iter().find(|(w, _, _, _)| w == "BWEPFYW").expect("BWEPFYW");
        assert_eq!(breath.1, "Hold Your Breath");
        assert_eq!(breath.2, "chat");
        assert!(breath.3.starts_with("2026-11-19"));
        // The dementia tip stays visible in unknown counters, never written.
        let mut st = state(&mut arena, &mut dmg);
        let out = dispatch(&pool, &gd, Some(&id()), &msgs[0], ts, &mut st);
        assert_eq!(out.unknown_json, vec!["Tip".to_string()]);
    }

    #[test]
    fn json_paragonxp_is_counted_not_stored() {
        // No DB home: the English Status twin owns the live tally. Typed so
        // the shape is pinned and `unknown_json` stays clean.
        let (_guard, pool) = test_pool();
        let gd = game_data();
        let mut arena = ArenaTracker::new();
        let mut dmg = Vec::new();
        let ts = "26-11-17 17:04:42";
        let msg = json_msg(
            "ParagonXP",
            "{\"Skill\":\"Werewolf\", \"XP\": 279200, \"OldLevel\": 6, \"NewLevel\": 6}",
            ts,
        );
        let mut st = state(&mut arena, &mut dmg);
        let out = dispatch(&pool, &gd, Some(&id()), &msg, ts, &mut st);
        assert!(out.unknown_json.is_empty());
        assert!(out.domains.is_empty());
    }

    #[test]
    fn combat_fatality_records_kill_rescan_safe() {
        let (_guard, pool) = test_pool();
        let gd = game_data();
        let mut arena = ArenaTracker::new();
        let mut dmg = Vec::new();
        let ts = "26-11-17 17:04:42";
        let msg = combat_msg(
            "Alice: Bruising Blow 8 on City Slime #488966! Dmg: 2943 health. (FATALITY!)",
            ts,
        );
        let mut st = state(&mut arena, &mut dmg);
        let out = dispatch(&pool, &gd, Some(&id()), &msg, ts, &mut st);
        assert!(out.enemy_killed.is_some());
        let row: (String, String) = pool
            .get_write()
            .unwrap()
            .query_row(
                "SELECT enemy_name, enemy_entity_id FROM enemy_kills",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(row, ("City Slime".to_string(), "488966".to_string()));
        // Rescan: same FATALITY replays to the existing row.
        let mut st = state(&mut arena, &mut dmg);
        dispatch(&pool, &gd, Some(&id()), &msg, ts, &mut st);
        let count: i64 = pool
            .get_write()
            .unwrap()
            .query_row("SELECT COUNT(*) FROM enemy_kills", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 1);
    }
    #[test]
    fn non_live_kill_records_null_zone() {
        let (_guard, pool) = test_pool();
        let gd = game_data();
        let mut arena = ArenaTracker::new();
        let mut dmg = Vec::new();
        let ts = "26-11-17 17:04:42";
        // Live kill under a known area stamps the zone.
        let live_msg = combat_msg(
            "Alice: Bruising Blow 8 on City Slime #488966! Dmg: 2943 health. (FATALITY!)",
            ts,
        );
        let mut st = state(&mut arena, &mut dmg);
        st.current_area = Some("Serbule".to_string());
        let out = dispatch(&pool, &gd, Some(&id()), &live_msg, ts, &mut st);
        assert!(out.enemy_killed.is_some());
        // Catch-up kill drained under a newer area records NULL, not the
        // drain area — a confident-wrong zone is unrecoverable, NULL reads
        // back as the Unknown bucket.
        let stale_msg = combat_msg(
            "Alice: Bruising Blow 8 on City Slime #488976! Dmg: 2943 health. (FATALITY!)",
            ts,
        );
        let mut st = state(&mut arena, &mut dmg);
        st.current_area = Some("Ilmari Desert".to_string());
        let out = dispatch_scan(&pool, &gd, Some(&id()), &stale_msg, ts, &mut st);
        assert!(out.enemy_killed.is_some());
        let zones: Vec<(String, Option<String>)> = pool
            .get_write()
            .unwrap()
            .prepare("SELECT enemy_entity_id, zone FROM enemy_kills ORDER BY enemy_entity_id")
            .unwrap()
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(
            zones,
            vec![
                ("488966".to_string(), Some("Serbule".to_string())),
                ("488976".to_string(), None),
            ]
        );
    }

    #[test]
    fn json_receipt_links_loot_to_kill() {
        let (_guard, pool) = test_pool();
        let gd = game_data();
        let mut arena = ArenaTracker::new();
        let mut dmg = Vec::new();
        // Kill first, receipt 5s later (live shape: FATALITY 17:04:42 →
        // receipt 17:04:47).
        let kill = combat_msg(
            "Alice: Bruising Blow 8 on City Slime #488966! Dmg: 2943 health. (FATALITY!)",
            "26-11-17 17:04:42",
        );
        let mut st = state(&mut arena, &mut dmg);
        dispatch(&pool, &gd, Some(&id()), &kill, "26-11-17 17:04:42", &mut st);
        let receipt = json_msg(
            "AddItem",
            "{\"Name\": \"Coin Sack\", \"TypeId\": 1325, \"Stack\": 1, \"Flags\": [], \"FromName\": \"City Slime\", \"FromEntityId\": 488966}",
            "26-11-17 17:04:47",
        );
        let mut st = state(&mut arena, &mut dmg);
        dispatch(&pool, &gd, Some(&id()), &receipt, "26-11-17 17:04:47", &mut st);
        // The ledger row carries the stamped entity — the join's only key.
        let ledger: Vec<(String, i64, Option<i64>)> = pool
            .get_write()
            .unwrap()
            .prepare("SELECT item_name, quantity, from_entity_id FROM item_transactions")
            .unwrap()
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(ledger, vec![("Coin Sack".to_string(), 1, Some(488966))]);
        // Join-equivalent attribution: the receipt falls in the kill's window.
        let join_on = crate::db::kill_tracking_commands::kill_receipt_join_on("k", "it");
        let joined: i64 = pool
            .get_write()
            .unwrap()
            .query_row(
                &format!("SELECT COUNT(*) FROM enemy_kills k JOIN item_transactions it ON {join_on}"),
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(joined, 1);
        // Receipt rescan: still one ledger row, join still attributes one.
        let mut st = state(&mut arena, &mut dmg);
        dispatch(&pool, &gd, Some(&id()), &receipt, "26-11-17 17:04:47", &mut st);
        let ledger_count: i64 = pool
            .get_write()
            .unwrap()
            .query_row("SELECT COUNT(*) FROM item_transactions", [], |r| r.get(0))
            .unwrap();
        assert_eq!(ledger_count, 1);
        let joined: i64 = pool
            .get_write()
            .unwrap()
            .query_row(
                &format!("SELECT COUNT(*) FROM enemy_kills k JOIN item_transactions it ON {join_on}"),
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(joined, 1);
    }

    #[test]
    fn unlooted_kill_leaves_no_loot_rows() {
        let (_guard, pool) = test_pool();
        let gd = game_data();
        let mut arena = ArenaTracker::new();
        let mut dmg = Vec::new();
        // FATALITY with no receipt (live #488964): kill recorded, join finds
        // no receipts.
        let kill = combat_msg(
            "Alice: Claw 9 on City Slime #488964! Dmg: 1822 health. (FATALITY!)",
            "26-11-17 17:05:16",
        );
        let mut st = state(&mut arena, &mut dmg);
        dispatch(&pool, &gd, Some(&id()), &kill, "26-11-17 17:05:16", &mut st);
        let kills: i64 = pool
            .get_write()
            .unwrap()
            .query_row("SELECT COUNT(*) FROM enemy_kills", [], |r| r.get(0))
            .unwrap();
        let join_on = crate::db::kill_tracking_commands::kill_receipt_join_on("k", "it");
        let joined: i64 = pool
            .get_write()
            .unwrap()
            .query_row(
                &format!("SELECT COUNT(*) FROM enemy_kills k JOIN item_transactions it ON {join_on}"),
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!((kills, joined), (1, 0));
    }

    #[test]
    fn utc_naive_to_rfc3339_converts_and_falls_back() {
        assert_eq!(
            utc_naive_to_rfc3339("2026-11-18 12:10:00"),
            "2026-11-18T12:10:00+00:00"
        );
        // Unparseable input falls back to now() — never blocks the write.
        let fallback = utc_naive_to_rfc3339("not-a-timestamp");
        assert!(fallback.ends_with("+00:00"), "unexpected stamp {fallback}");
    }

    // B finding (redirect re-review): the merge block must dedupe an auto
    // twin against a covering manual rule without dropping legit auto hits.
    #[test]
    fn ephemeral_auto_twin_dedupes_against_manual_cover() {
        use crate::chat_parser::ItemLink;
        use crate::settings::{ConditionMatch, WatchCondition, WatchNotifyConfig};
        use chrono::NaiveDateTime;

        fn notify() -> WatchNotifyConfig {
            WatchNotifyConfig { sound: false, toast: true, highlight: true }
        }
        fn auto_rule(id: u64, book: &str) -> WatchRule {
            WatchRule {
                id,
                name: format!("Skillbook: {}", book),
                enabled: true,
                channels: None,
                match_mode: ConditionMatch::Any,
                conditions: vec![WatchCondition::ContainsItemLink(book.to_string())],
                notify: notify(),
                source: Some("auto_skillbook".to_string()),
            }
        }
        fn manual_link_rule(id: u64, book: &str) -> WatchRule {
            WatchRule {
                id,
                name: "Manual".to_string(),
                enabled: true,
                channels: None,
                match_mode: ConditionMatch::Any,
                conditions: vec![WatchCondition::ContainsItemLink(book.to_string())],
                notify: notify(),
                source: None,
            }
        }
        fn link_msg(books: &[&str]) -> ChatMessage {
            ChatMessage {
                timestamp: NaiveDateTime::parse_from_str("2026-03-11 12:00:00", "%Y-%m-%d %H:%M:%S")
                    .unwrap(),
                channel: Some("Trade".to_string()),
                sender: Some("Seller".to_string()),
                message: books
                    .iter()
                    .map(|b| format!("[Item: {}]", b))
                    .collect::<Vec<_>>()
                    .join(" "),
                is_system: false,
                from_player: None,
                item_links: books
                    .iter()
                    .map(|b| ItemLink {
                        raw_text: format!("[Item: {}]", b),
                        item_name: b.to_string(),
                    })
                    .collect(),
                json_type: None,
                json_body: None,
            }
        }
        fn run(
            pool: &DbPool,
            gd: &GameDataState,
            msg: &ChatMessage,
            manual: &[WatchRule],
            auto: &[WatchRule],
        ) -> DispatchOutcome {
            let mut arena = ArenaTracker::new();
            let mut dmg = Vec::new();
            let mut st = state(&mut arena, &mut dmg);
            let ident = id();
            dispatch_chat_message(
                DispatchCtx {
                    db: pool,
                    game_data: gd,
                    identity: Some(&ident),
                    msg,
                    local_ts: "2026-03-11 12:00:00",
                    watch_rules: manual,
                    auto_watch_rules: auto,
                    source_line_id: None,
                    is_scan: false,
                    is_live: true,
                },
                None,
                &mut st,
            )
        }

        let (_guard, pool) = test_pool();
        let gd = game_data();
        // Manual covers the same book: exactly one hit (the manual one).
        let out = run(
            &pool,
            &gd,
            &link_msg(&["Sword Basics"]),
            &[manual_link_rule(1, "Sword Basics")],
            &[auto_rule(2, "Sword Basics")],
        );
        assert_eq!(out.watch_triggered.len(), 1);
        assert_eq!(out.watch_triggered[0].rule_id, 1);
        // Manual absent: the auto twin still fires (merge path is live).
        let out = run(&pool, &gd, &link_msg(&["Sword Basics"]), &[], &[auto_rule(2, "Sword Basics")]);
        assert_eq!(out.watch_triggered.len(), 1);
        assert_eq!(out.watch_triggered[0].rule_id, 2);
        // Two uncovered books: both twins fire (no over-suppression).
        let out = run(
            &pool,
            &gd,
            &link_msg(&["Sword Basics", "Unarmed Basics"]),
            &[],
            &[auto_rule(2, "Sword Basics"), auto_rule(3, "Unarmed Basics")],
        );
        assert_eq!(out.watch_triggered.len(), 2);
    }
}
