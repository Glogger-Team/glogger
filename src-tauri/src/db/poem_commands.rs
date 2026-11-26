//! Recorded poems (Chat Logs → Poems tab).
//!
//! Poems recited by other players via `Poem by X` NPC talk screens are captured
//! globally — they are *not* character-scoped, so the same poem heard by any
//! character (or heard more than once) is stored a single time. The
//! UNIQUE(author, title, content) index drives that dedup via INSERT OR IGNORE,
//! so re-scanning logs never produces duplicate rows. The `recorded_by_*`
//! columns retain which character/server first captured the poem for reference
//! only.
//!
//! Sources, oldest first: archived rotated copies (`<app data>/player-logs`,
//! mtime-sorted), then the game's rotated `Player-prev.log`, then the live
//! `Player.log`. The
//! live tailer resumes from a saved byte offset, so sessions played while
//! glogger was closed live only in the rotated/archived copies — scanning the
//! live log alone misses them.
use super::DbPool;
use crate::parsers::parse_timestamp;
use serde::Serialize;
use std::path::PathBuf;
use tauri::{AppHandle, Emitter, State};

#[derive(Serialize, specta::Type)]
pub struct PoemRow {
    #[specta(type = u32)]
    pub id: i64,
    pub author: String,
    pub title: String,
    pub content: String,
    pub recorded_at: String,
}

/// All recorded poems, newest first.
///
/// Poems are global (not scoped to the active character), so no
/// character/server filter is applied — the Poems tab shows every poem ever
/// captured. Searching/filtering is handled on the frontend against this list.
#[tauri::command(rename_all = "camelCase")]
pub fn get_poems(db: State<'_, DbPool>) -> Result<Vec<PoemRow>, String> {
    let conn = db.get().map_err(|e| e.to_string())?;

    let mut stmt = conn
        .prepare(
            "SELECT id, author, title, content, recorded_at
             FROM poems
             ORDER BY recorded_at DESC, id DESC",
        )
        .map_err(|e| e.to_string())?;

    let rows = stmt
        .query_map([], |row| {
            Ok(PoemRow {
                id: row.get(0)?,
                author: row.get(1)?,
                title: row.get(2)?,
                content: row.get(3)?,
                recorded_at: row.get(4)?,
            })
        })
        .map_err(|e| e.to_string())?;

    let mut poems = Vec::new();
    for row in rows {
        poems.push(row.map_err(|e| e.to_string())?);
    }
    Ok(poems)
}

/// Scan one Player.log-shaped file for `Poem by X` recitals. Each file's
/// HH:MM:SS lines are anchored to that file's last-modified date (a Player.log
/// is normally a single session), falling back to today's date when the mtime
/// is unavailable. Pure CPU — holds no DB lock.
fn parse_poems_from_path(path: &PathBuf) -> Vec<(String, String, String, String)> {
    use crate::parsers::to_utc_datetime_with_base;

    let base_date = std::fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .map(|mtime| chrono::DateTime::<chrono::Utc>::from(mtime).date_naive());
    let Ok(text) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    let mut parsed = Vec::new();
    for line in text.lines() {
        // Cheap pre-filter before the fuller parse.
        if !line.contains("\"Poem by ") {
            continue;
        }
        if let Some(poem) = parse_poem_line(line) {
            // Combine the log's HH:MM:SS with the file-date anchor.
            let recorded_at = to_utc_datetime_with_base(&poem.timestamp, base_date);
            parsed.push((poem.author, poem.title, poem.content, recorded_at));
        }
    }
    parsed
}

/// Backfill the `poems` table from every Player.log source, oldest first:
/// archived rotated copies, then the game's rotated `Player-prev.log`, then
/// the live `Player.log`. Idempotent (`INSERT OR IGNORE` on the
/// `UNIQUE(author, title, content)` index). Returns rows newly inserted.
///
/// PARSE FIRST, WRITER SECOND: the whole scan is pure CPU and must not hold
/// the sole write connection (a long session's log can parse for tens of
/// seconds); one BEGIN IMMEDIATE transaction then covers all inserts so
/// `busy_timeout` applies to acquisition (mirrors the arena-bet and
/// statehelm-gift backfills).
pub fn backfill_from_player_logs(
    settings: &crate::settings::SettingsManager,
    db: &DbPool,
) -> Result<usize, String> {
    let mut paths: Vec<PathBuf> = Vec::new();

    // Archived rotated copies first, oldest first. Content-hashed names carry
    // no date, so mtime (≈ session end) is the only ordering available.
    if let Some(dir) = crate::replay::player_log_archive_dir(settings) {
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
                Some((mtime, path))
            })
            .collect();
        archived.sort_by_key(|(mtime, _)| *mtime);
        paths.extend(archived.into_iter().map(|(_, p)| p));
    }

    // The two live logs are always scanned regardless of mtime.
    if let Some(p) = settings.get_player_prev_log_path()
        && p.is_file() {
            paths.push(p);
        }
    if let Some(p) = settings.get_player_log_path()
        && p.is_file() {
            paths.push(p);
        }

    let parsed: Vec<(String, String, String, String)> =
        paths.iter().flat_map(parse_poems_from_path).collect();
    if parsed.is_empty() {
        return Ok(0);
    }
    // Backfill sources (rotated/archived logs) carry no character context of
    // their own, so newly captured poems are attributed to the active
    // character/server for reference. Stays NULL when no character has ever
    // logged in — the live path (`game_state.rs` `PoemRecorded`) always
    // stamps both.
    let snapshot = settings.get();
    let recorded_by_character: Option<String> = snapshot
        .active_character_name
        .filter(|s| !s.is_empty());
    let recorded_by_server: Option<String> = snapshot
        .active_server_name
        .filter(|s| !s.is_empty());

    let mut conn = db
        .get_write()
        .map_err(|e| format!("Database connection error: {e}"))?;
    let tx = conn
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .map_err(|e| format!("Failed to begin transaction: {e}"))?;
    let mut inserted = 0usize;
    {
        let mut stmt = tx
            .prepare(
                "INSERT OR IGNORE INTO poems (author, title, content, recorded_at, recorded_by_character, recorded_by_server)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            )
            .map_err(|e| e.to_string())?;
        for (author, title, content, recorded_at) in &parsed {
            inserted += stmt
                .execute(rusqlite::params![
                    author,
                    title,
                    content,
                    recorded_at,
                    recorded_by_character,
                    recorded_by_server
                ])
                .map_err(|e| e.to_string())?;
        }
    }
    tx.commit().map_err(|e| format!("Commit error: {e}"))?;
    Ok(inserted)
}

/// Backfill poems from every Player.log source and emit `game-state-updated
/// ["poems"]` when anything new lands. Runs on the Step 5d startup chain and
/// (via `scan_player_log_for_poems`) from the Poems tab button.
pub fn backfill_from_player_logs_with_emit(
    settings: &crate::settings::SettingsManager,
    db: &DbPool,
    app: &AppHandle,
) -> Result<usize, String> {
    let inserted = backfill_from_player_logs(settings, db)?;
    if inserted > 0 {
        app.emit("game-state-updated", vec!["poems"]).ok();
    }
    Ok(inserted)
}

/// A poem parsed from a `Poem by X` talk-screen line.
struct ParsedPoem {
    /// Raw `HH:MM:SS` timestamp from the log line.
    timestamp: String,
    author: String,
    title: String,
    content: String,
}

/// Parse a single `Poem by X` talk-screen log line into a [`ParsedPoem`],
/// or `None` if the line is not a poem recital. Used by the one-time
/// Player.log poem backfill below.
fn parse_poem_line(line: &str) -> Option<ParsedPoem> {
    if !line.contains("ProcessTalkScreen(") {
        return None;
    }
    let timestamp = parse_timestamp(line)?;

    let args_start = line.find("ProcessTalkScreen(")? + "ProcessTalkScreen(".len();
    let args = &line[args_start..];
    let first_comma = args.find(',')?;
    let rest = &args[first_comma + 1..];

    // Title is the first quoted string. Poem titles are "Poem by <name>" and
    // never contain escaped quotes, so the naive scan is safe here.
    let q_start = rest.find('"')? + 1;
    let q_end = rest[q_start..].find('"')? + q_start;
    let title = &rest[q_start..q_end];
    let author = title.strip_prefix("Poem by ")?.trim().to_string();

    // Body is the next quoted string; it may contain `\"`-escaped quotes.
    let after_title = &rest[q_end + 1..];
    let raw_body = extract_quoted_escaped(after_title, 0)?;
    let (poem_title, content) = parse_poem_body(&raw_body)?;

    Some(ParsedPoem {
        timestamp,
        author,
        title: poem_title,
        content,
    })
}

/// Parse the raw body of a `Poem by X` talk screen into `(title, content)`.
///
/// The body has the shape (all on one log line, with `\n` escapes):
/// ```text
/// <i>The following poem was spoken by NAME…</i>\n\n---\n\n<b>TITLE</b>\n\n<poem>\n\n\n---\n\n<i>What did you think…</i>
/// ```
/// The bold `<b>TITLE</b>` is pulled out as the poem's name, the intro/outro
/// review blurbs (`<i>…</i>`) and the `---` fences are dropped, and the
/// remaining stanzas are returned with real newlines. Returns `None` only when
/// both the title and content come out empty.
fn parse_poem_body(raw_body: &str) -> Option<(String, String)> {
    // The bold tag holds the poem's name.
    let title = raw_body
        .find("<b>")
        .and_then(|s| {
            let inner = &raw_body[s + 3..];
            inner.find("</b>").map(|e| inner[..e].trim().to_string())
        })
        .unwrap_or_default();

    // Drop the leading intro blurb (everything up to and including its `</i>`)
    // and the trailing outro blurb (from its opening `<i>` to the end). Player
    // text is HTML-entity escaped by the game, so raw `<i>`/`</i>` only ever
    // come from the game-generated blurbs.
    let mut core = raw_body;
    if let Some(p) = core.find("</i>") {
        core = &core[p + "</i>".len()..];
    }
    if let Some(p) = core.rfind("<i>") {
        core = &core[..p];
    }

    // Remove the bold title line — it's stored separately.
    let core = if title.is_empty() {
        core.to_string()
    } else {
        core.replacen(&format!("<b>{title}</b>"), "", 1)
    };

    // Resolve `\n` (and friends) to real characters, then drop the blank and
    // `---` fence lines that bookend the poem, keeping interior lines intact.
    let unescaped = unescape_log_string(&core);
    let is_boundary = |line: &str| line.trim().is_empty() || line.trim().chars().all(|c| c == '-');
    let lines: Vec<&str> = unescaped.lines().collect();
    let mut start = 0;
    let mut end = lines.len();
    while start < end && is_boundary(lines[start]) {
        start += 1;
    }
    while end > start && is_boundary(lines[end - 1]) {
        end -= 1;
    }
    let content = lines[start..end].join("\n");

    if title.is_empty() && content.is_empty() {
        return None;
    }
    Some((title, content))
}

/// Extract the nth double-quoted string from `text`, honoring backslash escapes
/// (`\"`, `\\`) so an escaped quote inside the string is not mistaken for its
/// terminator. Returns the raw inner content with escapes left intact (use
/// [`unescape_log_string`] to resolve them).
fn extract_quoted_escaped(text: &str, n: usize) -> Option<String> {
    let bytes = text.as_bytes();
    let mut count = 0;
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'"' {
            let start = i + 1;
            let mut j = start;
            while j < bytes.len() {
                match bytes[j] {
                    b'\\' => j += 2, // skip the escaped character
                    b'"' => break,
                    _ => j += 1,
                }
            }
            if j >= bytes.len() {
                return None; // unterminated string
            }
            if count == n {
                return Some(text[start..j].to_string());
            }
            count += 1;
            i = j + 1;
        } else {
            i += 1;
        }
    }
    None
}

/// Resolve the log's backslash escapes (`\n`, `\t`, `\r`, `\"`, `\\`) into their
/// real characters. Unknown escapes are passed through verbatim.
fn unescape_log_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('n') => out.push('\n'),
                Some('t') => out.push('\t'),
                Some('r') => out.push('\r'),
                Some('"') => out.push('"'),
                Some('\\') => out.push('\\'),
                Some(other) => {
                    out.push('\\');
                    out.push(other);
                }
                None => out.push('\\'),
            }
        } else {
            out.push(c);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::sync::atomic::{AtomicU64, Ordering};

    static SEQ: AtomicU64 = AtomicU64::new(0);

    fn unique_temp_path(prefix: &str, ext: &str) -> PathBuf {
        let n = SEQ.fetch_add(1, Ordering::Relaxed);
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let mut path = std::env::temp_dir();
        path.push(format!("{prefix}_{}_{nanos}_{n}.{ext}", std::process::id()));
        path
    }

    fn poem_line(ts: &str, author: &str, title: &str, body: &str) -> String {
        format!(
            "[{ts}] LocalPlayer: ProcessTalkScreen(-51, \"Poem by {author}\", \
             \"<i>The following poem was spoken by {author}. Review it to receive Poetry Appreciation XP!</i>\\n\\n---\\n\\n<b>{title}</b>\\n\\n{body}\\n\\n\\n---\\n\\n<i>What did you think of the poem?</i>\", \
             \"\", [1,2,3,4,5,], System.String[], 0, Generic)"
        )
    }

    fn settings_with_logs(dir: &std::path::Path) -> (crate::settings::SettingsManager, PathBuf) {
        // `get_player_log_path` prefers `log_file_path`; the archive dir is
        // derived from the settings file's parent, so point both at one temp dir.
        let app_dir = dir.join(format!("app_{}", SEQ.fetch_add(1, Ordering::Relaxed)));
        std::fs::create_dir_all(&app_dir).unwrap();
        let sm = crate::settings::SettingsManager::new(app_dir.clone()).expect("settings");
        let player_log = dir.join("Player.log");
        let mut s = sm.get();
        s.log_file_path = player_log.to_string_lossy().to_string();
        s.game_data_path = dir.to_string_lossy().to_string();
        sm.update(s).expect("update settings");
        (sm, app_dir)
    }

    fn count_poems(pool: &DbPool) -> i64 {
        pool.get()
            .unwrap()
            .query_row("SELECT COUNT(*) FROM poems", [], |r| r.get(0))
            .unwrap()
    }

    #[test]
    fn backfill_covers_prev_log_and_dedups() {
        let dir = std::env::temp_dir().join(format!(
            "glogger_poem_dir_{}_{}",
            std::process::id(),
            SEQ.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let (sm, _app_dir) = settings_with_logs(&dir);

        // Same poem in Player-prev.log and Player.log: one row.
        let shared = poem_line("05:07:17", "Snowkit", "swiped snacks", "A flash of red.");
        std::fs::write(dir.join("Player-prev.log"), format!("{shared}\n")).unwrap();
        std::fs::write(
            dir.join("Player.log"),
            format!(
                "{shared}\n{}\n",
                poem_line("06:00:00", "Banbu", "ode", "Ode body here.")
            ),
        )
        .unwrap();

        let pool = crate::db::init_pool(unique_temp_path("glogger_poem_test", "db"), Some(0))
            .expect("init pool");
        let inserted = backfill_from_player_logs(&sm, &pool).expect("backfill");
        assert_eq!(
            inserted, 2,
            "prev-log poem + live-log poem, shared poem deduped"
        );
        assert_eq!(count_poems(&pool), 2);

        // Re-run is a no-op (INSERT OR IGNORE): the startup chain and the Poems
        // tab button both call this, so double coverage must not double rows.
        assert_eq!(
            backfill_from_player_logs(&sm, &pool).expect("re-backfill"),
            0
        );
        assert_eq!(count_poems(&pool), 2);
    }

    #[test]
    fn backfill_covers_archived_copies() {
        let dir = std::env::temp_dir().join(format!(
            "glogger_poem_dir_{}_{}",
            std::process::id(),
            SEQ.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let (sm, app_dir) = settings_with_logs(&dir);

        // A session that only survives in the archive: Player-prev.log has
        // since rotated past it and the live log never saw it.
        let archive_dir = app_dir.join("player-logs");
        std::fs::create_dir_all(&archive_dir).unwrap();
        let archived = poem_line("05:07:17", "Snowkit", "archived gem", "Kept only here.");
        std::fs::write(
            archive_dir.join("Player-abc123.log"),
            format!("{archived}\n"),
        )
        .unwrap();
        std::fs::write(dir.join("Player-prev.log"), "nothing here\n").unwrap();
        std::fs::write(dir.join("Player.log"), "nothing here either\n").unwrap();

        let pool = crate::db::init_pool(unique_temp_path("glogger_poem_test", "db"), Some(0))
            .expect("init pool");
        assert_eq!(backfill_from_player_logs(&sm, &pool).expect("backfill"), 1);
        assert_eq!(count_poems(&pool), 1);
    }

    #[test]
    fn backfill_needs_no_logs() {
        let dir = std::env::temp_dir().join(format!(
            "glogger_poem_dir_{}_{}",
            std::process::id(),
            SEQ.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let (sm, _app_dir) = settings_with_logs(&dir);
        let pool = crate::db::init_pool(unique_temp_path("glogger_poem_test", "db"), Some(0))
            .expect("init pool");
        assert_eq!(backfill_from_player_logs(&sm, &pool).expect("backfill"), 0);
    }
}

// ── Specta schema export (TODO.md:113 shared schema contract) ───────────────
// `poem` IPC payload shapes derive `specta::Type` so
// `src/types/poem.specta.ts` can be generated from them. Test-only plumbing:
// the exporters live in `[dev-dependencies]`, so no new runtime dependency
// ships beyond `specta` itself, and this fn compiles out of non-test builds.
// Regenerate with `OVERWRITE_BINDINGS=1 cargo test poem_specta`.
#[cfg(test)]
pub fn export_bindings() -> Result<String, Box<dyn std::error::Error>> {
    let types = specta::Types::default()
        .register::<PoemRow>()
        ;
    Ok(specta_typescript::Typescript::default().export(&types, specta_serde::Format)?)
}

#[cfg(test)]
mod specta_bindings_tests {
    use super::export_bindings;

    /// Freshness pin: the checked-in `src/types/poem.specta.ts` must equal
    /// what `export_bindings` derives from the `poem` payload shapes. Any
    /// shape change (field add/remove/rename, serde attribute) fails here until the
    /// bindings are regenerated — that failure IS the drift signal. Regenerate with
    /// `OVERWRITE_BINDINGS=1 cargo test poem_specta`, inspect the diff, and
    /// confirm it matches the hand-mirrored frontend types before committing.
    #[test]
    fn test_poem_specta_bindings_are_fresh() {
        let fresh = export_bindings().expect("specta export failed");
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../src/types/poem.specta.ts",
        );
        if std::env::var("OVERWRITE_BINDINGS").is_ok() {
            std::fs::write(path, &fresh).expect("bindings write failed");
            return;
        }
        let checked_in =
            std::fs::read_to_string(path).expect("checked-in specta bindings missing");
        assert_eq!(
            checked_in, fresh,
            "specta bindings drift: regenerate with OVERWRITE_BINDINGS=1",
        );
    }
}