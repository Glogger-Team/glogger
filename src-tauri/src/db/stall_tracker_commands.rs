use super::DbPool;
use crate::shop_log_parser::{parse_shop_log, ShopLog};
use crate::stall_aggregations::{
    aggregate_inventory, aggregate_revenue, aggregate_sales_timeseries, Granularity,
    InventoryEvent, InventoryResult, RevenueEvent, RevenueResult, SalesTimeseriesResult,
    TimeseriesEvent, TimeseriesMode, TimeseriesPeriod,
};
use chrono::{Datelike, Local};
use rusqlite::types::Value;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;
use tauri::State;

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

/// Bulk-insert stall events, skipping duplicates via the UNIQUE key
/// `(event_timestamp, raw_message, entry_index)`. Returns the number of
/// rows actually inserted (i.e. not skipped by `INSERT OR IGNORE`).
///
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

    tx.commit()
        .map_err(|e| format!("Failed to commit transaction: {e}"))?;

    Ok(inserted)
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

/// Inventory tier-stack snapshot for the Inventory tab. Replays every
/// non-ignored event for the owner, in `(event_at ASC, id ASC)` order so
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
    let sql = "SELECT item, event_at, action, quantity, price_unit, price_total \
               FROM stall_events \
               WHERE owner = ?1 AND ignored = 0 AND item IS NOT NULL AND event_at IS NOT NULL \
                 AND action != 'unknown' AND action != 'collected' \
               ORDER BY event_at ASC, id ASC";
    let mut stmt = conn
        .prepare(sql)
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

    let period_days = params.period_days.unwrap_or(7);
    Ok(aggregate_inventory(events, period_days))
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

/// Delete every stall event for a single character. Requires non-empty owner —
/// we never want a typo to wipe all data across every alt. Serialized with
/// live ingest at the SQLite level: this is a single `DELETE` on the
/// single-write pool while inserts run in a `BEGIN IMMEDIATE` transaction.
#[tauri::command]
pub fn clear_stall_events(db: State<'_, DbPool>, owner: Option<String>) -> Result<usize, String> {
    let owner = opt_nonempty(&owner)
        .ok_or_else(|| "clear_stall_events requires an owner".to_string())?
        .to_string();

    let conn = db
        .get_write()
        .map_err(|e| format!("Database connection error: {e}"))?;
    let deleted = conn
        .execute(
            "DELETE FROM stall_events WHERE owner = ?1",
            rusqlite::params![owner],
        )
        .map_err(|e| format!("Clear query failed: {e}"))?;
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
/// 1. Year comes from the filename if it contains a 4-digit year between
///    2000 and 2099, else `Local::now().year()`.
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

    let base_year = year_from_filename(&path_buf);
    let shop_log: ShopLog = parse_shop_log("Imported", &content, "imported", base_year);
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

/// Write one file per `(owner, year, month, day)` group, sorted newest-first
/// to match the in-game book format. Round-trip contract: any file produced
/// by Export must be re-importable by Import, landing identical rows
/// (same UNIQUE key → INSERT OR IGNORE skips them all).
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
            // Use `i` as the entry_index so dedup rejects re-seeding without
            // a clear in between.
            entry_index: i as i64,
        });
    }

    insert_stall_events(&db, &inputs)
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let entries = vec![
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
}
