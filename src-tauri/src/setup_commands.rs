use chrono::Local;
use serde::{Deserialize, Serialize};
/// Tauri commands for first-time setup and character management
///
/// Handles game folder validation, character discovery from Reports,
/// and user_characters CRUD for the startup flow.
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use tauri::State;

use crate::cdn_commands::GameDataState;
use crate::db::DbPool;
use crate::settings::{get_default_player_log_path, SettingsManager};

/// Timestamped log line for startup diagnostics.
macro_rules! startup_log {
    ($($arg:tt)*) => {
        eprintln!("[{}] {}", Local::now().format("%H:%M:%S%.3f"), format!($($arg)*));
    };
}

// ── Response Types ──────────────────────────────────────────────────────────

#[derive(Serialize)]
pub struct GameDataPathValidation {
    pub path_exists: bool,
    pub player_log_found: bool,
    pub chat_logs_found: bool,
    pub reports_found: bool,
}

#[derive(Serialize, Clone)]
pub struct DiscoveredCharacter {
    pub character_name: String,
    pub server_name: String,
    pub report_count: usize,
    pub latest_report_time: Option<String>,
}

#[derive(Serialize)]
pub struct UserCharacter {
    pub id: i64,
    pub character_name: String,
    pub server_name: String,
    pub source: String,
    pub is_active: bool,
    pub latest_report_time: Option<String>,
    pub last_login_time: Option<String>,
}

// ── Minimal JSON struct for reading report headers ──────────────────────────

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct ReportHeader {
    character: String,
    server_name: String,
    timestamp: Option<String>,
    report: Option<String>,
    #[serde(default)]
    race: Option<String>,
}

/// Placeholder reports from the game's extra-report bug: missing character
/// name, "???" name, or race "error". Filtered out of every scan/import path.
fn is_placeholder_report(character: &str, race: Option<&str>) -> bool {
    let name = character.trim();
    if name.is_empty() || name == "???" {
        return true;
    }
    if let Some(r) = race {
        if r.trim().eq_ignore_ascii_case("error") {
            return true;
        }
    }
    false
}

// ── Commands ────────────────────────────────────────────────────────────────

#[tauri::command]
pub fn validate_game_data_path(path: String) -> GameDataPathValidation {
    let base = Path::new(&path);

    // Player.log may live alongside game data (Windows) or at the default
    // macOS path (~/Library/Logs/Elder Game/Project Gorgon/Player.log).
    let player_log_found = base.join("Player.log").exists()
        || Path::new(&get_default_player_log_path()).exists();

    GameDataPathValidation {
        path_exists: base.exists() && base.is_dir(),
        player_log_found,
        chat_logs_found: base.join("ChatLogs").is_dir(),
        reports_found: base.join("Reports").is_dir(),
    }
}

#[tauri::command]
pub fn scan_reports_for_characters(path: String) -> Result<Vec<DiscoveredCharacter>, String> {
    let reports_dir = Path::new(&path).join("Reports");

    if !reports_dir.is_dir() {
        return Ok(Vec::new());
    }

    let entries = std::fs::read_dir(&reports_dir)
        .map_err(|e| format!("Failed to read Reports directory: {e}"))?;

    // Accumulate by (character_name, server_name)
    let mut char_map: HashMap<(String, String), (usize, Option<String>)> = HashMap::new();

    for entry in entries.flatten() {
        let file_name = entry.file_name().to_string_lossy().to_string();

        // Only process Character_*.json files
        if !file_name.starts_with("Character_") || !file_name.ends_with(".json") {
            continue;
        }

        let file_path = entry.path();
        let contents = match std::fs::read_to_string(&file_path) {
            Ok(c) => c,
            Err(_) => continue,
        };

        let header: ReportHeader = match serde_json::from_str(&contents) {
            Ok(h) => h,
            Err(_) => continue,
        };
        // Only count CharacterSheet reports
        if header.report.as_deref() != Some("CharacterSheet") {
            continue;
        }

        // Skip the game's placeholder extra report (??? name / error race)
        if is_placeholder_report(&header.character, header.race.as_deref()) {
            continue;
        }

        let key = (header.character.clone(), header.server_name.clone());
        let entry = char_map.entry(key).or_insert((0, None));
        entry.0 += 1;

        // Track the latest timestamp
        if let Some(ref ts) = header.timestamp {
            if entry.1.as_ref().map_or(true, |existing| ts > existing) {
                entry.1 = Some(ts.clone());
            }
        }
    }

    let mut result: Vec<DiscoveredCharacter> = char_map
        .into_iter()
        .map(|((name, server), (count, latest))| DiscoveredCharacter {
            character_name: name,
            server_name: server,
            report_count: count,
            latest_report_time: latest,
        })
        .collect();

    // Sort by latest report time descending, then by name
    result.sort_by(|a, b| {
        b.latest_report_time
            .cmp(&a.latest_report_time)
            .then_with(|| a.character_name.cmp(&b.character_name))
    });

    Ok(result)
}

#[tauri::command]
pub fn save_user_character(
    db: State<'_, DbPool>,
    character_name: String,
    server_name: String,
    source: String,
) -> Result<(), String> {
    let conn = db
        .get_write()
        .map_err(|e| format!("Database connection error: {e}"))?;

    conn.execute(
        "INSERT INTO user_characters (character_name, server_name, source)
         VALUES (?1, ?2, ?3)
         ON CONFLICT(character_name, server_name) DO UPDATE SET
            source = CASE
                WHEN excluded.source = 'report' THEN 'report'
                WHEN user_characters.source = 'report' THEN 'report'
                ELSE excluded.source
            END,
            updated_at = CURRENT_TIMESTAMP",
        rusqlite::params![character_name, server_name, source],
    )
    .map_err(|e| format!("Failed to save user character: {e}"))?;

    Ok(())
}

#[tauri::command]
pub fn get_user_characters(db: State<'_, DbPool>) -> Result<Vec<UserCharacter>, String> {
    let conn = db
        .get()
        .map_err(|e| format!("Database connection error: {e}"))?;

    let mut stmt = conn
        .prepare(
            "SELECT id, character_name, server_name, source, is_active,
                latest_report_time, last_login_time
         FROM user_characters
         ORDER BY is_active DESC, updated_at DESC",
        )
        .map_err(|e| format!("Failed to prepare query: {e}"))?;

    let rows = stmt
        .query_map([], |row| {
            Ok(UserCharacter {
                id: row.get(0)?,
                character_name: row.get(1)?,
                server_name: row.get(2)?,
                source: row.get(3)?,
                is_active: row.get(4)?,
                latest_report_time: row.get(5)?,
                last_login_time: row.get(6)?,
            })
        })
        .map_err(|e| format!("Query failed: {e}"))?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("Failed to read results: {e}"))
}

#[tauri::command]
pub fn set_active_character(
    db: State<'_, DbPool>,
    settings_manager: State<'_, Arc<SettingsManager>>,
    character_name: String,
    server_name: String,
) -> Result<(), String> {
    let mut conn = db
        .get_write()
        .map_err(|e| format!("Database connection error: {e}"))?;

    let tx = conn
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .map_err(|e| format!("Failed to begin transaction: {e}"))?;

    // Clear all active flags
    tx.execute("UPDATE user_characters SET is_active = 0", [])
        .map_err(|e| format!("Failed to clear active flags: {e}"))?;

    // Set the selected character as active
    tx.execute(
        "UPDATE user_characters SET is_active = 1, updated_at = CURRENT_TIMESTAMP
         WHERE character_name = ?1 AND server_name = ?2",
        rusqlite::params![character_name, server_name],
    )
    .map_err(|e| format!("Failed to set active character: {e}"))?;

    tx.commit()
        .map_err(|e| format!("Failed to commit transaction: {e}"))?;

    // Update settings
    let mut settings = settings_manager.get();
    settings.active_character_name = Some(character_name);
    settings.active_server_name = Some(server_name);
    settings_manager.update(settings)?;

    Ok(())
}

#[tauri::command]
pub fn complete_setup(settings_manager: State<'_, Arc<SettingsManager>>) -> Result<(), String> {
    let mut settings = settings_manager.get();
    settings.setup_completed = true;
    settings_manager.update(settings)
}

/// Delete a character and all their character-scoped data.
/// Cascades across all game_state_*, character_snapshots, inventory_snapshots, and related tables.
#[tauri::command]
pub fn delete_character(
    db: State<'_, DbPool>,
    settings_manager: State<'_, Arc<SettingsManager>>,
    character_name: String,
    server_name: String,
) -> Result<(), String> {
    let mut conn = db
        .get_write()
        .map_err(|e| format!("Database connection error: {e}"))?;

    let conn = conn
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .map_err(|e| format!("Failed to begin transaction: {e}"))?;
    // All character-scoped tables to cascade delete
    let tables = [
        "game_state_skills",
        "game_state_active_skills",
        "game_state_attributes",
        "game_state_combat",
        "game_state_mount",
        "game_state_inventory",
        "game_state_recipes",
        "game_state_equipment",
        "game_state_favor",
        "game_state_currencies",
        "game_state_effects",
        "game_state_session",
    ];

    for table in &tables {
        conn.execute(
            &format!("DELETE FROM {table} WHERE character_name = ?1 AND server_name = ?2"),
            rusqlite::params![character_name, server_name],
        )
        .map_err(|e| format!("Delete failed: {e}"))?;
    }

    // Delete character snapshots and their child data
    let snapshot_ids: Vec<i64> =
        {
            let mut stmt = conn.prepare(
            "SELECT id FROM character_snapshots WHERE character_name = ?1 AND server_name = ?2"
        ).map_err(|e| format!("Query error: {e}"))?;
            let ids: Vec<i64> = stmt
                .query_map(rusqlite::params![character_name, server_name], |row| {
                    row.get(0)
                })
                .map_err(|e| format!("Query error: {e}"))?
                .flatten()
                .collect();
            ids
        };

    for sid in &snapshot_ids {
        conn.execute(
            "DELETE FROM character_skill_levels WHERE snapshot_id = ?1",
            rusqlite::params![sid],
        )
        .map_err(|e| format!("Delete failed: {e}"))?;
        conn.execute(
            "DELETE FROM character_npc_favor WHERE snapshot_id = ?1",
            rusqlite::params![sid],
        )
        .map_err(|e| format!("Delete failed: {e}"))?;
        conn.execute(
            "DELETE FROM character_recipe_completions WHERE snapshot_id = ?1",
            rusqlite::params![sid],
        )
        .map_err(|e| format!("Delete failed: {e}"))?;
        conn.execute(
            "DELETE FROM character_stats WHERE snapshot_id = ?1",
            rusqlite::params![sid],
        )
        .map_err(|e| format!("Delete failed: {e}"))?;
        conn.execute(
            "DELETE FROM character_currencies WHERE snapshot_id = ?1",
            rusqlite::params![sid],
        )
        .map_err(|e| format!("Delete failed: {e}"))?;
    }
    conn.execute(
        "DELETE FROM character_snapshots WHERE character_name = ?1 AND server_name = ?2",
        rusqlite::params![character_name, server_name],
    )
    .map_err(|e| format!("Delete failed: {e}"))?;

    // Delete inventory snapshots and their child data
    let inv_snapshot_ids: Vec<i64> =
        {
            let mut stmt = conn.prepare(
            "SELECT id FROM inventory_snapshots WHERE character_name = ?1 AND server_name = ?2"
        ).map_err(|e| format!("Query error: {e}"))?;
            let ids: Vec<i64> = stmt
                .query_map(rusqlite::params![character_name, server_name], |row| {
                    row.get(0)
                })
                .map_err(|e| format!("Query error: {e}"))?
                .flatten()
                .collect();
            ids
        };

    for sid in &inv_snapshot_ids {
        conn.execute(
            "DELETE FROM inventory_snapshot_items WHERE snapshot_id = ?1",
            rusqlite::params![sid],
        )
        .map_err(|e| format!("Delete failed: {e}"))?;
    }
    conn.execute(
        "DELETE FROM inventory_snapshots WHERE character_name = ?1 AND server_name = ?2",
        rusqlite::params![character_name, server_name],
    )
    .map_err(|e| format!("Delete failed: {e}"))?;

    // Delete the user_characters record
    conn.execute(
        "DELETE FROM user_characters WHERE character_name = ?1 AND server_name = ?2",
        rusqlite::params![character_name, server_name],
    )
    .map_err(|e| format!("Delete failed: {e}"))?;

    conn.commit()
        .map_err(|e| format!("Failed to commit transaction: {e}"))?;

    // If the deleted character was the active one, clear it from settings
    let settings = settings_manager.get();
    if settings.active_character_name.as_deref() == Some(&character_name)
        && settings.active_server_name.as_deref() == Some(&server_name)
    {
        let mut updated = settings;
        updated.active_character_name = None;
        updated.active_server_name = None;
        settings_manager.update(updated).ok();
    }

    eprintln!("[setup] Deleted character {character_name} on {server_name}");
    Ok(())
}

/// Find the latest Character_*.json report file for a given character in the Reports folder
/// and import it if not already in the database.
/// When `server_name` is provided, only reports for that server are considered.
#[tauri::command]
pub fn import_latest_report_for_character(
    db: State<'_, DbPool>,
    cdn: State<'_, GameDataState>,
    settings_manager: State<'_, Arc<SettingsManager>>,
    character_name: String,
    server_name: Option<String>,
) -> Result<Option<crate::db::character_commands::ImportResult>, String> {
    let settings = settings_manager.get();
    let game_data_path = &settings.game_data_path;
    if game_data_path.is_empty() {
        return Err("Game data path not configured".into());
    }

    let reports_dir = Path::new(game_data_path).join("Reports");
    if !reports_dir.is_dir() {
        return Ok(None);
    }

    let entries = std::fs::read_dir(&reports_dir)
        .map_err(|e| format!("Failed to read Reports directory: {e}"))?;

    // Find all Character_*.json files for this character, pick the one with latest timestamp
    let mut best_file: Option<(String, std::path::PathBuf)> = None; // (timestamp, path)

    for entry in entries.flatten() {
        let file_name = entry.file_name().to_string_lossy().to_string();
        if !file_name.starts_with("Character_") || !file_name.ends_with(".json") {
            continue;
        }

        let file_path = entry.path();
        let contents = match std::fs::read_to_string(&file_path) {
            Ok(c) => c,
            Err(_) => continue,
        };

        let header: ReportHeader = match serde_json::from_str(&contents) {
            Ok(h) => h,
            Err(_) => continue,
        };

        if header.report.as_deref() != Some("CharacterSheet") {
            continue;
        }

        if is_placeholder_report(&header.character, header.race.as_deref()) {
            continue;
        }

        if header.character != character_name {
            continue;
        }

        // Filter by server when specified
        if let Some(ref wanted_server) = server_name {
            if &header.server_name != wanted_server {
                continue;
            }
        }

        if let Some(ref ts) = header.timestamp {
            if best_file
                .as_ref()
                .map_or(true, |(existing_ts, _)| ts > existing_ts)
            {
                best_file = Some((ts.clone(), file_path));
            }
        }
    }

    let Some((_, file_path)) = best_file else {
        return Ok(None);
    };

    let file_path_str = file_path.to_string_lossy().to_string();
    let data = cdn.blocking_read();
    let result = crate::db::character_commands::import_character_report_internal(
        &db,
        &file_path_str,
        &data,
    )?;

    // If it was a duplicate, return None (already imported)
    if result.was_duplicate {
        return Ok(None);
    }

    startup_log!("Imported character report for {}", character_name);
    Ok(Some(result))
}

/// Import the latest Character_*.json report for every character on a given server.
/// Scans the Reports directory once, groups by character, and imports each.
/// Skips duplicates (already-imported snapshots). Returns the count of new imports.
#[tauri::command]
pub fn import_reports_for_server(
    db: State<'_, DbPool>,
    cdn: State<'_, GameDataState>,
    settings_manager: State<'_, Arc<SettingsManager>>,
    server_name: String,
) -> Result<u32, String> {
    let settings = settings_manager.get();
    let game_data_path = &settings.game_data_path;
    if game_data_path.is_empty() {
        return Err("Game data path not configured".into());
    }

    let reports_dir = Path::new(game_data_path).join("Reports");
    if !reports_dir.is_dir() {
        return Ok(0);
    }

    let entries = std::fs::read_dir(&reports_dir)
        .map_err(|e| format!("Failed to read Reports directory: {e}"))?;

    // Scan all Character_*.json files for this server, keep the latest per character
    let mut best_per_character: std::collections::HashMap<String, (String, std::path::PathBuf)> =
        std::collections::HashMap::new();

    for entry in entries.flatten() {
        let file_name = entry.file_name().to_string_lossy().to_string();
        if !file_name.starts_with("Character_") || !file_name.ends_with(".json") {
            continue;
        }

        let file_path = entry.path();
        let contents = match std::fs::read_to_string(&file_path) {
            Ok(c) => c,
            Err(_) => continue,
        };

        let header: ReportHeader = match serde_json::from_str(&contents) {
            Ok(h) => h,
            Err(_) => continue,
        };
        if header.report.as_deref() != Some("CharacterSheet") {
            continue;
        }

        if is_placeholder_report(&header.character, header.race.as_deref()) {
            continue;
        }

        if header.server_name != server_name {
            continue;
        }

        if let Some(ref ts) = header.timestamp {
            let is_newer = best_per_character
                .get(&header.character)
                .map_or(true, |(existing_ts, _)| ts > existing_ts);
            if is_newer {
                best_per_character.insert(header.character.clone(), (ts.clone(), file_path));
            }
        }
    }

    // Import each character's latest report
    let data = cdn.blocking_read();
    let mut imported = 0u32;
    for (character, (_, file_path)) in &best_per_character {
        let file_path_str = file_path.to_string_lossy().to_string();
        match crate::db::character_commands::import_character_report_internal(
            &db,
            &file_path_str,
            &data,
        ) {
            Ok(result) if !result.was_duplicate => {
                startup_log!(
                    "Imported account character report: {} on {}",
                    character,
                    server_name
                );
                imported += 1;
            }
            Ok(_) => {} // duplicate, skip
            Err(e) => {
                startup_log!(
                    "Failed to import report for {} on {}: {e}",
                    character,
                    server_name
                );
            }
        }
    }

    if imported > 0 {
        startup_log!(
            "Account-wide import: {} new report(s) for {}",
            imported,
            server_name
        );
    }

    Ok(imported)
}

/// Find the latest *_items_*.json inventory report file for a given character
/// in the Reports folder and import it if not already in the database.
/// When `server_name` is provided, only reports for that server are considered.
#[tauri::command]
pub fn import_latest_inventory_for_character(
    db: State<'_, DbPool>,
    settings_manager: State<'_, Arc<SettingsManager>>,
    character_name: String,
    server_name: Option<String>,
) -> Result<Option<crate::db::inventory_commands::InventoryImportResult>, String> {
    let settings = settings_manager.get();
    let game_data_path = &settings.game_data_path;
    if game_data_path.is_empty() {
        return Err("Game data path not configured".into());
    }

    let reports_dir = Path::new(game_data_path).join("Reports");
    if !reports_dir.is_dir() {
        return Ok(None);
    }

    let entries = std::fs::read_dir(&reports_dir)
        .map_err(|e| format!("Failed to read Reports directory: {e}"))?;

    // Find all *_items_*.json files for this character, pick latest timestamp
    let mut best_file: Option<(String, std::path::PathBuf)> = None;

    for entry in entries.flatten() {
        let file_name = entry.file_name().to_string_lossy().to_string();
        if !file_name.ends_with(".json") || !file_name.contains("_items_") {
            continue;
        }

        let file_path = entry.path();
        let contents = match std::fs::read_to_string(&file_path) {
            Ok(c) => c,
            Err(_) => continue,
        };

        let header: ReportHeader = match serde_json::from_str(&contents) {
            Ok(h) => h,
            Err(_) => continue,
        };

        if header.report.as_deref() != Some("Storage") {
            continue;
        }

        if header.character != character_name {
            continue;
        }

        // Filter by server when specified
        if let Some(ref wanted_server) = server_name {
            if &header.server_name != wanted_server {
                continue;
            }
        }

        if let Some(ref ts) = header.timestamp {
            if best_file
                .as_ref()
                .map_or(true, |(existing_ts, _)| ts > existing_ts)
            {
                best_file = Some((ts.clone(), file_path));
            }
        }
    }

    let Some((_, file_path)) = best_file else {
        return Ok(None);
    };

    let file_path_str = file_path.to_string_lossy().to_string();
    let result =
        crate::db::inventory_commands::import_inventory_report_internal(&db, &file_path_str)?;

    if result.was_duplicate {
        return Ok(None);
    }

    startup_log!(
        "Imported inventory report for {} ({} items)",
        character_name,
        result.items_imported
    );
    Ok(Some(result))
}

/// Import the latest *_items_*.json (Storage) inventory report for every
/// character on a given server. Companion to `import_reports_for_server`, this
/// backfills inventory snapshots for characters the user hasn't recently loaded
/// so the cross-character storage search has data for all of them. Skips
/// duplicates. Returns the count of new imports.
#[tauri::command]
pub fn import_all_inventory_for_server(
    db: State<'_, DbPool>,
    settings_manager: State<'_, Arc<SettingsManager>>,
    server_name: String,
) -> Result<u32, String> {
    let settings = settings_manager.get();
    let game_data_path = &settings.game_data_path;
    if game_data_path.is_empty() {
        return Err("Game data path not configured".into());
    }

    let reports_dir = Path::new(game_data_path).join("Reports");
    if !reports_dir.is_dir() {
        return Ok(0);
    }

    let entries = std::fs::read_dir(&reports_dir)
        .map_err(|e| format!("Failed to read Reports directory: {e}"))?;

    // Scan all *_items_*.json Storage reports for this server, keep the latest
    // per character.
    let mut best_per_character: std::collections::HashMap<String, (String, std::path::PathBuf)> =
        std::collections::HashMap::new();

    for entry in entries.flatten() {
        let file_name = entry.file_name().to_string_lossy().to_string();
        if !file_name.ends_with(".json") || !file_name.contains("_items_") {
            continue;
        }

        let file_path = entry.path();
        let contents = match std::fs::read_to_string(&file_path) {
            Ok(c) => c,
            Err(_) => continue,
        };

        let header: ReportHeader = match serde_json::from_str(&contents) {
            Ok(h) => h,
            Err(_) => continue,
        };

        if header.report.as_deref() != Some("Storage") {
            continue;
        }

        if header.server_name != server_name {
            continue;
        }

        if let Some(ref ts) = header.timestamp {
            let is_newer = best_per_character
                .get(&header.character)
                .map_or(true, |(existing_ts, _)| ts > existing_ts);
            if is_newer {
                best_per_character.insert(header.character.clone(), (ts.clone(), file_path));
            }
        }
    }

    let mut imported = 0u32;
    for (character, (_, file_path)) in &best_per_character {
        let file_path_str = file_path.to_string_lossy().to_string();
        match crate::db::inventory_commands::import_inventory_report_internal(&db, &file_path_str) {
            Ok(result) if !result.was_duplicate => {
                startup_log!(
                    "Imported account inventory report: {} on {}",
                    character,
                    server_name
                );
                imported += 1;
            }
            Ok(_) => {} // duplicate, skip
            Err(e) => {
                startup_log!(
                    "Failed to import inventory for {} on {}: {e}",
                    character,
                    server_name
                );
            }
        }
    }

    if imported > 0 {
        startup_log!(
            "Account-wide inventory import: {} new report(s) for {}",
            imported,
            server_name
        );
    }

    Ok(imported)
}
