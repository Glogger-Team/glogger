use super::DbPool;
use crate::books_watcher::rescan_books_once;
use crate::settings::SettingsManager;
use crate::shop_log_parser::{parse_shop_log, ShopLog};
use crate::stall_aggregations::{
    aggregate_revenue, aggregate_sales_timeseries, BaselineSeed, Granularity,
    InventoryEvent, InventoryResult, RevenueEvent, RevenueResult, SalesTimeseriesResult,
    TimeseriesEvent, TimeseriesMode, TimeseriesPeriod,
};
use chrono::{Datelike, Local};
use rusqlite::types::Value;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;
use tauri::{AppHandle, Manager, State};

/// Input shape for a single stall event, built by the coordinator from the
/// shop log parser's output.
///
/// `owner` is `Option<String>` to keep the same struct usable by both the
/// live-ingest path (which always stamps `Some(active_character)`) and the
/// future Import command (which may pass the parser's advisory owner hint
/// or fall back to the caller-supplied current owner). In the live path,
/// `None` is never a valid input — the coordinator early-returns if no
/// active character is set.
#[derive(Debug, Clone)]
pub struct StallEventInput {
    pub event_timestamp: String,
    pub event_at: Option<String>,
    pub log_timestamp: String,
    pub log_title: String,
    pub action: String,
    pub player: String,
    pub owner: Option<String>,
    pub item: Option<String>,
    pub quantity: i64,
    pub price_unit: Option<f64>,
    pub price_total: Option<i64>,
    pub raw_message: String,
    pub entry_index: i64,
}

/// Bulk-insert stall events, de-duplicated per logical log line occurrence
/// witness. Identity is `(event_timestamp, raw_message)` with a multiplicity
/// rule: per line, the incoming batch becomes the witness iff its occurrence
/// count is >= the max occurrences any single stored batch holds (batch =
/// `log_title`/`log_timestamp`; ties go to the newest batch). A witness
/// re-ingest replaces the line's stored rows outright (DELETE + INSERT in
/// this same transaction, so a full-overlap re-ingest never grows the row
/// count and returns 0 net-new) — INCLUDING the incoming batch's own old
/// rows, so a same-batch re-process at shifted `entry_index` (sliding
/// window) self-heals instead of re-adding every line; a PARTIAL re-ingest
/// carrying fewer copies inserts and deletes nothing — its occurrences are
/// already witnessed, and a partial overlap must never delete real events.
/// Genuine same-minute repeats still insert: first sighting has no stored
/// batch, and a book holding MORE copies than stored becomes the new witness
/// (Deradon's 8x same-minute bulk buys are 8 real occurrences, not a
/// re-ingest). Witness replacement mutes the whole replacement set when any
/// removed row was muted (per-group OR, errs toward muted and reversible
/// via the per-row toggle) — positional carry is unsound across several
/// losing batches.
/// Only `QueryReturnedNoRows` means "no stored batch" — any other witness
/// lookup error aborts instead of deleting on a failed read.
/// Clear-vs-insert serialization comes from the single-write pool plus the
/// `BEGIN IMMEDIATE` transaction below: Clear is a single `DELETE` on the
/// same pool, so SQLite serializes the two statements atomically.
pub fn insert_stall_events(db: &DbPool, events: &[StallEventInput]) -> Result<usize, String> {
    if events.is_empty() {
        return Ok(0);
    }

    let mut conn = db
        .get_write()
        .map_err(|e| format!("Database connection error: {e}"))?;

    let tx = conn
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .map_err(|e| format!("Failed to begin transaction: {e}"))?;

    // Re-ingest guard (fullest-witness rule). Per logical line
    // (event_timestamp, raw_message) WITHIN the incoming batch's owner,
    // count the incoming occurrences and the max occurrences any single
    // stored batch holds (batch = log_title/log_timestamp; one call ingests
    // one book/file, and every call site passes a single title+timestamp
    // per call — and a single owner per call, which scopes every witness
    // query below via NULL-safe `IS`: an identical `bought` line in Alt B's
    // book must never delete Alt A's real sale of the same line). If the
    // incoming batch is at least as full (ties -> newest wins), it becomes
    // the witness: replace the line's stored rows outright (delete first,
    // insert after — including the incoming batch's own old rows, so a
    // same-batch re-process at shifted indexes self-heals). If it carries
    // FEWER copies (partial overlap), its occurrences are already
    // witnessed — insert nothing, delete nothing. A
    // partial overlap must never delete real events. Genuine same-minute
    // repeats still insert: first sighting has no stored batch (0 < n), and
    // a book with MORE copies than stored becomes the new witness.
    // Lines whose incoming batch is fully witnessed (partial overlap: fewer
    // copies than the max stored batch holds) skip the INSERT entirely —
    // their occurrences exist, and the shifted entry_index would otherwise
    // re-add them as new rows under OR IGNORE.
    // Muted-line carry: when any row the witness DELETE removes was muted,
    // the replacements inherit the mute (whole-set OR, errs toward muted —
    // reversible via the per-row toggle). Positional mapping is unsound
    // across several losing batches, so the carry is a set flag, never an
    // occurrence-index zip: identical lines can be distinct events with
    // different mutes.
    // Deleted-row count for the net-new return below.
    let mut mute_carry: std::collections::BTreeMap<(String, String), Vec<bool>> =
        std::collections::BTreeMap::new();
    let mut deleted_total = 0usize;
    // Lines for which the witness DELETE actually removed one of THIS batch's
    // owner rows. The collision guard below aborts the whole call only for
    // those lines; first-sighting lines with nothing deleted keep the prior
    // single-row `INSERT OR IGNORE` skip instead of blocking the batch.
    let mut deleted_lines: std::collections::BTreeSet<(String, String)> =
        std::collections::BTreeSet::new();
    let witnessed: std::collections::BTreeSet<(String, String)> = {
        use std::collections::BTreeMap;
        let mut incoming_count: BTreeMap<(String, String), usize> = BTreeMap::new();
        for ev in events {
            *incoming_count
                .entry((ev.event_timestamp.clone(), ev.raw_message.clone()))
                .or_default() += 1;
        }
        // Every call site stamps one owner per batch; `owner IS ?` (NULL-safe,
        // never `=`) keeps legacy NULL-owner rows matchable.
        let batch_owner: Option<String> =
            events.iter().find_map(|e| e.owner.clone());
        let mut max_stored = tx
            .prepare(
                "SELECT COUNT(*) FROM stall_events
                 WHERE event_timestamp = ?1 AND raw_message = ?2 AND owner IS ?3
                 GROUP BY log_title, log_timestamp
                 ORDER BY COUNT(*) DESC LIMIT 1",
            )
            .map_err(|e| format!("Failed to prepare witness query: {e}"))?;
        let mut doomed_mutes = tx
            .prepare(
                "SELECT ignored FROM stall_events
                 WHERE event_timestamp = ?1 AND raw_message = ?2 AND owner IS ?3
                 ORDER BY id ASC",
            )
            .map_err(|e| format!("Failed to prepare mute snapshot: {e}"))?;
        let mut del = tx
            .prepare(
                "DELETE FROM stall_events
                 WHERE event_timestamp = ?1 AND raw_message = ?2 AND owner IS ?3",
            )
            .map_err(|e| format!("Failed to prepare re-ingest delete: {e}"))?;
        let mut witnessed = std::collections::BTreeSet::new();
        for ((ts, msg), n_in) in &incoming_count {
            let n_max: usize = match max_stored
                .query_row(rusqlite::params![ts, msg, batch_owner.clone()], |r| {
                    r.get::<_, i64>(0)
                })
            {
                Ok(n) => n as usize,
                // No stored batch: first sighting, witness below.
                Err(rusqlite::Error::QueryReturnedNoRows) => 0,
                Err(e) => return Err(format!("Witness query failed: {e}")),
            };
            if *n_in >= n_max {
                // Witness (or first sighting): replace the line's stored rows
                // outright. The branch precondition (`n_in >= n_max`) makes a
                // full replace safe, INCLUDING the incoming batch's own old
                // rows: a same-batch re-ingest (watcher re-process, shifted
                // `entry_index` after the sliding window dropped the oldest
                // entry) becomes a true replace instead of re-adding every
                // line at its new index. Ids churn; the mute carry above and
                // the collision guard below keep that sound.
                // Snapshot BEFORE the delete: mute flags of the rows about
                // to go, oldest-first (whole-set OR at carry time).
                let was_muted: Vec<bool> = doomed_mutes
                    .query_map(
                        rusqlite::params![ts, msg, batch_owner.clone()],
                        |r| r.get::<_, i64>(0).map(|v| v != 0),
                    )
                    .map_err(|e| format!("Mute snapshot failed: {e}"))?
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(|e| format!("Mute snapshot failed: {e}"))?;
                if was_muted.iter().any(|m| *m) {
                    mute_carry.insert((ts.clone(), msg.clone()), was_muted);
                }
                let removed = del
                    .execute(rusqlite::params![ts, msg, batch_owner.clone()])
                    .map_err(|e| format!("Failed to delete re-ingested stall event: {e}"))?;
                deleted_total += removed;
                if removed > 0 {
                    deleted_lines.insert((ts.clone(), msg.clone()));
                }
            } else {
                witnessed.insert((ts.clone(), msg.clone()));
            }
        }
        witnessed
    };

    // Cross-owner index collision guard: the storage UNIQUE key
    // `(event_timestamp, raw_message, entry_index)` excludes the owner, so
    // if another character ALREADY holds an incoming occurrence's
    // (ts, msg, idx) triple, `INSERT OR IGNORE` would skip A's replacement
    // AFTER the witness DELETE already removed A's old row — silently
    // losing A's sale. Abort the whole call ONLY for lines where the
    // witness DELETE actually removed this owner's row (`deleted_lines`):
    // there the replacement is owed and the slot is held by someone else.
    // First-sighting lines with nothing deleted keep the prior single-row
    // skip (a colliding row drops, the rest of the batch inserts) instead
    // of blocking every later scan of the book.
    let mut collided: std::collections::BTreeSet<(String, String, i64)> =
        std::collections::BTreeSet::new();
    {
        let mut clash = tx
            .prepare(
                "SELECT COUNT(*) FROM stall_events
                 WHERE event_timestamp = ?1 AND raw_message = ?2 AND entry_index = ?3
                   AND NOT (owner IS ?4)",
            )
            .map_err(|e| format!("Failed to prepare owner-collision check: {e}"))?;
        for ev in events {
            let key = (ev.event_timestamp.clone(), ev.raw_message.clone());
            if witnessed.contains(&key) {
                continue;
            }
            let held: i64 = clash
                .query_row(
                    rusqlite::params![
                        ev.event_timestamp,
                        ev.raw_message,
                        ev.entry_index,
                        ev.owner
                    ],
                    |r| r.get(0),
                )
                .map_err(|e| format!("Owner-collision check failed: {e}"))?;
            if held > 0 {
                if deleted_lines.contains(&key) {
                    return Err(format!(
                        "Refusing re-ingest: another character holds '{}' at index {} — \
                         inserting would silently drop this batch's row. Re-scan that \
                         character's book first, or pick a book that reuses only this \
                         character's indexes.",
                        ev.raw_message, ev.entry_index
                    ));
                }
                collided.insert((key.0, key.1, ev.entry_index));
            }
        }
    }

    let mut inserted = 0usize;
    {
        let mut stmt = tx
            .prepare(
                "INSERT OR IGNORE INTO stall_events (
                    event_timestamp, event_at, log_timestamp, log_title,
                    action, player, owner, item, quantity,
                    price_unit, price_total, raw_message, entry_index
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
            )
            .map_err(|e| format!("Failed to prepare insert: {e}"))?;

        for ev in events {
            if witnessed.contains(&(ev.event_timestamp.clone(), ev.raw_message.clone())) {
                continue;
            }
            // First-sighting collision (guard above): someone else holds the
            // slot and this line deleted nothing of ours — skip just this
            // row (prior `INSERT OR IGNORE` behaviour), never the batch.
            if collided.contains(&(
                ev.event_timestamp.clone(),
                ev.raw_message.clone(),
                ev.entry_index,
            )) {
                continue;
            }
            let changed = stmt
                .execute(rusqlite::params![
                    ev.event_timestamp,
                    ev.event_at,
                    ev.log_timestamp,
                    ev.log_title,
                    ev.action,
                    ev.player,
                    ev.owner,
                    ev.item,
                    ev.quantity,
                    ev.price_unit,
                    ev.price_total,
                    ev.raw_message,
                    ev.entry_index,
                ])
                .map_err(|e| format!("Failed to insert stall event: {e}"))?;
            inserted += changed;
        }
    }

    // Mute carry: when any doomed row for a line was muted, mute the whole
    // replacement set (per-group OR, errs toward muted — reversible via the
    // per-row toggle). Positional oldest-first mapping is unsound once two
    // or more batches lose: `doomed_mutes` spans every losing batch while
    // the replacements are only the incoming batch, so a flat zip pairs
    // unrelated occurrences (mute leak onto live rows, or resurrection of
    // a deliberately muted event). Same policy as the v89 witness pass.
    if !mute_carry.is_empty() {
        let mut flag = tx
            .prepare(
                "UPDATE stall_events SET ignored = 1 WHERE id = ?1",
            )
            .map_err(|e| format!("Failed to prepare mute carry: {e}"))?;
        for ((ts, msg), mutes) in &mute_carry {
            if !mutes.iter().any(|m| *m) {
                continue;
            }
            // Owner-scoped like the witness queries above: the replacements
            // just inserted carry the batch owner, so only they can match.
            let batch_owner: Option<String> = events
                .iter()
                .find(|e| &e.event_timestamp == ts && &e.raw_message == msg)
                .and_then(|e| e.owner.clone());
            let mut ids = tx
                .prepare(
                    "SELECT id FROM stall_events
                     WHERE event_timestamp = ?1 AND raw_message = ?2 AND owner IS ?3
                     ORDER BY id ASC",
                )
                .map_err(|e| format!("Failed to prepare mute carry lookup: {e}"))?
                .query_map(rusqlite::params![ts, msg, batch_owner], |r| r.get::<_, i64>(0))
                .map_err(|e| format!("Mute carry lookup failed: {e}"))?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| format!("Mute carry lookup failed: {e}"))?;
            for id in ids.drain(..) {
                flag.execute(rusqlite::params![id])
                    .map_err(|e| format!("Mute carry failed: {e}"))?;
            }
        }
    }

    tx.commit()
        .map_err(|e| format!("Failed to commit transaction: {e}"))?;

    // Net new: replacements are not new rows (DELETE + INSERT of the same
    // occurrence), so the import/scan totals report growth only.
    Ok(inserted.saturating_sub(deleted_total.min(inserted)))
}

// ============================================================
// Phase 3 — Read/mutate commands
// ============================================================

/// A single stall event row as returned to the frontend.
#[derive(Debug, Clone, Serialize)]
pub struct StallEvent {
    pub id: i64,
    pub event_timestamp: String,
    pub event_at: Option<String>,
    pub log_timestamp: String,
    pub log_title: String,
    pub action: String,
    pub player: String,
    pub owner: Option<String>,
    pub item: Option<String>,
    pub quantity: i64,
    pub price_unit: Option<f64>,
    pub price_total: Option<i64>,
    pub raw_message: String,
    pub entry_index: i64,
    pub ignored: bool,
    pub created_at: String,
}

/// Filter shape shared by list, stats, and aggregation queries. Every field
/// is optional; `owner` is the only one that should always be populated by
/// the frontend (missing/empty owner → empty results, by design).
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StallEventsFilters {
    pub owner: Option<String>,
    pub action: Option<String>,
    pub player: Option<String>,
    pub item: Option<String>,
    pub date_from: Option<String>,
    pub date_to: Option<String>,
    pub include_ignored: Option<bool>,
}

/// Parameters for `get_stall_events` (pagination + sort + filters).
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StallEventsParams {
    #[serde(flatten)]
    pub filters: StallEventsFilters,
    pub sort_by: Option<String>,
    pub sort_dir: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
    /// Override the default `action != 'unknown'` filter. Pass `Some("unknown")`
    /// from the Shop Log modal's "show unknowns" toggle.
    pub force_action: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct StallEventsPage {
    pub rows: Vec<StallEvent>,
    pub total_count: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct StallStats {
    pub total_sales: i64,
    pub total_revenue: i64,
    pub unique_buyers: i64,
    pub unique_items: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct StallFilterOptions {
    pub buyers: Vec<String>,
    pub players: Vec<String>,
    pub items: Vec<String>,
    pub dates: Vec<String>,
    pub actions: Vec<String>,
}

fn opt_nonempty(s: &Option<String>) -> Option<&str> {
    s.as_deref().map(str::trim).filter(|s| !s.is_empty())
}

/// Build a SQL WHERE clause + bound params from a filter struct.
///
/// Encodes two non-obvious rules from plan §5.1:
///
/// 1. **Default action filter excludes `'unknown'`** — list views never show
///    unparseable entries. Callers that want unknowns pass
///    `force_action = Some("unknown")` (or the caller sets
///    `filters.action = Some("unknown")` explicitly).
/// 2. **Default `include_ignored = true`** — list views *do* show ignored
///    rows (greyed out). Aggregations pass `false` to exclude them.
///
/// Returns `None` if `owner` is missing/empty — callers should return empty
/// results rather than running an unscoped query.
fn build_filter_where(
    filters: &StallEventsFilters,
    force_action: Option<&str>,
) -> Option<(String, Vec<Value>)> {
    let owner = opt_nonempty(&filters.owner)?;

    let mut sql = String::from(" WHERE 1=1");
    let mut params: Vec<Value> = Vec::new();

    sql.push_str(" AND owner = ?");
    params.push(Value::Text(owner.to_string()));

    if let Some(force) = force_action {
        sql.push_str(" AND action = ?");
        params.push(Value::Text(force.to_string()));
    } else if let Some(action) = opt_nonempty(&filters.action) {
        sql.push_str(" AND action = ?");
        params.push(Value::Text(action.to_string()));
    } else {
        sql.push_str(" AND action != 'unknown'");
    }

    if let Some(player) = opt_nonempty(&filters.player) {
        sql.push_str(" AND player = ?");
        params.push(Value::Text(player.to_string()));
    }

    if let Some(item) = opt_nonempty(&filters.item) {
        sql.push_str(" AND item = ?");
        params.push(Value::Text(item.to_string()));
    }

    // Date inputs MUST be `YYYY-MM-DD` (10 chars). Reject anything else
    // loudly via empty result rather than producing the malformed bound
    // `"2026-04-13 14:29:00 00:00:00"` if the frontend ever ships a full
    // ISO timestamp by mistake.
    if let Some(from) = opt_nonempty(&filters.date_from) {
        if from.len() != 10 {
            return None;
        }
        sql.push_str(" AND event_at >= ?");
        params.push(Value::Text(format!("{from} 00:00:00")));
    }

    if let Some(to) = opt_nonempty(&filters.date_to) {
        if to.len() != 10 {
            return None;
        }
        sql.push_str(" AND event_at <= ?");
        params.push(Value::Text(format!("{to} 23:59:59")));
    }

    let include_ignored = filters.include_ignored.unwrap_or(true);
    if !include_ignored {
        sql.push_str(" AND ignored = 0");
    }

    Some((sql, params))
}

/// Whitelisted sort column + direction + stable tiebreakers.
fn resolve_sort(sort_by: Option<&str>, sort_dir: Option<&str>) -> String {
    let col = match sort_by.unwrap_or("event_at") {
        "event_at" | "event_timestamp" => "event_at",
        "player" => "player",
        "item" => "item",
        "action" => "action",
        "quantity" => "quantity",
        "price_unit" => "price_unit",
        "price_total" => "price_total",
        _ => "event_at",
    };
    let dir = match sort_dir.unwrap_or("desc").to_ascii_lowercase().as_str() {
        "asc" => "ASC",
        _ => "DESC",
    };
    // Stable tiebreakers ensure deterministic pagination across refetches.
    format!("{col} {dir}, event_at DESC, entry_index DESC, id DESC")
}

fn row_to_event(row: &rusqlite::Row) -> rusqlite::Result<StallEvent> {
    Ok(StallEvent {
        id: row.get(0)?,
        event_timestamp: row.get(1)?,
        event_at: row.get(2)?,
        log_timestamp: row.get(3)?,
        log_title: row.get(4)?,
        action: row.get(5)?,
        player: row.get(6)?,
        owner: row.get(7)?,
        item: row.get(8)?,
        quantity: row.get(9)?,
        price_unit: row.get(10)?,
        price_total: row.get(11)?,
        raw_message: row.get(12)?,
        entry_index: row.get(13)?,
        ignored: row.get::<_, i64>(14)? != 0,
        created_at: row.get(15)?,
    })
}

const STALL_EVENT_COLUMNS: &str = "id, event_timestamp, event_at, log_timestamp, log_title, \
    action, player, owner, item, quantity, price_unit, price_total, raw_message, entry_index, \
    ignored, created_at";

/// Paginated list of stall events with filters + sort.
///
/// Empty `owner` returns an empty page rather than leaking cross-character
/// data. `limit` is clamped to `[1, 10_000]`, `offset` floored at 0.
#[tauri::command]
pub fn get_stall_events(
    db: State<'_, DbPool>,
    params: StallEventsParams,
) -> Result<StallEventsPage, String> {
    let Some((where_sql, bound)) =
        build_filter_where(&params.filters, params.force_action.as_deref())
    else {
        return Ok(StallEventsPage {
            rows: Vec::new(),
            total_count: 0,
        });
    };

    let limit = params.limit.unwrap_or(500).clamp(1, 10_000);
    let offset = params.offset.unwrap_or(0).max(0);
    let order_by = resolve_sort(params.sort_by.as_deref(), params.sort_dir.as_deref());

    let conn = db
        .get()
        .map_err(|e| format!("Database connection error: {e}"))?;

    // Count query uses the same WHERE bound params.
    let count_sql = format!("SELECT COUNT(*) FROM stall_events{where_sql}");
    let total_count: i64 = conn
        .query_row(
            &count_sql,
            rusqlite::params_from_iter(bound.iter()),
            |row| row.get(0),
        )
        .map_err(|e| format!("Count query failed: {e}"))?;

    let list_sql = format!(
        "SELECT {STALL_EVENT_COLUMNS} FROM stall_events{where_sql} \
         ORDER BY {order_by} LIMIT ? OFFSET ?"
    );
    let mut list_params = bound;
    list_params.push(Value::Integer(limit));
    list_params.push(Value::Integer(offset));

    let mut stmt = conn
        .prepare(&list_sql)
        .map_err(|e| format!("Failed to prepare list query: {e}"))?;
    let rows = stmt
        .query_map(rusqlite::params_from_iter(list_params.iter()), row_to_event)
        .map_err(|e| format!("List query failed: {e}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("Failed to read list results: {e}"))?;

    Ok(StallEventsPage { rows, total_count })
}

/// Aggregate stats for the Sales header. Always forces `action = 'bought'`
/// and `include_ignored = false`, regardless of caller input, so the numbers
/// match "revenue from sales I haven't muted".
#[tauri::command]
pub fn get_stall_stats(
    db: State<'_, DbPool>,
    filters: StallEventsFilters,
) -> Result<StallStats, String> {
    // Stats are by definition over `bought + non-ignored`. Caller-supplied
    // `action` / `include_ignored` are intentionally overridden — the date
    // range, owner, player, and item filters still flow through.
    let mut filters = filters;
    filters.include_ignored = Some(false);

    let Some((where_sql, bound)) = build_filter_where(&filters, Some("bought")) else {
        return Ok(StallStats {
            total_sales: 0,
            total_revenue: 0,
            unique_buyers: 0,
            unique_items: 0,
        });
    };

    let sql = format!(
        "SELECT COUNT(*) AS total_sales, \
                COALESCE(SUM(price_total), 0) AS total_revenue, \
                COUNT(DISTINCT player) AS unique_buyers, \
                COUNT(DISTINCT item) AS unique_items \
         FROM stall_events{where_sql}"
    );

    let conn = db
        .get()
        .map_err(|e| format!("Database connection error: {e}"))?;

    conn.query_row(&sql, rusqlite::params_from_iter(bound.iter()), |row| {
        Ok(StallStats {
            total_sales: row.get(0)?,
            total_revenue: row.get(1)?,
            unique_buyers: row.get(2)?,
            unique_items: row.get(3)?,
        })
    })
    .map_err(|e| format!("Stats query failed: {e}"))
}

/// Distinct value lists for filter dropdowns, scoped to a single owner.
/// `buyers` = `DISTINCT player WHERE action='bought'`; `players` =
/// `DISTINCT player WHERE action != 'unknown'` (includes the stall owner).
#[tauri::command]
pub fn get_stall_filter_options(
    db: State<'_, DbPool>,
    owner: Option<String>,
) -> Result<StallFilterOptions, String> {
    let Some(owner) = opt_nonempty(&owner).map(str::to_string) else {
        return Ok(StallFilterOptions {
            buyers: Vec::new(),
            players: Vec::new(),
            items: Vec::new(),
            dates: Vec::new(),
            actions: Vec::new(),
        });
    };

    let conn = db
        .get()
        .map_err(|e| format!("Database connection error: {e}"))?;

    let collect_strings = |sql: &str| -> Result<Vec<String>, String> {
        let mut stmt = conn
            .prepare(sql)
            .map_err(|e| format!("Failed to prepare filter query: {e}"))?;
        let rows = stmt
            .query_map(rusqlite::params![owner], |row| row.get::<_, String>(0))
            .map_err(|e| format!("Filter query failed: {e}"))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("Failed to read filter results: {e}"))
    };

    let buyers = collect_strings(
        "SELECT DISTINCT player FROM stall_events \
         WHERE owner = ?1 AND action = 'bought' \
         ORDER BY player",
    )?;
    let players = collect_strings(
        "SELECT DISTINCT player FROM stall_events \
         WHERE owner = ?1 AND action != 'unknown' \
         ORDER BY player",
    )?;
    let items = collect_strings(
        "SELECT DISTINCT item FROM stall_events \
         WHERE owner = ?1 AND item IS NOT NULL \
         ORDER BY item",
    )?;
    let dates = collect_strings(
        "SELECT DISTINCT substr(event_at, 1, 10) AS d FROM stall_events \
         WHERE owner = ?1 AND event_at IS NOT NULL \
         ORDER BY d DESC",
    )?;
    let actions = collect_strings(
        "SELECT DISTINCT action FROM stall_events \
         WHERE owner = ?1 \
         ORDER BY action",
    )?;

    Ok(StallFilterOptions {
        buyers,
        players,
        items,
        dates,
        actions,
    })
}

/// Toggle the soft-mute flag on a single event. `ignored = true` excludes the
/// row from all aggregations (but not list views).
///
/// The `owner` parameter is defense-in-depth — the frontend already passes
/// IDs only from the currently-loaded (owner-scoped) list, but enforcing
/// `WHERE id = ?1 AND owner = ?2` at the SQL level closes the door on any
/// future caller that might forget the scope.
#[tauri::command]
pub fn toggle_stall_event_ignored(
    db: State<'_, DbPool>,
    id: i64,
    ignored: bool,
    owner: Option<String>,
) -> Result<(), String> {
    let owner = opt_nonempty(&owner)
        .ok_or_else(|| "toggle_stall_event_ignored requires an owner".to_string())?
        .to_string();
    let conn = db
        .get_write()
        .map_err(|e| format!("Database connection error: {e}"))?;
    conn.execute(
        "UPDATE stall_events SET ignored = ?1 WHERE id = ?2 AND owner = ?3",
        rusqlite::params![ignored as i64, id, owner],
    )
    .map_err(|e| format!("Toggle query failed: {e}"))?;
    Ok(())
}

// ============================================================
// Phase 4 — Aggregation commands
// ============================================================

/// Parameters for `get_stall_revenue`. Granularity is a string on the wire
/// for natural JS interop (`"daily"` / `"weekly"` / `"monthly"`).
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StallRevenueParams {
    pub owner: Option<String>,
    pub granularity: Option<String>,
    pub date_from: Option<String>,
    pub date_to: Option<String>,
    pub player: Option<String>,
    pub item: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StallInventoryParams {
    pub owner: Option<String>,
    pub period_days: Option<i64>,
    /// History window: replay only ledger events with `event_at >= date('now', 'localtime', '-' || window_days || ' days')`.
    /// `None` (or <= 0) replays the full history. The stale-delist problem
    /// (in-game takedowns log no `removed` line) means ancient `added` lines
    /// replay as phantom stock forever — a bounded window ages them out.
    /// `bought` rows are exempt (they feed SOLD / AVG DAILY REVENUE for any
    /// Sales period and cannot create phantom stock). Live 2026-11 case:
    /// full history shows 110 phantom rows for a 41-slot stall; a 100-day
    /// window shows 47. Frontend owns the value (view pref).
    pub window_days: Option<i64>,
}

fn parse_granularity(s: &str) -> Granularity {
    match s.to_ascii_lowercase().as_str() {
        "weekly" => Granularity::Weekly,
        "monthly" => Granularity::Monthly,
        _ => Granularity::Daily,
    }
}

/// Revenue pivot for the Revenue tab. Always over `bought + non-ignored`,
/// scoped to a single owner. Date range, buyer, and item filters flow
/// through. Returns an empty result on missing owner.
#[tauri::command]
pub fn get_stall_revenue(
    db: State<'_, DbPool>,
    params: StallRevenueParams,
) -> Result<RevenueResult, String> {
    let filters = StallEventsFilters {
        owner: params.owner,
        action: None,
        player: params.player,
        item: params.item,
        date_from: params.date_from,
        date_to: params.date_to,
        include_ignored: Some(false),
    };
    let Some((where_sql, bound)) = build_filter_where(&filters, Some("bought")) else {
        return Ok(RevenueResult {
            periods: Vec::new(),
            items: Vec::new(),
            cells: Vec::new(),
            row_totals: Vec::new(),
            col_totals: Vec::new(),
            grand_total: 0,
        });
    };

    let conn = db
        .get()
        .map_err(|e| format!("Database connection error: {e}"))?;

    // Only rows with item AND event_at can contribute to the pivot.
    let sql = format!(
        "SELECT item, event_at, price_total FROM stall_events{where_sql} \
         AND item IS NOT NULL AND event_at IS NOT NULL AND price_total IS NOT NULL"
    );
    let mut stmt = conn
        .prepare(&sql)
        .map_err(|e| format!("Failed to prepare revenue query: {e}"))?;
    let events = stmt
        .query_map(rusqlite::params_from_iter(bound.iter()), |row| {
            Ok(RevenueEvent {
                item: row.get::<_, String>(0)?,
                event_at: row.get::<_, String>(1)?,
                price_total: row.get::<_, i64>(2)?,
            })
        })
        .map_err(|e| format!("Revenue query failed: {e}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("Failed to read revenue results: {e}"))?;

    let granularity = params
        .granularity
        .as_deref()
        .map(parse_granularity)
        .unwrap_or(Granularity::Daily);

    Ok(aggregate_revenue(events, granularity))
}

/// Inventory tier-stack snapshot for the Inventory tab. Seeds the replay from
/// the owner's `stall_inventory_baseline` rows (opening stock counted
/// in-game), then replays every non-ignored shop-log event after each
/// baseline's timestamp on top — in `(event_at ASC, id ASC)` order so
/// same-minute `visible` events arrive after their `added` counterpart.
#[tauri::command]
pub fn get_stall_inventory(
    db: State<'_, DbPool>,
    params: StallInventoryParams,
) -> Result<InventoryResult, String> {
    let Some(owner) = opt_nonempty(&params.owner).map(str::to_string) else {
        return Ok(InventoryResult {
            items: Vec::new(),
            active_dates: Vec::new(),
            estimated_value: 0,
            total_sold: 0,
            avg_daily_revenue: 0.0,
        });
    };

    let conn = db
        .get()
        .map_err(|e| format!("Database connection error: {e}"))?;

    // CRITICAL: ORDER BY event_at ASC, id ASC. Without the id tiebreaker,
    // `visible` events could be processed before `added` events within the
    // same minute, leaving the tier unpriced.
    //
    // History window: `window_days > 0` bounds the STOCK replay to recent
    // events so ancient `added` lines for long-delisted items (the game logs
    // no `removed` on takedown) age out instead of replaying as phantom
    // stock. The window applies to ledger actions ONLY — `bought` rows
    // always ride along so SOLD / AVG DAILY REVENUE stay true for any Sales
    // period (an unbacked `bought` only pushes a negative tier the next
    // `added` clears and finalize drops; it never creates phantom stock).
    // `date('now','localtime', ...)` is local-time by SQLite convention
    // (bare `date('now', ...)` is UTC), matching the local-time `event_at`
    // stamps the year resolver writes. Without `'localtime'` the boundary
    // sits off by the TZ offset near midnight on the cutoff date.
    let (sql, window_days): (String, Option<i64>) = match params.window_days {
        Some(n) if n > 0 => (
            "SELECT item, event_at, action, quantity, price_unit, price_total \
             FROM stall_events \
             WHERE owner = ?1 AND ignored = 0 AND item IS NOT NULL AND event_at IS NOT NULL \
               AND action != 'unknown' AND action != 'collected' \
               AND (action = 'bought' OR event_at >= date('now', 'localtime', '-' || ?2 || ' days')) \
             ORDER BY event_at ASC, id ASC"
                .to_string(),
            Some(n),
        ),
        _ => (
            "SELECT item, event_at, action, quantity, price_unit, price_total \
             FROM stall_events \
             WHERE owner = ?1 AND ignored = 0 AND item IS NOT NULL AND event_at IS NOT NULL \
               AND action != 'unknown' AND action != 'collected' \
             ORDER BY event_at ASC, id ASC"
                .to_string(),
            None,
        ),
    };
    let mut stmt = conn
        .prepare(&sql)
        .map_err(|e| format!("Failed to prepare inventory query: {e}"))?;
    let events = match window_days {
        Some(n) => stmt
            .query_map(rusqlite::params![owner, n], |row| {
                Ok(InventoryEvent {
                    item: row.get::<_, String>(0)?,
                    event_at: row.get::<_, String>(1)?,
                    action: row.get::<_, String>(2)?,
                    quantity: row.get::<_, i64>(3)?,
                    price_unit: row.get::<_, Option<f64>>(4)?,
                    price_total: row.get::<_, Option<i64>>(5)?,
                })
            })
            .map_err(|e| format!("Inventory query failed: {e}"))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("Failed to read inventory results: {e}"))?,
        None => stmt
            .query_map(rusqlite::params![owner], |row| {
                Ok(InventoryEvent {
                    item: row.get::<_, String>(0)?,
                    event_at: row.get::<_, String>(1)?,
                    action: row.get::<_, String>(2)?,
                    quantity: row.get::<_, i64>(3)?,
                    price_unit: row.get::<_, Option<f64>>(4)?,
                    price_total: row.get::<_, Option<i64>>(5)?,
                })
            })
            .map_err(|e| format!("Inventory query failed: {e}"))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("Failed to read inventory results: {e}"))?,
    };

    // Baselines are few (one row per item the user counted) so a second
    // small query on the same read checkout is cheaper than a JOIN the
    // DB-free aggregator cannot consume anyway. The intermediate `rows`
    // binding forces the `MappedRows` temporary to drop before `bstmt`.
    let mut bstmt = conn
        .prepare(
            "SELECT item, quantity, price_unit, recorded_at FROM stall_inventory_baseline \
             WHERE owner = ?1",
        )
        .map_err(|e| format!("Failed to prepare baseline query: {e}"))?;
    let rows = bstmt
        .query_map(rusqlite::params![owner], |row| {
            Ok(BaselineSeed {
                item: row.get::<_, String>(0)?,
                quantity: row.get::<_, i64>(1)?,
                price_unit: row.get::<_, Option<f64>>(2)?,
                recorded_at: row.get::<_, String>(3)?,
            })
        })
        .map_err(|e| format!("Baseline query failed: {e}"))?;
    let baselines: Vec<BaselineSeed> = rows
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("Failed to read baselines: {e}"))?;

    let period_days = params.period_days.unwrap_or(7);
    Ok(crate::stall_aggregations::aggregate_inventory_with_baselines(
        events, period_days, baselines,
    ))
}

/// Parameters for `get_stall_sales_timeseries` (Trends tab).
///
/// `item`, `top_n`, and "neither" are mutually exclusive on the frontend and
/// select the line mode here:
/// - `item = Some(name)` → single-item line.
/// - `top_n = Some(5|10)` → that many top-grossing item lines.
/// - both `None` → a single `Total` line.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StallTimeseriesParams {
    pub owner: Option<String>,
    /// `"1d" | "3d" | "7d" | "1m" | "all"`. Defaults to `"7d"`.
    pub period: Option<String>,
    pub item: Option<String>,
    pub top_n: Option<i64>,
}

fn parse_timeseries_period(s: &str) -> TimeseriesPeriod {
    match s {
        "1d" => TimeseriesPeriod::Day1,
        "3d" => TimeseriesPeriod::Day3,
        "1m" => TimeseriesPeriod::Month1,
        "all" => TimeseriesPeriod::AllTime,
        _ => TimeseriesPeriod::Day7,
    }
}

/// Sales-over-time series for the Trends tab. Always over `bought + non-ignored`
/// for the owner. Returns both gold and units series per line so the frontend
/// Gold/Units toggle never refetches. Returns an empty result on missing owner.
///
/// Unlike Revenue, this never filters by item in SQL: `TopGrossing` ranking and
/// the `Total` line need every item, and `Item` mode filters in the aggregator
/// so the window can anchor to that item's own latest sale.
#[tauri::command]
pub fn get_stall_sales_timeseries(
    db: State<'_, DbPool>,
    params: StallTimeseriesParams,
) -> Result<SalesTimeseriesResult, String> {
    let filters = StallEventsFilters {
        owner: params.owner,
        include_ignored: Some(false),
        ..Default::default()
    };
    let Some((where_sql, bound)) = build_filter_where(&filters, Some("bought")) else {
        return Ok(SalesTimeseriesResult {
            periods: Vec::new(),
            lines: Vec::new(),
            bucket: "day".to_string(),
        });
    };

    let conn = db
        .get()
        .map_err(|e| format!("Database connection error: {e}"))?;

    let sql = format!(
        "SELECT item, event_at, quantity, price_total FROM stall_events{where_sql} \
         AND item IS NOT NULL AND event_at IS NOT NULL AND price_total IS NOT NULL"
    );
    let mut stmt = conn
        .prepare(&sql)
        .map_err(|e| format!("Failed to prepare timeseries query: {e}"))?;
    let events = stmt
        .query_map(rusqlite::params_from_iter(bound.iter()), |row| {
            Ok(TimeseriesEvent {
                item: row.get::<_, String>(0)?,
                event_at: row.get::<_, String>(1)?,
                quantity: row.get::<_, i64>(2)?,
                price_total: row.get::<_, i64>(3)?,
            })
        })
        .map_err(|e| format!("Timeseries query failed: {e}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("Failed to read timeseries results: {e}"))?;

    let period = params
        .period
        .as_deref()
        .map(parse_timeseries_period)
        .unwrap_or(TimeseriesPeriod::Day7);

    let mode = match (opt_nonempty(&params.item), params.top_n) {
        (Some(item), _) => TimeseriesMode::Item(item.to_string()),
        (None, Some(n)) if n > 0 => TimeseriesMode::TopGrossing(n as usize),
        _ => TimeseriesMode::Total,
    };

    Ok(aggregate_sales_timeseries(events, period, mode))
}

/// Delete every stall event + opening-stock baseline for a single character.
/// Requires non-empty owner — we never want a typo to wipe all data across
/// every alt. One BEGIN IMMEDIATE transaction for both DELETEs on the
/// single-write pool (pool-is-the-lock): either both commit or neither does,
/// serialized with live-ingest inserts which run in their own transaction.
#[tauri::command]
pub fn clear_stall_events(db: State<'_, DbPool>, owner: Option<String>) -> Result<usize, String> {
    let owner = opt_nonempty(&owner)
        .ok_or_else(|| "clear_stall_events requires an owner".to_string())?
        .to_string();

    // One BEGIN IMMEDIATE transaction for both DELETEs (pool-is-the-lock
    // invariant): either both commit or neither does — a baseline-only
    // second-statement failure can never leave phantom inventory behind.
    let mut conn = db
        .get_write()
        .map_err(|e| format!("Database connection error: {e}"))?;
    let tx = conn
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .map_err(|e| format!("Failed to begin transaction: {e}"))?;
    let deleted = tx
        .execute(
            "DELETE FROM stall_events WHERE owner = ?1",
            rusqlite::params![owner],
        )
        .map_err(|e| format!("Clear query failed: {e}"))?;
    tx.execute(
        "DELETE FROM stall_inventory_baseline WHERE owner = ?1",
        rusqlite::params![owner],
    )
    .map_err(|e| format!("Baseline clear failed: {e}"))?;
    tx.commit()
        .map_err(|e| format!("Failed to commit clear: {e}"))?;
    Ok(deleted)
}

// ============================================================
// Phase 10 — Import / Export
// ============================================================

#[derive(Debug, Clone, Serialize)]
pub struct ImportResult {
    pub total_entries: usize,
    pub new_entries: usize,
    /// The owner the rows were stamped with: parser-detected if the book
    /// contained owner-only actions, else the caller-supplied current_owner.
    pub effective_owner: Option<String>,
    /// True when the parser couldn't detect an owner (bought-only file) and
    /// we fell back to the caller's current_owner. The UI uses this to show
    /// "claimed for X — book did not identify an owner".
    pub owner_claimed: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct ExportResult {
    pub files_written: usize,
    pub events_exported: usize,
}

/// Scan a filename for a 4-digit year between 2000 and 2099. Falls back to
/// the current local year if no match. Used by Import and the Books-directory
/// watcher to seed the year resolver when the book content itself doesn't
/// carry an explicit year.
pub(crate) fn year_from_filename(path: &Path) -> i32 {
    let name = path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or_default();
    let bytes = name.as_bytes();
    let mut i = 0;
    while i + 4 <= bytes.len() {
        if bytes[i].is_ascii_digit()
            && bytes[i + 1].is_ascii_digit()
            && bytes[i + 2].is_ascii_digit()
            && bytes[i + 3].is_ascii_digit()
        {
            let year: i32 = std::str::from_utf8(&bytes[i..i + 4])
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(0);
            if (2000..=2099).contains(&year) {
                // Reject if surrounded by more digits (would be a longer number).
                let prev_digit = i > 0 && bytes[i - 1].is_ascii_digit();
                let next_digit = i + 4 < bytes.len() && bytes[i + 4].is_ascii_digit();
                if !prev_digit && !next_digit {
                    return year;
                }
            }
        }
        i += 1;
    }
    Local::now().year()
}

/// Base year for a whole-file parse (import + recount preview): probe-parse
/// the book and use the live resolver — wrap to the previous year when the
/// oldest entry is in the future (December book first opened in January).
/// Same parity as the Books watcher (`ingest_shop_log_file`) and the live
/// coordinator path; a filename-only year would stamp `book_latest_at` a
/// year in the future and collapse the recount's
/// `min(now, book_latest_at)` snapshot guard to now. Falls back to the
/// filename year only when the book has no parseable entries.
fn book_base_year(path_buf: &std::path::Path, content: &str) -> i32 {
    use crate::shop_log_parser::parse_shop_log;
    use crate::stall_year_resolver::base_year_for_live;
    let probe = parse_shop_log("Imported", content, "imported", 1970);
    match probe.entries.first() {
        Some(e) => base_year_for_live(e.timestamp.as_str()),
        None => year_from_filename(path_buf),
    }
}

/// Replace filename-hostile characters with `_` so user-controlled owner
/// names can't break out of the export directory.
fn sanitize_owner(owner: &str) -> String {
    owner
        .chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' | '\0' => '_',
            _ => c,
        })
        .collect()
}

/// Read a `.txt` shop log file, parse it, and persist its entries.
///
/// Owner resolution rules (plan §10.1 / §12.1):
/// 1. Year comes from the live resolver over the book's own oldest entry
///    (wrap to previous year when it is in the future — December book first
///    opened in January); only a book with no parseable entries falls back
///    to the filename year.
/// 2. `effective_owner = parser_hint.or(current_owner)`. Three cases:
///    - parser hint matches active character → normal import
///    - parser hint is a different character → stamps with the parsed owner;
///      the UI surfaces a "switch character to view" message
///    - parser hint is missing (bought-only file) → falls back to
///      current_owner and sets `owner_claimed = true`
#[tauri::command]
pub fn import_shop_log_file(
    db: State<'_, DbPool>,
    path: String,
    current_owner: Option<String>,
) -> Result<ImportResult, String> {
    let path_buf = std::path::PathBuf::from(&path);
    let content = std::fs::read_to_string(&path_buf)
        .map_err(|e| format!("Failed to read shop log file: {e}"))?;

    let base_year = book_base_year(&path_buf, &content);
    // Distinct batch per file: the witness rule keys batches on
    // (log_title, log_timestamp), and every import used to stamp the same
    // constant pair — so a second file's overlap deleted nothing and
    // re-inserted every shifted line as a duplicate. The file stem (same
    // shape as the Books watcher titles) plus the import moment identifies
    // this batch; re-importing the same file replays the same stem but a
    // newer stamp, so ties go newest and the row count never grows.
    let import_title = path_buf
        .file_stem()
        .and_then(|s| s.to_str())
        .filter(|s| !s.is_empty())
        .unwrap_or("Imported")
        .to_string();
    let import_stamp = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
    let shop_log: ShopLog = parse_shop_log(&import_title, &content, &import_stamp, base_year);
    let total_entries = shop_log.entries.len();

    let parsed_owner = shop_log.owner.clone();
    let effective_owner: Option<String> = parsed_owner
        .clone()
        .or_else(|| opt_nonempty(&current_owner).map(str::to_string));

    if effective_owner.is_none() {
        return Err(
            "Cannot import: file has no owner actions and no active character to claim it for"
                .to_string(),
        );
    }

    let owner_claimed = parsed_owner.is_none() && total_entries > 0;

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

    let new_entries = insert_stall_events(&db, &inputs)?;

    Ok(ImportResult {
        total_entries,
        new_entries,
        effective_owner,
        owner_claimed,
    })
}

/// One counted line of a recount preview: what the fresh book says the
/// stall holds for one item right now.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecountLine {
    pub item: String,
    pub quantity: i64,
    pub price_unit: Option<f64>,
}

/// Preview of `recount_stall_from_book`: per-item counts derived from the
/// fresh book, plus the in-stock items the book does NOT mention (they get
/// zero cutoffs on apply). Sales history is never touched — this only
/// stages `stall_inventory_baseline` rows.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecountPreview {
    pub owner: String,
    pub counted: Vec<RecountLine>,
    pub zeroed: Vec<String>,
    pub book_entries: usize,
    /// Content hash of the exact book bytes this preview was built from.
    /// `apply_stall_recount` must echo it back and aborts when the file no
    /// longer hashes the same — the confirm dialog's names only describe
    /// this content.
    pub content_hash: String,
    /// Newest `event_at` across the book's parsed entries (`YYYY-MM-DD
    /// HH:MM:SS`), if any. Apply stamps `recorded_at = min(now,
    /// book_latest_at)` so events that landed after the book's snapshot
    /// stay visible instead of being silently superseded by a now-stamp.
    pub book_latest_at: Option<String>,
}

/// Derive per-item counts from already-parsed shop-log entries by replaying
/// them oldest-first with the same semantics as the aggregator
/// (`stall_aggregations.rs`): `added` pushes stock, `visible`/`configured`
/// only price existing stock (they never add or remove units), `bought` and
/// `removed` deplete. Two book-window realities need explicit handling:
/// - A `visible` for stock the fresh book never `added` is the NORMAL
///   same-day case (the adds scrolled out of this book). The stock is really
///   there — seed one listing's worth, never 0.
/// - A `visible` for an item the book ALSO shows being sold out (`added`
///   covering the `bought`/`removed`, netting to zero) must NOT resurrect
///   it: seed only when the net is <= 0 AND the book's `added` lines do NOT
///   cover its depletions. A depletion the book never stocked is a
///   truncated ledger (adds scrolled out), not a sellout — the book's own
///   `visible` quantity states what is listed.
///
/// Items netting <= 0 are not in the stall. The LATEST priced
/// `visible`/`configured` event per item wins (the fresh book's own price is
/// the current listing). `bought` carries the sale price, never the listing.
fn recount_counts(entries: &[crate::shop_log_parser::ShopLogEntry]) -> Vec<RecountLine> {
    use std::collections::BTreeMap;
    let mut qty: BTreeMap<String, i64> = BTreeMap::new();
    let mut priced: BTreeMap<String, f64> = BTreeMap::new();
    // Per-item `added` coverage: only depletions the book itself stocked
    // block the priced-only seed below. A `bought` with no covering `added`
    // is scrolled-out history, and a later `visible` for it is a same-day
    // listing of live stock — the seed must not treat it as a sellout.
    let mut added_total: BTreeMap<String, i64> = BTreeMap::new();
    let mut depleted_total: BTreeMap<String, i64> = BTreeMap::new();
    for e in entries {
        let Some(item) = e.item.clone() else { continue };
        match e.action.as_str() {
            "added" => {
                *added_total.entry(item.clone()).or_default() += e.quantity;
                *qty.entry(item).or_default() += e.quantity;
            }
            "removed" | "bought" => {
                *depleted_total.entry(item.clone()).or_default() += e.quantity;
                // Clamp at zero: when earlier adds scrolled out of the book,
                // a remove-and-re-add session must replay to the fresh
                // quantity, not a negative hole a later `visible` cannot
                // seed (totals still record coverage for the guard below).
                let q = qty.entry(item).or_default();
                *q = (*q - e.quantity).max(0);
            }
            "visible" | "configured" => {
                if let Some(p) = e.price_unit {
                    priced.insert(item.clone(), p);
                    let entry = qty.entry(item.clone()).or_default();
                    // Seed one listing's worth only for the priced-only
                    // window case (net <= 0, in-book adds NOT covering
                    // depletions means the ledger is truncated and the
                    // book's own `visible` states live stock). Adds above 0
                    // are never lowered; a covered sellout (adds covering
                    // depletions, net 0) is never raised. A negative net
                    // always means uncovered depletions (adds scrolled out),
                    // so `<= 0` seeds both the never-stocked and the
                    // truncated cases.
                    let added = added_total.get(&item).copied().unwrap_or(0);
                    let depleted = depleted_total.get(&item).copied().unwrap_or(0);
                    if *entry <= 0 && (depleted == 0 || added < depleted) {
                        *entry = e.quantity;
                    }
                }
            }
            _ => {}
        }
    }
    qty.into_iter()
        .filter(|(_, q)| *q > 0)
        .map(|(item, quantity)| RecountLine {
            price_unit: priced.get(&item).copied(),
            item,
            quantity,
        })
        .collect()
}

/// Owner resolution shared by `preview_stall_recount` (and through it
/// `apply_stall_recount`): the book's advisory owner wins when present,
/// else the active character is claimed. A book that names a DIFFERENT
/// owner than the active character is rejected loudly — the Books dir holds
/// every alt's logs, the confirm dialog names the active character, and
/// applying against the alt would stamp zero baselines under the wrong
/// owner (recoverable only via the alt's Undo, which also eats real counts).
fn recount_owner(
    shop_log: &ShopLog,
    current_owner: &Option<String>,
) -> Result<String, String> {
    if let (Some(parsed), Some(requested)) = (
        shop_log.owner.as_ref(),
        opt_nonempty(current_owner).map(|s| s.to_string()),
    ) && parsed != &requested
    {
        return Err(format!(
            "This book belongs to {parsed}, not {requested}. Switch character or pick {requested}'s book."
        ));
    }
    shop_log
        .owner
        .clone()
        .or_else(|| opt_nonempty(current_owner).map(str::to_string))
        .ok_or_else(|| {
            "Cannot recount: file has no owner actions and no active character to claim it for".to_string()
        })
}

/// Preview a one-click recount from a fresh in-game shop log book: parse the
/// file, replay its lines into per-item counts, and diff against the current
/// in-stock set. Pure read — writes nothing. The frontend shows the
/// counted/zeroed split in the confirm dialog, then calls
/// `apply_stall_recount` with the same path.
#[tauri::command]
pub fn preview_stall_recount(
    db: State<'_, DbPool>,
    path: String,
    current_owner: Option<String>,
) -> Result<RecountPreview, String> {
    let path_buf = std::path::PathBuf::from(&path);
    let content = std::fs::read_to_string(&path_buf)
        .map_err(|e| format!("Failed to read shop log file: {e}"))?;
    preview_from_content(&db, &path_buf, &content, &current_owner)
}

/// File fingerprint for the preview→apply TOCTOU guard: (mtime nanos, bytes).
/// The game rewrites the book on every in-game save; a changed fingerprint
/// between preview and apply means the confirmed list no longer matches.
fn book_fingerprint(path: &std::path::Path) -> Result<(u128, u64), String> {
    let meta = std::fs::metadata(path).map_err(|e| format!("Failed to stat shop log file: {e}"))?;
    let mtime = meta
        .modified()
        .map_err(|e| format!("Failed to stat shop log file: {e}"))?
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| format!("Failed to stat shop log file: {e}"))?
        .as_nanos();
    Ok((mtime, meta.len()))
}

/// Content hash (FNV-1a 64) of the exact book bytes a preview was built
/// from. Mtime+len alone cannot tie apply to the previewed content (same
/// nanos + same length after a rewrite would pass); the frontend echoes
/// this hash back and apply aborts on mismatch.
fn book_content_hash(content: &str) -> String {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in content.bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    format!("{h:016x}")
}

/// Content-based recount preview shared by `preview_stall_recount` (file
/// read) and `apply_stall_recount` (read + fingerprint guard).
fn preview_from_content(
    db: &DbPool,
    path_buf: &std::path::Path,
    content: &str,
    current_owner: &Option<String>,
) -> Result<RecountPreview, String> {
    let base_year = book_base_year(path_buf, content);
    let shop_log: ShopLog = parse_shop_log("Imported", content, "imported", base_year);
    // Empty / stock-signal-free books must abort here, not at the UI: with
    // `counted == []` the caller would zero the entire in-stock set (every
    // item reads as "not listed"). A bought-only file replays to zero
    // quantities (clamped), same wipe. The user skipped the remove-all +
    // re-add; say so instead of offering Set 0 / Zero N.
    if !shop_log
        .entries
        .iter()
        .any(|e| matches!(e.action.as_str(), "added" | "visible" | "configured"))
    {
        return Err("This book has no stock signal (no `added`/`visible`/`configured` lines) — did you skip the remove-all + re-add in-game? Pick the fresh book.".to_string());
    }
    let owner = recount_owner(&shop_log, current_owner)?;
    let counted = recount_counts(&shop_log.entries);
    let in_stock: std::collections::BTreeSet<String> = {
        let conn = db
            .get()
            .map_err(|e| format!("Database connection error: {e}"))?;
        let mut stmt = conn
            .prepare(
                "SELECT item, event_at, action, quantity, price_unit, price_total
                 FROM stall_events
                 WHERE owner = ?1 AND ignored = 0 AND item IS NOT NULL AND event_at IS NOT NULL
                   AND action != 'unknown' AND action != 'collected'
                 ORDER BY event_at ASC, id ASC",
            )
            .map_err(|e| format!("Failed to prepare inventory query: {e}"))?;
        let events = stmt
            .query_map(rusqlite::params![owner], |row| {
                Ok(InventoryEvent {
                    item: row.get::<_, String>(0)?,
                    event_at: row.get::<_, String>(1)?,
                    action: row.get::<_, String>(2)?,
                    quantity: row.get::<_, i64>(3)?,
                    price_unit: row.get::<_, Option<f64>>(4)?,
                    price_total: row.get::<_, Option<i64>>(5)?,
                })
            })
            .map_err(|e| format!("Inventory query failed: {e}"))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("Failed to read inventory results: {e}"))?;
        let mut bstmt = conn
            .prepare(
                "SELECT item, quantity, price_unit, recorded_at FROM stall_inventory_baseline \
                 WHERE owner = ?1",
            )
            .map_err(|e| format!("Failed to prepare baseline query: {e}"))?;
        let rows = bstmt
            .query_map(rusqlite::params![owner], |row| {
                Ok(BaselineSeed {
                    item: row.get::<_, String>(0)?,
                    quantity: row.get::<_, i64>(1)?,
                    price_unit: row.get::<_, Option<f64>>(2)?,
                    recorded_at: row.get::<_, String>(3)?,
                })
            })
            .map_err(|e| format!("Baseline query failed: {e}"))?;
        let baselines: Vec<BaselineSeed> = rows
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("Failed to read baselines: {e}"))?;
        let result =
            crate::stall_aggregations::aggregate_inventory_with_baselines(events, 1, baselines);
        result
            .items
            .into_iter()
            .filter(|i| i.quantity > 0)
            .map(|i| i.item)
            .collect()
    };
    let counted_names: std::collections::BTreeSet<String> =
        counted.iter().map(|l| l.item.clone()).collect();
    let zeroed: Vec<String> = in_stock.difference(&counted_names).cloned().collect();
    // Newest entry timestamp in the book (`YYYY-MM-DD HH:MM:SS` strings
    // compare chronologically): apply stamps `min(now, this)` so post-book
    // events are not silently superseded (F2).
    let book_latest_at = shop_log
        .entries
        .iter()
        .filter_map(|e| e.event_at.clone())
        .max();
    Ok(RecountPreview {
        owner,
        counted,
        zeroed,
        book_entries: shop_log.entries.len(),
        content_hash: book_content_hash(content),
        book_latest_at,
    })
}

/// Apply a recount previewed by `preview_stall_recount`: upsert one baseline
/// per counted item (fresh-book quantity + price) and one qty-0 baseline per
/// zeroed item, in one BEGIN IMMEDIATE transaction (pool-is-the-lock: all or
/// nothing). Event rows are never touched — sales history survives intact.
/// Returns the number of baseline rows written. `recorded_at` stamps
/// `min(now, book_latest_at)`: the counts describe the book's snapshot, so
/// post-book events stay in the replay (a now-stamp would silently drop them).
/// The frontend echoes back the `RecountPreview` it confirmed
/// (`expected_hash` + `confirmed_counted`/`confirmed_zeroed`): apply
/// re-reads the book, aborts when the content hash differs (rewritten book
/// since preview) or when the recomputed sets differ from the confirmed
/// ones (DB drift while the dialog sat open), and only then writes.
#[tauri::command]
pub fn apply_stall_recount(
    db: State<'_, DbPool>,
    path: String,
    current_owner: Option<String>,
    expected_hash: String,
    confirmed_counted: Vec<RecountLine>,
    confirmed_zeroed: Vec<String>,
) -> Result<usize, String> {
    // TOCTOU guard: the preview's confirm dialog lists exact names, so the
    // file it was built from must be byte-identical at apply time. The game
    // rewrites the book on every in-game save; a changed fingerprint means
    // the confirmed list no longer matches — abort and ask for a re-preview
    // rather than zeroing items the user never saw. The stat→read→stat
    // check below only covers the apply-time read itself; the content-hash
    // compare ties this apply to the exact bytes the preview (and its
    // confirm dialog) was built from.
    let path_buf = std::path::PathBuf::from(&path);
    let before = book_fingerprint(&path_buf)?;
    let content = std::fs::read_to_string(&path_buf)
        .map_err(|e| format!("Failed to read shop log file: {e}"))?;
    let after = book_fingerprint(&path_buf)?;
    if before != after {
        return Err(
            "Shop log book changed during preview — re-run the recount preview before applying."
                .to_string(),
        );
    }
    if book_content_hash(&content) != expected_hash {
        return Err(
            "Shop log book changed since preview — re-run the recount preview before applying."
                .to_string(),
        );
    }
    let preview = preview_from_content(&db, &path_buf, &content, &current_owner)?;
    // Confirm-set guard: a Books-watcher ingest (or a baseline write) that
    // landed while the confirm sat open shifts the in-stock set, so the
    // recomputed `zeroed`/`counted` can differ from what the user saw.
    // Abort without writing rather than zeroing an item the dialog listed
    // as counted (or counting one it listed as zeroed). Multiset compare
    // (sorted triples incl. price): a bare set + `any` match lets duplicate
    // items collapse onto one fresh row.
    fn counted_key<'a>(lines: &'a [RecountLine]) -> Vec<(&'a str, i64, Option<u64>)> {
        let mut v: Vec<(&'a str, i64, Option<u64>)> = lines
            .iter()
            .map(|l| (l.item.as_str(), l.quantity, l.price_unit.map(f64::to_bits)))
            .collect();
        v.sort();
        v
    }
    if counted_key(&confirmed_counted) != counted_key(&preview.counted) {
        return Err(
            "Stall changed since preview — re-run the recount preview before applying."
                .to_string(),
        );
    }
    let zeroed_match = confirmed_zeroed.iter().collect::<std::collections::BTreeSet<_>>()
        == preview.zeroed.iter().collect::<std::collections::BTreeSet<_>>();
    if !zeroed_match {
        return Err(
            "Stall changed since preview — re-run the recount preview before applying."
                .to_string(),
        );
    }
    // One BEGIN IMMEDIATE transaction for the whole write set (pool-is-the-
    // lock invariant, same shape as `insert_stall_events` / Clear above):
    // either every baseline commits or none does — a mid-loop failure can
    // never leave a half-recounted stall.
    let mut conn = db
        .get_write()
        .map_err(|e| format!("Database connection error: {e}"))?;
    let tx = conn
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .map_err(|e| format!("Failed to begin transaction: {e}"))?;
    let mut written = 0usize;
    // `recorded_at = min(now, book_latest_at)`: the counts describe the
    // book's snapshot, so post-book events (book_latest, now] stay in the
    // replay instead of being silently superseded by a now-stamp (F2).
    let stamp = preview.book_latest_at.as_deref();
    for line in &preview.counted {
        save_stall_baseline_internal(
            &tx,
            &StallBaselineInput {
                owner: Some(preview.owner.clone()),
                item: Some(line.item.clone()),
                quantity: Some(line.quantity),
                price_unit: line.price_unit,
                note: Some("recount".to_string()),
            },
            stamp,
        )?;
        written += 1;
    }
    for item in &preview.zeroed {
        save_stall_baseline_internal(
            &tx,
            &StallBaselineInput {
                owner: Some(preview.owner.clone()),
                item: Some(item.clone()),
                quantity: Some(0),
                price_unit: None,
                note: Some("recount".to_string()),
            },
            stamp,
        )?;
        written += 1;
    }
    tx.commit()
        .map_err(|e| format!("Failed to commit recount: {e}"))?;
    Ok(written)
}
/// Write one file per `(owner, year, month, day)` group, sorted newest-first
/// to match the in-game book format. Round-trip contract: any file produced
/// by Export must be re-importable by Import, landing identical rows
/// (same batch key + same indexes → the fullest-witness rule ties newest
/// and the row count never grows).
#[tauri::command]
pub fn export_shop_log_files(
    db: State<'_, DbPool>,
    directory: String,
    owner: Option<String>,
) -> Result<ExportResult, String> {
    let owner = opt_nonempty(&owner)
        .ok_or_else(|| "export_shop_log_files requires an owner".to_string())?
        .to_string();

    let dir = std::path::PathBuf::from(&directory);
    std::fs::create_dir_all(&dir).map_err(|e| format!("Failed to create export directory: {e}"))?;

    let conn = db
        .get()
        .map_err(|e| format!("Database connection error: {e}"))?;

    // Fetch every non-unknown event for the owner with a resolved event_at.
    // event_at + entry_index ordering matches the in-game book order
    // (newest-first). The grouping below partitions by date.
    let mut stmt = conn
        .prepare(
            "SELECT event_timestamp, event_at, raw_message, entry_index \
             FROM stall_events \
             WHERE owner = ?1 AND action != 'unknown' AND event_at IS NOT NULL \
             ORDER BY event_at DESC, entry_index DESC, id DESC",
        )
        .map_err(|e| format!("Failed to prepare export query: {e}"))?;

    let rows = stmt
        .query_map(rusqlite::params![owner], |row| {
            Ok((
                row.get::<_, String>(0)?, // event_timestamp (raw "Mon Apr 13 14:29")
                row.get::<_, String>(1)?, // event_at ("YYYY-MM-DD HH:MM:SS")
                row.get::<_, String>(2)?, // raw_message
                row.get::<_, i64>(3)?,    // entry_index
            ))
        })
        .map_err(|e| format!("Export query failed: {e}"))?;

    // Group by date. BTreeMap keeps groups sorted; within a group we
    // preserve insertion order, which is already newest-first from the SQL.
    let mut by_date: BTreeMap<String, Vec<(String, String, i64)>> = BTreeMap::new();
    let mut total = 0usize;
    for row in rows {
        let (ts, event_at, msg, idx) = row.map_err(|e| format!("Failed to read row: {e}"))?;
        let date = event_at.get(..10).unwrap_or("unknown").to_string();
        by_date.entry(date).or_default().push((ts, msg, idx));
        total += 1;
    }

    let safe_owner = sanitize_owner(&owner);
    let mut files_written = 0usize;
    for (date, entries) in by_date {
        let file_path = dir.join(format!("{safe_owner}-shop-log-{date}.txt"));
        let mut body = String::new();
        for (i, (ts, msg, _idx)) in entries.iter().enumerate() {
            if i > 0 {
                body.push_str("\n\n");
            }
            body.push_str(ts);
            body.push_str(" - ");
            body.push_str(msg);
        }
        body.push('\n');
        std::fs::write(&file_path, body)
            .map_err(|e| format!("Failed to write {}: {e}", file_path.display()))?;
        files_written += 1;
    }

    Ok(ExportResult {
        files_written,
        events_exported: total,
    })
}

/// Dev-only bulk insert for benchmarking at 10k / 100k scale. Not registered
/// in any user-facing UI — the frontend exposes it via `window.stallBench`
/// for DevTools-driven scale testing per plan §14.5.
///
/// Generates a realistic-looking mix of `bought` / `added` / `visible` /
/// `configured` / `removed` events spread across the past year, attributed
/// to the active character. Uses the same `insert_stall_events` helper as
/// live ingest so the dedup behavior is identical.
#[tauri::command]
pub fn seed_stall_events_dev(
    db: State<'_, DbPool>,
    count: usize,
    owner: Option<String>,
) -> Result<usize, String> {
    let owner = opt_nonempty(&owner)
        .ok_or_else(|| "seed_stall_events_dev requires an owner".to_string())?
        .to_string();

    // Fixed pools to keep the seeded data realistic without any
    // randomness library — the synthetic distribution is deterministic
    // (modulo arithmetic), which makes scale-test runs reproducible.
    const ITEMS: &[(&str, i64)] = &[
        ("Quality Reins", 4500),
        ("Mystic Saddlebag", 40000),
        ("Nice Saddle", 4000),
        ("Decent Horseshoes", 3500),
        ("Amazing Horseshoes", 6000),
        ("Orcish Spell Pouch", 450),
        ("Barley Seeds", 100),
        ("Iron Shortsword", 8500),
        ("Sturdy Belt", 2200),
        ("Cured Pelt", 950),
    ];
    const BUYERS: &[&str] = &[
        "MrBonq",
        "AlestiarWolf",
        "Zangariel",
        "Brynn",
        "Kork",
        "Ashbringer",
        "Vexis",
        "Tomlin",
        "Sylvae",
        "Galadwen",
    ];

    let now = chrono::Local::now();
    let mut inputs: Vec<StallEventInput> = Vec::with_capacity(count);

    for i in 0..count {
        // Spread events across the past 365 days, ~once every few minutes
        // so a 100k seed lands a single event roughly every 5 minutes.
        let minutes_ago = (i as i64) * 5;
        let dt = now - chrono::Duration::minutes(minutes_ago);
        let event_at = dt.format("%Y-%m-%d %H:%M:%S").to_string();
        let event_timestamp = dt.format("%a %b %-d %H:%M").to_string();

        let (item_name, base_price) = ITEMS[i % ITEMS.len()];
        let buyer = BUYERS[i % BUYERS.len()];
        // Cycle through actions with a realistic distribution.
        let action_pick = i % 10;
        let (action, player, item, quantity, price_unit, price_total, raw): (
            &str,
            String,
            Option<String>,
            i64,
            Option<f64>,
            Option<i64>,
            String,
        ) = match action_pick {
            0..=5 => (
                "bought",
                buyer.to_string(),
                Some(item_name.to_string()),
                1,
                Some(base_price as f64),
                Some(base_price),
                format!(
                    "{buyer} bought {item_name} at a cost of {base_price} per 1 = {base_price}"
                ),
            ),
            6 => (
                "added",
                owner.clone(),
                Some(item_name.to_string()),
                1,
                None,
                None,
                format!("{owner} added {item_name} to shop"),
            ),
            7 => (
                "visible",
                owner.clone(),
                Some(item_name.to_string()),
                1,
                Some(base_price as f64),
                None,
                format!("{owner} made {item_name} visible in shop at a cost of {base_price} per 1"),
            ),
            8 => (
                "configured",
                owner.clone(),
                Some(item_name.to_string()),
                1,
                Some(base_price as f64),
                None,
                format!("{owner} configured {item_name} to cost {base_price} per 1."),
            ),
            _ => (
                "removed",
                owner.clone(),
                Some(item_name.to_string()),
                1,
                None,
                None,
                format!("{owner} removed {item_name} from shop"),
            ),
        };

        inputs.push(StallEventInput {
            event_timestamp,
            event_at: Some(event_at),
            log_timestamp: "seeded".to_string(),
            log_title: "Seeded".to_string(),
            action: action.to_string(),
            player,
            owner: Some(owner.clone()),
            item,
            quantity,
            price_unit,
            price_total,
            raw_message: raw,
            // Use `i` as the entry_index so the witness rule ties newest on
            // re-seeding (same batch, same count) without a clear in between.
            entry_index: i as i64,
        });
    }

    insert_stall_events(&db, &inputs)
}

/// Manual "Check for new logs" command: one stateless sweep over
/// `Books/PlayerShopLog_*.txt` for the active character. The Books watcher
/// owns its mtime map on its thread and offers no shared handle, so this
/// command re-runs the same `process_book_file` ingest statelessly — the
/// fullest-witness rule ties newest on already-seen files, so reprocessing
/// them is a row-count no-op.
///
/// `async` + `spawn_blocking`: the sweep stats + reads every book file, which
/// can stall the main thread on a cold disk (statehelm `backfill_statehelm_gifts`
/// precedent). Returns `(files_scanned, new_events)`.
#[tauri::command]
pub async fn rescan_stall_books(app: AppHandle) -> Result<(usize, usize), String> {
    let settings = app
        .try_state::<Arc<SettingsManager>>()
        .map(|s| s.inner().clone())
        .ok_or_else(|| "SettingsManager not managed yet".to_string())?;
    let db = app
        .try_state::<DbPool>()
        .map(|s| s.inner().clone())
        .ok_or_else(|| "DbPool not managed yet".to_string())?;
    tauri::async_runtime::spawn_blocking(move || rescan_books_once(&settings, &db, &app))
        .await
        .map_err(|e| format!("Rescan task failed: {e}"))?
}

/// One manual opening-stock row: what the user counted in their stall for one
/// item. `quantity` is the counted stock, `price_unit` the current per-unit
/// listing price (None = unpriced). Upserts on `UNIQUE(owner, item)` so
/// re-counting the same item replaces the old row instead of duplicating it.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StallBaselineInput {
    pub owner: Option<String>,
    pub item: Option<String>,
    pub quantity: Option<i64>,
    pub price_unit: Option<f64>,
    pub note: Option<String>,
}

/// The stored baseline row returned to the frontend.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StallBaseline {
    pub id: i64,
    pub owner: String,
    pub item: String,
    pub quantity: i64,
    pub price_unit: Option<f64>,
    pub recorded_at: String,
    pub note: Option<String>,
}

/// Upsert one baseline row for the active character. `quantity == 0` stores a
/// zero row (counted to zero = item cleared) and returns it; negative
/// quantities are rejected, never stored.
#[tauri::command]
pub fn save_stall_baseline(
    db: State<'_, DbPool>,
    input: StallBaselineInput,
) -> Result<Option<StallBaseline>, String> {
    let conn = db
        .get_write()
        .map_err(|e| format!("Database connection error: {e}"))?;
    save_stall_baseline_internal(&conn, &input, None)
}

/// Validated upsert behind `save_stall_baseline` and `apply_stall_recount`.
/// Takes a plain `Connection` so tests exercise the real SQL without a
/// Tauri `State`. `recorded_at_override` (a `YYYY-MM-DD HH:MM:SS` stamp)
/// replaces the now-stamp when `Some`: the recount passes
/// `min(now, book_latest_at)` so post-book events are not silently
/// superseded. The public command always stamps now (`None`).
fn save_stall_baseline_internal(
    conn: &rusqlite::Connection,
    input: &StallBaselineInput,
    recorded_at_override: Option<&str>,
) -> Result<Option<StallBaseline>, String> {
    let owner = opt_nonempty(&input.owner)
        .ok_or_else(|| "save_stall_baseline requires an owner".to_string())?
        .to_string();
    let item = opt_nonempty(&input.item)
        .ok_or_else(|| "save_stall_baseline requires an item name".to_string())?
        .to_string();
    let quantity = input
        .quantity
        .ok_or_else(|| "save_stall_baseline requires a quantity".to_string())?;
    if !(0..=1_000_000).contains(&quantity) {
        return Err("save_stall_baseline quantity out of range (0-1000000)".to_string());
    }
    if let Some(p) = input.price_unit
        && !(0.0..=1_000_000_000.0).contains(&p) {
            return Err("save_stall_baseline price out of range".to_string());
        }

    // qty 0 stores a zero row (not a DELETE): the zero seed makes the
    // aggregator drop all older events for the item and report quantity 0,
    // which is how Clear-unsure evicts guessed stock. `recorded_at` still
    // stamps now so only pre-clear history is superseded.

    // `recorded_at` stamps every upsert with now (localtime, matching the
    // column default): a re-count supersedes history up to *now*, not up to
    // the original count. `note` clears on each save unless re-supplied —
    // stale notes ("counted Tuesday") would otherwise outlive their count.
    // The recount override replaces now with the book's newest entry time
    // (validated `YYYY-MM-DD HH:MM:SS` shape, else now): counts describe the
    // book's snapshot, so the stamp must too — otherwise events between the
    // snapshot and apply vanish from the replay.
    let now_now: String = conn
        .query_row(
            "SELECT strftime('%Y-%m-%d %H:%M:%S', 'now', 'localtime')",
            [],
            |r| r.get(0),
        )
        .map_err(|e| format!("Baseline clock read failed: {e}"))?;
    let stamp_is_sane = |s: &str| {
        s.len() == 19
            && s.as_bytes()[4] == b'-'
            && s.as_bytes()[7] == b'-'
            && s.as_bytes()[10] == b' '
            && s.as_bytes()[13] == b':'
            && s.as_bytes()[16] == b':'
    };
    let recorded_at: String = match recorded_at_override {
        Some(s) => {
            if stamp_is_sane(s) && s < now_now.as_str() {
                s.to_string()
            } else {
                now_now
            }
        }
        None => now_now,
    };
    conn.execute(
        "INSERT INTO stall_inventory_baseline (owner, item, quantity, price_unit, recorded_at, note)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)
         ON CONFLICT(owner, item) DO UPDATE SET
            quantity = excluded.quantity,
            price_unit = excluded.price_unit,
            recorded_at = excluded.recorded_at,
            note = excluded.note",
        rusqlite::params![owner, item, quantity, input.price_unit, recorded_at, input.note],
    )
    .map_err(|e| format!("Baseline save failed: {e}"))?;

    conn.query_row(
        "SELECT id, owner, item, quantity, price_unit, recorded_at, note FROM stall_inventory_baseline \
         WHERE owner = ?1 AND item = ?2",
        rusqlite::params![owner, item],
        |row| {
            Ok(StallBaseline {
                id: row.get(0)?,
                owner: row.get(1)?,
                item: row.get(2)?,
                quantity: row.get(3)?,
                price_unit: row.get(4)?,
                recorded_at: row.get(5)?,
                note: row.get(6)?,
            })
        },
    )
    .map_err(|e| format!("Baseline readback failed: {e}"))
    .map(Some)
}

/// List every baseline row for the active character, newest-recorded first.
/// Empty owner returns no rows (never cross-character data).
#[tauri::command]
pub fn get_stall_baselines(
    db: State<'_, DbPool>,
    owner: Option<String>,
) -> Result<Vec<StallBaseline>, String> {
    let Some(owner) = opt_nonempty(&owner).map(str::to_string) else {
        return Ok(Vec::new());
    };
    let conn = db
        .get()
        .map_err(|e| format!("Database connection error: {e}"))?;
    let mut stmt = conn
        .prepare(
            "SELECT id, owner, item, quantity, price_unit, recorded_at, note FROM stall_inventory_baseline \
             WHERE owner = ?1 ORDER BY recorded_at DESC, item ASC",
        )
        .map_err(|e| format!("Failed to prepare baselines query: {e}"))?;
    // Intermediate `rows` binding: the `MappedRows` temporary must drop
    // before `stmt` (same shape as the inventory baseline block above).
    let rows = stmt
        .query_map(rusqlite::params![owner], |row| {
            Ok(StallBaseline {
                id: row.get(0)?,
                owner: row.get(1)?,
                item: row.get(2)?,
                quantity: row.get(3)?,
                price_unit: row.get(4)?,
                recorded_at: row.get(5)?,
                note: row.get(6)?,
            })
        })
        .map_err(|e| format!("Baselines query failed: {e}"))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("Failed to read baselines: {e}"))
}

/// Delete one baseline row by id, scoped to the owner so a stale id can never
/// clear another character's count. Returns rows deleted (0 or 1).
#[tauri::command]
pub fn delete_stall_baseline(
    db: State<'_, DbPool>,
    id: i64,
    owner: Option<String>,
) -> Result<usize, String> {
    let Some(owner) = opt_nonempty(&owner).map(str::to_string) else {
        return Err("delete_stall_baseline requires an owner".to_string());
    };
    let conn = db
        .get_write()
        .map_err(|e| format!("Database connection error: {e}"))?;
    conn.execute(
        "DELETE FROM stall_inventory_baseline WHERE id = ?1 AND owner = ?2",
        rusqlite::params![id, owner],
    )
    .map_err(|e| format!("Baseline delete failed: {e}"))
}

/// Delete every manual count for one character WITHOUT touching shop-log
/// events. The Inventory tab reverts to pure log estimates: all `added`,
/// `visible`, `bought`, and `removed` events replay with no timestamp
/// cutoff, so a fresh log rebuilds today's stock from its `added` lines.
/// Requires non-empty owner — never a cross-character wipe.
#[tauri::command]
pub fn delete_all_stall_baselines(
    db: State<'_, DbPool>,
    owner: Option<String>,
) -> Result<usize, String> {
    let Some(owner) = opt_nonempty(&owner).map(str::to_string) else {
        return Err("delete_all_stall_baselines requires an owner".to_string());
    };
    let conn = db
        .get_write()
        .map_err(|e| format!("Database connection error: {e}"))?;
    delete_all_stall_baselines_internal(&conn, &owner)
}

/// Owner-scoped bulk delete behind `delete_all_stall_baselines`. Takes a
/// plain `Connection` so tests exercise the real predicate without a Tauri
/// `State` (same shape as `save_stall_baseline_internal`).
fn delete_all_stall_baselines_internal(
    conn: &rusqlite::Connection,
    owner: &str,
) -> Result<usize, String> {
    conn.execute(
        "DELETE FROM stall_inventory_baseline WHERE owner = ?1",
        rusqlite::params![owner],
    )
    .map_err(|e| format!("Baseline delete failed: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Local;
    use std::path::Path;

    #[test]
    fn year_from_filename_extracts_iso_year() {
        assert_eq!(
            year_from_filename(Path::new("Deradon-shop-log-2026-04-13.txt")),
            2026
        );
    }

    #[test]
    fn year_from_filename_falls_back_to_current_year() {
        let now = Local::now().year();
        assert_eq!(year_from_filename(Path::new("nodate.txt")), now);
    }

    #[test]
    fn year_from_filename_rejects_bare_5_digit_runs() {
        // 12026 should not be read as 2026 — it's part of a longer number.
        let now = Local::now().year();
        assert_eq!(year_from_filename(Path::new("export-12026.txt")), now);
    }

    #[test]
    fn year_from_filename_rejects_year_outside_2000_2099() {
        let now = Local::now().year();
        assert_eq!(year_from_filename(Path::new("file-1999.txt")), now);
        assert_eq!(year_from_filename(Path::new("file-2100.txt")), now);
    }

    #[test]
    fn sanitize_owner_replaces_path_separators() {
        assert_eq!(sanitize_owner("foo/bar"), "foo_bar");
        assert_eq!(sanitize_owner("foo\\bar"), "foo_bar");
        assert_eq!(sanitize_owner("normal"), "normal");
        assert_eq!(sanitize_owner("with:colon"), "with_colon");
    }

    #[test]
    fn build_filter_where_single_day_range_produces_inclusive_bounds() {
        // A single-day filter (date_from == date_to) should produce both a
        // 00:00:00 lower bound and a 23:59:59 upper bound, matching all
        // events that occurred on that calendar day. Common user pattern:
        // "show me yesterday's sales".
        let filters = StallEventsFilters {
            owner: Some("Deradon".to_string()),
            date_from: Some("2026-04-13".to_string()),
            date_to: Some("2026-04-13".to_string()),
            ..Default::default()
        };
        let (sql, params) = build_filter_where(&filters, None).expect("filter should build");
        assert!(sql.contains("event_at >= ?"));
        assert!(sql.contains("event_at <= ?"));
        // Find the from / to values in the bound params.
        let strings: Vec<&str> = params
            .iter()
            .filter_map(|v| match v {
                rusqlite::types::Value::Text(s) => Some(s.as_str()),
                _ => None,
            })
            .collect();
        assert!(strings.contains(&"2026-04-13 00:00:00"));
        assert!(strings.contains(&"2026-04-13 23:59:59"));
    }

    #[test]
    fn build_filter_where_invalid_date_format_returns_none() {
        // The 10-char length guard rejects malformed dates loudly via empty
        // results rather than producing the malformed bound
        // "2026-04-13 14:29:00 00:00:00". Failure mode: empty result, not
        // bad SQL.
        let filters = StallEventsFilters {
            owner: Some("Deradon".to_string()),
            date_from: Some("2026-04-13 14:29:00".to_string()),
            ..Default::default()
        };
        assert!(build_filter_where(&filters, None).is_none());
    }

    /// Round-trip integration test (no DB, no Tauri State): build a body
    /// in the export format, parse it back through the parser, verify the
    /// inputs survive byte-for-byte. Catches any drift between the export
    /// body builder and `parse_shop_log`'s ENTRY_RE.
    #[test]
    fn export_format_roundtrips_through_parser() {
        use crate::shop_log_parser::parse_shop_log;

        // These mirror what export_shop_log_files writes for one date group:
        // entries newest-first, separated by `\n\n`, trailing `\n`.
        let entries = [
            (
                "Wed Apr 15 11:45",
                "MrBonq bought Quality Reins at a cost of 4500 per 1 = 4500",
            ),
            (
                "Tue Apr 14 10:30",
                "Deradon made Quality Reins visible in shop at a cost of 4500 per 1",
            ),
            ("Mon Apr 13 09:00", "Deradon added Quality Reins to shop"),
        ];
        let mut body = String::new();
        for (i, (ts, msg)) in entries.iter().enumerate() {
            if i > 0 {
                body.push_str("\n\n");
            }
            body.push_str(ts);
            body.push_str(" - ");
            body.push_str(msg);
        }
        body.push('\n');

        // Parse the body back through parse_shop_log.
        let parsed = parse_shop_log("Imported", &body, "imported", 2026);
        assert_eq!(parsed.entries.len(), 3);
        // The parser reverses, so index 0 is the OLDEST.
        assert_eq!(parsed.entries[0].action, "added");
        assert_eq!(parsed.entries[1].action, "visible");
        assert_eq!(parsed.entries[2].action, "bought");
        // Owner detection survives the round trip.
        assert_eq!(parsed.owner.as_deref(), Some("Deradon"));
        // event_at resolution survives.
        assert_eq!(
            parsed.entries[0].event_at.as_deref(),
            Some("2026-04-13 09:00:00")
        );
        assert_eq!(
            parsed.entries[2].event_at.as_deref(),
            Some("2026-04-15 11:45:00")
        );
    }

    fn baseline_pool() -> crate::db::DbPool {
        // Shared-cache memory URI: both pool legs hit the same in-memory DB.
        // Unique name per call keeps parallel tests isolated.
        use std::sync::atomic::{AtomicU64, Ordering};
        static SEQ: AtomicU64 = AtomicU64::new(0);
        let n = SEQ.fetch_add(1, Ordering::Relaxed);
        let mk = |query_only: bool| {
            r2d2_sqlite::SqliteConnectionManager::file(format!(
                "file:stall_baseline_test_{n}?mode=memory&cache=shared"
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
            .expect("writes pool");
        let reads = r2d2::Pool::builder()
            .max_size(2)
            .build(mk(true))
            .expect("reads pool");
        let conn = writes.get().expect("conn");
        crate::db::migrations::run_migrations(&conn, None).expect("migrations");
        drop(conn);
        crate::db::DbPool::from_pools(reads, writes)
    }

    fn baseline_input(owner: &str, item: &str, qty: i64) -> StallBaselineInput {
        StallBaselineInput {
            owner: Some(owner.to_string()),
            item: Some(item.to_string()),
            quantity: Some(qty),
            price_unit: Some(4500.0),
            note: None,
        }
    }

    #[test]
    fn baseline_upsert_replaces_same_item() {
        let db = baseline_pool();
        let conn = db.get_write().expect("write conn");
        let first = save_stall_baseline_internal(&conn, &baseline_input("Deradon", "Reins", 5), None)
            .expect("save")
            .expect("row");
        assert_eq!(first.quantity, 5);
        let second = save_stall_baseline_internal(&conn, &baseline_input("Deradon", "Reins", 3), None)
            .expect("re-save")
            .expect("row");
        assert_eq!(second.quantity, 3);
        assert_eq!(first.id, second.id, "upsert keeps the same row");
        drop(conn);
        let count: i64 = db
            .get()
            .expect("read conn")
            .query_row(
                "SELECT COUNT(*) FROM stall_inventory_baseline WHERE owner = 'Deradon'",
                [],
                |r| r.get(0),
            )
            .expect("count");
        assert_eq!(count, 1);
    }

    #[test]
    fn baseline_recorded_at_override_stamps_book_snapshot() {
        // F2: the recount stamps `min(now, book_latest_at)` so post-book
        // events stay in the replay. An old-but-sane stamp wins; a future
        // stamp and garbage fall back to now.
        let db = baseline_pool();
        let conn = db.get_write().expect("write conn");
        let old = save_stall_baseline_internal(
            &conn,
            &baseline_input("Deradon", "Reins", 5),
            Some("2026-11-01 10:00:00"),
        )
        .expect("save")
        .expect("row");
        assert_eq!(old.recorded_at, "2026-11-01 10:00:00", "sane past stamp wins");
        let now_row = save_stall_baseline_internal(
            &conn,
            &baseline_input("Deradon", "Saddle", 2),
            Some("2999-01-01 00:00:00"),
        )
        .expect("save")
        .expect("row");
        assert_ne!(now_row.recorded_at, "2999-01-01 00:00:00", "future stamp falls back to now");
        let garbage = save_stall_baseline_internal(
            &conn,
            &baseline_input("Deradon", "Belt", 1),
            Some("not-a-stamp"),
        )
        .expect("save")
        .expect("row");
        assert_ne!(garbage.recorded_at, "not-a-stamp", "garbage falls back to now");
    }

    #[test]
    fn baseline_zero_quantity_stores_zero_row() {
        let db = baseline_pool();
        let conn = db.get_write().expect("write conn");
        save_stall_baseline_internal(&conn, &baseline_input("Deradon", "Reins", 5), None).expect("save");
        // qty 0 = cleared: stored as a zero row (not deleted) so the
        // aggregator supersedes pre-clear history for the item.
        let cleared =
            save_stall_baseline_internal(&conn, &baseline_input("Deradon", "Reins", 0), None)
                .expect("zero-out")
                .expect("zero row stored");
        assert_eq!(cleared.quantity, 0);
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM stall_inventory_baseline WHERE owner = 'Deradon'",
                [],
                |r| r.get(0),
            )
            .expect("count");
        assert_eq!(count, 1);
    }

    #[test]
    fn baselines_are_owner_scoped() {
        let db = baseline_pool();
        let conn = db.get_write().expect("write conn");
        save_stall_baseline_internal(&conn, &baseline_input("Deradon", "Reins", 5), None).expect("save");
        save_stall_baseline_internal(&conn, &baseline_input("Alvida", "Reins", 2), None).expect("save");
        // Wrong-owner delete is a no-op (same predicate the command uses).
        let deleted = conn
            .execute(
                "DELETE FROM stall_inventory_baseline WHERE id = 1 AND owner = 'Alvida'",
                [],
            )
            .expect("delete");
        assert_eq!(deleted, 0);
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM stall_inventory_baseline",
                [],
                |r| r.get(0),
            )
            .expect("count");
        assert_eq!(count, 2);
    }
    #[test]
    fn delete_all_baselines_is_owner_scoped() {
        let db = baseline_pool();
        let conn = db.get_write().expect("write conn");
        save_stall_baseline_internal(&conn, &baseline_input("Deradon", "Reins", 5), None).expect("save");
        save_stall_baseline_internal(&conn, &baseline_input("Deradon", "Saddle", 2), None).expect("save");
        save_stall_baseline_internal(&conn, &baseline_input("Alvida", "Reins", 7), None).expect("save");
        // Same helper the command uses: one owner's rows go, the other's stay.
        let deleted = delete_all_stall_baselines_internal(&conn, "Deradon").expect("delete");
        assert_eq!(deleted, 2);
        let remaining: Vec<(String, String)> = conn
            .prepare("SELECT owner, item FROM stall_inventory_baseline")
            .expect("prepare")
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
            .expect("query")
            .collect::<Result<Vec<_>, _>>()
            .expect("collect");
        assert_eq!(remaining, vec![("Alvida".to_string(), "Reins".to_string())]);
    }

    #[test]
    fn insert_reingest_overlapping_book_replaces_witness() {
        // Live 2026-11 shape: the same log line ("Mon Apr 20 08:55 - removed
        // Kilderkin") sits in two overlapping books at different positions,
        // so it parses to a different entry_index in each. A full-overlap
        // re-ingest (1 copy in, max 1 stored) becomes the witness: the old
        // row is replaced, the genuinely new line inserts. Same-batch
        // repeats are all kept: Deradon's 8x same-minute bulk buys are 8
        // real occurrences, not a re-ingest.
        let db = baseline_pool();
        fn input(
            ts: &str,
            msg: &str,
            idx: i64,
            title: &str,
            log_ts: &str,
        ) -> StallEventInput {
            StallEventInput {
                event_timestamp: ts.to_string(),
                event_at: Some("2026-04-20 08:55:00".to_string()),
                log_timestamp: log_ts.to_string(),
                log_title: title.to_string(),
                action: "removed".to_string(),
                player: "Deradon".to_string(),
                owner: Some("Deradon".to_string()),
                item: Some("Kilderkin".to_string()),
                quantity: 1,
                price_unit: None,
                price_total: None,
                raw_message: msg.to_string(),
                entry_index: idx,
            }
        }
        let line = "Deradon removed Kilderkin from shop";
        // First book ingests the line at index 14.
        let first = vec![input("Mon Apr 20 08:55", line, 14, "Today's Shop Logs", "23:22:02")];
        assert_eq!(insert_stall_events(&db, &first).expect("first ingest"), 1);
        // Overlapping second book holds the same line at index 12, plus one
        // genuinely new line. The re-ingested occurrence replaces its row
        // (DELETE + INSERT) and returns net-new only: 1 for the genuinely
        // new line, 0 for the replacement.
        let second = vec![
            input("Mon Apr 20 08:55", line, 12, "Imported", "imported"),
            input("Mon Apr 20 08:56", "Deradon added Kilderkin to shop", 13, "Imported", "imported"),
        ];
        assert_eq!(insert_stall_events(&db, &second).expect("re-ingest"), 1);
        let count: i64 = db
            .get()
            .expect("read conn")
            .query_row(
                "SELECT COUNT(*) FROM stall_events WHERE owner = 'Deradon'",
                [],
                |r| r.get(0),
            )
            .expect("count");
        assert_eq!(count, 2);
        // Latest batch wins: the surviving row carries the re-ingest's index
        // and title, so a re-read line never multiplies.
        let (title, idx): (String, i64) = db
            .get()
            .expect("read conn")
            .query_row(
                "SELECT log_title, entry_index FROM stall_events WHERE raw_message = ?1",
                rusqlite::params![line],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .expect("row");
        assert_eq!(title, "Imported");
        assert_eq!(idx, 12);
    }

    #[test]
    fn insert_partial_reingest_inserts_nothing() {
        // Partial overlap: the earlier batch holds TWO copies of the line,
        // the re-ingest carries only ONE (1 < max-stored 2). Its occurrence
        // is already witnessed — insert nothing, delete nothing, so a
        // partial overlap can never drop real events.
        let db = baseline_pool();
        fn input(idx: i64, title: &str, log_ts: &str) -> StallEventInput {
            StallEventInput {
                event_timestamp: "Mon Apr 20 08:55".to_string(),
                event_at: Some("2026-04-20 08:55:00".to_string()),
                log_timestamp: log_ts.to_string(),
                log_title: title.to_string(),
                action: "removed".to_string(),
                player: "Deradon".to_string(),
                owner: Some("Deradon".to_string()),
                item: Some("Kilderkin".to_string()),
                quantity: 1,
                price_unit: None,
                price_total: None,
                raw_message: "Deradon removed Kilderkin from shop".to_string(),
                entry_index: idx,
            }
        }
        let first = vec![
            input(14, "Today's Shop Logs", "23:22:02"),
            input(15, "Today's Shop Logs", "23:22:02"),
        ];
        assert_eq!(insert_stall_events(&db, &first).expect("first ingest"), 2);
        let second = vec![input(12, "Imported", "imported")];
        assert_eq!(
            insert_stall_events(&db, &second).expect("re-ingest"),
            0,
            "partial re-ingest is fully witnessed: nothing inserted"
        );
        let rows: Vec<(String, i64)> = db
            .get()
            .expect("read conn")
            .prepare(
                "SELECT log_title, entry_index FROM stall_events
                 WHERE raw_message = 'Deradon removed Kilderkin from shop'
                 ORDER BY entry_index",
            )
            .expect("prepare")
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
            .expect("query")
            .collect::<Result<Vec<_>, _>>()
            .expect("collect");
        assert_eq!(
            rows,
            vec![
                ("Today's Shop Logs".to_string(), 14),
                ("Today's Shop Logs".to_string(), 15),
            ],
            "earlier batch untouched"
        );
    }

    #[test]
    fn insert_witness_never_deletes_other_owner_sale() {
        // Cross-owner guard: an identical `bought` line (buyer-named, so it
        // CAN match across alts) in Alt B's book must not delete Alt A's
        // real sale. Witness queries scope on the incoming batch's owner.
        let db = baseline_pool();
        fn input(owner: &str, idx: i64, title: &str, log_ts: &str) -> StallEventInput {
            StallEventInput {
                event_timestamp: "Mon Apr 20 08:55".to_string(),
                event_at: Some("2026-04-20 08:55:00".to_string()),
                log_timestamp: log_ts.to_string(),
                log_title: title.to_string(),
                action: "bought".to_string(),
                player: "MrBonq".to_string(),
                owner: Some(owner.to_string()),
                item: Some("Quality Reins".to_string()),
                quantity: 1,
                price_unit: Some(4500.0),
                price_total: Some(4500),
                raw_message: "MrBonq bought Quality Reins at a cost of 4500 per 1 = 4500".to_string(),
                entry_index: idx,
            }
        }
        assert_eq!(
            insert_stall_events(&db, &[input("Deradon", 14, "Today's Shop Logs", "23:22:02")])
                .expect("alt A ingest"),
            1
        );
        // Same line, other owner, other batch: first sighting FOR THAT OWNER.
        assert_eq!(
            insert_stall_events(&db, &[input("Alvida", 12, "Imported", "imported")])
                .expect("alt B ingest"),
            1,
            "other owner's sale inserts, never replaces"
        );
        let count: i64 = db
            .get()
            .expect("read conn")
            .query_row("SELECT COUNT(*) FROM stall_events", [], |r| r.get(0))
            .expect("count");
        assert_eq!(count, 2, "both alts keep their sale");
        // Same-owner re-ingest still witnesses (regression guard for the
        // scoping change): Alt A re-reads its line at a new index.
        assert_eq!(
            insert_stall_events(&db, &[input("Deradon", 9, "Imported", "imported")])
                .expect("alt A re-ingest"),
            0,
            "same-owner witness replace is net-new 0"
        );
        let alt_a: i64 = db
            .get()
            .expect("read conn")
            .query_row(
                "SELECT COUNT(*) FROM stall_events WHERE owner = 'Deradon'",
                [],
                |r| r.get(0),
            )
            .expect("count");
        assert_eq!(alt_a, 1, "alt A still holds exactly one row");
    }

    #[test]
    fn insert_first_sighting_collision_skips_row_keeps_batch() {
        // Narrowed guard (D round 2): Alt A holds the line at index 14; Alt
        // B's batch carries the same line at that SAME index as a FIRST
        // sighting (B holds no rows, so the witness DELETE removes nothing
        // of B's) plus a genuinely new line. The colliding row skips
        // (prior `INSERT OR IGNORE` behaviour); the rest of the batch
        // inserts — the call succeeds and later scans are NOT blocked.
        let db = baseline_pool();
        fn input(owner: &str, idx: i64, title: &str, log_ts: &str) -> StallEventInput {
            StallEventInput {
                event_timestamp: "Mon Apr 20 08:55".to_string(),
                event_at: Some("2026-04-20 08:55:00".to_string()),
                log_timestamp: log_ts.to_string(),
                log_title: title.to_string(),
                action: "bought".to_string(),
                player: "MrBonq".to_string(),
                owner: Some(owner.to_string()),
                item: Some("Quality Reins".to_string()),
                quantity: 1,
                price_unit: Some(4500.0),
                price_total: Some(4500),
                raw_message: "MrBonq bought Quality Reins at a cost of 4500 per 1 = 4500".to_string(),
                entry_index: idx,
            }
        }
        fn fresh(owner: &str) -> StallEventInput {
            StallEventInput {
                event_timestamp: "Mon Apr 20 08:56".to_string(),
                event_at: Some("2026-04-20 08:56:00".to_string()),
                log_timestamp: "imported".to_string(),
                log_title: "Imported".to_string(),
                action: "added".to_string(),
                player: "Deradon".to_string(),
                owner: Some(owner.to_string()),
                item: Some("Kilderkin".to_string()),
                quantity: 1,
                price_unit: None,
                price_total: None,
                raw_message: "Deradon added Kilderkin to shop".to_string(),
                entry_index: 20,
            }
        }
        assert_eq!(
            insert_stall_events(&db, &[input("Deradon", 14, "Today's Shop Logs", "23:22:02")])
                .expect("alt A ingest"),
            1
        );
        assert_eq!(
            insert_stall_events(
                &db,
                &[input("Alvida", 14, "Imported", "imported"), fresh("Alvida")]
            )
            .expect("first-sighting batch with one collision"),
            1,
            "colliding row skips, fresh row inserts"
        );
        let count: i64 = db
            .get()
            .expect("read conn")
            .query_row("SELECT COUNT(*) FROM stall_events", [], |r| r.get(0))
            .expect("count");
        assert_eq!(count, 2, "A's sale plus B's fresh row, no dupe");
        let kept_owner: String = db
            .get()
            .expect("read conn")
            .query_row(
                "SELECT owner FROM stall_events WHERE entry_index = 14",
                [],
                |r| r.get(0),
            )
            .expect("row");
        assert_eq!(kept_owner, "Deradon", "alt A's sale survives");
    }

    #[test]
    fn insert_witness_refuses_cross_owner_index_collision_after_own_delete() {
        // Abort arm: BOTH alts hold the line (A at 14, B at 12). B
        // re-ingests at A's index 14: the witness DELETE removes B's own old
        // row, then the slot is held by A — the call aborts via rollback
        // with both rows intact (nothing committed).
        let db = baseline_pool();
        fn input(owner: &str, idx: i64, title: &str, log_ts: &str) -> StallEventInput {
            StallEventInput {
                event_timestamp: "Mon Apr 20 08:55".to_string(),
                event_at: Some("2026-04-20 08:55:00".to_string()),
                log_timestamp: log_ts.to_string(),
                log_title: title.to_string(),
                action: "bought".to_string(),
                player: "MrBonq".to_string(),
                owner: Some(owner.to_string()),
                item: Some("Quality Reins".to_string()),
                quantity: 1,
                price_unit: Some(4500.0),
                price_total: Some(4500),
                raw_message: "MrBonq bought Quality Reins at a cost of 4500 per 1 = 4500".to_string(),
                entry_index: idx,
            }
        }
        assert_eq!(
            insert_stall_events(&db, &[input("Deradon", 14, "Today's Shop Logs", "23:22:02")])
                .expect("alt A ingest"),
            1
        );
        assert_eq!(
            insert_stall_events(&db, &[input("Alvida", 12, "Other Book", "22:00:00")])
                .expect("alt B ingest"),
            1
        );
        let err = insert_stall_events(&db, &[input("Alvida", 14, "Imported", "imported")])
            .expect_err("colliding re-ingest after own delete must refuse");
        assert!(
            err.contains("another character holds"),
            "actionable refusal, got: {err}"
        );
        let count: i64 = db
            .get()
            .expect("read conn")
            .query_row("SELECT COUNT(*) FROM stall_events", [], |r| r.get(0))
            .expect("count");
        assert_eq!(count, 2, "rollback: nothing deleted, nothing inserted");
    }

    #[test]
    fn insert_witness_replace_mutes_whole_set_on_any_doomed_mute() {
        // Mute carry (per-group OR): two occurrences of one line, first
        // muted. A witness re-ingest replaces both rows — the whole
        // replacement set inherits the mute (errs toward muted, reversible
        // via the per-row toggle), and muted rows stay out of the replay.
        // Positional oldest-first mapping is unsound across several losing
        // batches, so the carry is a set flag, never an occurrence zip.
        let db = baseline_pool();
        fn input(idx: i64, title: &str, log_ts: &str) -> StallEventInput {
            StallEventInput {
                event_timestamp: "Mon Apr 20 08:55".to_string(),
                event_at: Some("2026-04-20 08:55:00".to_string()),
                log_timestamp: log_ts.to_string(),
                log_title: title.to_string(),
                action: "added".to_string(),
                player: "Deradon".to_string(),
                owner: Some("Deradon".to_string()),
                item: Some("Kilderkin".to_string()),
                quantity: 1,
                price_unit: None,
                price_total: None,
                raw_message: "Deradon added Kilderkin to shop".to_string(),
                entry_index: idx,
            }
        }
        assert_eq!(
            insert_stall_events(
                &db,
                &[input(14, "Today's Shop Logs", "23:22:02"), input(15, "Today's Shop Logs", "23:22:02")]
            )
            .expect("first ingest"),
            2
        );
        db.get_write()
            .expect("write conn")
            .execute(
                "UPDATE stall_events SET ignored = 1 WHERE entry_index = 14",
                [],
            )
            .expect("mute one occurrence");
        // rows, net-new 0, whole replacement set muted.
        assert_eq!(
            insert_stall_events(
                &db,
                &[input(12, "Imported", "imported"), input(13, "Imported", "imported")]
            )
            .expect("re-ingest"),
            0,
            "witness replace is not new growth"
        );
        let mutes: Vec<(i64, i64)> = db
            .get()
            .expect("read conn")
            .prepare(
                "SELECT entry_index, ignored FROM stall_events
                 WHERE raw_message = 'Deradon added Kilderkin to shop'
                 ORDER BY entry_index",
            )
            .expect("prepare")
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
            .expect("query")
            .collect::<Result<Vec<_>, _>>()
            .expect("collect");
        assert_eq!(mutes, vec![(12, 1), (13, 1)], "any doomed mute mutes the set");
    }

    #[test]
    fn insert_same_batch_shifted_reingest_replaces_without_growth() {
        // F1: the watcher batch key is (file stem, constant "imported"), so
        // a re-processed book after the sliding window dropped the oldest
        // entry re-ingests the SAME batch at shifted indexes. The witness
        // branch must replace outright (net-new 0), never re-add every line
        // at its new index.
        let db = baseline_pool();
        fn input(idx: i64) -> StallEventInput {
            StallEventInput {
                event_timestamp: "Mon Apr 20 08:55".to_string(),
                event_at: Some("2026-04-20 08:55:00".to_string()),
                log_timestamp: "imported".to_string(),
                log_title: "Imported".to_string(),
                action: "added".to_string(),
                player: "Deradon".to_string(),
                owner: Some("Deradon".to_string()),
                item: Some("Kilderkin".to_string()),
                quantity: 1,
                price_unit: None,
                price_total: None,
                raw_message: "Deradon added Kilderkin to shop".to_string(),
                entry_index: idx,
            }
        }
        assert_eq!(
            insert_stall_events(&db, &[input(14), input(15)])
                .expect("first ingest"),
            2
        );
        // Same batch, every line shifted down one (oldest entry dropped).
        assert_eq!(
            insert_stall_events(&db, &[input(13), input(14)])
                .expect("shifted re-ingest"),
            0,
            "same-batch re-ingest replaces, never grows"
        );
        let rows: Vec<i64> = db
            .get()
            .expect("read conn")
            .prepare(
                "SELECT entry_index FROM stall_events
                 WHERE raw_message = 'Deradon added Kilderkin to shop'
                 ORDER BY entry_index",
            )
            .expect("prepare")
            .query_map([], |r| r.get(0))
            .expect("query")
            .collect::<Result<Vec<_>, _>>()
            .expect("collect");
        assert_eq!(rows, vec![13, 14], "shifted indexes replace the old rows");
    }

    #[test]
    fn recount_counts_matches_fresh_book_shape() {
        // Fresh-book 2026-11-09 shape: priced-only `visible` lines whose
        // `added` rows scrolled out of the book must still count (one
        // listing's worth), `bought` depletes and never sets the listing
        // price, and a bought-out priced-only item drops to zero.
        use crate::shop_log_parser::ShopLogEntry;
        fn e(action: &str, item: &str, qty: i64, price: Option<f64>) -> ShopLogEntry {
            ShopLogEntry {
                timestamp: "Mon Nov 9 16:58".to_string(),
                event_at: Some("2026-11-09 16:58:00".to_string()),
                action: action.to_string(),
                player: "TwinkleofToes".to_string(),
                item: Some(item.to_string()),
                quantity: qty,
                price_unit: price,
                price_total: None,
                raw_message: format!("{action} {item}"),
                entry_index: 0,
            }
        }
        let entries = vec![
            e("added", "Orcish Spell Pouch", 3, None),
            e("visible", "Orcish Spell Pouch", 3, Some(300.0)),
            e("visible", "Unicorn Milk", 5, Some(4000.0)),
            e("visible", "Typical Animal Fat", 1, Some(2000.0)),
            e("bought", "Typical Animal Fat", 1, Some(2000.0)),
            e("added", "Snail Sinews", 2, None),
            e("visible", "Snail Sinews", 2, Some(500.0)),
            e("removed", "Snail Sinews", 2, None),
            e("added", "Snail Sinews", 2, None),
            // Fae Felt 2026-11-09: bought-out in-book (added covers the
            // bought), then a same-day re-list `visible`. The covered-ledger
            // guard must win — no resurrect.
            e("added", "Fae Felt", 2, None),
            e("bought", "Fae Felt", 2, Some(1000.0)),
            e("visible", "Fae Felt", 2, Some(1000.0)),
            // Truncated ledger: the `bought` has no covering `added` (adds
            // scrolled out), so the book's own `visible` states live stock
            // and must seed — never a qty-0 wipe.
            e("bought", "Scrolled Saddle", 1, Some(4500.0)),
            e("visible", "Scrolled Saddle", 4, Some(4500.0)),
            // Remove-and-re-add with scrolled-out adds: depletions clamp at
            // zero so the replay lands on the fresh quantity (5), not a
            // negative hole the `visible` cannot seed.
            e("removed", "Restocked Reins", 5, None),
            e("added", "Restocked Reins", 5, None),
            e("visible", "Restocked Reins", 5, Some(4500.0)),
        ];
        let got = recount_counts(&entries);
        let find = |name: &str| got.iter().find(|l| l.item == name).cloned();
        // Adds + visible: full count, book price wins.
        let pouch = find("Orcish Spell Pouch").expect("pouch counted");
        assert_eq!(pouch.quantity, 3);
        assert_eq!(pouch.price_unit, Some(300.0));
        // Priced-only visible (adds scrolled out): one listing's worth.
        let milk = find("Unicorn Milk").expect("milk counted");
        assert_eq!(milk.quantity, 5);
        assert_eq!(milk.price_unit, Some(4000.0));
        // Bought out: gone, and the sale price never became the listing.
        assert!(find("Typical Animal Fat").is_none());
        // Remove-then-add replays to the fresh quantity.
        let sinews = find("Snail Sinews").expect("sinews counted");
        assert_eq!(sinews.quantity, 2);
        // Bought-out then re-listed in the same book: stays gone.
        assert!(find("Fae Felt").is_none());
        // Uncovered bought + later visible: truncated ledger, seed listed qty.
        let saddle = find("Scrolled Saddle").expect("scrolled saddle counted");
        assert_eq!(saddle.quantity, 4);
        assert_eq!(saddle.price_unit, Some(4500.0));
        // Remove-and-re-add with scrolled-out adds: fresh quantity, not zero.
        let restocked = find("Restocked Reins").expect("restocked reins counted");
        assert_eq!(restocked.quantity, 5);
        assert_eq!(restocked.price_unit, Some(4500.0));
    }

    #[test]
    fn book_content_hash_is_stable_and_content_sensitive() {
        // The preview→apply tie: identical bytes hash the same (preview and
        // apply agree), any byte change hashes differently (a game rewrite
        // between preview and apply must abort, even at the same length).
        assert_eq!(book_content_hash("a = 1"), book_content_hash("a = 1"));
        assert_ne!(book_content_hash("a = 1"), book_content_hash("a = 2"));
        assert_ne!(book_content_hash("abc"), book_content_hash("abcd"));
    }
    #[test]
    fn recount_owner_rejects_alt_book() {
        // Owner safety: a book naming a different owner than the active
        // character must reject, not diff/apply against the alt. Owner-less
        // books still claim the active character; matching owners pass.
        use crate::shop_log_parser::ShopLog;
        fn log(owner: Option<&str>) -> ShopLog {
            ShopLog {
                log_timestamp: "imported".to_string(),
                title: "Imported".to_string(),
                entries: Vec::new(),
                owner: owner.map(str::to_string),
            }
        }
        assert!(recount_owner(&log(Some("Alt")), &Some("TwinkleofToes".into())).is_err());
        assert_eq!(
            recount_owner(&log(Some("TwinkleofToes")), &Some("TwinkleofToes".into())).unwrap(),
            "TwinkleofToes"
        );
        assert_eq!(
            recount_owner(&log(None), &Some("TwinkleofToes".into())).unwrap(),
            "TwinkleofToes"
        );
        assert!(recount_owner(&log(None), &None).is_err());
    }

    #[test]
    fn preview_rejects_book_without_stock_signal() {
        // H2: an empty or bought-only book must abort in preview, not offer
        // Set 0 / Zero N against the whole in-stock set.
        let db = baseline_pool();
        let owner = Some("Deradon".to_string());
        let err = preview_from_content(
            &db,
            std::path::Path::new("PlayerShopLog_260420_085500.txt"),
            "Day Mon Apr 20 08:55 - Deradon opened shop\n",
            &owner,
        )
        .unwrap_err();
        assert!(err.contains("no stock signal"), "unexpected: {err}");
        let bought_only = "Day Mon Apr 20 08:55 - MrBonq bought Quality Reins at a cost of 4500 per 1 = 4500\n";
        let err2 = preview_from_content(
            &db,
            std::path::Path::new("PlayerShopLog_260420_085500.txt"),
            bought_only,
            &owner,
        )
        .unwrap_err();
        assert!(err2.contains("no stock signal"), "unexpected: {err2}");
    }
}
