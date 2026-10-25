/// Tauri commands for chat functionality
use crate::chat_combat_parser;
use crate::chat_parser;
use crate::db::game_state_commands::{record_milking_event, record_poop};
use crate::db::queries::log_positions;
use crate::db::{chat_commands, DbPool};
use crate::parsers::chat_local_to_utc;
use crate::settings::SettingsManager;
use std::path::PathBuf;
use std::sync::Arc;
use tauri::{AppHandle, Emitter, State};

#[derive(Clone, serde::Serialize)]
struct ScanProgress {
    current: usize,
    total: usize,
    file_name: String,
}

#[tauri::command]
pub async fn scan_chat_logs(
    path: String,
    db_pool: State<'_, DbPool>,
    settings: State<'_, Arc<SettingsManager>>,
    app: AppHandle,
) -> Result<ScanResult, String> {
    let chat_logs_path = PathBuf::from(&path);
    let app_settings = settings.get();
    let excluded_channels = app_settings.excluded_chat_channels.clone();
    let tz_offset = app_settings
        .manual_timezone_override
        .or(app_settings.timezone_offset_seconds)
        .unwrap_or(0);

    let log_files = chat_parser::get_chat_log_files(&chat_logs_path)
        .map_err(|e| format!("Failed to scan chat logs directory: {e}"))?;

    let total_files = log_files.len();
    let mut total_messages = 0;
    let mut files_processed = 0;

    for (index, log_file) in log_files.iter().enumerate() {
        let _ = app.emit(
            "chat-scan-progress",
            ScanProgress {
                current: index + 1,
                total: total_files,
                file_name: log_file.file_name.clone(),
            },
        );

        // Probe the stored position with a READ connection and do the disk
        // I/O + parsing BEFORE checking out the single writer, so a large
        // backfill can't monopolize the write queue and block live ingest.
        let file_path_str = log_file.file_path.to_string_lossy().to_string();
        let start_position;
        {
            let conn = db_pool.get().map_err(|e| format!("Database error: {e}"))?;
            start_position = log_positions::get_position(&conn, &file_path_str)
                .map_err(|e| format!("Failed to get log position: {e}"))?;
        }

        let player_name = if start_position == 0 {
            chat_parser::extract_player_name(&log_file.file_path)
                .map_err(|e| format!("Failed to extract player name: {e}"))?
        } else {
            None
        };

        let (messages, new_position) =
            chat_parser::read_chat_log(&log_file.file_path, start_position)
                .map_err(|e| format!("Failed to read chat log: {e}"))?;

        // Convert chat timestamps from local time to UTC
        let messages: Vec<_> = messages
            .into_iter()
            .map(|mut msg| {
                msg.timestamp = chat_local_to_utc(msg.timestamp, tz_offset);
                msg
            })
            .collect();

        // insert_chat_messages runs its own transaction internally, so no
        // outer transaction here — single-file and tail paths already call
        // it directly on the write connection. Position updates run on the
        // same single-writer checkout, so no concurrent tail can interleave.
        let inserted = if messages.is_empty() {
            // No rows: still record the position (empty read at EOF).
            let conn = db_pool
                .get_write()
                .map_err(|e| format!("Database error: {e}"))?;
            log_positions::update_position(
                &conn,
                &file_path_str,
                "chat",
                new_position,
                player_name.as_deref(),
                Some(&{
                    serde_json::json!({
                        "file_name": log_file.file_name,
                        "file_date": log_file.file_date.format("%Y-%m-%d").to_string()
                    })
                    .to_string()
                }),
            )
            .map_err(|e| format!("Failed to update log position: {e}"))?;
            0
        } else {
            let conn = db_pool
                .get_write()
                .map_err(|e| format!("Database error: {e}"))?;
            let inserted = chat_commands::insert_chat_messages(
                &conn,
                &messages,
                &log_file.file_name,
                &excluded_channels,
            )
            .map_err(|e| format!("Failed to insert messages: {e}"))?;
            log_positions::update_position(
                &conn,
                &file_path_str,
                "chat",
                new_position,
                player_name.as_deref(),
                Some(&{
                    serde_json::json!({
                        "file_name": log_file.file_name,
                        "file_date": log_file.file_date.format("%Y-%m-%d").to_string()
                    })
                    .to_string()
                }),
            )
            .map_err(|e| format!("Failed to update log position: {e}"))?;
            inserted
        };
        total_messages += inserted;
        files_processed += 1;
    }

    // Bulk backfill done: rebuild the FTS index so it's guaranteed consistent
    // even if any historical insert missed the sync trigger.
    if total_messages > 0 {
        let conn = db_pool
            .get_write()
            .map_err(|e| format!("Database error: {e}"))?;
        chat_commands::rebuild_chat_fts(&conn)
            .map_err(|e| format!("Failed to rebuild chat search index: {e}"))?;
    }

    Ok(ScanResult {
        files_processed,
        messages_imported: total_messages,
    })
}

#[derive(serde::Serialize)]
pub struct ScanResult {
    pub files_processed: usize,
    pub messages_imported: usize,
}

#[derive(Clone, serde::Serialize)]
pub struct AbilityHistoryBackfillResult {
    pub files_scanned: usize,
    /// Files the sweep couldn't read (locked, rotating, deleted mid-sweep). They
    /// are logged and skipped rather than aborting the rest of the archive.
    pub files_skipped: usize,
    pub poops_recorded: usize,
    pub self_milks_recorded: usize,
}

/// Result of scanning one Chat log for ability lines: the parsed candidate
/// messages plus the timezone offset recorded in that file's own login line
/// (seconds east of UTC), when it has one.
type ChatLogAbilityScan = (Vec<chat_parser::ChatMessage>, Option<i32>);

/// Backfill the per-ability history tables from every archived `Chat-*.log`:
/// `player_pooping_log` (Dungcrafting) and `player_milking_log` self-milks.
///
/// Both abilities only ever appear as Combat-channel lines, and Combat is
/// excluded from `chat_messages` by default — so nothing was stored anywhere and
/// position-based chat scans had already reached EOF before either feature
/// existed. Read each file from byte 0 instead. Poop timestamps keep the log's
/// LOCAL wall clock (the dedup key the live hook writes); self-milk timestamps are
/// UTC-converted RFC3339 (the shape `player_milking_log` has always used),
/// converted with **the log file's own login-line offset** rather than today's
/// setting, so re-running after a DST flip or a changed `manual_timezone_override`
/// re-derives the identical instant instead of inserting a twin row.
///
/// Both inserts only skip their own dedup key (`ON CONFLICT … DO NOTHING`), so
/// re-running records nothing new and a real write error still surfaces.
#[tauri::command]
pub async fn backfill_ability_history(
    db_pool: State<'_, DbPool>,
    settings: State<'_, Arc<SettingsManager>>,
    app: AppHandle,
    character_name: String,
    server_name: String,
    path: Option<String>,
) -> Result<AbilityHistoryBackfillResult, String> {
    let chat_logs_dir = match path.filter(|p| !p.trim().is_empty()) {
        Some(p) => PathBuf::from(p),
        None => settings
            .get_chat_logs_dir()
            .ok_or_else(|| "Chat logs directory is not configured".to_string())?,
    };

    // Fallback only: files that carry a login line convert with their own offset.
    let app_settings = settings.get();
    let fallback_tz_offset = app_settings
        .manual_timezone_override
        .or(app_settings.timezone_offset_seconds)
        .unwrap_or(0);

    let log_files = chat_parser::get_chat_log_files(&chat_logs_dir)
        .map_err(|e| format!("Failed to scan chat logs directory: {e}"))?;

    let mut files_scanned = 0;
    let mut files_skipped = 0;
    let mut poops_recorded = 0;
    let mut self_milks_recorded = 0;

    for log_file in &log_files {
        // Disk I/O + parsing before checking out the writer, so a long backfill
        // can't monopolize the write queue and block live ingest (same split as
        // `scan_chat_logs`). One unreadable file (locked, rotating, deleted
        // mid-sweep) must not abort the rest of the archive: the caller only gets
        // one shot per session, so a hard failure would silently strand every
        // later file.
        let (messages, file_offset) = match ability_lines_in_file(
            &log_file.file_path,
            &[POOP_LINE_MARKER, SELF_MILK_LINE_MARKER],
        ) {
            Ok(scan) => scan,
            Err(e) => {
                eprintln!(
                    "[backfill] Skipping unreadable chat log {}: {e}",
                    log_file.file_name
                );
                files_skipped += 1;
                continue;
            }
        };
        files_scanned += 1;

        let tz_offset = file_offset.unwrap_or(fallback_tz_offset);
        let poops = poop_events_in_messages(&messages, &character_name);
        let self_milks = self_milk_events_in_messages(&messages, &character_name, tz_offset);
        if poops.is_empty() && self_milks.is_empty() {
            continue;
        }

        let conn = db_pool
            .get_write()
            .map_err(|e| format!("Database error: {e}"))?;
        for (target, pooped_at) in poops {
            if record_poop(&conn, &character_name, &server_name, &target, &pooped_at)
                .map_err(|e| format!("Failed to record poop: {e}"))?
            {
                poops_recorded += 1;
            }
        }
        for (target, milked_at) in self_milks {
            if record_milking_event(
                &conn,
                &character_name,
                &server_name,
                &target,
                "self_milked",
                &milked_at,
            )
            .map_err(|e| format!("Failed to record self-milking: {e}"))?
            {
                self_milks_recorded += 1;
            }
        }
    }

    if poops_recorded > 0 || self_milks_recorded > 0 {
        app.emit("game-state-updated", vec!["report_stats", "milking"])
            .ok();
    }

    Ok(AbilityHistoryBackfillResult {
        files_scanned,
        files_skipped,
        poops_recorded,
        self_milks_recorded,
    })
}

/// Cheap pre-filters for `ability_lines_in_file`: the chat parser is only asked
/// about lines containing one of these.
const POOP_LINE_MARKER: &str = "Poop on ";
const SELF_MILK_LINE_MARKER: &str = "Collect Milk on ";

/// Parse only the lines of one Chat log that contain one of `needles`, plus the
/// offset from the file's login line.
///
/// The backfill sweeps every archived log (~100 files, hundreds of MB) for a
/// handful of ability lines, so scan the raw text first and hand the chat parser
/// only candidate lines: full parsing (a `chrono` parse per line) is ~10x the
/// cost and these lines are far rarer than one per file.
///
/// The login line is picked up in the same pass because it carries the file's own
/// `Timezone Offset` — the offset a line must be converted with is a property of
/// the log, not of whatever the settings say today (see the command's doc).
fn ability_lines_in_file(
    path: &std::path::Path,
    needles: &[&str],
) -> std::io::Result<ChatLogAbilityScan> {
    let bytes = std::fs::read(path)?;
    let text = String::from_utf8_lossy(&bytes);
    let mut messages = Vec::new();
    let mut login_offset = None;
    for line in text.lines() {
        // Same cheap text test as the needles: only a login line is parsed for
        // its offset, and only until one is found.
        if login_offset.is_none() && line.contains("Logged In As") {
            if let Some(info) = chat_parser::parse_chat_login_line(line) {
                login_offset = info.timezone_offset_seconds;
            }
        }
        if !needles.iter().any(|needle| line.contains(needle)) {
            continue;
        }
        if let Some(msg) = chat_parser::parse_chat_line(line) {
            messages.push(msg);
        }
    }
    Ok((messages, login_offset))
}

/// Poop events in one parsed Chat log: `(target, local wall-clock timestamp)`.
///
/// Split out from the command so the matching rules are testable without disk
/// or Tauri state.
fn poop_events_in_messages(
    messages: &[chat_parser::ChatMessage],
    character_name: &str,
) -> Vec<(String, String)> {
    messages
        .iter()
        .filter_map(|msg| {
            if msg.channel.as_deref() != Some("Combat") {
                return None;
            }
            let target = chat_combat_parser::parse_self_poop_line(
                msg.sender.as_deref(),
                &msg.message,
                character_name,
            )?;
            Some((
                target,
                msg.timestamp.format("%Y-%m-%d %H:%M:%S").to_string(),
            ))
        })
        .collect()
}

/// Self-milk events in one parsed Chat log:
/// `(target, RFC3339 UTC timestamp)`.
///
/// The chat log is local time, so convert with `tz_offset_seconds` first —
/// `player_milking_log.milked_at` is UTC, matching the live hook. Callers pass the
/// offset from that log file's own login line (falling back to the settings
/// offset), so the instant is stable no matter what the timezone setting says
/// later.
fn self_milk_events_in_messages(
    messages: &[chat_parser::ChatMessage],
    character_name: &str,
    tz_offset_seconds: i32,
) -> Vec<(String, String)> {
    messages
        .iter()
        .filter_map(|msg| {
            if msg.channel.as_deref() != Some("Combat") {
                return None;
            }
            let target = chat_combat_parser::parse_self_ability_line(
                msg.sender.as_deref(),
                &msg.message,
                character_name,
                "Collect Milk",
            )?;
            if target != character_name {
                return None;
            }
            let utc = crate::parsers::chat_local_to_utc(msg.timestamp, tz_offset_seconds);
            let milked_at =
                chrono::DateTime::<chrono::Utc>::from_naive_utc_and_offset(utc, chrono::Utc)
                    .to_rfc3339();
            Some((target, milked_at))
        })
        .collect()
}

#[tauri::command]
pub async fn scan_chat_log_file(
    path: String,
    db_pool: State<'_, DbPool>,
    settings: State<'_, Arc<SettingsManager>>,
) -> Result<ScanResult, String> {
    let log_path = PathBuf::from(&path);
    let app_settings = settings.get();
    let excluded_channels = app_settings.excluded_chat_channels.clone();
    let tz_offset = app_settings
        .manual_timezone_override
        .or(app_settings.timezone_offset_seconds)
        .unwrap_or(0);

    if !log_path.exists() {
        return Err(format!("Chat log file not found: {}", path));
    }

    let file_name = log_path
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or("Invalid file name")?
        .to_string();

    // Read + parse on a READ connection first; the writer is checked out
    // only for the short insert + position-update phase so a large file
    // backfill can't block live ingest behind disk I/O.
    let file_path_str = log_path.to_string_lossy().to_string();
    let (start_position, file_date) = {
        let conn = db_pool.get().map_err(|e| format!("Database error: {e}"))?;
        let pos = log_positions::get_position(&conn, &file_path_str)
            .map_err(|e| format!("Failed to get log position: {e}"))?;
        let parsed_name = chat_parser::parse_chat_log_filename(&file_name);
        (pos, parsed_name)
    };

    let player_name = if start_position == 0 {
        chat_parser::extract_player_name(&log_path)
            .map_err(|e| format!("Failed to extract player name: {e}"))?
    } else {
        None
    };

    let (messages, new_position) = chat_parser::read_chat_log(&log_path, start_position)
        .map_err(|e| format!("Failed to read chat log: {e}"))?;

    // Convert chat timestamps from local time to UTC
    let messages: Vec<_> = messages
        .into_iter()
        .map(|mut msg| {
            msg.timestamp = chat_local_to_utc(msg.timestamp, tz_offset);
            msg
        })
        .collect();

    let conn = db_pool
        .get_write()
        .map_err(|e| format!("Database error: {e}"))?;

    let mut messages_imported = 0;
    if !messages.is_empty() {
        messages_imported =
            chat_commands::insert_chat_messages(&conn, &messages, &file_name, &excluded_channels)
                .map_err(|e| format!("Failed to insert messages: {e}"))?;
    }

    let metadata = file_date.map(|d| {
        serde_json::json!({
            "file_name": file_name,
            "file_date": d.format("%Y-%m-%d").to_string()
        })
        .to_string()
    });

    log_positions::update_position(
        &conn,
        &file_path_str,
        "chat",
        new_position,
        player_name.as_deref(),
        metadata.as_deref(),
    )
    .map_err(|e| format!("Failed to update log position: {e}"))?;

    Ok(ScanResult {
        files_processed: 1,
        messages_imported,
    })
}

#[tauri::command(rename_all = "camelCase")]
pub async fn get_chat_messages(
    channel: Option<String>,
    sender: Option<String>,
    search_text: Option<String>,
    start_time: Option<String>,
    end_time: Option<String>,
    has_item_links: Option<bool>,
    item_name: Option<String>,
    tell_partner: Option<String>,
    limit: Option<i64>,
    offset: Option<i64>,
    sort_order: Option<String>,
    db_pool: State<'_, DbPool>,
) -> Result<Vec<chat_commands::ChatMessageRow>, String> {
    let conn = db_pool.get().map_err(|e| format!("Database error: {e}"))?;

    let filter = chat_commands::ChatMessageFilter {
        channel,
        sender,
        search_text,
        start_time,
        end_time,
        has_item_links,
        item_name,
        tell_partner,
        limit: limit.unwrap_or(100),
        offset: offset.unwrap_or(0),
        sort_order: sort_order.unwrap_or_else(|| "desc".to_string()),
    };

    chat_commands::get_chat_messages(&conn, &filter)
        .map_err(|e| format!("Failed to get messages: {e}"))
}

#[tauri::command(rename_all = "camelCase")]
pub async fn get_chat_days(
    offset_seconds: Option<i64>,
    db_pool: State<'_, DbPool>,
) -> Result<Vec<chat_commands::ChatDayRow>, String> {
    let conn = db_pool.get().map_err(|e| format!("Database error: {e}"))?;

    // `offsetSeconds` = viewer's display timezone (seconds east of UTC), so the
    // day list matches the times the UI renders.
    chat_commands::get_chat_days(&conn, offset_seconds.unwrap_or(0))
        .map_err(|e| format!("Failed to get chat days: {e}"))
}

#[tauri::command(rename_all = "camelCase")]
pub async fn count_chat_messages(
    channel: Option<String>,
    sender: Option<String>,
    search_text: Option<String>,
    start_time: Option<String>,
    end_time: Option<String>,
    has_item_links: Option<bool>,
    item_name: Option<String>,
    tell_partner: Option<String>,
    db_pool: State<'_, DbPool>,
) -> Result<i64, String> {
    let conn = db_pool.get().map_err(|e| format!("Database error: {e}"))?;

    let filter = chat_commands::ChatMessageFilter {
        channel,
        sender,
        search_text,
        start_time,
        end_time,
        has_item_links,
        item_name,
        tell_partner,
        ..Default::default()
    };

    chat_commands::count_chat_messages(&conn, &filter)
        .map_err(|e| format!("Failed to count messages: {e}"))
}

#[tauri::command]
pub async fn get_chat_channels(db_pool: State<'_, DbPool>) -> Result<Vec<String>, String> {
    let conn = db_pool.get().map_err(|e| format!("Database error: {e}"))?;

    chat_commands::get_channels(&conn).map_err(|e| format!("Failed to get channels: {e}"))
}

#[tauri::command]
pub async fn get_chat_channel_stats(
    db_pool: State<'_, DbPool>,
) -> Result<Vec<ChannelStat>, String> {
    let conn = db_pool.get().map_err(|e| format!("Database error: {e}"))?;

    let stats = chat_commands::get_channel_stats(&conn)
        .map_err(|e| format!("Failed to get channel stats: {e}"))?;

    Ok(stats
        .into_iter()
        .map(|(channel, count)| ChannelStat { channel, count })
        .collect())
}

#[derive(serde::Serialize)]
pub struct ChannelStat {
    pub channel: String,
    pub count: i64,
}

#[tauri::command]
pub async fn tail_chat_log(
    chat_log_file: String,
    db_pool: State<'_, DbPool>,
    settings: State<'_, Arc<SettingsManager>>,
) -> Result<Vec<chat_commands::ChatMessageRow>, String> {
    let log_path = PathBuf::from(&chat_log_file);
    let app_settings = settings.get();
    let excluded_channels = app_settings.excluded_chat_channels.clone();
    let tz_offset = app_settings
        .manual_timezone_override
        .or(app_settings.timezone_offset_seconds)
        .unwrap_or(0);

    if !log_path.exists() {
        return Err(format!("Chat log file not found: {}", chat_log_file));
    }

    // Read + parse on a READ connection first; the writer is checked out
    // only for the short insert + position-update phase (tail runs every
    // few seconds — it must never queue behind its own disk I/O).
    let file_path_str = log_path.to_string_lossy().to_string();
    let start_position = {
        let conn = db_pool.get().map_err(|e| format!("Database error: {e}"))?;
        log_positions::get_position(&conn, &file_path_str)
            .map_err(|e| format!("Failed to get log position: {e}"))?
    };

    let (messages, new_position) = chat_parser::read_chat_log(&log_path, start_position)
        .map_err(|e| format!("Failed to read chat log: {e}"))?;

    if messages.is_empty() {
        return Ok(Vec::new());
    }

    // Convert chat timestamps from local time to UTC
    let messages: Vec<_> = messages
        .into_iter()
        .map(|mut msg| {
            msg.timestamp = chat_local_to_utc(msg.timestamp, tz_offset);
            msg
        })
        .collect();

    let file_name = log_path
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or("Invalid file name")?;

    let player_name = if start_position == 0 {
        chat_parser::extract_player_name(&log_path)
            .map_err(|e| format!("Failed to extract player name: {e}"))?
    } else {
        None
    };

    let conn = db_pool
        .get_write()
        .map_err(|e| format!("Database error: {e}"))?;
    chat_commands::insert_chat_messages(&conn, &messages, file_name, &excluded_channels)
        .map_err(|e| format!("Failed to insert messages: {e}"))?;

    let file_date = chat_parser::parse_chat_log_filename(file_name)
        .ok_or("Invalid chat log filename format")?;

    let metadata = serde_json::json!({
        "file_name": file_name,
        "file_date": file_date.format("%Y-%m-%d").to_string()
    })
    .to_string();

    log_positions::update_position(
        &conn,
        &file_path_str,
        "chat",
        new_position,
        player_name.as_deref(),
        Some(&metadata),
    )
    .map_err(|e| format!("Failed to update log position: {e}"))?;

    // Convert to ChatMessageRow for return
    let rows: Vec<chat_commands::ChatMessageRow> = messages
        .iter()
        .enumerate()
        .map(|(i, msg)| chat_commands::ChatMessageRow {
            id: i as i64, // Temporary ID
            timestamp: msg.timestamp.format("%Y-%m-%d %H:%M:%S").to_string(),
            channel: msg.channel.clone(),
            sender: msg.sender.clone(),
            message: msg.message.clone(),
            is_system: msg.is_system,
            from_player: msg.from_player,
            item_links: msg
                .item_links
                .iter()
                .map(|link| chat_commands::ChatItemLinkRow {
                    raw_text: link.raw_text.clone(),
                    item_name: link.item_name.clone(),
                    item_id: None,
                })
                .collect(),
        })
        .collect();

    Ok(rows)
}

#[tauri::command]
pub async fn get_chat_stats(
    db_pool: State<'_, DbPool>,
) -> Result<chat_commands::ChatStats, String> {
    let conn = db_pool.get().map_err(|e| format!("Database error: {e}"))?;

    chat_commands::get_chat_stats(&conn).map_err(|e| format!("Failed to get chat stats: {e}"))
}

#[tauri::command]
pub async fn get_tell_conversations(
    db_pool: State<'_, DbPool>,
) -> Result<Vec<ChannelStat>, String> {
    let conn = db_pool.get().map_err(|e| format!("Database error: {e}"))?;

    let conversations = chat_commands::get_tell_conversations(&conn)
        .map_err(|e| format!("Failed to get conversations: {e}"))?;

    Ok(conversations
        .into_iter()
        .map(|(channel, count)| ChannelStat { channel, count })
        .collect())
}

#[tauri::command]
pub async fn purge_chat_messages(days: u32, db_pool: State<'_, DbPool>) -> Result<usize, String> {
    let mut conn = db_pool
        .get_write()
        .map_err(|e| format!("Database error: {e}"))?;

    let cutoff_date = chrono::Utc::now() - chrono::Duration::days(days as i64);
    let cutoff_str = cutoff_date.format("%Y-%m-%d %H:%M:%S").to_string();

    let tx = conn
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .map_err(|e| format!("Failed to begin transaction: {e}"))?;

    // Delete orphaned item links for messages that will be purged
    // (in case foreign_keys pragma wasn't active for older data)
    tx.execute(
        "DELETE FROM chat_item_links WHERE message_id IN (SELECT id FROM chat_messages WHERE timestamp < ?1)",
        [&cutoff_str],
    )
    .map_err(|e| format!("Failed to purge item links: {e}"))?;

    let deleted = tx
        .execute(
            "DELETE FROM chat_messages WHERE timestamp < ?1",
            [&cutoff_str],
        )
        .map_err(|e| format!("Failed to purge messages: {e}"))?;

    tx.commit()
        .map_err(|e| format!("Failed to commit transaction: {e}"))?;

    Ok(deleted)
}

#[tauri::command]
pub async fn delete_all_chat_messages(db_pool: State<'_, DbPool>) -> Result<usize, String> {
    let mut conn = db_pool
        .get_write()
        .map_err(|e| format!("Database error: {e}"))?;

    let deleted = delete_all_chat_messages_impl(&mut conn)?;

    // Reclaim disk space (retried on SQLITE_BUSY). Must run after the commit —
    // VACUUM cannot run inside a transaction.
    crate::db::vacuum_with_retry(&conn, 3)
        .map_err(|e| format!("Failed to vacuum database (busy after 3 attempts): {e}"))?;

    Ok(deleted)
}

/// Four DELETEs in one transaction. Split out from the command so the
/// all-or-nothing behaviour is unit-testable. `conn` is `&mut` because
/// `transaction_with_behavior` needs it.
fn delete_all_chat_messages_impl(conn: &mut rusqlite::Connection) -> Result<usize, String> {
    let tx = conn
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .map_err(|e| format!("Failed to begin transaction: {e}"))?;

    // Delete item links first (in case foreign_keys pragma wasn't active for older data)
    tx.execute("DELETE FROM chat_item_links", [])
        .map_err(|e| format!("Failed to delete item links: {e}"))?;

    let deleted = tx
        .execute("DELETE FROM chat_messages", [])
        .map_err(|e| format!("Failed to delete all messages: {e}"))?;

    // Clear legacy chat log file tracking
    tx.execute("DELETE FROM chat_log_files", [])
        .map_err(|e| format!("Failed to delete chat log files: {e}"))?;

    // Reset file positions for chat logs so they can be re-imported
    tx.execute(
        "DELETE FROM log_file_positions WHERE file_type = 'chat'",
        [],
    )
    .map_err(|e| format!("Failed to reset file positions: {e}"))?;

    tx.commit()
        .map_err(|e| format!("Failed to commit transaction: {e}"))?;

    Ok(deleted)
}

#[tauri::command]
pub async fn get_watch_rule_messages(
    rule_id: u64,
    limit: Option<i64>,
    offset: Option<i64>,
    db_pool: State<'_, DbPool>,
    settings: State<'_, Arc<SettingsManager>>,
) -> Result<Vec<chat_commands::ChatMessageRow>, String> {
    let app_settings = settings.get();
    let rule = app_settings
        .watch_rules
        .iter()
        .find(|r| r.id == rule_id)
        .ok_or_else(|| format!("Watch rule {} not found", rule_id))?;

    let conn = db_pool.get().map_err(|e| format!("Database error: {e}"))?;

    chat_commands::get_watch_rule_messages(
        &conn,
        rule,
        limit.unwrap_or(100),
        offset.unwrap_or(0),
        &app_settings.excluded_chat_channels,
    )
    .map_err(|e| format!("Failed to get watch rule messages: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn delete_all_chat_messages_rolls_back_on_partial_failure() {
        let mut conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE chat_messages (id INTEGER PRIMARY KEY);
             CREATE TABLE chat_item_links (id INTEGER PRIMARY KEY, message_id INTEGER);
             CREATE TABLE chat_log_files (id INTEGER PRIMARY KEY);
             CREATE TABLE log_file_positions (file_path TEXT, file_type TEXT);",
        )
        .unwrap();
        conn.execute_batch(
            "INSERT INTO chat_messages (id) VALUES (1);
             INSERT INTO chat_item_links (message_id) VALUES (1);",
        )
        .unwrap();
        // Break the SECOND delete so the transaction must roll back the first.
        conn.execute_batch("DROP TABLE chat_messages").unwrap();
        assert!(delete_all_chat_messages_impl(&mut conn).is_err());
        let remaining: i64 = conn
            .query_row("SELECT COUNT(*) FROM chat_item_links", [], |r| r.get(0))
            .unwrap();
        assert_eq!(
            remaining, 1,
            "first DELETE must roll back when a later statement fails"
        );
    }

    fn parse_lines(lines: &[&str]) -> Vec<chat_parser::ChatMessage> {
        lines
            .iter()
            .filter_map(|l| chat_parser::parse_chat_line(l))
            .collect()
    }

    #[test]
    fn poop_events_pick_only_self_poops() {
        let messages = parse_lines(&[
            "26-08-10 20:42:54\t[Combat] TwinkleofToes: Poop on TwinkleofToes!",
            // Someone else's poop line (another character's log) — not ours.
            "26-08-10 20:45:00\t[Combat] OtherPerson: Poop on OtherPerson!",
            // Excluded-ish noise on the same channel: must not match.
            "26-08-10 20:46:00\t[Combat] TwinkleofToes: Collect Milk on TwinkleofToes!",
            "26-08-10 20:47:00\t[Combat] Boar #12345: Gore on TwinkleofToes! Dmg: 30 health, 0 armor",
            // Pooping on something other than yourself still counts.
            "26-08-11 21:03:00\t[Combat] TwinkleofToes: Poop on the Floor!",
            // Wrong channel, right text.
            "26-08-11 21:04:00\t[Guild] TwinkleofToes: Poop on TwinkleofToes!",
        ]);

        let events = poop_events_in_messages(&messages, "TwinkleofToes");
        assert_eq!(
            events,
            vec![
                (
                    "TwinkleofToes".to_string(),
                    "2026-08-10 20:42:54".to_string()
                ),
                ("the Floor".to_string(), "2026-08-11 21:03:00".to_string()),
            ]
        );
    }

    #[test]
    fn self_milk_events_convert_to_utc_rfc3339_and_stay_self_only() {
        let messages = parse_lines(&[
            // 20:14:16 local at -05:00 → 2026-10-13T01:14:16Z.
            "26-10-12 20:14:16\t[Combat] TwinkleofToes: Collect Milk on TwinkleofToes!",
            // Milking somebody else is not a self-milk.
            "26-10-12 20:20:00\t[Combat] TwinkleofToes: Collect Milk on Homer!",
            // Another player's line, and non-milk Combat noise.
            "26-10-12 20:21:00\t[Combat] OtherPlayer: Collect Milk on OtherPlayer!",
            "26-10-12 20:22:29\t[Combat] TwinkleofToes: Poop on TwinkleofToes!",
        ]);

        let events = self_milk_events_in_messages(&messages, "TwinkleofToes", -5 * 3600);
        assert_eq!(
            events,
            vec![(
                "TwinkleofToes".to_string(),
                "2026-10-13T01:14:16+00:00".to_string()
            )]
        );
    }

    #[test]
    fn ability_lines_in_file_prefilters_without_losing_matches() {
        let path = std::env::temp_dir().join(format!(
            "glogger-ability-lines-{}-{}.log",
            std::process::id(),
            "prefilter"
        ));
        std::fs::write(
            &path,
            concat!(
                "**************************************** Logged In As Tufty. Server Dreva. Timezone Offset -05:00:00.\r\n",
                "26-10-12 20:14:16\t[Guild] Someone: hello there\r\n",
                "26-10-12 20:14:16\t[Combat] TwinkleofToes: Collect Milk on TwinkleofToes!\r\n",
                "26-10-12 20:14:20\t[Status] Bottle of Milk added to inventory.\r\n",
                "26-10-12 20:27:35\t[Combat] TwinkleofToes: Poop on TwinkleofToes!\r\n",
            ),
        )
        .unwrap();

        let (messages, file_offset) =
            ability_lines_in_file(&path, &[POOP_LINE_MARKER, SELF_MILK_LINE_MARKER]).unwrap();
        let _ = std::fs::remove_file(&path);

        // The file's own offset travels with its lines — that, not today's
        // setting, is what makes a re-run reprocess the same instant.
        assert_eq!(file_offset, Some(-5 * 3600));

        // Only the two ability lines survive the pre-filter, and they come back
        // as real parsed messages: CRLF stripped, sender split out, time intact.
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].sender.as_deref(), Some("TwinkleofToes"));
        assert_eq!(messages[0].message, "Collect Milk on TwinkleofToes!");
        assert_eq!(
            messages[0]
                .timestamp
                .format("%Y-%m-%d %H:%M:%S")
                .to_string(),
            "2026-10-12 20:14:16"
        );
        assert_eq!(messages[1].message, "Poop on TwinkleofToes!");
    }

    #[test]
    fn ability_lines_in_file_without_a_login_line_reports_no_offset() {
        let path = std::env::temp_dir().join(format!(
            "glogger-ability-lines-{}-{}.log",
            std::process::id(),
            "no-header"
        ));
        std::fs::write(
            &path,
            "26-10-12 20:27:35\t[Combat] TwinkleofToes: Poop on TwinkleofToes!\r\n",
        )
        .unwrap();

        let (messages, file_offset) = ability_lines_in_file(&path, &[POOP_LINE_MARKER]).unwrap();
        let _ = std::fs::remove_file(&path);

        // Caller falls back to the settings offset for such files.
        assert_eq!(file_offset, None);
        assert_eq!(messages.len(), 1);
    }

    /// Opt-in diagnostic over the machine's real ChatLogs (path-dependent, so
    /// ignored by default — same convention as the survey replay tests):
    /// `cargo test --lib -- --ignored backfill_ability_history_smoke`.
    /// Honours `GLOGGER_CHAT_LOGS_DIR` / `GLOGGER_POOP_SMOKE_CHAR` /
    /// `GLOGGER_TZ_OFFSET_SECONDS`.
    #[test]
    #[ignore]
    fn backfill_ability_history_smoke() {
        let dir = match std::env::var("GLOGGER_CHAT_LOGS_DIR") {
            Ok(d) => PathBuf::from(d),
            Err(_) => {
                let home = std::env::var("USERPROFILE")
                    .or_else(|_| std::env::var("HOME"))
                    .expect("no USERPROFILE/HOME");
                PathBuf::from(home).join("AppData/LocalLow/Elder Game/Project Gorgon/ChatLogs")
            }
        };
        let character = std::env::var("GLOGGER_POOP_SMOKE_CHAR")
            .unwrap_or_else(|_| "TwinkleofToes".to_string());
        // Fallback only: every real archive file carries its own login line, so the
        // recorded instants must not depend on this value (asserted below).
        let fallback_tz: i32 = std::env::var("GLOGGER_TZ_OFFSET_SECONDS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(0);

        let files = chat_parser::get_chat_log_files(&dir).unwrap();
        assert!(
            !files.is_empty(),
            "no Chat-*.log files in {}",
            dir.display()
        );

        let conn = rusqlite::Connection::open_in_memory().unwrap();
        crate::db::migrations::run_migrations(&conn, None).unwrap();

        let scan = |conn: &rusqlite::Connection, fallback: i32, record: bool| {
            let mut poops = 0;
            let mut self_milks = 0;
            let mut instants: Vec<String> = Vec::new();
            for f in &files {
                // Same helper the command uses — exercises the byte-level
                // pre-filter and the per-file offset too.
                let (messages, file_offset) =
                    ability_lines_in_file(&f.file_path, &[POOP_LINE_MARKER, SELF_MILK_LINE_MARKER])
                        .unwrap();
                let file_tz = file_offset.unwrap_or(fallback);
                for (target, ts) in self_milk_events_in_messages(&messages, &character, file_tz) {
                    instants.push(ts.clone());
                    if record
                        && record_milking_event(
                            conn,
                            &character,
                            "Dreva",
                            &target,
                            "self_milked",
                            &ts,
                        )
                        .unwrap()
                    {
                        self_milks += 1;
                    }
                }
                for (target, ts) in poop_events_in_messages(&messages, &character) {
                    if record && record_poop(conn, &character, "Dreva", &target, &ts).unwrap() {
                        poops += 1;
                    }
                }
            }
            (poops, self_milks, instants)
        };

        let (poops, self_milks, instants) = scan(&conn, fallback_tz, true);
        let stats =
            crate::db::game_state_commands::get_pooping_stats_for(&conn, &character, "Dreva")
                .unwrap();
        println!(
            "ability history smoke: {poops} poop(s) + {self_milks} self-milk(s) over {} files; {character} total_poops={} days={} first={:?} last={:?}",
            files.len(),
            stats.total_poops,
            stats.days_pooped,
            stats.first_poop,
            stats.last_poop
        );
        assert!(poops > 0, "expected poops in the archived chat logs");
        assert!(
            self_milks > 0,
            "expected self-milks in the archived chat logs"
        );
        assert_eq!(stats.total_poops as usize, poops);
        let second_pass = scan(&conn, fallback_tz, true);
        assert_eq!(
            (second_pass.0, second_pass.1),
            (0, 0),
            "second pass must record nothing new (ON CONFLICT DO NOTHING on both tables)"
        );

        // The self-milk instants are a function of the *logs*, not of the current
        // timezone setting: converting with a wildly different fallback offset must
        // yield byte-identical instants (every archive file has its own login line).
        assert_eq!(
            scan(&conn, fallback_tz + 7 * 3600, false).2,
            instants,
            "self-milk instants must not depend on the settings offset"
        );
    }
}
