use super::{queries, DbPool};
use serde::{Deserialize, Serialize};
/// Tauri commands for player data operations (market prices, sales, surveys, etc.)
use tauri::State;

// ── Market Price Commands ─────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct MarketPriceInput {
    pub item_id: u32,
    pub price: f64,
    pub quantity: u32,
    pub vendor_type: String, // "bazaar", "player_vendor", "work_order"
    pub vendor_name: Option<String>,
    pub notes: Option<String>,
}

#[tauri::command]
pub fn add_market_price(db: State<'_, DbPool>, input: MarketPriceInput) -> Result<i64, String> {
    let conn = db
        .get_write()
        .map_err(|e| format!("Database connection error: {e}"))?;

    queries::player_data::insert_market_price(
        &conn,
        input.item_id,
        input.price,
        input.quantity,
        &input.vendor_type,
        input.vendor_name.as_deref(),
        input.notes.as_deref(),
    )
    .map_err(|e| format!("Failed to insert market price: {e}"))
}

#[derive(Serialize)]
pub struct MarketPriceRecord {
    pub id: i64,
    pub item_id: u32,
    pub price: f64,
    pub quantity: u32,
    pub vendor_type: String,
    pub vendor_name: Option<String>,
    pub observed_at: String,
    pub notes: Option<String>,
}

#[tauri::command]
pub fn get_market_prices_for_item(
    db: State<'_, DbPool>,
    item_id: u32,
    limit: Option<usize>,
) -> Result<Vec<MarketPriceRecord>, String> {
    let conn = db
        .get()
        .map_err(|e| format!("Database connection error: {e}"))?;

    let mut stmt = conn
        .prepare(
            "SELECT id, item_id, price, quantity, vendor_type, vendor_name,
                    datetime(observed_at) as observed_at, notes
             FROM market_prices
             WHERE item_id = ?1
             ORDER BY observed_at DESC
             LIMIT ?2",
        )
        .map_err(|e| format!("Failed to prepare query: {e}"))?;

    let limit = limit.unwrap_or(20);
    let rows = stmt
        .query_map([item_id, limit as u32], |row| {
            Ok(MarketPriceRecord {
                id: row.get(0)?,
                item_id: row.get(1)?,
                price: row.get(2)?,
                quantity: row.get(3)?,
                vendor_type: row.get(4)?,
                vendor_name: row.get(5)?,
                observed_at: row.get(6)?,
                notes: row.get(7)?,
            })
        })
        .map_err(|e| format!("Query failed: {e}"))?;

    let mut results = Vec::new();
    for row in rows {
        results.push(row.map_err(|e| format!("Row parse error: {e}"))?);
    }

    Ok(results)
}

// ── Sales History Commands ────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct SaleInput {
    pub item_id: u32,
    pub quantity: u32,
    pub sale_price: f64,
    pub sale_method: String, // "vendor", "bazaar", "trade", "consignment"
    pub buyer_name: Option<String>,
    pub notes: Option<String>,
}

#[tauri::command]
pub fn add_sale(db: State<'_, DbPool>, input: SaleInput) -> Result<i64, String> {
    let conn = db
        .get_write()
        .map_err(|e| format!("Database connection error: {e}"))?;

    queries::player_data::insert_sale(
        &conn,
        input.item_id,
        input.quantity,
        input.sale_price,
        &input.sale_method,
        input.buyer_name.as_deref(),
        input.notes.as_deref(),
    )
    .map_err(|e| format!("Failed to insert sale: {e}"))
}

#[derive(Serialize)]
pub struct SaleRecord {
    pub id: i64,
    pub item_id: u32,
    pub quantity: u32,
    pub sale_price: f64,
    pub sale_method: String,
    pub buyer_name: Option<String>,
    pub sold_at: String,
    pub notes: Option<String>,
}

#[tauri::command]
pub fn get_recent_sales(
    db: State<'_, DbPool>,
    days: Option<u32>,
    limit: Option<usize>,
) -> Result<Vec<SaleRecord>, String> {
    let conn = db
        .get()
        .map_err(|e| format!("Database connection error: {e}"))?;

    let days = days.unwrap_or(30);
    let limit = limit.unwrap_or(50);

    let mut stmt = conn
        .prepare(
            "SELECT id, item_id, quantity, sale_price, sale_method, buyer_name,
                    datetime(sold_at) as sold_at, notes
             FROM sales_history
             WHERE sold_at > datetime('now', ?1)
             ORDER BY sold_at DESC
             LIMIT ?2",
        )
        .map_err(|e| format!("Failed to prepare query: {e}"))?;

    let rows = stmt
        .query_map([format!("-{} days", days), limit.to_string()], |row| {
            Ok(SaleRecord {
                id: row.get(0)?,
                item_id: row.get(1)?,
                quantity: row.get(2)?,
                sale_price: row.get(3)?,
                sale_method: row.get(4)?,
                buyer_name: row.get(5)?,
                sold_at: row.get(6)?,
                notes: row.get(7)?,
            })
        })
        .map_err(|e| format!("Query failed: {e}"))?;

    let mut results = Vec::new();
    for row in rows {
        results.push(row.map_err(|e| format!("Row parse error: {e}"))?);
    }

    Ok(results)
}

// ── Event Log Commands ────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct LogEventInput {
    pub event_type: String,
    pub event_data: serde_json::Value,
}

#[tauri::command]
pub fn log_event(db: State<'_, DbPool>, input: LogEventInput) -> Result<i64, String> {
    let conn = db
        .get_write()
        .map_err(|e| format!("Database connection error: {e}"))?;

    let event_data_json = serde_json::to_string(&input.event_data)
        .map_err(|e| format!("Failed to serialize event data: {e}"))?;

    queries::player_data::log_event(&conn, &input.event_type, &event_data_json)
        .map_err(|e| format!("Failed to log event: {e}"))
}

#[derive(Serialize)]
pub struct EventLogRecord {
    pub id: i64,
    pub event_type: String,
    pub event_data: serde_json::Value,
    pub created_at: String,
}

#[tauri::command]
pub fn get_recent_events(
    db: State<'_, DbPool>,
    event_type: Option<String>,
    limit: Option<usize>,
) -> Result<Vec<EventLogRecord>, String> {
    let conn = db
        .get()
        .map_err(|e| format!("Database connection error: {e}"))?;

    let limit: i64 = limit.unwrap_or(50) as i64;

    let mut results = Vec::new();

    if let Some(evt) = event_type {
        let mut stmt = conn
            .prepare(
                "SELECT id, event_type, event_data, datetime(created_at) as created_at
                 FROM event_log
                 WHERE event_type = ?1
                 ORDER BY created_at DESC
                 LIMIT ?2",
            )
            .map_err(|e| format!("Failed to prepare query: {e}"))?;

        let rows = stmt
            .query_map([evt, limit.to_string()], |row| {
                let event_data_str: String = row.get(2)?;
                let event_data: serde_json::Value =
                    serde_json::from_str(&event_data_str).unwrap_or(serde_json::Value::Null);

                Ok(EventLogRecord {
                    id: row.get(0)?,
                    event_type: row.get(1)?,
                    event_data,
                    created_at: row.get(3)?,
                })
            })
            .map_err(|e| format!("Query failed: {e}"))?;

        for row in rows {
            results.push(row.map_err(|e| format!("Row parse error: {e}"))?);
        }
    } else {
        let mut stmt = conn
            .prepare(
                "SELECT id, event_type, event_data, datetime(created_at) as created_at
                 FROM event_log
                 ORDER BY created_at DESC
                 LIMIT ?1",
            )
            .map_err(|e| format!("Failed to prepare query: {e}"))?;

        let rows = stmt
            .query_map([limit], |row| {
                let event_data_str: String = row.get(2)?;
                let event_data: serde_json::Value =
                    serde_json::from_str(&event_data_str).unwrap_or(serde_json::Value::Null);

                Ok(EventLogRecord {
                    id: row.get(0)?,
                    event_type: row.get(1)?,
                    event_data,
                    created_at: row.get(3)?,
                })
            })
            .map_err(|e| format!("Query failed: {e}"))?;

        for row in rows {
            results.push(row.map_err(|e| format!("Row parse error: {e}"))?);
        }
    }

    Ok(results)
}
