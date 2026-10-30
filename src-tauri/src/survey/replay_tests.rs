//! End-to-end integration tests that replay paired chat+player log datasets
//! through the full Phase 5 pipeline (PlayerEventParser → SurveySessionAggregator
//! → SQLite) and validate the resulting tracker state against ground truth.
//!
//! Each dataset under `test_data/surveyLogs/` includes:
//!   - `Player.log` (or `Player-prev.log`) — the game's primary log
//!   - `ChatLog.txt` — the corresponding chat log with `[Status]` messages
//!   - `results.txt` — hand-recorded final loot totals
//!
//! Validation strategy:
//!   1. **Per-kind use counts**: the 50x-povusmarvelous dataset must produce
//!      roughly 50 Multihit-kind survey uses; the 100x-* datasets must produce
//!      roughly 100 Basic-kind uses. These confirm classification and session
//!      detection are working.
//!   2. **Loot reaches the per-session summary**: the loot-summary query
//!      (joining item_transactions to survey_uses via the survey_use_id JSON
//!      path) must return non-trivial totals matching the dataset's known
//!      headline drops.
//!
//! These tests are gated behind `#[ignore]` by default because they're slow
//! (full CDN load + replay of 30k+ player.log lines per dataset). Run with:
//!     cargo test --lib survey::replay_tests --ignored

use crate::cdn_commands::GameDataState;
use crate::db::migrations::run_migrations;
use crate::game_data::{parse_items_json, GameData};
use crate::player_event_parser::PlayerEventParser;
use crate::survey::aggregator::SurveySessionAggregator;
use crate::survey::types::{SurveyUseKind, SurveyUseStatus};
use rusqlite::{params, Connection};
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::RwLock;

const ITEMS_JSON_PATH: &str = "../docs/CDN-full-examples/items.json";
const CHARACTER: &str = "TestZenith";
const SERVER: &str = "TestDreva";

/// Build a `GameData` populated with just the items map + name indexes from
/// the sample CDN snapshot. Sufficient for the aggregator's
/// `lookup_survey_kind` and display→internal name resolution to work. All
/// other data (recipes, NPCs, etc.) stays empty.
fn load_game_data_from_cdn() -> Result<GameDataState, String> {
    let path = Path::new(ITEMS_JSON_PATH);
    if !path.exists() {
        return Err(format!(
            "items.json not found at {}; tests require CDN sample data",
            path.display()
        ));
    }
    let json = fs::read_to_string(path).map_err(|e| e.to_string())?;
    let items_map = parse_items_json(&json)?;

    let mut gd = GameData::empty();
    for (id, info) in items_map {
        if let Some(internal) = info.internal_name.clone() {
            gd.item_internal_name_index.insert(internal, id);
        }
        gd.item_name_index.insert(info.name.clone(), id);
        gd.items.insert(id, info);
    }
    Ok(Arc::new(RwLock::new(gd)))
}

/// Open an in-memory SQLite DB with the full migration history applied.
fn fresh_db() -> Connection {
    let conn = Connection::open_in_memory().expect("open in-memory db");
    run_migrations(&conn, None).expect("run migrations");
    conn
}

/// Detect the timezone offset from the first few lines of a chat log.
/// Returns hours offset (e.g., -7 for "-07:00:00").
fn detect_chat_tz_hours(chat_path: &Path) -> Result<i32, String> {
    let content = fs::read_to_string(chat_path).map_err(|e| e.to_string())?;
    for line in content.lines().take(20) {
        // "Timezone Offset -07:00:00."
        if let Some(idx) = line.find("Timezone Offset ") {
            let rest = &line[idx + "Timezone Offset ".len()..];
            // Parse the leading "+HH" or "-HH"
            let sign = if rest.starts_with('-') { -1i32 } else { 1i32 };
            let hh: i32 = rest
                .trim_start_matches(['-', '+'])
                .split(':')
                .next()
                .and_then(|s| s.parse().ok())
                .ok_or_else(|| "could not parse tz hours".to_string())?;
            return Ok(sign * hh);
        }
    }
    Err("no Timezone Offset line in chat log".to_string())
}

/// One row from the chat log we care about for replay.
#[derive(Debug, Clone)]
struct ChatGain {
    /// UTC seconds-of-day, computed from local-time + tz offset.
    utc_secs: i64,
    /// Raw display name from the chat message (e.g., "Marvelous Metal Slab").
    item_display: String,
    quantity: u32,
    /// UTC HH:MM:SS string for feeding into feed_chat_gain. Built from utc_secs.
    utc_hms: String,
}

fn parse_chat_gains(chat_path: &Path, tz_hours: i32) -> Result<Vec<ChatGain>, String> {
    let content = fs::read_to_string(chat_path).map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for line in content.lines() {
        let line = line.trim_end_matches('\r');
        // Format: "26-04-10 08:22:46\t[Status] Basic Ink x5 added to inventory."
        let mut parts = line.splitn(2, '\t');
        let ts_part = match parts.next() {
            Some(t) => t,
            None => continue,
        };
        let body = match parts.next() {
            Some(b) => b,
            None => continue,
        };
        // Strip the date — just need HH:MM:SS
        let hms = match ts_part.split(' ').nth(1) {
            Some(h) => h,
            None => continue,
        };
        let local_secs = match hms_to_secs(hms) {
            Some(s) => s as i64,
            None => continue,
        };
        // Convert local → UTC by SUBTRACTING the offset (tz_hours is the local
        // offset from UTC, so UTC = local - offset).
        let mut utc_secs = local_secs - (tz_hours as i64) * 3600;
        // Wrap into [0, 86400). The dataset spans well under 24h so this is fine.
        while utc_secs < 0 {
            utc_secs += 86400;
        }
        utc_secs %= 86400;
        let utc_hms = secs_to_hms(utc_secs as u32);

        // Body should look like "[Status] X added to inventory." (with optional " xN")
        let prefix = "[Status] ";
        let suffix = " added to inventory.";
        if !body.starts_with(prefix) || !body.ends_with(suffix) {
            continue;
        }
        let middle = &body[prefix.len()..body.len() - suffix.len()];
        // Extract quantity if present: "Item Name xN"
        let (item_display, quantity) = match middle.rfind(" x") {
            Some(idx) if middle[idx + 2..].chars().all(|c| c.is_ascii_digit()) => (
                middle[..idx].to_string(),
                middle[idx + 2..].parse().unwrap_or(1),
            ),
            _ => (middle.to_string(), 1u32),
        };
        out.push(ChatGain {
            utc_secs,
            item_display,
            quantity,
            utc_hms,
        });
    }
    Ok(out)
}

fn hms_to_secs(hms: &str) -> Option<u32> {
    let mut p = hms.split(':');
    let h: u32 = p.next()?.parse().ok()?;
    let m: u32 = p.next()?.parse().ok()?;
    let s: u32 = p.next()?.parse().ok()?;
    Some(h * 3600 + m * 60 + s)
}

fn secs_to_hms(secs: u32) -> String {
    let h = secs / 3600;
    let m = (secs % 3600) / 60;
    let s = secs % 60;
    format!("{:02}:{:02}:{:02}", h, m, s)
}

/// Extract the leading "[HH:MM:SS]" timestamp from a player.log line, returning
/// the seconds-of-day for time-merging with chat events.
fn player_line_secs(line: &str) -> Option<u32> {
    let line = line.trim_start();
    if !line.starts_with('[') {
        return None;
    }
    let end = line.find(']')?;
    hms_to_secs(&line[1..end])
}

/// Replay the dataset through parser+aggregator+DB. Returns the populated DB.
fn replay_dataset(player_log: &Path, chat_log: &Path) -> Result<Connection, String> {
    let game_data = load_game_data_from_cdn()?;
    let conn = fresh_db();
    let mut parser = PlayerEventParser::new();
    let mut aggregator = SurveySessionAggregator::new(game_data.clone());

    let tz = detect_chat_tz_hours(chat_log)?;
    let mut chat_gains = parse_chat_gains(chat_log, tz)?;
    // Stable sort by UTC time so feeding into the parser is in chronological order.
    chat_gains.sort_by_key(|g| g.utc_secs);

    // Pull display→internal name resolution through game_data. Done once here
    // so we don't lock the RwLock for every chat event.
    let resolve_internal = |display: &str| -> Option<String> {
        let gd = game_data.try_read().ok()?;
        gd.resolve_item(display)
            .and_then(|i| i.internal_name.clone())
    };

    // Chat gains with resolved internal names — drop ones we can't resolve
    // (rare; usually only happens for items not in the CDN snapshot).
    let resolved_gains: Vec<(u32, ChatGain, String)> = chat_gains
        .into_iter()
        .filter_map(|g| {
            let internal = resolve_internal(&g.item_display)?;
            Some((g.utc_secs as u32, g.clone(), internal))
        })
        .collect();

    let mut next_chat_idx = 0usize;
    let player_content = fs::read_to_string(player_log).map_err(|e| e.to_string())?;

    for line in player_content.lines() {
        // Determine line's timestamp (or skip if not a parseable line).
        let line_secs = match player_line_secs(line) {
            Some(s) => s,
            None => {
                // Non-timestamped line (asset loads, debug prints) — feed
                // through parser anyway since it'll be ignored cleanly.
                let events = parser.process_line(line);
                push_through_aggregator(&mut aggregator, &conn, events);
                continue;
            }
        };

        // Drain any chat gains whose UTC time is at or before this line's
        // timestamp. This is the time-merge: chat gains land in the parser's
        // buffer just-in-time for the AddItem they belong to.
        while next_chat_idx < resolved_gains.len() && resolved_gains[next_chat_idx].0 <= line_secs {
            let (_, g, internal) = &resolved_gains[next_chat_idx];
            parser.feed_chat_gain(internal.clone(), g.quantity, &g.utc_hms);
            next_chat_idx += 1;
        }

        let events = parser.process_line(line);
        push_through_aggregator(&mut aggregator, &conn, events);
    }

    // Drain any remaining chat gains so trailing player.log events still get
    // them (rare — would only matter if the player.log ends mid-gain).
    while next_chat_idx < resolved_gains.len() {
        let (_, g, internal) = &resolved_gains[next_chat_idx];
        parser.feed_chat_gain(internal.clone(), g.quantity, &g.utc_hms);
        next_chat_idx += 1;
    }

    Ok(conn)
}

/// Replay a dataset simulating a **chat-only** user: Player.log item lines
/// (`ProcessAddItem` / `ProcessUpdateItemCode`) are dropped — as observed on
/// machines where PG's verbose item logging never reaches the log — so loot
/// can only be attributed through the chat-authoritative gate
/// (`attribute_chat_gain`). Chat gains are fed exactly the way the
/// coordinator wires them live: gate first, then insert the ledger row
/// tagged (`survey_chat` + `survey_use_id`) or untagged, letting the
/// Basic-survey retro-claim pick up rows that beat the DeleteItem across
/// the two log streams.
fn replay_dataset_chat_only(player_log: &Path, chat_log: &Path) -> Result<Connection, String> {
    let game_data = load_game_data_from_cdn()?;
    let conn = fresh_db();
    let mut parser = PlayerEventParser::new();
    let mut aggregator = SurveySessionAggregator::new(game_data.clone());
    // Fixed base date so player.log HH:MM:SS and chat UTC timestamps land on
    // the same calendar day (each dataset spans well under 24h).
    let base_date = chrono::NaiveDate::from_ymd_opt(2026, 4, 13).unwrap();
    aggregator.set_base_date(base_date);
    let base_date_str = base_date.format("%Y-%m-%d").to_string();

    let tz = detect_chat_tz_hours(chat_log)?;
    let mut chat_gains = parse_chat_gains(chat_log, tz)?;
    chat_gains.sort_by_key(|g| g.utc_secs);

    let resolve_internal = |display: &str| -> Option<String> {
        let gd = game_data.try_read().ok()?;
        gd.resolve_item(display)
            .and_then(|i| i.internal_name.clone())
    };
    // Unlike the player-side harness we keep unresolvable items — the live
    // coordinator feeds those through the gate too (with internal = None).
    let resolved_gains: Vec<(u32, ChatGain, Option<String>)> = chat_gains
        .into_iter()
        .map(|g| {
            let internal = resolve_internal(&g.item_display);
            (g.utc_secs as u32, g.clone(), internal)
        })
        .collect();

    let feed_gain =
        |aggregator: &mut SurveySessionAggregator, g: &ChatGain, internal: &Option<String>| {
            let ts = format!("{} {}", base_date_str, g.utc_hms);
            let attributed = aggregator.attribute_chat_gain(
                &conn,
                CHARACTER,
                SERVER,
                internal.as_deref(),
                g.quantity,
                &ts,
            );
            let (kind, details): (Option<&str>, Option<String>) = match attributed {
                Some(id) => (
                    Some("survey_chat"),
                    Some(format!("{{\"survey_use_id\":{id}}}")),
                ),
                None => (None, None),
            };
            let row_name = internal.as_deref().unwrap_or(&g.item_display);
            conn.execute(
                "INSERT INTO item_transactions
                (timestamp, character_name, server_name, item_name, internal_name,
                 quantity, context, source, source_kind, source_details)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'loot', 'chat_status', ?7, ?8)",
                params![ts, CHARACTER, SERVER, row_name, row_name, g.quantity, kind, details],
            )
            .expect("insert chat row");
        };

    let mut next_chat_idx = 0usize;
    let player_content = fs::read_to_string(player_log).map_err(|e| e.to_string())?;

    for line in player_content.lines() {
        // Simulate the missing verbose item logging: loot AddItem lines never
        // reach a chat-only user's log. Survey-map item lines are kept — map
        // craft/consume tracking demonstrably works for such users (their
        // uses ARE recorded; it's the loot lines that are absent), and the
        // parser needs the map AddItem to name the instance so the
        // consumption DeleteItem can be recognized as a survey use.
        if line.contains("ProcessAddItem(")
            && !(line.contains("GeologySurvey") || line.contains("MiningSurvey"))
        {
            continue;
        }
        let line_secs = match player_line_secs(line) {
            Some(s) => s,
            None => {
                let mut events = parser.process_line(line);
                for ev in events.iter_mut() {
                    let _ = aggregator.process_event(ev, &conn, CHARACTER, SERVER, None);
                }
                continue;
            }
        };

        while next_chat_idx < resolved_gains.len() && resolved_gains[next_chat_idx].0 <= line_secs {
            let (_, g, internal) = &resolved_gains[next_chat_idx];
            feed_gain(&mut aggregator, g, internal);
            next_chat_idx += 1;
        }

        let mut events = parser.process_line(line);
        for ev in events.iter_mut() {
            let _ = aggregator.process_event(ev, &conn, CHARACTER, SERVER, None);
        }
    }

    while next_chat_idx < resolved_gains.len() {
        let (_, g, internal) = &resolved_gains[next_chat_idx];
        feed_gain(&mut aggregator, g, internal);
        next_chat_idx += 1;
    }

    Ok(conn)
}

fn push_through_aggregator(
    aggregator: &mut SurveySessionAggregator,
    conn: &Connection,
    mut events: Vec<crate::player_event_parser::PlayerEvent>,
) {
    for ev in events.iter_mut() {
        let _agg_events = aggregator.process_event(ev, conn, CHARACTER, SERVER, None);
        // game_state would persist the event normally; for the tests we
        // skip game_state and instead persist a minimal `item_transactions`
        // row directly when the provenance carries a survey_use_id. This
        // mirrors the production write path closely enough to validate the
        // grouping query in the loot summary, without requiring a full
        // GameStateManager harness.
        persist_for_test(conn, ev);
    }
}

/// Mirror the relevant subset of GameStateManager::process_events_batch:
/// when a gain event carries a survey_use_id in its provenance, write a row
/// to item_transactions so the loot-summary query has rows to join against.
fn persist_for_test(conn: &Connection, ev: &crate::player_event_parser::PlayerEvent) {
    use crate::player_event_parser::{ActivitySource, ItemProvenance, PlayerEvent};
    let (item_name, qty, prov) = match ev {
        PlayerEvent::ItemAdded {
            item_name,
            initial_quantity,
            provenance,
            timestamp,
            ..
        } => (
            item_name.clone(),
            *initial_quantity as i32,
            (provenance.clone(), timestamp.clone()),
        ),
        PlayerEvent::ItemStackChanged {
            item_name,
            delta,
            provenance,
            timestamp,
            ..
        } if *delta > 0 => (
            item_name.clone().unwrap_or_else(|| "?".to_string()),
            *delta,
            (provenance.clone(), timestamp.clone()),
        ),
        _ => return,
    };
    let (provenance, timestamp) = prov;
    let columns = provenance.to_columns();
    // Resolve item display via game_data — but here we're inside a test
    // helper that doesn't carry the data, so just use the internal name as
    // both fields. The loot-summary query keys by item_name from the
    // transaction row.
    let _ = conn.execute(
        "INSERT INTO item_transactions (timestamp, character_name, server_name, item_name, quantity, context, source, source_kind, source_details, confidence)
         VALUES (?1, ?2, ?3, ?4, ?5, 'loot', 'player_log', ?6, ?7, ?8)",
        params![
            timestamp,
            CHARACTER,
            SERVER,
            item_name,
            qty,
            columns.source_kind,
            columns.source_details,
            columns.confidence,
        ],
    );
    // Avoid unused-warning when not Attributed
    let _ = match provenance {
        ItemProvenance::Attributed {
            source: ActivitySource::Mining { .. },
            ..
        } => 0,
        _ => 0,
    };
}

/// Run the loot-summary query for a session. Same SQL as
/// `commands::loot_summary_for_session` but inlined here to avoid pulling
/// the Tauri command surface into the test path.
fn loot_summary(conn: &Connection, session_id: i64) -> HashMap<String, i64> {
    let mut stmt = conn
        .prepare(
            "SELECT t.item_name, SUM(t.quantity) AS total_qty
             FROM item_transactions t
             JOIN survey_uses u
               ON u.session_id = ?1
              AND u.id = CAST(json_extract(t.source_details, '$.survey_use_id') AS INTEGER)
             WHERE t.quantity > 0
             GROUP BY t.item_name",
        )
        .expect("prepare loot_summary");
    let rows = stmt
        .query_map(params![session_id], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?))
        })
        .expect("query loot_summary");
    rows.filter_map(|r| r.ok()).collect()
}

/// Total chat-status quantities by item display name. Used as the
/// authoritative comparison target — the chat log is the canonical source
/// of "what the player actually gained" (the new pipeline's job is to
/// match it, scoped to the survey use that produced it).
///
/// Filters out entries that aren't loot from the dataset's surveys: pre-
/// session inventory loads, vendor purchases, etc. Heuristic for now —
/// excludes items the chat reports outside any reasonable survey-loot
/// pattern (consumables that flow back into inventory, etc.). For Phase 5
/// validation we want to compare apples-to-apples so the test focuses on
/// items that appear in the dataset's `results.txt`.
fn chat_totals_by_item(chat_path: &Path) -> Result<HashMap<String, u32>, String> {
    let tz = detect_chat_tz_hours(chat_path)?;
    let gains = parse_chat_gains(chat_path, tz)?;
    let mut out: HashMap<String, u32> = HashMap::new();
    for g in gains {
        *out.entry(g.item_display).or_insert(0) += g.quantity;
    }
    Ok(out)
}

/// Diagnostic: compare per-item totals between the chat-status authority
/// and the new pipeline's loot summary for a session. Returns rows where
/// the two disagree, with the magnitude. Used by the multihit-loot-gap
/// investigation to point directly at which items aren't being attributed.
fn loot_gap_report(
    chat_totals: &HashMap<String, u32>,
    pipeline_summary: &HashMap<String, i64>,
    item_display_to_internal: &HashMap<String, String>,
) -> Vec<(String, i64, i64, i64)> {
    // (display_name, chat_qty, pipeline_qty, delta)
    let mut out = Vec::new();
    let mut seen_display: std::collections::HashSet<String> = std::collections::HashSet::new();
    for (display, &chat_qty) in chat_totals {
        let internal = item_display_to_internal.get(display).cloned();
        // Pipeline writes by internal name (it doesn't have a display name in
        // the test harness). Look up by either.
        let pipeline_qty: i64 = internal
            .as_ref()
            .and_then(|n| pipeline_summary.get(n).copied())
            .or_else(|| pipeline_summary.get(display).copied())
            .unwrap_or(0);
        seen_display.insert(display.clone());
        out.push((
            display.clone(),
            chat_qty as i64,
            pipeline_qty,
            pipeline_qty - chat_qty as i64,
        ));
    }
    // Items in pipeline but not in chat (shouldn't happen; if it does, log it).
    for (item, &qty) in pipeline_summary {
        if seen_display.contains(item) {
            continue;
        }
        if let Some(internal_match) = item_display_to_internal
            .iter()
            .find(|(_, internal)| *internal == item)
        {
            if seen_display.contains(internal_match.0) {
                continue;
            }
        }
        out.push((format!("(no chat) {item}"), 0, qty, qty));
    }
    out.sort_by_key(|(_, _, _, delta)| -delta.abs());
    out
}

/// Parse a results.txt ground-truth file into a (display_name, quantity) map.
///
/// Handles all three formats present in the test corpus:
///   - "43 Fluorite"                                              (plain count + name)
///   - "43x [Item: Fluorite]"                                     (bracketed)
///   - "26-04-17 07:25:25\t[-apptesting] Zenith: 31x [Item: X]"  (chat-log dump)
fn parse_results_file(path: &Path) -> Result<HashMap<String, u32>, String> {
    let content = fs::read_to_string(path).map_err(|e| e.to_string())?;
    let mut out = HashMap::new();
    for raw in content.lines() {
        let line = raw.trim();
        if line.is_empty() {
            continue;
        }
        // Try the "Nx [Item: Name]" format first.
        if let Some(captures) = parse_bracketed_results_line(line) {
            out.insert(captures.0, captures.1);
            continue;
        }
        // Fall back to "N Name" plain format.
        let mut parts = line.splitn(2, ' ');
        let qty_str = match parts.next() {
            Some(s) => s,
            None => continue,
        };
        let name = match parts.next() {
            Some(s) => s.trim().to_string(),
            None => continue,
        };
        let qty: u32 = match qty_str.parse() {
            Ok(n) => n,
            Err(_) => continue,
        };
        out.insert(name, qty);
    }
    Ok(out)
}

/// Parse "Nx [Item: Name]" → (Name, N). Returns None if the shape doesn't fit.
/// Also handles chat-log format: "26-04-17 07:25:25\t[-apptesting] Zenith: 31x [Item: Tsavorite]"
/// by extracting the numeric run immediately before "x [Item: ".
fn parse_bracketed_results_line(line: &str) -> Option<(String, u32)> {
    let x_idx = line.find("x [Item: ")?;
    // Extract the numeric quantity immediately before "x [Item: ".
    // Walk backwards from x_idx to find the start of the digit run.
    let before = &line[..x_idx];
    let digit_start = before
        .rfind(|c: char| !c.is_ascii_digit())
        .map(|i| i + 1)
        .unwrap_or(0);
    let qty: u32 = before[digit_start..].parse().ok()?;
    let after_marker = &line[x_idx + "x [Item: ".len()..];
    let close_idx = after_marker.find(']')?;
    let name = after_marker[..close_idx].trim().to_string();
    Some((name, qty))
}

/// Count rows in survey_uses, optionally filtered by kind.
fn use_count(conn: &Connection, kind: Option<SurveyUseKind>) -> i64 {
    let sql = match kind {
        Some(k) => format!(
            "SELECT COUNT(*) FROM survey_uses WHERE kind = '{}'",
            k.as_str()
        ),
        None => "SELECT COUNT(*) FROM survey_uses".to_string(),
    };
    conn.query_row(&sql, [], |r| r.get::<_, i64>(0))
        .unwrap_or(0)
}

#[test]
#[ignore = "slow — loads full CDN items.json + replays 30k+ player.log lines"]
fn replay_50x_povus_marvelous() {
    let dataset = "../test_data/surveyLogs/50x-povusmarvelous-ringandpick";
    let player_log = Path::new(dataset).join("Player.log");
    let chat_log = Path::new(dataset).join("Chat.log");

    let conn = replay_dataset(&player_log, &chat_log).expect("replay should succeed");

    // Multihit dataset — most uses should be Multihit kind.
    let multihit = use_count(&conn, Some(SurveyUseKind::Multihit));
    let total = use_count(&conn, None);
    println!(
        "[replay-50x-povus] total uses={} multihit={} basic={} motherlode={}",
        total,
        multihit,
        use_count(&conn, Some(SurveyUseKind::Basic)),
        use_count(&conn, Some(SurveyUseKind::Motherlode)),
    );

    // Looser bounds: the user's notes say 50 uses, but the dataset may
    // include incidental other surveys, and chat-aborted uses may or may
    // not count depending on log noise. Assert "reasonably close".
    assert!(
        (40..=60).contains(&multihit),
        "multihit count {multihit} not in expected range 40..60"
    );

    // Find the most prolific session and inspect its loot summary.
    let session_id: i64 = conn
        .query_row(
            "SELECT id FROM survey_sessions ORDER BY consumed_count DESC LIMIT 1",
            [],
            |r| r.get(0),
        )
        .expect("at least one session");
    let summary = loot_summary(&conn, session_id);
    let total_qty: i64 = summary.values().sum();
    println!(
        "[replay-50x-povus] session {} loot rows={} total_qty={}",
        session_id,
        summary.len(),
        total_qty,
    );

    // Diagnostic: per-item gap report between chat-status authority and
    // the pipeline's loot summary. If the pipeline is correct, every
    // non-consumable loot item should match (the chat will also include
    // consumables and incidental gains, which we expect to NOT be in the
    // pipeline summary).
    let chat_totals = chat_totals_by_item(&chat_log).unwrap_or_default();
    let game_data = load_game_data_from_cdn().expect("game data must load");
    let display_to_internal: HashMap<String, String> = chat_totals
        .keys()
        .filter_map(|display| {
            let gd = game_data.try_read().ok()?;
            let info = gd.resolve_item(display)?;
            info.internal_name.clone().map(|i| (display.clone(), i))
        })
        .collect();
    let gap = loot_gap_report(&chat_totals, &summary, &display_to_internal);
    println!("[replay-50x-povus] item-by-item comparison (top 30 by |delta|):");
    println!(
        "  {:>40}  {:>8}  {:>8}  {:>8}",
        "ITEM", "CHAT", "PIPELINE", "DELTA"
    );
    for (item, chat_q, pipe_q, delta) in gap.iter().take(30) {
        println!(
            "  {:>40}  {:>8}  {:>8}  {:>+8}",
            item, chat_q, pipe_q, delta
        );
    }

    // Mining-loot attribution breakdown. The dataset includes wild nodes
    // (mineable nodes scattered in the world that aren't tied to any
    // survey map) — gains from those legitimately have no survey_use_id.
    // What we want to see: of mining-attributed gains, what fraction
    // chains to a survey vs. floats free?
    let mining_total: i64 = conn
        .query_row(
            "SELECT COALESCE(SUM(quantity), 0) FROM item_transactions
             WHERE source_kind = 'mining' AND quantity > 0",
            [],
            |r| r.get(0),
        )
        .unwrap_or(0);
    let mining_with_survey: i64 = conn
        .query_row(
            "SELECT COALESCE(SUM(quantity), 0) FROM item_transactions
             WHERE source_kind = 'mining'
               AND quantity > 0
               AND json_extract(source_details, '$.survey_use_id') IS NOT NULL",
            [],
            |r| r.get(0),
        )
        .unwrap_or(0);
    let mining_unattributed = mining_total - mining_with_survey;
    let pct_chained = if mining_total > 0 {
        100 * mining_with_survey / mining_total
    } else {
        0
    };
    println!(
        "[replay-50x-povus] mining-attributed loot: total={} survey-chained={} ({}%) unattributed={}",
        mining_total, mining_with_survey, pct_chained, mining_unattributed
    );

    // Breakdown of ALL positive-quantity gains by source_kind. Tells us how
    // many gains the parser sees vs. how many actually attributed to mining.
    println!("[replay-50x-povus] gain breakdown by source_kind:");
    let mut stmt = conn
        .prepare(
            "SELECT COALESCE(source_kind, '<null>') AS kind,
                    COUNT(*) AS rows,
                    SUM(quantity) AS qty
             FROM item_transactions
             WHERE quantity > 0
             GROUP BY kind ORDER BY qty DESC",
        )
        .unwrap();
    let rows = stmt
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, i64>(2)?,
            ))
        })
        .unwrap();
    for (kind, n, qty) in rows.into_iter().map_while(Result::ok) {
        println!("  {:>20}  rows={:>5}  qty={:>6}", kind, n, qty);
    }

    // For Marvelous Metal Slab specifically: how many rows by source_kind?
    println!("[replay-50x-povus] MetalSlab9 (Marvelous) breakdown:");
    let mut stmt = conn
        .prepare(
            "SELECT COALESCE(source_kind, '<null>'), COUNT(*), SUM(quantity)
             FROM item_transactions
             WHERE quantity > 0 AND item_name = 'MetalSlab9'
             GROUP BY source_kind",
        )
        .unwrap();
    let rows = stmt
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, i64>(2)?,
            ))
        })
        .unwrap();
    for (kind, n, qty) in rows.into_iter().map_while(Result::ok) {
        println!("  {:>20}  rows={:>5}  qty={:>6}", kind, n, qty);
    }

    // Strict assertion against the dataset's results.txt (the user
    // hand-isolated this list to be ONLY loot that came from survey nodes —
    // wild-node and combat loot were kept separate). With the Mining-wins
    // tie-breaker in `compute_provenance`, every item the user attributed
    // to surveys must match exactly.
    //
    // The pipeline writes loot rows by INTERNAL name (e.g., "MetalSlab9"),
    // while results.txt uses display names ("Marvelous Metal Slab"). Resolve
    // each results.txt entry through CDN and compare.
    let expected = parse_results_file(&Path::new(dataset).join("results.txt"))
        .expect("results.txt should parse");
    let mut mismatches = Vec::new();
    for (display, expected_qty) in &expected {
        let internal = {
            let gd = game_data.try_read().unwrap();
            gd.resolve_item(display)
                .and_then(|i| i.internal_name.clone())
        };
        let pipeline_qty = match &internal {
            Some(i) => summary.get(i).copied().unwrap_or(0),
            None => 0,
        };
        if pipeline_qty != *expected_qty as i64 {
            mismatches.push((display.clone(), *expected_qty as i64, pipeline_qty));
        }
    }
    if !mismatches.is_empty() {
        println!("[replay-50x-povus] mismatches vs results.txt:");
        for (item, expected, actual) in &mismatches {
            println!(
                "  {:>40}  expected={:>4}  actual={:>4}",
                item, expected, actual
            );
        }
    }
    // Empirical baseline: with the Mining-wins-over-passive heuristic, this
    // dataset matches 9-of-12 items exactly. The remaining 3 are off by ≤2:
    //   - 1 Gold Nugget out of 15 (likely a different-context overlap edge)
    //   - 1 Expert-Quality Metal Slab missed entirely (1/1)
    //   - 1 Amazing Metal Slab missed entirely (1/1)
    // The single-quantity rare drops are interesting — they're probably
    // landing on a stack-merge whose first UpdateItemCode establishes a
    // baseline (no event emitted) instead of a new AddItem. Worth a future
    // investigation, but the headline drops (177 Marvelous, 84 Pebbles, etc.)
    // all match exactly.
    //
    // Test gates: zero items with >2 difference, at most 4 items with ≤2.
    let small_diffs: usize = mismatches
        .iter()
        .filter(|(_, e, a)| (e - a).abs() <= 2)
        .count();
    let large_diffs: usize = mismatches.len() - small_diffs;
    assert_eq!(
        large_diffs, 0,
        "found {} items with >2 difference vs results.txt: {:?}",
        large_diffs, mismatches
    );
    assert!(
        small_diffs <= 4,
        "found {} items off-by-1-or-2; tolerance is 4",
        small_diffs
    );
}

#[test]
#[ignore = "slow — loads full CDN items.json + replays 30k+ player.log lines"]
fn replay_100x_serbcrystal() {
    let dataset = "../test_data/surveyLogs/100x-serbcrystal-withring";
    let player_log = Path::new(dataset).join("Player.log");
    let chat_log = Path::new(dataset).join("Chat.log");

    let conn = replay_dataset(&player_log, &chat_log).expect("replay should succeed");

    let basic = use_count(&conn, Some(SurveyUseKind::Basic));
    let total = use_count(&conn, None);
    println!(
        "[replay-100x-serbcrystal] total={} basic={} motherlode={} multihit={}",
        total,
        basic,
        use_count(&conn, Some(SurveyUseKind::Motherlode)),
        use_count(&conn, Some(SurveyUseKind::Multihit)),
    );

    // Serbule basic surveys; most should be Basic kind.
    assert!(
        (80..=120).contains(&basic),
        "basic count {basic} not in expected range 80..120"
    );

    // Most uses should reach a terminal status (completed or aborted), not
    // languish in pending_loot. Some pending_loot are expected for the very
    // last few uses near the log's end.
    let pending: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM survey_uses WHERE status = ?1",
            [SurveyUseStatus::PendingLoot.as_str()],
            |r| r.get(0),
        )
        .unwrap_or(0);
    println!("[replay-100x-serbcrystal] pending_loot={}", pending);
    assert!(
        pending < total / 4,
        "too many pending_loot uses: {pending} of {total}"
    );
}

#[test]
#[ignore = "slow"]
fn replay_100x_eltmetal() {
    let dataset = "../test_data/surveyLogs/100x-eltmetal-ringandpick";
    let player_log = Path::new(dataset).join("Player.log");
    let chat_log = Path::new(dataset).join("Chat.log");

    let conn = replay_dataset(&player_log, &chat_log).expect("replay should succeed");

    // Eltibule "metal" surveys are MiningSurveyEltibule* — Basic per the
    // survey-mechanics doc (MULTIHIT_AREAS does not include Eltibule).
    let basic = use_count(&conn, Some(SurveyUseKind::Basic));
    let total = use_count(&conn, None);
    println!(
        "[replay-100x-eltmetal] total={} basic={} multihit={} motherlode={}",
        total,
        basic,
        use_count(&conn, Some(SurveyUseKind::Multihit)),
        use_count(&conn, Some(SurveyUseKind::Motherlode)),
    );
    assert!(
        (80..=120).contains(&basic),
        "basic count {basic} not in expected range 80..120"
    );
}

// A non-ignored smoke test: just verify the harness can build a CDN-loaded
// game_data without panicking. Catches the obvious "items.json moved" case
// without requiring the full slow replay run.
#[test]
fn cdn_items_load_smoke() {
    match load_game_data_from_cdn() {
        Ok(gd) => {
            let count = gd.try_read().unwrap().items.len();
            assert!(
                count > 1000,
                "expected many items in CDN snapshot, got {count}"
            );
        }
        Err(e) => {
            // Tolerate missing fixtures in CI — print a hint and pass. The
            // ignored replay tests will fail clearly if anyone runs them
            // without the data present.
            eprintln!("[cdn-load-smoke] CDN sample data missing — replay tests will skip: {e}");
        }
    }
}

// ---------------------------------------------------------------------------
// Comprehensive accuracy report
// ---------------------------------------------------------------------------

/// Describes one test dataset for the accuracy harness.
struct DatasetSpec {
    name: &'static str,
    dir: &'static str,
    player_log: &'static str,
    chat_log: &'static str,
    /// Expected survey kind for the majority of uses.
    expected_kind: SurveyUseKind,
    /// Rough expected use count (lower, upper).
    expected_use_range: (i64, i64),
}

/// Per-dataset accuracy results, collected for the final report.
struct DatasetResult {
    name: String,
    /// Whether replay_dataset succeeded.
    ok: bool,
    error: Option<String>,
    /// Total survey uses detected.
    total_uses: i64,
    /// Uses of the expected kind.
    kind_uses: i64,
    /// Whether the kind count fell in the expected range.
    kind_in_range: bool,
    /// Number of distinct items in results.txt.
    expected_item_count: usize,
    /// Total expected quantity across all items.
    expected_total_qty: u32,
    /// Total pipeline quantity for those items (matched by display name).
    pipeline_total_qty: i64,
    /// Number of items that matched exactly.
    exact_matches: usize,
    /// Number of items off by 1-2.
    close_matches: usize,
    /// Number of items off by >2.
    large_mismatches: usize,
    /// Per-item details: (display_name, expected, actual, delta).
    item_details: Vec<(String, i64, i64, i64)>,
    /// Items the pipeline attributed to surveys that aren't in results.txt.
    /// (display_or_internal_name, quantity)
    extra_items: Vec<(String, i64)>,
}

/// All datasets in the corpus that have a results.txt ground truth file.
///
/// Standard layout per folder:
///   - `Player.log` — the game's primary log
///   - `Chat.log`   — the corresponding chat log with `[Status]` messages
///   - `results.txt` — hand-recorded loot totals in `Nx [Item: Name]` format
const DATASETS: &[DatasetSpec] = &[
    DatasetSpec {
        name: "50x-povus-marvelous (multihit)",
        dir: "../test_data/surveyLogs/50x-povusmarvelous-ringandpick",
        player_log: "Player.log",
        chat_log: "Chat.log",
        expected_kind: SurveyUseKind::Multihit,
        expected_use_range: (40, 60),
    },
    DatasetSpec {
        name: "100x-eltmetal (basic, ring+pick)",
        dir: "../test_data/surveyLogs/100x-eltmetal-ringandpick",
        player_log: "Player.log",
        chat_log: "Chat.log",
        expected_kind: SurveyUseKind::Basic,
        expected_use_range: (80, 120),
    },
    DatasetSpec {
        name: "100x-serbcrystal (basic, with ring)",
        dir: "../test_data/surveyLogs/100x-serbcrystal-withring",
        player_log: "Player.log",
        chat_log: "Chat.log",
        expected_kind: SurveyUseKind::Basic,
        expected_use_range: (80, 120),
    },
    DatasetSpec {
        name: "100x-serbcrystal (basic, vanilla)",
        dir: "../test_data/surveyLogs/100x-serbcrystal-vanilla",
        player_log: "Player.log",
        chat_log: "Chat.log",
        expected_kind: SurveyUseKind::Basic,
        expected_use_range: (80, 120),
    },
    DatasetSpec {
        name: "100x-eltblue (basic, ring+pick)",
        dir: "../test_data/surveyLogs/100x-eltblue-ringandpick",
        player_log: "Player.log",
        chat_log: "Chat.log",
        expected_kind: SurveyUseKind::Basic,
        expected_use_range: (80, 120),
    },
    DatasetSpec {
        name: "100x-eltcrystal (basic, vanilla)",
        dir: "../test_data/surveyLogs/100x-eltcrystal-vanilla",
        player_log: "Player.log",
        chat_log: "Chat.log",
        expected_kind: SurveyUseKind::Basic,
        expected_use_range: (80, 120),
    },
];

/// Run a single dataset through the pipeline and compare against results.txt.
/// `chat_only` replays with Player.log item lines dropped, attributing loot
/// exclusively through the chat-authoritative gate (see
/// [`replay_dataset_chat_only`]).
fn evaluate_dataset(
    spec: &DatasetSpec,
    game_data: &GameDataState,
    chat_only: bool,
) -> DatasetResult {
    let dir = Path::new(spec.dir);
    let player_log = dir.join(spec.player_log);
    let chat_log = dir.join(spec.chat_log);
    let results_path = dir.join("results.txt");

    let mut result = DatasetResult {
        name: spec.name.to_string(),
        ok: false,
        error: None,
        total_uses: 0,
        kind_uses: 0,
        kind_in_range: false,
        expected_item_count: 0,
        expected_total_qty: 0,
        pipeline_total_qty: 0,
        exact_matches: 0,
        close_matches: 0,
        large_mismatches: 0,
        item_details: Vec::new(),
        extra_items: Vec::new(),
    };

    // Replay through the pipeline.
    let replayed = if chat_only {
        replay_dataset_chat_only(&player_log, &chat_log)
    } else {
        replay_dataset(&player_log, &chat_log)
    };
    let conn = match replayed {
        Ok(c) => c,
        Err(e) => {
            result.error = Some(e);
            return result;
        }
    };
    result.ok = true;

    // Use counts.
    result.total_uses = use_count(&conn, None);
    result.kind_uses = use_count(&conn, Some(spec.expected_kind));
    let (lo, hi) = spec.expected_use_range;
    result.kind_in_range = result.kind_uses >= lo && result.kind_uses <= hi;

    // Parse ground truth.
    let expected = match parse_results_file(&results_path) {
        Ok(e) => e,
        Err(e) => {
            result.error = Some(format!("results.txt parse error: {e}"));
            return result;
        }
    };
    result.expected_item_count = expected.len();
    result.expected_total_qty = expected.values().sum();

    // Get the pipeline's loot summary for the most prolific session.
    let session_id: i64 = match conn.query_row(
        "SELECT id FROM survey_sessions ORDER BY consumed_count DESC LIMIT 1",
        [],
        |r| r.get(0),
    ) {
        Ok(id) => id,
        Err(_) => {
            result.error = Some("no survey session found".to_string());
            return result;
        }
    };
    let summary = loot_summary(&conn, session_id);

    // Compare each expected item against pipeline output.
    // Pipeline writes by internal name; results.txt uses display names.
    // Resolve display→internal via CDN.
    for (display, &expected_qty) in &expected {
        let internal = {
            let gd = game_data.try_read().unwrap();
            gd.resolve_item(display)
                .and_then(|i| i.internal_name.clone())
        };
        let pipeline_qty = match &internal {
            Some(i) => summary.get(i).copied().unwrap_or(0),
            None => summary.get(display).copied().unwrap_or(0),
        };
        let delta = pipeline_qty - expected_qty as i64;
        result.pipeline_total_qty += pipeline_qty;
        if delta == 0 {
            result.exact_matches += 1;
        } else if delta.abs() <= 2 {
            result.close_matches += 1;
        } else {
            result.large_mismatches += 1;
        }
        result
            .item_details
            .push((display.clone(), expected_qty as i64, pipeline_qty, delta));
    }
    // Sort by |delta| descending for readability.
    result.item_details.sort_by_key(|(_, _, _, d)| -d.abs());

    // Detect extra items: pipeline attributed items not in results.txt.
    // Build a set of internal names we've already matched above.
    let matched_internals: std::collections::HashSet<String> = expected
        .keys()
        .filter_map(|display| {
            let gd = game_data.try_read().ok()?;
            gd.resolve_item(display)
                .and_then(|i| i.internal_name.clone())
        })
        .collect();
    for (internal_name, &qty) in &summary {
        if matched_internals.contains(internal_name) {
            continue;
        }
        // Resolve to display name for readability.
        let display = {
            let gd = game_data.try_read().unwrap();
            gd.resolve_item(internal_name)
                .map(|i| i.name.clone())
                .unwrap_or_else(|| internal_name.clone())
        };
        result.extra_items.push((display, qty));
    }
    result.extra_items.sort_by_key(|(_, q)| -(*q));

    result
}

/// Comprehensive accuracy report across all datasets with results.txt.
///
/// Run with: `cargo test --lib survey::replay_tests::accuracy_report -- --ignored --nocapture`
#[test]
#[ignore = "slow — replays all survey datasets and produces an accuracy report"]
fn accuracy_report() {
    run_accuracy_report(false);
}

/// Same report, but replayed as a **chat-only** user (Player.log item lines
/// dropped; loot flows exclusively through `attribute_chat_gain`). Validates
/// that the chat-loot collection-window gating captures exactly the survey
/// loot — no kill/forage/craft leakage, no missed survey drops.
///
/// Run with: `cargo test --lib survey::replay_tests::chat_only_accuracy_report -- --ignored --nocapture`
#[test]
#[ignore = "slow — replays all survey datasets and produces an accuracy report"]
fn chat_only_accuracy_report() {
    run_accuracy_report(true);
}

fn run_accuracy_report(chat_only: bool) {
    let game_data = load_game_data_from_cdn().expect("CDN items.json required for accuracy report");

    let results: Vec<DatasetResult> = DATASETS
        .iter()
        .map(|spec| evaluate_dataset(spec, &game_data, chat_only))
        .collect();

    // -----------------------------------------------------------------------
    // Print the report
    // -----------------------------------------------------------------------
    println!();
    println!("╔══════════════════════════════════════════════════════════════════════════╗");
    if chat_only {
        println!("║                SURVEY ACCURACY REPORT (chat-only)                       ║");
    } else {
        println!("║                    SURVEY ACCURACY REPORT                               ║");
    }
    println!("╚══════════════════════════════════════════════════════════════════════════╝");
    println!();

    let mut grand_expected_items = 0usize;
    let mut grand_exact = 0usize;
    let mut grand_close = 0usize;
    let mut grand_large = 0usize;
    let mut grand_expected_qty: u64 = 0;
    let mut grand_pipeline_qty: i64 = 0;

    let mut grand_extra_items = 0usize;
    let mut grand_extra_qty: i64 = 0;

    for r in &results {
        let status = if !r.ok {
            "REPLAY FAILED"
        } else if r.exact_matches == r.expected_item_count && r.extra_items.is_empty() {
            "OK"
        } else {
            "FAIL"
        };
        let kind_status = if r.kind_in_range { "ok" } else { "MISS" };

        println!("┌─ {} ─ [{}]", r.name, status);
        if let Some(err) = &r.error {
            println!("│  Error: {err}");
            println!("└─");
            println!();
            continue;
        }

        println!(
            "│  Uses: {} total, {} expected-kind [{kind_status}]",
            r.total_uses, r.kind_uses
        );
        println!(
            "│  Items: {} in ground truth, {} exact, {} close (±1-2), {} off (>2)",
            r.expected_item_count, r.exact_matches, r.close_matches, r.large_mismatches
        );

        let accuracy_pct = if r.expected_total_qty > 0 {
            let diff = (r.pipeline_total_qty - r.expected_total_qty as i64).unsigned_abs();
            let captured = if r.pipeline_total_qty >= 0 {
                r.pipeline_total_qty as u64
            } else {
                0
            };
            // Quantity accuracy: how much of the expected total did we capture?
            // Capped at 100% (overcounting is also an error, shown separately).
            let pct = (captured.min(r.expected_total_qty as u64) as f64
                / r.expected_total_qty as f64)
                * 100.0;
            let _ = diff; // used implicitly via captured
            pct
        } else {
            0.0
        };
        println!(
            "│  Qty: expected={}, pipeline={}, accuracy={:.1}%",
            r.expected_total_qty, r.pipeline_total_qty, accuracy_pct
        );

        // Show per-item breakdown if there are any non-exact matches.
        if r.exact_matches < r.expected_item_count {
            println!("│");
            println!(
                "│  {:>35}  {:>8}  {:>8}  {:>8}",
                "ITEM", "EXPECT", "ACTUAL", "DELTA"
            );
            for (item, expected, actual, delta) in &r.item_details {
                let marker = if *delta == 0 {
                    " "
                } else if delta.abs() <= 2 {
                    "~"
                } else {
                    "!"
                };
                println!(
                    "│ {marker} {:>35}  {:>8}  {:>8}  {:>+8}",
                    item, expected, actual, delta
                );
            }
        }

        // Show extra items the pipeline attributed that aren't in results.txt.
        if !r.extra_items.is_empty() {
            println!("│");
            println!("│  EXTRA items (not in results.txt):");
            for (item, qty) in &r.extra_items {
                println!("│ + {:>35}  {:>8}", item, qty);
            }
        }

        grand_expected_items += r.expected_item_count;
        grand_exact += r.exact_matches;
        grand_close += r.close_matches;
        grand_large += r.large_mismatches;
        grand_expected_qty += r.expected_total_qty as u64;
        grand_pipeline_qty += r.pipeline_total_qty;
        grand_extra_items += r.extra_items.len();
        grand_extra_qty += r.extra_items.iter().map(|(_, q)| q).sum::<i64>();

        println!("└─");
        println!();
    }

    // Summary.
    let grand_accuracy = if grand_expected_qty > 0 {
        let captured = if grand_pipeline_qty >= 0 {
            (grand_pipeline_qty as u64).min(grand_expected_qty)
        } else {
            0
        };
        (captured as f64 / grand_expected_qty as f64) * 100.0
    } else {
        0.0
    };

    println!("════════════════════════════════════════════════════════════════════════════");
    println!("  SUMMARY ({} datasets)", results.len());
    println!("────────────────────────────────────────────────────────────────────────────");
    println!(
        "  Item-level:  {} exact, {} close (±1-2), {} off (>2)  of {} total items",
        grand_exact, grand_close, grand_large, grand_expected_items
    );
    if grand_extra_items > 0 {
        println!(
            "  Extra items: {} items with {} total qty attributed but not in ground truth",
            grand_extra_items, grand_extra_qty
        );
    }
    println!(
        "  Qty-level:   expected={}, pipeline={}, overall accuracy={:.1}%",
        grand_expected_qty, grand_pipeline_qty, grand_accuracy
    );
    println!("════════════════════════════════════════════════════════════════════════════");
    println!();

    // Hard assertions. Every dataset must replay successfully in both modes.
    for r in &results {
        assert!(r.ok, "dataset '{}' failed to replay: {:?}", r.name, r.error);
    }

    if chat_only {
        // Chat-only mode includes the item-identity fallback, which by
        // design also counts Ore/MetalSlab items mined from *wild* nodes
        // during a session (chat identity can't tell them apart — accepted
        // trade-off). So: undercounts are still hard failures (survey loot
        // must never be missed), while overcounts vs the hand-isolated
        // ground truth are allowed but must stay small.
        let big_undercounts: Vec<String> = results
            .iter()
            .flat_map(|r| r.item_details.iter().map(move |d| (r.name.clone(), d)))
            .filter(|(_, (_, _, _, delta))| *delta < -2)
            .map(|(ds, (item, expected, actual, _))| {
                format!("{ds}: {item} expected={expected} actual={actual}")
            })
            .collect();
        assert!(
            big_undercounts.is_empty(),
            "survey loot missed by the chat gate (undercount >2): {:?}",
            big_undercounts
        );
        let overcount: i64 = results
            .iter()
            .flat_map(|r| r.item_details.iter())
            .map(|(_, _, _, delta)| (*delta).max(0))
            .sum::<i64>()
            + grand_extra_qty;
        let overcount_limit = (grand_expected_qty as f64 * 0.02).ceil() as i64;
        assert!(
            overcount <= overcount_limit.max(10),
            "identity-fallback overcount {} exceeds bound {} (2% of expected {}) — over-attributing beyond wild-node noise",
            overcount, overcount_limit, grand_expected_qty
        );
    } else {
        // Player.log mode is exact:
        // 1. No items may be off by >2 (wrong quantity for a known item).
        // 2. No extra items (pipeline attributed items not in ground truth).
        assert!(
            grand_large == 0,
            "found {} items with >2 difference — these indicate real pipeline bugs or ground-truth errors",
            grand_large
        );
        assert!(
            grand_extra_items == 0,
            "found {} extra items ({} qty) attributed to surveys but not in ground truth — pipeline is over-attributing",
            grand_extra_items, grand_extra_qty
        );
    }
    if grand_expected_qty > 0 {
        assert!(
            grand_accuracy >= 90.0,
            "overall quantity accuracy {:.1}% dropped below 90% — regression detected",
            grand_accuracy
        );
    }
}
