use super::DbPool;
use crate::settings::SettingsManager;
use serde::{Deserialize, Serialize};
/// Tauri commands for market values — user-specified "sells for X to players" prices.
///
/// Supports two modes controlled by settings.market_price_mode:
/// - "universal": all prices stored with server_name = "*"
/// - "per_server": prices stored per actual server_name
use std::sync::Arc;
use tauri::State;

// ── Types ────────────────────────────────────────────────────────────────────

#[derive(Serialize, Deserialize, Clone)]
pub struct MarketValue {
    pub server_name: String,
    pub item_type_id: i32,
    pub item_name: String,
    pub market_value: i64,
    pub notes: Option<String>,
    pub updated_at: String,
}

#[derive(Deserialize)]
pub struct MarketValueImportEntry {
    pub item_type_id: i32,
    pub item_name: String,
    pub market_value: i64,
    pub notes: Option<String>,
    pub updated_at: String,
}

#[derive(Serialize)]
pub struct ImportMarketValuesResult {
    pub imported: usize,
    pub skipped: usize,
    pub updated: usize,
}

// ── Helpers ──────────────────────────────────────────────────────────────────

/// Resolve the server_name to use for DB operations based on market_price_mode.
fn resolve_server(settings: &SettingsManager, server_name: &Option<String>) -> String {
    let mode = &settings.get().market_price_mode;
    if mode == "universal" {
        "*".to_string()
    } else {
        server_name
            .clone()
            .or_else(|| settings.get().active_server_name.clone())
            .unwrap_or_else(|| "*".to_string())
    }
}

// ── Commands ─────────────────────────────────────────────────────────────────

#[tauri::command]
pub fn get_market_values(
    db: State<'_, DbPool>,
    settings_manager: State<'_, Arc<SettingsManager>>,
    server_name: Option<String>,
) -> Result<Vec<MarketValue>, String> {
    let server = resolve_server(&settings_manager, &server_name);
    let conn = db.get().map_err(|e| format!("Database error: {e}"))?;

    let mut stmt = conn
        .prepare(
            "SELECT server_name, item_type_id, item_name, market_value, notes, updated_at
         FROM market_values WHERE server_name = ?1
         ORDER BY item_name",
        )
        .map_err(|e| format!("Query error: {e}"))?;

    let rows = stmt
        .query_map(rusqlite::params![server], |row| {
            Ok(MarketValue {
                server_name: row.get(0)?,
                item_type_id: row.get(1)?,
                item_name: row.get(2)?,
                market_value: row.get(3)?,
                notes: row.get(4)?,
                updated_at: row.get(5)?,
            })
        })
        .map_err(|e| format!("Query error: {e}"))?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("Row error: {e}"))
}

#[tauri::command]
pub fn get_market_value(
    db: State<'_, DbPool>,
    settings_manager: State<'_, Arc<SettingsManager>>,
    item_type_id: i32,
    server_name: Option<String>,
) -> Result<Option<MarketValue>, String> {
    let server = resolve_server(&settings_manager, &server_name);
    let conn = db.get().map_err(|e| format!("Database error: {e}"))?;

    let result = conn.query_row(
        "SELECT server_name, item_type_id, item_name, market_value, notes, updated_at
         FROM market_values WHERE server_name = ?1 AND item_type_id = ?2",
        rusqlite::params![server, item_type_id],
        |row| {
            Ok(MarketValue {
                server_name: row.get(0)?,
                item_type_id: row.get(1)?,
                item_name: row.get(2)?,
                market_value: row.get(3)?,
                notes: row.get(4)?,
                updated_at: row.get(5)?,
            })
        },
    );

    match result {
        Ok(v) => Ok(Some(v)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(format!("Query error: {e}")),
    }
}

#[tauri::command]
pub fn set_market_value(
    db: State<'_, DbPool>,
    settings_manager: State<'_, Arc<SettingsManager>>,
    item_type_id: i32,
    item_name: String,
    market_value: i64,
    notes: Option<String>,
    server_name: Option<String>,
) -> Result<(), String> {
    let server = resolve_server(&settings_manager, &server_name);
    let conn = db.get_write().map_err(|e| format!("Database error: {e}"))?;

    conn.execute(
        "INSERT INTO market_values (server_name, item_type_id, item_name, market_value, notes, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, datetime('now'))
         ON CONFLICT(server_name, item_type_id) DO UPDATE SET
            item_name = excluded.item_name,
            market_value = excluded.market_value,
            notes = excluded.notes,
            updated_at = excluded.updated_at",
        rusqlite::params![server, item_type_id, item_name, market_value, notes],
    ).map_err(|e| format!("Insert error: {e}"))?;

    Ok(())
}

#[tauri::command]
pub fn delete_market_value(
    db: State<'_, DbPool>,
    settings_manager: State<'_, Arc<SettingsManager>>,
    item_type_id: i32,
    server_name: Option<String>,
) -> Result<(), String> {
    let server = resolve_server(&settings_manager, &server_name);
    let conn = db.get_write().map_err(|e| format!("Database error: {e}"))?;

    conn.execute(
        "DELETE FROM market_values WHERE server_name = ?1 AND item_type_id = ?2",
        rusqlite::params![server, item_type_id],
    )
    .map_err(|e| format!("Delete error: {e}"))?;

    Ok(())
}

#[tauri::command]
pub fn export_market_values(
    db: State<'_, DbPool>,
    settings_manager: State<'_, Arc<SettingsManager>>,
    server_name: Option<String>,
) -> Result<String, String> {
    let values = get_market_values_internal(&db, &settings_manager, &server_name)?;
    serde_json::to_string_pretty(&values).map_err(|e| format!("Serialization error: {e}"))
}

#[tauri::command]
pub fn import_market_values(
    db: State<'_, DbPool>,
    settings_manager: State<'_, Arc<SettingsManager>>,
    json_data: String,
    strategy: String,
    server_name: Option<String>,
) -> Result<ImportMarketValuesResult, String> {
    let entries: Vec<MarketValueImportEntry> =
        serde_json::from_str(&json_data).map_err(|e| format!("Invalid JSON: {e}"))?;

    let server = resolve_server(&settings_manager, &server_name);
    let mut conn = db.get_write().map_err(|e| format!("Database error: {e}"))?;

    let mut imported = 0usize;
    let mut skipped = 0usize;
    let mut updated = 0usize;

    let tx = conn
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .map_err(|e| format!("Failed to start transaction: {e}"))?;

    for entry in &entries {
        // Check for existing value
        let existing: Option<String> = tx
            .query_row(
                "SELECT updated_at FROM market_values WHERE server_name = ?1 AND item_type_id = ?2",
                rusqlite::params![server, entry.item_type_id],
                |row| row.get(0),
            )
            .ok();

        let should_write = match (existing.as_deref(), strategy.as_str()) {
            (None, _) => true,                   // No existing value — always import
            (Some(_), "overwrite") => true,      // Always overwrite
            (Some(_), "keep_existing") => false, // Keep what we have
            (Some(existing_ts), "newest") => {
                // Accept newest by updated_at
                entry.updated_at.as_str() > existing_ts
            }
            _ => true, // Default: import
        };

        if should_write {
            tx.execute(
                "INSERT INTO market_values (server_name, item_type_id, item_name, market_value, notes, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                 ON CONFLICT(server_name, item_type_id) DO UPDATE SET
                    item_name = excluded.item_name,
                    market_value = excluded.market_value,
                    notes = excluded.notes,
                    updated_at = excluded.updated_at",
                rusqlite::params![server, entry.item_type_id, entry.item_name, entry.market_value, entry.notes, entry.updated_at],
            ).map_err(|e| format!("Import error: {e}"))?;

            if existing.is_some() {
                updated += 1;
            } else {
                imported += 1;
            }
        } else {
            skipped += 1;
        }
    }

    tx.commit().map_err(|e| format!("Commit failed: {e}"))?;

    Ok(ImportMarketValuesResult {
        imported,
        skipped,
        updated,
    })
}

// ── Bulk operations ─────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct BulkMarketUpdate {
    pub item_type_id: i32,
    pub market_value: i64,
}

#[tauri::command]
pub fn bulk_update_market_values(
    db: State<'_, DbPool>,
    settings_manager: State<'_, Arc<SettingsManager>>,
    updates: Vec<BulkMarketUpdate>,
    server_name: Option<String>,
) -> Result<usize, String> {
    let server = resolve_server(&settings_manager, &server_name);
    let mut conn = db.get_write().map_err(|e| format!("Database error: {e}"))?;

    let tx = conn
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .map_err(|e| format!("Failed to start transaction: {e}"))?;

    let mut count = 0usize;
    for update in &updates {
        let affected = tx
            .execute(
                "UPDATE market_values SET market_value = ?1, updated_at = datetime('now')
                 WHERE server_name = ?2 AND item_type_id = ?3",
                rusqlite::params![update.market_value, server, update.item_type_id],
            )
            .map_err(|e| format!("Update error: {e}"))?;
        count += affected;
    }

    tx.commit().map_err(|e| format!("Commit failed: {e}"))?;
    Ok(count)
}

#[tauri::command]
pub fn bulk_delete_market_values(
    db: State<'_, DbPool>,
    settings_manager: State<'_, Arc<SettingsManager>>,
    item_ids: Vec<i32>,
    server_name: Option<String>,
) -> Result<usize, String> {
    if item_ids.is_empty() {
        return Ok(0);
    }
    let server = resolve_server(&settings_manager, &server_name);
    let conn = db.get_write().map_err(|e| format!("Database error: {e}"))?;

    // Array binding via rarray: one ?1 + ?2 instead of N placeholders.
    use std::rc::Rc;
    let sql =
        "DELETE FROM market_values WHERE server_name = ?1 AND item_type_id IN (SELECT value FROM rarray(?2))";

    let ids: Rc<Vec<rusqlite::types::Value>> = Rc::new(
        item_ids
            .iter()
            .map(|id| rusqlite::types::Value::Integer(*id as i64))
            .collect(),
    );

    let deleted = conn
        .execute(sql, rusqlite::params![server, ids])
        .map_err(|e| format!("Delete error: {e}"))?;

    Ok(deleted)
}

// ── Internal helpers ─────────────────────────────────────────────────────────

fn get_market_values_internal(
    db: &DbPool,
    settings_manager: &SettingsManager,
    server_name: &Option<String>,
) -> Result<Vec<MarketValue>, String> {
    let mode = &settings_manager.get().market_price_mode;
    let server = if mode == "universal" {
        "*".to_string()
    } else {
        server_name
            .clone()
            .or_else(|| settings_manager.get().active_server_name.clone())
            .unwrap_or_else(|| "*".to_string())
    };

    // Read-only helper (single SELECT) — keep on the read pool; routing it to
    // get_write() would queue market reads behind live ingest for no reason.
    let conn = db.get().map_err(|e| format!("Database error: {e}"))?;
    let mut stmt = conn
        .prepare(
            "SELECT server_name, item_type_id, item_name, market_value, notes, updated_at
         FROM market_values WHERE server_name = ?1
         ORDER BY item_name",
        )
        .map_err(|e| format!("Query error: {e}"))?;

    let rows = stmt
        .query_map(rusqlite::params![server], |row| {
            Ok(MarketValue {
                server_name: row.get(0)?,
                item_type_id: row.get(1)?,
                item_name: row.get(2)?,
                market_value: row.get(3)?,
                notes: row.get(4)?,
                updated_at: row.get(5)?,
            })
        })
        .map_err(|e| format!("Query error: {e}"))?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("Row error: {e}"))
}
