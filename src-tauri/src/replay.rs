/// Dual-log replay — simulates live tailing by interleaving Player.log and Chat.log
/// events by timestamp, processing them through the same coordinator pipelines.
///
/// This enables cross-referencing between the two log streams (e.g., correcting
/// motherlode loot quantities from Chat.log [Status] messages) using archived logs.
use std::fs::File;
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use tauri::{AppHandle, Emitter, Manager};

use crate::cdn_commands::GameDataState;
use crate::chat_parser::{
    is_timestamped_line, parse_chat_line, parse_chat_login_line, ChatMessage,
};
use crate::chat_status_parser::parse_status_message;
use crate::db::DbPool;
use crate::game_state::GameStateManager;
use crate::parsers::{chat_local_to_utc, parse_skill_update, parse_timestamp};
use crate::player_event_parser::{PlayerEvent, PlayerEventParser};
use crate::survey::aggregator::SurveySessionAggregator;

/// A timestamped event from either log source, used for interleaving.
#[derive(Debug)]
enum TimedEvent {
    /// Events from Player.log (processed first within a second)
    PlayerLine {
        /// UTC second (for ordering)
        utc_second: i64,
        /// The raw log line
        line: String,
    },
    /// A chat message from Chat.log
    ChatMessage {
        /// UTC second (for ordering)
        utc_second: i64,
        msg: ChatMessage,
    },
    /// Login detected from Chat.log — carries timezone offset
    ChatLogin {
        /// UTC second (for ordering)
        utc_second: i64,
        server_name: String,
        character_name: String,
        timezone_offset_seconds: Option<i32>,
    },
}

impl TimedEvent {
    fn utc_second(&self) -> i64 {
        match self {
            TimedEvent::PlayerLine { utc_second, .. } => *utc_second,
            TimedEvent::ChatMessage { utc_second, .. } => *utc_second,
            TimedEvent::ChatLogin { utc_second, .. } => *utc_second,
        }
    }

    /// Sort key: (utc_second, source_order)
    /// source_order: 0 = ChatLogin (timezone must come first), 1 = PlayerLine, 2 = ChatMessage
    fn sort_key(&self) -> (i64, u8) {
        match self {
            TimedEvent::ChatLogin { utc_second, .. } => (*utc_second, 0),
            TimedEvent::PlayerLine { utc_second, .. } => (*utc_second, 1),
            TimedEvent::ChatMessage { utc_second, .. } => (*utc_second, 2),
        }
    }
}

/// Progress event emitted to the frontend during replay.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ReplayProgress {
    pub phase: String,
    pub current: usize,
    pub total: usize,
    pub detail: String,
}

/// Replay result summary.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ReplayResult {
    pub player_lines_processed: usize,
    pub chat_messages_processed: usize,
    pub player_events_emitted: usize,
    pub chat_status_events_emitted: usize,
}

/// Parse Player.log into timestamped lines.
///
/// Player.log timestamps are local time `[HH:MM:SS]` with no date.
/// Player.log timestamps are already UTC with no date. We derive the date from
/// the chat log filename or fall back to today's UTC date.
fn parse_player_log_lines(
    path: &PathBuf,
    base_date: chrono::NaiveDate,
) -> Result<Vec<TimedEvent>, String> {
    let file = File::open(path).map_err(|e| format!("Failed to open Player.log: {}", e))?;
    let reader = BufReader::new(file);
    let mut events = Vec::new();

    for line in reader.lines() {
        let line = line.map_err(|e| format!("Read error: {}", e))?;
        let line = line.trim_end().to_string();
        if line.is_empty() {
            continue;
        }

        // Extract [HH:MM:SS] timestamp — already UTC
        if let Some(ts_str) = parse_timestamp(&line) {
            if let Ok(utc_time) = chrono::NaiveTime::parse_from_str(&ts_str, "%H:%M:%S") {
                let utc_dt = base_date.and_time(utc_time);
                let utc_second = utc_dt.and_utc().timestamp();

                events.push(TimedEvent::PlayerLine { utc_second, line });
            }
        }
        // Lines without timestamps (login announcements, etc.) get appended
        // with the same second as the previous event
        else if !events.is_empty() {
            let prev_second = events.last().unwrap().utc_second();
            events.push(TimedEvent::PlayerLine {
                utc_second: prev_second,
                line,
            });
        }
    }

    Ok(events)
}

/// Parse Chat.log into timestamped events.
/// Also extracts login lines for timezone/server detection.
fn parse_chat_log_events(path: &PathBuf) -> Result<Vec<TimedEvent>, String> {
    let mut file = File::open(path).map_err(|e| format!("Failed to open Chat.log: {}", e))?;
    let mut content = String::new();
    file.read_to_string(&mut content)
        .map_err(|e| format!("Failed to read Chat.log: {}", e))?;

    let mut events = Vec::new();

    for line in content.lines() {
        if line.trim().is_empty() {
            continue;
        }

        // Check for login line first (carries timezone offset)
        if let Some(info) = parse_chat_login_line(line) {
            // Login lines have a timestamp too — extract it
            let utc_second = if is_timestamped_line(line) {
                if let Some(msg) = parse_chat_line(line) {
                    msg.timestamp.and_utc().timestamp()
                } else {
                    0
                }
            } else {
                0
            };

            events.push(TimedEvent::ChatLogin {
                utc_second,
                server_name: info.server_name,
                character_name: info.character_name,
                timezone_offset_seconds: info.timezone_offset_seconds,
            });
            continue;
        }

        // Regular chat message
        if is_timestamped_line(line) {
            if let Some(msg) = parse_chat_line(line) {
                let utc_second = msg.timestamp.and_utc().timestamp();
                events.push(TimedEvent::ChatMessage { utc_second, msg });
            }
        }
    }

    Ok(events)
}

/// Extract a date from a chat log filename like "Chat-26-03-27.log"
fn date_from_chat_filename(path: &PathBuf) -> Option<chrono::NaiveDate> {
    let stem = path.file_stem()?.to_str()?;
    // "Chat-YY-MM-DD"
    let date_part = stem.strip_prefix("Chat-")?;
    chrono::NaiveDate::parse_from_str(date_part, "%y-%m-%d").ok()
}

/// Extract the date from the first chat message timestamp in the file.
/// Fallback when the filename doesn't follow the Chat-YY-MM-DD pattern.
fn date_from_chat_content(events: &[TimedEvent]) -> Option<chrono::NaiveDate> {
    for event in events {
        if let TimedEvent::ChatMessage { msg, .. } = event {
            return Some(msg.timestamp.date());
        }
        if let TimedEvent::ChatLogin { utc_second, .. } = event {
            if *utc_second > 0 {
                let dt = chrono::DateTime::from_timestamp(*utc_second, 0)?;
                return Some(dt.date_naive());
            }
        }
    }
    None
}

/// Core replay logic — processes both logs through the full coordinator pipeline.
fn run_replay(
    player_log_path: PathBuf,
    chat_log_path: PathBuf,
    app: &AppHandle,
    db: &DbPool,
    game_data: GameDataState,
) -> Result<ReplayResult, String> {
    // --- Phase 1: Pre-scan chat log for timezone offset ---
    app.emit(
        "replay-progress",
        ReplayProgress {
            phase: "scanning".into(),
            current: 0,
            total: 2,
            detail: "Scanning chat log for timezone info...".into(),
        },
    )
    .ok();

    let chat_events = parse_chat_log_events(&chat_log_path)?;

    // Find the first timezone offset from login lines
    let mut tz_offset: i32 = 0;
    for event in &chat_events {
        if let TimedEvent::ChatLogin {
            timezone_offset_seconds: Some(offset),
            ..
        } = event
        {
            tz_offset = *offset;
            break;
        }
    }

    // Derive base date from chat log filename, chat content, or today
    let base_date = date_from_chat_filename(&chat_log_path)
        .or_else(|| date_from_chat_content(&chat_events))
        .unwrap_or_else(|| chrono::Utc::now().date_naive());

    eprintln!(
        "[replay] Base date: {}, timezone offset: {}s",
        base_date, tz_offset
    );

    // --- Phase 2: Parse Player.log with correct timezone ---
    app.emit(
        "replay-progress",
        ReplayProgress {
            phase: "scanning".into(),
            current: 1,
            total: 2,
            detail: "Parsing Player.log...".into(),
        },
    )
    .ok();

    let player_events = parse_player_log_lines(&player_log_path, base_date)?;

    // --- Phase 3: Apply timezone offset to chat events and merge ---
    // Chat.log timestamps are local time; convert to UTC using the detected offset.
    let chat_events: Vec<TimedEvent> = chat_events
        .into_iter()
        .map(|event| match event {
            TimedEvent::ChatMessage { msg, .. } => {
                let mut msg = msg;
                msg.timestamp = chat_local_to_utc(msg.timestamp, tz_offset);
                let utc_second = msg.timestamp.and_utc().timestamp();
                TimedEvent::ChatMessage { utc_second, msg }
            }
            TimedEvent::ChatLogin {
                server_name,
                character_name,
                timezone_offset_seconds,
                ..
            } => {
                // Recalculate utc_second with offset applied
                TimedEvent::ChatLogin {
                    utc_second: 0, // Login lines sort first regardless
                    server_name,
                    character_name,
                    timezone_offset_seconds,
                }
            }
            other => other,
        })
        .collect();

    let total_events = player_events.len() + chat_events.len();
    let mut all_events: Vec<TimedEvent> = Vec::with_capacity(total_events);
    all_events.extend(player_events);
    all_events.extend(chat_events);

    // Stable sort: ChatLogin first (timezone), then PlayerLine, then ChatMessage
    all_events.sort_by_key(|e| e.sort_key());

    // Diagnostic: show first/last timestamps from each source
    if let Some(first_player) = all_events
        .iter()
        .find(|e| matches!(e, TimedEvent::PlayerLine { .. }))
    {
        if let Some(last_player) = all_events
            .iter()
            .rev()
            .find(|e| matches!(e, TimedEvent::PlayerLine { .. }))
        {
            eprintln!(
                "[replay] Player.log UTC range: {} .. {}",
                first_player.utc_second(),
                last_player.utc_second()
            );
        }
    }
    if let Some(first_chat) = all_events
        .iter()
        .find(|e| matches!(e, TimedEvent::ChatMessage { .. }))
    {
        if let Some(last_chat) = all_events
            .iter()
            .rev()
            .find(|e| matches!(e, TimedEvent::ChatMessage { .. }))
        {
            eprintln!(
                "[replay] Chat.log UTC range: {} .. {}",
                first_chat.utc_second(),
                last_chat.utc_second()
            );
        }
    }

    // --- Phase 4: Process through coordinator pipeline ---
    let mut player_parser = PlayerEventParser::new();
    let mut game_state = GameStateManager::new(game_data.clone());
    game_state.set_base_date(base_date);
    let mut survey_aggregator = SurveySessionAggregator::new(game_data);
    survey_aggregator.set_base_date(base_date);

    let mut result = ReplayResult {
        player_lines_processed: 0,
        chat_messages_processed: 0,
        player_events_emitted: 0,
        chat_status_events_emitted: 0,
    };

    let progress_interval = (total_events / 100).max(50); // emit ~100 progress events

    // Use the same batching strategy as the live coordinator to avoid
    // flooding the Windows message queue. Accumulate player events and
    // domain updates, then flush in consolidated emissions. This
    // dramatically reduces the number of PostMessage calls compared to
    // per-event emission.
    const BATCH_MAX_SIZE: usize = 50;
    const BATCH_MAX_AGE: Duration = Duration::from_millis(100);

    let mut player_event_batch: Vec<PlayerEvent> = Vec::new();
    let mut domains_batch: Vec<&'static str> = Vec::new();
    let mut batch_start = Instant::now();
    let mut emits_since_yield: u32 = 0;
    let mut last_yield = Instant::now();

    /// Flush accumulated player events and domain updates to the frontend.
    /// Yields briefly after each flush so the message queue can drain.
    macro_rules! flush_batches {
        ($app:expr, $gs:expr, $pe:expr, $dom:expr, $start:expr, $emits:expr, $last_yield:expr) => {
            if !$pe.is_empty() {
                let batch_result = $gs.process_events_batch(&$pe, &db);
                $dom.extend(batch_result.domains_updated);

                $app.emit("player-events-batch", &$pe).ok();
                $emits += 1;
                $pe.clear();
            }
            if !$dom.is_empty() {
                $dom.sort_unstable();
                $dom.dedup();
                $app.emit("game-state-updated", &$dom).ok();
                $emits += 1;
                $dom.clear();
            }
            $start = Instant::now();

            // Yield so the webview JS event loop can process the batch
            if $emits >= 4 {
                std::thread::sleep(Duration::from_millis(15));
                $last_yield = Instant::now();
                $emits = 0;
            }
        };
    }

    for (i, event) in all_events.iter().enumerate() {
        // Progress updates
        if i % progress_interval == 0 {
            app.emit(
                "replay-progress",
                ReplayProgress {
                    phase: "processing".into(),
                    current: i,
                    total: total_events,
                    detail: format!("Processing event {}/{}", i, total_events),
                },
            )
            .ok();
        }

        match event {
            TimedEvent::ChatLogin {
                server_name,
                character_name,
                ..
            } => {
                // Flush pending batches before identity change
                flush_batches!(app, game_state, player_event_batch, domains_batch, batch_start, emits_since_yield, last_yield);

                game_state.set_active_character_name(character_name);
                game_state.set_active_server_name(server_name);

                app.emit("character-login", character_name).ok();
                app.emit("server-detected", server_name).ok();
                emits_since_yield += 2;
            }

            TimedEvent::PlayerLine { line, .. } => {
                result.player_lines_processed += 1;

                // Skill updates (legacy)
                if let Some(update) = parse_skill_update(line) {
                    app.emit("skill-update", &update).ok();
                    emits_since_yield += 1;
                }

                // Player events
                let mut p_events = player_parser.process_line(line);

                // Survey aggregator runs first so any survey_use_id it
                // injects into provenance reaches item_transactions via
                // game_state. Matches the live coordinator ordering.
                let active_char = game_state.get_active_character().map(String::from);
                let active_server = game_state.get_active_server().map(String::from);
                for pe in p_events.iter_mut() {
                    if let (Some(character), Some(server), Ok(conn)) =
                        (&active_char, &active_server, db.get_write())
                    {
                        let _ = survey_aggregator.process_event(pe, &conn, character, server, None);
                    }
                }

                result.player_events_emitted += p_events.len();
                player_event_batch.extend(p_events);
            }

            TimedEvent::ChatMessage { msg, .. } => {
                result.chat_messages_processed += 1;

                // Status channel → ChatStatusParser
                if let Some(status_event) = parse_status_message(msg) {
                    app.emit("chat-status-event", &status_event).ok();
                    emits_since_yield += 1;
                    result.chat_status_events_emitted += 1;
                }
            }
        }

        // Flush when batch is full or old enough
        if player_event_batch.len() >= BATCH_MAX_SIZE
            || (!player_event_batch.is_empty() && batch_start.elapsed() >= BATCH_MAX_AGE)
        {
            flush_batches!(app, game_state, player_event_batch, domains_batch, batch_start, emits_since_yield, last_yield);
        }

        // Extra yield if we've been emitting a lot of non-batched events
        // (skill-update, chat-status-event, character-login, etc.)
        if emits_since_yield >= 20 && last_yield.elapsed() < Duration::from_millis(50) {
            std::thread::sleep(Duration::from_millis(15));
            last_yield = Instant::now();
            emits_since_yield = 0;
        } else if last_yield.elapsed() >= Duration::from_millis(50) {
            last_yield = Instant::now();
            emits_since_yield = 0;
        }
    }

    // Flush any remaining batched events
    flush_batches!(app, game_state, player_event_batch, domains_batch, batch_start, emits_since_yield, last_yield);
    // Suppress unused-assignment warnings from the final macro expansion
    let _ = (batch_start, emits_since_yield, last_yield);

    // Flush pending player events from the parser itself
    let flush_events = player_parser.flush_all_pending();
    if !flush_events.is_empty() {
        let batch_result = game_state.process_events_batch(&flush_events, db);
        app.emit("player-events-batch", &flush_events).ok();
        result.player_events_emitted += flush_events.len();

        if !batch_result.domains_updated.is_empty() {
            let mut domains = batch_result.domains_updated;
            domains.sort_unstable();
            domains.dedup();
            app.emit("game-state-updated", &domains).ok();
        }
        std::thread::sleep(Duration::from_millis(15));
    }

    // Final progress
    app.emit(
        "replay-progress",
        ReplayProgress {
            phase: "complete".into(),
            current: total_events,
            total: total_events,
            detail: format!(
                "Done: {} player events, {} chat messages",
                result.player_events_emitted, result.chat_messages_processed,
            ),
        },
    )
    .ok();

    Ok(result)
}


// ============================================================
// Tauri Command
// ============================================================

/// Replay both a Player.log and Chat.log file through the full coordinator pipeline,
/// interleaved by timestamp. This simulates live tailing with cross-referencing.
#[tauri::command]
pub async fn replay_dual_logs(
    player_log_path: String,
    chat_log_path: String,
    app: AppHandle,
) -> Result<ReplayResult, String> {
    let player_path = PathBuf::from(&player_log_path);
    let chat_path = PathBuf::from(&chat_log_path);

    if !player_path.exists() {
        return Err(format!("Player.log not found: {}", player_log_path));
    }
    if !chat_path.exists() {
        return Err(format!("Chat.log not found: {}", chat_log_path));
    }

    let db = app.state::<DbPool>().inner().clone();
    let game_data = app.state::<GameDataState>().inner().clone();

    // Run on a blocking thread since file I/O is synchronous
    let result = tokio::task::spawn_blocking(move || {
        run_replay(player_path, chat_path, &app, &db, game_data)
    })
    .await
    .map_err(|e| format!("Replay task failed: {}", e))??;

    Ok(result)
}

// ============================================================
// Historical kill/loot backfill (silent — no frontend events)
// ============================================================

/// Result of a historical ingest of a Player.log file.
#[derive(Debug, Clone, serde::Serialize)]
pub struct IngestResult {
    /// Lootable searched corpses recorded as kills (the drop-rate denominator).
    pub kills_added: usize,
    /// Loot rows attributed to those kills.
    pub loot_added: usize,
    /// True when this exact file content was already ingested before (no-op).
    pub already_ingested: bool,
}

/// FNV-1a hash of file contents — cheap, dependency-free idempotency key.
fn content_hash(bytes: &[u8]) -> String {
    let mut hash: u64 = 0xcbf29ce484222325;
    for &b in bytes {
        hash ^= b as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{hash:016x}")
}

/// Subdirectory (under the app data dir) holding rotated `Player.log` copies.
pub const PLAYER_LOG_ARCHIVE_SUBDIR: &str = "player-logs";
/// Default archive limits, mirrored by `AppSettings` (`player_log_archive_*`)
/// and editable in Settings → General.
pub const PLAYER_LOG_ARCHIVE_DEFAULT_RETENTION_DAYS: u32 = 14;
pub const PLAYER_LOG_ARCHIVE_DEFAULT_MAX_FILES: u32 = 20;
pub const PLAYER_LOG_ARCHIVE_DEFAULT_MAX_MB: u32 = 512;
/// Suffix used for the in-progress copy before it is renamed into place.
const PLAYER_LOG_ARCHIVE_PARTIAL_SUFFIX: &str = ".log.partial";

/// Effective rotated-`Player.log` archive limits. All three are enforced
/// oldest-copy-first; the byte budget is usually the binding one, since a
/// session's `Player.log` is 30–45 MB (20 copies ≈ 900 MB). Byte-budgeted
/// retention is what `journald` (`SystemMaxUse=`), Docker (`max-size`) and
/// `logrotate` (`size`) all gate on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ArchiveLimits {
    pub retention_days: u32,
    pub max_files: u32,
    pub max_mb: u32,
}

impl Default for ArchiveLimits {
    fn default() -> Self {
        Self {
            retention_days: PLAYER_LOG_ARCHIVE_DEFAULT_RETENTION_DAYS,
            max_files: PLAYER_LOG_ARCHIVE_DEFAULT_MAX_FILES,
            max_mb: PLAYER_LOG_ARCHIVE_DEFAULT_MAX_MB,
        }
    }
}

/// Read the archive limits from settings, clamped to at least 1 so a
/// hand-edited `0` cannot disable the bounds entirely. (A `max_mb` below one
/// session's size still cannot empty the archive — `prune_player_log_archive`
/// never evicts the newest copy.)
pub fn archive_limits(settings: &crate::settings::SettingsManager) -> ArchiveLimits {
    let s = settings.get();
    ArchiveLimits {
        retention_days: s.player_log_archive_retention_days.max(1),
        max_files: s.player_log_archive_max_files.max(1),
        max_mb: s.player_log_archive_max_mb.max(1),
    }
}

/// `<app data dir>/player-logs` — derived from the settings file's parent, so
/// it lives beside `glogger.db`/settings rather than in the game's log dir.
pub fn player_log_archive_dir(settings: &crate::settings::SettingsManager) -> Option<PathBuf> {
    Some(
        settings
            .settings_file_path()
            .parent()?
            .join(PLAYER_LOG_ARCHIVE_SUBDIR),
    )
}

/// Copy `src` into the archive as `Player-<fnv1a content hash>.log` unless that
/// file already exists; then prune to `limits`. Content-hashing means re-scans
/// and repeated startups never duplicate a session (the same session copies to
/// the same name). Returns Ok(true) when a copy was written.
fn archive_player_log(src: &Path, dir: &Path, limits: ArchiveLimits) -> std::io::Result<bool> {
    let bytes = std::fs::read(src)?;
    let hash = content_hash(&bytes);
    let dest = dir.join(format!("Player-{hash}.log"));
    if dest.exists() {
        return Ok(false);
    }
    std::fs::create_dir_all(dir)?;

    // Write under a partial name and rename into place. The archive name is
    // content-addressed, so a copy interrupted mid-write would exist under a
    // name that certifies the *wrong* content — and its existence check would
    // then suppress the correct copy forever. Rename is atomic, so the file
    // either does not exist or holds exactly the hashed bytes.
    let partial = dir.join(format!("Player-{hash}{PLAYER_LOG_ARCHIVE_PARTIAL_SUFFIX}"));
    std::fs::write(&partial, &bytes)?;
    match std::fs::rename(&partial, &dest) {
        Ok(()) => {}
        // Another writer got there first; identical content is already in place.
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
            let _ = std::fs::remove_file(&partial);
            return Ok(false);
        }
        Err(e) => {
            let _ = std::fs::remove_file(&partial);
            return Err(e);
        }
    }

    prune_player_log_archive(dir, limits);
    Ok(true)
}

/// Enforce `limits`: delete copies older than `retention_days`, then evict
/// oldest-first until both `max_files` and `max_mb` hold — never the newest
/// copy, so a byte budget below one session's size cannot empty the archive.
/// Also clears interrupted copies. Best-effort: every failure is ignored, and
/// only files matching this archive's own naming (`Player-<16 hex>.log`) are
/// ever touched, so a `Player.log`/`Player-prev.log` a user drops in here is
/// left alone.
fn prune_player_log_archive(dir: &Path, limits: ArchiveLimits) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };

    let mut files: Vec<(std::time::SystemTime, u64, PathBuf)> = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        // `archive_player_log`'s own naming only. `name.starts_with("Player-")
        // && ends_with(".log")` would also claim the live logs' names and any
        // `Player-prev.log` a user copied in here (which the backfill scans).
        let is_ours = name
            .strip_prefix("Player-")
            .and_then(|stem| {
                stem.strip_suffix(PLAYER_LOG_ARCHIVE_PARTIAL_SUFFIX)
                    .or_else(|| stem.strip_suffix(".log"))
            })
            .is_some_and(|hash| hash.len() == 16 && hash.bytes().all(|b| b.is_ascii_hexdigit()));
        if !is_ours {
            continue;
        }
        if name.ends_with(PLAYER_LOG_ARCHIVE_PARTIAL_SUFFIX) {
            // Interrupted copy (crash between write and rename) — never usable.
            let _ = std::fs::remove_file(&path);
            continue;
        }
        let Ok(meta) = entry.metadata() else {
            continue;
        };
        let Ok(mtime) = meta.modified() else {
            continue;
        };
        files.push((mtime, meta.len(), path));
    }

    if let Some(cutoff) = std::time::SystemTime::now().checked_sub(std::time::Duration::from_secs(
        u64::from(limits.retention_days) * 24 * 60 * 60,
    )) {
        files.retain(|(mtime, _, path)| {
            if *mtime < cutoff {
                let _ = std::fs::remove_file(path);
                false
            } else {
                true
            }
        });
    }

    // Oldest-first eviction until both bounds hold, keeping the newest copy:
    // the byte budget is the usual binding limit, and with a budget below one
    // session's size an unprotected loop would delete the file just written and
    // leave the archive permanently empty. The byte budget is checked after the
    // sum, so a single oversized session can never wedge the archive past its
    // limit permanently.
    let max_bytes = u64::from(limits.max_mb) * 1024 * 1024;
    let max_files = limits.max_files as usize;
    files.sort_by_key(|(mtime, _, _)| *mtime);
    let mut total: u64 = files.iter().map(|(_, len, _)| *len).sum();
    let mut remaining = files.len();
    let evictable = files.len().saturating_sub(1);
    for (evicted, (_, len, path)) in files.into_iter().enumerate() {
        if remaining <= max_files && total <= max_bytes {
            break;
        }
        if evicted >= evictable {
            break;
        }
        let _ = std::fs::remove_file(&path);
        total = total.saturating_sub(len);
        remaining -= 1;
    }
}

/// Silently ingest lootable kills + loot from a single Player.log into the
/// lifetime database, deduped against live data via the enemy_kills UNIQUE
/// index. Emits no frontend events and does not touch live game state.
///
/// The drop-rate model is corpse-search based: each lootable `Search Corpse of
/// X` (permission granted) is one kill row keyed by the corpse entity_id (the
/// FIRST search's timestamp), and loot is attributed by matching the same corpse
/// entity_id. Loot dedup is by the item's `instance_id` so two separate
/// single-stacks of the same item off one corpse stay as two rows. Player.log is
/// self-sufficient (character + UTC date from the login line) — no Chat.log.
pub fn ingest_kill_loot_from_logs(
    player_log_path: PathBuf,
    db: &DbPool,
) -> Result<IngestResult, String> {
    let player_bytes =
        std::fs::read(&player_log_path).map_err(|e| format!("Failed to read Player.log: {e}"))?;
    let hash = content_hash(&player_bytes);

    // Idempotency probe on a READ connection: the whole parse below must not
    // hold the sole write connection (a long session's Player-prev.log can
    // parse for tens of seconds; holding the writer that long starves live
    // ingest and can drop chat batches past the 30s r2d2 wait).
    let probe = db.get().map_err(|e| format!("DB connection error: {e}"))?;
    let already: bool = probe
        .query_row(
            "SELECT 1 FROM player_prev_ingests WHERE content_hash = ?1",
            [&hash],
            |_| Ok(true),
        )
        .unwrap_or(false);
    drop(probe);
    if already {
        return Ok(IngestResult {
            kills_added: 0,
            loot_added: 0,
            already_ingested: true,
        });
    }

    // First login line gives the character + UTC base date used to timestamp
    // every event. Without it we can't produce killed_at parity with the live
    // path, so bail (nothing to ingest).
    let content = String::from_utf8_lossy(&player_bytes);
    let (character_name, base_date) = match content
        .lines()
        .find_map(crate::parsers::parse_player_log_login_line)
        .map(|(name, utc)| (name, utc.date()))
    {
        Some(v) => v,
        None => {
            return Ok(IngestResult {
                kills_added: 0,
                loot_added: 0,
                already_ingested: false,
            });
        }
    };
    // Server isn't recorded in Player.log; mirror the live path's fallback so the
    // dedup key matches when a server-less session is involved.
    let server_name = "Unknown".to_string();

    let player_events = parse_player_log_lines(&player_log_path, base_date)?;

    // --- Pass A: collect searched corpses (with permission) + their loot ---
    let mut parser = PlayerEventParser::new();
    // corpse_entity_id -> (corpse_name, first-search killed_at, zone, combat_skills)
    let mut corpses: std::collections::HashMap<u32, (String, String, Option<String>, Option<String>)> =
        std::collections::HashMap::new();
    // ordered loot: (corpse_entity_id, item_name, quantity, instance_id)
    let mut loot: Vec<(u32, String, u32, i64)> = Vec::new();
    // Current zone (internal area key) tracked from `LOADING LEVEL <area>` lines,
    // mirroring the live tailer, so each corpse is tagged with where it was searched.
    let mut current_zone: Option<String> = None;
    // Current equipped combat-skill loadout, tracked the same way as the live path
    // (ProcessSetActiveSkills / ProcessLoadAbilities), so each corpse is tagged with it.
    let mut current_combat_skills: Option<String> = None;

    for event in &player_events {
        let line = match event {
            TimedEvent::PlayerLine { line, .. } => line,
            _ => continue,
        };
        if let Some(idx) = line.find("LOADING LEVEL ") {
            let area = line[idx + "LOADING LEVEL ".len()..].trim();
            if !area.is_empty() {
                current_zone = Some(area.to_string());
            }
        }
        for ev in parser.process_line(line) {
            match ev {
                PlayerEvent::ActiveSkillsChanged { skill1, skill2, .. }
                | PlayerEvent::AbilitiesLoaded { skill1, skill2, .. } => {
                    current_combat_skills =
                        crate::coordinator::normalize_combat_loadout(&skill1, &skill2);
                }
                PlayerEvent::CorpseSearched {
                    timestamp,
                    corpse_entity_id,
                    corpse_name,
                    has_permission,
                } => {
                    if !has_permission {
                        continue;
                    }
                    // Keep the FIRST search per corpse (parity with the live path).
                    corpses.entry(corpse_entity_id).or_insert((
                        corpse_name,
                        timestamp,
                        current_zone.clone(),
                        current_combat_skills.clone(),
                    ));
                }
                PlayerEvent::LootPickedUp {
                    corpse_entity_id: Some(cid),
                    item_name: Some(item),
                    instance_id,
                    quantity,
                    ..
                } => {
                    if !item.is_empty() {
                        loot.push((cid, item, quantity, instance_id as i64));
                    }
                }
                _ => {}
            }
        }
    }

    // --- Pass B: persist kills, then attribute loot ---
    // The parse is done; only NOW take the dedicated write connection.
    let mut conn = db.get_write().map_err(|e| format!("DB connection error: {e}"))?;
    let tx = conn
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .map_err(|e| format!("Failed to begin transaction: {e}"))?;
    let mut result = IngestResult {
        kills_added: 0,
        loot_added: 0,
        already_ingested: false,
    };

    // corpse_entity_id -> kill_id (row id of the persisted kill)
    let mut kill_ids: std::collections::HashMap<u32, i64> = std::collections::HashMap::new();

    // Server-independent lookup for an existing kill of this corpse. Player.log
    // has no server field, so this backfill only knows `server_name = "Unknown"`,
    // whereas the live tailer records the real server (e.g. "Arisetsu"). The
    // kill-dedup UNIQUE index includes server_name, so keying the match on server
    // would MISS a corpse already recorded live and insert a duplicate kill row —
    // re-attaching the same loot under a new kill_id and doubling both kills and
    // loot. A corpse is uniquely identified by (character, entity_id, killed_at)
    // regardless of server, so match on that natural key instead.
    let find_existing_kill = |entity_id_str: &str, killed_at: &str| -> Option<i64> {
        tx.query_row(
            "SELECT id FROM enemy_kills
             WHERE character_name IS ?1 AND enemy_entity_id = ?2 AND killed_at = ?3
             LIMIT 1",
            rusqlite::params![character_name, entity_id_str, killed_at],
            |row| row.get::<_, i64>(0),
        )
        .ok()
    };

    for (entity_id, (corpse_name, killed_at, zone, combat_skills)) in &corpses {
        let entity_id_str = entity_id.to_string();

        // Reuse a kill already recorded by the live tailer (or a prior ingest) for
        // this same corpse, ignoring server_name — loot is deduped by instance_id
        // against whichever kill_id we resolve to.
        let kill_id = if let Some(id) = find_existing_kill(&entity_id_str, killed_at) {
            id
        } else {
            // No prior row for this corpse — insert it. (INSERT OR IGNORE guards a
            // rare race with the live tailer inserting the same corpse between our
            // check and this write; the fallback re-lookup then resolves it.)
            let inserted = tx
                .execute(
                    "INSERT OR IGNORE INTO enemy_kills
                        (enemy_name, enemy_entity_id, killing_ability,
                         health_damage, armor_damage, killed_at, character_name, server_name, zone, combat_skills)
                     VALUES (?1, ?2, '', 0, 0, ?3, ?4, ?5, ?6, ?7)",
                    rusqlite::params![
                        corpse_name,
                        entity_id_str,
                        killed_at,
                        character_name,
                        server_name,
                        zone,
                        combat_skills,
                    ],
                )
                .map_err(|e| format!("Failed to insert kill: {e}"))?;
            if inserted > 0 {
                result.kills_added += 1;
                tx.last_insert_rowid()
            } else {
                find_existing_kill(&entity_id_str, killed_at).unwrap_or(-1)
            }
        };
        if kill_id >= 0 {
            kill_ids.insert(*entity_id, kill_id);
        }
    }

    for (entity_id, item_name, quantity, instance_id) in &loot {
        let kill_id = match kill_ids.get(entity_id) {
            Some(id) => *id,
            None => continue, // loot from a corpse we didn't (or couldn't) record
        };
        if tx
            .execute(
                "INSERT OR IGNORE INTO enemy_kill_loot
                    (kill_id, item_name, quantity, instance_id)
                 VALUES (?1, ?2, ?3, ?4)",
                rusqlite::params![kill_id, item_name, quantity, instance_id],
            )
            .map_err(|e| format!("Failed to insert loot: {e}"))?
            > 0
        {
            result.loot_added += 1;
        }
    }

    // Record this file as ingested (idempotency).
    tx.execute(
        "INSERT OR REPLACE INTO player_prev_ingests
            (content_hash, source_path, ingested_at, kills_added, loot_added)
         VALUES (?1, ?2, CURRENT_TIMESTAMP, ?3, ?4)",
        rusqlite::params![
            hash,
            player_log_path.to_string_lossy(),
            result.kills_added as i64,
            result.loot_added as i64,
        ],
    )
    .map_err(|e| format!("Failed to record ingest: {e}"))?;

    tx.commit()
        .map_err(|e| format!("Failed to commit transaction: {e}"))?;

    Ok(result)
}

/// Spawn a background thread that backs up Player-prev.log into the lifetime
/// kill/loot database whenever the game rotates it (mtime change). Ingestion is
/// idempotent (content-hash guarded) and deduped against live data, so this is
/// safe to run alongside live tailing. Honors the `auto_ingest_player_prev`
/// setting, re-checked each tick so toggling it takes effect without a restart.
pub fn spawn_player_prev_watcher(
    settings: std::sync::Arc<crate::settings::SettingsManager>,
    db: DbPool,
) {
    std::thread::spawn(move || {
        // Let startup settle before the first scan.
        std::thread::sleep(Duration::from_secs(10));
        let mut last_mtime: Option<std::time::SystemTime> = None;

        loop {
            // Enforce the archive limits every tick: a copy only prunes the
            // directory after a *new* session is archived, and the limits are
            // user-editable in Settings, so lowering one must take effect
            // without waiting for the game to rotate (or for a restart). A
            // readdir over a handful of entries is far cheaper than the log
            // scans below it.
            if let Some(dir) = player_log_archive_dir(&settings) {
                prune_player_log_archive(&dir, archive_limits(&settings));
            }

            if settings.get_auto_ingest_player_prev() {
                if let Some(prev_path) = settings.get_player_prev_log_path() {
                    if let Ok(mtime) = std::fs::metadata(&prev_path).and_then(|m| m.modified()) {
                        if last_mtime != Some(mtime) {
                            last_mtime = Some(mtime);
                            match ingest_kill_loot_from_logs(prev_path.clone(), &db) {
                                Ok(r) if !r.already_ingested => {
                                    eprintln!(
                                        "[ingest] Player-prev backfill: +{} kills, +{} loot",
                                        r.kills_added, r.loot_added
                                    );
                                }
                                Ok(_) => {}
                                Err(e) => eprintln!("[ingest] Player-prev backfill failed: {e}"),
                            }
                            // Archive the rotated session (content-hashed, so
                            // re-scans/repeated startups never duplicate it).
                            // Errors are non-fatal — never abort the loop.
                            if let Some(dir) = player_log_archive_dir(&settings) {
                                if let Err(e) =
                                    archive_player_log(&prev_path, &dir, archive_limits(&settings))
                                {
                                    eprintln!("[ingest] Player.log archive failed: {e}");
                                }
                            }
                        }
                    }
                }
            }
            std::thread::sleep(Duration::from_secs(30));
        }
    });
}

/// Tauri command: ingest a single Player.log into the lifetime kill/loot
/// database. Used for manual backfill of kept backups and by the automatic
/// Player-prev.log rotation watcher. Player.log is self-sufficient — no Chat.log
/// is needed.
#[tauri::command]
pub async fn ingest_player_log(
    player_log_path: String,
    app: AppHandle,
) -> Result<IngestResult, String> {
    let player_path = PathBuf::from(&player_log_path);
    if !player_path.exists() {
        return Err(format!("Player.log not found: {player_log_path}"));
    }

    let db = app.state::<DbPool>().inner().clone();
    let result = tokio::task::spawn_blocking(move || {
        ingest_kill_loot_from_logs(player_path, &db)
    })
    .await
    .map_err(|e| format!("Ingest task failed: {e}"))??;

    Ok(result)
}

#[cfg(test)]
mod ingest_tests {
    use super::*;
    use std::io::Write;

    /// Unique temp path per call: `SystemTime` nanos repeat when parallel tests
    /// start in the same tick, so a bare-nanos name let two helpers open one
    /// path — contents from one test leaked into another's assertion.
    fn unique_temp_path(prefix: &str, ext: Option<&str>) -> PathBuf {
        use std::sync::atomic::{AtomicU64, Ordering};
        static SEQ: AtomicU64 = AtomicU64::new(0);
        let n = SEQ.fetch_add(1, Ordering::Relaxed);
        // Nanos kept for human-sortable names; pid + counter carry uniqueness.
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let mut path = std::env::temp_dir();
        match ext {
            Some(ext) => path.push(format!("{prefix}_{}_{nanos}_{n}.{ext}", std::process::id())),
            None => path.push(format!("{prefix}_{}_{nanos}_{n}", std::process::id())),
        }
        path
    }

    /// Build a fresh migrated DB at a unique temp path.
    fn temp_pool() -> (DbPool, PathBuf) {
        let path = unique_temp_path("glogger_ingest_test", Some("db"));
        let pool = crate::db::init_pool(path.clone(), Some(0)).expect("init pool");
        (pool, path)
    }

    fn write_log(lines: &str) -> PathBuf {
        let path = unique_temp_path("glogger_ingest_test", Some("log"));
        let mut f = std::fs::File::create(&path).expect("create log");
        f.write_all(lines.as_bytes()).expect("write log");
        path
    }

    fn temp_dir() -> PathBuf {
        let path = unique_temp_path("glogger_archive_test", None);
        std::fs::create_dir_all(&path).expect("create dir");
        path
    }

    /// Count `Player-*.log` files in `dir`.
    fn archived_count(dir: &Path) -> usize {
        std::fs::read_dir(dir)
            .unwrap()
            .flatten()
            .filter(|e| {
                e.file_name()
                    .to_str()
                    .is_some_and(|n| n.starts_with("Player-") && n.ends_with(".log"))
            })
            .count()
    }

    #[test]
    fn archive_player_log_dedups_identical_content() {
        let dir = temp_dir();
        let limits = ArchiveLimits::default();
        let src = dir.join("src.log");
        std::fs::write(&src, b"session one").unwrap();

        assert!(
            archive_player_log(&src, &dir, limits).unwrap(),
            "first copy written"
        );
        assert_eq!(archived_count(&dir), 1);

        // Same content (a re-scan or an app restart) must not duplicate.
        assert!(
            !archive_player_log(&src, &dir, limits).unwrap(),
            "identical content is a no-op"
        );
        assert_eq!(archived_count(&dir), 1);

        // A different session produces a new file.
        std::fs::write(&src, b"session two").unwrap();
        assert!(archive_player_log(&src, &dir, limits).unwrap());
        assert_eq!(archived_count(&dir), 2);

        // No in-progress copy is left behind (write-then-rename).
        assert!(
            !std::fs::read_dir(&dir)
                .unwrap()
                .flatten()
                .any(|e| e.file_name().to_str().is_some_and(|n| n.ends_with(".partial"))),
            "no partial file remains"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn prune_enforces_the_byte_budget_oldest_first() {
        let dir = temp_dir();
        let limits = ArchiveLimits::default();
        let byte_budget = u64::from(limits.max_mb) * 1024 * 1024;
        let set_mtime = |path: &Path, secs_ago: u64| {
            let f = std::fs::OpenOptions::new().write(true).open(path).unwrap();
            f.set_times(
                std::fs::FileTimes::new()
                    .set_modified(std::time::SystemTime::now() - Duration::from_secs(secs_ago)),
            )
            .unwrap();
        };

        // Four sparse files of slightly more than a quarter of the budget each:
        // the count cap is nowhere near binding, so only the byte budget can
        // evict. (Sparse files cost no real disk space.)
        let quarter = byte_budget / 4 + 1;
        for i in 0..4u64 {
            let p = dir.join(format!("Player-{i:016x}.log"));
            std::fs::File::create(&p).unwrap().set_len(quarter).unwrap();
            set_mtime(&p, 1000 + i);
        }

        prune_player_log_archive(&dir, limits);

        // Total must fit the budget: three quarters is fine, four is not, so
        // the oldest copy (the largest mtime age) goes.
        assert_eq!(archived_count(&dir), 3, "byte budget enforced");
        assert!(
            !dir.join("Player-0000000000000003.log").exists(),
            "oldest evicted"
        );
        assert!(
            dir.join("Player-0000000000000000.log").exists(),
            "newest kept"
        );
        let total: u64 = std::fs::read_dir(&dir)
            .unwrap()
            .flatten()
            .map(|e| e.metadata().unwrap().len())
            .sum();
        assert!(total <= byte_budget, "remaining bytes within budget");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn prune_honours_narrowed_settings_limits() {
        let dir = temp_dir();
        let set_mtime = |path: &Path, secs_ago: u64| {
            let f = std::fs::OpenOptions::new().write(true).open(path).unwrap();
            f.set_times(
                std::fs::FileTimes::new()
                    .set_modified(std::time::SystemTime::now() - Duration::from_secs(secs_ago)),
            )
            .unwrap();
        };

        // Five current copies, all well inside the byte budget.
        for i in 0..5u64 {
            let p = dir.join(format!("Player-{i:016x}.log"));
            std::fs::write(&p, b"x").unwrap();
            set_mtime(&p, 100 + i);
        }

        // A narrowed file cap (what the Settings UI writes) evicts oldest-first.
        prune_player_log_archive(
            &dir,
            ArchiveLimits {
                max_files: 2,
                ..ArchiveLimits::default()
            },
        );
        assert_eq!(archived_count(&dir), 2, "settings file cap enforced");
        assert!(
            dir.join("Player-0000000000000000.log").exists()
                && dir.join("Player-0000000000000001.log").exists(),
            "newest two kept (largest mtime ages evicted)"
        );

        // A tightened byte budget does the same on its own (1 MiB, two 600 KiB
        // copies = 1.2 MiB > budget).
        let dir2 = temp_dir();
        for i in 0..2u64 {
            let p = dir2.join(format!("Player-{i:016x}.log"));
            std::fs::File::create(&p).unwrap().set_len(600 * 1024).unwrap();
            set_mtime(&p, 100 + i);
        }
        prune_player_log_archive(
            &dir2,
            ArchiveLimits {
                max_mb: 1,
                ..ArchiveLimits::default()
            },
        );
        assert_eq!(archived_count(&dir2), 1, "settings byte budget enforced");
        assert!(
            dir2.join("Player-0000000000000000.log").exists(),
            "newest copy kept (older one evicted for bytes)"
        );

        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_dir_all(&dir2);
    }

    #[test]
    fn prune_clears_interrupted_copies() {
        let dir = temp_dir();
        let partial = dir.join(format!(
            "Player-00000000000000cc{PLAYER_LOG_ARCHIVE_PARTIAL_SUFFIX}"
        ));
        std::fs::write(&partial, b"half a session").unwrap();
        let keep = dir.join("Player-00000000000000dd.log");
        std::fs::write(&keep, b"x").unwrap();

        prune_player_log_archive(&dir, ArchiveLimits::default());

        assert!(!partial.exists(), "interrupted copy removed");
        assert!(keep.exists(), "complete copy kept");

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A byte budget below one session's size (the clamp floor is 1 MiB; a
    /// session's Player.log is 30–45 MB) must not delete the copy that was just
    /// written — otherwise the archive is emptied on every tick and the backfill
    /// silently loses its extra reach-back.
    #[test]
    fn prune_keeps_the_newest_copy_even_when_the_byte_budget_is_impossible() {
        let dir = temp_dir();
        let only = dir.join("Player-00000000000000ee.log");
        std::fs::File::create(&only)
            .unwrap()
            .set_len(40 * 1024 * 1024)
            .unwrap();

        prune_player_log_archive(
            &dir,
            ArchiveLimits {
                max_mb: 1,
                ..ArchiveLimits::default()
            },
        );

        assert!(only.exists(), "the only copy survives an impossible budget");

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Only this feature's own naming is ever deleted. `Player-prev.log` matches
    /// the loose `Player-*.log` prefix, and the *backfill* scans that name as a
    /// source, so a user-dropped copy must not be pruned out from under it.
    #[test]
    fn prune_touches_only_its_own_naming() {
        let dir = temp_dir();
        let dropped = dir.join("Player-prev.log");
        std::fs::write(&dropped, b"x").unwrap();
        let live = dir.join("Player.log");
        std::fs::write(&live, b"x").unwrap();
        let ours = dir.join("Player-00000000000000ff.log");
        std::fs::write(&ours, b"x").unwrap();

        prune_player_log_archive(
            &dir,
            ArchiveLimits {
                max_files: 1,
                max_mb: 1,
                ..ArchiveLimits::default()
            },
        );

        assert!(dropped.exists(), "a user-dropped copy is left alone");
        assert!(live.exists(), "the live log's name is left alone");
        assert!(ours.exists(), "our own copy is kept");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn prune_enforces_retention_days_and_max_files() {
        let dir = temp_dir();
        let limits = ArchiveLimits::default();
        let set_mtime = |path: &Path, secs_ago: u64| {
            let f = std::fs::OpenOptions::new().write(true).open(path).unwrap();
            f.set_times(
                std::fs::FileTimes::new()
                    .set_modified(std::time::SystemTime::now() - Duration::from_secs(secs_ago)),
            )
            .unwrap();
        };

        // Retention: one copy older than the window, plus exactly the file cap
        // worth of current copies (so the cap prune stays out of the way).
        let ancient = dir.join("Player-00000000000000aa.log");
        std::fs::write(&ancient, b"x").unwrap();
        set_mtime(
            &ancient,
            (u64::from(limits.retention_days) + 3) * 24 * 60 * 60,
        );

        let fresh = dir.join("Player-00000000000000bb.log");
        std::fs::write(&fresh, b"x").unwrap();
        set_mtime(&fresh, 60);

        // A non-archive file must never be touched.
        let other = dir.join("not-an-archive.txt");
        std::fs::write(&other, b"x").unwrap();

        for i in 0..limits.max_files as usize - 1 {
            let p = dir.join(format!("Player-{i:016x}.log"));
            std::fs::write(&p, b"x").unwrap();
            set_mtime(&p, 120 + i as u64);
        }

        prune_player_log_archive(&dir, limits);

        assert!(!ancient.exists(), "copy beyond retention pruned");
        assert!(fresh.exists(), "current copy kept");
        assert!(other.exists(), "non-archive file untouched");
        assert_eq!(
            archived_count(&dir),
            limits.max_files as usize,
            "at the cap, nothing extra pruned"
        );

        // Cap: push past the file cap; the oldest copies go first.
        for i in 0..5u64 {
            let p = dir.join(format!("Player-{:016x}.log", 0xf00 + i));
            std::fs::write(&p, b"x").unwrap();
            // All five are older than the existing copies.
            set_mtime(&p, 1000 + i * 10);
        }

        prune_player_log_archive(&dir, limits);

        assert_eq!(
            archived_count(&dir),
            limits.max_files as usize,
            "oldest pruned down to the cap"
        );
        assert!(
            !dir.join("Player-0000000000000f04.log").exists(),
            "oldest extra removed"
        );
        assert!(
            !dir.join("Player-0000000000000f00.log").exists(),
            "all five oldest extras removed"
        );
        assert!(
            dir.join("Player-0000000000000000.log").exists(),
            "a recent copy survives"
        );
        assert!(fresh.exists(), "the current copy survives");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn corpse_search_ingest_logs_every_lootable_kill_and_keeps_duplicate_stacks() {
        // Corpse 500 (permission): two SEPARATE single-stacks of the same item
        // off one corpse (distinct instance ids 111/222) must stay two rows.
        // Corpse 600 (no permission): not the player's kill — excluded.
        // Corpse 700 (permission): a lootable kill that dropped nothing — still
        // logged (the "log every lootable kill" requirement).
        let log = "\
[15:41:09] Logged in as character TestPlayer. Time UTC=04/17/2026 15:41:09. Timezone Offset 00:00:00
[15:42:00] LocalPlayer: ProcessTalkScreen(500, \"Search Corpse of Goblin\", \"\", \"\", System.Int32[], System.String[], 0, Corpse)
[15:42:01] LocalPlayer: ProcessAddItem(HealthPotion(111), -1, True)
[15:42:01] LocalPlayer: ProcessRemoveLoot(111)
[15:42:02] LocalPlayer: ProcessAddItem(HealthPotion(222), -1, True)
[15:42:02] LocalPlayer: ProcessRemoveLoot(222)
[15:43:00] LocalPlayer: ProcessTalkScreen(600, \"Search Corpse of Wolf\", \"(You do not have permission to loot this corpse.)\", \"\", System.Int32[], System.String[], 0, Corpse)
[15:44:00] LocalPlayer: ProcessTalkScreen(700, \"Search Corpse of Rat\", \"\", \"\", System.Int32[], System.String[], 0, Corpse)
";
        let log_path = write_log(log);
        let (pool, db_path) = temp_pool();

        let r = ingest_kill_loot_from_logs(log_path.clone(), &pool).expect("ingest");
        assert!(!r.already_ingested);
        // Two permission corpses (Goblin + Rat); the no-permission Wolf excluded.
        assert_eq!(r.kills_added, 2, "lootable kills (permission corpses)");
        // Both single-stacks survive instance_id dedup.
        assert_eq!(r.loot_added, 2, "two distinct item instances");

        let conn = pool.get().unwrap();
        let kills: i64 = conn
            .query_row("SELECT COUNT(*) FROM enemy_kills", [], |row| row.get(0))
            .unwrap();
        assert_eq!(kills, 2);
        // The Goblin's two HealthPotion instances are two distinct loot rows.
        let goblin_loot: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM enemy_kill_loot l
                 JOIN enemy_kills k ON k.id = l.kill_id
                 WHERE k.enemy_entity_id = '500'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(goblin_loot, 2, "duplicate single-stacks not collapsed");

        // Re-ingesting the same file is a content-hash no-op.
        let r2 = ingest_kill_loot_from_logs(log_path.clone(), &pool).expect("re-ingest");
        assert!(r2.already_ingested);
        assert_eq!(r2.kills_added, 0);
        assert_eq!(r2.loot_added, 0);

        // The wrapped transaction committed exactly once, and the second
        // (no-op) call wrote nothing: kills/loot/ingest rows are unchanged.
        let kills_after: i64 = conn
            .query_row("SELECT COUNT(*) FROM enemy_kills", [], |row| row.get(0))
            .unwrap();
        assert_eq!(kills_after, 2, "no duplicate kills after re-ingest no-op");
        let loot_after: i64 = conn
            .query_row("SELECT COUNT(*) FROM enemy_kill_loot", [], |row| row.get(0))
            .unwrap();
        assert_eq!(loot_after, 2, "Goblin 2 + Rat 0 loot rows");
        let ingests_after: i64 = conn
            .query_row("SELECT COUNT(*) FROM player_prev_ingests", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(ingests_after, 1, "one ingest record for this content hash");

        drop(conn);
        let _ = std::fs::remove_file(&log_path);
        let _ = std::fs::remove_file(&db_path);
    }

    #[test]
    fn backfill_dedups_against_live_kill_despite_server_mismatch() {
        // Regression: the live tailer records kills under the real server name
        // (e.g. "Arisetsu") while this Player.log backfill only knows "Unknown".
        // The kill-dedup UNIQUE index includes server_name, so matching on server
        // would treat a corpse already recorded live as new — inserting a
        // duplicate kill row and re-attaching the same loot under a fresh kill_id,
        // doubling both kills and loot. The backfill must dedup on the
        // server-independent natural key (character, entity_id, killed_at).
        let (pool, db_path) = temp_pool();
        {
            let conn = pool.get_write().unwrap();
            // Simulate a live-recorded kill: corpse 500 "Goblin" at 15:42:00 under
            // the real server "Arisetsu", with one looted HealthPotion (inst 111).
            conn.execute(
                "INSERT INTO enemy_kills
                    (enemy_name, enemy_entity_id, killing_ability, health_damage, armor_damage,
                     killed_at, character_name, server_name, zone, combat_skills)
                 VALUES ('Goblin', '500', '', 0, 0, '15:42:00', 'TestPlayer', 'Arisetsu', NULL, NULL)",
                [],
            )
            .unwrap();
            let kid: i64 = conn
                .query_row(
                    "SELECT id FROM enemy_kills WHERE enemy_entity_id = '500'",
                    [],
                    |r| r.get(0),
                )
                .unwrap();
            conn.execute(
                "INSERT INTO enemy_kill_loot (kill_id, item_name, quantity, instance_id)
                 VALUES (?1, 'HealthPotion', 1, 111)",
                rusqlite::params![kid],
            )
            .unwrap();
        }

        // Backfill the rotated Player.log for the SAME session: same corpse 500 at
        // 15:42:00, re-looting instance 111 plus a genuinely new instance 222.
        let log = "\
[15:41:09] Logged in as character TestPlayer. Time UTC=04/17/2026 15:41:09. Timezone Offset 00:00:00
[15:42:00] LocalPlayer: ProcessTalkScreen(500, \"Search Corpse of Goblin\", \"\", \"\", System.Int32[], System.String[], 0, Corpse)
[15:42:01] LocalPlayer: ProcessAddItem(HealthPotion(111), -1, True)
[15:42:01] LocalPlayer: ProcessRemoveLoot(111)
[15:42:02] LocalPlayer: ProcessAddItem(HealthPotion(222), -1, True)
[15:42:02] LocalPlayer: ProcessRemoveLoot(222)
";
        let log_path = write_log(log);
        let r = ingest_kill_loot_from_logs(log_path.clone(), &pool).expect("ingest");
        assert!(!r.already_ingested);
        // The corpse already existed (recorded live) — no new kill row.
        assert_eq!(r.kills_added, 0, "no duplicate kill for an already-recorded corpse");
        // Instance 111 already recorded live (dedup); only 222 is genuinely new.
        assert_eq!(r.loot_added, 1, "only the new instance is added, 111 deduped");

        let conn = pool.get().unwrap();
        let kills: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM enemy_kills WHERE enemy_entity_id = '500'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(kills, 1, "corpse recorded once, not duplicated under 'Unknown'");
        let loot: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM enemy_kill_loot l
                 JOIN enemy_kills k ON k.id = l.kill_id
                 WHERE k.enemy_entity_id = '500'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(loot, 2, "111 (live) + 222 (backfill); 111 not duplicated");
        // The kept kill row retains the real server, not the backfill's "Unknown".
        let srv: String = conn
            .query_row(
                "SELECT server_name FROM enemy_kills WHERE enemy_entity_id = '500'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(srv, "Arisetsu");

        drop(conn);
        let _ = std::fs::remove_file(&log_path);
        let _ = std::fs::remove_file(&db_path);
    }
}
