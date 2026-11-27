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
use rusqlite::OptionalExtension;

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
    #[specta(type = i32)]
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
/// Additive parsed edge (`edge_kind`/`favored`/`opponent`/`edge_pct`):
/// `None` when the body doesn't parse — the widget falls back to its
/// client-side parse, so an unrecognized tip is never hidden. Existing
/// shape untouched otherwise.
#[derive(Debug, Serialize, Clone, specta::Type)]
pub struct ArenaTip {
    pub received_at: String,
    pub tipster: String,
    pub body: String,
    /// `"edge"` (matchup) or `"solo"` (single fighter), if parsed.
    pub edge_kind: Option<String>,
    /// Canonical fighter the edge favors, if parsed.
    pub favored: Option<String>,
    /// The other fighter (edge tips only), if parsed.
    pub opponent: Option<String>,
    /// Signed edge percent, if parsed.
    pub edge_pct: Option<i32>,
}

/// Canonical arena fighter for a raw tip name. Mirrors the gorgonzola
/// monitor's `_FIGHTERS` alias map, canonicalized to the dashboard
/// vocabulary (`SEED_ROSTER` in `arena_parser`: Vizlark, not Viz).
fn canonical_tip_fighter(raw: &str) -> Option<&'static str> {
    match raw.to_lowercase().as_str() {
        "otis" | "otiz" => Some("Otis"),
        "gloz" | "goz" => Some("Gloz"),
        "dura" => Some("Dura"),
        "ushug" | "ush" => Some("Ushug"),
        "viz" | "vizlark" | "vozlark" => Some("Vizlark"),
        "leo" => Some("Leo"),
        "corrrak" | "corrak" | "crrrak" | "corr" | "cor" => Some("Corrrak"),
        "na'a" | "naa" | "naaa" => Some("Na'a"),
        "lamp" => Some("Lamp"),
        _ => None,
    }
}

/// A parsed casino tip edge: which shape, who is favored, and by how much.
#[derive(Debug, Clone, PartialEq)]
pub struct ParsedTipEdge {
    pub kind: &'static str,
    pub favored: String,
    pub opponent: Option<String>,
    pub edge_pct: i32,
}

static TIP_EDGE_RE: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
    regex::Regex::new(
        r"(?is)\bIf\s+([A-Za-z']+)\s+and\s+([A-Za-z']+)\s+fight today\b.*?\bthat\s+([A-Za-z']+)\s+will have the upper hand\b.*?\(([+-]?\d+)%\)",
    )
    .expect("tip edge pattern is static")
});

static TIP_SOLO_RE: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
    regex::Regex::new(r"(?is)\b([A-Za-z']+)\b.*?\(([+-]?\d+)%\)")
        .expect("tip solo pattern is static")
});

/// Strip chat markup (`<i>…</i>`, `<indent=20>`) to a fixpoint so tag
/// residue can't glue itself to a fighter name.
fn strip_tip_tags(body: &str) -> String {
    static TAG_RE: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| regex::Regex::new(r"<[^>]*>").expect("tag pattern is static"));
    let mut out = body.to_string();
    loop {
        let next = TAG_RE.replace_all(&out, "").into_owned();
        if next == out {
            return out;
        }
        out = next;
    }
}

/// Parse a casino tip body into fighter+edge records. Edge shape first
/// (`If A and B fight today … that C will have the upper hand. (+N%)`),
/// then solo (`NAME … (±N%)`) — mirroring the monitor's `_parse_hot_tip_key`
/// ordering. An edge-shaped body that doesn't fully parse (`upper hand`
/// present, regex missed) returns `None` rather than misfiring as solo.
/// Unknown fighter names return `None` rather than misattributing.
pub fn parse_tip_edge(body: &str) -> Option<ParsedTipEdge> {
    let text = strip_tip_tags(body);
    if let Some(caps) = TIP_EDGE_RE.captures(&text) {
        let a = canonical_tip_fighter(caps.get(1)?.as_str())?;
        let b = canonical_tip_fighter(caps.get(2)?.as_str())?;
        let winner = canonical_tip_fighter(caps.get(3)?.as_str())?;
        if winner != a && winner != b {
            return None;
        }
        let loser = if winner == a { b } else { a };
        if loser == winner {
            return None;
        }
        let edge_pct: i32 = caps.get(4)?.as_str().parse().ok()?;
        return Some(ParsedTipEdge {
            kind: "edge",
            favored: winner.to_string(),
            opponent: Some(loser.to_string()),
            edge_pct,
        });
    }
    if text.to_lowercase().contains("upper hand") {
        return None;
    }
    let caps = TIP_SOLO_RE.captures(&text)?;
    let favored = canonical_tip_fighter(caps.get(1)?.as_str())?;
    let edge_pct: i32 = caps.get(2)?.as_str().parse().ok()?;
    Some(ParsedTipEdge {
        kind: "solo",
        favored: favored.to_string(),
        opponent: None,
        edge_pct,
    })
}

/// Eastern-midnight UTC range for the current Eastern day (precedent:
/// `get_current_moon_phase` in cdn_commands.rs). String comparison is exact
/// on the fixed-format stamps the dispatch writes. Shared by the
/// `entity_report` readers (tips, blood): both are day-scoped to the
/// casino's day, midnight America/New_York.
fn eastern_today_range() -> Result<(String, String), String> {
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
    Ok((
        start_utc.format("%Y-%m-%d %H:%M:%S").to_string(),
        end_utc.format("%Y-%m-%d %H:%M:%S").to_string(),
    ))
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
    let (start_s, end_s) = eastern_today_range()?;
    let mut stmt = conn
        .prepare(
            "SELECT timestamp, other_player, body FROM player_messages
             WHERE character_name = ?1 AND server_name = ?2
               AND message_type = 'entity_report'
               AND other_player <> 'Grasuul'
               AND body NOT LIKE '%Current Blood Season:%'
               AND timestamp >= ?3 AND timestamp < ?4
             ORDER BY timestamp DESC",
        )
        .map_err(|e| format!("Tips query prepare error: {e}"))?;
    let rows = stmt
        .query_map(
            rusqlite::params![character_name, server_name, start_s, end_s],
            |row| {
                let body: String = row.get(2)?;
                let edge = parse_tip_edge(&body);
                Ok(ArenaTip {
                    received_at: row.get(0)?,
                    tipster: row.get(1)?,
                    body,
                    edge_kind: edge.as_ref().map(|e| e.kind.to_string()),
                    favored: edge.as_ref().map(|e| e.favored.clone()),
                    opponent: edge.as_ref().and_then(|e| e.opponent.clone()),
                    edge_pct: edge.as_ref().map(|e| e.edge_pct),
                })
            },
        )
        .map_err(|e| format!("Tips query error: {e}"))?;
    rows
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("Tips row error: {e}"))
}

/// Grasuul's daily blood target, read off the stored EntityReport rows —
/// no new ingestion. The chat log carries his report window as an
/// `EntityReport` with title `Grasuul` and a body shaped
/// `<h1>Current Blood Season:</h1>...<h2>X Blood</h2>` (same pattern the
/// gorgonzola monitor matches). Day-scoped to the casino's Eastern day,
/// like the tips reader.
#[derive(Debug, Serialize, Clone, specta::Type)]
pub struct BloodTarget {
    pub target: String,
    pub received_at: String,
}

static BLOOD_SEASON_RE: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
    // (?is): case-insensitive + dot-matches-newline — live bodies carry a
    // real newline between the header and the indented target (monitor:
    // re.IGNORECASE | re.DOTALL).
    regex::Regex::new(r"(?is)Current Blood Season:.*?<h2>\s*([^<]+?)\s+Blood\s*</h2>")
        .expect("blood season pattern is static")
});

/// Extract the day's blood target from a Grasuul EntityReport body.
/// Multi-word targets ("Big Cat", "Fish and Snail") match — the group is
/// everything ahead of the trailing ` Blood`. Case-insensitive, mirroring
/// the gorgonzola monitor's `BLOOD_SEASON_RE`.
pub fn parse_blood_target(body: &str) -> Option<String> {
    BLOOD_SEASON_RE
        .captures(body)
        .and_then(|caps| caps.get(1))
        .map(|m| m.as_str().trim().to_string())
        .filter(|t| !t.is_empty())
}

/// Today's blood target for one character+server, if Grasuul has announced
/// it. Newest `entity_report` row from Grasuul whose body parses wins;
/// `None` when no announcement is on today's rows yet. Read-only.
pub fn get_blood_target<C: DbRead + ?Sized>(
    conn: &C,
    character_name: &str,
    server_name: &str,
) -> Result<Option<BloodTarget>, String> {
    let (start_s, end_s) = eastern_today_range()?;
    let mut stmt = conn
        .prepare(
            "SELECT timestamp, body FROM player_messages
             WHERE character_name = ?1 AND server_name = ?2
               AND message_type = 'entity_report'
               AND other_player = 'Grasuul'
               AND timestamp >= ?3 AND timestamp < ?4
             ORDER BY timestamp DESC",
        )
        .map_err(|e| format!("Blood target query prepare error: {e}"))?;
    let rows = stmt
        .query_map(
            rusqlite::params![character_name, server_name, start_s, end_s],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        )
        .map_err(|e| format!("Blood target query error: {e}"))?;
    for row in rows {
        let (received_at, body) = row.map_err(|e| format!("Blood target row error: {e}"))?;
        if let Some(target) = parse_blood_target(&body) {
            return Ok(Some(BloodTarget { target, received_at }));
        }
    }
    Ok(None)
}

/// Today's blood target for the dashboard Status widget (active identity).
#[tauri::command(rename_all = "camelCase")]
pub fn get_blood_target_cmd(
    db: State<'_, DbPool>,
    character_name: String,
    server_name: String,
) -> Result<Option<BloodTarget>, String> {
    let conn = db
        .get()
        .map_err(|e| format!("Database connection error: {e}"))?;
    get_blood_target(&conn, &character_name, &server_name)
}

/// One active warden alert, parsed from the "Active Warden Alerts" book.
#[derive(Debug, Serialize, Clone, PartialEq, specta::Type)]
pub struct WardenAlert {
    pub npc: String,
    pub zone: String,
    pub threat: String,
    pub ends: String,
}

/// The newest warden-alerts book read, with its alerts. `None` when no
/// warden book has landed yet — the widget shows an honest empty state.
#[derive(Debug, Serialize, Clone, specta::Type)]
pub struct WardenReport {
    pub captured_at: Option<String>,
    pub alerts: Vec<WardenAlert>,
}

static WARDEN_H2_RE: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
    // No look-ahead (unsupported): split on the literal `<h2>` (monitor is
    // case-sensitive here too) and match the header at each chunk start.
    // Inner threat/ends patterns take their FIRST hit, which always
    // belongs to the current alert even though the tail holds later ones.
    regex::Regex::new(r"(?is)\s*Aid Request from (.+?) in (.+?)</h2>(.*)")
        .expect("warden header pattern is static")
});

static WARDEN_THREAT_RE: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
    regex::Regex::new(r"(?is)assistance with (.+?)\.?\s*</indent>")
        .expect("warden threat pattern is static")
});

static WARDEN_INDENT_RE: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
    regex::Regex::new(r"(?is)<indent=\d+>(.+?)</indent>")
        .expect("warden indent pattern is static")
});

static WARDEN_ENDS_RE: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
    regex::Regex::new(r"(?is)Ends in ([^<]+?)\s*</i>").expect("warden ends pattern is static")
});

/// Parse an ActiveWardenAlerts book body into alerts, mirroring the
/// gorgonzola monitor's `_parse_warden_alerts` (alert per `<h2>Aid Request
/// from NPC in Zone</h2>`, threat after "assistance with", `Ends in …`
/// line). The monitor's absolute `ends_at` is deliberately not computed:
/// it bakes `now()` into stored rows, which would unpin tests — the widget
/// shows the raw relative `ends` text.
pub fn parse_warden_alerts(content: &str) -> Vec<WardenAlert> {
    content
        .split("<h2>")
        .filter_map(|chunk| WARDEN_H2_RE.captures(chunk))
        .map(|caps| {
            let npc = caps.get(1).map(|m| m.as_str().trim().to_string()).unwrap_or_default();
            let zone = caps.get(2).map(|m| m.as_str().trim().to_string()).unwrap_or_default();
            let rest = caps.get(3).map(|m| m.as_str()).unwrap_or("");
            let threat = WARDEN_THREAT_RE
                .captures(rest)
                .and_then(|t| t.get(1))
                .map(|m| m.as_str().trim().to_string())
                .or_else(|| {
                    WARDEN_INDENT_RE
                        .captures(rest)
                        .and_then(|t| t.get(1))
                        .map(|m| m.as_str().trim().to_string())
                })
                .unwrap_or_default();
            let ends = WARDEN_ENDS_RE
                .captures(rest)
                .and_then(|t| t.get(1))
                .map(|m| m.as_str().trim().to_string())
                .unwrap_or_default();
            WardenAlert { npc, zone, threat, ends }
        })
        .collect()
}

/// The newest "Active Warden Alerts" book for one character+server, with
/// its parsed alerts. The book lands in `game_state_books` through two
/// title-agnostic arms — the `[JSON:Book]` dispatch arm (book_type `JSON`,
/// title as-is) and the Books-dir watcher (book_type `ActiveWardenAlerts`,
/// title = export stem) — so the match is on the title, not the
/// book_type. No day gate: the book carries no Eastern-day notion and
/// alerts stay live across midnights (`Ends in …` is relative). Read-only.
pub fn get_warden_alerts<C: DbRead + ?Sized>(
    conn: &C,
    character_name: &str,
    server_name: &str,
) -> Result<Option<WardenReport>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT content, captured_at FROM game_state_books
             WHERE character_name = ?1 AND server_name = ?2
               AND title LIKE '%warden%alert%'
             ORDER BY captured_at DESC LIMIT 1",
        )
        .map_err(|e| format!("Warden alerts query prepare error: {e}"))?;
    let row: Option<(String, Option<String>)> = stmt
        .query_row(rusqlite::params![character_name, server_name], |row| {
            Ok((row.get(0)?, row.get(1)?))
        })
        .optional()
        .map_err(|e| format!("Warden alerts query error: {e}"))?;
    Ok(row.map(|(content, captured_at)| WardenReport {
        alerts: parse_warden_alerts(&content),
        captured_at,
    }))
}

/// The newest warden-alerts book for the dashboard Status widget (active identity).
#[tauri::command(rename_all = "camelCase")]
pub fn get_warden_alerts_cmd(
    db: State<'_, DbPool>,
    character_name: String,
    server_name: String,
) -> Result<Option<WardenReport>, String> {
    let conn = db
        .get()
        .map_err(|e| format!("Database connection error: {e}"))?;
    get_warden_alerts(&conn, &character_name, &server_name)
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

    fn setup_messages(conn: &Connection) {
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
    }

    fn blood_body(target: &str) -> String {
        format!(
            "<h1>Current Blood Season:</h1>\n<indent=20><h2>{target} Blood</h2></indent>..."
        )
    }

    #[test]
    fn blood_parse_covers_single_and_multiword_targets() {
        assert_eq!(
            parse_blood_target(&blood_body("Elf")).as_deref(),
            Some("Elf")
        );
        // Multi-word targets must survive the capture (monitor precedent:
        // "Big Cat", "Fish and Snail").
        assert_eq!(
            parse_blood_target(&blood_body("Big Cat")).as_deref(),
            Some("Big Cat")
        );
        assert_eq!(
            parse_blood_target(&blood_body("Fish and Snail")).as_deref(),
            Some("Fish and Snail")
        );
        assert_eq!(parse_blood_target("Here's a hot tip: Ushug (+10%)"), None);
        assert_eq!(parse_blood_target(""), None);
    }

    #[test]
    fn blood_target_reads_todays_grasuul_row() {
        let conn = setup();
        setup_messages(&conn);
        conn.execute(
            "INSERT INTO player_messages
                (character_name, server_name, timestamp, message_type, direction, other_player, body) VALUES
                ('Aka', 'Dreva', datetime('now'), 'entity_report', 'in', 'Grasuul', ?1)",
            rusqlite::params![blood_body("Big Cat")],
        )
        .unwrap();
        let got = get_blood_target(&conn, "Aka", "Dreva").unwrap();
        let got = got.expect("today's Grasuul row must parse");
        assert_eq!(got.target, "Big Cat");
    }

    #[test]
    fn blood_target_ignores_stale_and_other_identities() {
        let conn = setup();
        setup_messages(&conn);
        conn.execute_batch(
            "INSERT INTO player_messages
                (character_name, server_name, timestamp, message_type, direction, other_player, body) VALUES
                ('Aka', 'Dreva', datetime('now', '-1 day'), 'entity_report', 'in', 'Grasuul', '<h1>Current Blood Season:</h1><h2>Elf Blood</h2>'),
                ('Other', 'Dreva', datetime('now'), 'entity_report', 'in', 'Grasuul', '<h1>Current Blood Season:</h1><h2>Elf Blood</h2>');",
        )
        .unwrap();
        assert!(
            get_blood_target(&conn, "Aka", "Dreva").unwrap().is_none(),
            "yesterday's row and another character's row must not serve"
        );
    }

    #[test]
    fn blood_target_newest_row_wins() {
        let conn = setup();
        setup_messages(&conn);
        // Two announcements today: the later stamp is the day's target.
        conn.execute(
            "INSERT INTO player_messages
                (character_name, server_name, timestamp, message_type, direction, other_player, body) VALUES
                ('Aka', 'Dreva', datetime('now', '-2 hours'), 'entity_report', 'in', 'Grasuul', ?1),
                ('Aka', 'Dreva', datetime('now'), 'entity_report', 'in', 'Grasuul', ?2)",
            rusqlite::params![blood_body("Elf"), blood_body("Fish and Snail")],
        )
        .unwrap();
        let got = get_blood_target(&conn, "Aka", "Dreva").unwrap().expect("row expected");
        assert_eq!(got.target, "Fish and Snail");
    }

    #[test]
    fn today_tips_excludes_blood_but_keeps_casino_tips() {
        let conn = setup();
        setup_messages(&conn);
        // One casino day: a Grasuul blood announcement plus two real tips
        // (Eveline + a casino NPC). Blood leaves, both tips stay.
        conn.execute(
            "INSERT INTO player_messages
                (character_name, server_name, timestamp, message_type, direction, other_player, body) VALUES
                ('Aka', 'Dreva', datetime('now'), 'entity_report', 'in', 'Grasuul', ?1),
                ('Aka', 'Dreva', datetime('now'), 'entity_report', 'in', 'Eveline Rastin', 'Ushug has the upper hand.'),
                ('Aka', 'Dreva', datetime('now'), 'entity_report', 'in', 'Mandibles', 'Gloz is feeling great! (+5%)')",
            rusqlite::params![blood_body("Elf")],
        )
        .unwrap();
        let tips = get_today_tips(&conn, "Aka", "Dreva").unwrap();
        assert_eq!(tips.len(), 2, "blood row must leave, both tips stay");
        let tipsters: Vec<&str> = tips.iter().map(|t| t.tipster.as_str()).collect();
        assert!(tipsters.contains(&"Eveline Rastin"));
        assert!(tipsters.contains(&"Mandibles"));
        assert!(!tipsters.contains(&"Grasuul"));
    }

    #[test]
    fn tip_edge_parses_edge_shape() {
        // Verbatim body from test_data/vip_json/chat-entityreport.log.
        let edge = parse_tip_edge(
            "Here's a hot tip:\n\n<i>If Ushug and Leo fight today, I'm pretty sure that Ushug will have the upper hand. (+10%)</i>",
        )
        .expect("live exemplar must parse");
        assert_eq!(edge.kind, "edge");
        assert_eq!(edge.favored, "Ushug");
        assert_eq!(edge.opponent.as_deref(), Some("Leo"));
        assert_eq!(edge.edge_pct, 10);
    }

    #[test]
    fn tip_edge_winner_may_be_second_named() {
        let edge = parse_tip_edge(
            "If Dura and Corrrak fight today, that Corrrak will have the upper hand. (+10%)",
        )
        .expect("edge must parse");
        assert_eq!(edge.favored, "Corrrak");
        assert_eq!(edge.opponent.as_deref(), Some("Dura"));
    }

    #[test]
    fn tip_edge_shaped_but_unparsed_never_solo_misfires() {
        // "upper hand" present but no full matchup: None, not a solo tip.
        assert_eq!(
            parse_tip_edge("Ushug will have the upper hand today, trust me!"),
            None
        );
        assert_eq!(parse_tip_edge("No numbers here, just vibes."), None);
        assert_eq!(
            parse_tip_edge("If Zog and Blarg fight today, Zog wins. (+10%)"),
            None,
            "unknown fighters must not misattribute"
        );
    }

    #[test]
    fn tip_edge_parses_solo_shapes_signed() {
        let plus = parse_tip_edge("Gloz was training. She's feeling great! (+5%)")
            .expect("solo plus must parse");
        assert_eq!(plus.kind, "solo");
        assert_eq!(plus.favored, "Gloz");
        assert_eq!(plus.opponent, None);
        assert_eq!(plus.edge_pct, 5);
        // The widget's client parse only covers (+N%); the backend covers (-N%) too.
        let minus = parse_tip_edge("Leo is in terrible form today. (-5%)")
            .expect("solo minus must parse");
        assert_eq!(minus.favored, "Leo");
        assert_eq!(minus.edge_pct, -5);
    }

    #[test]
    fn tip_edge_canonicalizes_aliases_to_dashboard_names() {
        // Monitor aliases (Otiz, Viz) resolve to the dashboard vocabulary
        // (SEED_ROSTER: Otis, Vizlark).
        let edge = parse_tip_edge("If Otiz and Viz fight today, that Viz will have the upper hand. (+10%)")
            .expect("aliased edge must parse");
        assert_eq!(edge.favored, "Vizlark");
        assert_eq!(edge.opponent.as_deref(), Some("Otis"));
    }

    #[test]
    fn today_tips_carry_parsed_edge_fields() {
        let conn = setup();
        setup_messages(&conn);
        conn.execute(
            "INSERT INTO player_messages
                (character_name, server_name, timestamp, message_type, direction, other_player, body) VALUES
                ('Aka', 'Dreva', datetime('now'), 'entity_report', 'in', 'Eveline Rastin', ?1)",
            rusqlite::params![
                "If Ushug and Leo fight today, I'm pretty sure that Ushug will have the upper hand. (+10%)"
            ],
        )
        .unwrap();
        let tips = get_today_tips(&conn, "Aka", "Dreva").unwrap();
        assert_eq!(tips.len(), 1);
        assert_eq!(tips[0].edge_kind.as_deref(), Some("edge"));
        assert_eq!(tips[0].favored.as_deref(), Some("Ushug"));
        assert_eq!(tips[0].opponent.as_deref(), Some("Leo"));
        assert_eq!(tips[0].edge_pct, Some(10));
    }

    fn setup_books(conn: &Connection) {
        conn.execute_batch(
            "CREATE TABLE game_state_books (
                character_name TEXT NOT NULL,
                server_name TEXT NOT NULL,
                book_type TEXT NOT NULL,
                title TEXT NOT NULL,
                content TEXT NOT NULL DEFAULT '',
                captured_at TEXT,
                UNIQUE(character_name, server_name, book_type, title)
            );",
        )
        .unwrap();
    }

    fn warden_body() -> String {
        "<h1>Active Warden Alerts</h1>\n\
         <h2>Aid Request from Yusuf in Serbule</h2>\
         <indent=20>Yusuf requests assistance with culling undead rats.</indent>\
         <i>Ends in 23 hours 48 minutes</i>\n\
         <h2>Aid Request from Marna in Eltibule</h2>\
         <indent=20>Marna requests assistance with giant bats.</indent>\
         <i>Ends in 5 hours 10 minutes</i>"
            .to_string()
    }

    #[test]
    fn warden_parse_covers_alert_fields() {
        let alerts = parse_warden_alerts(&warden_body());
        assert_eq!(alerts.len(), 2);
        assert_eq!(alerts[0].npc, "Yusuf");
        assert_eq!(alerts[0].zone, "Serbule");
        assert_eq!(alerts[0].threat, "culling undead rats");
        assert_eq!(alerts[0].ends, "23 hours 48 minutes");
        assert_eq!(alerts[1].npc, "Marna");
        assert_eq!(alerts[1].ends, "5 hours 10 minutes");
        // No "assistance with": falls back to the first indent block.
        let fallback = parse_warden_alerts(
            "<h2>Aid Request from Joe in Kur</h2><indent=20>Something is wrong.</indent>",
        );
        assert_eq!(fallback.len(), 1);
        assert_eq!(fallback[0].threat, "Something is wrong.");
        assert_eq!(parse_warden_alerts("no alerts here"), vec![]);
    }

    #[test]
    fn warden_reader_serves_newest_row_either_title_shape() {
        let conn = setup();
        setup_books(&conn);
        // Both landing arms: the JSON dispatch row (title as-is) and the
        // Books-dir export row (title = export stem). Newest wins.
        conn.execute(
            "INSERT INTO game_state_books
                (character_name, server_name, book_type, title, content, captured_at) VALUES
                ('Aka', 'Dreva', 'JSON', 'Active Warden Alerts', ?1, '2026-01-01T00:00:00+00:00'),
                ('Aka', 'Dreva', 'ActiveWardenAlerts', 'ActiveWardenAlerts_260907_210009', ?2, '2026-06-01T00:00:00+00:00')",
            rusqlite::params![
                "<h2>Aid Request from Old in Zone</h2><indent=20>Old requests assistance with stale.</indent>",
                warden_body()
            ],
        )
        .unwrap();
        let report = get_warden_alerts(&conn, "Aka", "Dreva").unwrap().expect("row expected");
        assert_eq!(report.alerts.len(), 2);
        assert_eq!(report.alerts[0].npc, "Yusuf");
        assert_eq!(report.captured_at.as_deref(), Some("2026-06-01T00:00:00+00:00"));
    }

    #[test]
    fn warden_reader_empty_and_scoped() {
        let conn = setup();
        setup_books(&conn);
        assert!(get_warden_alerts(&conn, "Aka", "Dreva").unwrap().is_none());
        conn.execute(
            "INSERT INTO game_state_books
                (character_name, server_name, book_type, title, content, captured_at) VALUES
                ('Aka', 'Dreva', 'JSON', 'Gardening Almanac', 'garden', '2026-06-01T00:00:00+00:00'),
                ('Other', 'Dreva', 'JSON', 'Active Warden Alerts', ?1, '2026-06-01T00:00:00+00:00')",
            rusqlite::params![warden_body()],
        )
        .unwrap();
        assert!(
            get_warden_alerts(&conn, "Aka", "Dreva").unwrap().is_none(),
            "garden books and other characters' warden books must not serve"
        );
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
        .register::<BloodTarget>()
        .register::<WardenAlert>()
        .register::<WardenReport>()
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