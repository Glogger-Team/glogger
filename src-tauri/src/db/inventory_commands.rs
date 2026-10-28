use super::DbPool;
use serde::{Deserialize, Serialize};
/// Tauri commands for inventory report import and querying
use std::collections::HashMap;
use tauri::State;

// ── JSON Deserialization Structs (match game's Storage export format: Export Storage as JSON in Settings → VIP) ───────────

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct StorageReport {
    pub character: String,
    pub server_name: String,
    pub timestamp: String,
    pub report: String,
    #[allow(dead_code)]
    pub report_version: u32,
    pub items: Vec<StorageItem>,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct StorageItem {
    #[serde(rename = "TypeID")]
    pub type_id: i64,
    #[serde(default)]
    pub storage_vault: String,
    pub stack_size: i64,
    pub value: Option<f64>,
    pub name: String,
    pub pet_husbandry_state: Option<String>,
    pub is_in_inventory: Option<bool>,
    pub is_crafted: Option<bool>,
    pub crafter: Option<String>,
    pub rarity: Option<String>,
    pub slot: Option<String>,
    pub level: Option<i64>,
    #[serde(rename = "TSysPowers")]
    pub tsys_powers: Option<Vec<TsysPower>>,
    #[serde(rename = "TSysImbuePower")]
    pub tsys_imbue_power: Option<String>,
    #[serde(rename = "TSysImbuePowerTier")]
    pub tsys_imbue_power_tier: Option<i64>,
    pub durability: Option<f64>,
    pub craft_points: Option<i64>,
    pub uses_remaining: Option<i64>,
    pub transmute_count: Option<i64>,
    pub attuned_to: Option<String>,
    #[serde(default)]
    pub is_augmented: Option<bool>,
    #[serde(default)]
    pub dye_count: Option<i64>,
    #[serde(default)]
    pub inscription: Option<String>,
    #[serde(default)]
    pub inscriber: Option<String>,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct TsysPower {
    pub tier: i64,
    pub power: String,
}

// ── Command Response Types ───────────────────────────────────────────────────

#[derive(Serialize)]
pub struct InventoryImportResult {
    pub character_name: String,
    pub server_name: String,
    pub snapshot_timestamp: String,
    pub items_imported: usize,
    pub was_duplicate: bool,
}

#[derive(Serialize)]
pub struct InventorySnapshotSummary {
    pub id: i64,
    pub character_name: String,
    pub server_name: String,
    pub snapshot_timestamp: String,
    pub import_date: String,
    pub item_count: i64,
}

#[derive(Serialize)]
pub struct SnapshotItem {
    pub id: i64,
    pub type_id: i64,
    pub storage_vault: String,
    pub is_in_inventory: bool,
    pub stack_size: i64,
    pub value: Option<f64>,
    pub item_name: String,
    pub rarity: Option<String>,
    pub slot: Option<String>,
    pub level: Option<i64>,
    pub is_crafted: bool,
    pub crafter: Option<String>,
    pub durability: Option<f64>,
    pub craft_points: Option<i64>,
    pub uses_remaining: Option<i64>,
    pub transmute_count: Option<i64>,
    pub attuned_to: Option<String>,
    pub tsys_powers: Option<String>,
    pub tsys_imbue_power: Option<String>,
    pub tsys_imbue_power_tier: Option<i64>,
    pub pet_husbandry_state: Option<String>,
    pub is_augmented: Option<bool>,
    pub dye_count: Option<i64>,
    pub inscription: Option<String>,
    pub inscriber: Option<String>,
}

#[derive(Serialize)]
pub struct InventorySummary {
    pub total_items: i64,
    pub total_stacks: i64,
    pub total_value: f64,
    pub unique_items: i64,
    pub items_by_vault: HashMap<String, i64>,
    pub items_by_rarity: HashMap<String, i64>,
}

/// One item row from the latest inventory snapshot of a given character,
/// used by the cross-character storage search.
#[derive(Serialize)]
pub struct CrossCharacterItem {
    pub character_name: String,
    pub server_name: String,
    pub item_name: String,
    pub storage_vault: String,
    pub is_in_inventory: bool,
    pub stack_size: i64,
    pub type_id: i64,
    pub rarity: Option<String>,
    pub value: Option<f64>,
}

// ── Commands ─────────────────────────────────────────────────────────────────

#[tauri::command]
pub fn import_inventory_report(
    db: State<'_, DbPool>,
    file_path: String,
) -> Result<InventoryImportResult, String> {
    import_inventory_report_internal(&db, &file_path)
}

/// Internal import logic, callable without Tauri State wrapper
pub fn import_inventory_report_internal(
    db: &DbPool,
    file_path: &str,
) -> Result<InventoryImportResult, String> {
    // 1. Read file
    let raw_json =
        std::fs::read_to_string(file_path).map_err(|e| format!("Failed to read file: {e}"))?;

    // 2. Deserialize
    let report: StorageReport = serde_json::from_str(&raw_json)
        .map_err(|e| format!("Failed to parse inventory report: {e}"))?;

    // 3. Validate report type
    if report.report != "Storage" {
        return Err(format!(
            "Wrong report type: expected \"Storage\", got \"{}\"",
            report.report
        ));
    }

    let mut conn = db
        .get_write()
        .map_err(|e| format!("Database connection error: {e}"))?;

    // 4. Begin transaction
    let conn = conn
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .map_err(|e| format!("Failed to begin transaction: {e}"))?;

    let result = (|| -> Result<InventoryImportResult, String> {
        // 5. Insert snapshot (skip duplicates)
        conn.execute(
            "INSERT INTO character_item_snapshots (character_name, server_name, snapshot_timestamp, raw_json)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(character_name, server_name, snapshot_timestamp) DO NOTHING",
            rusqlite::params![
                report.character,
                report.server_name,
                report.timestamp,
                raw_json,
            ],
        ).map_err(|e| format!("Failed to store storage report: {e}"))?;

        // 6. Check if insert created a new row. Duplicates still refresh the
        // four v81 detail columns: already-imported reports predate them, so
        // their rows exist with NULLs. Re-import is the backfill path — update
        // existing rows when the file carries values (per-item overwrite, so
        // re-importing an older export after a newer one can only fill NULLs
        // when the older file omits the keys via serde default).
        let changes = conn.changes();
        if changes == 0 {
            let snapshot_id: i64 = conn
                .query_row(
                    "SELECT id FROM character_item_snapshots WHERE character_name = ?1 AND server_name = ?2 AND snapshot_timestamp = ?3",
                    rusqlite::params![report.character, report.server_name, report.timestamp],
                    |row| row.get(0),
                )
                .map_err(|e| format!("Failed to locate existing storage report: {e}"))?;
            let mut backfill_stmt = conn
                .prepare(
                    "UPDATE character_snapshot_items
                     SET is_augmented = COALESCE(?1, is_augmented),
                         dye_count = COALESCE(?2, dye_count),
                         inscription = COALESCE(?3, inscription),
                         inscriber = COALESCE(?4, inscriber)
                     WHERE item_snapshot_id = ?5 AND type_id = ?6 AND storage_vault = ?7",
                )
                .map_err(|e| format!("Failed to prepare detail backfill: {e}"))?;
            let mut items_imported = 0usize;
            for item in &report.items {
                items_imported += backfill_stmt
                    .execute(rusqlite::params![
                        item.is_augmented,
                        item.dye_count,
                        item.inscription,
                        item.inscriber,
                        snapshot_id,
                        item.type_id,
                        item.storage_vault,
                    ])
                    .map_err(|e| format!("Failed to backfill item {}: {e}", item.name))?;
            }
            return Ok(InventoryImportResult {
                character_name: report.character.clone(),
                server_name: report.server_name.clone(),
                snapshot_timestamp: report.timestamp.clone(),
                items_imported,
                was_duplicate: true,
            });
        }

        let snapshot_id = conn.last_insert_rowid();

        // 7. Batch insert items
        let mut item_stmt = conn.prepare(
            "INSERT INTO character_snapshot_items (
                item_snapshot_id, type_id, storage_vault, is_in_inventory,
                stack_size, value, item_name, rarity, slot, level,
                is_crafted, crafter, durability, craft_points, uses_remaining,
                transmute_count, attuned_to, tsys_powers, tsys_imbue_power,
                tsys_imbue_power_tier, pet_husbandry_state,
                is_augmented, dye_count, inscription, inscriber
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25)"
        ).map_err(|e| format!("Failed to prepare item insert: {e}"))?;

        let mut items_imported = 0usize;
        for item in &report.items {
            let is_inv = item.is_in_inventory.unwrap_or(false);
            let is_crafted = item.is_crafted.unwrap_or(false);

            // Serialize TSysPowers to JSON string if present
            let tsys_powers_json = item
                .tsys_powers
                .as_ref()
                .map(|powers| serde_json::to_string(powers).unwrap_or_default());

            items_imported += item_stmt
                .execute(rusqlite::params![
                    snapshot_id,
                    item.type_id,
                    item.storage_vault,
                    is_inv,
                    item.stack_size,
                    item.value,
                    item.name,
                    item.rarity,
                    item.slot,
                    item.level,
                    is_crafted,
                    item.crafter,
                    item.durability,
                    item.craft_points,
                    item.uses_remaining,
                    item.transmute_count,
                    item.attuned_to,
                    tsys_powers_json,
                    item.tsys_imbue_power,
                    item.tsys_imbue_power_tier,
                    item.pet_husbandry_state,
                    item.is_augmented,
                    item.dye_count,
                    item.inscription,
                    item.inscriber,
                ])
                .map_err(|e| format!("Failed to insert item {}: {e}", item.name))?;
        }

        // 8. Seed game_state_storage from this snapshot
        conn.execute(
            "DELETE FROM game_state_storage WHERE character_name = ?1 AND server_name = ?2",
            rusqlite::params![report.character, report.server_name],
        )
        .map_err(|e| format!("Failed to clear storage: {e}"))?;

        let mut storage_query = conn
            .prepare(
                "SELECT storage_vault, type_id, item_name, stack_size
             FROM character_snapshot_items
             WHERE item_snapshot_id = ?1 AND storage_vault != '' AND is_in_inventory = 0",
            )
            .map_err(|e| format!("Failed to prepare storage query: {e}"))?;

        let mut storage_insert = conn.prepare(
            "INSERT INTO game_state_storage (character_name, server_name, vault_key, instance_id, item_name, item_type_id, stack_size, last_confirmed_at, source)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 'snapshot')"
        ).map_err(|e| format!("Failed to prepare storage insert: {e}"))?;

        let storage_rows: Vec<(String, i64, String, i64)> = storage_query
            .query_map(rusqlite::params![snapshot_id], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, i64>(3)?,
                ))
            })
            .map_err(|e| format!("Failed to query storage items: {e}"))?
            .filter_map(|r| r.ok())
            .collect();

        let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
        for (i, (vault_key, type_id, item_name, stack_size)) in storage_rows.iter().enumerate() {
            let synthetic_id = -(i as i64 + 1);
            storage_insert
                .execute(rusqlite::params![
                    report.character,
                    report.server_name,
                    vault_key,
                    synthetic_id,
                    item_name,
                    type_id,
                    stack_size,
                    now,
                ])
                .map_err(|e| format!("Failed to insert storage item: {e}"))?;
        }

        // 9. Seed game_state_inventory from this snapshot (inventory items only)
        conn.execute(
            "DELETE FROM game_state_inventory WHERE character_name = ?1 AND server_name = ?2 AND source = 'snapshot'",
            rusqlite::params![report.character, report.server_name],
        ).map_err(|e| format!("Failed to clear inventory: {e}"))?;

        let mut inv_query = conn
            .prepare(
                "SELECT type_id, item_name, stack_size
             FROM character_snapshot_items
             WHERE item_snapshot_id = ?1 AND is_in_inventory = 1",
            )
            .map_err(|e| format!("Failed to prepare inventory query: {e}"))?;

        let mut inv_insert = conn.prepare(
            "INSERT INTO game_state_inventory (character_name, server_name, instance_id, item_name, item_type_id, stack_size, last_confirmed_at, source)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'snapshot')
             ON CONFLICT(character_name, server_name, instance_id) DO UPDATE SET
                item_name = excluded.item_name,
                item_type_id = excluded.item_type_id,
                stack_size = excluded.stack_size,
                last_confirmed_at = excluded.last_confirmed_at,
                source = 'snapshot'"
        ).map_err(|e| format!("Failed to prepare inventory insert: {e}"))?;

        let inv_rows: Vec<(i64, String, i64)> = inv_query
            .query_map(rusqlite::params![snapshot_id], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?,
                ))
            })
            .map_err(|e| format!("Failed to query inventory items: {e}"))?
            .filter_map(|r| r.ok())
            .collect();

        // Use negative synthetic IDs offset from storage to avoid collisions
        let inv_offset = storage_rows.len() as i64;
        for (i, (type_id, item_name, stack_size)) in inv_rows.iter().enumerate() {
            let synthetic_id = -(inv_offset + i as i64 + 1);
            inv_insert
                .execute(rusqlite::params![
                    report.character,
                    report.server_name,
                    synthetic_id,
                    item_name,
                    type_id,
                    stack_size,
                    now,
                ])
                .map_err(|e| format!("Failed to insert inventory item: {e}"))?;
        }

        Ok(InventoryImportResult {
            character_name: report.character.clone(),
            server_name: report.server_name.clone(),
            snapshot_timestamp: report.timestamp.clone(),
            items_imported,
            was_duplicate: false,
        })
    })();

    // 8. Commit (a dropped `Transaction` rolls back on any earlier `?`)
    let value = result?;
    conn.commit()
        .map_err(|e| format!("Failed to commit transaction: {e}"))?;

    Ok(value)
}

#[tauri::command]
pub fn get_inventory_snapshots(
    db: State<'_, DbPool>,
    character_name: String,
    server_name: Option<String>,
) -> Result<Vec<InventorySnapshotSummary>, String> {
    let conn = db
        .get()
        .map_err(|e| format!("Database connection error: {e}"))?;

    let (sql, params): (&str, Vec<Box<dyn rusqlite::types::ToSql>>) = if let Some(ref server) =
        server_name
    {
        (
            "SELECT cis.id, cis.character_name, cis.server_name, cis.snapshot_timestamp,
                    datetime(cis.import_date) as import_date,
                    (SELECT COUNT(*) FROM character_snapshot_items WHERE item_snapshot_id = cis.id) as item_count
             FROM character_item_snapshots cis
             WHERE cis.character_name = ?1 AND cis.server_name = ?2
             ORDER BY cis.snapshot_timestamp DESC",
            vec![
                Box::new(character_name.clone()) as Box<dyn rusqlite::types::ToSql>,
                Box::new(server.clone()),
            ],
        )
    } else {
        (
            "SELECT cis.id, cis.character_name, cis.server_name, cis.snapshot_timestamp,
                    datetime(cis.import_date) as import_date,
                    (SELECT COUNT(*) FROM character_snapshot_items WHERE item_snapshot_id = cis.id) as item_count
             FROM character_item_snapshots cis
             WHERE cis.character_name = ?1
             ORDER BY cis.snapshot_timestamp DESC",
            vec![Box::new(character_name.clone()) as Box<dyn rusqlite::types::ToSql>],
        )
    };

    let mut stmt = conn
        .prepare(sql)
        .map_err(|e| format!("Failed to prepare query: {e}"))?;

    let rows = stmt
        .query_map(rusqlite::params_from_iter(params.iter()), |row| {
            Ok(InventorySnapshotSummary {
                id: row.get(0)?,
                character_name: row.get(1)?,
                server_name: row.get(2)?,
                snapshot_timestamp: row.get(3)?,
                import_date: row.get(4)?,
                item_count: row.get(5)?,
            })
        })
        .map_err(|e| format!("Query failed: {e}"))?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("Failed to read results: {e}"))
}

#[tauri::command]
pub fn get_snapshot_items(
    db: State<'_, DbPool>,
    snapshot_id: i64,
) -> Result<Vec<SnapshotItem>, String> {
    let conn = db
        .get()
        .map_err(|e| format!("Database connection error: {e}"))?;

    let mut stmt = conn
        .prepare(
            "SELECT id, type_id, storage_vault, is_in_inventory, stack_size, value,
                item_name, rarity, slot, level, is_crafted, crafter, durability,
                craft_points, uses_remaining, transmute_count, attuned_to,
                tsys_powers, tsys_imbue_power, tsys_imbue_power_tier, pet_husbandry_state,
                is_augmented, dye_count, inscription, inscriber
         FROM character_snapshot_items
         WHERE item_snapshot_id = ?1
         ORDER BY item_name",
        )
        .map_err(|e| format!("Failed to prepare query: {e}"))?;

    let rows = stmt
        .query_map([snapshot_id], |row| {
            Ok(SnapshotItem {
                id: row.get(0)?,
                type_id: row.get(1)?,
                storage_vault: row.get(2)?,
                is_in_inventory: row.get(3)?,
                stack_size: row.get(4)?,
                value: row.get(5)?,
                item_name: row.get(6)?,
                rarity: row.get(7)?,
                slot: row.get(8)?,
                level: row.get(9)?,
                is_crafted: row.get(10)?,
                crafter: row.get(11)?,
                durability: row.get(12)?,
                craft_points: row.get(13)?,
                uses_remaining: row.get(14)?,
                transmute_count: row.get(15)?,
                attuned_to: row.get(16)?,
                tsys_powers: row.get(17)?,
                tsys_imbue_power: row.get(18)?,
                tsys_imbue_power_tier: row.get(19)?,
                pet_husbandry_state: row.get(20)?,
                is_augmented: row.get(21)?,
                dye_count: row.get(22)?,
                inscription: row.get(23)?,
                inscriber: row.get(24)?,
            })
        })
        .map_err(|e| format!("Query failed: {e}"))?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("Failed to read results: {e}"))
}

#[tauri::command]
pub fn get_inventory_summary(
    db: State<'_, DbPool>,
    snapshot_id: i64,
) -> Result<InventorySummary, String> {
    let conn = db
        .get()
        .map_err(|e| format!("Database connection error: {e}"))?;

    // Get aggregate stats
    let (total_stacks, total_items, total_value, unique_items): (i64, i64, f64, i64) = conn
        .query_row(
            "SELECT COUNT(*), COALESCE(SUM(stack_size), 0),
                COALESCE(SUM(COALESCE(value, 0) * stack_size), 0),
                COUNT(DISTINCT type_id)
         FROM character_snapshot_items
         WHERE item_snapshot_id = ?1",
            [snapshot_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .map_err(|e| format!("Failed to query summary: {e}"))?;

    // Items by vault
    let mut vault_stmt = conn
        .prepare(
            "SELECT CASE WHEN is_in_inventory THEN 'Inventory'
                     WHEN storage_vault = '' THEN 'Unknown'
                     ELSE storage_vault END as vault,
                SUM(stack_size)
         FROM character_snapshot_items
         WHERE item_snapshot_id = ?1
         GROUP BY vault
         ORDER BY vault",
        )
        .map_err(|e| format!("Failed to prepare vault query: {e}"))?;

    let mut items_by_vault = HashMap::new();
    let vault_rows = vault_stmt
        .query_map([snapshot_id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
        })
        .map_err(|e| format!("Vault query failed: {e}"))?;

    for row in vault_rows {
        let (vault, count) = row.map_err(|e| format!("Failed to read vault row: {e}"))?;
        items_by_vault.insert(vault, count);
    }

    // Items by rarity
    let mut rarity_stmt = conn
        .prepare(
            "SELECT COALESCE(rarity, 'Common'), SUM(stack_size)
         FROM character_snapshot_items
         WHERE item_snapshot_id = ?1
         GROUP BY COALESCE(rarity, 'Common')
         ORDER BY COALESCE(rarity, 'Common')",
        )
        .map_err(|e| format!("Failed to prepare rarity query: {e}"))?;

    let mut items_by_rarity = HashMap::new();
    let rarity_rows = rarity_stmt
        .query_map([snapshot_id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
        })
        .map_err(|e| format!("Rarity query failed: {e}"))?;

    for row in rarity_rows {
        let (rarity, count) = row.map_err(|e| format!("Failed to read rarity row: {e}"))?;
        items_by_rarity.insert(rarity, count);
    }

    Ok(InventorySummary {
        total_items,
        total_stacks,
        total_value,
        unique_items,
        items_by_vault,
        items_by_rarity,
    })
}

/// Return every item from the *latest* inventory snapshot of every character,
/// optionally scoped to a single server. Powers the "search all characters"
/// storage lookup — each row carries the owning character so the frontend can
/// group matches by item and show who has it and where.
#[tauri::command]
pub fn get_all_character_items(
    db: State<'_, DbPool>,
    server_name: Option<String>,
) -> Result<Vec<CrossCharacterItem>, String> {
    let conn = db
        .get()
        .map_err(|e| format!("Database connection error: {e}"))?;

    // `latest` picks, per (character, server), the snapshot with the newest
    // timestamp. The unique (character, server, timestamp) constraint guarantees
    // that MAX(timestamp) maps to exactly one snapshot row, so no duplicates.
    let mut stmt = conn
        .prepare(
            "WITH latest AS (
                SELECT cis.id, cis.character_name, cis.server_name
                FROM character_item_snapshots cis
                WHERE (?1 IS NULL OR cis.server_name = ?1)
                  AND cis.snapshot_timestamp = (
                      SELECT MAX(c2.snapshot_timestamp)
                      FROM character_item_snapshots c2
                      WHERE c2.character_name = cis.character_name
                        AND c2.server_name = cis.server_name
                  )
            )
            SELECT l.character_name, l.server_name, i.item_name, i.storage_vault,
                   i.is_in_inventory, i.stack_size, i.type_id, i.rarity, i.value
            FROM latest l
            JOIN character_snapshot_items i ON i.item_snapshot_id = l.id
            ORDER BY i.item_name, l.character_name",
        )
        .map_err(|e| format!("Failed to prepare query: {e}"))?;

    let rows = stmt
        .query_map(rusqlite::params![server_name], |row| {
            Ok(CrossCharacterItem {
                character_name: row.get(0)?,
                server_name: row.get(1)?,
                item_name: row.get(2)?,
                storage_vault: row.get(3)?,
                is_in_inventory: row.get(4)?,
                stack_size: row.get(5)?,
                type_id: row.get(6)?,
                rarity: row.get(7)?,
                value: row.get(8)?,
            })
        })
        .map_err(|e| format!("Query failed: {e}"))?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("Failed to read results: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::migrations::run_migrations;
    use std::sync::atomic::{AtomicU64, Ordering};

    static SEQ: AtomicU64 = AtomicU64::new(0);

    /// Migrated in-memory DB wrapped in the app's pool type. Shared-cache
    /// memory URI so `reads` and `writes` hit the same DB. Unique name per
    /// call keeps tests isolated.
    fn pool() -> DbPool {
        let n = SEQ.fetch_add(1, Ordering::Relaxed);
        let mk = |query_only: bool| {
            r2d2_sqlite::SqliteConnectionManager::file(format!(
                "file:inventory_details_test_{n}?mode=memory&cache=shared"
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
        run_migrations(&conn, None).expect("migrations");
        drop(conn);
        DbPool::from_pools(reads, writes)
    }

    const FIXTURE: &str =
        "../test_data/inventory/completeState/TestPlayer_Dreva_items_2026-04-14-15-57-59Z.json";

    #[test]
    fn import_persists_dropped_storage_fields_from_fixture() {
        let db = pool();
        let result = import_inventory_report_internal(&db, FIXTURE).expect("import fixture");
        assert!(!result.was_duplicate);

        let conn = db.get_write().expect("write conn");
        // Fixture counts: IsAugmented 8, DyeCount 4, Inscription 1, Inscriber 1.
        let (aug, dye, insc, inscr): (i64, i64, i64, i64) = conn
            .query_row(
                "SELECT COUNT(*) FILTER (WHERE is_augmented IS NOT NULL),
                        COUNT(*) FILTER (WHERE dye_count IS NOT NULL),
                        COUNT(*) FILTER (WHERE inscription IS NOT NULL),
                        COUNT(*) FILTER (WHERE inscriber IS NOT NULL)
                 FROM character_snapshot_items",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .expect("field counts");
        assert_eq!((aug, dye, insc, inscr), (8, 4, 1, 1), "dropped fields populated");

        // Spot-check values on a known augmented row.
        let (name, aug_flag): (String, Option<bool>) = conn
            .query_row(
                "SELECT item_name, is_augmented FROM character_snapshot_items WHERE type_id = 21803",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .expect("augmented row");
        assert_eq!(aug_flag, Some(true), "IsAugmented round-trips on {name}");

        let (inscription, inscriber): (Option<String>, Option<String>) = conn
            .query_row(
                "SELECT inscription, inscriber FROM character_snapshot_items WHERE type_id = 56109",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .expect("inscribed row");
        assert_eq!(inscription.as_deref(), Some("Limited 3/11"));
        assert_eq!(inscriber.as_deref(), Some("Deradon"));

        drop(conn);

        // DyeCount spot-check via a second read connection (drops the write guard).
        let conn = db.get().expect("read conn");
        let dye: Option<i64> = conn
            .query_row(
                "SELECT dye_count FROM character_snapshot_items WHERE type_id = 41344",
                [],
                |row| row.get(0),
            )
            .expect("dye row");
        assert_eq!(dye, Some(1));
    }

    #[test]
    fn manual_use_style_json_round_trips_new_fields() {
        let db = pool();
        let dir = std::env::temp_dir().join(format!(
            "inv_details_{}_{}",
            std::process::id(),
            SEQ.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).expect("tmp dir");
        let path = dir.join("TestChar_Dreva_items_2026-01-01-00-00-00Z.json");
        std::fs::write(
            &path,
            r#"{"Character":"TestChar","ServerName":"Dreva","Timestamp":"2026-01-01 00:00:00","Report":"Storage","ReportVersion":1,"Items":[
                {"TypeID":1,"StorageVault":"Vault","StackSize":1,"Value":10,"Name":"Plain Sword","IsAugmented":true,"DyeCount":2,"Inscription":"Hi","Inscriber":"Bob"},
                {"TypeID":2,"StorageVault":"","StackSize":5,"Value":1,"Name":"Plain Dagger"}
            ]}"#,
        )
        .expect("write report");
        let result =
            import_inventory_report_internal(&db, path.to_str().expect("path")).expect("import");
        assert_eq!(result.items_imported, 2);

        let conn = db.get().expect("read conn");
        let (aug, dye, insc, inscr): (Option<bool>, Option<i64>, Option<String>, Option<String>) =
            conn.query_row(
                "SELECT is_augmented, dye_count, inscription, inscriber FROM character_snapshot_items WHERE type_id = 1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .expect("new fields");
        assert_eq!((aug, dye), (Some(true), Some(2)));
        assert_eq!(insc.as_deref(), Some("Hi"));
        assert_eq!(inscr.as_deref(), Some("Bob"));
        // Omitted fields stay NULL.
        let (aug2, dye2): (Option<bool>, Option<i64>) = conn
            .query_row(
                "SELECT is_augmented, dye_count FROM character_snapshot_items WHERE type_id = 2",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .expect("null fields");
        assert_eq!((aug2, dye2), (None, None));
    }
    #[test]
    fn duplicate_import_backfills_v81_detail_columns() {
        // Major-1 regression: v81 adds nullable columns to rows that already
        // exist (imported before the migration). Re-importing the same file
        // hits ON CONFLICT DO NOTHING; the duplicate path must still refresh
        // the four columns (fill-only) so the backfill is reachable.
        let db = pool();
        let result = import_inventory_report_internal(&db, FIXTURE).expect("import");
        assert!(!result.was_duplicate);
        // Simulate a pre-v81 row: NULL out the columns on one known item.
        let conn = db.get_write().expect("write conn");
        conn.execute(
            "UPDATE character_snapshot_items SET is_augmented = NULL, dye_count = NULL, inscription = NULL, inscriber = NULL WHERE type_id = 21803",
            [],
        )
        .expect("null out row");
        drop(conn);
        // Re-import the same file: duplicate path runs the backfill.
        let result = import_inventory_report_internal(&db, FIXTURE).expect("re-import");
        assert!(result.was_duplicate);
        let conn = db.get().expect("read conn");
        let aug: Option<bool> = conn
            .query_row(
                "SELECT is_augmented FROM character_snapshot_items WHERE type_id = 21803",
                [],
                |row| row.get(0),
            )
            .expect("aug row");
        assert_eq!(aug, Some(true), "duplicate re-import must restore is_augmented");
    }

    #[test]
    fn duplicate_import_backfill_never_clobbers_newer_values() {
        // Fill-only semantics: re-importing an OLDER export after a newer one
        // populated the columns must not overwrite with NULLs or stale values.
        // The backfill UPDATE uses COALESCE per column, so NULLs in the file
        // (serde default for omitted keys) leave existing values alone.
        let db = pool();
        let result = import_inventory_report_internal(&db, FIXTURE).expect("import");
        assert!(!result.was_duplicate);
        // Stale file: same timestamp would be needed for the duplicate path,
        // so write a variant report with the same timestamp but the detail
        // keys omitted (simulates an older export that never carried them).
        let dir = std::env::temp_dir().join(format!(
            "inv_stale_{}_{}",
            std::process::id(),
            SEQ.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).expect("tmp dir");
        let raw: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(FIXTURE).expect("read fixture"),
        )
        .expect("parse fixture");
        let mut stripped = raw.clone();
        if let Some(items) = stripped.get_mut("Items").and_then(|v| v.as_array_mut()) {
            for item in items.iter_mut() {
                if let Some(obj) = item.as_object_mut() {
                    obj.remove("IsAugmented");
                    obj.remove("DyeCount");
                    obj.remove("Inscription");
                    obj.remove("Inscriber");
                }
            }
        }
        // Same character/server/timestamp → duplicate path.
        let path = dir.join("TestPlayer_Dreva_items_2026-04-14-15-57-59Z.json");
        std::fs::write(&path, serde_json::to_string(&stripped).expect("serialize"))
            .expect("write stripped");
        let result =
            import_inventory_report_internal(&db, path.to_str().expect("path")).expect("re-import");
        assert!(result.was_duplicate);
        // The known augmented row must survive the stripped re-import.
        let conn = db.get().expect("read conn");
        let aug: Option<bool> = conn
            .query_row(
                "SELECT is_augmented FROM character_snapshot_items WHERE type_id = 21803",
                [],
                |row| row.get(0),
            )
            .expect("aug row");
        assert_eq!(aug, Some(true), "stale re-import must not clobber is_augmented");
    }
}
