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

/// A chat `[Status] ItemGained` gain seen during catch-up, keyed by the
/// login session that produced the line (`ChatSessionTag`, re-derived
/// per line — not the drain-final identity), except a mid-session resume
/// (`None` tag), which falls back to the drain identity. Applied to the player
/// rows via `GameStateManager::correct_stack_for_session` just before the
/// live flip, because the Player.log drain already wrote fallback `stack_size
/// = 1` rows before the chat drain ran.
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

struct StagedChatGain {
    character: String,
    server: String,
    item_name: String,
    quantity: u32,
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
    /// Rez dedup: last persisted successful rez per target as (timestamp,
    /// caster). The game emits "X resuscitates Y" (named caster, pet rezzes)
    /// and "Y comes back to life!" (caster "Unknown", player/group rezzes)
    /// in different contexts; if both phrasings ever fire for the same rez
    /// (same target, seconds apart) only the first row is kept — same-phrasing
    /// repeats always record (a rapid re-rez is a genuine second death).
    /// Keyed PER TARGET (not a single last-tuple): interleaved rezzes of
    /// different targets within the window each still dedup (`rez A`,
    /// `rez B`, `A back`, `B back`). Timestamps are compared with a rolling
    /// 30-second window (`rez_within_dedup_window`), not minute buckets —
    /// a pair straddling a minute boundary is still one rez. Pruned to a
    /// bounded window below so it never grows unbounded.
    last_rez_dedup: std::collections::HashMap<String, (String, String)>,
    /// `character -> server` scanned from the chat logs at the start of the
    /// catch-up window (`refresh_catch_up_session_servers`); cleared once live.
    catch_up_chat_servers: std::collections::HashMap<String, String>,
    /// Whether `catch_up_chat_servers` was built for this catch-up window.
    /// One scan per window, not one per non-live poll: a poll that fails before
    /// reaching `set_live_mode` would otherwise re-read every `Chat-*.log` on
    /// the 500ms polling thread, holding the coordinator lock each time.
    catch_up_servers_scanned: bool,
    /// Chat stack gains staged during catch-up (see `StagedChatGain`); applied
    /// just before the live flip. Cleared once live.
    staged_chat_gains: Vec<StagedChatGain>,
    /// Skill updates skipped because the skill name did not resolve against
    /// current game data (new game skill or stale CDN). Monotonic within the
    /// process; logged with each skip so a growing total signals staleness.
    /// The row is deliberately not written (skill_id 0 would collide PKs).
    skipped_unknown_skills: u64,
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
        let mut game_state = GameStateManager::new(game_data);
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
            last_rez_dedup: std::collections::HashMap::new(),
            catch_up_chat_servers: std::collections::HashMap::new(),
            catch_up_servers_scanned: false,
            staged_chat_gains: Vec::new(),
            skipped_unknown_skills: 0,
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

    /// Apply chat stack gains staged during catch-up to their own sessions.
    /// Runs once, as the last step of the catch-up window (after the chat
    /// drain, before `set_live_mode`): the Player.log drain already wrote the
    /// rows, and the staged keys name the login session that produced each gain
    /// (mid-session resume gains excepted — see `StagedChatGain`).
    fn apply_staged_chat_corrections(&mut self) {
        let staged = std::mem::take(&mut self.staged_chat_gains);
        if staged.is_empty() {
            return;
        }
        let mut domains: Vec<&str> = Vec::new();
        for gain in &staged {
            domains.extend(self.game_state.correct_stack_for_session(
                &gain.character,
                &gain.server,
                &gain.item_name,
                gain.quantity,
                &self.db_pool,
            ));
        }
        domains.sort();
        domains.dedup();
        if !domains.is_empty() {
            self.app_handle.emit("game-state-updated", &domains).ok();
        }
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
            self.apply_staged_chat_corrections();
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
                            self.settings.update(settings).ok();
                            self.game_state.set_active_character_name(&character_name);
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

        // Get timezone offset for converting Chat.log local timestamps to UTC
        let tz_offset = self
            .settings
            .get()
            .manual_timezone_override
            .or(self.settings.get().timezone_offset_seconds)
            .unwrap_or(0);

        for event in events {
            match event {
                LogEvent::ChatMessage(msg, session_tag) => {
                    // Capture the local wall-clock timestamp BEFORE the UTC
                    // conversion below — Combat Wisdom / arena / poop dedup
                    // keys use local time to stay consistent with the chat-log
                    // backfill (which never converts), so live + backfill
                    // dedup correctly.
                    let local_ts = msg.timestamp.format("%Y-%m-%d %H:%M:%S").to_string();

                    // Convert chat timestamp from local time to UTC
                    let mut msg = msg;
                    msg.timestamp = chat_local_to_utc(msg.timestamp, tz_offset);

                    // Catch-up vs live: the Player.log drain already wrote
                    // fallback `stack_size = 1` rows, and no future
                    // `ProcessAddItem` will consult the parser buffer — so
                    // stage gains session-keyed for
                    // `apply_staged_chat_corrections` instead of
                    // feeding/correcting now under a drain-order identity.
                    // The tag is the login session that produced the line;
                    // only when the file was resumed mid-session (`None`) do
                    // we fall back to the drain identity. Live resolves the
                    // same way: a poll batch can span a login boundary (the
                    // watcher sorts the login marker first), so a pre-login
                    // line must keep its own tag instead of inheriting the
                    // new active identity. Tag-less live lines
                    // (resume-from-offset polls) use the active identity.
                    let is_live = self.game_state.is_live();
                    let identity: Option<DispatchIdentity> = session_tag
                        .as_ref()
                        .map(|tag| DispatchIdentity {
                            character: tag.character_name.clone(),
                            server: tag.server_name.clone(),
                        })
                        .or_else(|| {
                            match (
                                self.game_state.get_active_character(),
                                self.game_state.get_active_server(),
                            ) {
                                (Some(c), Some(s)) => Some(DispatchIdentity {
                                    character: c.to_string(),
                                    server: s.to_string(),
                                }),
                                _ => None,
                            }
                        });

                    // The shared dispatch owns every idempotent DB arm. Two
                    // live-only follow-ups stay here because they need
                    // `&mut self` borrows the dispatch does not take:
                    // (1) stack correction inline (live) vs session-keyed
                    // (catch-up), (2) survey attribution through the live
                    // aggregator's gating state.
                    let mut outcome = {
                        let mut dstate = DispatchState {
                            arena_tracker: &mut self.arena_tracker,
                            last_rez_dedup: &mut self.last_rez_dedup,
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
                            },
                            None,
                            &mut dstate,
                        )
                    };

                    // (1+3) Correlation feed + survey attribution per gain.
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
                        if is_live {
                            // Correct the gain's own session slice: the
                            // dispatch above wrote under `identity`
                            // (tag-preferred), so the correction must too —
                            // a B-login-then-A-gain poll would otherwise fix
                            // B's placeholder for A's gain. Matches active
                            // when tag and active agree; `None` corrects
                            // nothing (active is also `None` then).
                            let corrected = match identity.as_ref() {
                                Some(id) => self.game_state.correct_stack_for_session(
                                    &id.character,
                                    &id.server,
                                    &item_name,
                                    quantity,
                                    &self.db_pool,
                                ),
                                None => vec![],
                            };
                            if !corrected.is_empty() {
                                self.app_handle.emit("game-state-updated", &corrected).ok();
                            }
                        } else if let Some(id) = identity.as_ref() {
                            // Catch-up: the Player.log drain already wrote
                            // fallback rows — stage session-keyed for
                            // `apply_staged_chat_corrections`.
                            self.staged_chat_gains.push(StagedChatGain {
                                character: id.character.clone(),
                                server: id.server.clone(),
                                item_name: item_name.clone(),
                                quantity,
                            });
                        }
                        // (3) Survey, live loot gains only: attribute the
                        // just-written ledger row, then tag it — mirrors the
                        // old inline arm. Summoned gains never attribute.
                        // One write transaction for the qty bump + row tag: if
                        // the tag fails, the bump rolls back with it, leaving
                        // the row untagged with no qty behind it (a later
                        // rescan or retro-claim heals cleanly) instead of a
                        // bumped qty on an untagged row that a later claim
                        // would double-count. Same unchecked-transaction shape
                        // as the dispatch arms: fresh pool checkout, no outer
                        // transaction open here.
                        if is_live {
                            let is_loot = matches!(
                                crate::chat_status_parser::parse_status_message(&msg),
                                Some(ChatStatusEvent::ItemGained { .. })
                            ) || msg.json_type.as_deref() == Some("AddItem");
                            if is_loot
                                && let (Some(id), Ok(conn)) =
                                    (identity.as_ref(), self.db_pool.get_write())
                            {
                                let tx = match rusqlite::Transaction::new_unchecked(
                                    &conn,
                                    rusqlite::TransactionBehavior::Immediate,
                                ) {
                                    Ok(tx) => tx,
                                    Err(e) => {
                                        eprintln!("[coordinator] survey attr txn begin failed: {e}");
                                        continue;
                                    }
                                };
                                let use_id = match self.survey_aggregator.attribute_chat_gain(
                                    &tx,
                                    &id.character,
                                    &id.server,
                                    internal_name.as_deref(),
                                    quantity,
                                    &ts_utc,
                                    Some(item_name.as_str()),
                                ) {
                                    Some(use_id) => use_id,
                                    None => continue,
                                };
                                let source_details =
                                    format!("{{\"survey_use_id\":{use_id}}}");
                                let tag_rows = match tx.execute(
                                    "UPDATE item_transactions
                                        SET source_kind = 'survey_chat',
                                            source_details = ?1
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
                                        source_details,
                                        ts_utc,
                                        id.character,
                                        id.server,
                                        item_name,
                                        quantity as i64,
                                    ],
                                ) {
                                    Ok(n) => n,
                                    // A failed tag must not abort the whole
                                    // batch via `?`: roll back this gain and
                                    // move on so the remaining gains still
                                    // process (same shape as the zero-match
                                    // arm below).
                                    Err(e) => {
                                        eprintln!(
                                            "[coordinator] survey row tag failed: {e}, rolled back"
                                        );
                                        continue;
                                    }
                                };
                                // The tag must land on exactly one untagged
                                // ledger row: zero means the row vanished (or
                                // never matched) between checkouts — commit
                                // would bump a use with no provenance, so roll
                                // back and let a later pass reclaim the gain.
                                if tag_rows != 1 {
                                    eprintln!(
                                        "[coordinator] survey row tag matched {tag_rows} rows, rolled back"
                                    );
                                    continue;
                                }
                                if tx.commit().is_err() {
                                    eprintln!("[coordinator] survey attr txn commit failed, rolled back");
                                    continue;
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
                    if !outcome.unknown_skills.is_empty() {
                        self.skipped_unknown_skills += outcome.unknown_skills.len() as u64;
                        for skill in &outcome.unknown_skills {
                            eprintln!(
                                "[coordinator] Skipping chat level-up for unresolvable skill: {skill}"
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
                            self.ingest_teleportation_binds(&body, character, server);
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
                        if book_type == "PlayerAge" || book_type == "HelpScreen" {
                            self.ingest_report_stats(&book_type, &body, character, server);
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

                    // Emit to frontend
                    self.app_handle.emit("server-detected", &server_name).ok();
                }
                LogEvent::CharacterLogin { character_name, timestamp, .. } => {
                    // Chat log also detects character login — update active character
                    // and emit so player_watcher can stay in sync
                    self.app_handle
                        .emit("character-login", &character_name)
                        .ok();

                    // Auto-register character with current server
                    if let Ok(conn) = self.db_pool.get_write() {
                        conn.execute(
                            "INSERT INTO user_characters (character_name, server_name, source, last_login_time)
                             VALUES (?1, COALESCE(?2, 'Unknown'), 'login', CURRENT_TIMESTAMP)
                             ON CONFLICT(character_name, server_name) DO UPDATE SET
                                last_login_time = CURRENT_TIMESTAMP,
                                updated_at = CURRENT_TIMESTAMP",
                            rusqlite::params![
                                character_name,
                                self.settings.get().active_server_name,
                            ],
                        ).ok();
                    }

                    // Update active character in settings
                    let mut settings = self.settings.get();
                    settings.active_character_name = Some(character_name.clone());
                    let server = settings
                        .active_server_name
                        .clone()
                        .unwrap_or_else(|| "Unknown".to_string());
                    self.settings.update(settings).ok();

                    self.game_state
                        .set_active_character(&character_name, &server, &self.db_pool, Some(&timestamp.format("%Y-%m-%d %H:%M:%S").to_string()));
                }
                _ => {}
            }
        }

        // Batch insert messages (watch rules already evaluated per message
        // through the shared dispatch, which forwards what fired).
        if !messages.is_empty() {
            let conn = self
                .db_pool
                .get_write()
                .map_err(|e| format!("Database error: {}", e))?;

            // Use the actual file name from the watcher, not the position
            let log_file = self
                .chat_watcher
                .as_ref()
                .map(|w| w.get_file_name().to_string())
                .unwrap_or_else(|| "unknown".to_string());

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
    fn ingest_report_stats(&self, book_type: &str, content: &str, character: &str, server: &str) {
        ingest_report_stats_content(
            &self.db_pool,
            &self.app_handle,
            character,
            server,
            book_type,
            content,
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
    fn ingest_teleportation_binds(&self, content: &str, character: &str, server: &str) {
        // Live ProcessBook content carries escaped newlines; the file path
        // carries real ones. Normalize once here so `extract_bind_field`
        // sees real line breaks on both paths (historically the live path
        // silently matched no fields).
        let content = normalize_book_content(content);
        if let Err(e) = ingest_teleport_binds_content(
            &self.db_pool,
            &self.app_handle,
            character,
            server,
            &content,
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
/// Swallows SQL errors to keep ingest flowing, but returns `Err` on pool
/// acquisition failure so callers can log it.
pub fn persist_book_content(
    db: &DbPool,
    app_handle: &AppHandle,
    character: &str,
    server: &str,
    book_type: &str,
    title: &str,
    content: &str,
) -> Result<(), String> {
    let dt = chrono::Utc::now().to_rfc3339();
    let conn = db
        .get_write()
        .map_err(|e| format!("Database connection error: {e}"))?;
    conn.execute(
        "INSERT INTO game_state_books (character_name, server_name, book_type, title, content, captured_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)
         ON CONFLICT(character_name, server_name, book_type, title) DO UPDATE SET
            content = excluded.content,
            captured_at = excluded.captured_at
         WHERE captured_at <= excluded.captured_at",
        rusqlite::params![character, server, book_type, title, content, dt],
    )
    .map_err(|e| format!("Failed to persist book: {e}"))?;
    app_handle.emit("game-state-updated", vec!["books"]).ok();
    Ok(())
}

/// Parse structured stats from a PlayerAge or Behavior Report (HelpScreen)
/// book and persist them to `character_report_stats`. Shared by the live
/// coordinator and the Books-directory watcher.
pub fn ingest_report_stats_content(
    db: &DbPool,
    app_handle: &AppHandle,
    character: &str,
    server: &str,
    book_type: &str,
    content: &str,
) -> Result<usize, String> {
    let stats = match book_type {
        "PlayerAge" => crate::report_stats::parse_player_age(content),
        "HelpScreen" => crate::report_stats::parse_behavior_report(content),
        _ => return Ok(0),
    };
    if stats.is_empty() {
        return Ok(0);
    }
    let dt = chrono::Utc::now().to_rfc3339();
    let conn = db
        .get_write()
        .map_err(|e| format!("Failed to persist report stats: {e}"))?;
    let n = crate::report_stats::persist_stats(&conn, character, server, &stats, &dt)
        .map_err(|e| format!("Failed to persist report stats: {e}"))?;
    startup_log!(
        "[coordinator] Imported {} stats from {} report",
        n,
        book_type,
    );
    app_handle
        .emit("game-state-updated", vec!["report_stats"])
        .ok();
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
pub fn ingest_teleport_binds_content(
    db: &DbPool,
    app_handle: &AppHandle,
    character: &str,
    server: &str,
    content: &str,
) -> Result<(), String> {
    let primary = extract_bind_field(content, "Primary Bind Location:");
    let secondary = extract_bind_field(content, "Secondary Bind Location:");

    // Upsert MUST run on the writer connection: the read pool is
    // PRAGMA query_only, so `get()` here silently dropped every bind
    // update (execute failure swallowed by `.ok()`).
    let conn = db.get_write().map_err(|e| format!("Database error: {e}"))?;

    let dt = chrono::Utc::now().to_rfc3339();
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
        // The adopted session rebuilds its own inventory (report imports),
        // so start it from an empty slice — otherwise rows from an earlier
        // session pollute this one. Equipment/combat/mount/favor are
        // deliberately untouched: equipment self-heals via its full-dump
        // reinsert, and the rest have no repopulating source.
        if clear_inventory
            && let (Some(character), Ok(conn)) = (
                game_state.get_active_character().map(str::to_string),
                db.get_write(),
            )
                && let Err(e) = conn.execute(
                    "DELETE FROM game_state_inventory WHERE character_name = ?1 AND server_name = ?2",
                    rusqlite::params![character, server],
                ) {
                    startup_log!("[coordinator] clear-on-adopt inventory delete failed: {e}");
                }
    }
    source
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



/// True when two rez phrasings fall within the symmetric rez-dedup window
/// (`"YYYY-MM-DD HH:MM:SS"` UTC strings, zero-padded so lexicographic
/// order equals chronological order). The game emits both rez phrasings
/// for one event seconds apart — but never instantly, so the window
/// covers the pair while staying far below any plausible re-rez cooldown.
/// Backward pairs within the window also suppress (out-of-order replay /
/// clock skew); re-rez beyond 30 s either direction persists.
/// Malformed timestamps never match (fail-open: the event is persisted).
pub(crate) fn rez_within_dedup_window(earlier: &str, later: &str) -> bool {
    const WINDOW: chrono::Duration = chrono::Duration::seconds(30);
    match (
        chrono::NaiveDateTime::parse_from_str(earlier, "%Y-%m-%d %H:%M:%S"),
        chrono::NaiveDateTime::parse_from_str(later, "%Y-%m-%d %H:%M:%S"),
    ) {
        (Ok(prev), Ok(cur)) => {
            // Symmetric: either direction within the window is the same pair.
            (cur - prev).abs() <= WINDOW
        }
        _ => false,
    }
}

#[cfg(test)]
mod rez_dedup_tests {
    use super::rez_within_dedup_window as within;

    #[test]
    fn paired_phrasing_window() {
        // Pair from a single rez: seconds apart → suppressed.
        assert!(within("2026-10-04 12:34:56", "2026-10-04 12:34:59"));
        // Exactly at the window edge (30s) is still within it.
        assert!(within("2026-10-04 12:34:56", "2026-10-04 12:35:26"));
    }

    #[test]
    fn boundary_straddle_is_one_rez() {
        // Regression: minute buckets suppressed these; the rolling window
        // must keep treating a :59→:01 straddle as one rez.
        assert!(within("2026-10-04 12:34:59", "2026-10-04 12:35:01"));
        assert!(within("2026-10-04 23:59:59", "2026-10-05 00:00:10"));
    }

    #[test]
    fn legitimate_re_rez_is_kept() {
        // A real second rez of the same target is outside the window,
        // either direction.
        assert!(!within("2026-10-04 12:34:56", "2026-10-04 12:35:27"));
        assert!(!within("2026-10-04 12:34:56", "2026-10-04 12:35:31"));
        assert!(!within("2026-10-04 12:35:27", "2026-10-04 12:34:56"));
    }

    #[test]
    fn distinct_events_and_malformed() {
        // Different day/hour entirely.
        assert!(!within("2026-10-04 12:34:56", "2026-10-05 12:34:56"));
        // Backward timestamps within the window also dedup (out-of-order
        // replay, clock skew): same pair, reversed order.
        assert!(within("2026-10-04 12:35:00", "2026-10-04 12:34:59"));
        // Malformed/short timestamps never match (fail-open).
        assert!(!within("2026-10-04", "2026-10-04 12:34:56"));
        assert!(!within("", "2026-10-04 12:34:56"));
        assert!(!within("garbage", "2026-10-04 12:34:56"));
    }
}

/// Server resolution for a Player.log session: the `user_characters` row when the
/// character has one, else its own chat login line (see `apply_catch_up_login`).
#[cfg(test)]
mod session_identity_tests {
    use super::{
        apply_catch_up_login, chat_session_servers, session_server_for, SessionServerLookup,
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
        use std::sync::Arc;

        let mut gs = GameStateManager::new(Arc::new(tauri::async_runtime::RwLock::new(
            crate::game_data::GameData::empty(),
        )));
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
}
