/// Chat.log replay — re-parses archived chat logs and re-emits their events
/// (emit-only preview for debugging; never writes to the database).
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::Duration;

use tauri::{AppHandle, Emitter, Manager};

use crate::cdn_commands::GameDataState;
use crate::chat_parser::{
    is_timestamped_line, parse_chat_line, parse_chat_login_line, ChatMessage,
};
use crate::chat_status_parser::parse_status_message;
use crate::db::DbPool;
use crate::game_state::GameStateManager;
use crate::parsers::chat_local_to_utc;
/// A timestamped chat event, ordered for replay.
#[derive(Debug)]
enum TimedEvent {
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
            TimedEvent::ChatMessage { utc_second, .. } => *utc_second,
            TimedEvent::ChatLogin { utc_second, .. } => *utc_second,
        }
    }

    /// Sort key: (utc_second, source_order)
    /// source_order: 0 = ChatLogin (timezone must come first), 1 = ChatMessage
    fn sort_key(&self) -> (i64, u8) {
        match self {
            TimedEvent::ChatLogin { utc_second, .. } => (*utc_second, 0),
            TimedEvent::ChatMessage { utc_second, .. } => (*utc_second, 1),
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

/// Replay result summary (chat-only since the Player.log pipeline was deleted).
#[derive(Debug, Clone, serde::Serialize)]
pub struct ReplayResult {
    pub chat_messages_processed: usize,
    pub chat_status_events_emitted: usize,
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
        if is_timestamped_line(line)
            && let Some(msg) = parse_chat_line(line) {
                let utc_second = msg.timestamp.and_utc().timestamp();
                events.push(TimedEvent::ChatMessage { utc_second, msg });
            }
    }

    Ok(events)
}

/// Core replay logic — re-parses Chat.log events and re-emits them as frontend
/// events (emit-only; the live dispatch pipeline is never invoked).
fn run_replay(
    chat_log_path: PathBuf,
    app: &AppHandle,
    _db: &DbPool,
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

    eprintln!("[replay] Chat.log timezone offset: {tz_offset}s");


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
        })
        .collect();

    let total_events = chat_events.len();
    let mut all_events: Vec<TimedEvent> = Vec::with_capacity(total_events);
    all_events.extend(chat_events);

    // Stable sort: ChatLogin first, then ChatMessage
    all_events.sort_by_key(|e| e.sort_key());

    if let Some(first_chat) = all_events
        .iter()
        .find(|e| matches!(e, TimedEvent::ChatMessage { .. }))
        && let Some(last_chat) = all_events
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

    // --- Phase 4: Process through coordinator pipeline ---
    let mut game_state = GameStateManager::new(game_data.clone());

    let mut result = ReplayResult {
        chat_messages_processed: 0,
        chat_status_events_emitted: 0,
    };

    let progress_interval = (total_events / 100).max(50); // emit ~100 progress events

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

                game_state.set_active_character_name(character_name);
                game_state.set_active_server_name(server_name);

                app.emit("character-login", character_name).ok();
                app.emit("server-detected", server_name).ok();
            }


            TimedEvent::ChatMessage { msg, .. } => {
                result.chat_messages_processed += 1;

                // Status channel → ChatStatusParser
                if let Some(status_event) = parse_status_message(msg) {
                    app.emit("chat-status-event", &status_event).ok();
                    result.chat_status_events_emitted += 1;
                }
            }
        }


    }


    // Final progress
    app.emit(
        "replay-progress",
        ReplayProgress {
            phase: "complete".into(),
            current: total_events,
            total: total_events,
            detail: format!(
                "Done: {} chat messages",
                result.chat_messages_processed,
            ),
        },
    )
    .ok();

    Ok(result)
}

// ============================================================
// Tauri Command
// ============================================================

/// Re-parse a Chat.log file and re-emit its chat events, in timestamp order.
/// This is a parse-and-preview tool for debugging, NOT a backfill path: it
/// never writes to the database (the message branch parses status lines and
/// emits `chat-status-event` / `character-login` / `server-detected` only).
/// Identity: this is a debug preview only. All login lines are forced to
/// utc_second 0 so they sort before every message, and the last login in the
/// file wins for the whole preview (per-segment identity is not preserved).
/// Replay flips the active character/session feed to that identity while it
/// runs. Live tailing, scans, and report imports remain the only writers.
/// (The old dual-log replay also fed a Player.log; that pipeline was
/// deleted, so only the chat path is replayed.)
#[tauri::command]
pub async fn replay_chat_log(
    chat_log_path: String,
    app: AppHandle,
) -> Result<ReplayResult, String> {
    let chat_path = PathBuf::from(&chat_log_path);

    if !chat_path.exists() {
        return Err(format!("Chat.log not found: {}", chat_log_path));
    }

    let db = app.state::<DbPool>().inner().clone();
    let game_data = app.state::<GameDataState>().inner().clone();

    // Run on a blocking thread since file I/O is synchronous
    let result = tokio::task::spawn_blocking(move || {
        run_replay(chat_path, &app, &db, game_data)
    })
    .await
    .map_err(|e| format!("Replay task failed: {}", e))??;

    Ok(result)
}

// ============================================================
// Historical kill/loot backfill (silent — no frontend events)
// ============================================================


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

/// Spawn a background thread that archives Player-prev.log copies whenever the
/// game rotates it (mtime change) and prunes per the archive limits. Loot
/// ingestion is removed (current client writes no loot lines); the watcher now
/// only keeps the archive that poems backfill + Statehelm gifts still read.
pub fn spawn_player_prev_watcher(
    settings: std::sync::Arc<crate::settings::SettingsManager>,
    _db: DbPool,
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

            // Loot auto-ingest removed (current client writes no loot lines).
            // When the archive toggle is on, keep the rotated copy for poems
            // backfill + Statehelm gifts; pruning above always runs so lowered
            // limits take effect without waiting for a rotation.
            if settings.get_auto_ingest_player_prev()
                && let Some(prev_path) = settings.get_player_prev_log_path()
                && let Ok(mtime) = std::fs::metadata(&prev_path).and_then(|m| m.modified())
                && last_mtime != Some(mtime) {
                            last_mtime = Some(mtime);
                            // Archive the rotated session (content-hashed, so
                            // re-scans/repeated startups never duplicate it).
                            // Errors are non-fatal — never abort the loop.
                            if let Some(dir) = player_log_archive_dir(&settings)
                                && let Err(e) =
                                    archive_player_log(&prev_path, &dir, archive_limits(&settings))
                                {
                                    eprintln!("[ingest] Player.log archive failed: {e}");
                                }
                        }
            std::thread::sleep(Duration::from_secs(30));
        }
    });
}
