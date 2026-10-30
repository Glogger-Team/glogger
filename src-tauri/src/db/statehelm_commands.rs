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
use tauri::{AppHandle, Emitter, Manager};

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
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        // Not silently "no notes in this file": this is the difference between
        // "nothing to reconcile" and "the scan could not read the log".
        Err(e) => {
            startup_log!("Statehelm gift scan skipped {}: {e}", path.display());
            return Vec::new();
        }
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

/// [`has_gift_tastes`] for a row of the stored `npcs` table. The backfill
/// resolves notes against the DB rather than game data, so it evaluates the same
/// rule on the CDN entry it already has (`raw_json`) — one rule, two callers. An
/// unreadable entry keeps the count, mirroring the live arm's "unknown NPC"
/// fallback.
///
/// [`has_gift_tastes`]: crate::game_data::has_gift_tastes
fn stored_has_gift_tastes(key: &str, raw_json: &str) -> bool {
    let Ok(entry) = serde_json::from_str::<serde_json::Value>(raw_json) else {
        return true;
    };
    let mut map = serde_json::Map::new();
    map.insert(key.to_string(), entry);
    let one = serde_json::Value::Object(map).to_string();
    crate::game_data::parse_npcs_json(&one)
        .ok()
        .and_then(|npcs| npcs.get(key).map(crate::game_data::has_gift_tastes))
        .unwrap_or(true)
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
    // to CDN keys — and the widget has nothing to show then either. The caller
    // (`backfill_from_player_logs`) runs this *after* the CDN persist precisely
    // so this is a real "not yet" rather than a race; log it, because the
    // user-visible outcome is otherwise indistinguishable from "no notes found".
    {
        let conn = db.get().map_err(|e| format!("Database error: {e}"))?;
        let npc_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM npcs", [], |row| row.get(0))
            .unwrap_or(0);
        if npc_count == 0 {
            startup_log!(
                "Statehelm gift backfill skipped: npcs table is empty (CDN data not persisted yet)"
            );
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

    // Resolve each display name to its CDN key (case-insensitive and trimmed —
    // the game prints names exactly, but a note with stray casing or
    // whitespace must not fork a new tracker row; mirrors
    // `GameDataState::npc_name_index`; `ORDER BY key` because the table holds
    // duplicate names, e.g. 8× "Work Orders"). Fall back to the raw name,
    // mirroring `resolve_npc`'s raw-name fallback. The CDN entry rides along so
    // the tastes rule can be applied to notes for NPCs whose gifts cannot move
    let resolved: Vec<(String, String, GiftNote, bool)> = {
        let conn = db.get().map_err(|e| format!("Database error: {e}"))?;
        let mut stmt = conn
            .prepare("SELECT key, name, raw_json FROM npcs WHERE name = ?1 COLLATE NOCASE ORDER BY key LIMIT 1")
            .map_err(|e| format!("Query error: {e}"))?;
        latest
            .into_values()
            .map(|note| {
                let (key, name, tastes) = stmt
                    .query_row(rusqlite::params![note.npc_name.trim()], |row| {
                        let key: String = row.get(0)?;
                        let name: String = row.get(1)?;
                        let raw: String = row.get(2)?;
                        Ok((key, name, raw))
                    })
                    .map(|(key, name, raw)| {
                        let tastes = stored_has_gift_tastes(&key, &raw);
                        (key, name, tastes)
                    })
                    .unwrap_or_else(|_| {
                        let trimmed = note.npc_name.trim().to_string();
                        (trimmed.clone(), trimmed, true)
                    });
                (key, name, note, tastes)
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
    for (key, name, note, tastes) in &resolved {
        if !*tastes {
            continue;
        }
        let tx_conn: &rusqlite::Connection = &tx;
        if super::game_state_commands::reconcile_gift_count(
            &super::game_state_commands::GiftReconcile {
                conn: tx_conn,
                character: &note.character,
                server,
                npc_key: key,
                npc_name: name,
                at: &note.at,
                target: note.received,
                week_start,
            },
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
            let mut archived: Vec<(std::time::SystemTime, PathBuf)> = std::fs::read_dir(&dir)
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
    } else {
        // Sibling backfills log their zero results too — and here it is the only
        // way to tell "ran, nothing to reconcile" apart from "never ran".
        startup_log!(
            "Statehelm gift backfill: nothing to reconcile ({} log file(s) scanned)",
            paths.len()
        );
    }
    Ok(changed)
}

/// Tauri command: rescan the logs on demand and reconcile the week's gift
/// counts (the "Rescan logs" button on the Statehelm view).
///
/// `async` + `spawn_blocking`: the scan reads both live logs *and* the whole
/// rotated archive — up to `player_log_archive_max_mb` (512 MiB by default) —
/// and a non-async Tauri command would do that on the main thread, freezing the
/// window for the duration. Mirrors `replay::ingest_player_log`.
#[tauri::command]
pub async fn backfill_statehelm_gifts(app: AppHandle) -> Result<usize, String> {
    let settings = app.state::<Arc<SettingsManager>>().inner().clone();
    let db = app.state::<DbPool>().inner().clone();
    tauri::async_runtime::spawn_blocking(move || backfill_from_player_logs(&settings, &db, &app))
        .await
        .map_err(|e| format!("Backfill task failed: {e}"))?
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::io::Write;

    use rusqlite::Connection;

    const WEEK_START: &str = "2026-10-12 00:00:00";

    /// Unique temp path per call: `SystemTime` nanos repeat when parallel tests
    /// start in the same tick, so a bare-nanos name let two helpers open one
    /// path — contents from one test leaked into another's assertion.
    fn unique_temp_path(prefix: &str, ext: &str) -> PathBuf {
        use std::sync::atomic::{AtomicU64, Ordering};
        static SEQ: AtomicU64 = AtomicU64::new(0);
        let n = SEQ.fetch_add(1, Ordering::Relaxed);
        // Nanos kept for human-sortable names; pid + counter carry uniqueness.
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let mut path = std::env::temp_dir();
        path.push(format!("{prefix}_{}_{nanos}_{n}.{ext}", std::process::id()));
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

    fn write_log(lines: &str) -> PathBuf {
        let path = unique_temp_path("glogger_statehelm_test", "log");
        let mut f = std::fs::File::create(&path).expect("create log");
        f.write_all(lines.as_bytes()).expect("write log");
        path
    }

    fn temp_pool() -> (DbPool, PathBuf) {
        let path = unique_temp_path("glogger_statehelm_test", "db");
        let pool = crate::db::init_pool(path.clone(), Some(0)).expect("init pool");
        (pool, path)
    }

    /// A CDN-shaped `npcs` row. The gift backfill reads the stored entry's gift
    /// data (`Preferences` / `ItemGifts`) to decide whether the NPC's gifts can
    /// move their reputation at all, so this fixture carries real gift data —
    /// Corinth has one preference in the CDN.
    fn insert_npc(conn: &Connection, key: &str, name: &str) {
        conn.execute(
            "INSERT INTO npcs (key, name, raw_json) VALUES (?1, ?2, ?3)",
            rusqlite::params![
                key,
                name,
                r#"{"Preferences":[{"Name":"Magical Dust","Desire":"Like","Pref":1.0}]}"#
            ],
        )
        .unwrap();
    }

    /// A note for an NPC with no gift data in the CDN is ignored: gifts cannot
    /// move their reputation, so the weekly count is noise. Gossamyr the Great's
    /// real shape (no `Preferences`, no `ItemGifts`, `AreaStatehelm`) — his
    /// accepted gift produced no favor line at all, and the live arm drops his
    /// notes the same way, so the two paths must agree here.
    #[test]
    fn backfill_skips_notes_for_npcs_without_gift_data() {
        let (pool, db_path) = temp_pool();
        {
            let conn = pool.get_write().unwrap();
            conn.execute(
                "INSERT INTO npcs (key, name, raw_json) VALUES ('NPC_Gossamyr', \
                 'Gossamyr the Great', '{\"AreaName\":\"AreaStatehelm\"}')",
                [],
            )
            .unwrap();
            // A tasting NPC in the same run — the gate must be selective.
            insert_npc(&conn, "NPC_Corinth", "Corinth");
        }

        let log = format!(
            "[12:04:37] Logged in as character TestPlayer. Time UTC=10/14/2026 12:04:37. Timezone Offset -05:00:00\n{}\n{}\n",
            note_line("12:20:00", "Gossamyr the Great", 3),
            note_line("12:25:00", "Corinth", 1),
        );
        let path = write_log(&log);

        let changed =
            backfill_gifts_from_paths(std::slice::from_ref(&path), WEEK_START, "TestServer", &pool)
                .unwrap();
        assert_eq!(changed, 1, "only the NPC with gift data is reconciled");

        let rows: Vec<(String, i64)> = pool
            .get()
            .unwrap()
            .prepare("SELECT npc_key, COUNT(*) FROM game_state_gift_log GROUP BY npc_key")
            .unwrap()
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(rows, vec![("NPC_Corinth".to_string(), 1)]);

        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(&db_path);
    }

    /// The two "keep the count" fallbacks around `stored_has_gift_tastes`: a row
    /// whose `raw_json` is not readable keeps the count *inside* that function
    /// (defensive — production always persists a serialized CDN entry, so such a
    /// row is corrupt/foreign, not an unknown NPC), and an NPC absent from the
    /// `npcs` table never reaches it: the caller's `unwrap_or_else` resolves the
    /// note to its raw name with `tastes = true`, mirroring `resolve_npc`. Neither
    /// may be treated as tasteless — a regression there would silently stop
    /// reconciling notes for every NPC missing from `npcs`.
    #[test]
    fn backfill_keeps_notes_for_unresolved_and_unreadable_npcs() {
        let (pool, db_path) = temp_pool();
        {
            let conn = pool.get_write().unwrap();
            conn.execute(
                "INSERT INTO npcs (key, name, raw_json) VALUES ('NPC_Broken', 'Broken', 'not json')",
                [],
            )
            .unwrap();
        }

        let log = format!(
            "[12:04:37] Logged in as character TestPlayer. Time UTC=10/14/2026 12:04:37. Timezone Offset -05:00:00\n{}\n{}\n",
            note_line("12:20:00", "Broken", 2),
            note_line("12:25:00", "Absent From Cdn", 1),
        );
        let path = write_log(&log);

        let changed =
            backfill_gifts_from_paths(std::slice::from_ref(&path), WEEK_START, "TestServer", &pool)
                .unwrap();
        assert_eq!(changed, 2, "both fallbacks keep the count");

        let rows: Vec<(String, i64)> = pool
            .get()
            .unwrap()
            .prepare("SELECT npc_key, COUNT(*) FROM game_state_gift_log GROUP BY npc_key ORDER BY npc_key")
            .unwrap()
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(
            rows,
            vec![
                ("Absent From Cdn".to_string(), 1),
                ("NPC_Broken".to_string(), 2),
            ]
        );

        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(&db_path);
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
            &crate::db::game_state_commands::GiftReconcile {
                conn: &conn,
                character: "TestPlayer",
                server: "TestServer",
                npc_key: "NPC_Corinth",
                npc_name: "Corinth",
                at: "2026-10-12 04:00:00",
                target: 1,
                week_start: WEEK,
            },
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

    /// The backfill resolves display names against the persisted `npcs` table
    /// (the live arm uses the in-memory CDN index instead), so it is a no-op on a
    /// DB whose CDN data is not persisted yet — the reason `lib.rs` runs it
    /// *after* `persist_cdn_data` rather than in the Step 5d chain, which is
    /// spawned independently and loses that race on a fresh install. The same
    /// file must reconcile once `npcs` is populated.
    #[test]
    fn backfill_no_ops_until_npcs_are_persisted() {
        let (pool, db_path) = temp_pool();
        let log = format!(
            "[12:04:37] Logged in as character TestPlayer. Time UTC=10/14/2026 12:04:37. Timezone Offset -05:00:00\n{}\n",
            note_line("12:20:00", "Corinth", 2),
        );
        let path = write_log(&log);

        assert_eq!(
            backfill_gifts_from_paths(std::slice::from_ref(&path), WEEK_START, "TestServer", &pool)
                .unwrap(),
            0,
            "no NPC rows to resolve display names against"
        );
        let rows: i64 = pool
            .get()
            .unwrap()
            .query_row("SELECT COUNT(*) FROM game_state_gift_log", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(rows, 0, "and nothing is written");

        insert_npc(&pool.get_write().unwrap(), "NPC_Corinth", "Corinth");
        assert_eq!(
            backfill_gifts_from_paths(std::slice::from_ref(&path), WEEK_START, "TestServer", &pool)
                .unwrap(),
            1,
            "the same log reconciles once the CDN is persisted"
        );

        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(&db_path);
    }

    /// The live gift arm resolves a note's display name through the in-memory
    /// CDN index (`game_data::build_npc_name_index`) and the backfill through
    /// SQL (`ORDER BY key LIMIT 1`). Both must land one duplicate display name
    /// on the *same* NPC key — the tracker keys rows by key, so a disagreement
    /// splits one NPC's weekly count across two rows.
    #[test]
    fn duplicate_display_names_resolve_identically_in_the_live_and_sql_paths() {
        let (pool, db_path) = temp_pool();
        let pairs: Vec<(String, String)> = vec![
            ("NPC_TwinB".into(), "Twin".into()),
            ("NPC_TwinA".into(), "Twin".into()),
        ];
        {
            let conn = pool.get_write().unwrap();
            for (key, name) in &pairs {
                insert_npc(&conn, key, name);
            }
        }

        let sql_pick: String = pool
            .get()
            .unwrap()
            .query_row(
                "SELECT key FROM npcs WHERE name = ?1 COLLATE NOCASE ORDER BY key LIMIT 1",
                ["Twin"],
                |row| row.get(0),
            )
            .unwrap();
        let index_pick =
            crate::game_data::build_npc_name_index(pairs.iter().map(|(key, name)| (key, name)))
                .remove("Twin")
                .unwrap();

        assert_eq!(sql_pick, "NPC_TwinA", "the lowest key wins");
        assert_eq!(index_pick, sql_pick, "one key for both resolvers");

        let _ = std::fs::remove_file(&db_path);
    }

    /// Notes resolve case-insensitively: a mixed-case display name lands on the
    /// same CDN key instead of forking a raw-name row.
    #[test]
    fn backfill_resolves_mixed_case_npc_names() {
        let (pool, db_path) = temp_pool();
        {
            let conn = pool.get_write().unwrap();
            insert_npc(&conn, "NPC_Corinth", "Corinth");
        }

        let log = format!(
            "[12:04:37] Logged in as character TestPlayer. Time UTC=10/14/2026 12:04:37. Timezone Offset -05:00:00\n{}\n",
            note_line("12:20:00", "cOrInTh", 2),
        );
        let path = write_log(&log);

        let changed =
            backfill_gifts_from_paths(std::slice::from_ref(&path), WEEK_START, "TestServer", &pool)
                .unwrap();
        assert_eq!(changed, 1, "mixed-case note reconciles");

        let key: String = pool
            .get()
            .unwrap()
            .query_row("SELECT npc_key FROM game_state_gift_log", [], |row| row.get(0))
            .unwrap();
        assert_eq!(key, "NPC_Corinth", "lands on the CDN key, not the raw note");

        let _ = std::fs::remove_file(&path);
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

        let changed =
            backfill_gifts_from_paths(std::slice::from_ref(&path), WEEK_START, "TestServer", &pool)
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
        let changed_again =
            backfill_gifts_from_paths(std::slice::from_ref(&path), WEEK_START, "TestServer", &pool)
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

        let changed =
            backfill_gifts_from_paths(std::slice::from_ref(&path), WEEK_START, "TestServer", &pool)
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
