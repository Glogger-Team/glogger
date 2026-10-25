/// Pure aggregation logic for the Stall Tracker's Revenue and Inventory tabs.
///
/// This module has **no DB access and no Tauri state**. It takes plain event
/// structs and returns shaped result types. Two reasons for this isolation:
///
/// 1. **Scale** — at 100k events the bridge cost of shipping raw rows to the
///    frontend is what kills the UI. Computing here means the wire payloads
///    are bounded by the result shape (items × periods, not events).
/// 2. **Testability** — every piece of the math is unit-testable without
///    fixtures or a SQLite handle.
use chrono::{Datelike, Duration, NaiveDate, NaiveDateTime, Timelike, Weekday};
use serde::Serialize;
use std::collections::{BTreeMap, HashMap, HashSet};

// ============================================================
// Revenue aggregation
// ============================================================

/// Time bucketing for the Revenue pivot. Daily/Weekly/Monthly map to
/// distinct period keys and label formats; the BTreeMap collation order
/// works for all three because each key format sorts lexicographically.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Granularity {
    Daily,
    Weekly,
    Monthly,
}

/// A single `bought` event for revenue purposes. `event_at` is the ISO
/// `"YYYY-MM-DD HH:MM:SS"` populated by the year resolver — callers must
/// filter out events with NULL `event_at` before passing them in.
#[derive(Debug, Clone)]
pub struct RevenueEvent {
    pub item: String,
    pub event_at: String,
    pub price_total: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct RevenuePeriod {
    pub key: String,
    pub label: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct RevenueCell {
    pub item: String,
    pub period_key: String,
    pub revenue: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct RevenueResult {
    pub periods: Vec<RevenuePeriod>,
    pub items: Vec<String>,
    pub cells: Vec<RevenueCell>,
    pub row_totals: Vec<(String, i64)>,
    pub col_totals: Vec<(String, i64)>,
    pub grand_total: i64,
}

fn parse_event_at(s: &str) -> Option<NaiveDateTime> {
    NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S").ok()
}

fn period_key(date: NaiveDate, granularity: Granularity) -> String {
    match granularity {
        Granularity::Daily => date.format("%Y-%m-%d").to_string(),
        Granularity::Weekly => {
            let iso = date.iso_week();
            format!("{}-W{:02}", iso.year(), iso.week())
        }
        Granularity::Monthly => date.format("%Y-%m").to_string(),
    }
}

fn period_label(key: &str, granularity: Granularity, show_year: bool) -> String {
    match granularity {
        Granularity::Daily => {
            // key = "YYYY-MM-DD"
            if let Ok(d) = NaiveDate::parse_from_str(key, "%Y-%m-%d") {
                if show_year {
                    d.format("%b %-d %Y").to_string()
                } else {
                    d.format("%b %-d").to_string()
                }
            } else {
                key.to_string()
            }
        }
        Granularity::Weekly => {
            // key = "YYYY-Www"
            let parts: Vec<&str> = key.split("-W").collect();
            if parts.len() == 2 {
                if let (Ok(year), Ok(week)) = (parts[0].parse::<i32>(), parts[1].parse::<u32>()) {
                    if let Some(monday) = NaiveDate::from_isoywd_opt(year, week, Weekday::Mon) {
                        let sunday = monday + chrono::Duration::days(6);
                        return if show_year {
                            format!(
                                "{} – {} {}",
                                monday.format("%b %-d"),
                                sunday.format("%b %-d"),
                                year
                            )
                        } else {
                            format!("{} – {}", monday.format("%b %-d"), sunday.format("%b %-d"))
                        };
                    }
                }
            }
            key.to_string()
        }
        Granularity::Monthly => {
            // key = "YYYY-MM" — year is always shown
            let parts: Vec<&str> = key.split('-').collect();
            if parts.len() == 2 {
                if let (Ok(year), Ok(month)) = (parts[0].parse::<i32>(), parts[1].parse::<u32>()) {
                    if let Some(d) = NaiveDate::from_ymd_opt(year, month, 1) {
                        return d.format("%b %Y").to_string();
                    }
                }
            }
            key.to_string()
        }
    }
}

/// Bucket revenue events by `(item, period_key)` and emit the pivot shape.
///
/// `BTreeMap` is the workhorse here — its sorted iteration gives us
/// alphabetically-sorted item rows and chronologically-sorted period columns
/// for free, since `YYYY-MM-DD` / `YYYY-Www` / `YYYY-MM` all sort correctly
/// as plain strings.
pub fn aggregate_revenue(
    events: impl IntoIterator<Item = RevenueEvent>,
    granularity: Granularity,
) -> RevenueResult {
    // First pass: bucket cells, collect distinct period keys and items.
    let mut cells_map: BTreeMap<(String, String), i64> = BTreeMap::new();
    let mut period_keys: BTreeMap<String, NaiveDate> = BTreeMap::new();
    let mut items_set: BTreeMap<String, ()> = BTreeMap::new();
    let mut years: HashSet<i32> = HashSet::new();

    for ev in events {
        let Some(dt) = parse_event_at(&ev.event_at) else {
            continue;
        };
        let date = dt.date();
        years.insert(date.year());
        let key = period_key(date, granularity);
        period_keys.entry(key.clone()).or_insert(date);
        items_set.insert(ev.item.clone(), ());
        *cells_map.entry((ev.item, key)).or_insert(0) += ev.price_total;
    }

    let show_year = years.len() > 1;

    let periods: Vec<RevenuePeriod> = period_keys
        .keys()
        .map(|k| RevenuePeriod {
            key: k.clone(),
            label: period_label(k, granularity, show_year),
        })
        .collect();

    let items: Vec<String> = items_set.into_keys().collect();

    // Second pass: emit cells in deterministic order, compute totals.
    let mut cells: Vec<RevenueCell> = Vec::with_capacity(cells_map.len());
    let mut row_totals: BTreeMap<String, i64> = BTreeMap::new();
    let mut col_totals: BTreeMap<String, i64> = BTreeMap::new();
    let mut grand_total: i64 = 0;

    for ((item, key), revenue) in cells_map {
        *row_totals.entry(item.clone()).or_insert(0) += revenue;
        *col_totals.entry(key.clone()).or_insert(0) += revenue;
        grand_total += revenue;
        cells.push(RevenueCell {
            item,
            period_key: key,
            revenue,
        });
    }

    RevenueResult {
        periods,
        items,
        cells,
        row_totals: row_totals.into_iter().collect(),
        col_totals: col_totals.into_iter().collect(),
        grand_total,
    }
}

// ============================================================
// Inventory aggregation
// ============================================================

/// A single shop event for inventory purposes. Caller must order events
/// by `(event_at ASC, id ASC)` — the id tiebreaker is critical because
/// `visible` and `added` events within the same minute share an `event_at`,
/// and processing `visible` first would fail to price the tier.
#[derive(Debug, Clone)]
pub struct InventoryEvent {
    pub item: String,
    pub event_at: String,
    pub action: String,
    pub quantity: i64,
    pub price_unit: Option<f64>,
    pub price_total: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PriceTier {
    pub qty: i64,
    pub price: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct InventoryItem {
    pub item: String,
    pub quantity: i64,
    pub price_tiers: Vec<PriceTier>,
    pub estimated_value: i64,
    pub period_sold: i64,
    pub period_revenue: i64,
    pub avg_per_day: f64,
    pub last_sold_at: Option<String>,
    pub last_activity_at: Option<String>,
    /// Most recent priced event seen for this item — survives `finalize_tiers`
    /// even when all tiers drop to zero, so the Recently Sold Out view can
    /// still display "what customers were paying when this ran out".
    pub last_known_price: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct InventoryResult {
    pub items: Vec<InventoryItem>,
    /// Distinct dates with activity, **newest-first**, so the frontend can
    /// `.slice(0, N)` for the "Recently Sold Out in last N days" panel.
    pub active_dates: Vec<String>,
    pub estimated_value: i64,
    pub total_sold: i64,
    pub avg_daily_revenue: f64,
}

/// Sentinel: any `period_days` ≥ this is treated as "all time".
const ALL_TIME_SENTINEL: i64 = 99_999;

/// Per-item state tracked while replaying the event log.
#[derive(Debug, Default)]
struct ItemState {
    tiers: Vec<PriceTier>,
    period_sold: i64,
    period_revenue: i64,
    last_sold_at: Option<String>,
    last_activity_at: Option<String>,
    last_known_price: Option<f64>,
}

impl ItemState {
    fn total_qty(&self) -> i64 {
        self.tiers.iter().map(|t| t.qty).sum()
    }

    fn prune_zero(&mut self) {
        self.tiers.retain(|t| t.qty != 0);
    }
}

/// Apply an `added` event: if the running total is negative (truncated log),
/// clear the stack first so a fresh baseline can take hold; then push a new
/// unpriced tier.
fn apply_added(state: &mut ItemState, qty: i64) {
    if state.total_qty() < 0 {
        state.tiers.clear();
    }
    state.tiers.push(PriceTier { qty, price: None });
}

/// Apply a `visible` or `configured` event: walk tiers front-to-back, apply
/// the new price to up to `qty` units across **unpriced** tiers. Split a
/// tier when the event covers fewer units than it contains. If no unpriced
/// tier is found, overwrite the last tier's price as a fallback.
fn apply_pricing(state: &mut ItemState, qty: i64, price: f64) {
    let mut remaining = qty;
    let mut matched_any = false;
    let mut i = 0;
    while i < state.tiers.len() && remaining > 0 {
        if state.tiers[i].price.is_none() {
            let tier_qty = state.tiers[i].qty;
            if tier_qty <= remaining {
                state.tiers[i].price = Some(price);
                remaining -= tier_qty;
                matched_any = true;
                i += 1;
            } else {
                // Split: priced portion takes `remaining`, unpriced remainder stays.
                let priced = PriceTier {
                    qty: remaining,
                    price: Some(price),
                };
                state.tiers[i].qty = tier_qty - remaining;
                state.tiers.insert(i, priced);
                remaining = 0;
                matched_any = true;
            }
        } else {
            i += 1;
        }
    }
    if !matched_any {
        // No unpriced tier — overwrite the last one as a fallback so prices
        // still propagate when a `visible` arrives without a fresh `added`.
        if let Some(last) = state.tiers.last_mut() {
            last.price = Some(price);
        }
    }
}

/// Apply a `bought` event: deplete tiers, preferring an exact price match
/// (deterministic for users with multiple tiers at known prices), then
/// falling back to front-to-back depletion. If nothing matched, push a
/// negative tier — this represents a sale without a known baseline and
/// will be cleared by the next `added`.
fn apply_bought(state: &mut ItemState, qty: i64, price: Option<f64>) {
    let mut remaining = qty;

    // Pass 1: exact price match.
    if let Some(p) = price {
        for tier in state.tiers.iter_mut() {
            if remaining == 0 {
                break;
            }
            if tier.price.map(|tp| (tp - p).abs() < 0.001).unwrap_or(false) && tier.qty > 0 {
                let take = remaining.min(tier.qty);
                tier.qty -= take;
                remaining -= take;
            }
        }
    }

    // Pass 2: front-to-back fallback.
    for tier in state.tiers.iter_mut() {
        if remaining == 0 {
            break;
        }
        if tier.qty > 0 {
            let take = remaining.min(tier.qty);
            tier.qty -= take;
            remaining -= take;
        }
    }

    if remaining > 0 {
        // No matching tier — push a negative tier as a "sales without
        // baseline" marker. The next `added` event will clear it.
        state.tiers.push(PriceTier {
            qty: -remaining,
            price,
        });
    }
}

/// Apply a `removed` event: LIFO depletion — walk tiers back-to-front.
fn apply_removed(state: &mut ItemState, qty: i64) {
    let mut remaining = qty;
    for tier in state.tiers.iter_mut().rev() {
        if remaining == 0 {
            break;
        }
        if tier.qty > 0 {
            let take = remaining.min(tier.qty);
            tier.qty -= take;
            remaining -= take;
        }
    }
}

/// Collapse same-price tiers, drop non-positive tiers, sort by price ascending
/// (None last). Run once at the end of replay.
fn finalize_tiers(state: &mut ItemState) {
    state.prune_zero();

    // Collapse: group by price (treating None as a distinct bucket).
    let mut by_price: BTreeMap<i64, i64> = BTreeMap::new();
    let mut unpriced_qty: i64 = 0;
    for tier in state.tiers.drain(..) {
        if tier.qty <= 0 {
            continue;
        }
        match tier.price {
            // Multiply by 1000 + round so 1500.0 and 1500.0000001 collapse.
            Some(p) => *by_price.entry((p * 1000.0).round() as i64).or_insert(0) += tier.qty,
            None => unpriced_qty += tier.qty,
        }
    }

    let mut tiers: Vec<PriceTier> = by_price
        .into_iter()
        .map(|(scaled, qty)| PriceTier {
            qty,
            price: Some(scaled as f64 / 1000.0),
        })
        .collect();
    if unpriced_qty > 0 {
        tiers.push(PriceTier {
            qty: unpriced_qty,
            price: None,
        });
    }

    state.tiers = tiers;
}

/// Replay a stream of inventory events into per-item tier stacks, then
/// compute window-scoped sales metrics.
///
/// `period_days` semantics:
/// - **Positive < 99999** → take the N most recent distinct active dates
///   as the window. Divisor for `avg_per_day` is `period_days` (NOT the
///   active date count) — conservative forecasting.
/// - **Zero / negative / ≥ 99999** → "all time". Window is every distinct
///   date, divisor is `max(distinct_date_count, 1)`.
pub fn aggregate_inventory(
    events: impl IntoIterator<Item = InventoryEvent>,
    period_days: i64,
) -> InventoryResult {
    let events: Vec<InventoryEvent> = events.into_iter().collect();

    // First pass: collect distinct active dates (newest-first).
    let mut date_set: BTreeMap<String, ()> = BTreeMap::new();
    for ev in &events {
        if ev.event_at.len() >= 10 {
            date_set.insert(ev.event_at[..10].to_string(), ());
        }
    }
    let active_dates: Vec<String> = date_set.into_keys().rev().collect();

    // Resolve the window: which dates count as "in period" for sales metrics?
    let all_time = period_days <= 0 || period_days >= ALL_TIME_SENTINEL;
    let window_dates: HashSet<String> = if all_time {
        active_dates.iter().cloned().collect()
    } else {
        active_dates
            .iter()
            .take(period_days as usize)
            .cloned()
            .collect()
    };

    // Walk events in input order — caller must pre-sort by (event_at, id).
    let mut by_item: BTreeMap<String, ItemState> = BTreeMap::new();
    for ev in events {
        let state = by_item.entry(ev.item.clone()).or_default();
        let date_in_window = ev
            .event_at
            .get(..10)
            .map(|d| window_dates.contains(d))
            .unwrap_or(false);

        match ev.action.as_str() {
            "added" => apply_added(state, ev.quantity),
            "visible" | "configured" => {
                if let Some(price) = ev.price_unit {
                    apply_pricing(state, ev.quantity, price);
                    state.last_known_price = Some(price);
                }
            }
            "bought" => {
                apply_bought(state, ev.quantity, ev.price_unit);
                if let Some(price) = ev.price_unit {
                    state.last_known_price = Some(price);
                }
                if date_in_window {
                    state.period_sold += ev.quantity;
                    if let Some(total) = ev.price_total {
                        state.period_revenue += total;
                    }
                }
                state.last_sold_at = Some(ev.event_at.clone());
            }
            "removed" => apply_removed(state, ev.quantity),
            _ => {}
        }
        state.last_activity_at = Some(ev.event_at.clone());
        state.prune_zero();
    }

    // Finalize: collapse tiers and compute per-item summaries.
    let divisor: f64 = if all_time {
        active_dates.len().max(1) as f64
    } else {
        period_days as f64
    };

    let mut items: Vec<InventoryItem> = Vec::new();
    let mut grand_estimated_value: i64 = 0;
    let mut grand_total_sold: i64 = 0;
    let mut grand_period_revenue: i64 = 0;

    for (item_name, mut state) in by_item {
        finalize_tiers(&mut state);
        let quantity: i64 = state.tiers.iter().map(|t| t.qty).sum();
        let estimated_value: i64 = state
            .tiers
            .iter()
            .map(|t| (t.qty as f64) * t.price.unwrap_or(0.0))
            .sum::<f64>() as i64;
        let avg_per_day = state.period_sold as f64 / divisor;

        grand_estimated_value += estimated_value.max(0);
        grand_total_sold += state.period_sold;
        grand_period_revenue += state.period_revenue;

        items.push(InventoryItem {
            item: item_name,
            quantity,
            price_tiers: state.tiers,
            estimated_value,
            period_sold: state.period_sold,
            period_revenue: state.period_revenue,
            avg_per_day,
            last_sold_at: state.last_sold_at,
            last_activity_at: state.last_activity_at,
            last_known_price: state.last_known_price,
        });
    }

    InventoryResult {
        items,
        active_dates,
        estimated_value: grand_estimated_value,
        total_sold: grand_total_sold,
        avg_daily_revenue: grand_period_revenue as f64 / divisor,
    }
}

// ============================================================
// Sales time-series (Trends tab)
// ============================================================

/// A single `bought` event for the Trends chart. Like `RevenueEvent` but also
/// carries `quantity`, so the chart can plot units-sold as well as gold.
#[derive(Debug, Clone)]
pub struct TimeseriesEvent {
    pub item: String,
    pub event_at: String,
    pub quantity: i64,
    pub price_total: i64,
}

/// Look-back window for the Trends chart.
///
/// The window is anchored to the **latest relevant sale in the data**, NOT to
/// wall-clock now. This mirrors the Inventory tab's "most recent active dates"
/// convention: a player who imported historical books but hasn't sold in a
/// week still gets a populated "last 7 days" chart instead of an empty one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimeseriesPeriod {
    Day1,    // 24 hourly buckets ending at the anchor hour
    Day3,    // 3 daily buckets
    Day7,    // 7 daily buckets
    Month1,  // 30 daily buckets
    AllTime, // full span; daily / weekly / monthly chosen by length
}

/// Which lines the chart draws. The item picker and summary picker on the
/// frontend are mutually exclusive, so exactly one of these is in play.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TimeseriesMode {
    /// One line: the named item only. The window anchors to *this item's*
    /// latest sale so the chart stays focused on when it was actually active.
    Item(String),
    /// N lines: the N highest-grossing (by gold) items within the window.
    TopGrossing(usize),
    /// One line: every item summed per bucket.
    Total,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Bucket {
    Hour,
    Day,
    Week,
    Month,
}

impl Bucket {
    fn as_str(self) -> &'static str {
        match self {
            Bucket::Hour => "hour",
            Bucket::Day => "day",
            Bucket::Week => "week",
            Bucket::Month => "month",
        }
    }
}

/// One drawable line. `gold` and `units` are both returned so the frontend's
/// Gold/Units toggle never has to refetch — the payload is bounded by
/// `lines × periods`, not by event count.
#[derive(Debug, Clone, Serialize)]
pub struct TimeseriesLine {
    /// Item name, or the literal `"Total"` for the aggregate line.
    pub item: String,
    pub gold: Vec<i64>,
    pub units: Vec<i64>,
    pub total_gold: i64,
    pub total_units: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct SalesTimeseriesResult {
    /// X-axis buckets, oldest-first. Series indices align to this vector.
    pub periods: Vec<RevenuePeriod>,
    pub lines: Vec<TimeseriesLine>,
    /// Bucket granularity actually used: `"hour" | "day" | "week" | "month"`.
    pub bucket: String,
}

const TS_ALL_DAILY_MAX_DAYS: i64 = 92;
const TS_ALL_WEEKLY_MAX_DAYS: i64 = 730;

fn empty_timeseries(bucket: Bucket) -> SalesTimeseriesResult {
    SalesTimeseriesResult {
        periods: Vec::new(),
        lines: Vec::new(),
        bucket: bucket.as_str().to_string(),
    }
}

fn bucket_key(dt: &NaiveDateTime, bucket: Bucket) -> String {
    match bucket {
        Bucket::Hour => dt.format("%Y-%m-%d %H").to_string(),
        Bucket::Day => dt.format("%Y-%m-%d").to_string(),
        Bucket::Week => {
            let iso = dt.date().iso_week();
            format!("{}-W{:02}", iso.year(), iso.week())
        }
        Bucket::Month => dt.format("%Y-%m").to_string(),
    }
}

fn ts_label(key: &str, bucket: Bucket, show_year: bool) -> String {
    match bucket {
        Bucket::Hour => {
            // key = "YYYY-MM-DD HH" → "2PM"
            if let Ok(dt) =
                NaiveDateTime::parse_from_str(&format!("{key}:00:00"), "%Y-%m-%d %H:%M:%S")
            {
                dt.format("%-l%p").to_string()
            } else {
                key.to_string()
            }
        }
        Bucket::Day => {
            if let Ok(d) = NaiveDate::parse_from_str(key, "%Y-%m-%d") {
                if show_year {
                    d.format("%b %-d %Y").to_string()
                } else {
                    d.format("%b %-d").to_string()
                }
            } else {
                key.to_string()
            }
        }
        Bucket::Week => period_label(key, Granularity::Weekly, show_year),
        Bucket::Month => period_label(key, Granularity::Monthly, false),
    }
}

/// Advance a date by one calendar month, clamping the day (Jan 31 → Feb 28/29).
fn add_one_month(d: NaiveDate) -> NaiveDate {
    let (y, m) = if d.month() == 12 {
        (d.year() + 1, 1)
    } else {
        (d.year(), d.month() + 1)
    };
    // Walk back from the target day until a valid date exists.
    let mut day = d.day();
    loop {
        if let Some(nd) = NaiveDate::from_ymd_opt(y, m, day) {
            return nd;
        }
        day -= 1;
        if day == 0 {
            // Unreachable for real calendars, but never loop forever.
            return NaiveDate::from_ymd_opt(y, m, 1).unwrap();
        }
    }
}

/// Build the ordered list of bucket keys spanning `[start, anchor]` inclusive,
/// including empty buckets (a no-sale day is a real 0, not a gap). Returns the
/// keys oldest-first plus a `key → index` map for O(1) accumulation.
fn enumerate_buckets(
    start: NaiveDateTime,
    anchor: NaiveDateTime,
    bucket: Bucket,
) -> (Vec<String>, HashMap<String, usize>) {
    let mut keys: Vec<String> = Vec::new();
    match bucket {
        Bucket::Hour => {
            let mut cur = start
                .date()
                .and_hms_opt(start.hour(), 0, 0)
                .unwrap_or(start);
            let end = anchor;
            while cur <= end {
                keys.push(bucket_key(&cur, bucket));
                cur += Duration::hours(1);
            }
        }
        Bucket::Day => {
            let mut cur = start.date();
            let end = anchor.date();
            while cur <= end {
                keys.push(bucket_key(&cur.and_hms_opt(0, 0, 0).unwrap(), bucket));
                cur += Duration::days(1);
            }
        }
        Bucket::Week => {
            // Start at the Monday of the start week.
            let iso = start.date().iso_week();
            let mut cur = NaiveDate::from_isoywd_opt(iso.year(), iso.week(), Weekday::Mon)
                .unwrap_or_else(|| start.date());
            let end = anchor.date();
            while cur <= end {
                keys.push(bucket_key(&cur.and_hms_opt(0, 0, 0).unwrap(), bucket));
                cur += Duration::days(7);
            }
        }
        Bucket::Month => {
            let mut cur = NaiveDate::from_ymd_opt(start.year(), start.month(), 1)
                .unwrap_or_else(|| start.date());
            let end = NaiveDate::from_ymd_opt(anchor.year(), anchor.month(), 1)
                .unwrap_or_else(|| anchor.date());
            while cur <= end {
                keys.push(bucket_key(&cur.and_hms_opt(0, 0, 0).unwrap(), bucket));
                cur = add_one_month(cur);
            }
        }
    }

    let index: HashMap<String, usize> = keys
        .iter()
        .enumerate()
        .map(|(i, k)| (k.clone(), i))
        .collect();
    (keys, index)
}

/// Resolve the `(start, anchor, bucket)` window for a given period, anchored to
/// the supplied event datetimes (already filtered to the mode's relevant set).
fn resolve_window(
    min_dt: NaiveDateTime,
    anchor: NaiveDateTime,
    period: TimeseriesPeriod,
) -> (NaiveDateTime, NaiveDateTime, Bucket) {
    match period {
        TimeseriesPeriod::Day1 => {
            let anchor_hour = anchor
                .date()
                .and_hms_opt(anchor.hour(), 0, 0)
                .unwrap_or(anchor);
            (anchor_hour - Duration::hours(23), anchor_hour, Bucket::Hour)
        }
        TimeseriesPeriod::Day3 => day_window(anchor, 2),
        TimeseriesPeriod::Day7 => day_window(anchor, 6),
        TimeseriesPeriod::Month1 => day_window(anchor, 29),
        TimeseriesPeriod::AllTime => {
            let start = min_dt.date().and_hms_opt(0, 0, 0).unwrap_or(min_dt);
            let span_days = (anchor.date() - min_dt.date()).num_days();
            let bucket = if span_days <= TS_ALL_DAILY_MAX_DAYS {
                Bucket::Day
            } else if span_days <= TS_ALL_WEEKLY_MAX_DAYS {
                Bucket::Week
            } else {
                Bucket::Month
            };
            (start, anchor, bucket)
        }
    }
}

fn day_window(anchor: NaiveDateTime, days_back: i64) -> (NaiveDateTime, NaiveDateTime, Bucket) {
    let anchor_day = anchor.date().and_hms_opt(0, 0, 0).unwrap_or(anchor);
    (anchor_day - Duration::days(days_back), anchor, Bucket::Day)
}

/// Aggregate `bought` events into per-line, per-bucket gold/units series.
///
/// The caller passes **every** bought event for the owner (not pre-filtered by
/// item) so `TopGrossing` ranking and the `Total` line see the full picture.
/// `Item` mode filters internally and anchors the window to that item alone.
pub fn aggregate_sales_timeseries(
    events: impl IntoIterator<Item = TimeseriesEvent>,
    period: TimeseriesPeriod,
    mode: TimeseriesMode,
) -> SalesTimeseriesResult {
    // Parse once; drop rows with an unparseable event_at.
    let parsed: Vec<(NaiveDateTime, TimeseriesEvent)> = events
        .into_iter()
        .filter_map(|e| parse_event_at(&e.event_at).map(|dt| (dt, e)))
        .collect();

    if parsed.is_empty() {
        return empty_timeseries(Bucket::Day);
    }

    // Anchor to the mode's relevant events: Item mode centres on that item's
    // own activity; Total / TopGrossing use the global span.
    let relevant: Vec<&(NaiveDateTime, TimeseriesEvent)> = match &mode {
        TimeseriesMode::Item(item) => parsed.iter().filter(|(_, e)| &e.item == item).collect(),
        _ => parsed.iter().collect(),
    };
    if relevant.is_empty() {
        return empty_timeseries(Bucket::Day);
    }

    let anchor = relevant.iter().map(|(dt, _)| *dt).max().unwrap();
    let min_dt = relevant.iter().map(|(dt, _)| *dt).min().unwrap();

    let (start, anchor, bucket) = resolve_window(min_dt, anchor, period);
    let (keys, index) = enumerate_buckets(start, anchor, bucket);
    let n = keys.len();
    if n == 0 {
        return empty_timeseries(bucket);
    }

    let show_year = keys.first().map(|k| &k[..4]) != keys.last().map(|k| &k[..4]);
    let periods: Vec<RevenuePeriod> = keys
        .iter()
        .map(|k| RevenuePeriod {
            key: k.clone(),
            label: ts_label(k, bucket, show_year),
        })
        .collect();

    // Accumulate gold/units per item across the in-window buckets.
    struct Acc {
        gold: Vec<i64>,
        units: Vec<i64>,
        total_gold: i64,
        total_units: i64,
    }
    let mut by_item: BTreeMap<String, Acc> = BTreeMap::new();

    for (dt, e) in &parsed {
        if let TimeseriesMode::Item(item) = &mode {
            if &e.item != item {
                continue;
            }
        }
        let key = bucket_key(dt, bucket);
        let Some(&i) = index.get(&key) else {
            continue; // outside the window
        };
        let acc = by_item.entry(e.item.clone()).or_insert_with(|| Acc {
            gold: vec![0; n],
            units: vec![0; n],
            total_gold: 0,
            total_units: 0,
        });
        acc.gold[i] += e.price_total;
        acc.units[i] += e.quantity;
        acc.total_gold += e.price_total;
        acc.total_units += e.quantity;
    }

    let mut lines: Vec<TimeseriesLine> = Vec::new();

    match mode {
        TimeseriesMode::Item(item) => {
            // Single line. Emit zeros if the item had no in-window sales.
            let acc = by_item.remove(&item);
            let (gold, units, tg, tu) = match acc {
                Some(a) => (a.gold, a.units, a.total_gold, a.total_units),
                None => (vec![0; n], vec![0; n], 0, 0),
            };
            lines.push(TimeseriesLine {
                item,
                gold,
                units,
                total_gold: tg,
                total_units: tu,
            });
        }
        TimeseriesMode::Total => {
            let mut gold = vec![0i64; n];
            let mut units = vec![0i64; n];
            let mut tg = 0i64;
            let mut tu = 0i64;
            for acc in by_item.values() {
                for i in 0..n {
                    gold[i] += acc.gold[i];
                    units[i] += acc.units[i];
                }
                tg += acc.total_gold;
                tu += acc.total_units;
            }
            lines.push(TimeseriesLine {
                item: "Total".to_string(),
                gold,
                units,
                total_gold: tg,
                total_units: tu,
            });
        }
        TimeseriesMode::TopGrossing(count) => {
            // Rank by in-window gold desc, item name asc as a stable tiebreak.
            let mut ranked: Vec<(String, Acc)> = by_item.into_iter().collect();
            ranked.sort_by(|(a_item, a), (b_item, b)| {
                b.total_gold
                    .cmp(&a.total_gold)
                    .then_with(|| a_item.cmp(b_item))
            });
            for (item, acc) in ranked.into_iter().take(count) {
                lines.push(TimeseriesLine {
                    item,
                    gold: acc.gold,
                    units: acc.units,
                    total_gold: acc.total_gold,
                    total_units: acc.total_units,
                });
            }
        }
    }

    SalesTimeseriesResult {
        periods,
        lines,
        bucket: bucket.as_str().to_string(),
    }
}

// ============================================================
// Tests
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn rev(item: &str, event_at: &str, total: i64) -> RevenueEvent {
        RevenueEvent {
            item: item.into(),
            event_at: event_at.into(),
            price_total: total,
        }
    }

    fn inv(
        action: &str,
        item: &str,
        event_at: &str,
        qty: i64,
        price: Option<f64>,
    ) -> InventoryEvent {
        // Mirrors the real DB: only `bought` (and `collected`, irrelevant for
        // inventory) carry a `price_total`. Other actions leave it NULL.
        let price_total = if action == "bought" {
            price.map(|p| (p * qty as f64) as i64)
        } else {
            None
        };
        InventoryEvent {
            item: item.into(),
            event_at: event_at.into(),
            action: action.into(),
            quantity: qty,
            price_unit: price,
            price_total,
        }
    }

    // ---------- Revenue ----------

    #[test]
    fn revenue_daily_pivot() {
        let events = vec![
            rev("Quality Reins", "2026-04-13 09:00:00", 4500),
            rev("Quality Reins", "2026-04-13 10:00:00", 4500),
            rev("Quality Reins", "2026-04-14 11:00:00", 4500),
            rev("Nice Saddle", "2026-04-13 12:00:00", 4000),
            rev("Nice Saddle", "2026-04-14 13:00:00", 4000),
        ];
        let r = aggregate_revenue(events, Granularity::Daily);
        assert_eq!(r.items, vec!["Nice Saddle", "Quality Reins"]);
        assert_eq!(r.periods.len(), 2);
        assert_eq!(r.periods[0].key, "2026-04-13");
        assert_eq!(r.periods[1].key, "2026-04-14");
        assert_eq!(r.grand_total, 4500 * 3 + 4000 * 2);
        assert_eq!(r.cells.len(), 4); // (Nice Saddle, 13), (Nice Saddle, 14), (Quality Reins, 13), (Quality Reins, 14)
    }

    #[test]
    fn revenue_monthly_collapses_days() {
        let events: Vec<_> = (1..=30)
            .map(|d| rev("Quality Reins", &format!("2026-04-{:02} 09:00:00", d), 100))
            .collect();
        let r = aggregate_revenue(events, Granularity::Monthly);
        assert_eq!(r.periods.len(), 1);
        assert_eq!(r.periods[0].key, "2026-04");
        assert_eq!(r.cells.len(), 1);
        assert_eq!(r.cells[0].revenue, 3000);
        assert_eq!(r.grand_total, 3000);
    }

    #[test]
    fn revenue_multi_year_adds_year_suffix_to_daily_labels() {
        let events = vec![
            rev("Quality Reins", "2025-12-31 09:00:00", 100),
            rev("Quality Reins", "2026-01-01 09:00:00", 200),
        ];
        let r = aggregate_revenue(events, Granularity::Daily);
        // Both labels should include the year because the dataset spans years.
        assert!(r.periods[0].label.contains("2025") || r.periods[0].label.contains("2026"));
        assert!(r.periods[1].label.contains("2025") || r.periods[1].label.contains("2026"));
        // The actual labels:
        assert_eq!(r.periods[0].label, "Dec 31 2025");
        assert_eq!(r.periods[1].label, "Jan 1 2026");
    }

    #[test]
    fn revenue_single_year_omits_year_suffix() {
        let events = vec![
            rev("Quality Reins", "2026-04-13 09:00:00", 100),
            rev("Quality Reins", "2026-04-14 09:00:00", 200),
        ];
        let r = aggregate_revenue(events, Granularity::Daily);
        assert_eq!(r.periods[0].label, "Apr 13");
        assert_eq!(r.periods[1].label, "Apr 14");
    }

    #[test]
    fn revenue_weekly_year_boundary_uses_iso_year_in_label() {
        // ISO weeks belong to the year containing their Thursday, so a date
        // late in December can land in ISO week 1 of the following year.
        // This test pins the actual behavior so a future regex/format
        // change can't silently shift the year.
        //
        // Mon Dec 29 2025 → ISO 2026-W01 (Thursday Jan 1 2026 is in this week)
        // Mon Apr 13 2026 → ISO 2026-W16
        let events = vec![
            rev("Quality Reins", "2025-12-29 09:00:00", 100),
            rev("Quality Reins", "2026-04-13 09:00:00", 200),
        ];
        let r = aggregate_revenue(events, Granularity::Weekly);
        assert_eq!(r.periods.len(), 2);
        assert_eq!(r.periods[0].key, "2026-W01");
        assert_eq!(r.periods[1].key, "2026-W16");
        // Multi-year detection by CALENDAR year still fires (events span
        // 2025 and 2026 calendar years), so labels include year suffix.
        assert!(r.periods[0].label.contains("2025") || r.periods[0].label.contains("2026"));
        assert!(r.periods[1].label.contains("2026"));
    }

    #[test]
    fn revenue_weekly_groups_by_iso_week() {
        let events = vec![
            // Mon Apr 13 2026 = ISO week 16
            rev("Quality Reins", "2026-04-13 09:00:00", 100),
            rev("Quality Reins", "2026-04-15 09:00:00", 200), // Wed, same week
            rev("Quality Reins", "2026-04-20 09:00:00", 300), // Mon next week
        ];
        let r = aggregate_revenue(events, Granularity::Weekly);
        assert_eq!(r.periods.len(), 2);
        assert_eq!(r.periods[0].key, "2026-W16");
        assert_eq!(r.periods[1].key, "2026-W17");
    }

    #[test]
    fn revenue_empty_input() {
        let r = aggregate_revenue(Vec::<RevenueEvent>::new(), Granularity::Daily);
        assert_eq!(r.grand_total, 0);
        assert!(r.items.is_empty());
        assert!(r.periods.is_empty());
        assert!(r.cells.is_empty());
    }

    // ---------- Inventory ----------

    #[test]
    fn inventory_add_then_visible_then_sell() {
        let events = vec![
            inv("added", "Quality Reins", "2026-04-13 09:00:00", 5, None),
            inv(
                "visible",
                "Quality Reins",
                "2026-04-13 09:01:00",
                5,
                Some(4500.0),
            ),
            inv(
                "bought",
                "Quality Reins",
                "2026-04-14 10:00:00",
                2,
                Some(4500.0),
            ),
        ];
        let r = aggregate_inventory(events, 7);
        assert_eq!(r.items.len(), 1);
        let item = &r.items[0];
        assert_eq!(item.quantity, 3);
        assert_eq!(item.price_tiers.len(), 1);
        assert_eq!(item.price_tiers[0].qty, 3);
        assert_eq!(item.price_tiers[0].price, Some(4500.0));
        assert_eq!(item.estimated_value, 13500);
        assert_eq!(item.period_sold, 2);
    }

    #[test]
    fn inventory_same_minute_visible_after_added() {
        // visible and added share an event_at, but caller pre-sorts by (event_at, id)
        // so added arrives first. The tier gets priced correctly.
        let events = vec![
            inv("added", "Nice Saddle", "2026-04-13 09:00:00", 3, None),
            inv(
                "visible",
                "Nice Saddle",
                "2026-04-13 09:00:00",
                3,
                Some(4000.0),
            ),
        ];
        let r = aggregate_inventory(events, 7);
        let item = &r.items[0];
        assert_eq!(item.quantity, 3);
        assert_eq!(item.price_tiers[0].price, Some(4000.0));
    }

    #[test]
    fn inventory_negative_then_added_resets() {
        // Truncated log: we see the sale before we see the add.
        let events = vec![
            inv(
                "bought",
                "Mystic Saddlebag",
                "2026-04-13 09:00:00",
                2,
                Some(40000.0),
            ),
            inv("added", "Mystic Saddlebag", "2026-04-14 10:00:00", 5, None),
            inv(
                "visible",
                "Mystic Saddlebag",
                "2026-04-14 10:01:00",
                5,
                Some(40000.0),
            ),
        ];
        let r = aggregate_inventory(events, 7);
        let item = &r.items[0];
        // Negative tier from the orphan sale was cleared by the fresh `added`.
        assert_eq!(item.quantity, 5);
        assert_eq!(item.price_tiers.len(), 1);
        assert_eq!(item.price_tiers[0].price, Some(40000.0));
    }

    #[test]
    fn inventory_multi_tier_pricing() {
        // User adds 5 at price A, then later adds 3 at price B.
        let events = vec![
            inv("added", "Quality Reins", "2026-04-10 09:00:00", 5, None),
            inv(
                "visible",
                "Quality Reins",
                "2026-04-10 09:01:00",
                5,
                Some(4500.0),
            ),
            inv("added", "Quality Reins", "2026-04-12 09:00:00", 3, None),
            inv(
                "visible",
                "Quality Reins",
                "2026-04-12 09:01:00",
                3,
                Some(5000.0),
            ),
        ];
        let r = aggregate_inventory(events, 7);
        let item = &r.items[0];
        assert_eq!(item.quantity, 8);
        assert_eq!(item.price_tiers.len(), 2);
        // Sorted by price ascending.
        assert_eq!(item.price_tiers[0].price, Some(4500.0));
        assert_eq!(item.price_tiers[0].qty, 5);
        assert_eq!(item.price_tiers[1].price, Some(5000.0));
        assert_eq!(item.price_tiers[1].qty, 3);
    }

    #[test]
    fn inventory_period_window_takes_recent_active_dates() {
        // Sparse dataset: 3 days total (Apr 10, 12, 13).
        let events = vec![
            inv("added", "Quality Reins", "2026-04-10 09:00:00", 10, None),
            inv(
                "visible",
                "Quality Reins",
                "2026-04-10 09:01:00",
                10,
                Some(100.0),
            ),
            inv(
                "bought",
                "Quality Reins",
                "2026-04-10 10:00:00",
                1,
                Some(100.0),
            ),
            inv(
                "bought",
                "Quality Reins",
                "2026-04-12 10:00:00",
                2,
                Some(100.0),
            ),
            inv(
                "bought",
                "Quality Reins",
                "2026-04-13 10:00:00",
                3,
                Some(100.0),
            ),
        ];
        // window = 1 → only Apr 13 counts toward period_sold
        let r = aggregate_inventory(events.clone(), 1);
        assert_eq!(r.items[0].period_sold, 3);
        // window = 2 → Apr 12 + Apr 13 (the two most recent active dates)
        let r = aggregate_inventory(events.clone(), 2);
        assert_eq!(r.items[0].period_sold, 5);
        // all-time → all 3 sales
        let r = aggregate_inventory(events, ALL_TIME_SENTINEL);
        assert_eq!(r.items[0].period_sold, 6);
    }

    #[test]
    fn inventory_avg_per_day_uses_full_window_divisor() {
        // 3 sales across 3 distinct dates, period = 7.
        let events = vec![
            inv("added", "Quality Reins", "2026-04-10 09:00:00", 10, None),
            inv(
                "visible",
                "Quality Reins",
                "2026-04-10 09:01:00",
                10,
                Some(100.0),
            ),
            inv(
                "bought",
                "Quality Reins",
                "2026-04-10 10:00:00",
                1,
                Some(100.0),
            ),
            inv(
                "bought",
                "Quality Reins",
                "2026-04-12 10:00:00",
                1,
                Some(100.0),
            ),
            inv(
                "bought",
                "Quality Reins",
                "2026-04-13 10:00:00",
                1,
                Some(100.0),
            ),
        ];
        let r = aggregate_inventory(events, 7);
        // period_sold (within window) = 3, divisor = 7 (full window, not active days)
        assert!((r.items[0].avg_per_day - 3.0 / 7.0).abs() < 1e-9);
    }

    #[test]
    fn inventory_active_dates_newest_first() {
        let events = vec![
            inv(
                "bought",
                "Quality Reins",
                "2026-04-10 09:00:00",
                1,
                Some(100.0),
            ),
            inv(
                "bought",
                "Quality Reins",
                "2026-04-13 09:00:00",
                1,
                Some(100.0),
            ),
            inv(
                "bought",
                "Quality Reins",
                "2026-04-12 09:00:00",
                1,
                Some(100.0),
            ),
        ];
        let r = aggregate_inventory(events, 30);
        assert_eq!(
            r.active_dates,
            vec!["2026-04-13", "2026-04-12", "2026-04-10"]
        );
    }

    #[test]
    fn inventory_removed_lifo() {
        let events = vec![
            inv("added", "Quality Reins", "2026-04-10 09:00:00", 5, None),
            inv(
                "visible",
                "Quality Reins",
                "2026-04-10 09:01:00",
                5,
                Some(4500.0),
            ),
            inv("added", "Quality Reins", "2026-04-11 09:00:00", 3, None),
            inv(
                "visible",
                "Quality Reins",
                "2026-04-11 09:01:00",
                3,
                Some(5000.0),
            ),
            inv("removed", "Quality Reins", "2026-04-12 09:00:00", 2, None),
        ];
        let r = aggregate_inventory(events, 7);
        let item = &r.items[0];
        // LIFO: removes from the most recent tier (5000) first.
        assert_eq!(item.quantity, 6);
        // After collapse: 5×4500 and 1×5000.
        assert_eq!(item.price_tiers.len(), 2);
    }

    #[test]
    fn inventory_pricing_splits_unpriced_tier() {
        // Add 5 unpriced units, then price only 3 of them. The unpriced tier
        // must split into a priced front (qty 3) and an unpriced remainder (qty 2).
        let events = vec![
            inv("added", "Quality Reins", "2026-04-10 09:00:00", 5, None),
            inv(
                "visible",
                "Quality Reins",
                "2026-04-10 09:01:00",
                3,
                Some(4500.0),
            ),
        ];
        let r = aggregate_inventory(events, 7);
        let item = &r.items[0];
        assert_eq!(item.quantity, 5);
        assert_eq!(item.price_tiers.len(), 2);
        // After finalize: priced tier sorted first (None price last).
        assert_eq!(item.price_tiers[0].qty, 3);
        assert_eq!(item.price_tiers[0].price, Some(4500.0));
        assert_eq!(item.price_tiers[1].qty, 2);
        assert_eq!(item.price_tiers[1].price, None);
    }

    #[test]
    fn inventory_pricing_fallback_overwrites_last_tier_when_no_unpriced() {
        // Stack is fully priced, then a `configured` arrives at a new price.
        // Documented PoC behavior: the fallback overwrites the LAST tier's
        // price for the entire tier, even though the configured event names
        // a smaller quantity. Lossy but predictable.
        let events = vec![
            inv("added", "Quality Reins", "2026-04-10 09:00:00", 3, None),
            inv(
                "visible",
                "Quality Reins",
                "2026-04-10 09:01:00",
                3,
                Some(4500.0),
            ),
            inv(
                "configured",
                "Quality Reins",
                "2026-04-11 09:00:00",
                1,
                Some(5000.0),
            ),
        ];
        let r = aggregate_inventory(events, 7);
        let item = &r.items[0];
        assert_eq!(item.quantity, 3);
        assert_eq!(item.price_tiers.len(), 1);
        assert_eq!(item.price_tiers[0].qty, 3);
        assert_eq!(item.price_tiers[0].price, Some(5000.0));
    }

    #[test]
    fn inventory_bought_prefers_exact_price_tier() {
        // Two tiers: 5 @ 4500 and 3 @ 5000. A sale at 5000 must deplete
        // the 5000 tier, not the 4500 tier.
        let events = vec![
            inv("added", "Quality Reins", "2026-04-10 09:00:00", 5, None),
            inv(
                "visible",
                "Quality Reins",
                "2026-04-10 09:01:00",
                5,
                Some(4500.0),
            ),
            inv("added", "Quality Reins", "2026-04-11 09:00:00", 3, None),
            inv(
                "visible",
                "Quality Reins",
                "2026-04-11 09:01:00",
                3,
                Some(5000.0),
            ),
            inv(
                "bought",
                "Quality Reins",
                "2026-04-12 09:00:00",
                1,
                Some(5000.0),
            ),
        ];
        let r = aggregate_inventory(events, 7);
        let item = &r.items[0];
        assert_eq!(item.quantity, 7);
        assert_eq!(item.price_tiers.len(), 2);
        // 4500 tier untouched (still 5), 5000 tier depleted by 1 (now 2).
        assert_eq!(item.price_tiers[0].price, Some(4500.0));
        assert_eq!(item.price_tiers[0].qty, 5);
        assert_eq!(item.price_tiers[1].price, Some(5000.0));
        assert_eq!(item.price_tiers[1].qty, 2);
    }

    #[test]
    fn inventory_last_sold_and_last_activity_are_distinct() {
        // last_sold_at advances only on `bought` events.
        // last_activity_at advances on every event.
        let events = vec![
            inv("added", "Quality Reins", "2026-04-10 09:00:00", 5, None),
            inv(
                "visible",
                "Quality Reins",
                "2026-04-10 09:01:00",
                5,
                Some(4500.0),
            ),
            inv(
                "bought",
                "Quality Reins",
                "2026-04-11 10:00:00",
                1,
                Some(4500.0),
            ),
            inv(
                "configured",
                "Quality Reins",
                "2026-04-12 11:00:00",
                4,
                Some(4600.0),
            ),
        ];
        let r = aggregate_inventory(events, 30);
        let item = &r.items[0];
        assert_eq!(item.last_sold_at.as_deref(), Some("2026-04-11 10:00:00"));
        assert_eq!(
            item.last_activity_at.as_deref(),
            Some("2026-04-12 11:00:00")
        );
    }

    #[test]
    fn inventory_last_known_price_survives_sellout() {
        // Buy out the entire stack — finalize_tiers drops all tiers because
        // they're zero, but last_known_price should still hold the most recent
        // priced event (the bought event's price_unit, in this case).
        let events = vec![
            inv("added", "Quality Reins", "2026-04-10 09:00:00", 3, None),
            inv(
                "visible",
                "Quality Reins",
                "2026-04-10 09:01:00",
                3,
                Some(4500.0),
            ),
            inv(
                "bought",
                "Quality Reins",
                "2026-04-12 10:00:00",
                3,
                Some(4500.0),
            ),
        ];
        let r = aggregate_inventory(events, 7);
        let item = &r.items[0];
        assert_eq!(item.quantity, 0);
        assert!(item.price_tiers.is_empty());
        assert_eq!(item.last_known_price, Some(4500.0));
    }

    #[test]
    fn inventory_last_known_price_uses_most_recent_priced_event() {
        // visible at 4500, then configured at 5000 — last_known_price = 5000.
        let events = vec![
            inv("added", "Quality Reins", "2026-04-10 09:00:00", 5, None),
            inv(
                "visible",
                "Quality Reins",
                "2026-04-10 09:01:00",
                5,
                Some(4500.0),
            ),
            inv(
                "configured",
                "Quality Reins",
                "2026-04-11 09:00:00",
                5,
                Some(5000.0),
            ),
        ];
        let r = aggregate_inventory(events, 7);
        assert_eq!(r.items[0].last_known_price, Some(5000.0));
    }

    #[test]
    fn inventory_empty_input() {
        let r = aggregate_inventory(Vec::<InventoryEvent>::new(), 7);
        assert!(r.items.is_empty());
        assert!(r.active_dates.is_empty());
        assert_eq!(r.estimated_value, 0);
        assert_eq!(r.total_sold, 0);
    }

    // ---------- Sales time-series ----------

    fn ts(item: &str, event_at: &str, qty: i64, total: i64) -> TimeseriesEvent {
        TimeseriesEvent {
            item: item.into(),
            event_at: event_at.into(),
            quantity: qty,
            price_total: total,
        }
    }

    #[test]
    fn timeseries_empty_input() {
        let r = aggregate_sales_timeseries(
            Vec::<TimeseriesEvent>::new(),
            TimeseriesPeriod::Day7,
            TimeseriesMode::Total,
        );
        assert!(r.periods.is_empty());
        assert!(r.lines.is_empty());
    }

    #[test]
    fn timeseries_daily_window_anchors_to_latest_event() {
        // 7-day window ends on the latest sale (Apr 13), so buckets run
        // Apr 7 .. Apr 13 inclusive = 7 daily buckets, oldest-first.
        let events = vec![
            ts("Quality Reins", "2026-04-10 09:00:00", 1, 4500),
            ts("Quality Reins", "2026-04-13 10:00:00", 2, 9000),
        ];
        let r = aggregate_sales_timeseries(events, TimeseriesPeriod::Day7, TimeseriesMode::Total);
        assert_eq!(r.bucket, "day");
        assert_eq!(r.periods.len(), 7);
        assert_eq!(r.periods.first().unwrap().key, "2026-04-07");
        assert_eq!(r.periods.last().unwrap().key, "2026-04-13");
        // One Total line; gold lands in the Apr 10 (index 3) and Apr 13 (6) slots.
        assert_eq!(r.lines.len(), 1);
        let line = &r.lines[0];
        assert_eq!(line.item, "Total");
        assert_eq!(line.gold[3], 4500);
        assert_eq!(line.gold[6], 9000);
        assert_eq!(line.units[3], 1);
        assert_eq!(line.units[6], 2);
        assert_eq!(line.total_gold, 13500);
        assert_eq!(line.total_units, 3);
        // No-sale days are real zeros, not gaps.
        assert_eq!(line.gold[0], 0);
    }

    #[test]
    fn timeseries_total_sums_across_items_per_bucket() {
        let events = vec![
            ts("Quality Reins", "2026-04-13 09:00:00", 1, 4500),
            ts("Nice Saddle", "2026-04-13 10:00:00", 1, 4000),
        ];
        let r = aggregate_sales_timeseries(events, TimeseriesPeriod::Day7, TimeseriesMode::Total);
        let line = &r.lines[0];
        assert_eq!(*line.gold.last().unwrap(), 8500);
        assert_eq!(*line.units.last().unwrap(), 2);
    }

    #[test]
    fn timeseries_top_grossing_ranks_by_gold_and_limits_count() {
        let events = vec![
            ts("Cheap Thing", "2026-04-13 09:00:00", 10, 1000),
            ts("Mid Thing", "2026-04-13 09:00:00", 1, 5000),
            ts("Big Thing", "2026-04-13 09:00:00", 1, 40000),
        ];
        let r = aggregate_sales_timeseries(
            events,
            TimeseriesPeriod::Day7,
            TimeseriesMode::TopGrossing(2),
        );
        // Top 2 by gold: Big Thing (40000) then Mid Thing (5000).
        assert_eq!(r.lines.len(), 2);
        assert_eq!(r.lines[0].item, "Big Thing");
        assert_eq!(r.lines[1].item, "Mid Thing");
    }

    #[test]
    fn timeseries_item_mode_anchors_to_that_item() {
        // Other items sold more recently, but item mode anchors on Quality
        // Reins' own latest sale (Apr 10) so its 7-day window is Apr 4..10.
        let events = vec![
            ts("Quality Reins", "2026-04-10 10:00:00", 1, 4500),
            ts("Nice Saddle", "2026-04-20 10:00:00", 1, 4000),
        ];
        let r = aggregate_sales_timeseries(
            events,
            TimeseriesPeriod::Day7,
            TimeseriesMode::Item("Quality Reins".to_string()),
        );
        assert_eq!(r.lines.len(), 1);
        assert_eq!(r.lines[0].item, "Quality Reins");
        assert_eq!(r.periods.last().unwrap().key, "2026-04-10");
        assert_eq!(r.lines[0].total_gold, 4500);
    }

    #[test]
    fn timeseries_day1_uses_hourly_buckets() {
        let events = vec![
            ts("Quality Reins", "2026-04-13 08:30:00", 1, 4500),
            ts("Quality Reins", "2026-04-13 14:15:00", 1, 4500),
        ];
        let r = aggregate_sales_timeseries(events, TimeseriesPeriod::Day1, TimeseriesMode::Total);
        assert_eq!(r.bucket, "hour");
        // 24 hourly buckets ending at 14:00 → 15:00 of the prior day .. 14:00.
        assert_eq!(r.periods.len(), 24);
        assert_eq!(r.periods.last().unwrap().key, "2026-04-13 14");
        let line = &r.lines[0];
        assert_eq!(line.total_gold, 9000);
        // 08:00 sale falls outside the 24h window ending 14:00 the same day?
        // 14:00 - 23h = 15:00 previous day, so 08:30 IS inside.
        assert_eq!(*line.gold.last().unwrap(), 4500); // the 14:15 sale
    }

    #[test]
    fn timeseries_all_time_picks_weekly_for_medium_spans() {
        // ~100-day span exceeds the 92-day daily cap → weekly buckets.
        let events = vec![
            ts("Quality Reins", "2026-01-01 09:00:00", 1, 100),
            ts("Quality Reins", "2026-04-15 09:00:00", 1, 200),
        ];
        let r =
            aggregate_sales_timeseries(events, TimeseriesPeriod::AllTime, TimeseriesMode::Total);
        assert_eq!(r.bucket, "week");
        assert!(r.periods.len() > 12);
    }

    #[test]
    fn timeseries_all_time_picks_monthly_for_long_spans() {
        let events = vec![
            ts("Quality Reins", "2022-01-01 09:00:00", 1, 100),
            ts("Quality Reins", "2026-04-15 09:00:00", 1, 200),
        ];
        let r =
            aggregate_sales_timeseries(events, TimeseriesPeriod::AllTime, TimeseriesMode::Total);
        assert_eq!(r.bucket, "month");
        assert_eq!(r.periods.first().unwrap().key, "2022-01");
        assert_eq!(r.periods.last().unwrap().key, "2026-04");
    }
}
