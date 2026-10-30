//! Combat Wisdom award tracking.
//!
//! Combat Wisdom is a PG currency awarded (via Chat.log `[Status]` lines) for
//! killing notable monsters. We persist every *monster* award so the dashboard
//! widget can show per-monster reuse cooldowns that survive restarts and are
//! seeded from historical chat logs.
//!
//! The log never states a monster's class (boss/elite/named), so the cooldown
//! is derived **empirically**: the shortest real gap (≥ 60s, to skip duplicate
//! emits) observed between a monster's awards. The frontend falls back to a
//! verb-based wiki default until a gap has been observed.
//!
//! Prodigy-level awards ("Earned a Prodigy Level", no monster) are intentionally
//! NOT persisted — they have no cooldown and a NULL `source_name` can't dedup
//! under the unique index. They still count in the live session on the frontend.

use chrono::{Local, NaiveDateTime, TimeZone};
use serde::Serialize;
use std::fs::{self, File};
use std::io::{BufRead, BufReader};
use std::sync::Arc;
use tauri::State;

use super::{DbPool, DbRead};
use crate::chat_parser::parse_chat_line;
use crate::chat_status_parser::{parse_status_message, ChatStatusEvent};
use crate::settings::SettingsManager;

/// Per-monster aggregate for the Combat Wisdom widget's cooldown list.
#[derive(Debug, Serialize, Clone)]
pub struct CombatWisdomMonster {
    pub name: String,
    pub verb: String,
    /// Epoch milliseconds of the most recent award (local time interpreted).
    pub last_earned_ms: i64,
    pub count: u32,
    pub total_amount: i64,
    /// Smallest gap (seconds, ≥ 60) ever observed between this monster's awards.
    /// `None` until at least two awards more than a minute apart exist.
    pub min_gap_secs: Option<i64>,
}

/// Parse a "YYYY-MM-DD HH:MM:SS" local timestamp into epoch milliseconds.
fn local_ts_to_epoch_ms(ts: &str) -> Option<i64> {
    let naive = NaiveDateTime::parse_from_str(ts, "%Y-%m-%d %H:%M:%S").ok()?;
    match Local.from_local_datetime(&naive) {
        chrono::LocalResult::Single(dt) => Some(dt.timestamp_millis()),
        chrono::LocalResult::Ambiguous(dt, _) => Some(dt.timestamp_millis()),
        chrono::LocalResult::None => None,
    }
}

/// Persist a single Combat Wisdom *monster* award. No-op for non-monster awards
/// (prodigy levels). Idempotent via the `idx_cw_dedup` unique index.
pub fn record_combat_wisdom_earn(
    conn: &rusqlite::Connection,
    earned_at: &str,
    amount: u32,
    source_name: Option<&str>,
    verb: &str,
    zone: Option<&str>,
) -> Result<(), String> {
    let Some(name) = source_name else {
        return Ok(());
    };
    conn.execute(
        "INSERT OR IGNORE INTO combat_wisdom_earns (earned_at, amount, source_name, verb, zone)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        rusqlite::params![earned_at, amount, name, verb, zone],
    )
    .map(|_| ())
    .map_err(|e| format!("Failed to record combat wisdom earn: {e}"))
}

/// Aggregate persisted monster awards into per-monster cooldown rows.
#[tauri::command]
pub fn get_combat_wisdom_monsters(
    db: State<'_, DbPool>,
) -> Result<Vec<CombatWisdomMonster>, String> {
    let conn = db
        .get()
        .map_err(|e| format!("Database connection error: {e}"))?;
    aggregate_monsters(&conn)
}

/// Core of [`get_combat_wisdom_monsters`], factored out for testing.
pub fn aggregate_monsters<C: DbRead + ?Sized>(
    conn: &C,
) -> Result<Vec<CombatWisdomMonster>, String> {
    // Pull all monster rows ordered by monster then time; fold per-monster.
    let mut stmt = conn
        .prepare(
            "SELECT source_name, verb, earned_at, amount
             FROM combat_wisdom_earns
             WHERE source_name IS NOT NULL
             ORDER BY source_name ASC, earned_at ASC",
        )
        .map_err(|e| format!("Query prepare error: {e}"))?;

    let rows = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, i64>(3)?,
            ))
        })
        .map_err(|e| format!("Query error: {e}"))?;

    let mut out: Vec<CombatWisdomMonster> = Vec::new();
    // Per-monster running state.
    let mut cur_name: Option<String> = None;
    let mut cur_verb = String::new();
    let mut count: u32 = 0;
    let mut total: i64 = 0;
    let mut last_ms: i64 = 0;
    let mut prev_ms: Option<i64> = None;
    let mut min_gap: Option<i64> = None;

    let flush = |out: &mut Vec<CombatWisdomMonster>,
                 name: &Option<String>,
                 verb: &str,
                 count: u32,
                 total: i64,
                 last_ms: i64,
                 min_gap: Option<i64>| {
        if let Some(name) = name {
            out.push(CombatWisdomMonster {
                name: name.clone(),
                verb: verb.to_string(),
                last_earned_ms: last_ms,
                count,
                total_amount: total,
                min_gap_secs: min_gap,
            });
        }
    };

    for row in rows {
        let (name, verb, earned_at, amount) = row.map_err(|e| format!("Row error: {e}"))?;
        // Skip malformed timestamps: counting them as epoch 0 would corrupt
        // last_earned_ms ordering and min-gap computation.
        let Some(ms) = local_ts_to_epoch_ms(&earned_at) else {
            continue;
        };

        if cur_name.as_deref() != Some(name.as_str()) {
            // Boundary: emit the previous monster, then reset.
            flush(
                &mut out, &cur_name, &cur_verb, count, total, last_ms, min_gap,
            );
            cur_name = Some(name);
            cur_verb = verb;
            count = 0;
            total = 0;
            prev_ms = None;
            min_gap = None;
        } else {
            // Most-recent verb wins (keeps the latest classification).
            cur_verb = verb;
        }

        count += 1;
        total += amount;
        last_ms = ms;
        if let Some(p) = prev_ms {
            let gap = (ms - p) / 1000;
            if gap >= 60 {
                min_gap = Some(min_gap.map_or(gap, |m| m.min(gap)));
            }
        }
        prev_ms = Some(ms);
    }
    flush(
        &mut out, &cur_name, &cur_verb, count, total, last_ms, min_gap,
    );

    // Most-recently-earned first.
    out.sort_by_key(|a| std::cmp::Reverse(a.last_earned_ms));
    Ok(out)
}

/// Read every `Chat-*.log` in the ChatLogs directory and persist any Combat
/// Wisdom monster awards found. Idempotent (unique index). Returns rows inserted.
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
        // One parsed wisdom earn: (timestamp, amount, source, verb, zone).
        type WisdomEarn = (String, u32, Option<String>, String, Option<String>);
        // One IMMEDIATE transaction per file: four startup backfills run
        // concurrently, and a single transaction spanning every file holds
        // the write lock long enough for the others' busy_timeout (5s) to
        // expire. Per-file keeps each lock hold short; the inserts are
        // idempotent (unique index) so a crash mid-scan just re-runs.
        // PARSE FIRST, WRITER SECOND: the writer must not sit held while the
        // file is read and parsed — collect the records, then acquire the
        let earns: Vec<WisdomEarn> = {
            let reader = BufReader::new(file);
            let mut out = Vec::new();
            for line in reader.lines().map_while(Result::ok) {
                let Some(msg) = parse_chat_line(&line) else {
                    continue;
                };
                if let Some(ChatStatusEvent::CombatWisdomEarned {
                    timestamp,
                    amount,
                    source_name,
                    verb,
                    zone,
                }) = parse_status_message(&msg)
                {
                    if source_name.is_none() {
                        continue;
                    }
                    out.push((timestamp, amount, source_name, verb, zone));
                }
            }
            out
        };
        if earns.is_empty() {
            continue;
        }

        let mut conn = db
            .get_write()
            .map_err(|e| format!("Database connection error: {e}"))?;
        let tx = conn
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(|e| format!("Failed to begin transaction: {e}"))?;
        for (timestamp, amount, source_name, verb, zone) in &earns {
            let n = tx
                .execute(
                    "INSERT OR IGNORE INTO combat_wisdom_earns
                     (earned_at, amount, source_name, verb, zone)
                     VALUES (?1, ?2, ?3, ?4, ?5)",
                    rusqlite::params![timestamp, amount, source_name, verb, zone],
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
pub fn backfill_combat_wisdom_from_chat_logs(
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
            "CREATE TABLE combat_wisdom_earns (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                earned_at TEXT NOT NULL,
                amount INTEGER NOT NULL,
                source_name TEXT,
                verb TEXT NOT NULL,
                zone TEXT
            );
            CREATE UNIQUE INDEX idx_cw_dedup
                ON combat_wisdom_earns(earned_at, source_name, amount);",
        )
        .unwrap();
        conn
    }

    #[test]
    fn test_record_skips_non_monster_awards() {
        let conn = setup();
        record_combat_wisdom_earn(&conn, "2026-04-08 08:35:01", 1000, None, "Earned", None)
            .unwrap();
        let n: i64 = conn
            .query_row("SELECT COUNT(*) FROM combat_wisdom_earns", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 0, "prodigy-level (no monster) awards must not be stored");
    }

    #[test]
    fn test_record_is_idempotent() {
        let conn = setup();
        for _ in 0..3 {
            record_combat_wisdom_earn(
                &conn,
                "2026-04-08 10:53:42",
                64,
                Some("the Aktaari Queen"),
                "Killed",
                None,
            )
            .unwrap();
        }
        let n: i64 = conn
            .query_row("SELECT COUNT(*) FROM combat_wisdom_earns", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 1, "same award re-recorded must dedup via unique index");
    }

    #[test]
    fn test_min_gap_excludes_sub_minute_duplicates() {
        let conn = setup();
        // Two "Royal Guard" earns 9 seconds apart (duplicate emit) then one ~6h
        // later → the real cooldown is the multi-hour gap, not 9 seconds.
        for (ts, amt) in [
            ("2026-04-08 10:59:42", 95),
            ("2026-04-08 10:59:51", 95),
            ("2026-04-08 16:59:42", 95),
        ] {
            record_combat_wisdom_earn(
                &conn,
                ts,
                amt,
                Some("Aktaari Royal Guard"),
                "Defeated",
                None,
            )
            .unwrap();
        }
        let monsters = aggregate_monsters(&conn).unwrap();
        let guard = monsters
            .iter()
            .find(|m| m.name == "Aktaari Royal Guard")
            .unwrap();
        assert_eq!(guard.count, 3);
        // Sub-60s gap (9s, 10:59:42→10:59:51) is excluded; the remaining
        // consecutive gap (10:59:51→16:59:42 = 21591s) is the min observed.
        assert_eq!(guard.min_gap_secs, Some(21591));
    }

    #[test]
    fn test_min_gap_none_when_only_one_award() {
        let conn = setup();
        record_combat_wisdom_earn(
            &conn,
            "2026-04-08 05:37:49",
            73,
            Some("Elite Tactician"),
            "Defeated",
            None,
        )
        .unwrap();
        let monsters = aggregate_monsters(&conn).unwrap();
        assert_eq!(monsters.len(), 1);
        assert_eq!(monsters[0].min_gap_secs, None);
        assert_eq!(monsters[0].total_amount, 73);
    }
}
