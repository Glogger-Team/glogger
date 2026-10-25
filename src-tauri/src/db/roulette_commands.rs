//! Casino roulette outcome tracking.
//!
//! Project: Gorgon writes exactly one roulette event to the logs:
//! `[Status] Roulette ball ended on N!` — the winning number of a spin. The
//! player's own bet (amount + target) is an on-screen toast that is never
//! written to any log file, so only *outcomes* are recoverable. We persist
//! every observed winning number so the dashboard widget can show an
//! outcome-frequency pie chart that survives restarts and is seeded from
//! historical chat logs.
//!
//! Numbers are from a European single-zero wheel (0..=36). The widget derives
//! red/black/green buckets from the number; we keep the raw per-number counts
//! here so the frontend can slice them however it likes.

use serde::Serialize;
use std::fs::{self, File};
use std::io::{BufRead, BufReader};
use std::sync::Arc;
use tauri::State;

use super::{DbPool, DbRead};
use crate::chat_parser::parse_chat_line;
use crate::chat_status_parser::{parse_status_message, ChatStatusEvent};
use crate::settings::SettingsManager;

/// Count of spins that landed on a given number.
#[derive(Debug, Serialize, Clone)]
pub struct RouletteNumberCount {
    pub number: u32,
    pub count: u32,
}

/// Aggregate roulette outcome history for the dashboard widget.
#[derive(Debug, Serialize, Clone, Default)]
pub struct RouletteStats {
    /// Total spins observed.
    pub total: u32,
    /// Per-number counts, ascending by number (only numbers that appeared).
    pub counts: Vec<RouletteNumberCount>,
    /// ISO-ish "YYYY-MM-DD HH:MM:SS" of the most recent spin, if any.
    pub last_spun_at: Option<String>,
    /// The most recent winning number, if any.
    pub last_number: Option<u32>,
    /// Up to the last 10 winning numbers, most recent first.
    pub recent: Vec<u32>,
}

/// Persist a single roulette spin outcome. Idempotent via `idx_roulette_dedup`.
pub fn record_roulette_result(
    conn: &rusqlite::Connection,
    spun_at: &str,
    number: u32,
) -> Result<(), String> {
    conn.execute(
        "INSERT OR IGNORE INTO roulette_results (spun_at, number) VALUES (?1, ?2)",
        rusqlite::params![spun_at, number],
    )
    .map(|_| ())
    .map_err(|e| format!("Failed to record roulette result: {e}"))
}

/// Aggregate all persisted spins into outcome stats.
pub fn aggregate_stats<C: DbRead + ?Sized>(conn: &C) -> Result<RouletteStats, String> {
    let mut stmt = conn
        .prepare(
            "SELECT number, COUNT(*) FROM roulette_results
             GROUP BY number ORDER BY number ASC",
        )
        .map_err(|e| format!("Query prepare error: {e}"))?;

    let rows = stmt
        .query_map([], |row| {
            Ok(RouletteNumberCount {
                number: row.get::<_, i64>(0)? as u32,
                count: row.get::<_, i64>(1)? as u32,
            })
        })
        .map_err(|e| format!("Query error: {e}"))?;

    let mut counts = Vec::new();
    let mut total = 0u32;
    for r in rows {
        let c = r.map_err(|e| format!("Row error: {e}"))?;
        total += c.count;
        counts.push(c);
    }

    // Most recent spin (separate query keeps the GROUP BY above clean).
    let (last_spun_at, last_number) = conn
        .query_row(
            "SELECT spun_at, number FROM roulette_results
             ORDER BY spun_at DESC, id DESC LIMIT 1",
            [],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)? as u32)),
        )
        .map(|(ts, n)| (Some(ts), Some(n)))
        .unwrap_or((None, None));

    // Last 10 winning numbers, most recent first.
    let mut recent_stmt = conn
        .prepare(
            "SELECT number FROM roulette_results
             ORDER BY spun_at DESC, id DESC LIMIT 10",
        )
        .map_err(|e| format!("Query prepare error: {e}"))?;
    let recent = recent_stmt
        .query_map([], |row| Ok(row.get::<_, i64>(0)? as u32))
        .map_err(|e| format!("Query error: {e}"))?
        .collect::<Result<Vec<u32>, _>>()
        .map_err(|e| format!("Row error: {e}"))?;

    Ok(RouletteStats {
        total,
        counts,
        last_spun_at,
        last_number,
        recent,
    })
}

/// Aggregate persisted roulette outcomes for the dashboard widget.
#[tauri::command]
pub fn get_roulette_stats(db: State<'_, DbPool>) -> Result<RouletteStats, String> {
    let conn = db
        .get()
        .map_err(|e| format!("Database connection error: {e}"))?;
    aggregate_stats(&conn)
}

/// Read every `Chat-*.log` in the ChatLogs directory and persist any roulette
/// spin outcomes found. Idempotent (unique index). Returns rows inserted.
pub fn backfill_from_chat_logs(settings: &SettingsManager, db: &DbPool) -> Result<usize, String> {
    let Some(dir) = settings.get_chat_logs_dir() else {
        return Ok(0);
    };
    if !dir.is_dir() {
        return Ok(0);
    }

    let entries = fs::read_dir(&dir).map_err(|e| format!("Failed to read ChatLogs dir: {e}"))?;

    let mut inserted = 0usize;

    for entry in entries.flatten() {
        let path = entry.path();
        let is_chat_log = path
            .file_name()
            .and_then(|n| n.to_str())
            .map(|n| n.starts_with("Chat-") && n.ends_with(".log"))
            .unwrap_or(false);
        if !is_chat_log {
            continue;
        }

        let Ok(file) = File::open(&path) else {
            continue;
        };
        // One IMMEDIATE transaction per file: startup backfills run
        // concurrently and a whole-scan transaction holds the write lock
        // long enough for the others' busy_timeout (5s) to expire.
        // Inserts are idempotent (unique index), so per-file is safe.
        // PARSE FIRST, WRITER SECOND: the writer must not sit held while the
        // file is read and parsed — collect the results, then acquire the
        // writer only around the insert transaction.
        let spins: Vec<(String, u32)> = {
            let reader = BufReader::new(file);
            let mut out = Vec::new();
            for line in reader.lines().map_while(Result::ok) {
                let Some(msg) = parse_chat_line(&line) else {
                    continue;
                };
                if let Some(ChatStatusEvent::RouletteResult { timestamp, number }) =
                    parse_status_message(&msg)
                {
                    out.push((timestamp, number));
                }
            }
            out
        };
        if spins.is_empty() {
            continue;
        }

        let mut conn = db
            .get_write()
            .map_err(|e| format!("Database connection error: {e}"))?;
        let tx = conn
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(|e| format!("Failed to begin transaction: {e}"))?;
        for (timestamp, number) in &spins {
            let n = tx
                .execute(
                    "INSERT OR IGNORE INTO roulette_results (spun_at, number)
                     VALUES (?1, ?2)",
                    rusqlite::params![timestamp, number],
                )
                .map_err(|e| format!("Insert error: {e}"))?;
            inserted += n;
        }
        tx.commit().map_err(|e| format!("Commit error: {e}"))?;
        drop(conn); // release the writer before the next file
    }

    Ok(inserted)
}

/// Tauri command wrapper around [`backfill_from_chat_logs`].
#[tauri::command]
pub fn backfill_roulette_from_chat_logs(
    settings: State<'_, Arc<SettingsManager>>,
    db: State<'_, DbPool>,
) -> Result<usize, String> {
    backfill_from_chat_logs(&settings, &db)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    fn setup() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE roulette_results (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                spun_at TEXT NOT NULL,
                number INTEGER NOT NULL
            );
            CREATE UNIQUE INDEX idx_roulette_dedup
                ON roulette_results(spun_at, number);",
        )
        .unwrap();
        conn
    }

    #[test]
    fn test_record_is_idempotent() {
        let conn = setup();
        for _ in 0..3 {
            record_roulette_result(&conn, "2026-06-25 21:58:40", 25).unwrap();
        }
        let n: i64 = conn
            .query_row("SELECT COUNT(*) FROM roulette_results", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 1, "same spin re-recorded must dedup via unique index");
    }

    #[test]
    fn test_aggregate_counts_and_total() {
        let conn = setup();
        for (ts, num) in [
            ("2026-06-25 21:51:03", 31),
            ("2026-06-25 21:52:09", 23),
            ("2026-06-25 21:55:25", 8),
            ("2026-06-25 21:56:29", 0),
            ("2026-06-25 21:58:40", 25),
            ("2026-06-25 22:00:00", 25),
        ] {
            record_roulette_result(&conn, ts, num).unwrap();
        }
        let stats = aggregate_stats(&conn).unwrap();
        assert_eq!(stats.total, 6);
        // 25 appeared twice.
        let c25 = stats.counts.iter().find(|c| c.number == 25).unwrap();
        assert_eq!(c25.count, 2);
        // Counts ascending by number; first is 0.
        assert_eq!(stats.counts.first().unwrap().number, 0);
        // Most recent spin.
        assert_eq!(stats.last_number, Some(25));
        assert_eq!(stats.last_spun_at.as_deref(), Some("2026-06-25 22:00:00"));
        // Recent list is most-recent-first.
        assert_eq!(stats.recent.first(), Some(&25));
        assert_eq!(stats.recent, vec![25, 25, 0, 8, 23, 31]);
    }

    #[test]
    fn test_aggregate_empty() {
        let conn = setup();
        let stats = aggregate_stats(&conn).unwrap();
        assert_eq!(stats.total, 0);
        assert!(stats.counts.is_empty());
        assert_eq!(stats.last_number, None);
    }
}
