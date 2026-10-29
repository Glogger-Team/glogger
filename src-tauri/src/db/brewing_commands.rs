use super::{DbPool, DbRead};
use crate::cdn_commands::GameDataState;
use rusqlite::params;
use serde::Serialize;
use serde_json::Value;
use tauri::State;

// ── Response types ──────────────────────────────────────────────────────────

#[derive(Debug, Serialize, Clone)]
pub struct BrewingDiscovery {
    pub id: i64,
    pub character: String,
    pub recipe_id: u32,
    pub ingredient_ids: Vec<u32>,
    pub power: String,
    pub power_tier: i64,
    pub effect_label: Option<String>,
    pub race_restriction: Option<String>,
    pub item_name: Option<String>,
    pub first_seen_at: String,
    pub last_seen_at: String,
}

#[derive(Debug, Serialize)]
pub struct BrewingScanResult {
    pub new_discoveries: u32,
    pub updated_discoveries: u32,
    pub total_brewing_items: u32,
}

// ── String helpers ──────────────────────────────────────────────────────────

/// Strip numbers (and surrounding whitespace) from a string to create a template.
/// "Orcs gain +38 Max Power" → "orcs gain max power"
fn strip_numbers(s: &str) -> String {
    let mut result = String::new();
    let mut last_was_space = false;
    for c in s.chars() {
        if c.is_ascii_digit() || c == '+' || c == '-' || c == '.' || c == '%' {
            if !last_was_space && !result.is_empty() {
                result.push(' ');
                last_was_space = true;
            }
            continue;
        }
        if c == ' ' {
            if !last_was_space && !result.is_empty() {
                result.push(' ');
                last_was_space = true;
            }
        } else {
            result.push(c);
            last_was_space = false;
        }
    }
    result.trim().to_string()
}

// ── Race detection from power names ─────────────────────────────────────────

const RACE_MARKERS: &[(&str, &str)] = &[
    ("Elf", "Elf"),
    ("Rakshasa", "Rakshasa"),
    ("Orc", "Orc"),
    ("Dwarf", "Dwarf"),
    ("Fae", "Fae"),
    ("Human", "Human"),
    ("Lycanthrope", "Lycanthrope"),
];

fn detect_race_restriction(power: &str) -> Option<String> {
    for (marker, race) in RACE_MARKERS {
        if power.contains(marker) {
            return Some(race.to_string());
        }
    }
    None
}

// ── Effect label extraction ─────────────────────────────────────────────────

/// Extract the effect label from the drink name by comparing to the base recipe name.
/// E.g., "Partier's Dwarven Stout" vs base "Dwarven Stout" → "Partier's"
/// E.g., "Dwarven Stout of Elfinity" vs base "Dwarven Stout" → "of Elfinity"
fn extract_effect_label(item_name: &str, base_name: &str) -> Option<String> {
    let item_lower = item_name.to_lowercase();
    let base_lower = base_name.to_lowercase();

    // Check for suffix: "Base Name of Something"
    // NOTE: `pos` is a byte offset into the *lowercased* string. Lowercasing
    // can change byte length (e.g. U+0130 İ → i̇), so `pos` may not be a
    // char boundary in the original — and `base_name.len()` may not equal
    // the matched slice's length in the original. Guard every slice with
    // `get` (fall through to the generic handling on None) and measure the
    // match in the ORIGINAL string by consuming chars until their
    // lowercase equals the lowered base name.
    if let Some(pos) = item_lower.find(&base_lower) {
        if let (Some(prefix_raw), Some(rest)) = (item_name.get(..pos), item_name.get(pos..)) {
            let mut suffix_start = None;
            if !base_lower.is_empty() {
                let mut lowered = String::new();
                for (rel, c) in rest.char_indices() {
                    for lc in c.to_lowercase() {
                        lowered.push(lc);
                    }
                    if lowered == base_lower {
                        suffix_start = Some(pos + rel + c.len_utf8());
                        break;
                    }
                    if !base_lower.starts_with(lowered.as_str()) {
                        break;
                    }
                }
            }
            if let Some(suffix_start) = suffix_start {
                let prefix = prefix_raw.trim();
                let suffix = item_name.get(suffix_start..).map(str::trim).unwrap_or("");
                if !prefix.is_empty() && !suffix.is_empty() {
                    return Some(format!("{} ... {}", prefix, suffix));
                }
                if !prefix.is_empty() {
                    return Some(prefix.to_string());
                }
                if !suffix.is_empty() {
                    return Some(suffix.to_string());
                }
            }
        }
    }

    // Fallback: if names are identical, no label
    if item_lower == base_lower {
        return None;
    }

    // Can't determine — return the full name as the label
    Some(item_name.to_string())
}

// ── Delete a discovery ──────────────────────────────────────────────────────

#[tauri::command]
pub fn delete_brewing_discovery(discovery_id: i64, db: State<'_, DbPool>) -> Result<(), String> {
    let conn = db.get_write().map_err(|e| format!("DB error: {e}"))?;
    conn.execute(
        "DELETE FROM brewing_discoveries WHERE id = ?1",
        params![discovery_id],
    )
    .map_err(|e| format!("Delete error: {e}"))?;
    Ok(())
}

// ── Add a discovery manually ────────────────────────────────────────────────

#[tauri::command]
pub fn add_brewing_discovery_manual(
    character: String,
    recipe_id: u32,
    ingredient_ids: Vec<u32>,
    effect_label: Option<String>,
    db: State<'_, DbPool>,
) -> Result<BrewingDiscovery, String> {
    let conn = db.get_write().map_err(|e| format!("DB error: {e}"))?;

    let mut sorted_ids = ingredient_ids;
    sorted_ids.sort();
    let ing_ids_json = serde_json::to_string(&sorted_ids).unwrap_or_default();

    let timestamp = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();

    // Use "manual" as the power for manually entered discoveries without known effect data
    let power = "manual".to_string();
    let power_tier: i64 = 0;

    conn.execute(
        "INSERT INTO brewing_discoveries (
            character, recipe_id, ingredient_ids, power, power_tier,
            effect_label, race_restriction, item_name, first_seen_at, last_seen_at
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, NULL, NULL, ?7, ?7)
         ON CONFLICT(character, recipe_id, ingredient_ids) DO UPDATE SET
            last_seen_at = ?7,
            effect_label = COALESCE(?6, effect_label)",
        params![
            character,
            recipe_id,
            ing_ids_json,
            power,
            power_tier,
            effect_label,
            timestamp,
        ],
    )
    .map_err(|e| format!("Insert error: {e}"))?;

    // Fetch the inserted/updated row
    let disc = conn
        .query_row(
            "SELECT id, character, recipe_id, ingredient_ids, power, power_tier,
                    effect_label, race_restriction, item_name, first_seen_at, last_seen_at
             FROM brewing_discoveries
             WHERE character = ?1 AND recipe_id = ?2 AND ingredient_ids = ?3",
            params![character, recipe_id, ing_ids_json],
            map_discovery_row,
        )
        .map_err(|e| format!("Fetch error: {e}"))?;

    Ok(disc)
}

// ── Scan a snapshot's raw JSON for brewing discoveries ──────────────────────

#[tauri::command]
pub async fn scan_snapshot_for_brewing_discoveries(
    snapshot_id: i64,
    db: State<'_, DbPool>,
    game_data: State<'_, GameDataState>,
) -> Result<BrewingScanResult, String> {
    // Phase 1: read the snapshot row on a read connection.
    let (character, raw_json, timestamp) = {
        let conn = db.get().map_err(|e| format!("DB error: {e}"))?;
        read_snapshot_row(&conn, snapshot_id)?
    };

    // Phase 2: parse + resolve — the game-data guard is held only for this.
    let items = {
        let data = game_data.read().await;
        snapshot_brewing_items(&character, &raw_json, &data)?
    };

    // Phase 3: writer only around the upserts.
    let mut conn = db.get_write().map_err(|e| format!("DB error: {e}"))?;
    let tx = conn
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .map_err(|e| format!("Failed to start transaction: {e}"))?;
    let result = record_brewing_items(&*tx, &character, &timestamp, &items)?;
    tx.commit().map_err(|e| format!("Commit error: {e}"))?;
    Ok(result)
}

// ── Query discoveries ───────────────────────────────────────────────────────

#[tauri::command]
pub fn get_brewing_discoveries(
    character: String,
    recipe_id: Option<u32>,
    db: State<'_, DbPool>,
) -> Result<Vec<BrewingDiscovery>, String> {
    let conn = db.get().map_err(|e| format!("DB error: {e}"))?;

    if let Some(rid) = recipe_id {
        let mut stmt = conn
            .prepare(
                "SELECT id, character, recipe_id, ingredient_ids, power, power_tier,
                        effect_label, race_restriction, item_name, first_seen_at, last_seen_at
                 FROM brewing_discoveries
                 WHERE character = ?1 AND recipe_id = ?2
                 ORDER BY first_seen_at DESC",
            )
            .map_err(|e| format!("Query error: {e}"))?;

        let discoveries: Vec<BrewingDiscovery> = stmt
            .query_map(params![character, rid], map_discovery_row)
            .map_err(|e| format!("Query error: {e}"))?
            .filter_map(|r| r.ok())
            .collect();

        Ok(discoveries)
    } else {
        let mut stmt = conn
            .prepare(
                "SELECT id, character, recipe_id, ingredient_ids, power, power_tier,
                        effect_label, race_restriction, item_name, first_seen_at, last_seen_at
                 FROM brewing_discoveries
                 WHERE character = ?1
                 ORDER BY first_seen_at DESC",
            )
            .map_err(|e| format!("Query error: {e}"))?;

        let discoveries: Vec<BrewingDiscovery> = stmt
            .query_map(params![character], map_discovery_row)
            .map_err(|e| format!("Query error: {e}"))?
            .filter_map(|r| r.ok())
            .collect();

        Ok(discoveries)
    }
}

fn map_discovery_row(row: &rusqlite::Row) -> rusqlite::Result<BrewingDiscovery> {
    let ingredient_ids_json: String = row.get(3)?;
    let ingredient_ids: Vec<u32> = serde_json::from_str(&ingredient_ids_json).unwrap_or_default();

    Ok(BrewingDiscovery {
        id: row.get(0)?,
        character: row.get(1)?,
        recipe_id: row.get::<_, u32>(2)?,
        ingredient_ids,
        power: row.get(4)?,
        power_tier: row.get(5)?,
        effect_label: row.get(6)?,
        race_restriction: row.get(7)?,
        item_name: row.get(8)?,
        first_seen_at: row.get(9)?,
        last_seen_at: row.get(10)?,
    })
}

/// Scan ALL snapshots for a character to backfill brewing discoveries.
#[tauri::command]
pub async fn scan_all_snapshots_for_brewing(
    character: String,
    db: State<'_, DbPool>,
    game_data: State<'_, GameDataState>,
) -> Result<BrewingScanResult, String> {
    // Phase 1: read every snapshot row on a read connection, oldest first.
    let snapshots: Vec<(String, String, String)> = {
        let conn = db.get().map_err(|e| format!("DB error: {e}"))?;
        let mut stmt = conn
            .prepare(
                "SELECT character_name, raw_json, snapshot_timestamp
                 FROM character_item_snapshots
                 WHERE character_name = ?1
                 ORDER BY snapshot_timestamp ASC",
            )
            .map_err(|e| format!("Query error: {e}"))?;
        let rows = stmt
            .query_map(params![character], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            })
            .map_err(|e| format!("Query error: {e}"))?
            .filter_map(|r| r.ok())
            .collect::<Vec<_>>();
        drop(stmt);
        rows
    };

    // Phase 2: parse each snapshot — no connection held. The game-data read
    // guard is taken once for the whole parse loop (sync, no awaits inside).
    let parsed: Vec<(String, String, Vec<BrewingItem>)> = {
        let data = game_data.read().await;
        let mut out = Vec::with_capacity(snapshots.len());
        for (character_name, raw_json, timestamp) in snapshots {
            let items = snapshot_brewing_items(&character_name, &raw_json, &data)?;
            out.push((character_name, timestamp, items));
        }
        out
    };

    // Phase 3: one writer checkout, one transaction over all backfills.
    let mut conn = db.get_write().map_err(|e| format!("DB error: {e}"))?;
    let tx = conn
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .map_err(|e| format!("Failed to start transaction: {e}"))?;

    let mut total_new = 0u32;
    let mut total_updated = 0u32;
    let mut total_items = 0u32;
    for (character_name, timestamp, items) in &parsed {
        let result = record_brewing_items(&*tx, character_name, timestamp, items)?;
        total_new += result.new_discoveries;
        total_updated += result.updated_discoveries;
        total_items += result.total_brewing_items;
    }
    tx.commit().map_err(|e| format!("Commit error: {e}"))?;

    Ok(BrewingScanResult {
        new_discoveries: total_new,
        updated_discoveries: total_updated,
        total_brewing_items: total_items,
    })
}

/// One brewing-relevant item parsed from a snapshot, ready to upsert.
/// `power: None` means the item still counts toward `total_brewing_items`
/// but had no resolvable TSys power, so no row is written (matches the
/// original combined scan's skip-after-count behavior).
struct BrewingItem {
    recipe_id: u32,
    ing_ids_json: String,
    power: Option<String>,
    power_tier: i64,
    effect_label: Option<String>,
    race_restriction: Option<String>,
    item_name: String,
}

/// Fetch a snapshot's (character_name, raw_json, snapshot_timestamp) row.
/// Read-side helper — takes any connection, use `db.get()` for scans.
fn read_snapshot_row<C: DbRead + ?Sized>(
    conn: &C,
    snapshot_id: i64,
) -> Result<(String, String, String), String> {
    conn.query_row(
        "SELECT character_name, raw_json, snapshot_timestamp FROM character_item_snapshots WHERE id = ?1",
        params![snapshot_id],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    )
    .map_err(|e| format!("Storage report not found: {e}"))
}

/// Pure parse + resolve: turn a snapshot's raw JSON into the brewing items it
/// contains. No connection — this runs before the writer is checked out so
/// JSON parsing and game-data lookups never hold the sole write connection.
fn snapshot_brewing_items(
    character: &str,
    raw_json: &str,
    data: &crate::game_data::GameData,
) -> Result<Vec<BrewingItem>, String> {
    let report: Value =
        serde_json::from_str(raw_json).map_err(|e| format!("Failed to parse JSON: {e}"))?;

    let items = match report.get("Items").and_then(|v| v.as_array()) {
        Some(arr) => arr,
        None => return Ok(Vec::new()),
    };

    // Build result_item_id → recipe mapping
    let mut result_to_recipe: std::collections::HashMap<
        u32,
        &crate::game_data::brewing::BrewingRecipe,
    > = std::collections::HashMap::new();
    for recipe in &data.brewing_recipes {
        if let Some(result_id) = recipe.result_item_id {
            result_to_recipe.insert(result_id, recipe);
        }
    }

    let mut out = Vec::new();
    for item in items {
        let ingredient_ids = match item.get("IngredientItemTypeIds").and_then(|v| v.as_array()) {
            Some(arr) => arr,
            None => continue,
        };
        let tsys_powers = match item.get("TSysPowers").and_then(|v| v.as_array()) {
            Some(arr) if !arr.is_empty() => arr,
            _ => continue,
        };
        let type_id = match item.get("TypeID").and_then(|v| v.as_u64()) {
            Some(id) => id as u32,
            None => continue,
        };

        // Only count items crafted by this character — skip other players' brews
        let crafter = item.get("Crafter").and_then(|v| v.as_str()).unwrap_or("");
        if !crafter.eq_ignore_ascii_case(character) {
            continue;
        }

        let recipe = match result_to_recipe.get(&type_id) {
            Some(r) => r,
            None => continue,
        };

        let mut ing_ids: Vec<u32> = ingredient_ids
            .iter()
            .filter_map(|v| v.as_u64().map(|n| n as u32))
            .collect();
        ing_ids.sort();
        let ing_ids_json = serde_json::to_string(&ing_ids).unwrap_or_default();

        let power = tsys_powers[0]
            .get("Power")
            .and_then(|v| v.as_str())
            .map(String::from);
        let power_tier = tsys_powers[0]
            .get("Tier")
            .and_then(|v| v.as_i64())
            .unwrap_or(0);

        let item_name = item.get("Name").and_then(|v| v.as_str()).unwrap_or("");
        let base_name = data
            .items
            .get(&type_id)
            .map(|i| i.name.as_str())
            .unwrap_or("");

        let race_restriction = power.as_deref().and_then(detect_race_restriction);

        out.push(BrewingItem {
            recipe_id: recipe.recipe_id,
            ing_ids_json,
            power,
            power_tier,
            effect_label: extract_effect_label(item_name, base_name),
            race_restriction,
            item_name: item_name.to_string(),
        });
    }
    Ok(out)
}

/// Write phase: upsert parsed brewing items under this character/timestamp.
/// Works on any connection (writer or transaction). Items with no resolved
/// power still count toward `total_brewing_items` but write no row.
fn record_brewing_items(
    conn: &rusqlite::Connection,
    character: &str,
    timestamp: &str,
    items: &[BrewingItem],
) -> Result<BrewingScanResult, String> {
    let mut new_discoveries = 0u32;
    let mut updated_discoveries = 0u32;
    let total_brewing_items = items.len() as u32;

    let mut insert_stmt = conn
        .prepare(
            "INSERT INTO brewing_discoveries (
                character, recipe_id, ingredient_ids, power, power_tier,
                effect_label, race_restriction, item_name, first_seen_at, last_seen_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?9)
             ON CONFLICT(character, recipe_id, ingredient_ids) DO UPDATE SET
                last_seen_at = ?9",
        )
        .map_err(|e| format!("Failed to prepare insert: {e}"))?;

    for item in items {
        let power = match &item.power {
            Some(p) => p,
            None => continue,
        };

        let changes = insert_stmt
            .execute(params![
                character,
                item.recipe_id,
                item.ing_ids_json,
                power,
                item.power_tier,
                item.effect_label,
                item.race_restriction,
                item.item_name,
                timestamp,
            ])
            .map_err(|e| format!("Failed to insert discovery: {e}"))?;

        if changes > 0 {
            let disc_id = conn.last_insert_rowid();
            let is_new: bool = conn
                .query_row(
                    "SELECT first_seen_at = last_seen_at FROM brewing_discoveries WHERE rowid = ?1",
                    params![disc_id],
                    |row| row.get(0),
                )
                .unwrap_or(true);

            if is_new {
                new_discoveries += 1;
            } else {
                updated_discoveries += 1;
            }
        }
    }

    Ok(BrewingScanResult {
        new_discoveries,
        updated_discoveries,
        total_brewing_items,
    })
}

// ── CSV import ──────────────────────────────────────────────────────────────

/// Import brewing discoveries from a CSV file.
///
/// Flexible header-based format. Required: `recipe_name` + at least one `ingredientN`.
/// Optional: `effect_name`, `power`, `power_tier`, `item_name`, `type_id`.
///
/// If `power` is missing, we store `effect_name` as the label with power="unknown".
/// If `type_id` is missing, we match `recipe_name` against CDN recipe names.
#[tauri::command]
pub async fn import_brewing_discoveries_csv(
    file_path: String,
    character: String,
    db: State<'_, DbPool>,
    game_data: State<'_, GameDataState>,
) -> Result<BrewingScanResult, String> {
    let data = game_data.read().await;

    let csv_content =
        std::fs::read_to_string(&file_path).map_err(|e| format!("Failed to read file: {e}"))?;

    let mut lines = csv_content.lines();

    // Parse header to find column indices
    let header_line = lines.next().ok_or("CSV file is empty")?;
    let headers: Vec<String> = header_line
        .split(',')
        .map(|s| s.trim().to_lowercase())
        .collect();

    let col = |name: &str| -> Option<usize> { headers.iter().position(|h| h == name) };

    // Required: recipe_name OR type_id, plus ingredients
    let col_recipe_name = col("recipe_name");
    let col_ing1 = col("ingredient1").or_else(|| col("ingredient_1"));
    let col_ing2 = col("ingredient2").or_else(|| col("ingredient_2"));
    let col_ing3 = col("ingredient3").or_else(|| col("ingredient_3"));
    let col_ing4 = col("ingredient4").or_else(|| col("ingredient_4"));
    let col_effect_name = col("effect_name").or_else(|| col("effect"));
    let col_effect_desc = col("effect_desc")
        .or_else(|| col("effects"))
        .or_else(|| col("description"));
    let col_power = col("power");
    let col_power_tier = col("power_tier");
    let col_item_name = col("item_name");
    let col_type_id = col("type_id");

    let ingredient_cols = [col_ing1, col_ing2, col_ing3, col_ing4];

    if col_recipe_name.is_none() && col_type_id.is_none() {
        return Err("CSV must have a 'recipe_name' or 'type_id' column".to_string());
    }
    if ingredient_cols.iter().all(|c| c.is_none()) {
        return Err(
            "CSV must have at least one ingredient column (ingredient1..ingredient4)".to_string(),
        );
    }

    // Build lookups
    let mut name_to_id: std::collections::HashMap<String, u32> = std::collections::HashMap::new();
    for (id, item) in &data.items {
        name_to_id.insert(item.name.to_lowercase(), *id);
        if let Some(ref iname) = item.internal_name {
            name_to_id.insert(iname.to_lowercase(), *id);
        }
    }

    // recipe name → recipe (case-insensitive, match base name without "(One Glass)" etc.)
    let mut recipe_name_to_recipe: std::collections::HashMap<
        String,
        &crate::game_data::brewing::BrewingRecipe,
    > = std::collections::HashMap::new();
    let mut result_to_recipe: std::collections::HashMap<
        u32,
        &crate::game_data::brewing::BrewingRecipe,
    > = std::collections::HashMap::new();
    for recipe in &data.brewing_recipes {
        let name_lower = recipe.name.to_lowercase();
        recipe_name_to_recipe.insert(name_lower.clone(), recipe);
        // Also index without parenthetical suffixes: "Dwarven Stout (One Glass)" → "dwarven stout"
        if let Some(pos) = name_lower.find('(') {
            let short = name_lower[..pos].trim().to_string();
            recipe_name_to_recipe.entry(short).or_insert(recipe);
        }
        if let Some(iname) = &recipe.internal_name {
            recipe_name_to_recipe.insert(iname.to_lowercase(), recipe);
        }
        if let Some(result_id) = recipe.result_item_id {
            result_to_recipe.insert(result_id, recipe);
        }
    }

    // TSys prefix/suffix → power name (for resolving effect names like "Partier's")
    // Also build effect description text → power name lookup
    let mut label_to_power: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();
    let mut desc_to_power: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();
    for info in data.tsys.client_info.values() {
        if let Some(ref iname) = info.internal_name {
            if !iname.starts_with("Brewing") {
                continue;
            }
            if let Some(ref prefix) = info.prefix {
                label_to_power.insert(prefix.to_lowercase(), iname.clone());
            }
            if let Some(ref suffix) = info.suffix {
                label_to_power.insert(suffix.to_lowercase(), iname.clone());
            }
            // Build description templates from all tiers
            for tier_info in info.tiers.values() {
                for desc in &tier_info.effect_descs {
                    if let Some(resolved) =
                        crate::cdn_commands::resolve_single_effect_public(desc, &data)
                    {
                        // Strip numbers to create a matchable template
                        let template = strip_numbers(&resolved.formatted).to_lowercase();
                        if !template.is_empty() {
                            desc_to_power.insert(template, iname.clone());
                        }
                    }
                }
            }
        }
    }

    let mut conn = db.get_write().map_err(|e| format!("DB error: {e}"))?;
    let conn = conn
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .map_err(|e| format!("Failed to start transaction: {e}"))?;
    let mut insert_stmt = conn
        .prepare(
            "INSERT INTO brewing_discoveries (
                character, recipe_id, ingredient_ids, power, power_tier,
                effect_label, race_restriction, item_name, first_seen_at, last_seen_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?9)
             ON CONFLICT(character, recipe_id, ingredient_ids) DO UPDATE SET
                last_seen_at = ?9",
        )
        .map_err(|e| format!("Failed to prepare insert: {e}"))?;

    let timestamp = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();

    let mut new_discoveries = 0u32;
    let mut updated_discoveries = 0u32;
    let mut total_brewing_items = 0u32;
    let mut skipped_lines = Vec::new();

    for (line_num, line) in lines.enumerate() {
        let line_display = line_num + 2; // +2 because header is line 1 and enumerate starts at 0
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        let fields: Vec<&str> = line.split(',').collect();

        let get_field = |col_idx: Option<usize>| -> &str {
            col_idx
                .and_then(|i| fields.get(i))
                .map(|s| s.trim())
                .unwrap_or("")
        };

        // Resolve ingredients
        let mut ingredient_ids: Vec<u32> = Vec::new();
        let mut resolve_failed = false;
        for col_opt in &ingredient_cols {
            let name = get_field(*col_opt);
            if name.is_empty() {
                continue;
            }
            match name_to_id.get(&name.to_lowercase()) {
                Some(id) => ingredient_ids.push(*id),
                None => {
                    skipped_lines.push(format!(
                        "Line {}: unknown ingredient \"{}\"",
                        line_display, name
                    ));
                    resolve_failed = true;
                    break;
                }
            }
        }
        if resolve_failed || ingredient_ids.is_empty() {
            continue;
        }

        // Resolve recipe: try type_id first, then recipe_name
        let type_id_str = get_field(col_type_id);
        let type_id: Option<u32> = if type_id_str.is_empty() {
            None
        } else {
            type_id_str.parse().ok()
        };

        let recipe = if let Some(tid) = type_id {
            result_to_recipe.get(&tid).copied()
        } else {
            None
        }
        .or_else(|| {
            let name = get_field(col_recipe_name);
            if name.is_empty() {
                return None;
            }
            recipe_name_to_recipe.get(&name.to_lowercase()).copied()
        });

        let recipe = match recipe {
            Some(r) => r,
            None => {
                skipped_lines.push(format!("Line {}: could not match recipe", line_display));
                continue;
            }
        };

        // Resolve power: try explicit power field, then effect_name, then effect_desc
        let mut power = get_field(col_power).to_string();
        let effect_name = get_field(col_effect_name).to_string();
        let effect_desc = get_field(col_effect_desc).to_string();

        if power.is_empty() && !effect_name.is_empty() {
            // Try to resolve effect_name to a power via TSys prefix/suffix
            let label_lower = effect_name.to_lowercase();
            if let Some(resolved) = label_to_power.get(&label_lower) {
                power = resolved.clone();
            } else {
                // Try with "of " prefix stripped
                let stripped = label_lower
                    .strip_prefix("of ")
                    .unwrap_or(&label_lower)
                    .to_string();
                if let Some(resolved) = label_to_power.get(&stripped) {
                    power = resolved.clone();
                }
            }
        }

        // Try matching effect_desc (e.g., "Orcs gain +38 Max Power") against
        // resolved TSys effect descriptions by stripping numbers
        if power.is_empty() && !effect_desc.is_empty() {
            // The desc might contain multiple effects separated by " / "
            // Try matching the first one
            let first_desc = effect_desc.split(" / ").next().unwrap_or(&effect_desc);
            let template = strip_numbers(first_desc).to_lowercase();
            if let Some(resolved) = desc_to_power.get(&template) {
                power = resolved.clone();
            }
        }

        // If we still don't have a power, use "unknown" — we at least record the combo
        if power.is_empty() {
            power = "unknown".to_string();
        }

        let power_tier: i64 = get_field(col_power_tier).parse().unwrap_or(0);
        let item_name = get_field(col_item_name);

        total_brewing_items += 1;

        ingredient_ids.sort();
        let ing_ids_json = serde_json::to_string(&ingredient_ids).unwrap_or_default();

        let effect_label = if !effect_name.is_empty() {
            Some(effect_name.clone())
        } else if !effect_desc.is_empty() {
            // Use the effect description as the label when no name is given
            Some(effect_desc.clone())
        } else if !item_name.is_empty() {
            let base_name = type_id
                .and_then(|tid| data.items.get(&tid))
                .map(|i| i.name.as_str())
                .unwrap_or("");
            extract_effect_label(item_name, base_name)
        } else {
            None
        };

        let race_restriction = detect_race_restriction(&power);

        let changes = insert_stmt
            .execute(params![
                character,
                recipe.recipe_id,
                ing_ids_json,
                power,
                power_tier,
                effect_label,
                race_restriction,
                if !item_name.is_empty() {
                    item_name
                } else if !effect_name.is_empty() {
                    &effect_name
                } else if !effect_desc.is_empty() {
                    &effect_desc
                } else {
                    ""
                },
                timestamp,
            ])
            .map_err(|e| format!("Failed to insert discovery: {e}"))?;

        if changes > 0 {
            let disc_id = conn.last_insert_rowid();
            let is_new: bool = conn
                .query_row(
                    "SELECT first_seen_at = last_seen_at FROM brewing_discoveries WHERE rowid = ?1",
                    params![disc_id],
                    |row| row.get(0),
                )
                .unwrap_or(true);

            if is_new {
                new_discoveries += 1;
            } else {
                updated_discoveries += 1;
            }
        }
    }

    if !skipped_lines.is_empty() {
        eprintln!(
            "Brewing CSV import: {} lines skipped:\n{}",
            skipped_lines.len(),
            skipped_lines.join("\n")
        );
    }

    drop(insert_stmt);
    conn.commit()
        .map_err(|e| format!("Failed to commit transaction: {e}"))?;

    Ok(BrewingScanResult {
        new_discoveries,
        updated_discoveries,
        total_brewing_items,
    })
}
