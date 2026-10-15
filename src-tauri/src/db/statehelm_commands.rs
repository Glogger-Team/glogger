//! Statehelm gift-status backfill from the game's own logs.
//!
//! The Statehelm weekly gift tracker (`game_state_gift_log`) only reflects
//! gifts glogger was *running* for. Gifts given while glogger was closed are
//! lost: `Player.log` and the game's rotated `Player-prev.log` hold the
//! gift-capped NPCs' authoritative weekly counts ("… has received <b>2</b> so
//! far this week"), but only the live parser ever reads them.
//!
//! This module re-reads those counts from the logs glogger already watches
//! (current `Player.log`, rotated `Player-prev.log`, plus archived rotated
//! copies) and reconciles each capped NPC's current-week row count to the
//! game's number. No new game interaction — log text only (see
//! `docs/research/glogger-game-interaction-limits.md` §3).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use chrono::{NaiveTime, TimeZone, Utc};
use tauri::{AppHandle, Emitter, State};

use super::DbPool;
use crate::player_event_parser::{parse_gift_count_line, PlayerEvent};
use crate::settings::SettingsManager;

/// Timestamped log line for startup diagnostics (mirrors the other
/// `db::*_commands` modules' local macro — `startup_log!` is per-module).
macro_rules! startup_log {
    ($($arg:tt)*) => {
        eprintln!("[{}] {}", chrono::Local::now().format("%H:%M:%S%.3f"), format!($($arg)*));
    };
}

/// One gift-cap note observed in a Player.log-family file.
pub struct GiftNote {
    pub character: String,
    /// `"YYYY-MM-DD HH:MM:SS"` (DB format, UTC).
    pub at: String,
    /// Display name as printed by the game.
    pub npc_name: String,
    /// The game's count "so far this week".
    pub received: u32,
}

/// Scan one Player.log-family file for gift-cap notes inside `week_start` onward.
///
/// Single pass with a cheap pre-filter and no [`PlayerEventParser`] instance.
/// Lines carry only `[HH:MM:SS]` (UTC, no date), so the date is anchored to the
/// file's `Logged in as character …` line and advanced one day whenever the
/// time of day goes backwards (midnight rollover). A file with no login line is
/// undatable and yields nothing rather than guessing a date.
///
/// Deliberately does NOT replay `ProcessDeltaFavor` lines: their `FavorChanged`
/// handling *adds* to `game_state_favor.cumulative_delta`, so replaying them
/// would double-count favor. The absolute note count is the only signal needed.
pub fn scan_gift_notes(path: &Path, week_start: &str) -> Vec<GiftNote> {
    let Ok(text) = std::fs::read_to_string(path) else {
        return Vec::new();
    };

    let mut notes = Vec::new();
    let mut character: Option<String> = None;
    let mut date: Option<chrono::NaiveDate> = None;
    let mut prev_time: Option<NaiveTime> = None;

    for line in text.lines() {
        if line.contains("Logged in as character ") {
            if let Some((name, utc)) = crate::parsers::parse_player_log_login_line(line) {
                character = Some(name);
                date = Some(utc.date());
                // Seed the rollover baseline from the login line's own time of
                // day, as `LogDateResolver::anchor_session` does — otherwise the
                // first line after a login can never roll at UTC midnight and a
                // note near the week boundary is dated a day early.
                prev_time = Some(utc.time());
            }
            continue;
        }

        // Lines with no timestamp cannot be dated (and are never gift notes).
        let Some(ts) = crate::parsers::parse_timestamp(line) else {
            continue;
        };
        let Ok(time) = NaiveTime::parse_from_str(&ts, "%H:%M:%S") else {
            continue;
        };
        // PG writes `[HH:MM:SS]` in UTC, so `date + time` is the UTC instant. A
        // decrease in time-of-day means the file crossed midnight.
        if let (Some(prev), Some(d)) = (prev_time, date) {
            if time < prev {
                date = d.succ_opt();
            }
        }
        prev_time = Some(time);

        let (Some(character), Some(date)) = (character.as_deref(), date) else {
            continue;
        };

        // Identical condition to the live parser's dispatch — backfill and live
        // agree line-for-line.
        if !line.contains("gifts per calendar week and has received") {
            continue;
        }
        let Some(PlayerEvent::GiftCountObserved {
            npc_name, received, ..
        }) = parse_gift_count_line(line)
        else {
            continue;
        };

        let at = format!("{} {}", date.format("%Y-%m-%d"), ts);
        // Drop notes outside the requested week — a stale last-week note would
        // otherwise be reconciled against this week's rows. Both stamps are in
        // the same DB format, so the string compare is chronological.
        if at.as_str() < week_start {
            continue;
        }

        notes.push(GiftNote {
            character: character.to_string(),
            at,
            npc_name,
            received,
        });
    }

    notes
}

/// Reconcile this week's gift-log rows for every capped NPC named in `paths`.
///
/// Idempotent by construction: each NPC's row count is set to the game's
/// absolute weekly count, so re-running after the rows already match changes
/// nothing. Returns the number of NPCs whose row count changed.
pub fn backfill_gifts_from_paths(
    paths: &[PathBuf],
    week_start: &str,
    server: &str,
    db: &DbPool,
) -> Result<usize, String> {
    if paths.is_empty() {
        return Ok(0);
    }

    // A fresh DB has no CDN persisted yet, so display names cannot be resolved
    // to CDN keys — and the widget has nothing to show then either.
    {
        let conn = db.get().map_err(|e| format!("Database error: {e}"))?;
        let npc_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM npcs", [], |row| row.get(0))
            .unwrap_or(0);
        if npc_count == 0 {
            return Ok(0);
        }
    }

    // Parse everything first, then merge per (character, npc_name) keeping the
    // latest note — the game's own count at its last observation (e.g. a
    // pre-gift "0" prompt then a post-gift "5" → 5).
    let mut latest: HashMap<(String, String), GiftNote> = HashMap::new();
    for path in paths {
        for note in scan_gift_notes(path, week_start) {
            let key = (note.character.clone(), note.npc_name.clone());
            match latest.get(&key) {
                Some(existing) if existing.at >= note.at => {}
                _ => {
                    latest.insert(key, note);
                }
            }
        }
    }
    if latest.is_empty() {
        return Ok(0);
    }

    // Resolve each display name to its CDN key (exact-name match, mirroring
    // `GameDataState::npc_name_index`; `ORDER BY key` because the table holds
    // duplicate names, e.g. 8× "Work Orders"). Fall back to the raw name,
    // mirroring `resolve_npc`'s raw-name fallback.
    let resolved: Vec<(String, String, GiftNote)> = {
        let conn = db.get().map_err(|e| format!("Database error: {e}"))?;
        let mut stmt = conn
            .prepare("SELECT key, name FROM npcs WHERE name = ?1 ORDER BY key LIMIT 1")
            .map_err(|e| format!("Query error: {e}"))?;
        latest
            .into_values()
            .map(|note| {
                let (key, name) = stmt
                    .query_row(rusqlite::params![&note.npc_name], |row| {
                        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                    })
                    .unwrap_or_else(|_| (note.npc_name.clone(), note.npc_name.clone()));
                (key, name, note)
            })
            .collect()
    };

    // PARSE FIRST, WRITER SECOND: never hold the sole write connection while
    // reading/parsing the logs. BEGIN IMMEDIATE so `busy_timeout` applies to
    // acquisition (mirrors `backfill_bets_from_player_logs`).
    let mut conn = db
        .get_write()
        .map_err(|e| format!("Database connection error: {e}"))?;
    let tx = conn
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .map_err(|e| format!("Failed to begin transaction: {e}"))?;

    let mut changed = 0usize;
    for (key, name, note) in &resolved {
        if super::game_state_commands::reconcile_gift_count(
            &tx,
            &note.character,
            server,
            key,
            name,
            &note.at,
            note.received,
            week_start,
        )
        .map_err(|e| format!("Reconcile error: {e}"))?
        {
            changed += 1;
        }
    }

    tx.commit().map_err(|e| format!("Commit error: {e}"))?;
    Ok(changed)
}

/// mtime threshold for archived logs: `week_start` minus one day. Archived
/// copies are named by content hash; a file whose mtime (≈ its session end) is
/// older than a day before this week began cannot hold this week's notes.
fn archive_mtime_threshold(week_start: &str) -> Option<std::time::SystemTime> {
    let dt = chrono::NaiveDateTime::parse_from_str(week_start, "%Y-%m-%d %H:%M:%S")
        .ok()?
        .checked_sub_signed(chrono::Duration::days(1))?;
    let secs = Utc.from_utc_datetime(&dt).timestamp().max(0) as u64;
    Some(std::time::UNIX_EPOCH + std::time::Duration::from_secs(secs))
}

/// Backfill the current week's gift counts from `Player-prev.log`, `Player.log`,
/// then any recent archived rotated copies. Emits `game-state-updated
/// ["favor"]` when any NPC was reconciled. Returns the number changed.
pub fn backfill_from_player_logs(
    settings: &SettingsManager,
    db: &DbPool,
    app: &AppHandle,
) -> Result<usize, String> {
    let week_start = crate::game_state::current_week_start_utc();

    // Rows must be attributable to the same (character, server) the widget
    // queries; without an active server there is nothing to reconcile.
    let server = match settings.get().active_server_name {
        Some(s) if !s.is_empty() => s,
        _ => {
            eprintln!("[statehelm] gift backfill skipped: no active server set");
            return Ok(0);
        }
    };

    let mut paths: Vec<PathBuf> = Vec::new();

    // Archived rotated copies first — only those recent enough to possibly hold
    // this week's notes. (All files are still scanned for the week filter.)
    if let Some(dir) = crate::replay::player_log_archive_dir(settings) {
        if let Some(threshold) = archive_mtime_threshold(&week_start) {
            let mut archived: Vec<(std::time::SystemTime, PathBuf)> =
                std::fs::read_dir(&dir)
                    .into_iter()
                    .flatten()
                    .flatten()
                    .filter_map(|entry| {
                        let path = entry.path();
                        let name = path.file_name()?.to_str()?;
                        if !name.starts_with("Player-") || !name.ends_with(".log") {
                            return None;
                        }
                        let mtime = entry.metadata().ok()?.modified().ok()?;
                        if mtime < threshold {
                            return None;
                        }
                        Some((mtime, path))
                    })
                    .collect();
            archived.sort_by_key(|(mtime, _)| *mtime);
            paths.extend(archived.into_iter().map(|(_, p)| p));
        }
    }

    // The two live logs are always scanned regardless of mtime.
    if let Some(p) = settings.get_player_prev_log_path() {
        if p.is_file() {
            paths.push(p);
        }
    }
    if let Some(p) = settings.get_player_log_path() {
        if p.is_file() {
            paths.push(p);
        }
    }

    let changed = backfill_gifts_from_paths(&paths, &week_start, &server, db)?;
    if changed > 0 {
        app.emit("game-state-updated", vec!["favor"]).ok();
        startup_log!(
            "Statehelm gift backfill: {} NPC count(s) reconciled",
            changed
        );
    }
    Ok(changed)
}

/// Tauri command: rescan the logs on demand and reconcile the week's gift
/// counts (the "Rescan logs" button on the Statehelm view).
#[tauri::command]
pub fn backfill_statehelm_gifts(
    settings: State<'_, Arc<SettingsManager>>,
    db: State<'_, DbPool>,
    app: AppHandle,
) -> Result<usize, String> {
    backfill_from_player_logs(&settings, &db, &app)
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::io::Write;

    use rusqlite::Connection;

    const WEEK_START: &str = "2026-10-12 00:00:00";

    fn write_log(lines: &str) -> PathBuf {
        let mut path = std::env::temp_dir();
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        path.push(format!("glogger_statehelm_test_{nanos}.log"));
        let mut f = std::fs::File::create(&path).expect("create log");
        f.write_all(lines.as_bytes()).expect("write log");
        path
    }

    /// A note line as the game prints it, embedded in a talk screen. The real
    /// note is preceded by a sentence ending in ". " (the parser takes the NPC
    /// name from the text after that last ". "), so the fixture must include it.
    fn note_line(ts: &str, npc: &str, received: u32) -> String {
        format!(
            "[{ts}] LocalPlayer: ProcessTalkScreen(25493, \"\", \"Thanks!\\n\\n<i>[Note: Statehelm villagers value quality over quantity. {npc} will accept up to <b>5</b> gifts per calendar week and has received <b>{received}</b> so far this week. (The week resets on Monday EDT.)]</i>\", \"\", [-1201,], System.String[], -1301, Generic)"
        )
    }

    fn temp_pool() -> (DbPool, PathBuf) {
        let mut path = std::env::temp_dir();
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        path.push(format!("glogger_statehelm_test_{nanos}.db"));
        let pool = crate::db::init_pool(path.clone(), Some(0)).expect("init pool");
        (pool, path)
    }

    fn insert_npc(conn: &Connection, key: &str, name: &str) {
        conn.execute(
            "INSERT INTO npcs (key, name, raw_json) VALUES (?1, ?2, '{}')",
            rusqlite::params![key, name],
        )
        .unwrap();
    }

    #[test]
    fn scan_keeps_only_in_week_notes() {
        // Two sessions in one file: a pre-week note (Sunday 10/11) must be
        // dropped, the in-week note (10/14) kept; the second login re-anchors
        // the date.
        let log = format!(
            "[12:00:00] Logged in as character TestPlayer. Time UTC=10/11/2026 12:00:00. Timezone Offset -05:00:00\n{}\n[12:04:37] Logged in as character TestPlayer. Time UTC=10/14/2026 12:04:37. Timezone Offset -05:00:00\n{}\n",
            note_line("12:30:00", "Corinth", 1),
            note_line("12:10:00", "Corinth", 2),
        );
        let path = write_log(&log);

        let notes = scan_gift_notes(&path, WEEK_START);
        assert_eq!(notes.len(), 1, "only the in-week note survives");
        assert_eq!(notes[0].npc_name, "Corinth");
        assert_eq!(notes[0].received, 2);
        assert_eq!(notes[0].character, "TestPlayer");
        assert_eq!(notes[0].at, "2026-10-14 12:10:00");

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn scan_drops_notes_dated_before_week_start() {
        // Login on the day before the week began (Sunday 10/11), then a note
        // that same day → outside the week.
        let log = format!(
            "[12:00:00] Logged in as character TestPlayer. Time UTC=10/11/2026 12:00:00. Timezone Offset -05:00:00\n{}\n",
            note_line("12:30:00", "Corinth", 2),
        );
        let path = write_log(&log);
        assert!(scan_gift_notes(&path, WEEK_START).is_empty());
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn scan_rolls_date_over_midnight() {
        let log = format!(
            "[23:55:00] Logged in as character TestPlayer. Time UTC=10/13/2026 23:55:00. Timezone Offset -05:00:00\n{}\n{}\n",
            note_line("23:58:00", "Corinth", 1),
            note_line("00:05:00", "Corinth", 2),
        );
        let path = write_log(&log);

        let notes = scan_gift_notes(&path, WEEK_START);
        assert_eq!(notes.len(), 2);
        // Post-midnight note rolls to 10/14.
        assert_eq!(notes[1].at, "2026-10-14 00:05:00");

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn scan_without_login_line_yields_nothing() {
        let log = format!("{}\n", note_line("12:10:00", "Corinth", 2));
        let path = write_log(&log);
        assert!(scan_gift_notes(&path, WEEK_START).is_empty());
        let _ = std::fs::remove_file(&path);
    }

    /// A manual `+` row written by an older build carries an ISO stamp. Falling
    /// before the Monday-04:00-UTC reset it is *last* week's gift (Sunday
    /// evening ET), so the week-boundary comparisons must not count it just
    /// because `'T' > ' '` sorts it after the same date. `add_manual_gift` now
    /// writes the DB format; this pins the tolerance for legacy rows.
    #[test]
    fn legacy_iso_rows_are_not_counted_into_the_new_week() {
        const WEEK: &str = "2026-10-12 04:00:00";
        let (pool, db_path) = temp_pool();
        {
            let conn = pool.get_write().unwrap();
            insert_npc(&conn, "NPC_Corinth", "Corinth");
            conn.execute(
                "INSERT INTO game_state_gift_log
                   (character_name, server_name, npc_key, npc_name, gifted_at, favor_delta)
                 VALUES ('TestPlayer', 'TestServer', 'NPC_Corinth', 'Corinth',
                         '2026-10-12T02:00:00Z', 0.0)",
                [],
            )
            .unwrap();
        }

        let conn = pool.get_write().unwrap();
        let changed = crate::db::game_state_commands::reconcile_gift_count(
            &conn,
            "TestPlayer",
            "TestServer",
            "NPC_Corinth",
            "Corinth",
            "2026-10-12 04:00:00",
            1,
            WEEK,
        )
        .unwrap();
        assert!(
            changed,
            "the stale ISO row must not satisfy the game's count of 1"
        );

        let in_week: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM game_state_gift_log
                 WHERE replace(gifted_at, 'T', ' ') >= ?1",
                [WEEK],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(in_week, 1, "exactly the game's one gift this week");

        let total: i64 = conn
            .query_row("SELECT COUNT(*) FROM game_state_gift_log", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(total, 2, "the stale row is kept, just not counted");

        let _ = std::fs::remove_file(&db_path);
    }

    #[test]
    fn backfill_adds_rows_to_match_game_count_and_is_idempotent() {
        let (pool, db_path) = temp_pool();
        {
            let conn = pool.get_write().unwrap();
            insert_npc(&conn, "NPC_Corinth", "Corinth");
        }

        // Two notes for the same NPC: keep the latest (2).
        let log = format!(
            "[12:04:37] Logged in as character TestPlayer. Time UTC=10/14/2026 12:04:37. Timezone Offset -05:00:00\n{}\n{}\n",
            note_line("12:10:00", "Corinth", 0),
            note_line("12:20:00", "Corinth", 2),
        );
        let path = write_log(&log);

        let changed = backfill_gifts_from_paths(
            std::slice::from_ref(&path),
            WEEK_START,
            "TestServer",
            &pool,
        )
        .unwrap();
        assert_eq!(changed, 1, "Corinth reconciled");

        {
            let conn = pool.get().unwrap();
            let count: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM game_state_gift_log
                     WHERE character_name = 'TestPlayer' AND server_name = 'TestServer'
                       AND npc_key = 'NPC_Corinth' AND gifted_at >= ?1",
                    [WEEK_START],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(count, 2, "two rows for the game's count of 2");
            let sum: f64 = conn
                .query_row(
                    "SELECT COALESCE(SUM(favor_delta), 0) FROM game_state_gift_log
                     WHERE npc_key = 'NPC_Corinth'",
                    [],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(sum, 0.0, "backfilled rows carry zero favor delta");
        }

        // Idempotent: second run changes nothing.
        let changed_again = backfill_gifts_from_paths(
            std::slice::from_ref(&path),
            WEEK_START,
            "TestServer",
            &pool,
        )
        .unwrap();
        assert_eq!(changed_again, 0, "second call is a no-op");
        {
            let conn = pool.get().unwrap();
            let count: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM game_state_gift_log WHERE npc_key = 'NPC_Corinth'",
                    [],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(count, 2, "table unchanged on re-run");
        }

        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(&db_path);
    }

    #[test]
    fn backfill_trims_over_count_and_leaves_other_weeks_alone() {
        let (pool, db_path) = temp_pool();
        {
            let conn = pool.get_write().unwrap();
            insert_npc(&conn, "NPC_Corinth", "Corinth");
            // Five rows this week, plus one row from a previous week.
            for i in 0..5 {
                conn.execute(
                    "INSERT INTO game_state_gift_log (character_name, server_name, npc_key, npc_name, gifted_at, favor_delta)
                     VALUES ('TestPlayer', 'TestServer', 'NPC_Corinth', 'Corinth', ?1, 0.0)",
                    rusqlite::params![format!("2026-10-13 1{i}:00:00")],
                )
                .unwrap();
            }
            conn.execute(
                "INSERT INTO game_state_gift_log (character_name, server_name, npc_key, npc_name, gifted_at, favor_delta)
                 VALUES ('TestPlayer', 'TestServer', 'NPC_Corinth', 'Corinth', '2026-10-05 12:00:00', 0.0)",
                [],
            )
            .unwrap();
        }

        let log = format!(
            "[12:04:37] Logged in as character TestPlayer. Time UTC=10/14/2026 12:04:37. Timezone Offset -05:00:00\n{}\n",
            note_line("12:20:00", "Corinth", 2),
        );
        let path = write_log(&log);

        let changed = backfill_gifts_from_paths(
            std::slice::from_ref(&path),
            WEEK_START,
            "TestServer",
            &pool,
        )
        .unwrap();
        assert_eq!(changed, 1, "over-count reconcile counts as a change");

        {
            let conn = pool.get().unwrap();
            let this_week: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM game_state_gift_log
                     WHERE npc_key = 'NPC_Corinth' AND gifted_at >= ?1",
                    [WEEK_START],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(this_week, 2, "trimmed down to the game's count");

            let prev_week: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM game_state_gift_log
                     WHERE npc_key = 'NPC_Corinth' AND gifted_at < ?1",
                    [WEEK_START],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(prev_week, 1, "previous-week row untouched");
        }

        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(&db_path);
    }
}
