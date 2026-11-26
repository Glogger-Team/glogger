//! Casino arena fight tracking.
//!
//! Persists resolved arena matches (who fought whom, and who won) parsed from
//! Kuzavek's `[NPC Chatter]` narration by [`crate::arena_parser`]. The dashboard
//! Arena widget reads the aggregate to rank fighters and predict the favorite in
//! any matchup — driven entirely from the chat log, with no manual tip entry.
//!
//! Only outcomes are stored; the `arena_bets` table holds historical wagers from
//! the retired `Player.log` bet scan (no new bets are discovered — the game's
//! verbose LocalPlayer lines no longer arrive). Rows dedup on
//! `(fought_at, fighter_a, fighter_b)` via a unique index, so re-ingesting the
//! same match from chat backfill is a no-op.

use serde::Serialize;
use std::fs::{self, File};
use std::io::{BufRead, BufReader};
use std::sync::Arc;
use tauri::State;

use super::{DbPool, DbRead};
use crate::arena_parser::ArenaTracker;
use crate::chat_parser::parse_chat_line;
use crate::settings::SettingsManager;

/// Per-fighter overall record.
#[derive(Debug, Serialize, Clone, specta::Type)]
pub struct ArenaFighter {
    pub name: String,
    pub wins: u32,
    pub losses: u32,
    /// Win percentage 0..=100, one decimal of precision retained by the frontend.
    pub win_pct: f64,
}

/// One directed head-to-head cell: `fighter` vs `opponent`.
#[derive(Debug, Serialize, Clone, specta::Type)]
pub struct ArenaHeadToHead {
    pub fighter: String,
    pub opponent: String,
    pub wins: u32,
    pub losses: u32,
}

/// A recently-observed resolved match, newest first.
#[derive(Debug, Serialize, Clone, specta::Type)]
pub struct ArenaRecentMatch {
    pub fought_at: String,
    pub fighter_a: String,
    pub fighter_b: String,
    pub winner: String,
}

/// The player's personal betting summary (their own bets, from Player.log).
#[derive(Debug, Serialize, Clone, Default, specta::Type)]
pub struct ArenaBettingSummary {
    /// Resolved bets (win or loss).
    pub total: u32,
    /// Bets won.
    pub won: u32,
    /// Win percentage 0..=100.
    pub win_pct: f64,
    /// Total councils wagered across all resolved bets.
    #[specta(type = u32)]
    pub total_wagered: i64,
    /// Total councils received from winning bets.
    #[specta(type = u32)]
    pub total_won: i64,
    /// Net profit/loss (total_won - total_wagered).
    #[specta(type = u32)]
    pub net_profit: i64,
}

/// Aggregate arena history for the dashboard widget.
#[derive(Debug, Serialize, Clone, Default, specta::Type)]
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

/// One arena tip for the current day, via the chat `EntityReport` twin
/// (observed tipster: Eveline Rastin). Tips land in `player_messages`
/// (message_type `entity_report`); the widget shows only today's.
#[derive(Debug, Serialize, Clone, specta::Type)]
pub struct ArenaTip {
    pub received_at: String,
    pub tipster: String,
    pub body: String,
}

/// Today's arena tips for one character+server, newest first. Tips are
/// per-character rows (like pigeons), unlike the global fight history, so
/// callers pass the active identity. The game day resets at midnight
/// America/New_York, so "today" is the current Eastern calendar day: the
/// UTC-converted stamps the dispatch writes (see `crate::chat_json_parser`)
/// are compared against the Eastern-midnight UTC range as fixed-format
/// strings (no schema change; `timestamp` stays `"%Y-%m-%d %H:%M:%S"`).
pub fn get_today_tips<C: DbRead + ?Sized>(
    conn: &C,
    character_name: &str,
    server_name: &str,
) -> Result<Vec<ArenaTip>, String> {
    // Eastern-midnight UTC range for the current Eastern day (precedent:
    // `get_current_moon_phase` in cdn_commands.rs). String comparison is
    // exact on the fixed-format stamps the dispatch writes.
    let eastern = chrono_tz::US::Eastern;
    let eastern_today = chrono::Utc::now().with_timezone(&eastern).date_naive();
    let day_start = eastern_today
        .and_hms_opt(0, 0, 0)
        .ok_or_else(|| "Eastern midnight is not a valid NaiveTime".to_string())?;
    let start_utc = chrono::TimeZone::from_local_datetime(&eastern, &day_start)
        .single()
        .map(|dt| dt.with_timezone(&chrono::Utc))
        .ok_or_else(|| "Eastern midnight did not resolve to a single UTC instant".to_string())?;
    let next_start = (eastern_today + chrono::Duration::days(1))
        .and_hms_opt(0, 0, 0)
        .ok_or_else(|| "Eastern midnight is not a valid NaiveTime".to_string())?;
    let end_utc = chrono::TimeZone::from_local_datetime(&eastern, &next_start)
        .single()
        .map(|dt| dt.with_timezone(&chrono::Utc))
        .ok_or_else(|| "Eastern midnight did not resolve to a single UTC instant".to_string())?;
    let start_s = start_utc.format("%Y-%m-%d %H:%M:%S").to_string();
    let end_s = end_utc.format("%Y-%m-%d %H:%M:%S").to_string();
    let mut stmt = conn
        .prepare(
            "SELECT timestamp, other_player, body FROM player_messages
             WHERE character_name = ?1 AND server_name = ?2
               AND message_type = 'entity_report'
               AND timestamp >= ?3 AND timestamp < ?4
             ORDER BY timestamp DESC",
        )
        .map_err(|e| format!("Tips query prepare error: {e}"))?;
    let rows = stmt
        .query_map(
            rusqlite::params![character_name, server_name, start_s, end_s],
            |row| {
                Ok(ArenaTip {
                    received_at: row.get(0)?,
                    tipster: row.get(1)?,
                    body: row.get(2)?,
                })
            },
        )
        .map_err(|e| format!("Tips query error: {e}"))?;
    rows
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("Tips row error: {e}"))
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

/// Aggregate the player's historical betting record (frozen — no new wagers).
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

/// Today's arena tips for the dashboard widget (active character+server).
#[tauri::command(rename_all = "camelCase")]
pub fn get_arena_tips(
    db: State<'_, DbPool>,
    character_name: String,
    server_name: String,
) -> Result<Vec<ArenaTip>, String> {
    let conn = db
        .get()
        .map_err(|e| format!("Database connection error: {e}"))?;
    get_today_tips(&conn, &character_name, &server_name)
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
        // Two wins (7500→14250 each) and one loss (7500→0), seeded as the
        // retired Player.log scan left them.
        conn.execute_batch(
            "INSERT INTO arena_bets (placed_at, pick, opponent, wager, payout, won) VALUES
                ('2026-07-20 10:00:00', 'Otis', 'Leo', 7500, 14250, 1),
                ('2026-07-20 11:00:00', 'Corrrak', 'Gloz', 7500, 14250, 1),
                ('2026-07-20 12:00:00', 'Leo', 'Dura', 7500, 14250, 0);",
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
    fn today_tips_filters_day_type_and_identity() {
        let conn = setup();
        conn.execute_batch(
            "CREATE TABLE player_messages (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                character_name TEXT NOT NULL,
                server_name TEXT NOT NULL,
                timestamp TEXT NOT NULL,
                message_type TEXT NOT NULL,
                direction TEXT NOT NULL,
                other_player TEXT NOT NULL,
                body TEXT NOT NULL DEFAULT '',
                item_name TEXT
            );",
        )
        .unwrap();
        // One tip today (kept), one yesterday (stale), one pigeon today
        // (wrong type), one tip today for another character (wrong identity).
        conn.execute_batch(
            "INSERT INTO player_messages
                (character_name, server_name, timestamp, message_type, direction, other_player, body) VALUES
                ('Aka', 'Dreva', datetime('now'), 'entity_report', 'in', 'Eveline Rastin', 'Ushug has the upper hand.'),
                ('Aka', 'Dreva', datetime('now', '-1 day'), 'entity_report', 'in', 'Eveline Rastin', 'stale tip'),
                ('Aka', 'Dreva', datetime('now'), 'pigeon', 'in', 'Friend', 'hello'),
                ('Other', 'Dreva', datetime('now'), 'entity_report', 'in', 'Eveline Rastin', 'not mine');",
        )
        .unwrap();

        // Eastern-midnight boundary: stamps an hour inside either side of
        // today's Eastern midnight, expressed in UTC as the dispatch writes
        // them. The 01:00 UTC shape lands on the previous Eastern day
        // (20:00/21:00 Eastern) and must be excluded.
        let eastern = chrono_tz::US::Eastern;
        let eastern_today = chrono::Utc::now()
            .with_timezone(&eastern)
            .date_naive();
        let eastern_midnight_utc = chrono::TimeZone::from_local_datetime(
            &eastern,
            &eastern_today.and_hms_opt(0, 0, 0).unwrap(),
        )
        .single()
        .expect("eastern midnight resolves")
        .with_timezone(&chrono::Utc);
        let just_after = (eastern_midnight_utc + chrono::Duration::hours(1))
            .format("%Y-%m-%d %H:%M:%S")
            .to_string();
        let just_before = (eastern_midnight_utc - chrono::Duration::hours(1))
            .format("%Y-%m-%d %H:%M:%S")
            .to_string();
        conn.execute(
            "INSERT INTO player_messages
                (character_name, server_name, timestamp, message_type, direction, other_player, body) VALUES
                ('Aka', 'Dreva', ?1, 'entity_report', 'in', 'Eveline Rastin', 'boundary kept'),
                ('Aka', 'Dreva', ?2, 'entity_report', 'in', 'Eveline Rastin', 'boundary stale'),
                ('Aka', 'Dreva', '2026-06-15 01:00:00', 'entity_report', 'in', 'Eveline Rastin', '01:00 utc stale')",
            rusqlite::params![just_after, just_before],
        )
        .unwrap();

        let tips = get_today_tips(&conn, "Aka", "Dreva").unwrap();
        assert_eq!(tips.len(), 2);
        let bodies: Vec<&str> = tips.iter().map(|t| t.body.as_str()).collect();
        assert!(bodies.iter().any(|b| b.contains("Ushug")));
        assert!(bodies.iter().any(|b| b.contains("boundary kept")));
        // 2026-06-15 01:00 UTC is 2026-06-14 21:00 EDT: previous Eastern
        // day, so the 01:00 UTC shape is always excluded.
        assert!(!tips.iter().any(|t| t.body.contains("01:00 utc")));
    }
}

// ── Specta schema export (TODO.md:113 shared schema contract) ───────────────
// `arena` IPC payload shapes derive `specta::Type` so
// `src/types/arena.specta.ts` can be generated from them. Test-only plumbing:
// the exporters live in `[dev-dependencies]`, so no new runtime dependency
// ships beyond `specta` itself, and this fn compiles out of non-test builds.
// Regenerate with `OVERWRITE_BINDINGS=1 cargo test arena_specta`.
#[cfg(test)]
pub fn export_bindings() -> Result<String, Box<dyn std::error::Error>> {
    let types = specta::Types::default()
        .register::<ArenaFighter>()
        .register::<ArenaHeadToHead>()
        .register::<ArenaRecentMatch>()
        .register::<ArenaBettingSummary>()
        .register::<ArenaStats>()
        .register::<ArenaTip>()
        ;
    Ok(specta_typescript::Typescript::default().export(&types, specta_serde::Format)?)
}

#[cfg(test)]
mod specta_bindings_tests {
    use super::export_bindings;

    /// Freshness pin: the checked-in `src/types/arena.specta.ts` must equal
    /// what `export_bindings` derives from the `arena` payload shapes. Any
    /// shape change (field add/remove/rename, serde attribute) fails here until the
    /// bindings are regenerated — that failure IS the drift signal. Regenerate with
    /// `OVERWRITE_BINDINGS=1 cargo test arena_specta`, inspect the diff, and
    /// confirm it matches the hand-mirrored frontend types before committing.
    #[test]
    fn test_arena_specta_bindings_are_fresh() {
        let fresh = export_bindings().expect("specta export failed");
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../src/types/arena.specta.ts",
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