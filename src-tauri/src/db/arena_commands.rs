//! Casino arena fight tracking.
//!
//! Persists resolved arena matches (who fought whom, and who won) parsed from
//! Kuzavek's `[NPC Chatter]` narration by [`crate::arena_parser`]. The dashboard
//! Arena widget reads the aggregate to rank fighters and predict the favorite in
//! any matchup — driven entirely from the chat log, with no manual tip entry.
//!
//! Only outcomes are stored; the player's own wager (a `Player.log` talk screen)
//! is not recorded here. Rows dedup on `(fought_at, fighter_a, fighter_b)` via a
//! unique index, so re-ingesting the same match from chat backfill is a no-op.

use serde::Serialize;
use std::fs::{self, File};
use std::io::{BufRead, BufReader};
use std::sync::Arc;
use tauri::State;

use super::{DbPool, DbRead};
use crate::arena_parser::{ArenaBetTracker, ArenaTracker};
use crate::chat_parser::parse_chat_line;
use crate::settings::SettingsManager;

/// Per-fighter overall record.
#[derive(Debug, Serialize, Clone)]
pub struct ArenaFighter {
    pub name: String,
    pub wins: u32,
    pub losses: u32,
    /// Win percentage 0..=100, one decimal of precision retained by the frontend.
    pub win_pct: f64,
}

/// One directed head-to-head cell: `fighter` vs `opponent`.
#[derive(Debug, Serialize, Clone)]
pub struct ArenaHeadToHead {
    pub fighter: String,
    pub opponent: String,
    pub wins: u32,
    pub losses: u32,
}

/// A recently-observed resolved match, newest first.
#[derive(Debug, Serialize, Clone)]
pub struct ArenaRecentMatch {
    pub fought_at: String,
    pub fighter_a: String,
    pub fighter_b: String,
    pub winner: String,
}

/// The player's personal betting summary (their own bets, from Player.log).
#[derive(Debug, Serialize, Clone, Default)]
pub struct ArenaBettingSummary {
    /// Resolved bets (win or loss).
    pub total: u32,
    /// Bets won.
    pub won: u32,
    /// Win percentage 0..=100.
    pub win_pct: f64,
    /// Total councils wagered across all resolved bets.
    pub total_wagered: i64,
    /// Total councils received from winning bets.
    pub total_won: i64,
    /// Net profit/loss (total_won - total_wagered).
    pub net_profit: i64,
}

/// Aggregate arena history for the dashboard widget.
#[derive(Debug, Serialize, Clone, Default)]
pub struct ArenaStats {
    pub total_matches: u32,
    /// Fighters sorted by win percentage descending (ties by wins).
    pub fighters: Vec<ArenaFighter>,
    /// Every directed matchup with at least one observed result.
    pub head_to_head: Vec<ArenaHeadToHead>,
    /// Up to the last 15 matches, newest first.
    pub recent: Vec<ArenaRecentMatch>,
    /// The player's own betting record (from Player.log NPC 5712 bets).
    pub betting: ArenaBettingSummary,
}

/// Persist a single resolved match. Idempotent via `idx_arena_dedup`.
pub fn record_arena_match(
    conn: &rusqlite::Connection,
    fought_at: &str,
    fighter_a: &str,
    fighter_b: &str,
    winner: &str,
) -> Result<usize, String> {
    conn.execute(
        "INSERT OR IGNORE INTO arena_matches (fought_at, fighter_a, fighter_b, winner)
         VALUES (?1, ?2, ?3, ?4)",
        rusqlite::params![fought_at, fighter_a, fighter_b, winner],
    )
    .map_err(|e| format!("Failed to record arena match: {e}"))
}

/// Persist a single resolved personal bet. Idempotent via `idx_arena_bets_dedup`.
pub fn record_arena_bet(
    conn: &rusqlite::Connection,
    placed_at: &str,
    pick: &str,
    opponent: &str,
    wager: i64,
    payout: i64,
    won: bool,
) -> Result<usize, String> {
    conn.execute(
        "INSERT OR IGNORE INTO arena_bets
            (placed_at, pick, opponent, wager, payout, won)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        rusqlite::params![placed_at, pick, opponent, wager, payout, won as i64],
    )
    .map_err(|e| format!("Failed to record arena bet: {e}"))
}

/// Aggregate the player's personal betting record.
fn aggregate_betting<C: DbRead + ?Sized>(conn: &C) -> Result<ArenaBettingSummary, String> {
    // won-bet payouts and all-bet wagers, in one pass.
    let (total, won, total_wagered, total_won) = conn
        .query_row(
            "SELECT
                COUNT(*),
                COALESCE(SUM(won), 0),
                COALESCE(SUM(wager), 0),
                COALESCE(SUM(CASE WHEN won = 1 THEN payout ELSE 0 END), 0)
             FROM arena_bets",
            [],
            |row| {
                Ok((
                    row.get::<_, i64>(0)? as u32,
                    row.get::<_, i64>(1)? as u32,
                    row.get::<_, i64>(2)?,
                    row.get::<_, i64>(3)?,
                ))
            },
        )
        .map_err(|e| format!("Betting aggregate error: {e}"))?;

    let win_pct = if total == 0 {
        0.0
    } else {
        (won as f64 / total as f64) * 100.0
    };

    Ok(ArenaBettingSummary {
        total,
        won,
        win_pct,
        total_wagered,
        total_won,
        net_profit: total_won - total_wagered,
    })
}

/// Aggregate all persisted matches into fighter records, a head-to-head matrix,
/// and a recent-results list.
pub fn aggregate_stats<C: DbRead + ?Sized>(conn: &C) -> Result<ArenaStats, String> {
    // Pull every match once; the dataset is tiny (hundreds of rows at most).
    let mut stmt = conn
        .prepare("SELECT fighter_a, fighter_b, winner FROM arena_matches")
        .map_err(|e| format!("Query prepare error: {e}"))?;

    let rows = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })
        .map_err(|e| format!("Query error: {e}"))?;

    use std::collections::HashMap;
    // name → (wins, losses)
    let mut overall: HashMap<String, (u32, u32)> = HashMap::new();
    // (winner, loser) → count
    let mut h2h: HashMap<(String, String), u32> = HashMap::new();
    let mut total = 0u32;

    for r in rows {
        let (a, b, winner) = r.map_err(|e| format!("Row error: {e}"))?;
        total += 1;
        let loser = if winner == a { b.clone() } else { a.clone() };
        overall.entry(winner.clone()).or_default().0 += 1;
        overall.entry(loser.clone()).or_default().1 += 1;
        *h2h.entry((winner, loser)).or_default() += 1;
    }

    let mut fighters: Vec<ArenaFighter> = overall
        .into_iter()
        .map(|(name, (wins, losses))| {
            let played = wins + losses;
            let win_pct = if played == 0 {
                0.0
            } else {
                (wins as f64 / played as f64) * 100.0
            };
            ArenaFighter {
                name,
                wins,
                losses,
                win_pct,
            }
        })
        .collect();
    fighters.sort_by(|x, y| {
        y.win_pct
            .partial_cmp(&x.win_pct)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(y.wins.cmp(&x.wins))
            .then(x.name.cmp(&y.name))
    });

    // Build directed head-to-head cells from the win-count map (both directions).
    let mut cells: HashMap<(String, String), ArenaHeadToHead> = HashMap::new();
    for ((winner, loser), count) in &h2h {
        cells
            .entry((winner.clone(), loser.clone()))
            .or_insert_with(|| ArenaHeadToHead {
                fighter: winner.clone(),
                opponent: loser.clone(),
                wins: 0,
                losses: 0,
            })
            .wins += count;
        cells
            .entry((loser.clone(), winner.clone()))
            .or_insert_with(|| ArenaHeadToHead {
                fighter: loser.clone(),
                opponent: winner.clone(),
                wins: 0,
                losses: 0,
            })
            .losses += count;
    }
    let head_to_head: Vec<ArenaHeadToHead> = cells.into_values().collect();

    // Recent matches, newest first.
    let mut recent_stmt = conn
        .prepare(
            "SELECT fought_at, fighter_a, fighter_b, winner FROM arena_matches
             ORDER BY fought_at DESC, id DESC LIMIT 15",
        )
        .map_err(|e| format!("Query prepare error: {e}"))?;
    let recent = recent_stmt
        .query_map([], |row| {
            Ok(ArenaRecentMatch {
                fought_at: row.get(0)?,
                fighter_a: row.get(1)?,
                fighter_b: row.get(2)?,
                winner: row.get(3)?,
            })
        })
        .map_err(|e| format!("Query error: {e}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("Row error: {e}"))?;

    let betting = aggregate_betting(conn)?;

    Ok(ArenaStats {
        total_matches: total,
        fighters,
        head_to_head,
        recent,
        betting,
    })
}

/// Aggregate persisted arena outcomes for the dashboard widget.
#[tauri::command]
pub fn get_arena_stats(db: State<'_, DbPool>) -> Result<ArenaStats, String> {
    let conn = db
        .get()
        .map_err(|e| format!("Database connection error: {e}"))?;
    aggregate_stats(&conn)
}

/// Read every `Chat-*.log` in the ChatLogs directory and persist any arena
/// matches found. Idempotent (unique index). Returns rows inserted.
///
/// A fresh [`ArenaTracker`] is driven line-by-line per file. Chat logs are read
/// oldest-to-newest within each file; intros and results interleave with all
/// other chatter, which the tracker filters to Kuzavek's `NPC Chatter` lines.
pub fn backfill_from_chat_logs(settings: &SettingsManager, db: &DbPool) -> Result<usize, String> {
    let Some(dir) = settings.get_chat_logs_dir() else {
        return Ok(0);
    };
    if !dir.is_dir() {
        return Ok(0);
    }

    // Read files in name order so timestamps advance monotonically across the
    // backfill (Chat-YY-MM-DD.log sorts chronologically).
    let mut paths: Vec<_> = fs::read_dir(&dir)
        .map_err(|e| format!("Failed to read ChatLogs dir: {e}"))?
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .map(|n| n.starts_with("Chat-") && n.ends_with(".log"))
                .unwrap_or(false)
        })
        .collect();
    paths.sort();

    let mut inserted = 0usize;

    for path in paths {
        let Ok(file) = File::open(&path) else {
            continue;
        };
        // Each daily log is its own arena session; a match never straddles two
        // files, so a per-file tracker avoids stale pairings across a midnight
        // gap.
        let mut tracker = ArenaTracker::new();

        // PARSE FIRST, WRITER SECOND: the app's single pooled writer must not
        // sit held while megabytes of log text are read and parsed. Collect
        // the matches for this file, then acquire the writer only around the
        // insert transaction. One IMMEDIATE transaction per file: startup
        // backfills run concurrently and a whole-scan transaction holds the
        // write lock long enough for the others' busy_timeout (5s) to expire;
        // inserts are idempotent (unique index), so per-file is safe.
        let matches: Vec<(String, String, String, String)> = {
            let reader = BufReader::new(file);
            let mut out = Vec::new();
            for line in reader.lines().map_while(Result::ok) {
                let Some(msg) = parse_chat_line(&line) else {
                    continue;
                };
                let ts = msg.timestamp.format("%Y-%m-%d %H:%M:%S").to_string();
                if let Some(m) = tracker.observe(
                    msg.channel.as_deref(),
                    msg.sender.as_deref(),
                    &msg.message,
                    &ts,
                ) {
                    out.push((m.fought_at, m.fighter_a, m.fighter_b, m.winner));
                }
            }
            out
        };
        if matches.is_empty() {
            continue;
        }

        let mut conn = db
            .get_write()
            .map_err(|e| format!("Database connection error: {e}"))?;
        let tx = conn
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(|e| format!("Failed to begin transaction: {e}"))?;
        for (fought_at, fighter_a, fighter_b, winner) in &matches {
            inserted += tx
                .execute(
                    "INSERT OR IGNORE INTO arena_matches
                        (fought_at, fighter_a, fighter_b, winner)
                     VALUES (?1, ?2, ?3, ?4)",
                    rusqlite::params![fought_at, fighter_a, fighter_b, winner],
                )
                .map_err(|e| format!("Insert error: {e}"))?;
        }
        tx.commit().map_err(|e| format!("Commit error: {e}"))?;
        drop(conn); // release the writer before the next file
    }

    Ok(inserted)
}

/// Tauri command wrapper around [`backfill_from_chat_logs`].
#[tauri::command]
pub fn backfill_arena_from_chat_logs(
    settings: State<'_, Arc<SettingsManager>>,
    db: State<'_, DbPool>,
) -> Result<usize, String> {
    backfill_from_chat_logs(&settings, &db)
}

/// Scan `Player-prev.log` then `Player.log` for the player's own arena bets and
/// persist the resolved ones. Idempotent (unique index). Returns rows inserted.
///
/// Unlike the chat-based fight backfill (many daily files), bets live only in
/// the current + previous Player.log, so betting history reaches back only as
/// far as those two files — it then accumulates permanently as new bets resolve
/// on each rescan. Player.log lines carry no date, so each file's last-modified
/// date anchors its bets (the poem-backfill pattern). A single tracker spans
/// both files in chronological order so a bet placed at the tail of the previous
/// log still resolves against a payout in the current one.
pub fn backfill_bets_from_player_logs(
    settings: &SettingsManager,
    db: &DbPool,
) -> Result<usize, String> {
    // Oldest first: previous log, then the current one.
    let paths: Vec<std::path::PathBuf> = [
        settings.get_player_prev_log_path(),
        settings.get_player_log_path(),
    ]
    .into_iter()
    .flatten()
    .filter(|p| p.is_file())
    .collect();

    if paths.is_empty() {
        return Ok(0);
    }

    // Writes route through the dedicated write connection like every other
    // writer (single-writer queue); BEGIN IMMEDIATE makes busy_timeout apply
    // to acquisition. One transaction PER FILE: a whole-scan transaction
    // holds the write lock across both Player logs while live ingest waits.
    // The bet tracker still spans files in order — only the transaction
    // boundary is per file. Idempotent (unique index), so per-file is safe.
    // PARSE FIRST, WRITER SECOND: the writer must not sit held while the
    // files are read and parsed — collect resolved bets per file, then
    // acquire the writer only around the insert transaction.
    let mut tracker = ArenaBetTracker::new();
    let mut inserted = 0usize;

    for path in &paths {
        // Anchor this file's HH:MM:SS lines to its last-modified date.
        let base_date = fs::metadata(path)
            .and_then(|m| m.modified())
            .ok()
            .map(|mtime| chrono::DateTime::<chrono::Utc>::from(mtime).date_naive());

        let Ok(text) = fs::read_to_string(path) else {
            continue;
        };
        let bets: Vec<_> = {
            let mut out = Vec::new();
            for line in text.lines() {
                // Cheap pre-filter — bet lines are a tiny fraction of Player.log.
                if !line.contains("You are betting ")
                    && !line.contains("Success! You have placed your bet for ")
                    && !line.contains("You received ")
                {
                    continue;
                }
                if let Some(bet) = tracker.observe_line(line, base_date) {
                    out.push(bet);
                }
            }
            out
        };
        if bets.is_empty() {
            continue;
        }

        let mut conn = db
            .get_write()
            .map_err(|e| format!("Database connection error: {e}"))?;
        let tx = conn
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(|e| format!("Failed to begin transaction: {e}"))?;
        for bet in &bets {
            // `&tx` derefs to `&Connection` for the shared recorder.
            inserted += record_arena_bet(
                &tx,
                &bet.placed_at,
                &bet.pick,
                &bet.opponent,
                bet.wager,
                bet.payout,
                bet.won,
            )?;
        }
        tx.commit().map_err(|e| format!("Commit error: {e}"))?;
        drop(conn); // release the writer before the next file
    }

    Ok(inserted)
}

/// Tauri command wrapper around [`backfill_bets_from_player_logs`].
#[tauri::command]
pub fn backfill_arena_bets_from_player_logs(
    settings: State<'_, Arc<SettingsManager>>,
    db: State<'_, DbPool>,
) -> Result<usize, String> {
    backfill_bets_from_player_logs(&settings, &db)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    fn setup() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE arena_matches (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                fought_at TEXT NOT NULL,
                fighter_a TEXT NOT NULL,
                fighter_b TEXT NOT NULL,
                winner TEXT NOT NULL
            );
            CREATE UNIQUE INDEX idx_arena_dedup
                ON arena_matches(fought_at, fighter_a, fighter_b);
            CREATE TABLE arena_bets (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                placed_at TEXT NOT NULL,
                pick TEXT NOT NULL,
                opponent TEXT NOT NULL,
                wager INTEGER NOT NULL,
                payout INTEGER NOT NULL,
                won INTEGER NOT NULL
            );
            CREATE UNIQUE INDEX idx_arena_bets_dedup
                ON arena_bets(placed_at, pick);",
        )
        .unwrap();
        conn
    }

    #[test]
    fn record_is_idempotent() {
        let conn = setup();
        for _ in 0..3 {
            record_arena_match(&conn, "2026-07-20 20:52:59", "Otis", "Leo", "Leo").unwrap();
        }
        let n: i64 = conn
            .query_row("SELECT COUNT(*) FROM arena_matches", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 1, "same match re-recorded must dedup via unique index");
    }

    #[test]
    fn aggregates_records_and_h2h() {
        let conn = setup();
        // Leo beats Otis twice; Otis beats Leo once.
        record_arena_match(&conn, "2026-07-20 10:00:00", "Otis", "Leo", "Leo").unwrap();
        record_arena_match(&conn, "2026-07-20 11:00:00", "Leo", "Otis", "Leo").unwrap();
        record_arena_match(&conn, "2026-07-20 12:00:00", "Otis", "Leo", "Otis").unwrap();

        let stats = aggregate_stats(&conn).unwrap();
        assert_eq!(stats.total_matches, 3);

        let leo = stats.fighters.iter().find(|f| f.name == "Leo").unwrap();
        assert_eq!((leo.wins, leo.losses), (2, 1));
        let otis = stats.fighters.iter().find(|f| f.name == "Otis").unwrap();
        assert_eq!((otis.wins, otis.losses), (1, 2));

        // Higher win pct sorts first.
        assert_eq!(stats.fighters.first().unwrap().name, "Leo");

        // Directed H2H: Leo vs Otis = 2-1; Otis vs Leo = 1-2.
        let lo = stats
            .head_to_head
            .iter()
            .find(|c| c.fighter == "Leo" && c.opponent == "Otis")
            .unwrap();
        assert_eq!((lo.wins, lo.losses), (2, 1));
        let ol = stats
            .head_to_head
            .iter()
            .find(|c| c.fighter == "Otis" && c.opponent == "Leo")
            .unwrap();
        assert_eq!((ol.wins, ol.losses), (1, 2));
    }

    #[test]
    fn empty_aggregate() {
        let conn = setup();
        let stats = aggregate_stats(&conn).unwrap();
        assert_eq!(stats.total_matches, 0);
        assert!(stats.fighters.is_empty());
        assert!(stats.head_to_head.is_empty());
        assert_eq!(stats.betting.total, 0);
        assert_eq!(stats.betting.net_profit, 0);
    }

    #[test]
    fn betting_summary_win_rate_and_pnl() {
        let conn = setup();
        // Two wins (7500→14250 each) and one loss (7500→0).
        record_arena_bet(
            &conn,
            "2026-07-20 10:00:00",
            "Otis",
            "Leo",
            7500,
            14250,
            true,
        )
        .unwrap();
        record_arena_bet(
            &conn,
            "2026-07-20 11:00:00",
            "Corrrak",
            "Gloz",
            7500,
            14250,
            true,
        )
        .unwrap();
        record_arena_bet(
            &conn,
            "2026-07-20 12:00:00",
            "Leo",
            "Dura",
            7500,
            14250,
            false,
        )
        .unwrap();

        let b = aggregate_stats(&conn).unwrap().betting;
        assert_eq!(b.total, 3);
        assert_eq!(b.won, 2);
        assert!((b.win_pct - 66.666).abs() < 0.01);
        assert_eq!(b.total_wagered, 22_500); // 3 × 7500
        assert_eq!(b.total_won, 28_500); // 2 × 14250
        assert_eq!(b.net_profit, 6_000); // 28500 - 22500
    }

    #[test]
    fn bet_record_is_idempotent() {
        let conn = setup();
        for _ in 0..3 {
            record_arena_bet(
                &conn,
                "2026-07-20 10:00:00",
                "Otis",
                "Leo",
                7500,
                14250,
                true,
            )
            .unwrap();
        }
        let n: i64 = conn
            .query_row("SELECT COUNT(*) FROM arena_bets", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 1);
    }
}
