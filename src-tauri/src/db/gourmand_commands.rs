use rusqlite::params;
use serde::Serialize;
use std::path::Path;
use std::sync::Arc;
use tauri::State;

use super::DbPool;
use crate::settings::SettingsManager;

// ── Response types ────────────────────────────────────────────────────────────

#[derive(Debug, Serialize, Clone)]
pub struct FoodItemInfo {
    pub item_id: u32,
    pub name: String,
    pub icon_id: Option<u32>,
    pub food_category: String,
    pub food_level: i64,
    pub gourmand_req: Option<i64>,
    pub effect_descs: Vec<String>,
    pub keywords: Vec<String>,
    pub value: Option<f32>,
    /// Where this food comes from, most-significant kind first. See
    /// `game_data::food_sources`. Empty until the CDN has been re-persisted
    /// after migration v67.
    pub source_kinds: Vec<String>,
    /// Display names of the skills that can craft it (empty if uncraftable).
    pub craft_skills: Vec<String>,
}

#[derive(Debug, Serialize, Clone)]
pub struct GourmandFoodEntry {
    pub name: String,
    pub count: u32,
    pub manually_marked: bool,
}

#[derive(Debug, Serialize, Clone)]
pub struct GourmandImportResult {
    pub foods_imported: usize,
}

// ── Tauri commands ────────────────────────────────────────────────────────────

/// Get all food items from the pre-parsed foods table
#[tauri::command]
pub fn get_all_foods(db: State<'_, DbPool>) -> Result<Vec<FoodItemInfo>, String> {
    let conn = db
        .get()
        .map_err(|e| format!("Database connection error: {e}"))?;

    let mut stmt = conn.prepare(
        "SELECT item_id, name, icon_id, food_category, food_level, gourmand_req, effect_descs, keywords, value, source_kinds, craft_skills
         FROM foods
         ORDER BY food_level ASC, name ASC"
    ).map_err(|e| format!("Query prepare error: {e}"))?;

    let foods = stmt
        .query_map([], |row| {
            let effects_str: String = row.get(6)?;
            let keywords_str: String = row.get(7)?;
            let source_kinds_str: String = row.get(9)?;
            let craft_skills_str: String = row.get(10)?;

            let effect_descs: Vec<String> = serde_json::from_str(&effects_str).unwrap_or_default();
            let keywords: Vec<String> = serde_json::from_str(&keywords_str).unwrap_or_default();
            let source_kinds: Vec<String> =
                serde_json::from_str(&source_kinds_str).unwrap_or_default();
            let craft_skills: Vec<String> =
                serde_json::from_str(&craft_skills_str).unwrap_or_default();

            Ok(FoodItemInfo {
                item_id: row.get(0)?,
                name: row.get(1)?,
                icon_id: row.get(2)?,
                food_category: row.get(3)?,
                food_level: row.get(4)?,
                gourmand_req: row.get(5)?,
                effect_descs,
                keywords,
                value: row.get(8)?,
                source_kinds,
                craft_skills,
            })
        })
        .map_err(|e| format!("Query error: {e}"))?;

    let result: Vec<FoodItemInfo> = foods.filter_map(|r| r.ok()).collect();
    Ok(result)
}

/// Import a gourmand report from a content string (e.g. from a ProcessBook event).
/// Called by the coordinator when a "Foods Consumed" SkillReport book is opened.
/// Preserves manually-marked foods that aren't in the report.
pub fn import_gourmand_from_content(
    conn: &rusqlite::Connection,
    content: &str,
) -> Result<usize, String> {
    let entries = parse_gourmand_report(content)?;
    if entries.is_empty() {
        return Ok(0);
    }

    let tx = conn
        .unchecked_transaction()
        .map_err(|e| format!("Failed to begin transaction: {e}"))?;

    // Delete only non-manual entries; manual marks are preserved
    tx.execute(
        "DELETE FROM gourmand_eaten_foods WHERE manually_marked = 0",
        [],
    )
    .map_err(|e| format!("Failed to clear old data: {e}"))?;

    // Upsert: if a manually-marked entry matches a report entry, upgrade it to imported
    let mut stmt = tx
        .prepare(
            "INSERT INTO gourmand_eaten_foods (food_name, times_eaten, manually_marked)
             VALUES (?1, ?2, 0)
             ON CONFLICT(food_name) DO UPDATE SET times_eaten = excluded.times_eaten, manually_marked = 0",
        )
        .map_err(|e| format!("Prepare error: {e}"))?;

    for entry in &entries {
        stmt.execute(params![&entry.name, entry.count])
            .map_err(|e| format!("Insert error: {e}"))?;
    }
    drop(stmt);

    tx.commit()
        .map_err(|e| format!("Failed to commit transaction: {e}"))?;

    Ok(entries.len())
}

/// Import a gourmand report from a text file, persist as "last known" snapshot
#[tauri::command]
pub fn import_gourmand_report(
    db: State<'_, DbPool>,
    file_path: String,
) -> Result<GourmandImportResult, String> {
    let content =
        std::fs::read_to_string(&file_path).map_err(|e| format!("Failed to read file: {e}"))?;

    let entries = parse_gourmand_report(&content)?;

    let mut conn = db
        .get_write()
        .map_err(|e| format!("Database connection error: {e}"))?;
    let conn = conn
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .map_err(|e| format!("Failed to begin transaction: {e}"))?;

    // Delete only non-manual entries; manual marks are preserved
    conn.execute(
        "DELETE FROM gourmand_eaten_foods WHERE manually_marked = 0",
        [],
    )
    .map_err(|e| format!("Failed to clear old data: {e}"))?;

    // Upsert: if a manually-marked entry matches a report entry, upgrade it to imported
    let mut stmt = conn
        .prepare(
            "INSERT INTO gourmand_eaten_foods (food_name, times_eaten, manually_marked)
             VALUES (?1, ?2, 0)
             ON CONFLICT(food_name) DO UPDATE SET times_eaten = excluded.times_eaten, manually_marked = 0",
        )
        .map_err(|e| format!("Prepare error: {e}"))?;

    for entry in &entries {
        stmt.execute(params![&entry.name, entry.count])
            .map_err(|e| format!("Insert error: {e}"))?;
    }
    drop(stmt);

    conn.commit()
        .map_err(|e| format!("Failed to commit transaction: {e}"))?;

    Ok(GourmandImportResult {
        foods_imported: entries.len(),
    })
}

/// Record a single food-eaten event detected live from the log
/// (`ProcessDoDelayLoop(1.5, Eat, "Using <Food Name>", ...)`). Increments the
/// existing count if present, otherwise inserts a new non-manual entry.
/// Does not touch `manually_marked` so a manual flag survives a live update.
pub fn record_food_eaten(conn: &rusqlite::Connection, food_name: &str) -> Result<(), String> {
    conn.execute(
        "INSERT INTO gourmand_eaten_foods (food_name, times_eaten, manually_marked)
         VALUES (?1, 1, 0)
         ON CONFLICT(food_name) DO UPDATE SET times_eaten = times_eaten + 1",
        params![food_name],
    )
    .map(|_| ())
    .map_err(|e| format!("Failed to record eaten food: {e}"))
}

/// Get the last-imported eaten foods from the database
#[tauri::command]
pub fn get_gourmand_eaten_foods(db: State<'_, DbPool>) -> Result<Vec<GourmandFoodEntry>, String> {
    let conn = db
        .get()
        .map_err(|e| format!("Database connection error: {e}"))?;

    let mut stmt = conn
        .prepare("SELECT food_name, times_eaten, manually_marked FROM gourmand_eaten_foods ORDER BY food_name ASC")
        .map_err(|e| format!("Query prepare error: {e}"))?;

    let entries = stmt
        .query_map([], |row| {
            let manually_marked_int: i32 = row.get(2)?;
            Ok(GourmandFoodEntry {
                name: row.get(0)?,
                count: row.get(1)?,
                manually_marked: manually_marked_int != 0,
            })
        })
        .map_err(|e| format!("Query error: {e}"))?;

    let result: Vec<GourmandFoodEntry> = entries.filter_map(|r| r.ok()).collect();
    Ok(result)
}

/// Toggle a food's eaten status manually.
/// If the food is not in the eaten table, insert it as manually marked.
/// If it's already there and was manually marked, remove it.
/// If it was imported (not manually marked), remove it (user is overriding).
#[tauri::command]
pub fn toggle_food_eaten_status(db: State<'_, DbPool>, food_name: String) -> Result<bool, String> {
    let conn = db
        .get_write()
        .map_err(|e| format!("Database connection error: {e}"))?;

    // Check if the food is already in the eaten table
    let existing: Option<bool> = conn
        .query_row(
            "SELECT manually_marked FROM gourmand_eaten_foods WHERE food_name = ?1",
            params![&food_name],
            |row| {
                let val: i32 = row.get(0)?;
                Ok(val != 0)
            },
        )
        .ok();

    match existing {
        Some(_) => {
            // Food exists — remove it (toggle off)
            conn.execute(
                "DELETE FROM gourmand_eaten_foods WHERE food_name = ?1",
                params![&food_name],
            )
            .map_err(|e| format!("Delete error: {e}"))?;
            Ok(false) // now uneaten
        }
        None => {
            // Food not in table — add it as manually marked
            conn.execute(
                "INSERT INTO gourmand_eaten_foods (food_name, times_eaten, manually_marked) VALUES (?1, 1, 1)",
                params![&food_name],
            )
            .map_err(|e| format!("Insert error: {e}"))?;
            Ok(true) // now eaten
        }
    }
}

/// Write text content to a file (used for uneaten food export)
#[tauri::command]
pub fn export_text_file(file_path: String, content: String) -> Result<(), String> {
    std::fs::write(&file_path, &content).map_err(|e| format!("Failed to write file: {e}"))
}

/// Scan the Books folder for the latest gourmand report and auto-import it.
/// Returns Some(result) if a new report was imported, None otherwise.
#[tauri::command]
pub fn import_latest_gourmand_report(
    db: State<'_, DbPool>,
    settings_manager: State<'_, Arc<SettingsManager>>,
) -> Result<Option<GourmandImportResult>, String> {
    let settings = settings_manager.get();
    let game_data_path = &settings.game_data_path;
    if game_data_path.is_empty() {
        return Ok(None);
    }

    let books_dir = Path::new(game_data_path).join("Books");
    if !books_dir.is_dir() {
        return Ok(None);
    }

    let best_file = find_latest_gourmand_report(&books_dir)?;
    let file_path = match best_file {
        Some(p) => p,
        None => return Ok(None),
    };

    let content =
        std::fs::read_to_string(&file_path).map_err(|e| format!("Failed to read file: {e}"))?;

    let entries = parse_gourmand_report(&content)?;
    if entries.is_empty() {
        return Ok(None);
    }

    let mut conn = db
        .get_write()
        .map_err(|e| format!("Database connection error: {e}"))?;
    let conn = conn
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .map_err(|e| format!("Failed to begin transaction: {e}"))?;

    // Skip only when the report matches what we already store exactly. A
    // same-sized but different report (one food swapped for another) must
    // still import, so diff the (food_name, times_eaten) sets in both
    // directions with EXCEPT. Only non-manual rows are compared; manual
    // marks are preserved regardless.
    conn.execute_batch(
        "CREATE TEMPORARY TABLE gourmand_import_diff (food_name TEXT NOT NULL, times_eaten INTEGER NOT NULL)",
    )
    .map_err(|e| format!("Failed to stage import diff: {e}"))?;
    {
        let mut stmt = conn
            .prepare(
                "INSERT INTO gourmand_import_diff (food_name, times_eaten) VALUES (?1, ?2)",
            )
            .map_err(|e| format!("Failed to stage import diff: {e}"))?;
        for entry in &entries {
            stmt.execute(params![&entry.name, entry.count])
                .map_err(|e| format!("Failed to stage import diff: {e}"))?;
        }
    }
    let diff_count: i64 = conn
        .query_row(
            "SELECT
               (SELECT COUNT(*) FROM (
                    SELECT food_name, times_eaten FROM gourmand_eaten_foods WHERE manually_marked = 0
                    EXCEPT
                    SELECT food_name, times_eaten FROM gourmand_import_diff
                )) +
               (SELECT COUNT(*) FROM (
                    SELECT food_name, times_eaten FROM gourmand_import_diff
                    EXCEPT
                    SELECT food_name, times_eaten FROM gourmand_eaten_foods WHERE manually_marked = 0
                ))",
            [],
            |row| row.get(0),
        )
        .map_err(|e| format!("Failed to compare import data: {e}"))?;
    conn.execute_batch("DROP TABLE gourmand_import_diff")
        .map_err(|e| format!("Failed to clean up import diff: {e}"))?;

    if diff_count == 0 {
        // Identical data — skip the re-import. (Returning here drops the
        // transaction, rolling back the no-op.)
        return Ok(None);
    }

    // Delete only non-manual entries; manual marks are preserved
    conn.execute(
        "DELETE FROM gourmand_eaten_foods WHERE manually_marked = 0",
        [],
    )
    .map_err(|e| format!("Failed to clear old data: {e}"))?;

    // Upsert: if a manually-marked entry matches a report entry, upgrade it to imported
    let mut stmt = conn
        .prepare(
            "INSERT INTO gourmand_eaten_foods (food_name, times_eaten, manually_marked)
             VALUES (?1, ?2, 0)
             ON CONFLICT(food_name) DO UPDATE SET times_eaten = excluded.times_eaten, manually_marked = 0",
        )
        .map_err(|e| format!("Prepare error: {e}"))?;

    for entry in &entries {
        stmt.execute(params![&entry.name, entry.count])
            .map_err(|e| format!("Insert error: {e}"))?;
    }
    drop(stmt);

    conn.commit()
        .map_err(|e| format!("Failed to commit transaction: {e}"))?;

    Ok(Some(GourmandImportResult {
        foods_imported: entries.len(),
    }))
}

/// Find the latest SkillReport_*.txt file in the Books folder that is a gourmand report
fn find_latest_gourmand_report(books_dir: &Path) -> Result<Option<std::path::PathBuf>, String> {
    let entries =
        std::fs::read_dir(books_dir).map_err(|e| format!("Failed to read Books directory: {e}"))?;

    let mut best: Option<(String, std::path::PathBuf)> = None;

    for entry in entries.flatten() {
        let file_name = entry.file_name().to_string_lossy().to_string();
        if !file_name.starts_with("SkillReport_") || !file_name.ends_with(".txt") {
            continue;
        }

        let file_path = entry.path();

        // Read the first line to check if it's a gourmand report
        let content = match std::fs::read_to_string(&file_path) {
            Ok(c) => c,
            Err(_) => continue,
        };

        let first_line = content.lines().next().unwrap_or("");
        if first_line.trim() != "Foods Consumed:" {
            continue;
        }

        // Use the filename as the sort key (contains timestamp: SkillReport_YYMMDD_HHMMSS.txt)
        if best
            .as_ref()
            .is_none_or(|(existing, _)| file_name > *existing)
        {
            best = Some((file_name, file_path));
        }
    }

    Ok(best.map(|(_, path)| path))
}

// ── Report parsing ────────────────────────────────────────────────────────────

/// Parse a gourmand report text file into food entries.
///
/// Expected format:
/// ```text
/// Foods Consumed:
///
///   All-Flavor Chicken (HAS MEAT) (HAS DAIRY): 2
///   Almonds: 1
/// ```
pub fn parse_gourmand_report(content: &str) -> Result<Vec<GourmandFoodEntry>, String> {
    let mut entries = Vec::new();

    for line in content.lines() {
        let trimmed = line.trim();

        // Skip header, blank lines, and non-food lines
        if trimmed.is_empty() || trimmed == "Foods Consumed:" {
            continue;
        }

        // Find the last colon — count is after it
        let last_colon = match trimmed.rfind(':') {
            Some(idx) => idx,
            None => continue,
        };

        let count_str = trimmed[last_colon + 1..].trim();
        let count: u32 = match count_str.parse() {
            Ok(n) => n,
            Err(_) => continue,
        };

        let name_part = trimmed[..last_colon].trim();

        // Strip parenthetical tags like (HAS MEAT), (HAS DAIRY), (HAS EGGS)
        let name = strip_food_tags(name_part);

        if !name.is_empty() {
            entries.push(GourmandFoodEntry {
                name,
                count,
                manually_marked: false,
            });
        }
    }

    Ok(entries)
}

/// Strip known parenthetical tags like "(HAS MEAT)", "(HAS DAIRY)", "(HAS EGGS)" from a food name,
/// but preserve other parenthetical content (e.g. "Flesh Lump (Elf)").
fn strip_food_tags(name: &str) -> String {
    const TAGS: &[&str] = &["(HAS MEAT)", "(HAS DAIRY)", "(HAS EGGS)"];
    let mut result = name.to_string();
    for tag in TAGS {
        result = result.replace(tag, "");
    }
    result.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_strip_food_tags() {
        assert_eq!(
            strip_food_tags("All-Flavor Chicken (HAS MEAT) (HAS DAIRY)"),
            "All-Flavor Chicken"
        );
        assert_eq!(strip_food_tags("Almonds"), "Almonds");
        assert_eq!(
            strip_food_tags("Cottage Pie (HAS MEAT) (HAS DAIRY)"),
            "Cottage Pie"
        );
        // Parenthetical content that is NOT a known tag should be preserved
        assert_eq!(
            strip_food_tags("Flesh Lump (Elf) (HAS MEAT)"),
            "Flesh Lump (Elf)"
        );
        assert_eq!(
            strip_food_tags("Flesh Lump (Human) (HAS MEAT)"),
            "Flesh Lump (Human)"
        );
    }

    #[test]
    fn test_parse_gourmand_report() {
        let content = "Foods Consumed:\n\n  All-Flavor Chicken (HAS MEAT) (HAS DAIRY): 2\n  Almonds: 1\n  Cactus Juice: 38\n";
        let entries = parse_gourmand_report(content).unwrap();
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].name, "All-Flavor Chicken");
        assert_eq!(entries[0].count, 2);
        assert_eq!(entries[1].name, "Almonds");
        assert_eq!(entries[1].count, 1);
        assert_eq!(entries[2].name, "Cactus Juice");
        assert_eq!(entries[2].count, 38);
    }
}
