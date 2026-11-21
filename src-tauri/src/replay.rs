/// Chat.log replay — re-parses archived chat logs and re-emits their events
/// (emit-only preview for debugging; never writes to the database).
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::Duration;

use tauri::{AppHandle, Emitter, Manager, Runtime};

use crate::cdn_commands::GameDataState;
use crate::chat_parser::{
    is_timestamped_line, parse_chat_line, parse_chat_login_line, ChatMessage,
};
use crate::chat_status_parser::{parse_status_message, ChatStatusEvent};
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

/// Envelope tagging replay-originated status events so the live session
/// feed skips accumulation: existing listeners switch on `payload.kind`,
/// which an envelope lacks (no `kind` key), so replayed XP/wisdom never
/// feeds live combat/session totals. The preview keeps working off
/// `replay-progress` + `ReplayResult` counts, not this payload.
#[derive(Debug, Clone, serde::Serialize)]
struct ReplayChatStatusEnvelope<'a> {
    is_replay: bool,
    event: &'a ChatStatusEvent,
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
/// events (preview-only; the live dispatch pipeline is never invoked and no
/// report/import path runs here).
///
/// The preview deliberately emits NO live identity events (`character-login` /
/// `server-detected`): the gameStateStore `character-login` listener answers
/// any identity it sees with `initForActiveCharacter`, which invokes
/// `import_latest_report_for_character` + `import_reports_for_server` (plus the
/// inventory import) — production DB writes. Emitting the live names would
/// turn this debug preview into a report auto-import. Callers needing the
/// replayed identity read the log, never the event bus.
fn run_replay<R: Runtime>(
    chat_log_path: PathBuf,
    app: &AppHandle<R>,
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
                // Local bookkeeping only — deliberately NO `character-login` /
                // `server-detected` emits (see `run_replay` docs): those are
                // the live identity events, and the frontend answers each one
                // with report auto-imports (production DB writes).
                game_state.set_active_character_name(character_name);
                game_state.set_active_server_name(server_name);
            }


            TimedEvent::ChatMessage { msg, .. } => {
                result.chat_messages_processed += 1;

                // Status channel → ChatStatusParser. The preview wraps each
                // event in the `is_replay` envelope so the live session feed
                // skips it (existing listeners switch on `payload.kind`,
                // which the envelope lacks). Emit-only: never writes, never
                // imports reports — no report path exists here. The shadow
                // keeps the emit line byte-identical: the event name is
                // unchanged, only the payload is tagged.
                if let Some(status_event) = parse_status_message(msg) {
                    let status_event =
                        ReplayChatStatusEnvelope { is_replay: true, event: &status_event };
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
/// never writes to the database and never emits live identity events. The
/// message branch parses status lines and emits the `is_replay`-enveloped
/// `chat-status-event` (which the live session feed skips) plus
/// `replay-progress`; `character-login` / `server-detected` are deliberately
/// absent because the frontend answers each with report auto-imports.
/// Identity: this is a debug preview only. All login lines are forced to
/// utc_second 0 so they sort before every message, and the last login in the
/// file wins for the whole preview (per-segment identity is not preserved).
/// The live character/session feed is untouched — the UI never reloads and
/// report imports never run. Live tailing, scans, and report imports remain
/// the only writers.
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
    let result = tokio::task::spawn_blocking(move || run_replay(chat_path, &app, &db, game_data))
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

#[cfg(test)]
mod tests {
    use super::*;
    use tauri::Listener;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    static SEQ: AtomicUsize = AtomicUsize::new(0);

    /// Migrated pool over a unique shared-cache memory DB, mirroring the
    /// `db::character_commands` test helper (migrations run through `writes`;
    /// report imports would write through `writes` too).
    fn pool() -> DbPool {
        let n = SEQ.fetch_add(1, Ordering::Relaxed);
        let mk = |query_only: bool| {
            r2d2_sqlite::SqliteConnectionManager::file(format!(
                "file:replay_preview_test_{n}?mode=memory&cache=shared"
            ))
            .with_init(move |conn| {
                if query_only {
                    conn.execute_batch(
                        "PRAGMA journal_mode=WAL;
                         PRAGMA busy_timeout=5000;
                         PRAGMA synchronous=NORMAL;
                         PRAGMA foreign_keys=ON;
                         PRAGMA query_only=ON;",
                    )
                } else {
                    conn.execute_batch(
                        "PRAGMA journal_mode=WAL;
                         PRAGMA busy_timeout=5000;
                         PRAGMA synchronous=NORMAL;
                         PRAGMA foreign_keys=ON;",
                    )
                }
            })
        };
        let writes = r2d2::Pool::builder()
            .max_size(1)
            .build(mk(false))
            .expect("writes pool");
        let reads = r2d2::Pool::builder()
            .max_size(2)
            .build(mk(true))
            .expect("reads pool");
        let conn = writes.get().expect("conn");
        crate::db::migrations::run_migrations(&conn, None).expect("migrations");
        drop(conn);
        DbPool::from_pools(reads, writes)
    }

    fn empty_game_data() -> GameDataState {
        Arc::new(tokio::sync::RwLock::new(crate::game_data::GameData::empty()))
    }

    /// Chat.log exercising the preview's login arm: one login line (timezone
    /// + server identity) plus a status line and a plain chat line.
    fn fixture_log() -> PathBuf {
        let n = SEQ.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "replay_preview_test_{}_{n}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).expect("tmp dir");
        let path = dir.join("Chat.log");
        std::fs::write(
            &path,
            "**************************************** Logged In As TestReplayer. Server Dreva. Timezone Offset -05:00:00.\n\
             26-03-26 15:12:48\t[Status] You earned 62 XP in Endurance.\n\
             26-03-26 15:13:02\t[Guild] Mate: hello\n",
        )
        .expect("fixture");
        path
    }

    /// The preview must stay a preview: a log containing a login line must
    /// not emit the live identity events (the gameStateStore
    /// `character-login` listener answers each with report auto-imports —
    /// production DB writes) and must leave the report tables empty.
    #[test]
    fn replay_preview_emits_no_live_identity_and_writes_no_report_rows() {
        let path = fixture_log();

        // Positive control: the fixture really reaches the login arm, so the
        // no-emit asserts below pin behaviour rather than an empty file.
        let parsed = parse_chat_log_events(&path).expect("parse fixture");
        assert!(
            parsed
                .iter()
                .any(|e| matches!(e, TimedEvent::ChatLogin { .. })),
            "fixture must contain a login line"
        );

        let db = pool();
        let game_data = empty_game_data();
        let app = tauri::test::mock_app();
        let logins_seen = Arc::new(AtomicUsize::new(0));
        let servers_seen = Arc::new(AtomicUsize::new(0));
        // Listener guards stay alive until after the replay runs (drop =
        // unlisten), so every emit during the preview is counted.
        let _unlisten_login = app.listen("character-login", {
            let count = logins_seen.clone();
            move |_| {
                count.fetch_add(1, Ordering::SeqCst);
            }
        });
        let _unlisten_server = app.listen("server-detected", {
            let count = servers_seen.clone();
            move |_| {
                count.fetch_add(1, Ordering::SeqCst);
            }
        });

        let result = run_replay(path, app.handle(), &db, game_data).expect("replay");
        assert_eq!(result.chat_messages_processed, 2);
        assert_eq!(result.chat_status_events_emitted, 1);
        assert_eq!(
            logins_seen.load(Ordering::SeqCst),
            0,
            "preview must not emit character-login"
        );
        assert_eq!(
            servers_seen.load(Ordering::SeqCst),
            0,
            "preview must not emit server-detected"
        );

        let conn = db.get_write().expect("write conn");
        let snapshots: i64 = conn
            .query_row("SELECT COUNT(*) FROM character_snapshots", [], |row| {
                row.get(0)
            })
            .expect("snapshot count");
        assert_eq!(snapshots, 0, "preview must write no report rows");
    }
}
