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

#[derive(Serialize, specta::Type)]
pub struct WordOfPower {
    /// SQLite rowid. Exported as `number`: JSON has no int widths, and
    /// rowids fit `u32` for any realistic database. The override keeps the
    /// `specta-typescript` BigInt guard (which forbids bare `i64`) satisfied
    /// with `specta` alone, so the TS exporter stays a dev-only dependency.
    #[specta(type = u32)]
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

#[derive(Deserialize, specta::Type)]
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

/// Current UTC time in the `words_of_power.discovered_at` contract format:
/// space-separated naive UTC (`%Y-%m-%d %H:%M:%S`), matching the v98 backfill
/// of legacy columns. RFC3339 (`T` separator / `Z` suffix) must NOT be
/// written: `T` (0x54) sorts after ` ` (0x20), so an RFC3339 row misorders
/// in `ORDER BY discovered_at` against space-format rows.
fn utc_now_stamp() -> String {
    chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string()
}

#[tauri::command]
pub fn add_word_of_power(
    db: State<'_, DbPool>,
    character_name: String,
    server_name: String,
    input: AddWordInput,
) -> Result<WordOfPower, String> {
    let conn = db.get_write().map_err(|e| e.to_string())?;
    let now = utc_now_stamp();

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
    let now = utc_now_stamp();

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

/// Best-effort parsing of common date+time formats into space-format naive UTC
/// (`YYYY-MM-DD HH:MM:SS`, the `discovered_at` contract — see [`utc_now_stamp`]).
/// Accepts "MM/DD/YYYY HH:MM:SS AM/PM", "YYYY-MM-DD HH:MM:SS", RFC3339
/// ("YYYY-MM-DDTHH:MM:SSZ", normalized), etc.
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
            return Some(dt.format("%Y-%m-%d %H:%M:%S").to_string());
        }
    }

    // An RFC3339-ish stamp is normalized to the space contract rather than
    // passed through: a stored `T` separator misorders ORDER BY (see
    // `utc_now_stamp`). Unparseable input falls through to None so the
    // caller stamps `now` instead of persisting garbage.
    if s.contains('T') {
        let normalized = s.replace('T', " ");
        let normalized = normalized.trim_end_matches('Z').trim();
        for fmt in &["%Y-%m-%d %H:%M:%S", "%Y-%m-%d %H:%M"] {
            if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(normalized, fmt) {
                return Some(dt.format("%Y-%m-%d %H:%M:%S").to_string());
            }
        }
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

// ── Specta pilot (TODO: shared schema contract) ─────────────────────────────
//
// Words-of-power pilot for `specta` (plain `specta`, no `tauri-specta`):
// `WordOfPower` (output) + `AddWordInput` (input) derive `specta::Type` so
// `src/types/wordsOfPower.specta.ts` can be generated from these shapes.
// Test-only plumbing: the exporters live in `[dev-dependencies]`, so no new
// runtime dependency ships beyond `specta` itself, and this fn compiles out
// of non-test builds. Regenerate with
// `OVERWRITE_BINDINGS=1 cargo test words_of_power_specta`.
#[cfg(test)]
pub fn export_bindings() -> Result<String, Box<dyn std::error::Error>> {
    let types = specta::Types::default()
        .register::<WordOfPower>()
        .register::<AddWordInput>();
    Ok(specta_typescript::Typescript::default().export(&types, specta_serde::Format)?)
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

    fn seed_word(
        conn: &Connection,
        character: &str,
        server: &str,
        word: &str,
        power: &str,
        discovered_at: &str,
    ) {
        conn.execute(
            "INSERT INTO words_of_power (character_name, server_name, word, power_name, description, discovered_at, source)
             VALUES (?1, ?2, ?3, ?4, NULL, ?5, 'auto')",
            rusqlite::params![character, server, word, power, discovered_at],
        )
        .unwrap();
    }

    fn count(conn: &Connection) -> i64 {
        conn.query_row("SELECT COUNT(*) FROM words_of_power", [], |r| r.get(0))
            .unwrap()
    }

    #[test]
    fn test_delete_words_by_word_removes_all_characters_copies() {
        let conn = setup();
        // Same word saved by two characters, plus an unrelated word.
        seed_word(&conn, "Alt", "Dreva", "ABCWORD", "Haste", "2026-07-01 00:00:00");
        seed_word(&conn, "Main", "Dreva", "ABCWORD", "Haste", "2026-07-01 00:00:00");
        seed_word(&conn, "Main", "Dreva", "OTHERWORD", "Luck", "2026-07-02 00:00:00");

        let n = delete_words_by_word(&conn, "ABCWORD").unwrap();
        assert_eq!(n, 2);
        assert_eq!(count(&conn), 1);
    }

    #[test]
    fn test_delete_words_by_word_case_and_whitespace_insensitive() {
        let conn = setup();
        // Manual entries may be lowercased or carry stray whitespace.
        seed_word(&conn, "Main", "Dreva", " abcword ", "Haste", "2026-07-01 00:00:00");

        let n = delete_words_by_word(&conn, "ABCWORD").unwrap();
        assert_eq!(n, 1);
        assert_eq!(count(&conn), 0);
    }

    #[test]
    fn test_delete_words_by_word_unknown_word_is_noop() {
        let conn = setup();
        seed_word(&conn, "Main", "Dreva", "ABCWORD", "Haste", "2026-07-01 00:00:00");

        let n = delete_words_by_word(&conn, "NEVERSPOKEN").unwrap();
        assert_eq!(n, 0);
        assert_eq!(count(&conn), 1);
    }

    /// Both writers must stamp space-format naive UTC (`YYYY-MM-DD HH:MM:SS`,
    /// no `T` separator): the v98 backfill normalized legacy columns to that
    /// shape, and an RFC3339 row misorders under `ORDER BY discovered_at`
    /// (`T` 0x54 sorts after ` ` 0x20).
    #[test]
    fn test_utc_now_stamp_is_space_format_naive_utc() {
        let stamp = utc_now_stamp();
        assert_eq!(stamp.len(), 19, "expected YYYY-MM-DD HH:MM:SS, got {stamp}");
        assert_eq!(&stamp[10..11], " ", "expected space separator, got {stamp}");
        assert!(!stamp.contains('T'), "T separator in new stamp {stamp}");
        assert!(!stamp.contains('Z'), "Z suffix in new stamp {stamp}");
        assert!(
            chrono::NaiveDateTime::parse_from_str(&stamp, "%Y-%m-%d %H:%M:%S").is_ok(),
            "new stamp does not parse as naive UTC: {stamp}"
        );
    }

    #[test]
    fn test_parse_csv_datetime_emits_space_format() {
        // Every accepted input shape — including RFC3339 — must come out in
        // the space-format contract, never passed through with a `T`.
        for input in [
            "07/01/2026 02:30:00 PM",
            "07/01/2026 02:30 PM",
            "07/01/2026 14:30:00",
            "07/01/2026 14:30",
            "2026-07-01 14:30:00",
            "2026-07-01 14:30",
            "2026-07-01T14:30:00Z",
        ] {
            let out =
                parse_csv_datetime(input).unwrap_or_else(|| panic!("unparsed input {input}"));
            assert_eq!(out, "2026-07-01 14:30:00", "wrong normalization of {input}");
        }
        assert_eq!(parse_csv_datetime("not a date"), None);
    }

    /// Written-row pin: rows stamped as the writers bind them (manual `now`
    /// plus a CSV cell in RFC3339, as `parse_csv_datetime` normalizes it)
    /// persist in space format and order chronologically under the
    /// `get_words_of_power` ORDER BY. Pre-fix this failed: the T-row
    /// (`2026-07-01T14:30:00Z`) sorted ahead of the later space row
    /// (`2026-07-01 15:00:00`) under DESC.
    #[test]
    fn test_written_rows_order_chronologically_in_space_format() {
        let conn = setup();
        // Legacy v98-shape row, later clock time.
        seed_word(&conn, "Main", "Dreva", "LATEWORD", "Haste", "2026-07-01 15:00:00");
        // CSV-shape row: an RFC3339 cell normalized exactly as the CSV
        // writer binds it (parse, falling back to `now` on garbage).
        let csv_cell = "2026-07-01T14:30:00Z";
        let csv_stamped =
            parse_csv_datetime(csv_cell).unwrap_or_else(utc_now_stamp);
        seed_word(&conn, "Main", "Dreva", "EARLYWORD", "Luck", &csv_stamped);
        // Manual-shape row: stamped exactly as `add_word_of_power` binds it.
        seed_word(&conn, "Main", "Dreva", "NOWWORD", "Haste", &utc_now_stamp());

        let rows: Vec<(String, String)> = {
            let mut stmt = conn
                .prepare(
                    "SELECT word, discovered_at FROM words_of_power
                     ORDER BY discovered_at DESC",
                )
                .unwrap();
            stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
                .unwrap()
                .collect::<Result<_, _>>()
                .unwrap()
        };
        assert_eq!(rows.len(), 3);
        for (_, discovered_at) in &rows {
            assert!(
                !discovered_at.contains('T'),
                "T separator persisted in row {discovered_at}"
            );
            assert!(
                chrono::NaiveDateTime::parse_from_str(discovered_at, "%Y-%m-%d %H:%M:%S")
                    .is_ok(),
                "row does not parse as space-format naive UTC: {discovered_at}"
            );
        }
        // Newest (now) first, then 15:00, then the normalized 14:30 CSV row.
        assert_eq!(rows[0].0, "NOWWORD");
        assert_eq!(rows[1].0, "LATEWORD");
        assert_eq!(rows[2], ("EARLYWORD".to_string(), "2026-07-01 14:30:00".to_string()));
    }

    /// Specta pilot pin: the checked-in `src/types/wordsOfPower.specta.ts`
    /// must equal what `export_bindings` derives from `WordOfPower` +
    /// `AddWordInput`. Any shape change (field add/remove/rename, serde
    /// attribute) fails here until the bindings are regenerated — that
    /// failure IS the drift signal. Regenerate with
    /// `OVERWRITE_BINDINGS=1 cargo test words_of_power_specta`, inspect the
    /// diff, and confirm it matches the hand-mirrored `WordOfPower`
    /// interface in `WordsOfPowerWidget.vue` before committing.
    #[test]
    fn test_words_of_power_specta_bindings_are_fresh() {
        let fresh = super::export_bindings().expect("specta export failed");
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../src/types/wordsOfPower.specta.ts"
        );
        if std::env::var("OVERWRITE_BINDINGS").is_ok() {
            std::fs::write(path, &fresh).expect("bindings write failed");
            return;
        }
        let checked_in =
            std::fs::read_to_string(path).expect("checked-in specta bindings missing");
        assert_eq!(
            checked_in, fresh,
            "specta bindings drift: regenerate with OVERWRITE_BINDINGS=1"
        );
    }
}
