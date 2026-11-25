use crate::cdn_commands::GameDataState;
use crate::chat_status_dispatch::{self, DispatchIdentity, DispatchState};
use crate::chat_status_parser::ChatStatusEvent;
use crate::db::chat_commands::insert_chat_messages;
use crate::db::queries::log_positions;
use crate::db::DbPool;
use crate::debug_capture::{self, DebugCaptureResult, DebugCaptureState, DebugCaptureStatus};
use crate::game_state::GameStateManager;
use crate::log_watchers::{ChatLogWatcher, LogEvent, LogFileWatcher, PlayerLogWatcher};
use crate::parsers::chat_local_to_utc;
use crate::settings::SettingsManager;
use crate::survey::aggregator::SurveySessionAggregator;
use chrono::{Datelike, Local};
use serde::Serialize;
/// DataIngestCoordinator - Central coordinator for all file watching and database operations
///
/// This coordinator manages:
/// - PlayerLogWatcher (master log - tracks active character and chat log path)
/// - ChatLogWatcher (daily chat logs)
/// - Operation locking to prevent conflicts
/// - Database write coordination
/// - Progress event emission to frontend
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, TryLockError};
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};

/// Timestamped log line for startup diagnostics.
macro_rules! startup_log {
    ($($arg:tt)*) => {
        eprintln!("[{}] {}", Local::now().format("%H:%M:%S%.3f"), format!($($arg)*))
    };
}

/// Status of the coordinator. Watcher activity is tracked separately via the
/// watcher `Option` fields; there is no cross-operation lock (see the
/// pool-is-the-lock invariant in `db/mod.rs`).
#[derive(Debug, Clone, Serialize)]
pub struct CoordinatorStatus {
    pub player_log_active: bool,
    pub chat_log_active: bool,
    pub active_character: Option<String>,
    pub current_chat_log: Option<String>,
}

/// Which VIP report a `ReportSaved` chat line points at, by the game's own
/// filename convention (mirrors `setup_commands.rs` `Character_*.json` and
/// `*_items_*.json` scans). Matched on the file name only — plus a `Reports/`
/// path segment, so a stray same-named file elsewhere never triggers an
/// auto-import (the scans run over the Reports dir listing; this runs on an
/// arbitrary path from a chat line).
enum ReportKind {
    Character,
    Items,
}

fn report_kind_for_path(path: &str) -> Option<ReportKind> {
    // Lexically normalize: split on both separators, drop empties and `.`,
    // resolve `..` by popping. This defeats `Reports/../Downloads/...`
    // traversal that a naive any-segment check would accept.
    let mut segments: Vec<&str> = Vec::new();
    for seg in path.split(['/', '\\']) {
        if seg.is_empty() || seg == "." {
            continue;
        } else if seg == ".." {
            segments.pop();
        } else {
            segments.push(seg);
        }
    }
    let file = segments.last().copied().unwrap_or(path);
    if !file.ends_with(".json") {
        return None;
    }
    // The game always writes VIP reports into <game root>/Reports/. A matching
    // file name without that parent segment is not a VIP report — refuse it
    // rather than importing (and snapshotting) an arbitrary file.
    let parent_is_reports = segments
        .len()
        .checked_sub(2)
        .and_then(|i| segments.get(i))
        .is_some_and(|seg| seg.eq_ignore_ascii_case("Reports"));
    if !parent_is_reports {
        return None;
    }
    if file.starts_with("Character_") {
        Some(ReportKind::Character)
    } else if file
        .strip_suffix(".json")
        .and_then(|stem| stem.split_once("_items_"))
        .is_some_and(|(left, right)| !left.is_empty() && !right.is_empty())
    {
        Some(ReportKind::Items)
    } else {
        None
    }
}


/// A survey gain whose live attribute+tag unit rolled back mid-batch,
/// staged for one bounded retry at batch end (TODO 53). The retry re-runs
/// the whole unit, so the re-bump exactly replaces the rolled-back one and
/// a later pass never double-counts.
struct PendingSurveyTag {
    character: String,
    server: String,
    internal_name: Option<String>,
    item_name: String,
    quantity: u32,
    ts_utc: String,
}

/// Bound on staged survey-tag retries per chat batch: a DB outage must not
/// grow the batch's memory without limit. Overflow gains stay untagged with
/// no qty behind them (the unit rolled back): a later use recorded within
/// the claim window may still reclaim them via retro-claim, otherwise they
/// stay unattributed — the same missing-gain-only state as before the retry
/// existed.
const MAX_SURVEY_TAG_RETRIES: usize = 32;

/// Stage a rolled-back survey gain for the batch-end retry, bounded by
/// [`MAX_SURVEY_TAG_RETRIES`]. Overflow gains stay untagged with no qty
/// behind them (the unit rolled back): a later in-window use may still
/// reclaim them via retro-claim, otherwise they stay unattributed.
fn stage_survey_tag_retry(
    staging: &mut Vec<PendingSurveyTag>,
    character: &str,
    server: &str,
    internal_name: Option<&str>,
    item_name: &str,
    quantity: u32,
    ts_utc: &str,
) {
    if staging.len() < MAX_SURVEY_TAG_RETRIES {
        staging.push(PendingSurveyTag {
            character: character.to_string(),
            server: server.to_string(),
            internal_name: internal_name.map(str::to_string),
            item_name: item_name.to_string(),
            quantity,
            ts_utc: ts_utc.to_string(),
        });
    } else {
        eprintln!(
            "[coordinator] survey retry staging full ({MAX_SURVEY_TAG_RETRIES}); gain left unattributed (an in-window later use may still reclaim it)"
        );
    }
}

/// Central coordinator for all data ingestion
pub struct DataIngestCoordinator {
    player_watcher: Option<PlayerLogWatcher>,
    chat_watcher: Option<ChatLogWatcher>,
    db_pool: DbPool,
    settings: Arc<SettingsManager>,
    app_handle: AppHandle,
    /// Phase 5 survey tracker. Owned by the coordinator so its state survives
    /// screen navigation per the backend-owned-state rule. Fed by chat-gain
    /// attribution on the live event loop, persists per-use rows to
    /// `survey_uses`, and stitches the survey_use_id link into provenance for
    /// downstream item_transactions writes.
    survey_aggregator: SurveySessionAggregator,
    game_state: GameStateManager,
    game_data: GameDataState,
    /// Current area name, updated from AreaTransition events.
    /// Used to attach area context to combat death records.
    current_area: Option<String>,
    /// Rolling buffer of recent damage-on-player events (max 10).
    /// Snapshotted into DB when a death occurs for "what killed me" context.
    recent_damage: Vec<crate::chat_combat_parser::ChatCombatEvent>,
    /// Debug capture state — buffers raw log lines when a capture is active.
    debug_capture: DebugCaptureState,
    /// Casino arena narration tracker — resolves Kuzavek's `[NPC Chatter]`
    /// intro/result lines into completed matches for the Arena widget.
    arena_tracker: crate::arena_parser::ArenaTracker,
    /// `character -> server` scanned from the chat logs at the start of the
    /// catch-up window (`refresh_catch_up_session_servers`); cleared once live.
    catch_up_chat_servers: std::collections::HashMap<String, String>,
    /// Whether `catch_up_chat_servers` was built for this catch-up window.
    /// One scan per window, not one per non-live poll: a poll that fails before
    /// reaching `set_live_mode` would otherwise re-read every `Chat-*.log` on
    /// the 500ms polling thread, holding the coordinator lock each time.
    catch_up_servers_scanned: bool,
    /// Skill updates skipped because the skill name did not resolve against
    /// current game data (new game skill or stale CDN). Monotonic within the
    /// process; logged with each skip so a growing total signals staleness.
    /// The row is deliberately not written (skill_id 0 would collide PKs).
    skipped_unknown_skills: u64,
    /// Suppressed unknown-skill repeats (twin seconds and consecutive genuine
    /// repeats on one identity — indistinguishable at this layer).
    /// Observability only: incremented when the twin slot collapses a
    /// sighting, never re-bumps `skipped_unknown_skills`, writes no ledger
    /// rows. A growing total beside a flat skip total means real repeats are
    /// being hidden by the adjacency contract, not new-skill drift.
    suppressed_unknown_skill_repeats: u64,
    /// Last unresolvable-skill sighting awaiting its twin
    /// (`note_unknown_skill_for` slot, keyed by character + server + skill).
    /// Lives on the coordinator — not in the poll batch — so an English
    /// `LevelUp` at the end of one batch and its JSON `Xp` twin at the start
    /// of the next still collapse to one skip. Identity scoping keeps one
    /// character's half from collapsing another character's same-skill
    /// sighting. A twin consumes the slot, so a later same-skill event
    /// counts again. A poll that drains chat lines without any unknown-skill
    /// sighting expires a stale arming (an unpaired half is dead); empty
    /// polls preserve the slot.
    last_unknown_skill: Option<chat_status_dispatch::UnknownSkillSlot>,
    /// `[JSON:Type]` lines whose body failed to parse. Monotonic within the
    /// process; the raw chat row is still stored, so this only signals format
    /// drift in the game's JSON output.
    skipped_json_malformed: u64,
}

impl DataIngestCoordinator {
    /// Create a new coordinator
    pub fn new(
        db_pool: DbPool,
        settings: Arc<SettingsManager>,
        app_handle: AppHandle,
        game_data: GameDataState,
    ) -> Result<Self, String> {
        // Seed game state manager with persisted character+server so that
        // Player.log events during the initial replay have a valid server key.
        let current_settings = settings.get();
        let game_data_clone = game_data.clone();
        let mut game_state = GameStateManager::new();
        if let (Some(char_name), Some(server_name)) = (
            &current_settings.active_character_name,
            &current_settings.active_server_name,
        ) {
            game_state.set_active_character_name(char_name);
            game_state.set_active_server_name(server_name);
        }

        let survey_aggregator = SurveySessionAggregator::new(game_data_clone.clone());

        Ok(Self {
            player_watcher: None,
            chat_watcher: None,
            db_pool,
            settings,
            app_handle,
            survey_aggregator,
            game_state,
            game_data: game_data_clone,
            current_area: None,
            recent_damage: Vec::new(),
            debug_capture: DebugCaptureState::new(),
            arena_tracker: crate::arena_parser::ArenaTracker::new(),
            catch_up_chat_servers: std::collections::HashMap::new(),
            catch_up_servers_scanned: false,
            skipped_unknown_skills: 0,
            suppressed_unknown_skill_repeats: 0,
            last_unknown_skill: None,
            skipped_json_malformed: 0,
        })
    }

    /// Get current coordinator status
    pub fn get_status(&self) -> CoordinatorStatus {
        CoordinatorStatus {
            player_log_active: self
                .player_watcher
                .as_ref()
                .is_some_and(|w| w.is_active()),
            chat_log_active: self.chat_watcher.as_ref().is_some_and(|w| w.is_active()),
            active_character: self
                .player_watcher
                .as_ref()
                .and_then(|w| w.get_active_character().map(String::from)),
            current_chat_log: self.player_watcher.as_ref().and_then(|w| {
                w.get_chat_log_path()
                    .and_then(|p| p.to_str().map(String::from))
            }),
        }
    }

    /// Active (character, server) pair, if known. Used by feature commands
    /// (notably the survey tracker) that scope all writes by character/server.
    pub fn active_character_server(&self) -> Option<(String, String)> {
        let c = self.game_state.get_active_character()?;
        let s = self.game_state.get_active_server()?;
        Some((c.to_string(), s.to_string()))
    }

    /// Adopt a Player.log session's own character as the ingest identity.
    ///
    /// The Player.log is a sequence of per-character sessions; each
    /// `Logged in as character X` line starts one, and everything after it must
    /// be keyed to X. Only the game state is written — it is the single owner of
    /// the ingest identity, and every log-derived row write reads it. The
    /// durable/display selection in `settings` is not this path's business: the
    /// chat log's own login marker writes it during the replay too (that arm has
    /// no `is_live` gate), and `poll` syncs both fields from the ingest identity
    /// right after the Player.log drain — which is the value the frontend loads
    /// once `poll_watchers` returns.
    ///
    /// The server is not in the Player.log: it is the character's own
    /// `user_characters` row when it has one (picker-active, else newest), else
    /// the character's own chat login line from this catch-up's
    /// `chat_session_servers` scan, else the server in use. See
    /// `apply_catch_up_login`.
    fn adopt_player_log_session(
        &mut self,
        character_name: &str,
        clear_inventory: bool,
    ) -> SessionServerSource {
        let source = apply_catch_up_login(
            &mut self.game_state,
            &self.db_pool,
            &self.catch_up_chat_servers,
            character_name,
            clear_inventory,
        );
        startup_log!(
            "Player.log session identity: {} on {} (server from {})",
            character_name,
            self.game_state
                .get_active_server()
                .unwrap_or("(unknown server)"),
            source.label()
        );
        source
    }

    /// One chat-login scan per catch-up window, before the Player.log drain.
    ///
    /// Guarded by `catch_up_servers_scanned` rather than by `is_live()`, so a
    /// poll that fails before `set_live_mode` — the only thing that ends the
    /// window — re-drains the log without re-reading every `Chat-*.log`.
    fn refresh_catch_up_session_servers(&mut self) {
        if self.catch_up_servers_scanned {
            return;
        }
        self.catch_up_servers_scanned = true;
        let dir = self.settings.get_chat_logs_dir().or_else(|| {
            self.player_watcher
                .as_ref()
                .and_then(|w| w.get_chat_log_path())
                .and_then(|p| p.parent().map(std::path::PathBuf::from))
        });
        let Some(dir) = dir else {
            self.catch_up_chat_servers.clear();
            startup_log!("Catch-up chat-login scan skipped: no chat log directory");
            return;
        };
        self.catch_up_chat_servers = chat_session_servers(&dir);
        startup_log!(
            "Catch-up chat-login scan: {} character(s) with a server in {}",
            self.catch_up_chat_servers.len(),
            dir.display()
        );
    }


    /// Read access to the DB pool. Feature commands need this to issue
    /// queries without going through the GameStateManager.
    pub fn db_pool(&self) -> &DbPool {
        &self.db_pool
    }

    /// Mutable access to the survey aggregator. Lets command handlers issue
    /// session start/end and other lifecycle operations without exposing the
    /// full coordinator surface.
    pub fn survey_aggregator_mut(&mut self) -> &mut SurveySessionAggregator {
        &mut self.survey_aggregator
    }

    /// Start player log tailing
    pub fn start_player_log_tailing(&mut self) -> Result<(), String> {

        // Already tailing? No-op.
        if self
            .player_watcher
            .as_ref()
            .is_some_and(|w| w.is_active())
        {
            return Ok(());
        }

        // Get player log path from settings
        let player_log_path = self
            .settings
            .get_player_log_path()
            .ok_or_else(|| "Game data path not configured".to_string())?;

        // Load saved position (and last-known character) from database
        let conn = self
            .db_pool
            .get()
            .map_err(|e| format!("Database error: {}", e))?;
        let (mut position, saved_character) =
            log_positions::get_position_with_player(&conn, player_log_path.to_str().unwrap_or(""))
                .unwrap_or((0, None));

        // Detect file rotation: if saved position is past current file size,
        // the game restarted and created a fresh Player.log
        if position > 0
            && let Ok(meta) = std::fs::metadata(&player_log_path)
                && meta.len() < position {
                    startup_log!("Player.log was rotated (size {} < saved position {}), starting from beginning",
                        meta.len(), position);
                    // Persist the reset NOW: update_position's monotonic MAX
                    // guard would keep the stale larger cursor forever, so
                    // every subsequent launch would re-detect the rotation,
                    // re-parse Player.log from 0, and re-insert non-idempotent
                    // catch-up rows (corpse_extracts, item_transactions)
                    // until the new file outgrows the stale offset. The
                    // watcher built below is fresh (position_reset = false),
                    // so no later save cycle can force this cursor down —
                    // this is the only site that can persist the reset.
                    let wconn = self
                        .db_pool
                        .get_write()
                        .map_err(|e| format!("Database error: {}", e))?;
                    log_positions::force_position(
                        &wconn,
                        player_log_path.to_str().unwrap_or(""),
                        "player",
                        0,
                        saved_character.as_deref(),
                        None,
                    )
                    .map_err(|e| format!("Failed to force-save rotated position: {}", e))?;
                    position = 0;
                }

        // Re-seed game state from settings — the setup wizard may have set
        // the active character+server after the coordinator was created.
        let current_settings = self.settings.get();
        if let (Some(char_name), Some(server_name)) = (
            &current_settings.active_character_name,
            &current_settings.active_server_name,
        ) {
            startup_log!(
                "Seeding game state from settings: {} on {}",
                char_name,
                server_name
            );
            self.game_state.set_active_character_name(char_name);
            self.game_state.set_active_server_name(server_name);
        }

        drop(conn);

        // Create watcher
        let mut watcher = if position > 0 {
            startup_log!(
                "Starting Player.log catch-up from byte position {}",
                position
            );
            let mut w = PlayerLogWatcher::from_position(player_log_path, position);

            // Seed identity from the saved position so we know who's playing
            // even if no new login line appears in the resumed log content.
            if let Some(ref name) = saved_character {
                startup_log!("Restoring active character from saved position: {}", name);
                w.set_active_character(name.clone());
            }
            w
        } else {
            startup_log!("Starting Player.log from beginning (no saved position)");
            PlayerLogWatcher::new(player_log_path)
        };

        // Start watching
        watcher.start()?;
        startup_log!("Player.log watcher started");

        // Store watcher
        self.player_watcher = Some(watcher);

        // Emit status change event
        self.emit_status_change()?;

        Ok(())
    }



    /// Stop player log tailing
    pub fn stop_player_log_tailing(&mut self) -> Result<(), String> {
        if let Some(mut watcher) = self.player_watcher.take() {
            watcher.stop();

            // Save position to database
            let position = watcher.get_position();
            if let Some(path) = self.settings.get_player_log_path() {
                let conn = self
                    .db_pool
                    .get_write()
                    .map_err(|e| format!("Database error: {}", e))?;
                if watcher.position_was_reset() {
                    // Rotation reset pending (stop raced the next save
                    // cycle): force the reset value — the monotonic update
                    // would keep the stale larger cursor. The watcher is
                    // dropped below, so no flag clearing is needed.
                    log_positions::force_position(
                        &conn,
                        path.to_str().unwrap_or(""),
                        "player",
                        position,
                        watcher.get_active_character(),
                        None,
                    )
                    .map_err(|e| format!("Failed to force-save position: {}", e))?;
                } else {
                    log_positions::update_position(
                        &conn,
                        path.to_str().unwrap_or(""),
                        "player",
                        position,
                        watcher.get_active_character(),
                        None,
                    )
                    .map_err(|e| format!("Failed to save position: {}", e))?;
                }
            }
        }

        // Emit status change event
        self.emit_status_change()?;

        Ok(())
    }

    /// Start chat log tailing
    pub fn start_chat_log_tailing(&mut self, chat_log_path: PathBuf) -> Result<(), String> {

        // Already tailing the same file? No-op.
        let should_switch = match &self.chat_watcher {
            Some(existing)
                if existing.is_active() && existing.get_file_path() == &chat_log_path =>
            {
                return Ok(());
            }
            Some(_) => true,
            None => false,
        };

        if should_switch {
            // Different file — do a final flush of any unread content
            // before switching, so we don't lose messages written between
            // the last poll and this switch. The events go through the
            // shared drain (raw insert + structured dispatch): the old
            // raw-only flush saved the bytes but never created the
            // `player_messages` rows for structured lines in the tail.
            // End the watcher borrow before dispatching — the drain needs
            // `&mut self` for its trackers and follow-ups.
            let flushed: Vec<LogEvent> = if let Some(watcher) = &mut self.chat_watcher {
                startup_log!(
                    "Chat log switching from {:?} → {:?}, flushing old watcher",
                    watcher.get_file_path(),
                    chat_log_path
                );
                watcher.poll().unwrap_or_default()
            } else {
                Vec::new()
            };
            if !flushed.is_empty() {
                startup_log!("Final flush: {} events from old chat log", flushed.len());
                self.process_chat_events(flushed)?;
            }
            self.stop_chat_log_tailing()?;
        }

        // Load saved position from database
        let conn = self
            .db_pool
            .get()
            .map_err(|e| format!("Database error: {}", e))?;
        let position =
            log_positions::get_position(&conn, chat_log_path.to_str().unwrap_or("")).unwrap_or(0);
        drop(conn);

        // Create watcher — parses all channels; filtering happens at persistence layer
        let mut watcher = if position > 0 {
            startup_log!("Starting chat log catch-up from byte position {}", position);
            ChatLogWatcher::from_position(chat_log_path, position)
        } else {
            startup_log!("Starting chat log from beginning");
            ChatLogWatcher::new(chat_log_path)
        };

        // Set player name if known
        if let Some(pw) = &self.player_watcher
            && let Some(name) = pw.get_active_character() {
                watcher.set_player_name(name.to_string());
            }

        // Start watching
        watcher.start()?;

        // Store watcher
        self.chat_watcher = Some(watcher);

        // Emit status change event
        self.emit_status_change()?;

        Ok(())
    }

    /// Stop chat log tailing
    pub fn stop_chat_log_tailing(&mut self) -> Result<(), String> {
        if let Some(mut watcher) = self.chat_watcher.take() {
            watcher.stop();

            // Save position to database using the actual file path
            let position = watcher.get_position();
            let file_path_str = watcher.get_file_path().to_string_lossy().to_string();
            let file_name = watcher.get_file_name().to_string();

            let conn = self
                .db_pool
                .get_write()
                .map_err(|e| format!("Database error: {}", e))?;
            let metadata = serde_json::json!({ "file_name": file_name }).to_string();
            if watcher.position_was_reset() {
                // Rotation reset pending (stop raced the next save cycle):
                // the monotonic update would keep the stale larger cursor —
                // force the reset value instead. The watcher is dropped
                // below, so no flag clearing is needed.
                log_positions::force_position(
                    &conn,
                    &file_path_str,
                    "chat",
                    position,
                    None,
                    Some(&metadata),
                )
                .map_err(|e| format!("Failed to force-save position: {}", e))?;
            } else {
                log_positions::update_position(
                    &conn,
                    &file_path_str,
                    "chat",
                    position,
                    None,
                    Some(&metadata),
                )
                .map_err(|e| format!("Failed to save position: {}", e))?;
            }
        }

        // Emit status change event
        self.emit_status_change()?;

        Ok(())
    }

    /// Poll all active watchers and process events
    pub fn poll(&mut self) -> Result<(), String> {
        // One chat-login scan per catch-up window: the Player.log names no
        // server, and a character with no `user_characters` row has no other
        // log-derived source. Built before the drain so every login line sees it.
        if !self.game_state.is_live() {
            self.refresh_catch_up_session_servers();
        }

        // Poll player log watcher
        if let Some(watcher) = &mut self.player_watcher {
            let events = watcher.poll()?;
            self.process_player_events(events)?;
        }

        // After the first poll cycle, switch game state to live mode so future
        // logins will properly clear transient state. During the initial catch-up
        // replay we skip clearing to preserve data built for each character.
        if !self.game_state.is_live() {
            // No login line was seen at all (the log was resumed mid-session
            // from a saved position): the watcher's restored character is then
            // the only identity source. Without this, the chat drain drops
            // every live event until the chat marker fires.
            if self.game_state.get_active_character().is_none()
                && let Some(name) = self
                    .player_watcher
                    .as_ref()
                    .and_then(|w| w.get_active_character())
                    .map(str::to_string)
                {
                    startup_log!(
                        "Player.log catch-up saw no login line; re-arming from the log's saved position: '{}'",
                        name
                    );
                    self.adopt_player_log_session(&name, false);
                }

            // The log's newest session is who is playing now: make the durable
            // selection agree with the ingest identity — character *and* server,
            // because every query and every row is keyed by both, and the
            // frontend re-reads settings right after this poll. This is the
            // Player.log drain's identity, not the poll's: the chat drain below
            // runs after it and its own login marker may then re-set both fields
            // (it has no `is_live` gate), and the frontend reads the result of
            // the whole poll.
            if let Some((name, server)) = self.active_character_server() {
                let mut settings = self.settings.get();
                if settings.active_character_name.as_deref() != Some(name.as_str())
                    || settings.active_server_name.as_deref() != Some(server.as_str())
                {
                    startup_log!("Catch-up identity: {} on {}", name, server);
                    settings.active_character_name = Some(name);
                    settings.active_server_name = Some(server);
                    self.settings.update(settings).ok();
                }
            } else {
                startup_log!(
                    "Player.log catch-up finished with no ingest identity — live events will be dropped"
                );
            }
        }

        // Poll chat log watcher
        if let Some(watcher) = &mut self.chat_watcher {
            let events = watcher.poll()?;
            self.process_chat_events(events)?;
        }

        // Live mode turns on only after the whole catch-up — the Player.log drain
        // *and* the chat drain — has been replayed: the chat login marker's
        // `set_active_character` clears transient state when live, and that must
        // not happen to rows the replay just built. Re-evaluated every poll, so a
        // failed poll simply retries (without re-running the chat-login scan —
        // see `catch_up_servers_scanned`).
        if !self.game_state.is_live() {
            startup_log!("Catch-up complete — switching to live tailing mode");
            self.game_state.set_live_mode();
            self.catch_up_chat_servers.clear();
            self.catch_up_servers_scanned = false;
        }

        // Drain raw lines from watchers into the debug capture temp file
        if self.debug_capture.is_active() {
            self.drain_capture_lines();
        }

        // Persist watcher positions every poll cycle so a crash doesn't
        // lose all progress and cause a full re-parse on next launch.
        self.save_watcher_positions_mut();

        Ok(())
    }

    /// Persist current watcher byte offsets to the database.
    /// Called every poll cycle so a crash only loses ~1 polling interval of progress.
    /// `&mut self` because a detected truncation/rotation must also force the
    /// stored cursor down (and clear the watcher's reset flag).
    fn save_watcher_positions_mut(&mut self) {
        let conn = match self.db_pool.get_write() {
            Ok(c) => c,
            Err(_) => return,
        };

        if let Some(watcher) = &mut self.player_watcher
            && let Some(path) = self.settings.get_player_log_path() {
                log_positions::update_position(
                    &conn,
                    path.to_str().unwrap_or(""),
                    "player",
                    watcher.get_position(),
                    watcher.get_active_character(),
                    None,
                )
                .ok();
                if watcher.position_was_reset() {
                    // Truncation/rotation this cycle: the monotonic MAX guard
                    // would keep the stale larger offset — force the cursor
                    // down. Clear the flag ONLY after the force succeeds:
                    // a discarded failure would leave the stale cursor in
                    // place with no later poll retrying the reset.
                    if log_positions::force_position(
                        &conn,
                        path.to_str().unwrap_or(""),
                        "player",
                        watcher.get_position(),
                        watcher.get_active_character(),
                        None,
                    )
                    .is_ok()
                    {
                        watcher.clear_position_reset();
                    } else {
                        eprintln!(
                            "[coordinator] Failed to persist rotation reset for Player.log; will retry next cycle"
                        );
                    }
                }
            }

        if let Some(watcher) = &mut self.chat_watcher {
            let file_path_str = watcher.get_file_path().to_string_lossy().to_string();
            let file_name = watcher.get_file_name().to_string();
            let metadata = serde_json::json!({ "file_name": file_name }).to_string();
            log_positions::update_position(
                &conn,
                &file_path_str,
                "chat",
                watcher.get_position(),
                None,
                Some(&metadata),
            )
            .ok();
            if watcher.position_was_reset() {
                if log_positions::force_position(
                    &conn,
                    &file_path_str,
                    "chat",
                    watcher.get_position(),
                    None,
                    Some(&metadata),
                )
                .is_ok()
                {
                    watcher.clear_position_reset();
                } else {
                    eprintln!(
                        "[coordinator] Failed to persist rotation reset for {}; will retry next cycle",
                        file_name
                    );
                }
            }
        }
    }

    /// Process session signals from Player.log (login lines, chat-path
    /// announcements, area loads). The `LocalPlayer: Process*` verbose lines
    /// are gone, so this drain carries identity and area only — no game data.
    /// Domains touched here (`area`) batch into one `game-state-updated`
    /// emit; identity changes stay the chat login marker's job.
    fn process_player_events(&mut self, events: Vec<LogEvent>) -> Result<(), String> {
        let mut domains_batch: Vec<&'static str> = Vec::new();

        for event in events {
            match event {
                LogEvent::CharacterLogin { character_name, .. } => {

                    startup_log!("Character detected from Player.log: {}", character_name);

                    // Live identity changes are the chat log's job: its
                    // `******** Logged In As <name>. Server <server>` marker also
                    // knows the server, and it is what the frontend's reload path
                    // expects (see `process_chat_events`) — so don't emit a second
                    // `character-login` per login here.
                    //
                    // During catch-up this line *attributes* the session that
                    // follows it: the Player.log is a sequence of per-character
                    // sessions, so its own login line says whose rows everything
                    // after it produces. Lives below.
                    if self.game_state.is_live() {
                        // A live login line re-arms an identity that was never
                        // established (fresh settings, no Player.log login line,
                        // no chat marker): `active_character = None` makes the
                        // chat drain drop *every* event, and the chat
                        // log's marker is the only other re-arm — it can be
                        // missed (the chat watcher tails from a saved position)
                        // or never start. Nothing clears the identity after
                        // `GameStateManager::new` any more (the catch-up guard's
                        // `clear_active_character` is gone), so this is the only
                        // remaining way that window closes. Do the same settings
                        // + game-state update the marker does, minus the
                        // `character-login` emit (live identity events stay the
                        // marker's job).
                        if self.game_state.get_active_character().is_none() {
                            startup_log!(
                                "Live login line re-armed the unset active character: '{}'",
                                character_name
                            );
                            let mut settings = self.settings.get();
                            settings.active_character_name = Some(character_name.clone());
                            // Player.log login lines carry no server: a stale
                            // ambient server (previous session, shared PC)
                            // would key every tagless line to the wrong
                            // server until the chat marker fires. Fail closed
                            // (lines skip dispatch without identity) rather
                            // than persist a guess.
                            settings.active_server_name = None;
                            self.settings.update(settings).ok();
                            self.game_state.set_active_character_name(&character_name);
                            self.game_state.clear_active_server_name();
                        }
                        continue;
                    }

                    // Catch-up: the Player.log is a sequence of per-character
                    // sessions and every login line starts one. Adopt that
                    // session's own character so subsequent lines attribute
                    // to the character that produced them instead of being
                    // dropped. Dropping was only needed while the ingest
                    // identity had two owners: a switch that moved the game
                    // state but not `settings` split-brained the DB. The
                    // identity now has one owner (`GameStateManager`), and the
                    // durable selection is synced to it when the replay ends —
                    // see `poll`.
                    self.adopt_player_log_session(&character_name, true);
                    // No `character-login` emit: live identity changes (and the
                    // frontend reload they trigger) are the chat log's job, and
                    // the startup sequence loads the character explicitly once
                    // the poll completes — including one adopted here.
                }
                LogEvent::ChatLogPath { path, .. } => {
                    startup_log!("Chat log path detected: {}", path);
                    // Chat log path changed - start/switch chat watcher
                    // start_chat_log_tailing handles stopping the old watcher if needed
                    self.start_chat_log_tailing(PathBuf::from(&path))?;
                }
                LogEvent::AreaTransition { area, .. } => {
                    self.current_area = Some(area.clone());
                    // Persist area to game state
                    if let (Some(character), Some(server)) = (
                        self.game_state.get_active_character(),
                        self.game_state.get_active_server(),
                    )
                        && let Ok(conn) = self.db_pool.get_write() {
                            conn.execute(
                                "INSERT INTO game_state_area (character_name, server_name, area_name, last_confirmed_at)
                                 VALUES (?1, ?2, ?3, datetime('now'))
                                 ON CONFLICT(character_name, server_name) DO UPDATE SET
                                    area_name = excluded.area_name,
                                    last_confirmed_at = excluded.last_confirmed_at",
                                rusqlite::params![character, server, area],
                            ).ok();
                            domains_batch.push("area");
                        }
                    self.app_handle.emit("area-transition", &area).ok();
                }
                _ => {
                    // Other events not yet implemented
                }
            }
        }

        if !domains_batch.is_empty() {
            domains_batch.sort_unstable();
            domains_batch.dedup();
            self.app_handle.emit("game-state-updated", &domains_batch).ok();
        }

        Ok(())
    }

    /// Process events from chat log.
    ///
    /// Chat messages are already batched (bulk DB insert + single count emit).
    /// The per-event emits here (chat-status-event, server-detected, etc.) are
    /// low-volume and don't need the same batching treatment as player events.
    fn process_chat_events(&mut self, events: Vec<LogEvent>) -> Result<(), String> {
        let mut messages = Vec::new();

        // Pre-batch identity for leading untagged lines: the watcher emits
        // file-ordered, so a line ahead of the first marker resolves against
        // the session in force when the batch arrived — not the one the
        // batch switches to. Gated on `identity_mutated`: past the first
        // marker, lines carry their own session tag — except trailing
        // post-logout lines, which must see the cleared live pair (drop)
        // rather than the pre-batch session. Batches without markers never
        // mutate mid-batch, so this equals the live fallback below and
        // resume-from-offset behavior is unchanged.
        let preceding_identity: Option<DispatchIdentity> =
            match (
                self.game_state.get_active_character(),
                self.game_state.get_active_server(),
            ) {
                (Some(c), Some(s)) => Some(DispatchIdentity {
                    character: c.to_string(),
                    server: s.to_string(),
                }),
                _ => None,
            };
        // Whether a session signal (login, server, logout) already moved or
        // cleared the active pair mid-batch. Leading lines consult the
        // pre-batch pair; everything after the first marker resolves live.
        let mut identity_mutated = false;

        // Get timezone offset for converting Chat.log local timestamps to UTC
        let tz_offset = self
            .settings
            .get()
            .manual_timezone_override
            .or(self.settings.get().timezone_offset_seconds)
            .unwrap_or(0);
        // Twin-collapse slot for unresolvable-skill sightings (English
        // `LevelUp` + JSON `Xp` arrive as consecutive messages). Owned by the
        // coordinator across polls: a twin split over a batch boundary still
        // collapses, since the slot carries over. Taken locally for the drain
        // (the loop also borrows other `self` fields) and stored back below.
        // The slot is keyed by character + server + skill, so one
        // character's half never collapses another character's same-skill
        // sighting. An unpaired sighting must not arm the slot forever (a
        // lone English half whose JSON twin malformed, or a half dropped by
        // a session marker, would swallow a genuine lone sighting N polls
        // later), so the drain reports whether it saw any unknown skill: a
        // poll that drained chat lines but saw none expires the slot. Empty
        // polls (zero chat lines) preserve it — a twin split across idle
        // polls is still consecutive in log order.
        let mut last_unknown_skill: Option<chat_status_dispatch::UnknownSkillSlot> =
            std::mem::take(&mut self.last_unknown_skill);
        let mut saw_unknown_skill = false;
        // Survey tag-failure staging (TODO 53): gains whose live
        // attribute+tag unit rolls back mid-batch wait here for one bounded
        // retry at batch end (below, after the event loop). Single pass —
        // a retry that still fails leaves the gain unattributed (an
        // in-window later use may still reclaim it via retro-claim).
        let mut survey_tag_retries: Vec<PendingSurveyTag> = Vec::new();

        // The watcher's file name scopes per-line byte offsets to one log
        // generation (chat logs are dated per-day files): the ledger guards'
        // `file:offset` identity below. Reused by the tail batch insert.
        let log_file = self
            .chat_watcher
            .as_ref()
            .map(|w| w.get_file_name().to_string())
            .unwrap_or_else(|| "unknown".to_string());
        for event in events {
            match event {
                LogEvent::ChatMessage(msg, session_tag, offset) => {
                    // Capture the local wall-clock timestamp BEFORE the UTC
                    // conversion below — Combat Wisdom / arena / poop dedup
                    // keys use local time to stay consistent with the chat-log
                    // backfill (which never converts), so live + backfill
                    // dedup correctly.
                    let local_ts = msg.timestamp.format("%Y-%m-%d %H:%M:%S").to_string();

                    // Convert chat timestamp from local time to UTC
                    let mut msg = msg;
                    msg.timestamp = chat_local_to_utc(msg.timestamp, tz_offset);

                    // Per-occurrence line identity for the ledger guards:
                    // `file:offset` is stable across rescans (byte offsets
                    // never move) and distinct per line even when two lines
                    // are byte-identical (they share one raw row under
                    // INSERT OR IGNORE, so a rowid could not separate them).
                    let source_line_id: Option<String> =
                        Some(format!("{log_file}:{offset}"));

                    // Catch-up vs live identity: the tag is the login session
                    // that produced the line; only when the file was resumed
                    // mid-session (`None`) do we fall back to the drain
                    // identity. Live resolves the same way: a poll batch can
                    // span a login boundary (the watcher emits file-ordered),
                    // so a pre-login line must keep its own tag instead of
                    // inheriting the new active identity. Tag-less live lines
                    // (resume-from-offset polls) use the active identity.
                    let is_live = self.game_state.is_live();
                    let identity: Option<DispatchIdentity> = {
                        // Leading lines of a boundary-spanning batch carry no
                        // tag for the session that produced them; resolve
                        // against the pre-batch pair first — but only ahead
                        // of the first marker. Marker-free batches never
                        // moved the active pair, so this is the same value
                        // the live fallback below would read.
                        let live_identity = match (
                            self.game_state.get_active_character(),
                            self.game_state.get_active_server(),
                        ) {
                            (Some(c), Some(s)) => Some(DispatchIdentity {
                                character: c.to_string(),
                                server: s.to_string(),
                            }),
                            _ => None,
                        };
                        resolve_chat_drain_identity(
                            session_tag.as_ref(),
                            identity_mutated,
                            preceding_identity.as_ref(),
                            live_identity,
                        )
                    };

                    // The shared dispatch owns every idempotent DB arm. The
                    // live-only follow-up stays here because it needs
                    // `&mut self` borrows the dispatch does not take:
                    // survey attribution through the live aggregator's
                    // gating state.
                    let mut outcome = {
                        let mut dstate = DispatchState {
                            arena_tracker: &mut self.arena_tracker,
                            recent_damage: &mut self.recent_damage,
                            current_area: self.current_area.clone(),
                            // Live drain owns the accumulator: each line is
                            // seen once here, so accrual is exactly-once.
                            count_currency: true,
                        };
                        // Live feeds the survey hook so loot rows stamp
                        // `survey_chat` immediately; catch-up/scan rows stay
                        // untagged for `claim_unlinked_chat_loot_near`.
                        // The hook needs `&mut self.survey_aggregator` while
                        // the dispatch also runs — so the dispatch takes no
                        // hook, and attribution runs here per staged gain
                        // after the dispatch returns (same timestamps the
                        // dispatch just wrote).
                        chat_status_dispatch::dispatch_chat_message(
                            chat_status_dispatch::DispatchCtx {
                                db: &self.db_pool,
                                game_data: &self.game_data,
                                identity: identity.as_ref(),
                                msg: &msg,
                                local_ts: &local_ts,
                                watch_rules: &self.settings.get().watch_rules,
                                // Per-occurrence `file:offset` identity from
                                // above: genuine same-second duplicates
                                // insert, replays collapse.
                                source_line_id,
                                is_scan: false,
                                is_live,
                            },
                            None,
                            &mut dstate,
                        )
                    };

                    // Correlation feed + survey attribution per gain.
                    // (Inventory stack correction is gone: snapshot imports
                    // carry authoritative quantities, so there are no
                    // Player.log-era placeholders left to patch.)
                    for (item_name, quantity) in std::mem::take(&mut outcome.staged_gains) {
                        let (internal_name, item_type_id): (Option<String>, Option<i64>) =
                            self.game_data
                                .try_read()
                                .ok()
                                .and_then(|gd| {
                                    gd.resolve_item(&item_name).map(|info| {
                                        (info.internal_name.clone(), Some(info.id as i64))
                                    })
                                })
                                .unwrap_or((None, None));
                        let ts_utc = msg.timestamp.format("%Y-%m-%d %H:%M:%S").to_string();
                        // (3) Survey, live loot gains only: attribute the
                        // just-written ledger row, then tag it — mirrors the
                        // old inline arm. Summoned gains never attribute.
                        // One atomic attribute+tag unit (see
                        // `attribute_and_tag_chat_gain`): if the tag fails,
                        // the bump rolls back with it, leaving the row
                        // untagged with no qty behind it. A failure no longer
                        // just drops the gain — the inputs stage for one
                        // bounded retry at batch end (below), which re-runs
                        // the whole unit so the re-bump exactly replaces the
                        // rolled-back one.
                        if is_live {
                            let is_loot = matches!(
                                crate::chat_status_parser::parse_status_message(&msg),
                                Some(ChatStatusEvent::ItemGained { .. })
                            ) || msg.json_type.as_deref() == Some("AddItem");
                            // Identity-less gains were already skipped by the
                            // dispatch above; a pool-checkout failure stages
                            // too — the retry below checks out fresh.
                            if is_loot && let Some(id) = identity.as_ref() {
                                match self.db_pool.get_write() {
                                    Ok(conn) => match self
                                        .survey_aggregator
                                        .attribute_and_tag_chat_gain(
                                            &conn,
                                            &id.character,
                                            &id.server,
                                            internal_name.as_deref(),
                                            quantity,
                                            &ts_utc,
                                            Some(item_name.as_str()),
                                        ) {
                                        Ok(_) => {}
                                        // A failed unit must not abort the
                                        // whole batch via `?`: the rollback
                                        // already undid the bump, so stage
                                        // the inputs for the batch-end retry
                                        // and move on so the remaining gains
                                        // still process.
                                        Err(e) => {
                                            eprintln!(
                                                "[coordinator] survey attr/tag failed ({e}); staged for batch-end retry"
                                            );
                                            stage_survey_tag_retry(
                                                &mut survey_tag_retries,
                                                &id.character,
                                                &id.server,
                                                internal_name.as_deref(),
                                                &item_name,
                                                quantity,
                                                &ts_utc,
                                            );
                                        }
                                    },
                                    Err(e) => {
                                        eprintln!(
                                            "[coordinator] survey attr skipped, pool checkout failed ({e}); staged for batch-end retry"
                                        );
                                        stage_survey_tag_retry(
                                            &mut survey_tag_retries,
                                            &id.character,
                                            &id.server,
                                            internal_name.as_deref(),
                                            &item_name,
                                            quantity,
                                            &ts_utc,
                                        );
                                    }
                                }
                            }
                        }
                        let _ = item_type_id;
                    }

                    // Forward structured emits + touched domains.
                    if let Some(ev) = outcome.chat_status_event.take() {
                        self.app_handle.emit("chat-status-event", &ev).ok();
                    }
                    if let Some(ev) = outcome.character_death.take() {
                        self.app_handle.emit("character-death", &ev).ok();
                    }
                    if let Some(ev) = outcome.enemy_killed.take() {
                        self.app_handle.emit("enemy-killed", &ev).ok();
                    }
                    if let Some(ev) = outcome.character_resuscitated.take() {
                        self.app_handle.emit("character-resuscitated", &ev).ok();
                    }
                    for ev in std::mem::take(&mut outcome.watch_triggered) {
                        self.app_handle.emit("watch-rule-triggered", &ev).ok();
                    }
                    for path in std::mem::take(&mut outcome.report_saved_paths) {
                        // Live import for the just-written report (same as
                        // the old inline arm); failures just log.
                        self.import_report_saved(&path);
                    }
                    // Twin-collapse: an English `LevelUp` and its JSON `Xp`
                    // twin arrive as consecutive messages for one event.
                    // `last_unknown_skill` is the coordinator-owned slot taken
                    // above, so twins split across poll batches still collapse.
                    // The slot is identity-scoped (character + server + skill):
                    // a same-skill sighting from another identity counts as a
                    // new skip instead of collapsing. Any sighting (counted
                    // or collapsed) marks the drain, so the writeback can
                    // expire a stale arming below.
                    for skill in std::mem::take(&mut outcome.unknown_skills) {
                        saw_unknown_skill = true;
                        if crate::chat_status_dispatch::note_unknown_skill_for(
                            &mut last_unknown_skill,
                            identity.as_ref(),
                            skill.clone(),
                        ) {
                            self.skipped_unknown_skills += 1;
                            eprintln!(
                                "[coordinator] Skipping chat level-up for unresolvable skill: {skill}"
                            );
                        } else {
                            // Observability only: a twin second (or a
                            // consecutive genuine repeat on the same identity
                            // — indistinguishable here). Never re-bumps
                            // `skipped_unknown_skills`, writes no ledger rows.
                            self.suppressed_unknown_skill_repeats += 1;
                            eprintln!(
                                "[coordinator] Suppressed repeat for unresolvable skill (twin or genuine repeat, no recount): {skill}"
                            );
                        }
                    }

                    if !outcome.malformed_json.is_empty() {
                        self.skipped_json_malformed += outcome.malformed_json.len() as u64;
                        for t in &outcome.malformed_json {
                            eprintln!(
                                "[coordinator] Skipping malformed JSON chat line of type: {t}"
                            );
                        }
                    }

                    for t in std::mem::take(&mut outcome.unknown_json) {
                        eprintln!("[coordinator] Unobserved JSON chat line of type: {t} (raw row kept)");
                    }

                    // VIP JSON books: the upsert already ran in shared dispatch
                    // (live + scan + replay); the content triggers below mirror
                    // the live `BookOpened` arm verbatim. They run on live AND
                    // catch-up, attributed to the line's own session identity
                    // (live identity IS the active pair, so live behavior is
                    // unchanged). Dispatch only populates `json_book` when the
                    // line had identity, so `id` is always `Some` here.
                    // JSON bodies are serde_json-decoded, so newlines are
                    // already real and the `normalize_book_content` step is skipped.
                    if let Some((book_type, title, body, at)) = outcome.json_book.take()
                        && let Some(id) = identity.as_ref()
                    {
                        let character = id.character.as_str();
                        let server = id.server.as_str();
                        if book_type == "SkillReport"
                            && body.trim_start().starts_with("Foods Consumed:")
                            && let Ok(conn) = self.db_pool.get_write()
                        {
                            match crate::db::gourmand_commands::import_gourmand_from_content(
                                &conn, &body,
                            ) {
                                Ok(n) if n > 0 => {
                                    self.app_handle.emit("gourmand-updated", n).ok();
                                }
                                Ok(_) => {}
                                Err(e) => {
                                    eprintln!(
                                        "[coordinator] Failed to auto-import gourmand report from JSON book: {e}"
                                    );
                                }
                            }
                        }
                        if book_type == "SkillReport"
                            && body.trim_start().starts_with("Equipment Studied:")
                        {
                            self.ingest_hoplology_report(&at, &body, character, server);
                        }
                        if book_type == "SkillReport"
                            && body.contains("Primary Bind Location:")
                        {
                            self.ingest_teleportation_binds(&at, &body, character, server);
                        }
                        if title == "Gardening Almanac" {
                            // Title-only is the whole gate: `json_book` is
                            // only populated by the Book arm, and the title
                            // classifier leaves this title in the "JSON"
                            // bucket (never "SkillReport").
                            if let Ok(conn) = self.db_pool.get_write() {
                                match crate::db::game_state_commands::import_garden_almanac(
                                    &conn, character, server, &body, &at,
                                ) {
                                    Ok(n) if n > 0 => outcome.domains.push("garden_almanac".into()),
                                    Ok(_) => {}
                                    Err(e) => {
                                        eprintln!(
                                            "[coordinator] Failed to import gardening seasons from JSON book: {e}"
                                        );
                                    }
                                }
                            }
                        }
                        if title == "Emotion-Harvesting Almanac" {
                            // Title-only is the whole gate: `json_book` is
                            // only populated by the Book arm, and the title
                            // classifier leaves this title in the "JSON"
                            // bucket (never "SkillReport").
                            if let Ok(conn) = self.db_pool.get_write() {
                                match crate::db::game_state_commands::import_harvest_almanac(
                                    &conn, character, server, &body, &at,
                                ) {
                                    Ok(n) if n > 0 => outcome.domains.push("harvest_almanac".into()),
                                    Ok(_) => {}
                                    Err(e) => {
                                        eprintln!(
                                            "[coordinator] Failed to import harvesting focuses from JSON book: {e}"
                                        );
                                    }
                                }
                            }
                        }
                        if book_type == "PlayerAge" || book_type == "HelpScreen" {
                            self.ingest_report_stats(&at, &book_type, &body, character, server);
                        }
                    }
                    if !outcome.domains.is_empty() {
                        self.app_handle.emit("game-state-updated", &outcome.domains).ok();
                    }

                    messages.push(msg);
                }
                LogEvent::ServerDetected {
                    server_name,
                    character_name,
                    timezone_offset_seconds,
                } => {
                    startup_log!(
                        "Server detected: {} (character: {})",
                        server_name,
                        character_name
                    );

                    // Store timezone offset for chat timestamp conversion
                    if let Some(offset) = timezone_offset_seconds {
                        startup_log!("Timezone offset detected: {}s from UTC", offset);
                        let mut settings = self.settings.get();
                        settings.timezone_offset_seconds = Some(offset);
                        self.settings.update(settings).ok();
                    }

                    // Auto-create server record
                    if let Ok(conn) = self.db_pool.get_write() {
                        conn.execute(
                            "INSERT INTO servers (server_name) VALUES (?1) ON CONFLICT DO NOTHING",
                            rusqlite::params![server_name],
                        )
                        .ok();
                    }

                    // Update active server in settings
                    let mut settings = self.settings.get();
                    settings.active_server_name = Some(server_name.clone());
                    self.settings.update(settings).ok();

                    // Update game state so the chat drain attributes to this server
                    self.game_state.set_active_server_name(&server_name);
                    // A session signal moved the active pair: later untagged
                    // lines resolve live, never against the pre-batch pair.
                    identity_mutated = true;

                    // Emit to frontend
                    self.app_handle.emit("server-detected", &server_name).ok();
                }
                LogEvent::CharacterLogin { character_name, server_name, timestamp, .. } => {
                    // Chat log also detects character login — update active character
                    // and emit so player_watcher can stay in sync
                    self.app_handle
                        .emit("character-login", &character_name)
                        .ok();

                    // Prefer the server from the login line itself: the chat
                    // marker declares it, and ambient active_server may still
                    // hold the previous session (shared PC) or a stale boot
                    // value. Persisting that guess poisons every future
                    // session_server_for lookup for this character.
                    let login_server = server_name.clone().or_else(|| {
                        self.settings.get().active_server_name.clone()
                    });
                    // Auto-register character with login server
                    if let Ok(conn) = self.db_pool.get_write() {
                        conn.execute(
                            "INSERT INTO user_characters (character_name, server_name, source, last_login_time)
                             VALUES (?1, COALESCE(?2, 'Unknown'), 'login', CURRENT_TIMESTAMP)
                             ON CONFLICT(character_name, server_name) DO UPDATE SET
                                last_login_time = CURRENT_TIMESTAMP,
                                updated_at = CURRENT_TIMESTAMP",
                            rusqlite::params![
                                character_name,
                                login_server.clone(),
                            ],
                        ).ok();
                    }

                    // Update active character in settings
                    let mut settings = self.settings.get();
                    settings.active_character_name = Some(character_name.clone());
                    let server = login_server.unwrap_or_else(|| "Unknown".to_string());
                    self.settings.update(settings).ok();

                    self.game_state
                        .set_active_character(&character_name, &server, &self.db_pool, Some(&timestamp.format("%Y-%m-%d %H:%M:%S").to_string()));
                    // A session signal moved the active pair: later untagged
                    // lines resolve live, never against the pre-batch pair.
                    identity_mutated = true;
                }
                LogEvent::CharacterLogout { character_name, .. } => {
                    // A logout ends the session: trailing `None`-tagged lines
                    // must not inherit the stale pair. The owning session's
                    // logout clears the server half of the in-memory
                    // identity, so every pair read below is `None` and
                    // identity-less lines drop until the next login re-arms
                    // both halves (the character slot keeps its last name —
                    // same half-clear shape as the Player.log re-arm path).
                    // A foreign logout never wipes the live identity. The
                    // logout-then-trailing-`None` sequence is pinned in
                    // `session_identity_tests`, through the functions below.
                    apply_chat_logout_identity(&mut self.game_state, &character_name);
                    identity_mutated = true;
                }
                _ => {}
            }
        }
        // Batch-end retry for survey tags that failed mid-batch (TODO 53):
        // one more pass over the staged gains through the same atomic unit.
        // Single pass only — a retry that still fails is logged and the gain
        // stays unattributed (the rollback leaves no qty behind, so a later
        // in-window use reclaimed via retro-claim cannot double-count).
        // Runs before the fallible message-insert tail so a tail failure
        // cannot strand staged gains that a fresh checkout would have healed.
        if !survey_tag_retries.is_empty() {
            match self.db_pool.get_write() {
                Ok(conn) => {
                    for pending in std::mem::take(&mut survey_tag_retries) {
                        if let Err(e) =
                            self.survey_aggregator.attribute_and_tag_chat_gain(
                                &conn,
                                &pending.character,
                                &pending.server,
                                pending.internal_name.as_deref(),
                                pending.quantity,
                                &pending.ts_utc,
                                Some(pending.item_name.as_str()),
                            )
                        {
                            eprintln!(
                                "[coordinator] survey batch-end retry failed for {} x{} ({}): {e}",
                                pending.item_name, pending.quantity, pending.ts_utc
                            );
                        }
                    }
                }
                Err(e) => {
                    eprintln!(
                        "[coordinator] survey batch-end retry skipped, pool checkout failed ({} gains left unattributed; an in-window later use may still reclaim them): {e}",
                        survey_tag_retries.len()
                    );
                }
            }
        }

        // Store the twin slot back before any fallible tail: the skips above
        // already counted, so a later insert failure must not replay them.
        // Expire a stale arming: a poll that drained chat lines but produced
        // no unknown-skill sighting means no twin is pending across the
        // boundary (an unpaired half from an earlier poll is dead). Empty
        // polls preserve the slot — a twin split across idle polls is still
        // consecutive in log order.
        self.last_unknown_skill = if !messages.is_empty() && !saw_unknown_skill {
            None
        } else {
            last_unknown_skill
        };

        // Batch insert messages (watch rules already evaluated per message
        // through the shared dispatch, which forwards what fired).
        if !messages.is_empty() {
            let conn = self
                .db_pool
                .get_write()
                .map_err(|e| format!("Database error: {}", e))?;

            let excluded_channels = &self.settings.get().excluded_chat_channels;
            let inserted = insert_chat_messages(&conn, &messages, &log_file, excluded_channels)
                .map_err(|e| format!("Failed to insert messages: {}", e))?;

            if inserted > 0 {
                self.app_handle
                    .emit("chat-messages-inserted", inserted)
                    .ok();
            }
        }

        Ok(())
    }

    /// Emit status change event to frontend
    fn emit_status_change(&self) -> Result<(), String> {
        let status = self.get_status();
        self.app_handle
            .emit("coordinator-status", status)
            .map_err(|e| format!("Failed to emit event: {}", e))
    }

    /// Auto-import one VIP report the game just wrote (`ReportSaved` chat
    /// line). Same behavior as the old inline arm; failures just log.
    fn import_report_saved(&self, file_path: &str) {
        match report_kind_for_path(file_path) {
            Some(ReportKind::Character) => {
                match crate::db::character_commands::import_character_report_internal(
                    &self.db_pool,
                    file_path,
                    &self.game_data,
                ) {
                    Ok(r) if !r.was_duplicate => {
                        self.app_handle
                            .emit(
                                "game-state-updated",
                                vec![
                                    "skills",
                                    "recipes",
                                    "favor",
                                    "vendor",
                                    "currencies",
                                    "storage",
                                    "currency_estimate",
                                ],
                            )
                            .ok();
                    }
                    Ok(_) => {}
                    Err(e) => eprintln!(
                        "[coordinator] ReportSaved auto-import failed for {file_path}: {e}"
                    ),
                }
            }
            Some(ReportKind::Items) => {
                match crate::db::inventory_commands::import_inventory_report_internal(
                    &self.db_pool,
                    file_path,
                ) {
                    Ok(r) if !r.was_duplicate => {
                        self.app_handle
                            .emit("game-state-updated", vec!["storage", "inventory"])
                            .ok();
                    }
                    Ok(_) => {}
                    Err(e) => eprintln!(
                        "[coordinator] ReportSaved auto-import failed for {file_path}: {e}"
                    ),
                }
            }
            None => {}
        }
    }

    /// Parse structured stats from PlayerAge or Behavior Report books and
    /// persist them to the `character_stats` table as key-value pairs.
    /// `at` is the book's own observation time (same naive shape the
    /// teleport-bind arm parses); an unparsable stamp degrades to
    /// fill-empty-only like the bind writer.
    fn ingest_report_stats(&self, at: &str, book_type: &str, content: &str, character: &str, server: &str) {
        // The book's own observation time is the freshness signal: a stale
        // replay must not clobber newer stats (unparsable `at` degrades to
        // untimestamped, which only fills an empty table).
        let captured_at = chrono::NaiveDateTime::parse_from_str(at, "%Y-%m-%d %H:%M:%S").ok();
        ingest_report_stats_content(
            &self.db_pool,
            &self.app_handle,
            character,
            server,
            book_type,
            content,
            captured_at,
        )
        .map_err(|e| eprintln!("[coordinator] {e}"))
        .ok();
    }

    /// Parse teleportation bind locations from a "Skill Info" SkillReport book
    /// and persist them to `game_state_teleportation`.
    ///
    /// Expected content format:
    /// ```text
    /// Teleportation Status:\n\n\n
    /// Primary Bind Location: Red Wing Casino\n
    /// Secondary Bind Location: Caves Beneath Gazluk\n
    /// Last Used Location: Statehelm\n
    /// Most Common Destination: Serbule\n
    /// ```
    fn ingest_teleportation_binds(&self, at: &str, content: &str, character: &str, server: &str) {
        // Live ProcessBook content carries escaped newlines; the file path
        // carries real ones. Normalize once here so `extract_bind_field`
        // sees real line breaks on both paths (historically the live path
        // silently matched no fields).
        let content = normalize_book_content(content);
        // The book's own observation time is the freshness signal: a stale
        // replay must not clobber a newer bind (unparsable `at` degrades to
        // untimestamped, which only fills an empty table).
        let captured_at = chrono::NaiveDateTime::parse_from_str(at, "%Y-%m-%d %H:%M:%S").ok();
        if let Err(e) = ingest_teleport_binds_content(
            &self.db_pool,
            &self.app_handle,
            character,
            server,
            &content,
            captured_at,
        ) {
            eprintln!("[coordinator] Teleportation binds failed: {e}");
        }
    }

    /// Parse a hoplology "Equipment Studied:" skill report and backfill studied items.
    ///
    /// Content format (escaped newlines):
    ///   "Equipment Studied:\n\n  CrudBurst's Hammer of Thumping\n  Thentree Harness\n"
    fn ingest_hoplology_report(&self, _timestamp: &str, content: &str, character: &str, server: &str) {
        // Live ProcessBook content carries escaped newlines; the file path
        // carries real ones. Normalize once here so the line splitter sees
        // real line breaks on both paths (historically the live path split
        // on literal `\n` only).
        let content = normalize_book_content(content);
        match ingest_hoplology_content(
            &self.db_pool,
            &self.game_data,
            &self.app_handle,
            character,
            server,
            &content,
        ) {
            Ok(0) => startup_log!("[coordinator] Hoplology report: no new items"),
            Ok(_) => {} // logged inside
            Err(e) => eprintln!("[coordinator] Hoplology report failed: {e}"),
        }
    }

    // ── Debug Capture ──────────────────────────────────────────────────

    /// Start a debug capture session. Snapshots current game state and
    /// enables raw line buffering on both watchers.
    pub fn debug_capture_start(&mut self) -> Result<(), String> {
        if self.debug_capture.is_active() {
            return Err("A debug capture is already active".to_string());
        }
        if self.debug_capture.is_pending_save() {
            return Err(
                "A stopped capture is waiting to be saved — save or discard it before starting a new one"
                    .to_string(),
            );
        }

        let (char, server) = self.active_character_server().unwrap_or_default();
        let snapshot =
            debug_capture::snapshot_game_state(&self.db_pool, Some(&char), Some(&server))?;

        let temp_dir = self
            .app_handle
            .path()
            .app_data_dir()
            .map_err(|e| format!("Cannot resolve app data dir: {e}"))?;

        // Enable raw line capture on watchers
        if let Some(w) = &mut self.player_watcher {
            w.set_capture_raw_lines(true);
        }
        if let Some(w) = &mut self.chat_watcher {
            w.set_capture_raw_lines(true);
        }

        self.debug_capture.start(snapshot, &temp_dir)?;
        startup_log!("[debug-capture] Capture started");
        Ok(())
    }

    /// Stop recording the active debug capture. The data is kept on disk
    /// so the user can review stats and edit notes before saving.
    pub fn debug_capture_stop(&mut self) -> Result<DebugCaptureResult, String> {
        if !self.debug_capture.is_active() {
            return Err("No debug capture is active".to_string());
        }

        // Drain any remaining raw lines before stopping
        self.drain_capture_lines();

        // Disable raw line capture on watchers
        if let Some(w) = &mut self.player_watcher {
            w.set_capture_raw_lines(false);
        }
        if let Some(w) = &mut self.chat_watcher {
            w.set_capture_raw_lines(false);
        }

        let (char, server) = self.active_character_server().unwrap_or_default();
        let snapshot =
            debug_capture::snapshot_game_state(&self.db_pool, Some(&char), Some(&server))?;

        let app_version = self
            .app_handle
            .config()
            .version
            .clone()
            .unwrap_or_else(|| "unknown".to_string());

        let result = self.debug_capture.stop(snapshot, app_version)?;

        startup_log!(
            "[debug-capture] Capture stopped — {} lines ({} player, {} chat)",
            result.line_count,
            result.player_line_count,
            result.chat_line_count,
        );

        Ok(result)
    }

    /// Save the stopped capture to a file with the given notes, filter mode,
    /// and explicit overwrite flag (existing files are never replaced
    /// silently).
    pub fn debug_capture_save(
        &mut self,
        notes: String,
        filter_mode: String,
        output_path: String,
        overwrite: bool,
    ) -> Result<DebugCaptureResult, String> {
        if !self.debug_capture.is_pending_save() {
            return Err("No stopped capture is pending save".to_string());
        }

        let result =
            self.debug_capture
                .save(notes, filter_mode.clone(), &output_path, overwrite)?;

        startup_log!(
            "[debug-capture] Capture saved ({} mode) — {} lines ({} player, {} chat)",
            filter_mode,
            result.line_count,
            result.player_line_count,
            result.chat_line_count,
        );

        Ok(result)
    }

    /// Discard the capture (active or pending save) without saving.
    pub fn debug_capture_discard(&mut self) {
        // Disable raw line capture on watchers (in case still active)
        if let Some(w) = &mut self.player_watcher {
            w.set_capture_raw_lines(false);
        }
        if let Some(w) = &mut self.chat_watcher {
            w.set_capture_raw_lines(false);
        }

        self.debug_capture.discard();
        startup_log!("[debug-capture] Capture discarded");
    }

    /// Get the current debug capture status.
    pub fn debug_capture_status(&self) -> DebugCaptureStatus {
        self.debug_capture.status()
    }

    /// Drain buffered raw lines from watchers into the capture temp file.
    fn drain_capture_lines(&mut self) {
        if let Some(w) = &mut self.player_watcher {
            for line in w.drain_raw_lines() {
                self.debug_capture.push_line("player", line);
            }
        }
        if let Some(w) = &mut self.chat_watcher {
            for line in w.drain_raw_lines() {
                self.debug_capture.push_line("chat", line);
            }
        }
    }
}

// ============================================================
// Tauri Commands (frontend interface)
// ============================================================

use tauri::State;

/// Start player log tailing (called from frontend)
#[tauri::command]
pub fn start_player_tailing(
    coordinator: State<'_, Arc<Mutex<DataIngestCoordinator>>>,
) -> Result<(), String> {
    let mut coord = coordinator.lock().map_err(|e| e.to_string())?;
    coord.start_player_log_tailing()
}

/// Stop player log tailing (called from frontend)
#[tauri::command]
pub fn stop_player_tailing(
    coordinator: State<'_, Arc<Mutex<DataIngestCoordinator>>>,
) -> Result<(), String> {
    let mut coord = coordinator.lock().map_err(|e| e.to_string())?;
    coord.stop_player_log_tailing()
}

/// Scan every Player.log source for past poem recitals and backfill the poems table
/// (called from the Poems tab). Returns the number of newly inserted poems.
///
/// Lock discipline: snapshot the Arc handles under a short lock, then run the
/// (potentially tens-of-seconds) scan + write outside it. The backfill takes
/// only `&SettingsManager` + `&DbPool` — both shared, never coordinator
/// state — so holding the mutex across it would stall polling and every
/// other command for no safety gain (pool-is-the-lock invariant).
#[tauri::command]
pub fn scan_player_log_for_poems(
    coordinator: State<'_, Arc<Mutex<DataIngestCoordinator>>>,
) -> Result<usize, String> {
    let (settings, db_pool, app_handle) = {
        let coord = coordinator.lock().map_err(|e| e.to_string())?;
        (
            Arc::clone(&coord.settings),
            coord.db_pool.clone(),
            coord.app_handle.clone(),
        )
    };
    crate::db::poem_commands::backfill_from_player_logs_with_emit(&settings, &db_pool, &app_handle)
}

/// Start chat log tailing (called from frontend)
#[tauri::command]
pub fn start_chat_tailing(
    coordinator: State<'_, Arc<Mutex<DataIngestCoordinator>>>,
    settings: State<'_, Arc<SettingsManager>>,
) -> Result<(), String> {
    // Get today's chat log file
    let chat_logs_dir = settings
        .get_chat_logs_dir()
        .ok_or_else(|| "Chat logs directory not configured".to_string())?;

    let today = chrono::Local::now();
    let date_str = format!(
        "{}-{:02}-{:02}",
        today.year() % 100,
        today.month(),
        today.day()
    );
    let chat_log_file = chat_logs_dir.join(format!("Chat-{}.log", date_str));

    let mut coord = coordinator.lock().map_err(|e| e.to_string())?;
    coord.start_chat_log_tailing(chat_log_file)
}

/// Stop chat log tailing (called from frontend)
#[tauri::command]
pub fn stop_chat_tailing(
    coordinator: State<'_, Arc<Mutex<DataIngestCoordinator>>>,
) -> Result<(), String> {
    let mut coord = coordinator.lock().map_err(|e| e.to_string())?;
    coord.stop_chat_log_tailing()
}

/// Get coordinator status (called from frontend)
#[tauri::command]
pub fn get_coordinator_status(
    coordinator: State<'_, Arc<Mutex<DataIngestCoordinator>>>,
) -> Result<CoordinatorStatus, String> {
    let coord = coordinator.lock().map_err(|e| e.to_string())?;
    Ok(coord.get_status())
}

/// Poll all watchers (one-shot, for manual/diagnostic use from frontend)
#[tauri::command]
pub fn poll_watchers(
    coordinator: State<'_, Arc<Mutex<DataIngestCoordinator>>>,
) -> Result<(), String> {
    let mut coord = coordinator.lock().map_err(|e| e.to_string())?;
    coord.poll()
}

// ── Background Polling ─────────────────────────────────────────────────

/// Shared flag that the background polling thread checks each iteration.
/// When false the thread stays alive but skips the poll() call.
pub type PollingFlag = Arc<AtomicBool>;

/// Spawn the background polling thread and return a handle to control it.
///
/// The thread acquires the coordinator Mutex for each poll cycle, so it
/// cooperates with Tauri commands that also lock the coordinator.
pub fn spawn_polling_thread(
    coordinator: Arc<Mutex<DataIngestCoordinator>>,
    interval: Duration,
) -> PollingHandle {
    let active = Arc::new(AtomicBool::new(false));
    let flag = active.clone();
    let alive = Arc::new(AtomicBool::new(true));
    let alive_clone = alive.clone();

    let handle = std::thread::Builder::new()
        .name("poll-watchers".into())
        .spawn(move || {
            let mut skipped: u64 = 0;
            while alive_clone.load(Ordering::Relaxed) {
                if active.load(Ordering::Relaxed) {
                    match coordinator.try_lock() {
                        Ok(mut coord) => {
                            if let Err(e) = coord.poll() {
                                eprintln!("[poll-thread] poll error: {e}");
                            }
                        }
                        Err(TryLockError::WouldBlock) => {
                            skipped += 1;
                            if skipped == 1 || skipped.is_multiple_of(50) {
                                eprintln!(
                                    "[poll-thread] coordinator busy; skipped {skipped} poll cycle(s)"
                                );
                            }
                        }
                        Err(TryLockError::Poisoned(p)) => {
                            eprintln!("[poll-thread] coordinator lock poisoned; recovering");
                            let mut coord = p.into_inner();
                            if let Err(e) = coord.poll() {
                                eprintln!("[poll-thread] poll error: {e}");
                            }
                        }
                    }
                }
                std::thread::sleep(interval);
            }
        })
        .expect("failed to spawn poll-watchers thread");

    PollingHandle {
        flag,
        alive,
        join_handle: Some(handle),
    }
}

/// Wrapper managed as Tauri state so commands can toggle polling.
pub struct PollingHandle {
    pub flag: PollingFlag,
    pub alive: Arc<AtomicBool>,
    pub join_handle: Option<std::thread::JoinHandle<()>>,
}

impl PollingHandle {
    pub fn start(&self) {
        self.flag.store(true, Ordering::Relaxed);
    }
    pub fn stop(&self) {
        self.flag.store(false, Ordering::Relaxed);
    }
    pub fn shutdown(&mut self) {
        self.flag.store(false, Ordering::Relaxed);
        self.alive.store(false, Ordering::Relaxed);
        if let Some(h) = self.join_handle.take() {
            let _ = h.join();
        }
    }
}

/// Start background polling (called from frontend after startup catch-up)
#[tauri::command]
pub fn start_background_polling(
    handle: State<'_, Arc<Mutex<PollingHandle>>>,
) -> Result<(), String> {
    handle.lock().unwrap().start();
    Ok(())
}

/// Stop background polling (called from frontend or on shutdown)
#[tauri::command]
pub fn stop_background_polling(handle: State<'_, Arc<Mutex<PollingHandle>>>) -> Result<(), String> {
    handle.lock().unwrap().stop();
    Ok(())
}

// ── Debug Capture Commands ──────────────────────────────────────────────

/// Start a debug capture session.
#[tauri::command]
pub fn debug_capture_start(
    coordinator: State<'_, Arc<Mutex<DataIngestCoordinator>>>,
) -> Result<(), String> {
    let mut coord = coordinator.lock().map_err(|e| e.to_string())?;
    coord.debug_capture_start()
}

/// Stop recording the active debug capture. Data is kept for review.
#[tauri::command]
pub fn debug_capture_stop(
    coordinator: State<'_, Arc<Mutex<DataIngestCoordinator>>>,
) -> Result<DebugCaptureResult, String> {
    let mut coord = coordinator.lock().map_err(|e| e.to_string())?;
    coord.debug_capture_stop()
}

/// Save the stopped capture to a file with notes, filter mode, and explicit
/// overwrite flag. Defaults to false at the call site when omitted.
#[tauri::command]
pub fn debug_capture_save(
    coordinator: State<'_, Arc<Mutex<DataIngestCoordinator>>>,
    notes: String,
    filter_mode: String,
    output_path: String,
    overwrite: Option<bool>,
) -> Result<DebugCaptureResult, String> {
    let mut coord = coordinator.lock().map_err(|e| e.to_string())?;
    coord.debug_capture_save(notes, filter_mode, output_path, overwrite.unwrap_or(false))
}

/// Discard the active debug capture without saving.
#[tauri::command]
pub fn debug_capture_discard(
    coordinator: State<'_, Arc<Mutex<DataIngestCoordinator>>>,
) -> Result<(), String> {
    let mut coord = coordinator.lock().map_err(|e| e.to_string())?;
    coord.debug_capture_discard();
    Ok(())
}

/// Get the current debug capture status.
#[tauri::command]
pub fn debug_capture_status(
    coordinator: State<'_, Arc<Mutex<DataIngestCoordinator>>>,
) -> Result<DebugCaptureStatus, String> {
    let coord = coordinator.lock().map_err(|e| e.to_string())?;
    Ok(coord.debug_capture_status())
}

// ============================================================
// Book-content ingestion (shared by live ProcessBook dispatch and
// the Books-directory backfill watcher)
// ============================================================

/// Normalize escaped-newline book content into real newlines.
///
/// Book bodies arrive in two shapes: literal `\n` (and `\t`) escape
/// sequences from JSON chat lines and older report formats, and real
/// newlines in files under `<game_data>/Books/`. Feeding both through this
/// no-op-on-real-newlines replace makes every downstream line splitter
/// behave identically on either shape. (Extracted per-shape helpers
/// historically normalized inconsistently — `parse_shop_log` and
/// `parse_player_age` did, the teleport-bind / hoplology / gourmand
/// splitters did not — so live-path bind parsing silently matched nothing.)
pub(crate) fn normalize_book_content(content: &str) -> String {
    content.replace("\\n", "\n").replace("\\t", "\t")
}

/// Persist a book report to `game_state_books`, upserting by
/// (character, server, book_type, title) so re-running a report overwrites.
/// Shared by the live coordinator and the Books-directory watcher.
///
/// Recency: `captured_at` is the book's own observation time (live book `at`,
/// or the watcher's `*_YYMMDD_HHMMSS` filename suffix). An older capture
/// replayed over a newer row is skipped, and an identical re-read writes
/// nothing (no stamp advance) — forward-only, same rule as the
/// teleport-bind writer. `None` (untimestamped source) stamps the oldest
/// representable instant, so it only fills an empty row.
///
/// Swallows SQL errors to keep ingest flowing, but returns `Err` on pool
/// acquisition failure so callers can log it.
#[allow(clippy::too_many_arguments)] // shared live+watcher shape: identity (4) + book key (2) + body + capture
pub fn persist_book_content(
    db: &DbPool,
    _app_handle: &AppHandle,
    character: &str,
    server: &str,
    book_type: &str,
    title: &str,
    content: &str,
    captured_at: Option<chrono::NaiveDateTime>,
) -> Result<(), String> {
    // Same oldest-instant rule as the teleport-bind writer: wall-clock
    // `now()` here would beat every real older book and wedge the row.
    let dt = captured_at
        .map(|c| c.and_utc().to_rfc3339())
        .unwrap_or_else(|| "0001-01-01T00:00:00+00:00".to_string());
    let conn = db
        .get_write()
        .map_err(|e| format!("Database connection error: {e}"))?;
    upsert_book_content(&conn, character, server, book_type, title, content, &dt)
        .map_err(|e| format!("Failed to persist book: {e}"))?;
    Ok(())
}

/// Testable core of [`persist_book_content`] (no pool, no `AppHandle`):
/// identical content skips the write, and the guarded upsert drops an older
/// capture over a newer row. Returns `true` when the write landed.
pub(crate) fn upsert_book_content(
    conn: &rusqlite::Connection,
    character: &str,
    server: &str,
    book_type: &str,
    title: &str,
    content: &str,
    captured_at: &str,
) -> Result<bool, String> {
    // Identical re-read (rescan): nothing to write, stamp stays put.
    let same: bool = conn
        .query_row(
            "SELECT content FROM game_state_books
             WHERE character_name = ?1 AND server_name = ?2
               AND book_type = ?3 AND title = ?4",
            rusqlite::params![character, server, book_type, title],
            |r| r.get::<_, String>(0),
        )
        .map(|c| c == content)
        .unwrap_or(false);
    if same {
        return Ok(false);
    }
    let n = conn
        .execute(
            "INSERT INTO game_state_books (character_name, server_name, book_type, title, content, captured_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)
         ON CONFLICT(character_name, server_name, book_type, title) DO UPDATE SET
            content = excluded.content,
            captured_at = excluded.captured_at
         WHERE captured_at <= excluded.captured_at",
            rusqlite::params![character, server, book_type, title, content, captured_at],
        )
        .map_err(|e| format!("Failed to persist book: {e}"))?;
    Ok(n > 0)
}

/// Parse structured stats from a PlayerAge or Behavior Report (HelpScreen)
/// book and persist them to `character_report_stats`. Shared by the live
/// coordinator and the Books-directory watcher.
///
/// Recency: `captured_at` is the book's own observation time (same sources
/// as [`persist_book_content`]). It threads through as the row stamp so the
/// forward-only guard in `persist_stats` can tell a stale replay from a
/// fresh report; `None` degrades to fill-empty-only like the bind writer.
/// A no-op import (identical or older) stays silent — no `report_stats`
/// domain, same rescan-silence as the garden importer.
pub fn ingest_report_stats_content(
    db: &DbPool,
    app_handle: &AppHandle,
    character: &str,
    server: &str,
    book_type: &str,
    content: &str,
    captured_at: Option<chrono::NaiveDateTime>,
) -> Result<usize, String> {
    let stats = match book_type {
        "PlayerAge" => crate::report_stats::parse_player_age(content),
        "HelpScreen" => crate::report_stats::parse_behavior_report(content),
        _ => return Ok(0),
    };
    if stats.is_empty() {
        return Ok(0);
    }
    let dt = captured_at
        .map(|c| c.and_utc().to_rfc3339())
        .unwrap_or_else(|| "0001-01-01T00:00:00+00:00".to_string());
    let conn = db
        .get_write()
        .map_err(|e| format!("Failed to persist report stats: {e}"))?;
    let n = crate::report_stats::persist_stats(&conn, character, server, &stats, &dt)
        .map_err(|e| format!("Failed to persist report stats: {e}"))?;
    if n > 0 {
        startup_log!(
            "[coordinator] Imported {} stats from {} report",
            n,
            book_type,
        );
        app_handle
            .emit("game-state-updated", vec!["report_stats"])
            .ok();
    }
    Ok(n)
}

/// Extract a named field value from teleportation status text.
/// Returns `None` if the field is missing or has value "(none)".
pub(crate) fn extract_bind_field(content: &str, field: &str) -> Option<String> {
    for line in content.split('\n') {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix(field) {
            let value = rest.trim();
            if value.is_empty() || value == "(none)" {
                return None;
            }
            return Some(value.to_string());
        }
    }
    None
}

/// Parse teleportation bind locations from a "Skill Info" SkillReport book
/// and persist them to `game_state_teleportation`. Shared by the live
/// coordinator and the Books-directory watcher.
///
/// Recency gate: `captured_at` is the book's own observation time (live book
/// `at`, or the watcher's `SkillReport_YYMMDD_HHMMSS` filename suffix). An
/// older capture replayed at startup must not clobber a newer bind, so a
/// capture older than the stored row is skipped silently. `None`
/// (untimestamped source) only fills an empty table. Free function so the
/// gate is unit-testable without an `AppHandle`.
fn bind_capture_is_fresh(
    stored_last_updated: Option<&str>,
    captured_at: Option<chrono::NaiveDateTime>,
) -> bool {
    let Some(captured) = captured_at else {
        return stored_last_updated.is_none();
    };
    let Some(stored) = stored_last_updated else {
        return true;
    };
    // Stored stamps are RFC3339 (this writer); parse flexibly so a legacy
    // row never blocks a write it can't be compared against.
    let stored_dt = chrono::DateTime::parse_from_rfc3339(stored)
        .map(|d| d.naive_utc())
        .or_else(|_| chrono::NaiveDateTime::parse_from_str(stored, "%Y-%m-%d %H:%M:%S"));
    match stored_dt {
        Ok(s) => captured >= s,
        Err(_) => true,
    }
}
pub fn ingest_teleport_binds_content(
    db: &DbPool,
    app_handle: &AppHandle,
    character: &str,
    server: &str,
    content: &str,
    captured_at: Option<chrono::NaiveDateTime>,
) -> Result<(), String> {
    let primary = extract_bind_field(content, "Primary Bind Location:");
    let secondary = extract_bind_field(content, "Secondary Bind Location:");

    // Upsert MUST run on the writer connection: the read pool is
    // PRAGMA query_only, so `get()` here silently dropped every bind
    // update (execute failure swallowed by `.ok()`).
    let conn = db.get_write().map_err(|e| format!("Database error: {e}"))?;

    // Stale-capture veto (see `bind_capture_is_fresh`): a read error here
    // degrades to "no stored row" and the upsert below surfaces it.
    let stored: Option<String> = conn
        .query_row(
            "SELECT last_updated FROM game_state_teleportation WHERE character_name = ?1 AND server_name = ?2",
            rusqlite::params![character, server],
            |r| r.get(0),
        )
        .ok();
    if !bind_capture_is_fresh(stored.as_deref(), captured_at) {
        return Ok(());
    }
    // Untimestamped backfill carries no observation time: stamp the oldest
    // representable instant so any genuine capture supersedes it. A
    // wall-clock `now()` here would beat every real older book and wedge
    // the row (the watcher sorts untimestamped files first).
    let dt = captured_at
        .map(|c| c.and_utc().to_rfc3339())
        .unwrap_or_else(|| "0001-01-01T00:00:00+00:00".to_string());
    conn.execute(
        "INSERT INTO game_state_teleportation
            (character_name, server_name, primary_bind, secondary_bind, last_updated)
         VALUES (?1, ?2, ?3, ?4, ?5)
         ON CONFLICT(character_name, server_name) DO UPDATE SET
            primary_bind = excluded.primary_bind,
            secondary_bind = excluded.secondary_bind,
            last_updated = excluded.last_updated",
        rusqlite::params![character, server, primary, secondary, dt],
    )
    .map_err(|e| format!("Failed to upsert teleportation binds: {e}"))?;

    startup_log!(
        "[coordinator] Teleportation binds updated: primary={:?}, secondary={:?}",
        primary,
        secondary
    );
    app_handle
        .emit("game-state-updated", vec!["teleportation"])
        .ok();
    Ok(())
}

/// Parse a hoplology "Equipment Studied:" skill report and backfill studied
/// items. Returns the number of newly inserted studies. Shared by the live
/// coordinator and the Books-directory watcher.
pub fn ingest_hoplology_content(
    db: &DbPool,
    game_data: &GameDataState,
    app_handle: &AppHandle,
    character: &str,
    server: &str,
    content: &str,
) -> Result<usize, String> {
    // CDN data MUST be loaded before backfilling: the dedup key is the
    // (CDN-resolved) base item name, so a report ingested before
    // `init_game_data` swaps the real data in would store raw crafted
    // names ("Hailin' Thorian Kilt of Deathspark") under a different
    // UNIQUE key than the same item's base name ("Thorian Kilt") —
    // permanently polluting the to-study list with rows the widget can
    // never reconcile. The Books watcher treats Err as retry-later (and
    // this error as transient — it never trips the give-up counter), so
    // the window self-heals once the CDN load lands, however long that
    // takes. (The live path's per-open dispatch has the same latency but
    // re-fires on every book open; the watcher is one-shot per mtime,
    // making a silent fallback permanent there — hence the hard gate
    // rather than best-effort.)
    // The read guard is held for the WHOLE function: base-name resolution
    // happens per line below, and dropping the guard after the emptiness
    // check would reopen a window where a concurrent refresh (holding the
    // write lock) makes the later `try_read().ok()` fail and silently
    // fall back to raw crafted names — the exact pollution this gate
    // exists to prevent.
    let game_data = game_data
        .try_read()
        .map_err(|_| "Game data not loaded yet; retrying later".to_string())?;
    if game_data.items.is_empty() {
        return Err("Game data not loaded yet; retrying later".to_string());
    }

    // Inserts MUST run on the writer connection: the read pool is
    // PRAGMA query_only, so `get()` here silently dropped every study
    // row. Failures propagate so the Books watcher can retry the file.
    // Taken AFTER the game-data gate: a DB error is transient (never
    // trips the watcher's give-up counter) the same as a not-loaded-yet
    // gate error.
    let conn = db.get_write().map_err(|e| format!("Database error: {e}"))?;

    // Use current wall-clock time as the "first seen" timestamp for
    // report-backfilled items (the player.log timestamp is just HH:MM:SS
    // which isn't a valid datetime).
    let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();

    let mut inserted = 0usize;
    // Normalized content (real newlines) — one entry per line
    for line in content.split('\n') {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with("Equipment Studied:") {
            continue;
        }
        // Resolve to base equipment name if CDN data is available
        let base_name = game_data
            .find_equipment_base_name(trimmed)
            .unwrap_or_else(|| trimmed.to_string());

        match crate::db::hoplology_commands::insert_hoplology_study_from_report(
            &conn, character, server, &base_name, &now,
        ) {
            Ok(Some(_)) => inserted += 1,
            Ok(None) => {} // already known
            Err(e) => {
                // Propagate the first failure: already-inserted rows are
                // idempotent, so returning Err makes the Books watcher treat
                // the file as unseen and retry it — instead of marking the
                // mtime seen over a partial import.
                eprintln!(
                    "[coordinator] Failed to insert hoplology study '{}': {}",
                    trimmed, e
                );
                return Err(format!("hoplology insert failed for '{}': {e}", trimmed));
            }
        }
    }

    if inserted > 0 {
        startup_log!(
            "[coordinator] Hoplology report: backfilled {} new items",
            inserted
        );
        app_handle
            .emit("game-state-updated", vec!["hoplology"])
            .ok();
    }
    Ok(inserted)
}

/// Where a catch-up session's server came from (startup log line + tests).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SessionServerSource {
    /// A `user_characters` row for this character sits on the server in use.
    InUse,
    /// The character's own `user_characters` row (picker-active, else newest).
    UserCharacters,
    /// The catch-up chat-login scan (`chat_session_servers`).
    ChatLog,
    /// Nothing knew it: the server in use is kept.
    None,
    /// The lookup could not run: the server in use is kept, and the chat-login
    /// tier is *not* consulted — a read failure is not an unknown character.
    LookupFailed,
}

impl SessionServerSource {
    fn label(self) -> &'static str {
        match self {
            SessionServerSource::InUse => "server in use",
            SessionServerSource::UserCharacters => "user_characters",
            SessionServerSource::ChatLog => "chat log",
            SessionServerSource::None => "none — kept the server in use",
            SessionServerSource::LookupFailed => "lookup failed — kept the server in use",
        }
    }
}

/// Answer of [`session_server_for`]. Public to the crate so scan paths
/// (`chat_commands`) resolve the same per-file identity the drain uses.
///
/// `NoRow` and `Failed` must stay distinct: the caller keys a whole replayed
/// session on the answer, and the accepted residual is that those rows are not
/// self-correcting — a transient read failure must not borrow another
/// character's server by looking like an unknown character.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum SessionServerLookup {
    /// The character's own `user_characters` row.
    Row(String),
    /// The character has no row at all: consult the chat-login scan.
    NoRow,
    /// Pool / prepare / SQL failure (already logged): keep the server in use.
    Failed,
}

/// Adopt a drained Player.log login line as the ingest identity.
///
/// Free function (not a method) so the routing — the only place that decides
/// which `(character, server)` the following session's rows are keyed to — is
/// testable without a coordinator or an `AppHandle`.
fn apply_catch_up_login(
    game_state: &mut GameStateManager,
    db: &DbPool,
    chat_servers: &std::collections::HashMap<String, String>,
    character_name: &str,
    clear_inventory: bool,
) -> SessionServerSource {
    let current_server = game_state.get_active_server().map(str::to_string);
    game_state.set_active_character_name(character_name);
    let (server, source) = match session_server_for(db, character_name, current_server.as_deref()) {
        SessionServerLookup::Row(server) => {
            let source = if current_server.as_deref() == Some(server.as_str()) {
                SessionServerSource::InUse
            } else {
                SessionServerSource::UserCharacters
            };
            (Some(server), source)
        }
        // No `user_characters` row at all — the case that used to borrow the
        // selected character's server: take this character's own chat login line.
        SessionServerLookup::NoRow => match chat_servers.get(character_name) {
            Some(server) => (Some(server.clone()), SessionServerSource::ChatLog),
            None => (current_server.clone(), SessionServerSource::None),
        },
        // A read failure says nothing about the character, so it must not reach
        // the chat tier (whose answer would then own the session's rows for good).
        SessionServerLookup::Failed => (current_server.clone(), SessionServerSource::LookupFailed),
    };
    if let Some(server) = server {
        game_state.set_active_server_name(&server);
        // The adopted session rebuilds its own inventory from report imports
        // (which delete-then-reinsert the snapshot slice), so only obsolete
        // log-derived rows are cleared here. Saved snapshot rows survive the
        // adopt: no production writer creates 'log' inventory rows anymore,
        // and deleting the snapshot on every login emptied inventory until
        // the next fresh import. Equipment/combat/mount/favor are deliberately
        // untouched — none has a production writer (no full-dump reinsert, no
        // report import, no live/chat feed), so nothing would repopulate the
        // slice after a wipe.
        if clear_inventory
            && let (Some(character), Ok(conn)) = (
                game_state.get_active_character().map(str::to_string),
                db.get_write(),
            )
                && let Err(e) = conn.execute(
                    "DELETE FROM game_state_inventory WHERE character_name = ?1 AND server_name = ?2 AND source = 'log'",
                    rusqlite::params![character, server],
                ) {
                    startup_log!("[coordinator] clear-on-adopt inventory delete failed: {e}");
                }
    }
    source
}
/// Whether a `CharacterLogout` clears the in-memory ingest identity.
///
/// Free function (not a method) so the routing is testable without a
/// coordinator or an `AppHandle`, like [`apply_catch_up_login`]. Only the
/// owning session clears: a foreign logout (stale batch, crossed sessions)
/// never wipes the live identity.
fn logout_clears_identity(active_character: Option<&str>, logout_name: &str) -> bool {
    active_character == Some(logout_name)
}

/// Apply a `CharacterLogout` to the in-memory ingest identity: the owning
/// session's logout clears the server half, so every pair read
/// (`active_character_server`, the drain fallbacks) is `None` and
/// identity-less lines drop until the next login re-arms both halves.
/// Returns whether it cleared. The drain calls this, then marks the batch
/// mutated so trailing `None` tags resolve live instead of pre-batch.
fn apply_chat_logout_identity(game_state: &mut GameStateManager, logout_name: &str) -> bool {
    if logout_clears_identity(game_state.get_active_character(), logout_name) {
        game_state.clear_active_server_name();
        true
    } else {
        false
    }
}

/// Identity a tag-less chat line resolves to: its session tag when tagged;
/// the pre-batch pair ahead of the first marker; the live pair after.
/// The drain calls this per line; `session_identity_tests` scripts the
/// logout sequence through it on a real `GameStateManager` — no coordinator
/// or `AppHandle` needed.
fn resolve_chat_drain_identity(
    session_tag: Option<&crate::chat_parser::ChatSessionTag>,
    identity_mutated: bool,
    preceding_identity: Option<&DispatchIdentity>,
    live_identity: Option<DispatchIdentity>,
) -> Option<DispatchIdentity> {
    session_tag
        .map(|tag| DispatchIdentity {
            character: tag.character_name.clone(),
            server: tag.server_name.clone(),
        })
        .or_else(|| {
            if identity_mutated {
                None
            } else {
                preceding_identity.cloned()
            }
        })
        .or(live_identity)
}

/// Server to attribute a Player.log session to: [`SessionServerLookup`].
///
/// A Player.log login line names the character but not the server, so take it
/// from `user_characters` — the only table that pairs the two, written by the
/// chat-login marker (`process_chat_events`), which does read the server out of
/// Chat.log. Prefer the server already in use (keeps the selected character's
/// rows where they already are), then the picker's active row, then the most
/// recently updated one. `NoRow` when the character is unknown there: the caller
/// then consults the catch-up chat-login scan and keeps the server it has only
/// if that is empty too. A failed read is `Failed`, never `NoRow`.
///
/// The order is total: `updated_at` is written with second precision
/// (`CURRENT_TIMESTAMP`), so `id` breaks its ties and the same rows resolve the
/// same way on every launch.
pub(crate) fn session_server_for(
    db: &DbPool,
    character_name: &str,
    current_server: Option<&str>,
) -> SessionServerLookup {
    // Only "the character has no row" is `NoRow`; a pool/prepare/SQL failure is
    // `Failed`, because the caller then takes the chat-login tier (and those rows
    // are not self-correcting) — a read error must not masquerade as an unknown
    // character. Each failure is logged with its cause.
    let conn = match db.get() {
        Ok(c) => c,
        Err(e) => {
            startup_log!("[coordinator] session server lookup skipped: pool error: {e}");
            return SessionServerLookup::Failed;
        }
    };
    let mut stmt = match conn.prepare(
        "SELECT server_name FROM user_characters
         WHERE character_name = ?1
         ORDER BY (server_name = ?2) DESC, is_active DESC, updated_at DESC, id DESC
         LIMIT 1",
    ) {
        Ok(s) => s,
        Err(e) => {
            startup_log!("[coordinator] session server lookup failed to prepare: {e}");
            return SessionServerLookup::Failed;
        }
    };
    match stmt.query_row(
        rusqlite::params![character_name, current_server.unwrap_or("")],
        |row| row.get(0),
    ) {
        Ok(server) => SessionServerLookup::Row(server),
        Err(rusqlite::Error::QueryReturnedNoRows) => SessionServerLookup::NoRow,
        Err(e) => {
            startup_log!("[coordinator] session server lookup failed: {e}");
            SessionServerLookup::Failed
        }
    }
}

/// `character -> server` from every `Chat-*.log` in `dir`.
///
/// A Player.log login line has no server; the chat log's login line
/// (`chat_parser::parse_chat_login_line`) is the only log that names one. Files
/// are visited oldest-name-first (`Chat-YY-MM-DD.log` sorts chronologically) and
/// the last login line wins, so the answer is a function of the log files alone.
/// Missing directory / unreadable file: logged and skipped — the caller keeps
/// its pre-existing fallback.
fn chat_session_servers(dir: &std::path::Path) -> std::collections::HashMap<String, String> {
    use std::io::BufRead;

    let mut servers = std::collections::HashMap::new();
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(e) => {
            startup_log!(
                "Catch-up chat-login scan: cannot read {}: {e}",
                dir.display()
            );
            return servers;
        }
    };
    let mut files: Vec<std::path::PathBuf> = entries
        .filter_map(|entry| entry.ok().map(|e| e.path()))
        .filter(|path| {
            path.file_name()
                .and_then(|n| n.to_str())
                .map(|n| n.starts_with("Chat-") && n.ends_with(".log"))
                .unwrap_or(false)
        })
        .collect();
    files.sort();
    for path in files {
        let Ok(file) = std::fs::File::open(&path) else {
            startup_log!("Catch-up chat-login scan: cannot open {}", path.display());
            continue;
        };
        for line in std::io::BufReader::new(file).lines() {
            let Ok(line) = line else { continue };
            if let Some(info) = crate::chat_parser::parse_chat_login_line(&line) {
                servers.insert(info.character_name, info.server_name);
            }
        }
    }
    servers
}

#[cfg(test)]
mod report_kind_tests {
    use super::{report_kind_for_path, ReportKind};

    #[test]
    fn report_kind_classifies_character_path() {
        let kind = report_kind_for_path(
            "C:/Users/TestUser/AppData/LocalLow/Elder Game/Project Gorgon/Reports/Character_TestPlayer_Dreva.json",
        );
        assert!(matches!(kind, Some(ReportKind::Character)));
    }

    #[test]
    fn report_kind_classifies_items_path() {
        let kind = report_kind_for_path(
            "C:/Users/TestUser/AppData/LocalLow/Elder Game/Project Gorgon/Reports/TwinkleofToes_Dreva_items_2026-10-25-19-29-53Z.json",
        );
        assert!(matches!(kind, Some(ReportKind::Items)));
    }

    #[test]
    fn report_kind_rejects_book_and_bare_paths() {
        assert!(report_kind_for_path(
            "C:/Users/TestUser/AppData/LocalLow/Elder Game/Project Gorgon/Books/HelpScreen_260419_185352.txt"
        )
        .is_none());
        assert!(report_kind_for_path(
            "C:/Users/TestUser/AppData/LocalLow/Elder Game/Project Gorgon/Reports/Character_TestPlayer_Dreva.log"
        )
        .is_none());
    }

    #[test]
    fn report_kind_rejects_matching_name_outside_reports() {
        // Same file name the game would write, but not under Reports/:
        // never auto-import it.
        assert!(report_kind_for_path(
            "C:/Users/TestUser/Downloads/Character_TestPlayer_Dreva.json"
        )
        .is_none());
        assert!(report_kind_for_path(
            "C:/Users/TestUser/Downloads/TestPlayer_Dreva_items_2026-10-25-19-29-53Z.json"
        )
        .is_none());
        assert!(report_kind_for_path("Character_TestPlayer_Dreva.json").is_none());
    }

    #[test]
    fn report_kind_rejects_traversal_and_non_reports_parent() {
        // `Reports` cancelled by `..` resolves outside Reports/ — refuse.
        assert!(report_kind_for_path(
            "C:/Users/TestUser/AppData/LocalLow/Elder Game/Project Gorgon/Reports/../Downloads/x_items_y.json"
        )
        .is_none());
        // `_items_` in a non-Reports parent directory — refuse.
        assert!(report_kind_for_path(
            "C:/Users/TestUser/Downloads/my_items_stash/TestPlayer_Dreva_items_2026-10-25-19-29-53Z.json"
        )
        .is_none());
        // Bare `_items_` stem edge (empty left side) — refuse.
        assert!(report_kind_for_path(
            "C:/Users/TestUser/AppData/LocalLow/Elder Game/Project Gorgon/Reports/_items_2026-10-25.json"
        )
        .is_none());
        // Valid game shape still accepted.
        assert!(matches!(
            report_kind_for_path(
                "C:/Users/TestUser/AppData/LocalLow/Elder Game/Project Gorgon/Reports/TestPlayer_Dreva_items_2026-10-25-19-29-53Z.json"
            ),
            Some(ReportKind::Items)
        ));
    }
}

/// Server resolution for a Player.log session: the `user_characters` row when the
/// character has one, else its own chat login line (see `apply_catch_up_login`).
#[cfg(test)]
mod session_identity_tests {
    use super::{
        apply_chat_logout_identity, apply_catch_up_login, chat_session_servers,
        resolve_chat_drain_identity, session_server_for, DispatchIdentity, SessionServerLookup,
        SessionServerSource,
    };
    use crate::db::DbPool;
    use crate::game_state::GameStateManager;

    /// Unique temp path per call: `SystemTime` nanos repeat when parallel tests
    /// start in the same tick, so a shared `glogger_catch_up_login_test_*`
    /// prefix let two `seed_pool` callers open one DB — rows from one test
    /// leaked into the other's assertion (v1.3.8 Windows gate flake).
    fn unique_temp_path(prefix: &str, ext: &str) -> std::path::PathBuf {
        use std::sync::atomic::{AtomicU64, Ordering};
        static SEQ: AtomicU64 = AtomicU64::new(0);
        let n = SEQ.fetch_add(1, Ordering::Relaxed);
        // Nanos kept for human-sortable names; pid + counter carry uniqueness.
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        std::env::temp_dir().join(format!("{prefix}_{}_{nanos}_{n}.{ext}", std::process::id()))
    }

    /// Write a temp `Chat-*.log` set and return the dir holding it.
    fn write_chat_dir(files: &[(&str, &[&str])]) -> std::path::PathBuf {
        let dir = unique_temp_path("glogger_chat_scan_test", "d");
        std::fs::create_dir_all(&dir).unwrap();
        for (name, lines) in files {
            std::fs::write(dir.join(name), format!("{}\n", lines.join("\n"))).unwrap();
        }
        dir
    }

    /// A pool holding only the picker's row for `TestPlayer / Dreva` (what a user
    /// who chose that character leaves behind).
    const PICKER_ROW: &str =
        "INSERT INTO user_characters (character_name, server_name, source, is_active)
         VALUES ('TestPlayer', 'Dreva', 'manual', 1)";

    /// A temp pool seeded with a `user_characters` INSERT.
    fn seed_pool(insert: &str) -> (DbPool, std::path::PathBuf) {
        let path = unique_temp_path("glogger_catch_up_login_test", "db");
        let pool = crate::db::init_pool(path.clone(), Some(0)).expect("init pool");
        pool.get_write().unwrap().execute(insert, []).unwrap();
        (pool, path)
    }

    /// Fresh game state seeded like the coordinator's (settings' character+server).
    fn game_state(character: &str, server: &str) -> GameStateManager {
        let mut gs = GameStateManager::new();
        gs.set_active_character_name(character);
        gs.set_active_server_name(server);
        gs
    }

    /// Two servers for the selected character (the picker's active one older than
    /// the other's `updated_at`) plus one row for an alt on a server no other row
    /// uses — the three tie-breakers in the resolver, in different orders.
    fn seed() -> (DbPool, std::path::PathBuf) {
        let path = unique_temp_path("glogger_session_server_test", "db");
        let pool = crate::db::init_pool(path.clone(), Some(0)).expect("init pool");
        {
            let conn = pool.get_write().unwrap();
            conn.execute(
                "INSERT INTO user_characters (character_name, server_name, source, is_active, updated_at)
                 VALUES ('TestPlayer', 'Dreva', 'login', 1, '2026-10-01 00:00:00'),
                        ('TestPlayer', 'Legacy', 'login', 0, '2026-10-14 00:00:00'),
                        ('OtherAlt', 'OtherAltServer', 'login', 0, '2026-10-02 00:00:00')",
                [],
            )
            .unwrap();
        }
        (pool, path)
    }

    #[test]
    fn session_server() {
        let (db, path) = seed();
        let row = |server: &str| SessionServerLookup::Row(server.to_string());

        // (a) The server already in use wins, even though another row is newer.
        assert_eq!(
            session_server_for(&db, "TestPlayer", Some("Dreva")),
            row("Dreva")
        );
        // (b) Same, the other way round: rows stay where they already are.
        assert_eq!(
            session_server_for(&db, "TestPlayer", Some("Legacy")),
            row("Legacy")
        );
        // (c) An alt resolves through its own row, not through the server in
        // use: no other row uses `OtherAltServer`, so a dropped
        // `WHERE character_name = ?1` would answer 'Dreva' here.
        assert_eq!(
            session_server_for(&db, "OtherAlt", Some("Dreva")),
            row("OtherAltServer")
        );
        // (d) Unknown character → the caller keeps the server it has.
        assert_eq!(
            session_server_for(&db, "Unknown", Some("Dreva")),
            SessionServerLookup::NoRow
        );
        // (e) No server in use yet: the picker's active row wins over the
        // more recently updated one.
        assert_eq!(session_server_for(&db, "TestPlayer", None), row("Dreva"));
        // (f) A read that cannot run is not an unknown character: a failing
        // statement must answer `Failed`, so the caller never falls through to
        // the chat-login tier on an error. `DROP TABLE` reproduces one: the
        // startup line it prints matches the query arm's wording
        // (`session server lookup failed: no such table: user_characters`, not
        // the prepare arm's `… failed to prepare: …`), so that is the arm this
        // case covers — the pool and prepare arms are not reachable from here.
        db.get_write()
            .unwrap()
            .execute("DROP TABLE user_characters", [])
            .unwrap();
        assert_eq!(
            session_server_for(&db, "TestPlayer", Some("Dreva")),
            SessionServerLookup::Failed
        );

        // Drop the pool first: Windows won't delete a file with an open handle.
        drop(db);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn chat_session_servers_reads_oldest_file_first() {
        // `Chat-YY-MM-DD.log` sorts chronologically; the newer file is created
        // *first* here so directory order cannot decide the answer. `notes.txt`
        // holds a name that would show up if the glob were wider.
        let dir = write_chat_dir(&[
            (
                "Chat-26-03-24.log",
                &[
                    "26-03-24 09:00:00\t**************************************** Logged In As TestPlayer. Server Legacy. Timezone Offset -07:00:00.",
                    "26-03-24 09:00:00\t**************************************** Logged In As OtherAlt. Server OtherAltServer. Timezone Offset -07:00:00.",
                ],
            ),
            (
                "Chat-26-03-22.log",
                &["26-03-22 18:12:49\t**************************************** Logged In As TestPlayer. Server Dreva. Timezone Offset -07:00:00."],
            ),
            (
                "notes.txt",
                &["26-03-25 09:00:00\t**************************************** Logged In As TestPlayer. Server NotALog. Timezone Offset -07:00:00."],
            ),
        ]);

        let map = chat_session_servers(&dir);
        assert_eq!(map.len(), 2, "notes.txt is not a chat log: {map:?}");
        assert_eq!(map.get("TestPlayer").map(String::as_str), Some("Legacy"));
        assert_eq!(
            map.get("OtherAlt").map(String::as_str),
            Some("OtherAltServer")
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn chat_session_servers_missing_dir_is_empty() {
        let missing = std::env::temp_dir()
            .join("glogger_chat_scan_absent")
            .join("nope");
        assert!(chat_session_servers(&missing).is_empty());
    }

    #[test]
    fn catch_up_login_takes_the_server_from_the_chat_log() {
        // Real-format chat login lines, read through the real scanner.
        let chat_dir = write_chat_dir(&[(
            "Chat-26-10-15.log",
            &[
                "26-10-15 07:00:01\t**************************************** Logged In As TestPlayer. Server Dreva. Timezone Offset -05:00:00.",
                "26-10-15 07:05:01\t**************************************** Logged In As OtherAlt. Server OtherAltServer. Timezone Offset -05:00:00.",
            ],
        )]);
        let map = chat_session_servers(&chat_dir);
        assert_eq!(
            map.get("OtherAlt").map(String::as_str),
            Some("OtherAltServer")
        );

        // The picker's row, and nothing at all for the alt.
        let (pool, db_path) = seed_pool(PICKER_ROW);
        let mut gs = game_state("TestPlayer", "Dreva");

        assert_eq!(
            apply_catch_up_login(&mut gs, &pool, &map, "TestPlayer", true),
            SessionServerSource::InUse
        );
        pool.get_write().unwrap().execute(
            "INSERT INTO game_state_skills
             (character_name, server_name, skill_id, skill_name, level, last_confirmed_at)
             VALUES ('TestPlayer', 'Dreva', 1001, 'Combat', 7, '2026-10-15 07:00:01')",
            [],
        ).unwrap();

        // The alt has no `user_characters` row: without the chat tier it borrows
        // the selected character's server (`Dreva`) and replaces its rows.
        assert_eq!(
            apply_catch_up_login(&mut gs, &pool, &map, "OtherAlt", true),
            SessionServerSource::ChatLog
        );
        pool.get_write().unwrap().execute(
            "INSERT INTO game_state_skills
             (character_name, server_name, skill_id, skill_name, level, last_confirmed_at)
             VALUES ('OtherAlt', 'OtherAltServer', 1001, 'Combat', 99, '2026-10-15 07:05:01')",
            [],
        ).unwrap();

        let level = |character: &str, server: &str| -> Option<i64> {
            pool.get()
                .unwrap()
                .query_row(
                    "SELECT level FROM game_state_skills
                     WHERE character_name = ?1 AND server_name = ?2 AND skill_name = 'Combat'",
                    rusqlite::params![character, server],
                    |row| row.get(0),
                )
                .ok()
        };
        assert_eq!(level("TestPlayer", "Dreva"), Some(7));
        assert_eq!(level("OtherAlt", "OtherAltServer"), Some(99));
        assert_eq!(
            level("OtherAlt", "Dreva"),
            None,
            "no row under the borrowed server"
        );

        // A repeated login line for the same character resolves the same way.
        assert_eq!(
            apply_catch_up_login(&mut gs, &pool, &map, "OtherAlt", true),
            SessionServerSource::ChatLog
        );
        assert_eq!(gs.get_active_server(), Some("OtherAltServer"));

        // Drop the pool first: Windows won't delete a file with an open handle.
        drop(pool);
        let _ = std::fs::remove_file(&db_path);
        let _ = std::fs::remove_dir_all(&chat_dir);
    }

    #[test]
    fn catch_up_login_clears_the_adopted_inventory_slice() {
        let (pool, db_path) = seed_pool(PICKER_ROW);
        let mut gs = game_state("TestPlayer", "Dreva");

        let seed_inventory = |character: &str, instance_id: i64| {
            pool.get_write()
                .unwrap()
                .execute(
                    "INSERT INTO game_state_inventory
                     (character_name, server_name, instance_id, item_name, stack_size, last_confirmed_at, source)
                     VALUES (?1, 'Dreva', ?2, 'Fluorite', 1, '2026-10-19 12:00:00', 'log')",
                    rusqlite::params![character, instance_id],
                )
                .unwrap()
        };
        seed_inventory("TestPlayer", 1);
        seed_inventory("OtherAlt", 2);

        // Adopting OtherAlt's session clears only its slice; the replay burst
        // (`is_new=false` upserts + UpdateItemCode baselines) repopulates it.
        let map = std::collections::HashMap::from([("OtherAlt".to_string(), "Dreva".to_string())]);
        apply_catch_up_login(&mut gs, &pool, &map, "OtherAlt", true);

        let count: i64 = pool
            .get()
            .unwrap()
            .query_row(
                "SELECT COUNT(*) FROM game_state_inventory
                 WHERE character_name = 'OtherAlt' AND server_name = 'Dreva'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 0, "adopted slice cleared");

        let count: i64 = pool
            .get()
            .unwrap()
            .query_row(
                "SELECT COUNT(*) FROM game_state_inventory
                 WHERE character_name = 'TestPlayer' AND server_name = 'Dreva'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1, "other character untouched");

        // Drop the pool first: Windows won't delete a file with an open handle.
        drop(pool);
        let _ = std::fs::remove_file(&db_path);
    }
    #[test]
    fn catch_up_login_preserves_snapshot_inventory() {
        let (pool, db_path) = seed_pool(PICKER_ROW);
        let mut gs = game_state("TestPlayer", "Dreva");

        // Same slice, two provenances: an obsolete log-derived row and a
        // saved snapshot row (what report imports write).
        for (instance_id, source) in [(1, "log"), (2, "snapshot")] {
            pool.get_write()
                .unwrap()
                .execute(
                    "INSERT INTO game_state_inventory
                     (character_name, server_name, instance_id, item_name, stack_size, last_confirmed_at, source)
                     VALUES ('OtherAlt', 'Dreva', ?1, 'Fluorite', 1, '2026-10-19 12:00:00', ?2)",
                    rusqlite::params![instance_id, source],
                )
                .unwrap();
        }

        let map = std::collections::HashMap::from([("OtherAlt".to_string(), "Dreva".to_string())]);
        apply_catch_up_login(&mut gs, &pool, &map, "OtherAlt", true);

        let remaining: Vec<String> = pool
            .get()
            .unwrap()
            .prepare(
                "SELECT source FROM game_state_inventory
                 WHERE character_name = 'OtherAlt' AND server_name = 'Dreva'",
            )
            .unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(remaining, vec!["snapshot".to_string()], "snapshot survives adopt");

        drop(pool);
        let _ = std::fs::remove_file(&db_path);
    }

    #[test]
    fn catch_up_login_unknown_character_keeps_the_server_in_use() {
        let (pool, db_path) = seed_pool(PICKER_ROW);
        let mut gs = game_state("TestPlayer", "Dreva");

        let source = apply_catch_up_login(
            &mut gs,
            &pool,
            &std::collections::HashMap::new(),
            "Ghost",
            true,
        );

        assert_eq!(source, SessionServerSource::None);
        assert_eq!(gs.get_active_server(), Some("Dreva"));
        assert_eq!(gs.get_active_character(), Some("Ghost"));

        drop(pool);
        let _ = std::fs::remove_file(&db_path);
    }

    #[test]
    fn catch_up_login_never_borrows_another_characters_marker() {
        // The alt has a row (and the scan knows its server); TestPlayer has none.
        let (pool, db_path) = seed_pool(
            "INSERT INTO user_characters (character_name, server_name, source, is_active)
             VALUES ('OtherAlt', 'OtherAltServer', 'login', 0)",
        );
        let mut gs = game_state("TestPlayer", "Dreva");
        // TestPlayer must not inherit the only entry in the map.
        let map = std::collections::HashMap::from([(
            "OtherAlt".to_string(),
            "OtherAltServer".to_string(),
        )]);

        let source = apply_catch_up_login(&mut gs, &pool, &map, "TestPlayer", true);

        assert_eq!(source, SessionServerSource::None);
        assert_eq!(gs.get_active_server(), Some("Dreva"));

        drop(pool);
        let _ = std::fs::remove_file(&db_path);
    }

    #[test]
    fn drain_identity_logout_drops_trailing_untagged_keeps_leading() {
        // The drain's per-line sequence through the extracted routing, on a
        // real `GameStateManager`: a leading untagged line resolves
        // pre-batch, the owning logout clears the pair, and a trailing
        // untagged line drops instead of inheriting the stale session.
        use crate::chat_parser::ChatSessionTag;

        let key = |id: Option<DispatchIdentity>| id.map(|i| (i.character, i.server));
        let pair_of = |gs: &GameStateManager| match (gs.get_active_character(), gs.get_active_server()) {
            (Some(c), Some(s)) => Some(DispatchIdentity {
                character: c.to_string(),
                server: s.to_string(),
            }),
            _ => None,
        };

        let mut gs = game_state("Zenith", "Dreva");
        let preceding = pair_of(&gs);
        // Leading untagged line, ahead of any marker: the pre-batch pair.
        assert_eq!(
            key(resolve_chat_drain_identity(None, false, preceding.as_ref(), preceding.clone())),
            Some(("Zenith".to_string(), "Dreva".to_string()))
        );
        // A tagged line carries its own session even after mutation.
        let alt = ChatSessionTag {
            character_name: "AltB".to_string(),
            server_name: "Legacy".to_string(),
        };
        assert_eq!(
            key(resolve_chat_drain_identity(Some(&alt), true, preceding.as_ref(), None)),
            Some(("AltB".to_string(), "Legacy".to_string()))
        );

        // The owning logout clears the pair (the drain then marks mutated).
        assert!(apply_chat_logout_identity(&mut gs, "Zenith"));
        assert_eq!(gs.get_active_server(), None);

        // Trailing untagged line: the pre-batch pair must not leak back in
        // — live is now `None`, so the line resolves to `None` (drop).
        let live = pair_of(&gs);
        assert!(live.is_none());
        assert_eq!(key(resolve_chat_drain_identity(None, true, preceding.as_ref(), live)), None);

        // A foreign logout never wipes the live identity.
        let mut gs2 = game_state("Zenith", "Dreva");
        assert!(!apply_chat_logout_identity(&mut gs2, "AltB"));
        assert_eq!(gs2.get_active_server(), Some("Dreva"));
    }
}

#[cfg(test)]
mod bind_freshness_tests {
    use super::bind_capture_is_fresh;
    use chrono::NaiveDateTime;

    fn dt(s: &str) -> Option<NaiveDateTime> {
        NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S").ok()
    }

    #[test]
    fn older_capture_does_not_clobber_newer_bind() {
        // Stored bind observed 12:10; replayed book from 12:00 loses.
        assert!(!bind_capture_is_fresh(
            Some("2026-11-18T12:10:00+00:00"),
            dt("2026-11-18 12:00:00"),
        ));
    }

    #[test]
    fn newer_capture_overwrites_and_empty_table_fills() {
        assert!(bind_capture_is_fresh(
            Some("2026-11-18T12:00:00+00:00"),
            dt("2026-11-18 12:10:00"),
        ));
        assert!(bind_capture_is_fresh(None, dt("2026-11-18 12:10:00")));
        // Same-instant re-capture is idempotent, not a clobber.
        assert!(bind_capture_is_fresh(
            Some("2026-11-18T12:10:00+00:00"),
            dt("2026-11-18 12:10:00"),
        ));
    }

    #[test]
    fn untimestamped_source_only_fills_empty_table() {
        assert!(bind_capture_is_fresh(None, None));
        assert!(!bind_capture_is_fresh(Some("2026-11-18T12:10:00+00:00"), None));
    }

    #[test]
    fn unparseable_stored_stamp_never_blocks() {
        assert!(bind_capture_is_fresh(Some("not-a-timestamp"), dt("2026-11-18 12:00:00")));
    }
}

#[cfg(test)]
mod book_content_recency_tests {
    use super::upsert_book_content;

    fn setup() -> rusqlite::Connection {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        crate::db::migrations::run_migrations(&conn, None).unwrap();
        conn
    }

    fn read_book(conn: &rusqlite::Connection) -> (String, String) {
        conn.query_row(
            "SELECT content, captured_at FROM game_state_books
             WHERE character_name = 'T' AND server_name = 'Dreva'
               AND book_type = 'PlayerAge' AND title = 'PlayerAge_261118_121000'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap()
    }

    #[test]
    fn older_book_after_newer_is_noop() {
        let conn = setup();
        assert!(upsert_book_content(
            &conn,
            "T",
            "Dreva",
            "PlayerAge",
            "PlayerAge_261118_121000",
            "newer body",
            "2026-11-18T12:10:00+00:00",
        )
        .unwrap());
        // An older capture over a newer row: no write, content and stamp kept.
        assert!(!upsert_book_content(
            &conn,
            "T",
            "Dreva",
            "PlayerAge",
            "PlayerAge_261118_121000",
            "older body",
            "2026-11-18T12:00:00+00:00",
        )
        .unwrap());
        assert_eq!(
            read_book(&conn),
            (
                "newer body".to_string(),
                "2026-11-18T12:10:00+00:00".to_string()
            )
        );
    }

    #[test]
    fn identical_reread_writes_nothing_and_keeps_stamp() {
        let conn = setup();
        assert!(upsert_book_content(
            &conn,
            "T",
            "Dreva",
            "PlayerAge",
            "PlayerAge_261118_121000",
            "same body",
            "2026-11-18T12:10:00+00:00",
        )
        .unwrap());
        // Identical content re-observed later: no write, stamp stays at the
        // first observation instead of advancing.
        assert!(!upsert_book_content(
            &conn,
            "T",
            "Dreva",
            "PlayerAge",
            "PlayerAge_261118_121000",
            "same body",
            "2026-11-18T12:20:00+00:00",
        )
        .unwrap());
        assert_eq!(
            read_book(&conn),
            (
                "same body".to_string(),
                "2026-11-18T12:10:00+00:00".to_string()
            )
        );
    }

    #[test]
    fn equal_capture_retry_replaces() {
        // Same retry-eligibility rule as the garden importer: an equal
        // capture with different content still replaces.
        let conn = setup();
        assert!(upsert_book_content(
            &conn,
            "T",
            "Dreva",
            "PlayerAge",
            "PlayerAge_261118_121000",
            "first body",
            "2026-11-18T12:10:00+00:00",
        )
        .unwrap());
        assert!(upsert_book_content(
            &conn,
            "T",
            "Dreva",
            "PlayerAge",
            "PlayerAge_261118_121000",
            "second body",
            "2026-11-18T12:10:00+00:00",
        )
        .unwrap());
        assert_eq!(read_book(&conn).0, "second body".to_string());
    }

    #[test]
    fn newer_book_after_older_applies() {
        // Twin arrival order, forward direction: older first, then newer —
        // the newer body lands with the newer stamp.
        let conn = setup();
        assert!(upsert_book_content(
            &conn,
            "T",
            "Dreva",
            "PlayerAge",
            "PlayerAge_261118_121000",
            "older body",
            "2026-11-18T12:00:00+00:00",
        )
        .unwrap());
        assert!(upsert_book_content(
            &conn,
            "T",
            "Dreva",
            "PlayerAge",
            "PlayerAge_261118_121000",
            "newer body",
            "2026-11-18T12:10:00+00:00",
        )
        .unwrap());
        assert_eq!(
            read_book(&conn),
            (
                "newer body".to_string(),
                "2026-11-18T12:10:00+00:00".to_string()
            )
        );
    }
}

#[cfg(test)]
mod logout_identity_tests {
    use super::logout_clears_identity;

    #[test]
    fn owning_session_clears_foreign_logout_does_not() {
        assert!(logout_clears_identity(Some("Zenith"), "Zenith"));
        assert!(!logout_clears_identity(Some("Zenith"), "AltB"));
        assert!(!logout_clears_identity(None, "Zenith"));
    }
}
