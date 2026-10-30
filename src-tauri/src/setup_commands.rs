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
use crate::settings::SettingsManager;

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
/// name, "???" name, or race "error". Filtered on the CharacterSheet scan
/// and import paths (scan_reports_for_characters,
/// import_reports_for_server); storage/inventory imports key on the
/// Storage header/struct instead and never see these rows.
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

    // Player.log is only detected next to the chosen game-data folder.
    // (A global/default-path hit is unrelated to THIS folder and must not
    // make a bad path look valid — previously ORed.)
    let player_log_found = base.join("Player.log").exists();

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
        if let Some(ts) = &header.timestamp {
            if entry.1.as_ref().is_none_or(|existing| ts > existing) {
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

/// Record a character in `user_characters` (startup picker state).
///
/// `source` is sticky toward `'report'`: a row is `'report'` once a VIP
/// report names the character (`SetupCharacterStep.vue` picks `"report"` vs
/// `"manual"`), and neither a later manual pick nor a later report import
/// can move it back (see the `ON CONFLICT` CASE). Consumers treat `'report'`
/// as "a real export exists"; `'manual'` means typed-in by hand.
#[tauri::command]
pub fn save_user_character(
    db: State<'_, DbPool>,
    character_name: String,
    server_name: String,
    source: String,
) -> Result<(), String> {
    // The schema only CHECKs `source`, not names — blank names would
    // otherwise insert empty-string rows and pollute identity lookups.
    // Reject them (and off-schema sources) here with the column named.
    if character_name.trim().is_empty() {
        return Err("character_name must not be empty".to_string());
    }
    if server_name.trim().is_empty() {
        return Err("server_name must not be empty".to_string());
    }
    if !["report", "manual", "login"].contains(&source.as_str()) {
        return Err("source must be one of 'report', 'manual', 'login'".to_string());
    }
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

    // Existence gate first: the clear-all below is destructive, so a typo'd
    // name must fail with the flags untouched rather than deactivating
    // every character and committing a no-op SET.
    let exists: bool = tx
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM user_characters WHERE character_name = ?1 AND server_name = ?2)",
            rusqlite::params![character_name, server_name],
            |row| row.get(0),
        )
        .map_err(|e| format!("Failed to look up character: {e}"))?;
    if !exists {
        drop(tx); // rollback: no rows touched yet, but be explicit
        return Err(format!(
            "Unknown character: {character_name} on {server_name}"
        ));
    }

    // Clear all active flags
    tx.execute("UPDATE user_characters SET is_active = 0", [])
        .map_err(|e| format!("Failed to clear active flags: {e}"))?;

    // Set the selected character as active. `exists` was just verified, so
    // rows == 1 is expected; treat anything else as a race and roll back
    // rather than committing a flag-less state or writing settings.
    let rows = tx
        .execute(
            "UPDATE user_characters SET is_active = 1, updated_at = CURRENT_TIMESTAMP
         WHERE character_name = ?1 AND server_name = ?2",
            rusqlite::params![character_name, server_name],
        )
        .map_err(|e| format!("Failed to set active character: {e}"))?;
    if rows != 1 {
        drop(tx); // rollback
        return Err(format!(
            "Failed to set active character: {character_name} on {server_name} (updated {rows} rows)"
        ));
    }

    tx.commit()
        .map_err(|e| format!("Failed to commit transaction: {e}"))?;

    // Update settings only after a committed, verified activation.
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
///
/// Deletes every row keyed by `(character_name, server_name)` — all
/// `game_state_*` tables, every log/almanac/tracker table scoped to a
/// character, the snapshot families (via their parent rows), and the
/// `user_characters` record itself. See the inline table lists for the exact
/// coverage, verified against `db/migrations.rs`.
///
/// Deliberately NOT deleted (not character-scoped):
/// - `game_state_weather`, `game_state_moon` — world-state singletons.
/// - `arena_bets`, `arena_matches` — no character columns.
/// - `stall_events` — keyed by `owner` (a stall label, not a character).
/// - `sales_history`, `event_log`, `chat_messages`, `hoplology_studies_new`
///   (legacy duplicate), `milking_timers_new`/`player_milking_log_new`
///   (migration temporaries), snapshot-child tables (deleted via parents).
/// - `character` (not `character_name`) in `brewing_discoveries` — that
///   column's shape is unverified, so it is left alone rather than guessed.
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

    delete_character_on_write_conn(&mut conn, &character_name, &server_name)?;

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

/// Transaction body of [`delete_character`], split out so tests can run it
/// against a plain `rusqlite::Connection` without a Tauri `State` wrapper.
fn delete_character_on_write_conn(
    conn: &mut rusqlite::Connection,
    character_name: &str,
    server_name: &str,
) -> Result<(), String> {
    let conn = conn
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .map_err(|e| format!("Failed to begin transaction: {e}"))?;

    // FK-safe order matters only for parent/child pairs deleted by id below;
    // every (character_name, server_name) table is independent, so plain
    // deletes are listed alphabetically within each group.
    //
    // Group 1: game_state_* tables with a (character_name, server_name) scope.
    // Verified against db/migrations.rs; game_state_weather/moon are world
    // singletons and are intentionally absent.
    let game_state_tables = [
        "game_state_active_skills",
        "game_state_area",
        "game_state_attributes",
        "game_state_books",
        "game_state_combat",
        "game_state_currencies",
        "game_state_directed_goals",
        "game_state_effects",
        "game_state_equipment",
        "game_state_favor",
        "game_state_gift_log",
        "game_state_guild",
        "game_state_inventory",
        "game_state_mount",
        "game_state_npc_vendor",
        "game_state_recipes",
        "game_state_session",
        "game_state_skills",
        "game_state_storage",
        "game_state_strings",
        "game_state_teleportation",
    ];

    for table in &game_state_tables {
        conn.execute(
            &format!("DELETE FROM {table} WHERE character_name = ?1 AND server_name = ?2"),
            rusqlite::params![character_name, server_name],
        )
        .map_err(|e| format!("Delete failed: {e}"))?;
    }

    // Group 2: other (character_name, server_name)-scoped tables, verified
    // against db/migrations.rs. enemy_kills/corpse_extracts use nullable
    // character columns but still carry the character scope, so the same
    // predicate applies.
    //
    // FK-safe order: open_multihit_nodes references survey_uses (CASCADE),
    // so it goes first; survey_uses (SET NULL to survey_sessions) goes
    // before survey_sessions.
    let scoped_tables = [
        "open_multihit_nodes",
        "character_deaths",
        "character_item_snapshots",
        "character_report_stats",
        "character_resuscitations",
        "corpse_extracts",
        "currency_estimate",
        "enemy_kills",
        "garden_almanac",
        "garden_almanac_history",
        "harvest_almanac",
        "hoplology_studies",
        "item_transactions",
        "milking_timers",
        "player_messages",
        "player_milking_log",
        "player_pooping_log",
        "stall_price_observations",
        "survey_uses",
        "survey_sessions",
        "tracked_skills",
        "user_timers",
        "words_of_power",
    ];

    for table in &scoped_tables {
        conn.execute(
            &format!("DELETE FROM {table} WHERE character_name = ?1 AND server_name = ?2"),
            rusqlite::params![character_name, server_name],
        )
        .map_err(|e| format!("Delete failed: {e}"))?;
    }

    // Group 3: child rows deleted via their parent ids (FK order: children
    // before parents). character_snapshots children all CASCADE, but explicit
    // deletes keep the cascade working even where FKs are off, matching the
    // previous behavior. survey_uses → survey_sessions is SET NULL, and
    // open_multihit_nodes was already cleared by scope above, so deleting
    // survey_sessions after survey_uses is safe either way.
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
        conn.execute(
            "DELETE FROM character_active_quests WHERE snapshot_id = ?1",
            rusqlite::params![sid],
        )
        .map_err(|e| format!("Delete failed: {e}"))?;
        conn.execute(
            "DELETE FROM snapshot_skill_abilities WHERE snapshot_id = ?1",
            rusqlite::params![sid],
        )
        .map_err(|e| format!("Delete failed: {e}"))?;
    }
    conn.execute(
        "DELETE FROM character_snapshots WHERE character_name = ?1 AND server_name = ?2",
        rusqlite::params![character_name, server_name],
    )
    .map_err(|e| format!("Delete failed: {e}"))?;

    // character_item_snapshots children (character_snapshot_items).
    let item_snapshot_ids: Vec<i64> = {
        let mut stmt = conn
            .prepare(
                "SELECT id FROM character_item_snapshots WHERE character_name = ?1 AND server_name = ?2",
            )
            .map_err(|e| format!("Query error: {e}"))?;
        let rows = stmt.query_map(rusqlite::params![character_name, server_name], |row| {
            row.get(0)
        })
        .map_err(|e| format!("Query error: {e}"))?;
        rows.flatten().collect()
    };
    for sid in &item_snapshot_ids {
        conn.execute(
            "DELETE FROM character_snapshot_items WHERE item_snapshot_id = ?1",
            rusqlite::params![sid],
        )
        .map_err(|e| format!("Delete failed: {e}"))?;
    }
    // Already deleted by scope in Group 2; kept as a no-op-safe second pass
    // so the parent rows are gone even if the scoped delete above is edited.
    conn.execute(
        "DELETE FROM character_item_snapshots WHERE character_name = ?1 AND server_name = ?2",
        rusqlite::params![character_name, server_name],
    )
    .map_err(|e| format!("Delete failed: {e}"))?;

    // character_deaths children (death_damage_sources) and enemy_kills
    // children (enemy_kill_loot): deleted by parent id before the parents
    // were removed in Group 2... except Group 2 already removed the parents.
    // Re-resolve: both child tables CASCADE on FK, and FKs are enforced on
    // the pool, so orphans cannot survive. Belt-and-braces: delete any child
    // rows whose parent id no longer exists.
    conn.execute(
        "DELETE FROM death_damage_sources WHERE death_id NOT IN (SELECT id FROM character_deaths)",
        [],
    )
    .map_err(|e| format!("Delete failed: {e}"))?;
    conn.execute(
        "DELETE FROM enemy_kill_loot WHERE kill_id NOT IN (SELECT id FROM enemy_kills)",
        [],
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

        if let Some(ts) = &header.timestamp {
            if best_file
                .as_ref()
                .is_none_or(|(existing_ts, _)| ts > existing_ts)
            {
                best_file = Some((ts.clone(), file_path));
            }
        }
    }

    let Some((_, file_path)) = best_file else {
        return Ok(None);
    };

    let file_path_str = file_path.to_string_lossy().to_string();
    let result = crate::db::character_commands::import_character_report_internal(
        &db,
        &file_path_str,
        &cdn,
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

        if let Some(ts) = &header.timestamp {
            let is_newer = best_per_character
                .get(&header.character)
                .is_none_or(|(existing_ts, _)| ts > existing_ts);
            if is_newer {
                best_per_character.insert(header.character.clone(), (ts.clone(), file_path));
            }
        }
    }

    // Import each character's latest report
    let mut imported = 0u32;
    for (character, (_, file_path)) in &best_per_character {
        let file_path_str = file_path.to_string_lossy().to_string();
        match crate::db::character_commands::import_character_report_internal(
            &db,
            &file_path_str,
            &cdn,
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

        if let Some(ts) = &header.timestamp {
            if best_file
                .as_ref()
                .is_none_or(|(existing_ts, _)| ts > existing_ts)
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

        if let Some(ts) = &header.timestamp {
            let is_newer = best_per_character
                .get(&header.character)
                .is_none_or(|(existing_ts, _)| ts > existing_ts);
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::migrations::run_migrations;

    const CHAR: &str = "DeleteMe";
    const SERVER: &str = "TestServer";
    const OTHER: &str = "KeepMe";

    fn fresh_db() -> rusqlite::Connection {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys=ON;").unwrap();
        run_migrations(&conn, None).unwrap();
        conn
    }

    fn count(conn: &rusqlite::Connection, table: &str, who: &str) -> i64 {
        conn.query_row(
            &format!("SELECT COUNT(*) FROM {table} WHERE character_name = ?1 AND server_name = ?2"),
            rusqlite::params![who, SERVER],
            |row| row.get(0),
        )
        .unwrap()
    }

    /// Seed one row per character-scoped table for `who` (plus a snapshot
    /// family, a death/kill family, and a survey session/use/node chain).
    /// Mirrors the NOT NULL shapes in `db/migrations.rs`.
    fn seed_character(conn: &rusqlite::Connection, who: &str) {
        let ts = "2026-01-01 00:00:00";
        // user_characters parent row.
        conn.execute(
            "INSERT INTO user_characters (character_name, server_name, source) VALUES (?1, ?2, 'manual')",
            rusqlite::params![who, SERVER],
        )
        .unwrap();
        // Group 1: game_state_*.
        let gs: &[&str] = &[
            "INSERT INTO game_state_skills (character_name,server_name,skill_id,skill_name,level,last_confirmed_at) VALUES (?1,?2,1,'Sword',1,'2026-01-01')",
            "INSERT INTO game_state_active_skills (character_name,server_name,skill1_id,skill1_name,skill2_id,skill2_name,last_confirmed_at) VALUES (?1,?2,1,'A',2,'B','2026-01-01')",
            "INSERT INTO game_state_attributes (character_name,server_name,attribute_name,value,last_confirmed_at,min_value,max_value) VALUES (?1,?2,'Health',100,'2026-01-01',100,100)",
            "INSERT INTO game_state_combat (character_name,server_name,in_combat,last_confirmed_at) VALUES (?1,?2,0,'2026-01-01')",
            "INSERT INTO game_state_mount (character_name,server_name,is_mounted,last_confirmed_at) VALUES (?1,?2,0,'2026-01-01')",
            "INSERT INTO game_state_inventory (character_name,server_name,instance_id,item_name,item_type_id,stack_size,slot_index,last_confirmed_at,source) VALUES (?1,?2,7,'Sword',1,1,0,'2026-01-01','log')",
            "INSERT INTO game_state_recipes (character_name,server_name,recipe_id,completion_count,last_confirmed_at,source) VALUES (?1,?2,9,1,'2026-01-01','log')",
            "INSERT INTO game_state_equipment (character_name,server_name,slot,appearance_key,last_confirmed_at) VALUES (?1,?2,'MainHand','sword_key','2026-01-01')",
            "INSERT INTO game_state_favor (character_name,server_name,npc_key,npc_name,cumulative_delta,favor_tier,last_confirmed_at,source) VALUES (?1,?2,'npc','Npc',1.0,NULL,'2026-01-01','log')",
            "INSERT INTO game_state_currencies (character_name,server_name,currency_name,amount,last_confirmed_at,source) VALUES (?1,?2,'Councils',5,'2026-01-01','snapshot')",
            "INSERT INTO game_state_effects (character_name,server_name,effect_instance_id,effect_name,source_entity_id,last_confirmed_at) VALUES (?1,?2,3,'Buff',0,'2026-01-01')",
            "INSERT INTO game_state_storage (character_name,server_name,vault_key,instance_id,item_name,item_type_id,stack_size,slot_index,last_confirmed_at,source) VALUES (?1,?2,'vault',7,'Sword',1,1,0,'2026-01-01','log')",
            "INSERT INTO game_state_gift_log (character_name,server_name,npc_key,npc_name,gifted_at,favor_delta) VALUES (?1,?2,'npc','Npc','2026-01-01',1.0)",
            "INSERT INTO game_state_npc_vendor VALUES (?1,?2,'npc',NULL,NULL,NULL,NULL,NULL,'2026-01-01',0,0)",
            "INSERT INTO game_state_area (character_name,server_name,area_name,last_confirmed_at) VALUES (?1,?2,'Serbule','2026-01-01')",
            "INSERT INTO game_state_guild VALUES (?1,?2,1,'Guild','', '2026-01-01')",
            "INSERT INTO game_state_directed_goals VALUES (?1,?2,1,'2026-01-01')",
            "INSERT INTO game_state_strings VALUES (?1,?2,'k','v','2026-01-01')",
            "INSERT INTO game_state_books (character_name,server_name,book_type,title,content,captured_at) VALUES (?1,?2,'SkillReport','T','c','2026-01-01')",
            "INSERT INTO game_state_teleportation VALUES (?1,?2,NULL,NULL,NULL,NULL,'2026-01-01')",
        ];
        // game_state_session singleton row (id = 1, so only the first seed
        // can insert; the second seed updates it — then the delete test only
        // asserts the deleted character's scope, not the singleton).
        for stmt in gs {
            conn.execute(stmt, rusqlite::params![who, SERVER]).unwrap();
        }
        // Group 2: other scoped tables.
        conn.execute(
            "INSERT INTO character_deaths (character_name,server_name,died_at,killer_name,killing_ability,health_damage) VALUES (?1,?2,'2026-01-01','Rat','Bite',5)",
            rusqlite::params![who, SERVER],
        )
        .unwrap();
        let death_id = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO death_damage_sources (death_id,event_order,timestamp,attacker_name,ability_name) VALUES (?1,0,'2026-01-01','Rat','Bite')",
            rusqlite::params![death_id],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO character_item_snapshots (character_name,server_name,snapshot_timestamp,raw_json) VALUES (?1,?2,'2026-01-01','{}')",
            rusqlite::params![who, SERVER],
        )
        .unwrap();
        let item_snap = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO character_snapshot_items (item_snapshot_id,type_id,stack_size,item_name) VALUES (?1,1,1,'Sword')",
            rusqlite::params![item_snap],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO character_report_stats VALUES (?1,?2,'cat','stat','1','2026-01-01')",
            rusqlite::params![who, SERVER],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO character_resuscitations (character_name,server_name,occurred_at,caster_name,target_name) VALUES (?1,?2,'2026-01-01','A','B')",
            rusqlite::params![who, SERVER],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO corpse_extracts (character_name,server_name,item_name,skill) VALUES (?1,?2,'Bone','Anatomy')",
            rusqlite::params![who, SERVER],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO currency_estimate (character_name,server_name) VALUES (?1,?2)",
            rusqlite::params![who, SERVER],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO enemy_kills (enemy_name,enemy_entity_id,killing_ability,killed_at,character_name,server_name) VALUES ('Rat','e1','Sword','2026-01-01',?1,?2)",
            rusqlite::params![who, SERVER],
        )
        .unwrap();
        let kill_id = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO enemy_kill_loot (kill_id,item_name) VALUES (?1,'Bone')",
            rusqlite::params![kill_id],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO garden_almanac VALUES (?1,?2,'Wheat','Serbule',NULL,NULL,0,'2026-01-01')",
            rusqlite::params![who, SERVER],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO garden_almanac_history (character_name,server_name,crop_name,zone_name,captured_at) VALUES (?1,?2,'Wheat','Serbule','2026-01-01')",
            rusqlite::params![who, SERVER],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO harvest_almanac (character_name,server_name,monster_name,captured_at) VALUES (?1,?2,'Rat','2026-01-01')",
            rusqlite::params![who, SERVER],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO hoplology_studies (character_name,server_name,item_name,studied_at) VALUES (?1,?2,'Bottle','2026-01-01')",
            rusqlite::params![who, SERVER],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO item_transactions (timestamp,character_name,server_name,item_name,quantity,context,source) VALUES ('2026-01-01',?1,?2,'Sword',1,'loot','log')",
            rusqlite::params![who, SERVER],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO milking_timers (character_name,server_name,cow_name,zone,last_milked_at) VALUES (?1,?2,'Bessy','Serbule','2026-01-01')",
            rusqlite::params![who, SERVER],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO player_messages (character_name,server_name,timestamp,message_type,direction,other_player) VALUES (?1,?2,'2026-01-01','tell','in','Bob')",
            rusqlite::params![who, SERVER],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO player_milking_log (character_name,server_name,other_player,direction,milked_at) VALUES (?1,?2,'Bob','milked','2026-01-01')",
            rusqlite::params![who, SERVER],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO player_pooping_log (character_name,server_name,target_name,pooped_at) VALUES (?1,?2,'Bob','2026-01-01')",
            rusqlite::params![who, SERVER],
        )
        .unwrap();
        // UNIQUE(observed_at, item_name, stall_npc_entity_id, price_unit) has
        // no character column, so vary observed_at per character.
        let observed_at = format!("2026-01-01 00:00:{:02}", if who == CHAR { 1 } else { 2 });
        conn.execute(
            "INSERT INTO stall_price_observations (character_name,server_name,item_name,price_unit,stall_npc_entity_id,stall_label,source,observed_at) VALUES (?1,?2,'Sword',5,1,'Stall','auto',?3)",
            rusqlite::params![who, SERVER, observed_at],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO survey_sessions (character_name,server_name,started_at,start_trigger) VALUES (?1,?2,'2026-01-01','manual')",
            rusqlite::params![who, SERVER],
        )
        .unwrap();
        let session_id = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO survey_uses (session_id,character_name,server_name,used_at,map_internal_name,map_display_name,kind) VALUES (?1,?2,?3,'2026-01-01','Map','Map','multihit')",
            rusqlite::params![session_id, who, SERVER],
        )
        .unwrap();
        let use_id = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO open_multihit_nodes (node_entity_id,character_name,server_name,survey_use_id,opened_at,last_hit_at) VALUES (99,?1,?2,?3,'2026-01-01','2026-01-01')",
            rusqlite::params![who, SERVER, use_id],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO tracked_skills (character_name,server_name,skill_name,sort_order) VALUES (?1,?2,'Sword',0)",
            rusqlite::params![who, SERVER],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO user_timers (character_name,server_name,label,duration_seconds,started_at) VALUES (?1,?2,'T',60,'2026-01-01')",
            rusqlite::params![who, SERVER],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO words_of_power (character_name,server_name,word,power_name,discovered_at) VALUES (?1,?2,'W','P','2026-01-01')",
            rusqlite::params![who, SERVER],
        )
        .unwrap();
        // Snapshot family + children.
        conn.execute(
            "INSERT INTO character_snapshots (character_name,server_name,snapshot_timestamp,raw_json) VALUES (?1,?2,'2026-01-01','{}')",
            rusqlite::params![who, SERVER],
        )
        .unwrap();
        let snap = conn.last_insert_rowid();
        for stmt in [
            "INSERT INTO character_skill_levels (snapshot_id,skill_name,level) VALUES (?1,'Sword',1)",
            "INSERT INTO character_npc_favor (snapshot_id,npc_key,favor_level) VALUES (?1,'npc','Friends')",
            "INSERT INTO character_recipe_completions (snapshot_id,recipe_key,completions) VALUES (?1,'r',1)",
            "INSERT INTO character_stats (snapshot_id,stat_key,value) VALUES (?1,'s',1.0)",
            "INSERT INTO character_currencies (snapshot_id,currency_key,amount) VALUES (?1,'c',1)",
            "INSERT INTO character_active_quests (snapshot_id,quest_key) VALUES (?1,'q')",
            "INSERT INTO snapshot_skill_abilities (snapshot_id,skill_name,ability_key) VALUES (?1,'Sword','a')",
        ] {
            conn.execute(stmt, rusqlite::params![snap]).unwrap();
        }
        let _ = ts;
    }

    const SCOPED_TABLES: &[&str] = &[
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
        "game_state_storage",
        "game_state_gift_log",
        "game_state_npc_vendor",
        "game_state_area",
        "game_state_guild",
        "game_state_directed_goals",
        "game_state_strings",
        "game_state_books",
        "game_state_teleportation",
        "character_deaths",
        "character_item_snapshots",
        "character_report_stats",
        "character_resuscitations",
        "corpse_extracts",
        "currency_estimate",
        "enemy_kills",
        "garden_almanac",
        "garden_almanac_history",
        "harvest_almanac",
        "hoplology_studies",
        "item_transactions",
        "milking_timers",
        "open_multihit_nodes",
        "player_messages",
        "player_milking_log",
        "player_pooping_log",
        "stall_price_observations",
        "survey_sessions",
        "survey_uses",
        "tracked_skills",
        "user_timers",
        "words_of_power",
        "character_snapshots",
        "user_characters",
    ];

    #[test]
    fn delete_character_removes_every_scoped_row_but_keeps_other_character() {
        let mut conn = fresh_db();
        seed_character(&conn, CHAR);
        seed_character(&conn, OTHER);
        // game_state_session is a singleton (id = 1): point it at CHAR so the
        // delete visibly clears it, then re-point at OTHER afterwards.
        conn.execute(
            "INSERT OR REPLACE INTO game_state_session (id,character_name,server_name) VALUES (1,?1,?2)",
            rusqlite::params![CHAR, SERVER],
        )
        .unwrap();

        delete_character_on_write_conn(&mut conn, CHAR, SERVER).unwrap();

        for table in SCOPED_TABLES {
            assert_eq!(count(&conn, table, CHAR), 0, "table {table} still has {CHAR} rows");
            assert_eq!(count(&conn, table, OTHER), 1, "table {table} lost {OTHER} rows");
        }
        // Singleton cleared with the deleted character.
        let session_left: i64 = conn
            .query_row("SELECT COUNT(*) FROM game_state_session", [], |row| row.get(0))
            .unwrap();
        assert_eq!(session_left, 0);
        // Child tables cascade / are explicitly cleared: only OTHER's single
        // row per table survives.
        for table in [
            "character_skill_levels",
            "character_npc_favor",
            "character_recipe_completions",
            "character_stats",
            "character_currencies",
            "character_active_quests",
            "snapshot_skill_abilities",
            "character_snapshot_items",
            "death_damage_sources",
            "enemy_kill_loot",
        ] {
            let total: i64 = conn
                .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| row.get(0))
                .unwrap();
            assert_eq!(total, 1, "child table {table} should keep only {OTHER}'s row");
        }
    }
}
