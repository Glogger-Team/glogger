use super::{DbPool, DbRead};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use serde::{Deserialize, Serialize};
/// Crafting helper project persistence commands
use std::collections::HashMap;
use tauri::State;

// ── Input types ─────────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct CreateProjectInput {
    pub name: String,
    pub notes: Option<String>,
    pub group_name: Option<String>,
    pub fee_config: Option<String>,
}

#[derive(Deserialize)]
pub struct UpdateProjectInput {
    pub id: i64,
    pub name: String,
    pub notes: String,
    pub group_name: Option<String>,
    pub fee_config: Option<String>,
    pub customer_provides: Option<String>,
}

#[derive(Deserialize)]
pub struct AddProjectEntryInput {
    pub project_id: i64,
    pub recipe_id: i64,
    pub recipe_name: String,
    pub quantity: i32,
    pub target_stock: Option<i32>,
    /// Concrete item IDs pinned to the recipe's variable (keyword) slots, in slot
    /// order. `None`/empty resolves slots generically. Used by the Brewery tab to
    /// turn a specific discovery combo into a project entry with exact materials.
    pub slot_item_ids: Option<Vec<i64>>,
}

#[derive(Deserialize)]
pub struct UpdateProjectEntryInput {
    pub id: i64,
    pub quantity: i32,
    pub expanded_ingredient_ids: Vec<i64>,
    pub target_stock: Option<i32>,
}

#[derive(Deserialize)]
pub struct BatchUpdateExpansionsEntry {
    pub id: i64,
    pub expanded_ingredient_ids: Vec<i64>,
}

#[derive(Deserialize)]
pub struct ReorderEntriesInput {
    pub project_id: i64,
    pub entry_ids: Vec<i64>,
}

// ── Output types ────────────────────────────────────────────────────────────

#[derive(Serialize)]
pub struct CraftingProject {
    pub id: i64,
    pub name: String,
    pub notes: String,
    pub group_name: Option<String>,
    pub fee_config: String,
    pub customer_provides: String,
    pub created_at: String,
    pub updated_at: String,
    pub entries: Vec<CraftingProjectEntry>,
}

#[derive(Serialize)]
pub struct CraftingProjectEntry {
    pub id: i64,
    pub project_id: i64,
    pub recipe_id: i64,
    pub recipe_name: String,
    pub quantity: i32,
    pub sort_order: i32,
    pub expanded_ingredient_ids: Vec<i64>,
    pub target_stock: Option<i32>,
    /// Item IDs pinned to the recipe's variable slots, in slot order (see
    /// `AddProjectEntryInput::slot_item_ids`). Empty when slots resolve generically.
    pub slot_item_ids: Vec<i64>,
}

#[derive(Serialize)]
pub struct CraftingProjectSummary {
    pub id: i64,
    pub name: String,
    pub notes: String,
    pub group_name: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub entry_count: i64,
}

// ── Commands ────────────────────────────────────────────────────────────────

#[tauri::command]
pub fn create_crafting_project(
    db: State<'_, DbPool>,
    input: CreateProjectInput,
) -> Result<i64, String> {
    let conn = db
        .get_write()
        .map_err(|e| format!("Database connection error: {e}"))?;

    let default_fee = r#"{"per_craft_fee":0,"material_pct":0,"material_pct_basis":"total","flat_fee":0}"#;
    let fee_config = input.fee_config.as_deref().unwrap_or(default_fee);

    conn.execute(
        "INSERT INTO crafting_projects (name, notes, group_name, fee_config) VALUES (?1, ?2, ?3, ?4)",
        rusqlite::params![input.name, input.notes.unwrap_or_default(), input.group_name, fee_config],
    )
    .map_err(|e| format!("Failed to create crafting project: {e}"))?;

    Ok(conn.last_insert_rowid())
}

#[tauri::command]
pub fn get_crafting_projects(db: State<'_, DbPool>) -> Result<Vec<CraftingProjectSummary>, String> {
    let conn = db
        .get()
        .map_err(|e| format!("Database connection error: {e}"))?;

    let mut stmt = conn
        .prepare(
            "SELECT p.id, p.name, p.notes, p.group_name, datetime(p.created_at), datetime(p.updated_at),
                (SELECT COUNT(*) FROM crafting_project_entries WHERE project_id = p.id)
         FROM crafting_projects p
         ORDER BY p.updated_at DESC",
        )
        .map_err(|e| format!("Failed to prepare query: {e}"))?;

    let rows = stmt
        .query_map([], |row| {
            Ok(CraftingProjectSummary {
                id: row.get(0)?,
                name: row.get(1)?,
                notes: row.get(2)?,
                group_name: row.get(3)?,
                created_at: row.get(4)?,
                updated_at: row.get(5)?,
                entry_count: row.get(6)?,
            })
        })
        .map_err(|e| format!("Query failed: {e}"))?;

    let mut projects = Vec::new();
    for row in rows {
        projects.push(row.map_err(|e| format!("Row parse error: {e}"))?);
    }

    Ok(projects)
}

#[tauri::command]
pub fn get_crafting_project(
    db: State<'_, DbPool>,
    project_id: i64,
) -> Result<CraftingProject, String> {
    let conn = db
        .get()
        .map_err(|e| format!("Database connection error: {e}"))?;

    let project = conn
        .query_row(
            "SELECT id, name, notes, group_name, fee_config, customer_provides, datetime(created_at), datetime(updated_at)
         FROM crafting_projects WHERE id = ?1",
            [project_id],
            |row| {
                Ok(CraftingProject {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    notes: row.get(2)?,
                    group_name: row.get(3)?,
                    fee_config: row.get(4)?,
                    customer_provides: row.get(5)?,
                    created_at: row.get(6)?,
                    updated_at: row.get(7)?,
                    entries: Vec::new(),
                })
            },
        )
        .map_err(|e| format!("Project not found: {e}"))?;

    let mut entry_stmt = conn.prepare(
        "SELECT id, project_id, recipe_id, recipe_name, quantity, sort_order, expanded_ingredient_ids, target_stock, slot_item_ids
         FROM crafting_project_entries
         WHERE project_id = ?1
         ORDER BY sort_order ASC"
    ).map_err(|e| format!("Failed to prepare entry query: {e}"))?;

    let entry_rows = entry_stmt
        .query_map([project_id], |row| {
            let ids_json: String = row.get(6)?;
            let expanded_ids: Vec<i64> = serde_json::from_str(&ids_json).unwrap_or_default();
            let slot_json: String = row.get(8)?;
            let slot_item_ids: Vec<i64> = serde_json::from_str(&slot_json).unwrap_or_default();
            Ok(CraftingProjectEntry {
                id: row.get(0)?,
                project_id: row.get(1)?,
                recipe_id: row.get(2)?,
                recipe_name: row.get(3)?,
                quantity: row.get(4)?,
                sort_order: row.get(5)?,
                expanded_ingredient_ids: expanded_ids,
                target_stock: row.get(7)?,
                slot_item_ids,
            })
        })
        .map_err(|e| format!("Entry query failed: {e}"))?;

    let mut project = project;
    for row in entry_rows {
        project
            .entries
            .push(row.map_err(|e| format!("Entry row error: {e}"))?);
    }

    Ok(project)
}

#[tauri::command]
pub fn update_crafting_project(
    db: State<'_, DbPool>,
    input: UpdateProjectInput,
) -> Result<(), String> {
    let conn = db
        .get_write()
        .map_err(|e| format!("Database connection error: {e}"))?;

    conn.execute(
        "UPDATE crafting_projects SET name = ?1, notes = ?2, group_name = ?3, fee_config = COALESCE(?4, fee_config), customer_provides = COALESCE(?5, customer_provides), updated_at = CURRENT_TIMESTAMP
         WHERE id = ?6",
        rusqlite::params![input.name, input.notes, input.group_name, input.fee_config, input.customer_provides, input.id],
    )
    .map_err(|e| format!("Failed to update project: {e}"))?;

    Ok(())
}

#[tauri::command]
pub fn delete_crafting_project(db: State<'_, DbPool>, project_id: i64) -> Result<(), String> {
    let conn = db
        .get_write()
        .map_err(|e| format!("Database connection error: {e}"))?;

    conn.execute("DELETE FROM crafting_projects WHERE id = ?1", [project_id])
        .map_err(|e| format!("Failed to delete project: {e}"))?;

    Ok(())
}

#[tauri::command]
pub fn add_project_entry(
    db: State<'_, DbPool>,
    input: AddProjectEntryInput,
) -> Result<i64, String> {
    let mut conn = db
        .get_write()
        .map_err(|e| format!("Database connection error: {e}"))?;
    let conn = conn
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .map_err(|e| format!("Failed to begin transaction: {e}"))?;

    // Get next sort_order
    let next_order: i32 = conn.query_row(
        "SELECT COALESCE(MAX(sort_order), -1) + 1 FROM crafting_project_entries WHERE project_id = ?1",
        [input.project_id],
        |row| row.get(0),
    ).map_err(|e| format!("Failed to get sort order: {e}"))?;

    let slot_ids_json = serde_json::to_string(&input.slot_item_ids.unwrap_or_default())
        .map_err(|e| format!("Failed to serialize slot_item_ids: {e}"))?;

    conn.execute(
        "INSERT INTO crafting_project_entries (project_id, recipe_id, recipe_name, quantity, sort_order, target_stock, slot_item_ids)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        rusqlite::params![input.project_id, input.recipe_id, input.recipe_name, input.quantity, next_order, input.target_stock, slot_ids_json],
    ).map_err(|e| format!("Failed to add entry: {e}"))?;

    // Touch the project's updated_at
    conn.execute(
        "UPDATE crafting_projects SET updated_at = CURRENT_TIMESTAMP WHERE id = ?1",
        [input.project_id],
    )
    .map_err(|e| format!("Failed to touch project: {e}"))?;

    let new_id = conn.last_insert_rowid();
    conn.commit()
        .map_err(|e| format!("Failed to commit transaction: {e}"))?;
    Ok(new_id)
}

#[tauri::command]
pub fn update_project_entry(
    db: State<'_, DbPool>,
    input: UpdateProjectEntryInput,
) -> Result<(), String> {
    let mut conn = db
        .get_write()
        .map_err(|e| format!("Database connection error: {e}"))?;
    let conn = conn
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .map_err(|e| format!("Failed to begin transaction: {e}"))?;

    let ids_json = serde_json::to_string(&input.expanded_ingredient_ids)
        .map_err(|e| format!("Failed to serialize expanded_ingredient_ids: {e}"))?;
    conn.execute(
        "UPDATE crafting_project_entries SET quantity = ?1, expanded_ingredient_ids = ?2, target_stock = ?3
         WHERE id = ?4",
        rusqlite::params![input.quantity, ids_json, input.target_stock, input.id],
    )
    .map_err(|e| format!("Failed to update entry: {e}"))?;

    // Touch the project's updated_at
    conn.execute(
        "UPDATE crafting_projects SET updated_at = CURRENT_TIMESTAMP
         WHERE id = (SELECT project_id FROM crafting_project_entries WHERE id = ?1)",
        [input.id],
    )
    .map_err(|e| format!("Failed to touch project: {e}"))?;

    conn.commit()
        .map_err(|e| format!("Failed to commit transaction: {e}"))?;
    Ok(())
}

/// Batch-update only the expanded_ingredient_ids for multiple entries in one DB transaction.
/// Does NOT reload the project — the frontend manages its own state after calling this.
#[tauri::command]
pub fn batch_update_entry_expansions(
    db: State<'_, DbPool>,
    entries: Vec<BatchUpdateExpansionsEntry>,
) -> Result<(), String> {
    let mut conn = db
        .get_write()
        .map_err(|e| format!("Database connection error: {e}"))?;
    let conn = conn
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .map_err(|e| format!("Failed to begin transaction: {e}"))?;

    for entry in &entries {
        let ids_json = serde_json::to_string(&entry.expanded_ingredient_ids)
            .map_err(|e| format!("Failed to serialize expanded_ingredient_ids: {e}"))?;
        conn.execute(
            "UPDATE crafting_project_entries SET expanded_ingredient_ids = ?1 WHERE id = ?2",
            rusqlite::params![ids_json, entry.id],
        )
        .map_err(|e| format!("Failed to update entry {}: {e}", entry.id))?;
    }

    // Touch the project's updated_at once (use the first entry to find the project)
    if let Some(first) = entries.first() {
        conn.execute(
            "UPDATE crafting_projects SET updated_at = CURRENT_TIMESTAMP
             WHERE id = (SELECT project_id FROM crafting_project_entries WHERE id = ?1)",
            [first.id],
        )
        .map_err(|e| format!("Failed to touch project: {e}"))?;
    }

    conn.commit()
        .map_err(|e| format!("Failed to commit transaction: {e}"))?;
    Ok(())
}

#[tauri::command]
pub fn remove_project_entry(db: State<'_, DbPool>, entry_id: i64) -> Result<(), String> {
    let mut conn = db
        .get_write()
        .map_err(|e| format!("Database connection error: {e}"))?;
    let conn = conn
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .map_err(|e| format!("Failed to begin transaction: {e}"))?;

    // Touch the project's updated_at before deleting
    conn.execute(
        "UPDATE crafting_projects SET updated_at = CURRENT_TIMESTAMP
         WHERE id = (SELECT project_id FROM crafting_project_entries WHERE id = ?1)",
        [entry_id],
    )
    .map_err(|e| format!("Failed to touch project: {e}"))?;

    conn.execute(
        "DELETE FROM crafting_project_entries WHERE id = ?1",
        [entry_id],
    )
    .map_err(|e| format!("Failed to remove entry: {e}"))?;

    conn.commit()
        .map_err(|e| format!("Failed to commit transaction: {e}"))?;
    Ok(())
}

#[tauri::command]
pub fn reorder_project_entries(
    db: State<'_, DbPool>,
    input: ReorderEntriesInput,
) -> Result<(), String> {
    let mut conn = db
        .get_write()
        .map_err(|e| format!("Database connection error: {e}"))?;
    let conn = conn
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .map_err(|e| format!("Failed to begin transaction: {e}"))?;

    for (index, entry_id) in input.entry_ids.iter().enumerate() {
        conn.execute(
            "UPDATE crafting_project_entries SET sort_order = ?1
             WHERE id = ?2 AND project_id = ?3",
            rusqlite::params![index as i32, entry_id, input.project_id],
        )
        .map_err(|e| format!("Failed to reorder entry: {e}"))?;
    }

    conn.execute(
        "UPDATE crafting_projects SET updated_at = CURRENT_TIMESTAMP WHERE id = ?1",
        [input.project_id],
    )
    .map_err(|e| format!("Failed to touch project: {e}"))?;

    conn.commit()
        .map_err(|e| format!("Failed to commit transaction: {e}"))?;
    Ok(())
}

#[tauri::command]
pub fn duplicate_crafting_project(db: State<'_, DbPool>, project_id: i64) -> Result<i64, String> {
    let mut conn = db
        .get_write()
        .map_err(|e| format!("Database connection error: {e}"))?;
    let conn = conn
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .map_err(|e| format!("Failed to begin transaction: {e}"))?;

    // Get original project
    let (name, notes, group_name, fee_config, customer_provides): (String, String, Option<String>, String, String) = conn
        .query_row(
            "SELECT name, notes, group_name, fee_config, customer_provides FROM crafting_projects WHERE id = ?1",
            [project_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?)),
        )
        .map_err(|e| format!("Project not found: {e}"))?;

    // Create copy
    conn.execute(
        "INSERT INTO crafting_projects (name, notes, group_name, fee_config, customer_provides) VALUES (?1, ?2, ?3, ?4, ?5)",
        rusqlite::params![format!("{name} (copy)"), notes, group_name, fee_config, customer_provides],
    )
    .map_err(|e| format!("Failed to duplicate project: {e}"))?;

    let new_id = conn.last_insert_rowid();

    // Copy entries
    conn.execute(
        "INSERT INTO crafting_project_entries (project_id, recipe_id, recipe_name, quantity, sort_order, expanded_ingredient_ids, target_stock, slot_item_ids)
         SELECT ?1, recipe_id, recipe_name, quantity, sort_order, expanded_ingredient_ids, target_stock, slot_item_ids
         FROM crafting_project_entries
         WHERE project_id = ?2
         ORDER BY sort_order",
        rusqlite::params![new_id, project_id],
    ).map_err(|e| format!("Failed to copy entries: {e}"))?;

    conn.commit()
        .map_err(|e| format!("Failed to commit transaction: {e}"))?;
    Ok(new_id)
}

// ── Import/Export ───────────────────────────────────────────────────────────

/// Portable project format for sharing between players
#[derive(Serialize, Deserialize)]
struct ExportedProject {
    version: u32,
    name: String,
    notes: String,
    group_name: Option<String>,
    fee_config: Option<String>,
    customer_provides: Option<String>,
    entries: Vec<ExportedProjectEntry>,
}

#[derive(Serialize, Deserialize)]
struct ExportedProjectEntry {
    recipe_id: i64,
    recipe_name: String,
    quantity: i32,
    sort_order: i32,
    #[serde(default)]
    expanded_ingredient_ids: Vec<i64>,
    #[serde(default)]
    target_stock: Option<i32>,
    #[serde(default)]
    slot_item_ids: Vec<i64>,
}

#[tauri::command]
pub fn export_crafting_project(db: State<'_, DbPool>, project_id: i64) -> Result<String, String> {
    let conn = db
        .get()
        .map_err(|e| format!("Database connection error: {e}"))?;
    export_crafting_project_impl(&conn, project_id)
}

fn export_crafting_project_impl<C: DbRead + ?Sized>(
    conn: &C,
    project_id: i64,
) -> Result<String, String> {
    let (name, notes, group_name, fee_config, customer_provides): (String, String, Option<String>, String, String) = conn
        .query_row(
            "SELECT name, notes, group_name, fee_config, customer_provides FROM crafting_projects WHERE id = ?1",
            [project_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?)),
        )
        .map_err(|e| format!("Project not found: {e}"))?;

    let mut stmt = conn
        .prepare("SELECT recipe_id, recipe_name, quantity, sort_order, expanded_ingredient_ids, target_stock, slot_item_ids FROM crafting_project_entries WHERE project_id = ?1 ORDER BY sort_order")
        .map_err(|e| format!("Failed to query entries: {e}"))?;
    let entries: Vec<ExportedProjectEntry> = stmt
        .query_map([project_id], |row| {
            let expanded_json: String = row.get(4)?;
            let slot_json: String = row.get(6)?;
            Ok(ExportedProjectEntry {
                recipe_id: row.get(0)?,
                recipe_name: row.get(1)?,
                quantity: row.get(2)?,
                sort_order: row.get(3)?,
                expanded_ingredient_ids: serde_json::from_str(&expanded_json).unwrap_or_default(),
                target_stock: row.get(5)?,
                slot_item_ids: serde_json::from_str(&slot_json).unwrap_or_default(),
            })
        })
        .map_err(|e| format!("Entry query failed: {e}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("Entry parse error: {e}"))?;

    let exported = ExportedProject {
        version: 1,
        name,
        notes,
        group_name,
        fee_config: Some(fee_config),
        customer_provides: Some(customer_provides),
        entries,
    };

    let json = serde_json::to_string(&exported).map_err(|e| format!("Serialization error: {e}"))?;
    Ok(BASE64.encode(json.as_bytes()))
}

#[tauri::command]
pub fn import_crafting_project(db: State<'_, DbPool>, encoded: String) -> Result<i64, String> {
    let mut conn = db
        .get_write()
        .map_err(|e| format!("Database connection error: {e}"))?;
    let conn = conn
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .map_err(|e| format!("Failed to begin transaction: {e}"))?;
    let new_id = import_crafting_project_impl(&conn, &encoded)?;
    conn.commit()
        .map_err(|e| format!("Failed to commit transaction: {e}"))?;
    Ok(new_id)
}

fn import_crafting_project_impl(
    conn: &rusqlite::Connection,
    encoded: &str,
) -> Result<i64, String> {
    let json_bytes = BASE64
        .decode(encoded.trim())
        .map_err(|e| format!("Invalid project code: {e}"))?;
    let json_str = String::from_utf8(json_bytes).map_err(|e| format!("Invalid UTF-8: {e}"))?;
    let project: ExportedProject =
        serde_json::from_str(&json_str).map_err(|e| format!("Invalid project data: {e}"))?;

    if project.version != 1 {
        return Err(format!(
            "Unsupported project version {} (this app supports version 1)",
            project.version
        ));
    }

    let default_fee = r#"{"per_craft_fee":0,"material_pct":0,"material_pct_basis":"total","flat_fee":0}"#;
    conn.execute(
        "INSERT INTO crafting_projects (name, notes, group_name, fee_config, customer_provides) VALUES (?1, ?2, ?3, ?4, ?5)",
        rusqlite::params![
            project.name,
            project.notes,
            project.group_name,
            project.fee_config.as_deref().unwrap_or(default_fee),
            project.customer_provides.as_deref().unwrap_or("{}"),
        ],
    )
    .map_err(|e| format!("Failed to create project: {e}"))?;

    let new_id = conn.last_insert_rowid();

    {
        let mut stmt = conn
            .prepare("INSERT INTO crafting_project_entries (project_id, recipe_id, recipe_name, quantity, sort_order, expanded_ingredient_ids, target_stock, slot_item_ids) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)")
            .map_err(|e| format!("Failed to prepare entry insert: {e}"))?;
        for entry in &project.entries {
            let expanded = serde_json::to_string(&entry.expanded_ingredient_ids)
                .unwrap_or_else(|_| "[]".to_string());
            let slots = serde_json::to_string(&entry.slot_item_ids)
                .unwrap_or_else(|_| "[]".to_string());
            stmt.execute(rusqlite::params![
                new_id,
                entry.recipe_id,
                entry.recipe_name,
                entry.quantity,
                entry.sort_order,
                expanded,
                entry.target_stock,
                slots,
            ])
            .map_err(|e| format!("Failed to insert entry: {e}"))?;
        }
    }

    Ok(new_id)
}

// ── Material availability ───────────────────────────────────────────────────

#[derive(Serialize)]
pub struct VaultStock {
    pub vault_name: String,
    pub quantity: i64,
}

#[derive(Serialize)]
pub struct MaterialAvailability {
    pub item_type_id: i64,
    pub item_name: String,
    pub inventory_quantity: i64,
    pub storage_quantity: i64,
    pub vault_breakdown: Vec<VaultStock>,
    pub total_available: i64,
}

/// Check material availability across the persisted game state inventory (from log events),
/// the latest storage snapshot, and item name lookups. Takes a list of item type IDs to check.
#[tauri::command]
pub fn check_material_availability(
    db: State<'_, DbPool>,
    character_name: String,
    server_name: String,
    item_type_ids: Vec<i64>,
) -> Result<Vec<MaterialAvailability>, String> {
    if item_type_ids.is_empty() {
        return Ok(Vec::new());
    }

    let conn = db
        .get()
        .map_err(|e| format!("Database connection error: {e}"))?;

    let mut results: HashMap<i64, MaterialAvailability> = HashMap::new();

    // Initialize all requested IDs
    for &id in &item_type_ids {
        results.insert(
            id,
            MaterialAvailability {
                item_type_id: id,
                item_name: String::new(),
                inventory_quantity: 0,
                storage_quantity: 0,
                vault_breakdown: Vec::new(),
                total_available: 0,
            },
        );
    }

    // ── 1. Query persisted inventory from game_state_inventory (log-driven) ────
    {
        use std::rc::Rc;

        let sql =
            "SELECT item_type_id, item_name, SUM(stack_size) as qty
             FROM game_state_inventory
             WHERE character_name = ?1 AND item_type_id IN (SELECT value FROM rarray(?2))
             GROUP BY item_type_id";

        let mut stmt = conn
            .prepare(sql)
            .map_err(|e| format!("Failed to prepare game state inventory query: {e}"))?;

        let ids: Rc<Vec<rusqlite::types::Value>> = Rc::new(
            item_type_ids
                .iter()
                .map(|id| rusqlite::types::Value::Integer(*id))
                .collect(),
        );

        let rows = stmt
            .query_map(rusqlite::params![character_name.clone(), ids], |row| {
                Ok((
                    row.get::<_, i64>(0)?,    // item_type_id
                    row.get::<_, String>(1)?, // item_name
                    row.get::<_, i64>(2)?,    // qty
                ))
            })
            .map_err(|e| format!("Game state inventory query failed: {e}"))?;

        for row in rows {
            let (type_id, item_name, qty) = row.map_err(|e| format!("Row parse error: {e}"))?;

            if let Some(entry) = results.get_mut(&type_id) {
                entry.item_name = item_name;
                entry.inventory_quantity = qty;
            }
        }
    }

    // ── 2. Query storage vaults from latest snapshot ───────────────────────────
    let latest_snapshot_id: Option<i64> = conn
        .query_row(
            "SELECT id FROM character_item_snapshots
         WHERE character_name = ?1 AND server_name = ?2
         ORDER BY snapshot_timestamp DESC LIMIT 1",
            rusqlite::params![character_name, server_name],
            |row| row.get(0),
        )
        .ok();

    if let Some(snapshot_id) = latest_snapshot_id {
        use std::rc::Rc;

        let sql =
            "SELECT type_id, item_name, storage_vault, SUM(stack_size) as qty
             FROM character_snapshot_items
             WHERE item_snapshot_id = ?1 AND type_id IN (SELECT value FROM rarray(?2)) AND is_in_inventory = 0
             GROUP BY type_id, storage_vault";

        let mut stmt = conn
            .prepare(sql)
            .map_err(|e| format!("Failed to prepare availability query: {e}"))?;

        let ids: Rc<Vec<rusqlite::types::Value>> = Rc::new(
            item_type_ids
                .iter()
                .map(|id| rusqlite::types::Value::Integer(*id))
                .collect(),
        );

        let rows = stmt
            .query_map(rusqlite::params![snapshot_id, ids], |row| {
                Ok((
                    row.get::<_, i64>(0)?,    // type_id
                    row.get::<_, String>(1)?, // item_name
                    row.get::<_, String>(2)?, // storage_vault
                    row.get::<_, i64>(3)?,    // qty
                ))
            })
            .map_err(|e| format!("Availability query failed: {e}"))?;

        for row in rows {
            let (type_id, item_name, vault, qty) =
                row.map_err(|e| format!("Row parse error: {e}"))?;

            if let Some(entry) = results.get_mut(&type_id) {
                if entry.item_name.is_empty() {
                    entry.item_name = item_name;
                }
                entry.storage_quantity += qty;
                let vault_name = if vault.is_empty() {
                    "Unknown".to_string()
                } else {
                    vault
                };
                entry.vault_breakdown.push(VaultStock {
                    vault_name,
                    quantity: qty,
                });
            }
        }
    }

    // ── 3. Fill in item names and compute totals ───────────────────────────────
    for (&id, entry) in results.iter_mut() {
        if entry.item_name.is_empty() {
            let name: Option<String> = conn
                .query_row("SELECT name FROM items WHERE id = ?1", [id], |row| {
                    row.get(0)
                })
                .ok();
            entry.item_name = name.unwrap_or_else(|| format!("Item #{}", id));
        }
        entry.total_available = entry.inventory_quantity + entry.storage_quantity;
    }

    Ok(item_type_ids
        .iter()
        .filter_map(|id| results.remove(id))
        .collect())
}

// ── Work order data from snapshot ────────────────────────────────────────────

#[derive(Serialize)]
pub struct WorkOrderData {
    pub active: Vec<String>,
    pub completed: Vec<String>,
    /// TypeIDs of work order scroll items found in inventory/storage
    pub inventory_item_ids: Vec<u32>,
}

/// Extract ActiveWorkOrders, CompletedWorkOrders, and inventory work order scroll items.
#[tauri::command]
pub fn get_work_orders_from_snapshot(
    db: State<'_, DbPool>,
    character_name: String,
    server_name: String,
) -> Result<WorkOrderData, String> {
    let conn = db
        .get()
        .map_err(|e| format!("Database connection error: {e}"))?;

    // Get active/completed work orders from character snapshot
    let raw_json: Option<String> = conn
        .query_row(
            "SELECT raw_json FROM character_snapshots
         WHERE character_name = ?1 AND server_name = ?2
         ORDER BY snapshot_timestamp DESC LIMIT 1",
            rusqlite::params![character_name, server_name],
            |row| row.get(0),
        )
        .ok();

    let (active, completed) = if let Some(raw) = raw_json {
        let parsed: serde_json::Value = serde_json::from_str(&raw)
            .map_err(|e| format!("Failed to parse snapshot JSON: {e}"))?;

        let active = parsed
            .get("ActiveWorkOrders")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();

        let completed = parsed
            .get("CompletedWorkOrders")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();

        (active, completed)
    } else {
        (Vec::new(), Vec::new())
    };

    // Get work order scroll items from the latest inventory snapshot
    let inventory_item_ids: Vec<u32> = conn
        .query_row(
            "SELECT id FROM character_item_snapshots
         WHERE character_name = ?1 AND server_name = ?2
         ORDER BY snapshot_timestamp DESC LIMIT 1",
            rusqlite::params![character_name, server_name],
            |row| row.get::<_, i64>(0),
        )
        .ok()
        .map(|snapshot_id| {
            let mut stmt = conn
                .prepare(
                    "SELECT DISTINCT type_id FROM character_snapshot_items
             WHERE item_snapshot_id = ?1
               AND (item_name LIKE 'Work Order for %' OR item_name LIKE 'Scroll\\_%' ESCAPE '\\')",
                )
                .unwrap();
            stmt.query_map(rusqlite::params![snapshot_id], |row| row.get::<_, u32>(0))
                .unwrap()
                .filter_map(|r| r.ok())
                .collect()
        })
        .unwrap_or_default();

    Ok(WorkOrderData {
        active,
        completed,
        inventory_item_ids,
    })
}

#[cfg(test)]
mod import_export_tests {
    use super::*;
    use rusqlite::Connection;

    /// Current shape of the crafting project tables (post-migrations).
    fn setup() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE crafting_projects (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                name TEXT NOT NULL,
                notes TEXT NOT NULL DEFAULT '',
                group_name TEXT DEFAULT NULL,
                fee_config TEXT NOT NULL DEFAULT '{\"per_craft_fee\":0,\"material_pct\":0,\"material_pct_basis\":\"total\",\"flat_fee\":0}',
                customer_provides TEXT NOT NULL DEFAULT '{}',
                created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
                updated_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
            );
            CREATE TABLE crafting_project_entries (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                project_id INTEGER NOT NULL,
                recipe_id INTEGER NOT NULL,
                recipe_name TEXT NOT NULL,
                quantity INTEGER NOT NULL DEFAULT 1,
                sort_order INTEGER NOT NULL DEFAULT 0,
                expanded_ingredient_ids TEXT NOT NULL DEFAULT '[]',
                target_stock INTEGER DEFAULT NULL,
                slot_item_ids TEXT NOT NULL DEFAULT '[]',
                FOREIGN KEY (project_id) REFERENCES crafting_projects(id) ON DELETE CASCADE
            );",
        )
        .unwrap();
        conn
    }

    fn insert_sample_project(conn: &Connection) -> i64 {
        conn.execute(
            "INSERT INTO crafting_projects (name, notes, group_name, fee_config, customer_provides)
             VALUES ('Brew Batch', 'for the guild', 'Orders', '{\"per_craft_fee\":5,\"material_pct\":10,\"material_pct_basis\":\"total\",\"flat_fee\":0}', '{\"item:123\":4}')",
            [],
        )
        .unwrap();
        let id = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO crafting_project_entries (project_id, recipe_id, recipe_name, quantity, sort_order, expanded_ingredient_ids, target_stock, slot_item_ids)
             VALUES (?1, 7001, 'Orcish Bock', 3, 0, '[55,66]', 12, '[901,902]')",
            [id],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO crafting_project_entries (project_id, recipe_id, recipe_name, quantity, sort_order, expanded_ingredient_ids, target_stock, slot_item_ids)
             VALUES (?1, 7002, 'Oat Bread', 10, 1, '[]', NULL, '[]')",
            [id],
        )
        .unwrap();
        id
    }

    #[test]
    fn export_import_round_trips_all_fields() {
        let conn = setup();
        let original_id = insert_sample_project(&conn);

        let code = export_crafting_project_impl(&conn, original_id).unwrap();
        let new_id = import_crafting_project_impl(&conn, &code).unwrap();
        assert_ne!(new_id, original_id);

        // Re-exporting the imported project must produce an identical payload.
        let round_tripped = export_crafting_project_impl(&conn, new_id).unwrap();
        assert_eq!(code, round_tripped);

        // Spot-check the imported rows directly.
        let (name, notes, group_name, fee_config, customer_provides): (String, String, Option<String>, String, String) = conn
            .query_row(
                "SELECT name, notes, group_name, fee_config, customer_provides FROM crafting_projects WHERE id = ?1",
                [new_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?)),
            )
            .unwrap();
        assert_eq!(name, "Brew Batch");
        assert_eq!(notes, "for the guild");
        assert_eq!(group_name.as_deref(), Some("Orders"));
        assert!(fee_config.contains("\"per_craft_fee\":5"));
        assert_eq!(customer_provides, "{\"item:123\":4}");

        let entries: Vec<(i64, String, i32, i32, String, Option<i32>, String)> = conn
            .prepare("SELECT recipe_id, recipe_name, quantity, sort_order, expanded_ingredient_ids, target_stock, slot_item_ids FROM crafting_project_entries WHERE project_id = ?1 ORDER BY sort_order")
            .unwrap()
            .query_map([new_id], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?, row.get(5)?, row.get(6)?))
            })
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0], (7001, "Orcish Bock".to_string(), 3, 0, "[55,66]".to_string(), Some(12), "[901,902]".to_string()));
        assert_eq!(entries[1], (7002, "Oat Bread".to_string(), 10, 1, "[]".to_string(), None, "[]".to_string()));
    }

    #[test]
    fn import_rejects_garbage_and_future_versions() {
        let conn = setup();

        assert!(import_crafting_project_impl(&conn, "not base64!!!").is_err());
        assert!(import_crafting_project_impl(&conn, &BASE64.encode(b"not json")).is_err());

        let future = BASE64.encode(
            br#"{"version":99,"name":"x","notes":"","group_name":null,"fee_config":null,"customer_provides":null,"entries":[]}"#,
        );
        let err = import_crafting_project_impl(&conn, &future).unwrap_err();
        assert!(err.contains("Unsupported project version 99"), "got: {err}");
    }

    /// Older/minimal exports without optional fields must still import.
    #[test]
    fn import_defaults_missing_optional_fields() {
        let conn = setup();
        let minimal = BASE64.encode(
            br#"{"version":1,"name":"Bare","notes":"","group_name":null,"fee_config":null,"customer_provides":null,"entries":[{"recipe_id":1,"recipe_name":"Pine Board","quantity":2,"sort_order":0}]}"#,
        );
        let id = import_crafting_project_impl(&conn, &minimal).unwrap();

        let (fee_config, customer_provides): (String, String) = conn
            .query_row(
                "SELECT fee_config, customer_provides FROM crafting_projects WHERE id = ?1",
                [id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert!(fee_config.contains("per_craft_fee"));
        assert_eq!(customer_provides, "{}");

        let (expanded, target, slots): (String, Option<i32>, String) = conn
            .query_row(
                "SELECT expanded_ingredient_ids, target_stock, slot_item_ids FROM crafting_project_entries WHERE project_id = ?1",
                [id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(expanded, "[]");
        assert_eq!(target, None);
        assert_eq!(slots, "[]");
    }
}
