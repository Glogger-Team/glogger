use super::word_of_power_catalog;
use super::DbPool;
use crate::chat_parser::{parse_chat_line, parse_chat_log_filename};
use crate::chat_status_parser::{parse_status_message, ChatStatusEvent};
use crate::settings::SettingsManager;
use serde::{Deserialize, Serialize};
use std::io::BufRead;
use std::sync::Arc;
use tauri::State;

// ── Output types ────────────────────────────────────────────────────────────

#[derive(Serialize)]
pub struct WordOfPower {
    pub id: i64,
    pub character_name: String,
    pub server_name: String,
    pub word: String,
    pub power_name: String,
    pub description: Option<String>,
    pub discovered_at: String,
    pub source: String,
    /// Looked up from the static catalog (`word_of_power_catalog`); "Unknown"
    /// for names not yet cataloged (e.g. manual entries with a custom name).
    pub category: String,
    /// Looked up from the static catalog; `None` if `power_name` is uncataloged.
    pub level: Option<u32>,
}

impl WordOfPower {
    /// Row fields for a new entry; the catalog lookup fills `category`/`level`.
    fn with_catalog_lookup(row: WordOfPowerRow) -> Self {
        let (category, level) = word_of_power_catalog::lookup(&row.power_name);
        Self {
            id: row.id,
            character_name: row.character_name,
            server_name: row.server_name,
            word: row.word,
            power_name: row.power_name,
            description: row.description,
            discovered_at: row.discovered_at,
            source: row.source,
            category: category.to_string(),
            level,
        }
    }
}

/// Owned row fields for [`WordOfPower::with_catalog_lookup`]; groups the
/// constructor args so it stays under the arity lint.
struct WordOfPowerRow {
    id: i64,
    character_name: String,
    server_name: String,
    word: String,
    power_name: String,
    description: Option<String>,
    discovered_at: String,
    source: String,
}

// ── Input types ─────────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct AddWordInput {
    pub word: String,
    pub power_name: String,
    pub description: Option<String>,
}

// ── Commands ────────────────────────────────────────────────────────────────

#[tauri::command]
pub fn get_words_of_power(
    db: State<'_, DbPool>,
    character_name: String,
    server_name: String,
) -> Result<Vec<WordOfPower>, String> {
    let conn = db.get().map_err(|e| e.to_string())?;
    let mut stmt = conn
        .prepare(
            "SELECT id, character_name, server_name, word, power_name, description, discovered_at, source
             FROM words_of_power
             WHERE character_name = ?1 AND server_name = ?2
             ORDER BY discovered_at DESC",
        )
        .map_err(|e| e.to_string())?;

    let rows = stmt
        .query_map([&character_name, &server_name], |row| {
            Ok(WordOfPower::with_catalog_lookup(WordOfPowerRow {
                id: row.get(0)?,
                character_name: row.get(1)?,
                server_name: row.get(2)?,
                word: row.get(3)?,
                power_name: row.get(4)?,
                description: row.get(5)?,
                discovered_at: row.get(6)?,
                source: row.get(7)?,
            }))
        })
        .map_err(|e| e.to_string())?;

    let mut words = Vec::new();
    for row in rows {
        words.push(row.map_err(|e| e.to_string())?);
    }
    Ok(words)
}

#[tauri::command]
pub fn add_word_of_power(
    db: State<'_, DbPool>,
    character_name: String,
    server_name: String,
    input: AddWordInput,
) -> Result<WordOfPower, String> {
    let conn = db.get_write().map_err(|e| e.to_string())?;
    let now = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();

    conn.execute(
        "INSERT INTO words_of_power (character_name, server_name, word, power_name, description, discovered_at, source)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'manual')",
        rusqlite::params![
            &character_name,
            &server_name,
            &input.word,
            &input.power_name,
            &input.description,
            &now,
        ],
    )
    .map_err(|e| e.to_string())?;

    let id = conn.last_insert_rowid();
    Ok(WordOfPower::with_catalog_lookup(WordOfPowerRow {
        id,
        character_name,
        server_name,
        word: input.word,
        power_name: input.power_name,
        description: input.description,
        discovered_at: now,
        source: "manual".to_string(),
    }))
}

#[tauri::command]
pub fn delete_word_of_power(db: State<'_, DbPool>, id: i64) -> Result<(), String> {
    let conn = db.get_write().map_err(|e| e.to_string())?;
    conn.execute("DELETE FROM words_of_power WHERE id = ?1", [id])
        .map_err(|e| e.to_string())?;
    Ok(())
}

// ── CSV import ──────────────────────────────────────────────────────────────

#[tauri::command]
pub fn import_words_of_power_csv(
    db: State<'_, DbPool>,
    character_name: String,
    server_name: String,
    file_path: String,
) -> Result<usize, String> {
    let content =
        std::fs::read_to_string(&file_path).map_err(|e| format!("Failed to read CSV file: {e}"))?;

    let mut reader = csv::Reader::from_reader(content.as_bytes());
    let headers = reader
        .headers()
        .map_err(|e| format!("Failed to read CSV headers: {e}"))?
        .clone();

    let col_word = find_header(&headers, &["Word", "word"])?;
    let col_power = find_header(&headers, &["Power Name", "power_name", "Power Name"])?;

    let col_date = find_header_opt(&headers, &["Date", "date"]);
    let col_time = find_header_opt(&headers, &["Time", "time"]);
    let col_desc = find_header_opt(&headers, &["Description", "description"]);

    let mut conn = db.get_write().map_err(|e| e.to_string())?;
    let conn = conn
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .map_err(|e| format!("Failed to start transaction: {e}"))?;
    let now = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();

    let mut imported = 0usize;

    for result in reader.records() {
        let record = result.map_err(|e| format!("Failed to read CSV record: {e}"))?;

        let word = record
            .get(col_word)
            .ok_or_else(|| "Missing Word field in CSV row".to_string())?
            .trim();
        let power_name = record
            .get(col_power)
            .ok_or_else(|| "Missing Power Name field in CSV row".to_string())?
            .trim();

        if word.is_empty() || power_name.is_empty() {
            continue;
        }

        let discovered_at = if let (Some(di), Some(ti)) = (col_date, col_time) {
            let date = record.get(di).unwrap_or("");
            let time = record.get(ti).unwrap_or("");
            let combined = format!("{} {}", date, time).trim().to_string();
            if combined.is_empty() || combined == " " {
                now.clone()
            } else {
                // Try parsing common date/time formats into ISO 8601
                parse_csv_datetime(&combined).unwrap_or_else(|| now.clone())
            }
        } else {
            now.clone()
        };

        let description = col_desc
            .and_then(|i| record.get(i))
            .filter(|s: &&str| !s.is_empty())
            .map(|s| s.to_string());

        conn.execute(
            "INSERT INTO words_of_power (character_name, server_name, word, power_name, description, discovered_at, source)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'csv-import')",
            rusqlite::params![
                &character_name,
                &server_name,
                word,
                power_name,
                description,
                &discovered_at,
            ],
        )
        .map_err(|e| format!("Failed to insert word '{}': {e}", word))?;

        imported += 1;
    }

    conn.commit()
        .map_err(|e| format!("Failed to commit transaction: {e}"))?;
    Ok(imported)
}

fn find_header(headers: &csv::StringRecord, candidates: &[&str]) -> Result<usize, String> {
    for c in candidates {
        if let Some(i) = headers
            .iter()
            .position(|h| h.trim().eq_ignore_ascii_case(c))
        {
            return Ok(i);
        }
    }
    Err(format!(
        "CSV is missing a required column. Expected one of: {}",
        candidates.join(", ")
    ))
}

fn find_header_opt(headers: &csv::StringRecord, candidates: &[&str]) -> Option<usize> {
    for c in candidates {
        if let Some(i) = headers
            .iter()
            .position(|h| h.trim().eq_ignore_ascii_case(c))
        {
            return Some(i);
        }
    }
    None
}

/// Best-effort parsing of common date+time formats into ISO 8601.
/// Accepts "MM/DD/YYYY HH:MM:SS AM/PM", "YYYY-MM-DD HH:MM:SS", etc.
fn parse_csv_datetime(s: &str) -> Option<String> {
    let s = s.trim();

    // Try common formats in order
    let formats = &[
        "%m/%d/%Y %I:%M:%S %p",
        "%m/%d/%Y %I:%M %p",
        "%m/%d/%Y %H:%M:%S",
        "%m/%d/%Y %H:%M",
        "%Y-%m-%d %H:%M:%S",
        "%Y-%m-%d %H:%M",
    ];

    for fmt in formats {
        if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(s, fmt) {
            return Some(dt.format("%Y-%m-%dT%H:%M:%SZ").to_string());
        }
    }

    // If it's already ISO-like, return as-is
    if s.contains('T') && s.ends_with('Z') {
        return Some(s.to_string());
    }

    None
}

// ── Used-word removal ───────────────────────────────────────────────────────

/// Delete every saved copy of a spoken word. Words of power are consumed
/// game-wide when spoken, so copies saved by any character are dead once
/// used. Matching is case-insensitive and whitespace-tolerant to cover
/// manual entries. Returns the number of rows deleted.
pub fn delete_words_by_word(
    conn: &rusqlite::Connection,
    word: &str,
) -> Result<usize, rusqlite::Error> {
    conn.execute(
        "DELETE FROM words_of_power WHERE TRIM(word) = TRIM(?1) COLLATE NOCASE",
        [word],
    )
}

/// Scan historical chat logs for `[Status] You use the word of power WORD!`
/// lines and delete any saved words that were spoken while glogger wasn't
/// tailing (live uses are handled by the coordinator's chat-status path).
/// A use can't precede its word's discovery, so only files dated on or after
/// the earliest saved discovery (minus one day of timezone slack — chat
/// filenames are local dates, `discovered_at` is UTC) are read, keeping the
/// startup scan cheap. Returns rows deleted.
pub fn backfill_used_words_from_chat_logs(
    settings: &SettingsManager,
    db: &DbPool,
) -> Result<usize, String> {
    // The initial saved-words query is read-only: use the read pool so the
    // app's single writer stays free until a delete actually happens.
    let saved: Vec<String> = {
        let conn = db
            .get()
            .map_err(|e| format!("Database connection error: {e}"))?;
        let mut stmt = conn
            .prepare("SELECT discovered_at FROM words_of_power")
            .map_err(|e| format!("Query prepare error: {e}"))?;
        let rows = stmt
            .query_map([], |row| row.get(0))
            .map_err(|e| format!("Query error: {e}"))?;
        rows.collect::<Result<_, _>>()
            .map_err(|e| format!("Row error: {e}"))?
    };
    if saved.is_empty() {
        return Ok(0);
    }

    let Some(dir) = settings.get_chat_logs_dir() else {
        return Ok(0);
    };
    if !dir.is_dir() {
        return Ok(0);
    }

    let min_date = saved
        .iter()
        .filter_map(|d| chrono::NaiveDate::parse_from_str(d.get(..10)?, "%Y-%m-%d").ok())
        .min()
        .map(|d| d - chrono::Duration::days(1));

    // BEGIN IMMEDIATE (not DEFERRED) so busy_timeout applies to acquisition;
    // one transaction per file keeps each write-lock hold short — a
    // whole-scan transaction starves the concurrent startup backfills'
    // busy_timeout (5s). Deletes are idempotent, so per-file is safe.
    // The WRITER CONNECTION is scoped per file as well: checkout only when a
    // file is about to be processed, drop it right after the commit, so
    // live-ingest writers can interleave between files.
    let mut deleted = 0usize;
    for entry in std::fs::read_dir(&dir)
        .map_err(|e| format!("Failed to read ChatLogs dir: {e}"))?
        .flatten()
    {
        let path = entry.path();
        let Some(file_date) = path
            .file_name()
            .and_then(|n| n.to_str())
            .and_then(parse_chat_log_filename)
        else {
            continue;
        };
        if matches!(min_date, Some(min) if file_date < min) {
            continue;
        }

        let Ok(file) = std::fs::File::open(&path) else {
            continue;
        };
        // PARSE FIRST, WRITER SECOND: the writer must not sit held while the
        // file is read and parsed — collect the used words, then acquire the
        // writer only around the delete transaction. Deletes are idempotent,
        // so collecting first then deleting loses nothing.
        let used: Vec<String> = {
            let reader = std::io::BufReader::new(file);
            let mut out = Vec::new();
            for line in reader.lines().map_while(Result::ok) {
                let Some(msg) = parse_chat_line(&line) else {
                    continue;
                };
                if let Some(ChatStatusEvent::WordOfPowerUsed { word, .. }) =
                    parse_status_message(&msg)
                {
                    out.push(word);
                }
            }
            out
        };
        if used.is_empty() {
            continue;
        }

        let mut conn = db
            .get_write()
            .map_err(|e| format!("Database connection error: {e}"))?;
        let tx = conn
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(|e| format!("Failed to begin transaction: {e}"))?;
        for word in &used {
            deleted += delete_words_by_word(&tx, word).map_err(|e| format!("Delete error: {e}"))?;
        }
        tx.commit().map_err(|e| format!("Commit error: {e}"))?;
        drop(conn); // release the writer before the next file
    }

    Ok(deleted)
}

/// Tauri command wrapper around [`backfill_used_words_from_chat_logs`].
#[tauri::command]
pub fn backfill_words_of_power_usage(
    settings: State<'_, Arc<SettingsManager>>,
    db: State<'_, DbPool>,
) -> Result<usize, String> {
    backfill_used_words_from_chat_logs(&settings, &db)
}

// ── Internal helper (called from coordinator, not a Tauri command) ──────────

pub fn insert_word_of_power(
    conn: &rusqlite::Connection,
    character_name: &str,
    server_name: &str,
    word: &str,
    power_name: &str,
    description: Option<&str>,
    discovered_at: &str,
) -> Result<i64, rusqlite::Error> {
    conn.execute(
        "INSERT INTO words_of_power (character_name, server_name, word, power_name, description, discovered_at, source)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'auto')",
        rusqlite::params![character_name, server_name, word, power_name, description, discovered_at],
    )?;
    Ok(conn.last_insert_rowid())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    fn setup() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE words_of_power (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                character_name TEXT NOT NULL,
                server_name TEXT NOT NULL,
                word TEXT NOT NULL,
                power_name TEXT NOT NULL,
                description TEXT,
                discovered_at TEXT NOT NULL,
                source TEXT NOT NULL DEFAULT 'auto',
                created_at TEXT NOT NULL DEFAULT (datetime('now'))
            );",
        )
        .unwrap();
        conn
    }

    fn count(conn: &Connection) -> i64 {
        conn.query_row("SELECT COUNT(*) FROM words_of_power", [], |r| r.get(0))
            .unwrap()
    }

    #[test]
    fn test_delete_words_by_word_removes_all_characters_copies() {
        let conn = setup();
        // Same word saved by two characters, plus an unrelated word.
        insert_word_of_power(
            &conn,
            "Alt",
            "Dreva",
            "ABCWORD",
            "Haste",
            None,
            "2026-07-01T00:00:00Z",
        )
        .unwrap();
        insert_word_of_power(
            &conn,
            "Main",
            "Dreva",
            "ABCWORD",
            "Haste",
            None,
            "2026-07-01T00:00:00Z",
        )
        .unwrap();
        insert_word_of_power(
            &conn,
            "Main",
            "Dreva",
            "OTHERWORD",
            "Luck",
            None,
            "2026-07-02T00:00:00Z",
        )
        .unwrap();

        let n = delete_words_by_word(&conn, "ABCWORD").unwrap();
        assert_eq!(n, 2);
        assert_eq!(count(&conn), 1);
    }

    #[test]
    fn test_delete_words_by_word_case_and_whitespace_insensitive() {
        let conn = setup();
        // Manual entries may be lowercased or carry stray whitespace.
        insert_word_of_power(
            &conn,
            "Main",
            "Dreva",
            " abcword ",
            "Haste",
            None,
            "2026-07-01T00:00:00Z",
        )
        .unwrap();

        let n = delete_words_by_word(&conn, "ABCWORD").unwrap();
        assert_eq!(n, 1);
        assert_eq!(count(&conn), 0);
    }

    #[test]
    fn test_delete_words_by_word_unknown_word_is_noop() {
        let conn = setup();
        insert_word_of_power(
            &conn,
            "Main",
            "Dreva",
            "ABCWORD",
            "Haste",
            None,
            "2026-07-01T00:00:00Z",
        )
        .unwrap();

        let n = delete_words_by_word(&conn, "NEVERSPOKEN").unwrap();
        assert_eq!(n, 0);
        assert_eq!(count(&conn), 1);
    }
}
