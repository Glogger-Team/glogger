//! Books-directory watcher — ingests the game's own `.txt` book exports.
//!
//! The game writes one `.txt` per opened book into `<game_data_path>/Books/`
//! (`SkillReport_*`, `PlayerAge_*`, `HelpScreen_*`, `PlayerShopLog_*`,
//! `GuildMotd_*`, `MessageOfTheDay_*`, `NewQuestquest_*`). Files written while
//! glogger was closed previously sat unimported: the live coordinator only
//! sees books whose `ProcessBook` line streams through the running log tail.
//! This watcher mirrors `replay::spawn_player_prev_watcher` (background
//! thread, periodic scan, change detection) but tracks per-file mtimes and
//! needs no settings gate — book ingestion is unconditional.
//!
//! Content contract: live `ProcessBook` content carries escaped newlines
//! (`\n`), book files carry real newlines. Everything here normalizes through
//! `coordinator::normalize_book_content` once per file, so the shared
//! ingestion helpers in `coordinator` see identical content on both paths.

use crate::cdn_commands::GameDataState;
use crate::coordinator::{
    ingest_hoplology_content, ingest_report_stats_content, ingest_teleport_binds_content,
    persist_book_content,
};
use crate::db::stall_tracker_commands::{insert_stall_events, StallEventInput, StallOpsLock};
use crate::db::DbPool;
use crate::settings::SettingsManager;
use crate::shop_log_parser::parse_shop_log;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};
use tauri::{AppHandle, Emitter, Manager};

/// Initial settle delay before the first scan so startup DB churn
/// (migrations, CDN init, catch-up polling handoff) isn't contended.
const INITIAL_DELAY_SECS: u64 = 15;

/// Rescan cadence.
const SCAN_INTERVAL_SECS: u64 = 30;

/// Spawn the Books-directory watcher thread. Runs for the process lifetime;
/// re-reads settings each tick so `game_data_path` changes take effect
/// without a restart.
pub fn spawn_books_watcher(settings: std::sync::Arc<SettingsManager>, db: DbPool, app: AppHandle) {
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_secs(INITIAL_DELAY_SECS));
        let mut seen: HashMap<PathBuf, SystemTime> = HashMap::new();
        // Consecutive-failure counter per (path, mtime); keyed by mtime so a
        // rewrite of a failing file resets the count. Thread-local to the
        // watcher loop, alongside `seen`.
        let mut failures: HashMap<(PathBuf, SystemTime), u32> = HashMap::new();

        loop {
            scan_books_dir(&settings, &db, &app, &mut seen, &mut failures);
            std::thread::sleep(Duration::from_secs(SCAN_INTERVAL_SECS));
        }
    });
}

/// Consecutive failed attempts after which a book file is given up on (logged
/// once, then recorded in `seen` so it stops retrying every tick). A
/// deterministically broken file — permanently non-parseable content, a
/// vanished encoding — would otherwise retry forever. Mid-write partial
/// files are not dropped prematurely: a file rewritten after a failed
/// attempt gets a fresh mtime, which resets its counter to zero, so the
/// settle window is preserved.
const GIVE_UP_AFTER_ATTEMPTS: u32 = 3;

/// Errors that mean "not the file's fault, retrying makes sense": the DB
/// write pool is momentarily contended, or shared game data hasn't loaded
/// yet (hoplology's CDN gate). These never advance the give-up counter, so
/// a file stuck behind transient contention waits indefinitely instead of
/// being dropped after 3 × `SCAN_INTERVAL_SECS`.
const TRANSIENT_ERROR_MARKERS: &[&str] = &[
    "Database error:", // DbPool::get_write r2d2 error prefix
    "Database connection error:", // coordinator::persist_book_content / books_watcher gourmand + stall paths
    "Failed to persist report stats:", // coordinator::ingest_report_stats_content gate
    "Game data not loaded yet", // coordinator::ingest_hoplology_content gate
    "GameDataState not managed yet", // books_watcher hoplology path before GameDataState is provided
];

/// One scan pass. First pass ingests everything on disk (backfill); later
/// passes only touch files whose mtime changed. Removed files are dropped
/// from the tracking map so it never grows unbounded.
///
/// Attribution caveat for the first-scan backfill: `<game_data>/Books/`
/// accumulates exports across ALL alts, so the first scan ingests every file
/// under whichever character is active at scan time. Stats reports
/// (PlayerAge / HelpScreen) self-identify their owner and are skipped when it
/// differs (see `stats_attribution_skip`); SkillReport files carry no owner in
/// their content and CANNOT be attributed — their binds / hoplology / gourmand
/// backfills land under the currently-active character. The live
/// per-session path remains authoritative for SkillReport-derived state.
fn scan_books_dir(
    settings: &SettingsManager,
    db: &DbPool,
    app: &AppHandle,
    seen: &mut HashMap<PathBuf, SystemTime>,
    failures: &mut HashMap<(PathBuf, SystemTime), u32>,
) {
    let settings = settings.get();
    let Some(game_data_path) = opt_nonempty(settings.game_data_path.as_str()) else {
        return;
    };
    let Some(character) = opt_nonempty(settings.active_character_name.as_deref().unwrap_or(""))
    else {
        return;
    };
    // Mirror the live `active_character_server()` guard: persisting under an
    // empty server would land rows the UI (scoped to the active server) never
    // sees, and once the real server is detected the same books would be
    // persisted again as a second row set.
    let Some(server) = opt_nonempty(settings.active_server_name.as_deref().unwrap_or("")) else {
        return;
    };

    let books_dir = Path::new(&game_data_path).join("Books");
    if !books_dir.is_dir() {
        return;
    }

    let Ok(entries) = std::fs::read_dir(&books_dir) else {
        return;
    };

    let mut to_process: Vec<(PathBuf, SystemTime)> = Vec::new();

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let is_txt = path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| e.eq_ignore_ascii_case("txt"));
        if !is_txt {
            continue;
        }
        let Ok(mtime) = entry.metadata().and_then(|m| m.modified()) else {
            continue;
        };
        if seen.get(&path) != Some(&mtime) {
            to_process.push((path, mtime));
        }
    }

    if to_process.is_empty() {
        // Still prune: removed files must leave the map or it grows
        // unboundedly (and switching game_data_path strands every entry).
        seen.retain(|path, _| path.is_file() && path.starts_with(&books_dir));
        failures.retain(|(path, _), _| path.is_file() && path.starts_with(&books_dir));
        return;
    }

    // Process oldest-to-newest by REPORT TIMESTAMP, not mtime: several
    // imports overwrite current state (gourmand clears/replaces its table;
    // stats/binds upsert values), so an older report running last would
    // leave stale state while every file is still marked seen. mtime is
    // unreliable here — copying/restoring the Books directory rewrites
    // mtimes, and equal mtimes leave `read_dir` order unspecified. The
    // `SkillReport_YYMMDD_HHMMSS` filename suffix is the authoritative
    // game timestamp (the gourmand latest-lookup already relies on
    // filename order being chronological). Files without a parsable
    // suffix sort BEFORE all timestamped ones (false < true in Rust) —
    // that's the safe end: timestamped reports always overwrite whatever
    // untimestamped files persisted, never the reverse.
    to_process.sort_by(|(path_a, _), (path_b, _)| {
        let key = |path: &PathBuf| -> (bool, String) {
            let stem = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or_default();
            // Take the last two underscore-delimited groups; a real
            // timestamp is two digit-groups of 6 and 6 chars.
            let groups: Vec<&str> = stem.split('_').collect();
            if groups.len() >= 2 {
                let d = groups[groups.len() - 2];
                let t = groups[groups.len() - 1];
                if d.len() == 6
                    && t.len() == 6
                    && d.chars().all(|c| c.is_ascii_digit())
                    && t.chars().all(|c| c.is_ascii_digit())
                {
                    return (true, format!("{}{}", d, t));
                }
            }
            (false, stem.to_string())
        };
        let (ts_a, key_a) = key(path_a);
        let (ts_b, key_b) = key(path_b);
        // Timestamped files sort by their (older-first) key after the
        // untimestamped group.
        ts_a.cmp(&ts_b).then_with(|| key_a.cmp(&key_b))
    });

    // Shared stall-ops lock instance (managed in lib.rs before the watcher
    // spawns) so shop-log writes serialize with live ingest and Clear — same
    // synchronous State borrow the coordinator's ingest_shop_log uses.
    let Some(ops_lock) = app.try_state::<StallOpsLock>() else {
        eprintln!("[books-watcher] StallOpsLock not managed yet; skipping tick");
        return;
    };

    for (path, mtime) in to_process {
        let file_name = path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or_default()
            .to_string();
        // Mid-write guard: the game writes files incrementally. If mtime
        // changed since the pre-read metadata captured above, the content
        // read here may be truncated — skip this tick without
        // marking seen or counting a failure, and retry next scan. Note the
        // pre-read `mtime` is exactly what's stored in `seen` on success, so
        // a file that stays stable through processing is recorded with its
        // actual mtime.
        if let Ok(current_mtime) = std::fs::metadata(&path).and_then(|m| m.modified()) {
            if current_mtime != mtime {
                // Mid-write: the content read here may be truncated —
                // skip this tick without marking seen or counting a
                // failure, and retry next scan.
                continue;
            }
        } else {
            // Stat failed after the read succeeded (file deleted
            // mid-tick) — skip, don't count a failure.
            continue;
        }

        // Read bytes and decode lossily (log_watchers.rs precedent): book
        // files should be UTF-8, but a bad byte must not turn this file into
        // a permanent retry loop.
        match std::fs::read(&path) {
            // Deleted between the mid-write guard and the read: same class
            // as the guard's own Err arm — skip, don't count a failure (a
            // vanished path can never be retried into success, so counting
            // it would only leak a failures entry).
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => continue,
            Err(err) => {
                record_failure(
                    failures,
                    seen,
                    &path,
                    mtime,
                    &file_name,
                    &format!("unreadable: {err}"),
                );
            }
            Ok(bytes) => {
                let content = String::from_utf8_lossy(&bytes);
                match process_book_file(&path, &content, &character, &server, db, &ops_lock, app) {
                    Ok(note) => {
                        // Success — including a stats-report attribution
                        // skip for another alt: the file belongs to that
                        // character and should never be ingested here, so
                        // marking it seen (and re-skipping forever) is
                        // correct. When the owning alt is active, the game
                        // rewrites the file (new mtime) on report open and
                        // it's ingested under the right character then.
                        failures.remove(&(path.clone(), mtime));
                        seen.insert(path.clone(), mtime);
                        eprintln!("[books-watcher] {file_name}: {note}");
                    }
                    Err(err) => {
                        record_failure(failures, seen, &path, mtime, &file_name, &err);
                    }
                }
            }
        }
    }

    // Prune removed files so the tracking map never grows unboundedly
    // (the doc comment promises this; wrongly pruning a transiently
    // unreadable file is harmless — ingestion is idempotent and the
    // mtime tracking reprocesses it).
    seen.retain(|path, _| path.is_file() && path.starts_with(&books_dir));
    failures.retain(|(path, _), _| path.is_file() && path.starts_with(&books_dir));
}

/// Count one failed attempt for a file — unless the error is transient
/// (`TRANSIENT_ERROR_MARKERS`): those are logged at every retry but never
/// advance the give-up counter, so momentary DB/CDN contention can't
/// permanently drop a healthy file. After `GIVE_UP_AFTER_ATTEMPTS`
/// consecutive non-transient failures, log a single giving-up line and mark
/// the file seen at its current mtime so it stops retrying every tick.
fn record_failure(
    failures: &mut HashMap<(PathBuf, SystemTime), u32>,
    seen: &mut HashMap<PathBuf, SystemTime>,
    path: &Path,
    mtime: SystemTime,
    file_name: &str,
    err: &str,
) {
    if TRANSIENT_ERROR_MARKERS.iter().any(|m| err.contains(m)) {
        eprintln!("[books-watcher] {file_name}: {err} (transient; will retry)");
        return;
    }
    let count = failures.entry((path.to_path_buf(), mtime)).or_default();
    *count += 1;
    if *count >= GIVE_UP_AFTER_ATTEMPTS {
        eprintln!(
            "[books-watcher] giving up on {file_name} after {count} consecutive failures: {err}"
        );
        seen.insert(path.to_path_buf(), mtime);
        failures.remove(&(path.to_path_buf(), mtime));
    } else {
        eprintln!("[books-watcher] {file_name}: {err} (attempt {count}; will retry)");
    }
}

/// `Some(s)` when the string is non-empty after trim, else `None`.
fn opt_nonempty(s: &str) -> Option<String> {
    let trimmed = s.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

// ============================================================
// File classification + dispatch
// ============================================================

/// What ingestion a classified book file gets, mirroring the live
/// coordinator's `BookOpened` dispatch (coordinator.rs). `BookOnly` files are
/// captured into `game_state_books` for queryability even where no live
/// consumer exists.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BookKind {
    /// Shop log: stall-event ingest ONLY (no book row) — the live path
    /// persists PlayerShopLog to `stall_events`, never `game_state_books`.
    ShopLog,
    /// SkillReport with content-driven extra ingest (gourmand / hoplology /
    /// binds) on top of the always-on book upsert.
    SkillReport,
    /// PlayerAge / HelpScreen: book upsert + structured stats ingest.
    Stats,
    /// Book upsert only (GuildMotd, MessageOfTheDay, NewQuest, unknown,
    /// ServerStatus, GardeningAlmanac — the watcher captures content but does
    /// not duplicate the live almanac extraction).
    BookOnly,
}

/// Classify a Books-dir file by its stem. Returns the `book_type` used for
/// the `game_state_books` upsert plus the ingest kind.
///
/// Filename shapes seen on disk: `SkillReport_261001_212859`,
/// `PlayerShopLog_260826_194723`, `NewQuestquest_45455_Name_260623_174509`
/// (the quest book's stem prefix is `NewQuestquest`, normalized to
/// `NewQuestBook` — a file-local book_type; these streams don't appear in any
/// live `matches!` list).
pub(crate) fn classify_book_file(stem: &str) -> (String, BookKind) {
    let prefix = stem.split('_').next().unwrap_or_default();
    match prefix {
        "PlayerShopLog" => ("PlayerShopLog".to_string(), BookKind::ShopLog),
        "SkillReport" => ("SkillReport".to_string(), BookKind::SkillReport),
        "PlayerAge" => ("PlayerAge".to_string(), BookKind::Stats),
        "HelpScreen" => ("HelpScreen".to_string(), BookKind::Stats),
        "NewQuestquest" => ("NewQuestBook".to_string(), BookKind::BookOnly),
        other => (other.to_string(), BookKind::BookOnly),
    }
}

/// Ingest one book file. `content` may be raw file text (real newlines) —
/// normalization to the shared live-path shape happens here, once.
pub(crate) fn process_book_file(
    path: &Path,
    content: &str,
    character: &str,
    server: &str,
    db: &DbPool,
    ops_lock: &StallOpsLock,
    app: &AppHandle,
) -> Result<String, String> {
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .ok_or_else(|| "filename has no stem".to_string())?
        .to_string();
    let (book_type, kind) = classify_book_file(&stem);
    let title = stem;

    // Normalize escaped-newline content (live-path shape) into real newlines.
    // File content already has real newlines; this is a no-op there.
    let content = crate::coordinator::normalize_book_content(content);

    match kind {
        BookKind::ShopLog => ingest_shop_log_file(&title, &content, character, db, ops_lock, app),
        BookKind::SkillReport => {
            // Attribution limitation: SkillReport content (binds, hoplology,
            // gourmand) carries no character name, so first-scan backfills
            // attribute to the currently-active character. With multiple
            // alts in the same Books dir that can misattribute backfilled
            // state; the live per-session path remains authoritative for
            // SkillReport-derived state.
            let mut notes: Vec<String> = Vec::new();

            // Live path persists ALL SkillReports to game_state_books first,
            // then runs content-specific branches. Mirror that.
            persist_book_content(db, app, character, server, &book_type, &title, &content)?;
            notes.push("book upserted".to_string());

            let trimmed = content.trim_start();
            if trimmed.starts_with("Foods Consumed:") {
                let n = import_gourmand(db, &content, app)?;
                if n > 0 {
                    notes.push(format!("gourmand +{n} foods"));
                }
            }
            if trimmed.starts_with("Equipment Studied:") {
                let game_data = app
                    .try_state::<GameDataState>()
                    .map(|s| s.inner().clone())
                    .ok_or_else(|| "GameDataState not managed yet".to_string())?;
                let inserted =
                    ingest_hoplology_content(db, &game_data, app, character, server, &content)
                        .map_err(|e| format!("hoplology ingest failed: {e}"))?;
                if inserted > 0 {
                    notes.push(format!("hoplology +{inserted}"));
                }
            }
            if content.contains("Primary Bind Location:") {
                ingest_teleport_binds_content(db, app, character, server, &content)
                    .map_err(|e| format!("teleportation binds failed: {e}"))?;
                notes.push("binds updated".to_string());
            }
            Ok(notes.join(", "))
        }
        BookKind::Stats => {
            // First-scan backfill attribution (finding #4): <game_data>/Books/
            // accumulates PlayerAge/HelpScreen exports across ALL alts, and
            // only PlayerAge/HelpScreen content self-identifies its owner via
            // the leading "<Name> was created on ..." line. Skip files that
            // belong to another character instead of misattributing their
            // stats permanently. Returning Ok lets scan_books_dir mark the
            // file seen; skipping forever is correct (the file belongs to
            // someone else), and when the owning alt is active the game
            // rewrites the file (new mtime) on report open, so it's ingested
            // under the right character then. SkillReport files carry no
            // owner in their content and cannot be filtered this way.
            if let Some(note) = stats_attribution_skip(&content, character) {
                return Ok(note);
            }
            persist_book_content(db, app, character, server, &book_type, &title, &content)?;
            let n = ingest_report_stats_content(db, app, character, server, &book_type, &content)
                .map_err(|e| format!("stats import failed: {e}"))?;
            if n > 0 {
                Ok(format!("book + stats ingested (+{n})"))
            } else {
                Ok("book + stats ingested".to_string())
            }
        }
        BookKind::BookOnly => {
            persist_book_content(db, app, character, server, &book_type, &title, &content)?;
            Ok("book upserted".to_string())
        }
    }
}

/// The character name self-identified by PlayerAge / HelpScreen content, from
/// the leading "<Name> was created on ..." line. `None` when the line is
/// missing or has no name before that phrase.
fn stats_owner(content: &str) -> Option<String> {
    let first_line = content.lines().next()?;
    // A UTF-8 BOM prefix would glue itself to the name ('\u{feff}Zaxxas'),
    // making every comparison miss and the file permanently skipped.
    let first_line = first_line.strip_prefix('\u{feff}').unwrap_or(first_line);
    let pos = first_line.find("was created on")?;
    let name = first_line[..pos].trim();
    if name.is_empty() {
        None
    } else {
        Some(name.to_string())
    }
}

/// `Some(skip note)` when PlayerAge / HelpScreen content self-identifies a
/// different character than the one the watcher would attribute it to (see
/// the `BookKind::Stats` branch for why skipping-and-marking-seen is
/// correct). `None` keeps ingestion on the normal path.
fn stats_attribution_skip(content: &str, character: &str) -> Option<String> {
    let owner = stats_owner(content)?;
    // eq_ignore_ascii_case only folds ASCII; game character names can
    // contain non-ASCII letters whose case differs between the game's
    // export and our settings string. Full Unicode case folding keeps the
    // comparison an exact-name check rather than a byte check.
    if owner.to_lowercase() == character.to_lowercase() {
        return None;
    }
    Some(format!("skipped: belongs to {owner}; not {character}"))
}

/// Ingest a PlayerShopLog file into `stall_events` (mirrors the live
/// `ingest_shop_log` + Import's owner resolution). No `game_state_books` row —
/// the live path never persists shop logs there.
fn ingest_shop_log_file(
    title: &str,
    content: &str,
    character: &str,
    db: &DbPool,
    ops_lock: &StallOpsLock,
    app: &AppHandle,
) -> Result<String, String> {
    // Probe parse (base year 1970) only to learn whether the file has any
    // parseable entries at all; the real parse uses the resolved base year.
    let probe = parse_shop_log(title, content, "imported", 1970);
    if probe.entries.is_empty() {
        return Err("no parseable shop-log entries".to_string());
    }

    // Game-written books are named PlayerShopLog_YYMMDD_HHMMSS.txt — no
    // 4-digit year, so year_from_filename always falls back to the current
    // year. That is wrong for the watcher's flagship scenario (glogger closed
    // across the New Year): a December book first scanned in January would
    // resolve its entries a year in the future. Use the live path's resolver
    // instead: wrap to the previous year when the oldest entry is in the
    // future relative to now.
    let base_year = crate::stall_year_resolver::base_year_for_live(
        probe
            .entries
            .first()
            .map(|e| e.timestamp.as_str())
            .unwrap_or_default(),
    );
    let shop_log = parse_shop_log(title, content, "imported", base_year);

    // Owner resolution (mirrors import_shop_log_file): the parsed advisory
    // owner wins; a bought-only file is claimed for the active character.
    let effective_owner = shop_log
        .owner
        .clone()
        .or_else(|| Some(character.to_string()));

    let inputs: Vec<StallEventInput> = shop_log
        .entries
        .iter()
        .map(|e| StallEventInput {
            event_timestamp: e.timestamp.clone(),
            event_at: e.event_at.clone(),
            log_timestamp: shop_log.log_timestamp.clone(),
            log_title: shop_log.title.clone(),
            action: e.action.clone(),
            player: e.player.clone(),
            owner: effective_owner.clone(),
            item: e.item.clone(),
            quantity: e.quantity,
            price_unit: e.price_unit,
            price_total: e.price_total,
            raw_message: e.raw_message.clone(),
            entry_index: e.entry_index,
        })
        .collect();

    let inserted = insert_stall_events(db, ops_lock, &inputs)?;
    if inserted > 0 {
        app.emit("stall-events-updated", inserted).ok();
    }
    Ok(format!("stall events +{inserted}"))
}

/// Gourmand import with the live path's emit contract
/// (`gourmand-updated` with the imported count when n > 0).
fn import_gourmand(db: &DbPool, content: &str, app: &AppHandle) -> Result<usize, String> {
    let conn = db
        .get_write()
        .map_err(|e| format!("Database connection error: {e}"))?;
    let n = crate::db::gourmand_commands::import_gourmand_from_content(&conn, content)?;
    if n > 0 {
        app.emit("gourmand-updated", n).ok();
    }
    Ok(n)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::migrations::run_migrations;
    use chrono::Datelike;
    use rusqlite::Connection;

    /// Migrated in-memory DB wrapped in the app's pool type.
    fn pool() -> DbPool {
        // Shared-cache memory URI: both pools must hit the SAME in-memory DB
        // (migrations run through `writes`; reads come through `reads`). A
        // unique name per call keeps parallel tests from sharing one DB.
        use std::sync::atomic::{AtomicU64, Ordering};
        static SEQ: AtomicU64 = AtomicU64::new(0);
        let n = SEQ.fetch_add(1, Ordering::Relaxed);
        // Mirrors db::init_pool: query_only on the READ pool only, so a test
        // writing through pool.get() fails fast instead of silently racing
        // the writer.
        let mk = |query_only: bool| {
            r2d2_sqlite::SqliteConnectionManager::file(format!(
                "file:books_watcher_test_{n}?mode=memory&cache=shared"
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
            .expect("pool");
        let reads = r2d2::Pool::builder()
            .max_size(2)
            .build(mk(true))
            .expect("pool");
        let conn = writes.get().expect("conn");
        run_migrations(&conn, None).expect("migrations");
        drop(conn);
        DbPool::from_pools(reads, writes)
    }

    // ── classify_book_file ────────────────────────────────────────

    #[test]
    fn classify_seven_on_disk_prefixes() {
        use BookKind::*;
        let cases = [
            ("SkillReport_261001_212859", ("SkillReport", SkillReport)),
            ("PlayerShopLog_260826_194723", ("PlayerShopLog", ShopLog)),
            ("PlayerAge_261001_212847", ("PlayerAge", Stats)),
            ("HelpScreen_261001_212849", ("HelpScreen", Stats)),
            (
                "NewQuestquest_45455_Name_260623_174509",
                ("NewQuestBook", BookOnly),
            ),
            ("GuildMotd_260712_173954", ("GuildMotd", BookOnly)),
            (
                "MessageOfTheDay_260623_174529",
                ("MessageOfTheDay", BookOnly),
            ),
        ];
        for (stem, (book_type, kind)) in cases {
            let (bt, k) = classify_book_file(stem);
            assert_eq!(bt, book_type, "book_type for {stem}");
            assert_eq!(k, kind, "kind for {stem}");
        }
    }

    #[test]
    fn classify_unknown_prefix_is_book_only() {
        let (bt, kind) = classify_book_file("WhateverElse_261001_101010");
        assert_eq!(bt, "WhateverElse");
        assert_eq!(kind, BookKind::BookOnly);
    }

    #[test]
    fn classify_defensive_prefixes() {
        let (bt, kind) = classify_book_file("ServerStatus_261001_101010");
        assert_eq!(bt, "ServerStatus");
        assert_eq!(kind, BookKind::BookOnly);
    }

    // ── persist_book_content ──────────────────────────────────────

    #[test]
    fn persist_book_is_idempotent_upsert() {
        let pool = pool();
        // AppHandle isn't constructible in unit tests; persist_book_content
        // emits through it. Split: verify the SQL contract by calling the
        // same statement shape the helper runs. The emit path is exercised
        // by the live-path regression proof.
        let conn = pool.get_write().unwrap();
        for _ in 0..2 {
            let dt = chrono::Utc::now().to_rfc3339();
            conn.execute(
                "INSERT INTO game_state_books (character_name, server_name, book_type, title, content, captured_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                 ON CONFLICT(character_name, server_name, book_type, title) DO UPDATE SET
                    content = excluded.content,
                    captured_at = excluded.captured_at",
                rusqlite::params!["TestChar", "TestServer", "GuildMotd", "GuildMotd_260712_173954", "body", dt],
            )
            .unwrap();
        }
        let (count, captured): (i64, String) = conn
            .query_row(
                "SELECT COUNT(*), MAX(captured_at) FROM game_state_books
                 WHERE character_name = 'TestChar' AND book_type = 'GuildMotd'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(count, 1, "idempotent upsert keeps one row");
        assert!(!captured.is_empty());
    }

    // ── teleportation binds ───────────────────────────────────────

    #[test]
    fn binds_real_newline_content_parses_fields() {
        let content =
            "Teleportation Status:\n\nPrimary Bind Location: Serbule\nSecondary Bind Location: (none)\n";
        let primary = crate::coordinator::extract_bind_field(content, "Primary Bind Location:");
        let secondary = crate::coordinator::extract_bind_field(content, "Secondary Bind Location:");
        assert_eq!(primary.as_deref(), Some("Serbule"));
        assert_eq!(secondary, None, "\"(none)\" maps to NULL");
    }

    #[test]
    fn binds_escaped_and_real_newlines_agree() {
        let escaped = "Primary Bind Location: Red Wing Casino\\nSecondary Bind Location: Caves Beneath Gazluk\\n";
        // Production path: normalize_book_content turns escaped newlines real
        // before extraction; the real-newline variant skips through unchanged.
        let normalized = crate::coordinator::normalize_book_content(escaped);
        for content in [normalized.as_str(), escaped.replace("\\n", "\n").as_str()] {
            let primary = crate::coordinator::extract_bind_field(content, "Primary Bind Location:");
            let secondary =
                crate::coordinator::extract_bind_field(content, "Secondary Bind Location:");
            assert_eq!(primary.as_deref(), Some("Red Wing Casino"));
            assert_eq!(secondary.as_deref(), Some("Caves Beneath Gazluk"));
        }
    }

    #[test]
    fn binds_created_on_shape_yields_null_primary() {
        // Real on-disk HelpScreen first lines — no bind fields at all.
        let content =
            "TwinkleofToes was created on Sat Jan 31 09:47:43 EST 2026\n\nPrimary Bind Location: (none)";
        assert_eq!(
            crate::coordinator::extract_bind_field(content, "Primary Bind Location:"),
            None
        );
    }

    // ── report stats ──────────────────────────────────────────────

    #[test]
    fn player_age_content_produces_stats() {
        let conn = Connection::open_in_memory().unwrap();
        run_migrations(&conn, None).unwrap();
        let content = "TwinkleofToes was created on <b>Sat Jan 31 09:47:43 EST 2026</b>.\nYou have spent <b>61 days 10 hours</b> logged into the game.\nYou have died <b>1,841</b> times.\n";
        let stats = crate::report_stats::parse_player_age(content);
        assert!(stats.len() >= 3, "expected created_on/time_played/deaths");
        let n = crate::report_stats::persist_stats(&conn, "TestChar", "TestServer", &stats, "now")
            .unwrap();
        assert_eq!(n, stats.len());
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM character_report_stats WHERE character_name = 'TestChar'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(count > 0);
    }

    #[test]
    fn stats_attribution_skips_other_characters_stats() {
        // PlayerAge content owned by another alt: the helper must return the
        // skip note, and process_book_file must return Ok WITHOUT
        // persisting anything under the active character. AppHandle isn't
        // constructible here, so verify the skip decision directly; the
        // Stats branch returns Ok(note) before any persist call.
        let content = "AnotherAlt was created on <b>Sat Jan 31 09:47:43 EST 2026</b>.\nYou have died <b>1,841</b> times.\n";
        assert_eq!(
            stats_attribution_skip(content, "TestChar"),
            Some("skipped: belongs to AnotherAlt; not TestChar".to_string())
        );
        // Same owner (case-insensitive) → no skip; missing created-on line →
        // no skip (nothing to attribute against).
        assert_eq!(stats_attribution_skip(content, "anotheralt"), None);
        assert_eq!(
            stats_attribution_skip("TwinkleofToes was created on ...", "TwinkleofToes"),
            None
        );
        assert_eq!(
            stats_attribution_skip("no attribution line here", "TestChar"),
            None
        );
        // Case-insensitivity on the comparison only: the note preserves the
        // content's own casing.
        assert_eq!(
            stats_attribution_skip(content, "testchar").as_deref(),
            Some("skipped: belongs to AnotherAlt; not testchar")
        );
        // Non-ASCII case differences fold too (eq_ignore_ascii_case missed
        // these; the game's export casing can differ from the settings
        // string's).
        assert_eq!(
            stats_attribution_skip("Éclair was created on ...", "éclair"),
            None
        );
        // A UTF-8 BOM before the name must not glue itself to it (would
        // make every same-character file look foreign and get skipped
        // forever).
        assert_eq!(
            stats_attribution_skip(
                "\u{feff}AnotherAlt was created on <b>Sat Jan 31 09:47:43 EST 2026</b>.\n",
                "AnotherAlt"
            ),
            None
        );
    }

    // ── record_failure / retry semantics ─────────────────────────

    fn failure_map() -> HashMap<(PathBuf, SystemTime), u32> {
        HashMap::new()
    }

    fn seen_map() -> HashMap<PathBuf, SystemTime> {
        HashMap::new()
    }

    fn mtime() -> SystemTime {
        SystemTime::UNIX_EPOCH
    }

    #[test]
    fn transient_errors_never_advance_give_up() {
        let mut failures = failure_map();
        let mut seen = seen_map();
        let path = Path::new("C:/books/SkillReport_261001_212859.txt");
        let m = mtime();
        for i in 0..10 {
            record_failure(
                &mut failures,
                &mut seen,
                path,
                m,
                "file.txt",
                "Database error: pool timed out",
            );
            assert!(seen.is_empty(), "transient failure {i} must not mark seen");
        }
        assert!(
            failures.is_empty(),
            "transient failures must not enter the map"
        );
        // Hoplology CDN gate marker too.
        record_failure(
            &mut failures,
            &mut seen,
            path,
            m,
            "file.txt",
            "Game data not loaded yet; retrying later",
        );
        assert!(seen.is_empty() && failures.is_empty());
    }

    #[test]
    fn deterministic_failures_give_up_and_mark_seen() {
        let mut failures = failure_map();
        let mut seen = seen_map();
        let path = Path::new("C:/books/SkillReport_261001_212859.txt");
        let m = mtime();
        for count in 1..GIVE_UP_AFTER_ATTEMPTS {
            record_failure(
                &mut failures,
                &mut seen,
                path,
                m,
                "file.txt",
                "no parseable shop-log entries",
            );
            assert!(seen.is_empty(), "not yet at threshold");
            assert_eq!(failures.get(&(path.to_path_buf(), m)), Some(&count));
        }
        record_failure(
            &mut failures,
            &mut seen,
            path,
            m,
            "file.txt",
            "no parseable shop-log entries",
        );
        assert_eq!(seen.get(path), Some(&m), "threshold reached → marked seen");
        assert!(
            !failures.contains_key(&(path.to_path_buf(), m)),
            "counter cleared"
        );
    }

    #[test]
    fn rewritten_file_mtime_resets_counter() {
        // A rewrite after failed attempts gets a new mtime → fresh count.
        let mut failures = failure_map();
        let mut seen = seen_map();
        let path = Path::new("C:/books/SkillReport_261001_212859.txt");
        let m1 = SystemTime::UNIX_EPOCH;
        let m2 = SystemTime::UNIX_EPOCH + Duration::from_secs(1);
        record_failure(
            &mut failures,
            &mut seen,
            path,
            m1,
            "file.txt",
            "no parseable shop-log entries",
        );
        record_failure(
            &mut failures,
            &mut seen,
            path,
            m2,
            "file.txt",
            "no parseable shop-log entries",
        );
        record_failure(
            &mut failures,
            &mut seen,
            path,
            m2,
            "file.txt",
            "no parseable shop-log entries",
        );
        assert!(
            seen.is_empty(),
            "2 failures at m2 + 1 at m1 never reaches 3"
        );
    }

    // ── gourmand ──────────────────────────────────────────────────

    #[test]
    fn gourmand_real_file_fixture_idempotent() {
        let pool = pool();
        let conn = pool.get_write().unwrap();
        let content =
            "Foods Consumed:\n\n  \"Meaty\" Tomato Soup: 7\n  8-Year Steamed Cake (HAS DAIRY): 1\n";
        let n1 =
            crate::db::gourmand_commands::import_gourmand_from_content(&conn, content).unwrap();
        assert_eq!(n1, 2);
        let n2 =
            crate::db::gourmand_commands::import_gourmand_from_content(&conn, content).unwrap();
        assert_eq!(n2, 2, "re-import stays stable");
        let soup: i64 = conn
            .query_row(
                "SELECT times_eaten FROM gourmand_eaten_foods WHERE food_name = '\"Meaty\" Tomato Soup'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(soup, 7, "count preserved across re-import");
        let total: i64 = conn
            .query_row("SELECT COUNT(*) FROM gourmand_eaten_foods", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(total, 2, "idempotent import keeps one row per food");
    }

    // ── shop log ──────────────────────────────────────────────────

    #[test]
    fn shop_log_real_line_parses_one_entry() {
        let content = "Mon Aug 24 23:52 - GorgonoidNeurodivergente bought Mastery Spark: Staves at a cost of 10000 per 1 = 10000\n";
        let parsed = parse_shop_log("Imported", content, "imported", 2026);
        assert_eq!(parsed.entries.len(), 1);
        let e = &parsed.entries[0];
        assert_eq!(e.action, "bought");
        assert_eq!(e.item.as_deref(), Some("Mastery Spark: Staves"));
        assert_eq!(e.price_total, Some(10000));
    }

    #[test]
    fn year_from_filename_on_disk_name() {
        // No 4-digit year in range → falls back to current local year.
        let path = Path::new("PlayerShopLog_260826_194723.txt");
        let expected = chrono::Local::now().year();
        assert_eq!(
            crate::db::stall_tracker_commands::year_from_filename(path),
            expected
        );
    }

    // ── hoplology splitter (post-normalization) ───────────────────

    #[test]
    fn hoplology_content_split_skips_header() {
        // Post-normalization shape: real newlines, one item per line.
        let content =
            "Equipment Studied:\n\n  CrudBurst's Hammer of Thumping\n  Thentree Harness\n";
        let items: Vec<&str> = content
            .split('\n')
            .map(str::trim)
            .filter(|l| !l.is_empty() && !l.starts_with("Equipment Studied:"))
            .collect();
        assert_eq!(items.len(), 2);
        assert_eq!(items[0], "CrudBurst's Hammer of Thumping");
    }
}
