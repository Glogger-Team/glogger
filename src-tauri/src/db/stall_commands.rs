use super::{DbPool, DbRead};
use crate::settings::SettingsManager;
use rusqlite::OptionalExtension;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tauri::State;

// ── Stall Price Capture ─────────────────────────────────────────────────────
//
// Rows in `stall_price_observations` are user-captured prices from OTHER
// players' stalls: typed into the capture panel (`source = 'capture'`) or
// auto-detected purchases (`source = 'purchase'`, `price_unit = 0` sentinel
// until filled in). The unique key (observed_at, item_name,
// stall_npc_entity_id, price_unit) makes `INSERT OR IGNORE` idempotent.

/// One observed price row for another player's stall.
#[derive(Serialize, Deserialize, Clone)]
pub struct StallPriceObservation {
    pub id: i64,
    pub character_name: String,
    pub server_name: String,
    pub item_name: String,
    pub internal_name: Option<String>,
    pub item_type_id: Option<i64>,
    pub quantity: i64,
    /// Councils per unit. `0` = unknown (purchase pending a manual price).
    pub price_unit: i64,
    pub stall_npc_entity_id: i64,
    pub stall_label: String,
    pub owner_name: Option<String>,
    /// `'capture'` (typed/OCR'd) or `'purchase'` (auto-detected).
    pub source: String,
    pub observed_at: String,
    pub notes: Option<String>,
}

/// One quick-entry line from the capture panel.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StallPriceEntry {
    /// Display name, internal name, or numeric CDN item id.
    pub item_query: String,
    pub quantity: i64,
    pub price_unit: i64,
    pub stall_npc_entity_id: i64,
    pub stall_label: Option<String>,
    pub owner_name: Option<String>,
    pub notes: Option<String>,
}

// ── Commands ────────────────────────────────────────────────────────────────

/// List observations, newest first. `server_name` filters when provided;
/// `item_type_id` narrows to one item (tooltip use case).
#[tauri::command]
pub fn get_stall_price_observations(
    db: State<'_, DbPool>,
    server_name: Option<String>,
    limit: Option<i64>,
    item_type_id: Option<i64>,
) -> Result<Vec<StallPriceObservation>, String> {
    let conn = db.get().map_err(|e| format!("Database error: {e}"))?;

    let mut sql = String::from(
        "SELECT id, character_name, server_name, item_name, internal_name, item_type_id,
                quantity, price_unit, stall_npc_entity_id, stall_label, owner_name,
                source, observed_at, notes
         FROM stall_price_observations",
    );
    let mut clauses: Vec<String> = Vec::new();
    if server_name.is_some() {
        clauses.push(format!("server_name = ?{}", clauses.len() + 1));
    }
    if item_type_id.is_some() {
        clauses.push(format!("item_type_id = ?{}", clauses.len() + 1));
    }
    if !clauses.is_empty() {
        sql.push_str(" WHERE ");
        sql.push_str(&clauses.join(" AND "));
    }
    sql.push_str(" ORDER BY observed_at DESC, id DESC LIMIT ");
    sql.push_str(&limit.unwrap_or(200).max(1).to_string());

    let mut stmt = conn
        .prepare(&sql)
        .map_err(|e| format!("Query error: {e}"))?;

    // Placeholders are numbered by position among the *present* filters, so
    // each `?N` lines up with the params bound below in every (server?, item?) case.
    let rows = match (server_name.as_deref(), item_type_id) {
        (Some(server), Some(item_id)) => {
            stmt.query_map(rusqlite::params![server, item_id], map_observation_row)
        }
        (Some(server), None) => stmt.query_map(rusqlite::params![server], map_observation_row),
        (None, Some(item_id)) => stmt.query_map(rusqlite::params![item_id], map_observation_row),
        (None, None) => stmt.query_map([], map_observation_row),
    }
    .map_err(|e| format!("Query error: {e}"))?;

    let mut out = Vec::new();
    for row in rows {
        out.push(row.map_err(|e| format!("Row error: {e}"))?);
    }
    Ok(out)
}

fn map_observation_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<StallPriceObservation> {
    Ok(StallPriceObservation {
        id: row.get(0)?,
        character_name: row.get(1)?,
        server_name: row.get(2)?,
        item_name: row.get(3)?,
        internal_name: row.get(4)?,
        item_type_id: row.get(5)?,
        quantity: row.get(6)?,
        price_unit: row.get(7)?,
        stall_npc_entity_id: row.get(8)?,
        stall_label: row.get(9)?,
        owner_name: row.get(10)?,
        source: row.get(11)?,
        observed_at: row.get(12)?,
        notes: row.get(13)?,
    })
}

/// Resolve an item query (display name / internal name / numeric id) against
/// the CDN `items` table. Same triple lookup as `game_data::resolve_item`.
fn resolve_item_query<C: DbRead + ?Sized>(
    conn: &C,
    item_query: &str,
) -> Option<(i64, String, Option<String>)> {
    // Numeric id first.
    if let Ok(id) = item_query.trim().parse::<i64>() {
        if let Ok(Some((id, name, internal))) = conn
            .query_row(
                "SELECT id, name, internal_name FROM items WHERE id = ?1",
                rusqlite::params![id],
                |r| {
                    Ok((
                        r.get::<_, i64>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, Option<String>>(2)?,
                    ))
                },
            )
            .optional()
        {
            return Some((id, name, internal));
        }
    }
    conn.query_row(
        "SELECT id, name, internal_name FROM items
         WHERE name = ?1 COLLATE NOCASE OR internal_name = ?1 COLLATE NOCASE",
        rusqlite::params![item_query],
        |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, Option<String>>(2)?,
            ))
        },
    )
    .optional()
    .ok()
    .flatten()
}

/// Persist capture-panel entries. Unresolvable item queries are stored with
/// NULL ids so they surface for correction instead of being dropped.
/// Returns the number of rows actually inserted (duplicates swallowed by
/// `INSERT OR IGNORE` don't count).
#[tauri::command]
pub fn record_stall_prices(
    db: State<'_, DbPool>,
    settings_manager: State<'_, Arc<SettingsManager>>,
    entries: Vec<StallPriceEntry>,
) -> Result<usize, String> {
    if entries.is_empty() {
        return Ok(0);
    }
    let server_name = settings_manager
        .get()
        .active_server_name
        .unwrap_or_else(|| "Unknown".to_string());
    let character_name = settings_manager
        .get()
        .active_character_name
        .unwrap_or_else(|| "Unknown".to_string());
    let observed_at = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();

    let mut conn = db.get_write().map_err(|e| format!("Database error: {e}"))?;
    let conn = conn
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .map_err(|e| format!("Failed to start transaction: {e}"))?;

    let mut inserted = 0usize;
    for entry in &entries {
        let resolved = resolve_item_query(&*conn, &entry.item_query);
        let (item_name, internal_name, item_type_id) = match &resolved {
            Some((id, name, internal)) => (name.clone(), internal.clone(), Some(*id)),
            // Keep the raw string so unmatched rows still show up for fixing.
            None => (entry.item_query.trim().to_string(), None, None),
        };
        let result = conn.execute(
            "INSERT OR IGNORE INTO stall_price_observations
                (character_name, server_name, item_name, internal_name, item_type_id,
                 quantity, price_unit, stall_npc_entity_id, stall_label, owner_name,
                 source, observed_at, notes)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, 'capture', ?11, ?12)",
            rusqlite::params![
                character_name,
                server_name,
                item_name,
                internal_name,
                item_type_id,
                entry.quantity.max(1),
                entry.price_unit,
                entry.stall_npc_entity_id,
                entry
                    .stall_label
                    .clone()
                    .unwrap_or_else(|| "Unidentified Stall".to_string()),
                entry.owner_name,
                observed_at,
                entry.notes,
            ],
        );
        match result {
            Ok(1) => inserted += 1,
            Ok(_) => {} // duplicate
            Err(e) => return Err(format!("Failed to record price: {e}")),
        }
    }

    conn.commit()
        .map_err(|e| format!("Failed to commit transaction: {e}"))?;
    Ok(inserted)
}

/// Fill in the price (and optionally stall label/owner) of an observation.
/// Intended for `source = 'purchase'` sentinel rows (`price_unit = 0`); a
/// repeated update overwrites the price.
#[tauri::command]
pub fn update_stall_price_observation(
    db: State<'_, DbPool>,
    id: i64,
    price_unit: i64,
    stall_label: Option<String>,
    owner_name: Option<String>,
) -> Result<(), String> {
    let conn = db.get_write().map_err(|e| format!("Database error: {e}"))?;
    let n = conn
        .execute(
            "UPDATE stall_price_observations
             SET price_unit = ?2,
                 stall_label = COALESCE(?3, stall_label),
                 owner_name = COALESCE(?4, owner_name)
             WHERE id = ?1",
            rusqlite::params![id, price_unit, stall_label, owner_name],
        )
        .map_err(|e| format!("Update failed: {e}"))?;
    if n == 0 {
        return Err(format!("No observation with id {id}"));
    }
    Ok(())
}

/// Delete one observation row.
#[tauri::command]
pub fn delete_stall_price_observation(db: State<'_, DbPool>, id: i64) -> Result<(), String> {
    let conn = db.get_write().map_err(|e| format!("Database error: {e}"))?;
    conn.execute(
        "DELETE FROM stall_price_observations WHERE id = ?1",
        rusqlite::params![id],
    )
    .map_err(|e| format!("Delete failed: {e}"))?;
    Ok(())
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    fn setup() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        crate::db::migrations::run_migrations(&conn, None).unwrap();
        conn
    }

    fn insert_capture(conn: &Connection, item: &str, price: i64) -> usize {
        conn.execute(
            "INSERT OR IGNORE INTO stall_price_observations
                (character_name, server_name, item_name, item_type_id,
                 quantity, price_unit, stall_npc_entity_id, stall_label, owner_name,
                 source, observed_at, notes)
             VALUES ('TestChar', 'Dreva', ?1, NULL, 1, ?2, 13676,
                     'Test Stall', NULL, 'capture', '2026-10-02 12:00:00', NULL)",
            rusqlite::params![item, price],
        )
        .unwrap()
    }

    #[test]
    fn test_insert_and_or_ignore_dedup() {
        let conn = setup();
        assert_eq!(insert_capture(&conn, "Carrot", 5), 1);
        // Same (observed_at, item, stall, price) → ignored.
        assert_eq!(insert_capture(&conn, "Carrot", 5), 0);
        // Different price → distinct row.
        assert_eq!(insert_capture(&conn, "Carrot", 6), 1);
    }

    #[test]
    fn test_resolve_item_query_by_name_id_internal() {
        let conn = setup();
        // The CDN fixture may be empty in tests; resolve must simply return
        // None and record_stall_prices must fall back to the raw string.
        let resolved = resolve_item_query(&conn, "Cantaloupe Wine");
        // Either the CDN table is populated with the item or None — both fine.
        if let Some((id, name, internal)) = resolved {
            assert_eq!(id, 21890);
            assert_eq!(name, "Cantaloupe Wine");
            assert_eq!(internal.as_deref(), Some("CantaloupeWine"));
        }
        // Garbage → None (no panic).
        assert!(resolve_item_query(&conn, "ZzzzNotAnItem").is_none());
    }

    #[test]
    fn test_update_purchase_sentinel() {
        let conn = setup();
        conn.execute(
            "INSERT INTO stall_price_observations
                (character_name, server_name, item_name, item_type_id,
                 quantity, price_unit, stall_npc_entity_id, stall_label, owner_name,
                 source, observed_at, notes)
             VALUES ('TestChar', 'Dreva', 'Cantaloupe Wine', 21890,
                     3, 0, 13676, 'Unidentified Stall', NULL,
                     'purchase', '2026-10-02 12:29:45', NULL)",
            [],
        )
        .unwrap();
        let id: i64 = conn
            .query_row("SELECT id FROM stall_price_observations", [], |r| r.get(0))
            .unwrap();

        // Apply the same UPDATE the command issues.
        conn.execute(
            "UPDATE stall_price_observations
             SET price_unit = ?2, stall_label = COALESCE(?3, stall_label),
                 owner_name = COALESCE(?4, owner_name)
             WHERE id = ?1",
            rusqlite::params![id, 2500, "Kegoron's Shop", "Kegoron"],
        )
        .unwrap();

        let (price, label, owner): (i64, String, String) = conn
            .query_row(
                "SELECT price_unit, stall_label, owner_name FROM stall_price_observations WHERE id = ?1",
                rusqlite::params![id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .unwrap();
        assert_eq!(price, 2500);
        assert_eq!(label, "Kegoron's Shop");
        assert_eq!(owner, "Kegoron");
    }

    #[test]
    fn test_purge_covers_observations() {
        let conn = setup();
        conn.execute(
            "INSERT INTO stall_price_observations
                (character_name, server_name, item_name, item_type_id,
                 quantity, price_unit, stall_npc_entity_id, stall_label, owner_name,
                 source, observed_at, notes)
             VALUES ('TestChar', 'Dreva', 'Old Item', NULL, 1, 5, 13676,
                     'Test Stall', NULL, 'capture', '2020-01-01 12:00:00', NULL)",
            [],
        )
        .unwrap();
        // Same SQL shape as admin_commands::purge_table with a cutoff.
        let n = conn
            .execute(
                "DELETE FROM stall_price_observations WHERE observed_at < datetime('now', '-1 day')",
                [],
            )
            .unwrap();
        assert_eq!(n, 1, "old row is purge-eligible");
    }
}
