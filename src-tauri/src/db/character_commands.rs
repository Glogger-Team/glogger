use super::DbPool;
use crate::cdn_commands::GameDataState;
use crate::game_data::GameData;
use serde::{Deserialize, Serialize};
/// Tauri commands for character report import and querying
use std::collections::HashMap;
use tauri::State;

// ── JSON Deserialization Structs (match game's /outputcharacter format) ───────

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct CharacterReport {
    pub character: String,
    pub server_name: String,
    pub timestamp: String,
    pub report: String,
    #[allow(dead_code)]
    pub report_version: u32,
    pub race: String,
    pub skills: HashMap<String, SkillData>,
    pub recipe_completions: HashMap<String, i64>,
    pub current_stats: HashMap<String, f64>,
    pub currencies: HashMap<String, i64>,
    #[serde(rename = "NPCs")]
    pub npcs: HashMap<String, NpcFavorData>,
    #[serde(default)]
    pub active_quests: Vec<String>,
    #[serde(default)]
    pub active_work_orders: Vec<String>,
    #[serde(default)]
    pub completed_work_orders: Vec<String>,
    /// Completed (non-work-order) quests. Only present in newer VIP exports;
    /// older exports omit the field entirely, hence `default`.
    #[serde(default)]
    pub completed_quests: Vec<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct SkillData {
    pub level: i32,
    pub bonus_levels: i32,
    pub xp_toward_next_level: i64,
    pub xp_needed_for_next_level: i64,
    #[allow(dead_code)]
    pub abilities: Option<Vec<String>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct NpcFavorData {
    pub favor_level: String,
}

// ── Command Response Types ───────────────────────────────────────────────────

#[derive(Serialize)]
pub struct ImportResult {
    pub character_name: String,
    pub server_name: String,
    pub snapshot_timestamp: String,
    pub skills_imported: usize,
    pub npcs_imported: usize,
    pub recipes_imported: usize,
    pub stats_imported: usize,
    pub currencies_imported: usize,
    pub quests_imported: usize,
    pub was_duplicate: bool,
}

#[derive(Serialize)]
pub struct CharacterSnapshotSummary {
    pub id: i64,
    pub character_name: String,
    pub server_name: String,
    pub snapshot_timestamp: String,
    pub race: String,
    pub import_date: String,
    pub skill_count: usize,
}

#[derive(Serialize)]
pub struct SnapshotSkillLevel {
    pub skill_name: String,
    pub level: i32,
    pub bonus_levels: i32,
    pub xp_toward_next: i64,
    pub xp_needed_for_next: i64,
}

#[derive(Serialize)]
pub struct SnapshotNpcFavor {
    pub npc_key: String,
    pub favor_level: String,
}

#[derive(Serialize)]
pub struct SnapshotRecipeCompletion {
    pub recipe_key: String,
    pub completions: i64,
}

#[derive(Serialize)]
pub struct SnapshotStat {
    pub stat_key: String,
    pub value: f64,
}

#[derive(Serialize)]
pub struct SnapshotCurrency {
    pub currency_key: String,
    pub amount: i64,
}

#[derive(Serialize)]
pub struct SnapshotActiveQuest {
    pub quest_key: String,
    pub category: String,
}

#[derive(Serialize)]
pub struct CharacterInfo {
    pub character_name: String,
    pub server_name: String,
    pub latest_snapshot: String,
    pub snapshot_count: usize,
}

#[derive(Serialize)]
pub struct SkillDiff {
    pub skill_name: String,
    pub old_level: i32,
    pub new_level: i32,
    pub level_change: i32,
    pub old_xp: i64,
    pub new_xp: i64,
}

// ── Commands ─────────────────────────────────────────────────────────────────

#[tauri::command]
pub fn import_character_report(
    db: State<'_, DbPool>,
    cdn: State<'_, GameDataState>,
    file_path: String,
) -> Result<ImportResult, String> {
    let data = cdn.blocking_read();
    import_character_report_internal(&db, &file_path, &data)
}

/// Internal import logic, callable without Tauri State wrapper
pub fn import_character_report_internal(
    db: &DbPool,
    file_path: &str,
    game_data: &GameData,
) -> Result<ImportResult, String> {
    // 1. Read file
    let raw_json =
        std::fs::read_to_string(file_path).map_err(|e| format!("Failed to read file: {e}"))?;

    // 2. Deserialize
    let report: CharacterReport = serde_json::from_str(&raw_json)
        .map_err(|e| format!("Failed to parse character report: {e}"))?;

    // 3. Validate report type
    if report.report != "CharacterSheet" {
        return Err(format!(
            "Wrong report type: expected \"CharacterSheet\", got \"{}\"",
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

    let result = (|| -> Result<ImportResult, String> {
        // 5. Insert snapshot (skip duplicates)
        conn.execute(
            "INSERT INTO character_snapshots (character_name, server_name, snapshot_timestamp, race, raw_json)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(character_name, server_name, snapshot_timestamp) DO NOTHING",
            rusqlite::params![
                report.character,
                report.server_name,
                report.timestamp,
                report.race,
                raw_json,
            ],
        ).map_err(|e| format!("Failed to insert snapshot: {e}"))?;

        // 6. Check if insert created a new row
        let changes = conn.changes();
        if changes == 0 {
            return Ok(ImportResult {
                character_name: report.character.clone(),
                server_name: report.server_name.clone(),
                snapshot_timestamp: report.timestamp.clone(),
                skills_imported: 0,
                npcs_imported: 0,
                recipes_imported: 0,
                stats_imported: 0,
                currencies_imported: 0,
                quests_imported: 0,
                was_duplicate: true,
            });
        }

        let snapshot_id = conn.last_insert_rowid();

        // 7. Batch insert skill levels
        let mut skill_stmt = conn.prepare(
            "INSERT INTO character_skill_levels (snapshot_id, skill_name, level, bonus_levels, xp_toward_next, xp_needed_for_next)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)"
        ).map_err(|e| format!("Failed to prepare skill insert: {e}"))?;

        for (skill_name, skill_data) in &report.skills {
            let base_level = skill_data.level - skill_data.bonus_levels;
            skill_stmt
                .execute(rusqlite::params![
                    snapshot_id,
                    skill_name,
                    base_level,
                    skill_data.bonus_levels,
                    skill_data.xp_toward_next_level,
                    skill_data.xp_needed_for_next_level,
                ])
                .map_err(|e| format!("Failed to insert skill {skill_name}: {e}"))?;
        }

        // 8. Batch insert NPC favor
        let mut npc_stmt = conn
            .prepare(
                "INSERT INTO character_npc_favor (snapshot_id, npc_key, favor_level)
             VALUES (?1, ?2, ?3)",
            )
            .map_err(|e| format!("Failed to prepare NPC insert: {e}"))?;

        for (npc_key, npc_data) in &report.npcs {
            npc_stmt
                .execute(rusqlite::params![
                    snapshot_id,
                    npc_key,
                    npc_data.favor_level,
                ])
                .map_err(|e| format!("Failed to insert NPC {npc_key}: {e}"))?;
        }

        // 9. Batch insert recipe completions
        let mut recipe_stmt = conn
            .prepare(
                "INSERT INTO character_recipe_completions (snapshot_id, recipe_key, completions)
             VALUES (?1, ?2, ?3)",
            )
            .map_err(|e| format!("Failed to prepare recipe insert: {e}"))?;

        for (recipe_key, completions) in &report.recipe_completions {
            recipe_stmt
                .execute(rusqlite::params![snapshot_id, recipe_key, completions,])
                .map_err(|e| format!("Failed to insert recipe {recipe_key}: {e}"))?;
        }

        // 10. Batch insert stats
        let mut stat_stmt = conn
            .prepare(
                "INSERT INTO character_stats (snapshot_id, stat_key, value)
             VALUES (?1, ?2, ?3)",
            )
            .map_err(|e| format!("Failed to prepare stat insert: {e}"))?;

        for (stat_key, value) in &report.current_stats {
            stat_stmt
                .execute(rusqlite::params![snapshot_id, stat_key, value,])
                .map_err(|e| format!("Failed to insert stat {stat_key}: {e}"))?;
        }

        // 11. Batch insert currencies
        let mut currency_stmt = conn
            .prepare(
                "INSERT INTO character_currencies (snapshot_id, currency_key, amount)
             VALUES (?1, ?2, ?3)",
            )
            .map_err(|e| format!("Failed to prepare currency insert: {e}"))?;

        for (currency_key, amount) in &report.currencies {
            currency_stmt
                .execute(rusqlite::params![snapshot_id, currency_key, amount,])
                .map_err(|e| format!("Failed to insert currency {currency_key}: {e}"))?;
        }

        // 12. Batch insert active quests
        let mut quest_stmt = conn
            .prepare(
                "INSERT INTO character_active_quests (snapshot_id, quest_key, category)
             VALUES (?1, ?2, ?3)",
            )
            .map_err(|e| format!("Failed to prepare quest insert: {e}"))?;

        for quest_key in &report.active_quests {
            quest_stmt
                .execute(rusqlite::params![snapshot_id, quest_key, "active"])
                .map_err(|e| format!("Failed to insert active quest {quest_key}: {e}"))?;
        }
        for quest_key in &report.active_work_orders {
            quest_stmt
                .execute(rusqlite::params![snapshot_id, quest_key, "work_order"])
                .map_err(|e| format!("Failed to insert work order {quest_key}: {e}"))?;
        }
        for quest_key in &report.completed_work_orders {
            quest_stmt
                .execute(rusqlite::params![
                    snapshot_id,
                    quest_key,
                    "completed_work_order"
                ])
                .map_err(|e| format!("Failed to insert completed work order {quest_key}: {e}"))?;
        }
        for quest_key in &report.completed_quests {
            quest_stmt
                .execute(rusqlite::params![snapshot_id, quest_key, "completed"])
                .map_err(|e| format!("Failed to insert completed quest {quest_key}: {e}"))?;
        }
        let quests_imported = report.active_quests.len()
            + report.active_work_orders.len()
            + report.completed_work_orders.len()
            + report.completed_quests.len();

        // 13. Seed game state from snapshot (timestamp deconfliction: only overwrite if newer)
        seed_game_state_from_snapshot(&conn, &report, game_data)?;

        Ok(ImportResult {
            character_name: report.character.clone(),
            server_name: report.server_name.clone(),
            snapshot_timestamp: report.timestamp.clone(),
            skills_imported: report.skills.len(),
            npcs_imported: report.npcs.len(),
            recipes_imported: report.recipe_completions.len(),
            stats_imported: report.current_stats.len(),
            currencies_imported: report.currencies.len(),
            quests_imported,
            was_duplicate: false,
        })
    })();

    // 12. Commit (a dropped `Transaction` rolls back on any earlier `?`)
    let value = result?;
    conn.commit()
        .map_err(|e| format!("Failed to commit transaction: {e}"))?;

    Ok(value)
}

#[tauri::command]
pub fn get_characters(db: State<'_, DbPool>) -> Result<Vec<CharacterInfo>, String> {
    let conn = db
        .get()
        .map_err(|e| format!("Database connection error: {e}"))?;

    let mut stmt = conn
        .prepare(
            "SELECT character_name, server_name,
                MAX(snapshot_timestamp) as latest_snapshot,
                COUNT(*) as snapshot_count
         FROM character_snapshots
         GROUP BY character_name, server_name
         ORDER BY latest_snapshot DESC",
        )
        .map_err(|e| format!("Failed to prepare query: {e}"))?;

    let rows = stmt
        .query_map([], |row| {
            Ok(CharacterInfo {
                character_name: row.get(0)?,
                server_name: row.get(1)?,
                latest_snapshot: row.get(2)?,
                snapshot_count: row.get::<_, i64>(3)? as usize,
            })
        })
        .map_err(|e| format!("Query failed: {e}"))?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("Failed to read results: {e}"))
}

#[tauri::command]
pub fn get_character_snapshots(
    db: State<'_, DbPool>,
    character_name: String,
    server_name: Option<String>,
) -> Result<Vec<CharacterSnapshotSummary>, String> {
    let conn = db
        .get()
        .map_err(|e| format!("Database connection error: {e}"))?;

    let (sql, params): (&str, Vec<Box<dyn rusqlite::types::ToSql>>) = if let Some(ref server) =
        server_name
    {
        (
            "SELECT cs.id, cs.character_name, cs.server_name, cs.snapshot_timestamp,
                    cs.race, datetime(cs.import_date) as import_date,
                    (SELECT COUNT(*) FROM character_skill_levels WHERE snapshot_id = cs.id) as skill_count
             FROM character_snapshots cs
             WHERE cs.character_name = ?1 AND cs.server_name = ?2
             ORDER BY cs.snapshot_timestamp DESC",
            vec![
                Box::new(character_name.clone()) as Box<dyn rusqlite::types::ToSql>,
                Box::new(server.clone()),
            ],
        )
    } else {
        (
            "SELECT cs.id, cs.character_name, cs.server_name, cs.snapshot_timestamp,
                    cs.race, datetime(cs.import_date) as import_date,
                    (SELECT COUNT(*) FROM character_skill_levels WHERE snapshot_id = cs.id) as skill_count
             FROM character_snapshots cs
             WHERE cs.character_name = ?1
             ORDER BY cs.snapshot_timestamp DESC",
            vec![Box::new(character_name.clone()) as Box<dyn rusqlite::types::ToSql>],
        )
    };

    let mut stmt = conn
        .prepare(sql)
        .map_err(|e| format!("Failed to prepare query: {e}"))?;

    let rows = stmt
        .query_map(rusqlite::params_from_iter(params.iter()), |row| {
            Ok(CharacterSnapshotSummary {
                id: row.get(0)?,
                character_name: row.get(1)?,
                server_name: row.get(2)?,
                snapshot_timestamp: row.get(3)?,
                race: row.get(4)?,
                import_date: row.get(5)?,
                skill_count: row.get::<_, i64>(6)? as usize,
            })
        })
        .map_err(|e| format!("Query failed: {e}"))?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("Failed to read results: {e}"))
}

#[tauri::command]
pub fn get_snapshot_skills(
    db: State<'_, DbPool>,
    snapshot_id: i64,
) -> Result<Vec<SnapshotSkillLevel>, String> {
    let conn = db
        .get()
        .map_err(|e| format!("Database connection error: {e}"))?;

    let mut stmt = conn
        .prepare(
            "SELECT skill_name, level, bonus_levels, xp_toward_next, xp_needed_for_next
         FROM character_skill_levels
         WHERE snapshot_id = ?1
         ORDER BY skill_name",
        )
        .map_err(|e| format!("Failed to prepare query: {e}"))?;

    let rows = stmt
        .query_map([snapshot_id], |row| {
            Ok(SnapshotSkillLevel {
                skill_name: row.get(0)?,
                level: row.get(1)?,
                bonus_levels: row.get(2)?,
                xp_toward_next: row.get(3)?,
                xp_needed_for_next: row.get(4)?,
            })
        })
        .map_err(|e| format!("Query failed: {e}"))?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("Failed to read results: {e}"))
}

#[tauri::command]
pub fn get_snapshot_npc_favor(
    db: State<'_, DbPool>,
    snapshot_id: i64,
) -> Result<Vec<SnapshotNpcFavor>, String> {
    let conn = db
        .get()
        .map_err(|e| format!("Database connection error: {e}"))?;

    let mut stmt = conn
        .prepare(
            "SELECT npc_key, favor_level
         FROM character_npc_favor
         WHERE snapshot_id = ?1
         ORDER BY npc_key",
        )
        .map_err(|e| format!("Failed to prepare query: {e}"))?;

    let rows = stmt
        .query_map([snapshot_id], |row| {
            Ok(SnapshotNpcFavor {
                npc_key: row.get(0)?,
                favor_level: row.get(1)?,
            })
        })
        .map_err(|e| format!("Query failed: {e}"))?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("Failed to read results: {e}"))
}

#[tauri::command]
pub fn get_snapshot_recipes(
    db: State<'_, DbPool>,
    snapshot_id: i64,
) -> Result<Vec<SnapshotRecipeCompletion>, String> {
    let conn = db
        .get()
        .map_err(|e| format!("Database connection error: {e}"))?;

    let mut stmt = conn
        .prepare(
            "SELECT recipe_key, completions
         FROM character_recipe_completions
         WHERE snapshot_id = ?1
         ORDER BY recipe_key",
        )
        .map_err(|e| format!("Failed to prepare query: {e}"))?;

    let rows = stmt
        .query_map([snapshot_id], |row| {
            Ok(SnapshotRecipeCompletion {
                recipe_key: row.get(0)?,
                completions: row.get(1)?,
            })
        })
        .map_err(|e| format!("Query failed: {e}"))?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("Failed to read results: {e}"))
}

#[tauri::command]
pub fn get_snapshot_stats(
    db: State<'_, DbPool>,
    snapshot_id: i64,
) -> Result<Vec<SnapshotStat>, String> {
    let conn = db
        .get()
        .map_err(|e| format!("Database connection error: {e}"))?;

    let mut stmt = conn
        .prepare(
            "SELECT stat_key, value
         FROM character_stats
         WHERE snapshot_id = ?1
         ORDER BY stat_key",
        )
        .map_err(|e| format!("Failed to prepare query: {e}"))?;

    let rows = stmt
        .query_map([snapshot_id], |row| {
            Ok(SnapshotStat {
                stat_key: row.get(0)?,
                value: row.get(1)?,
            })
        })
        .map_err(|e| format!("Query failed: {e}"))?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("Failed to read results: {e}"))
}

#[tauri::command]
pub fn get_snapshot_currencies(
    db: State<'_, DbPool>,
    snapshot_id: i64,
) -> Result<Vec<SnapshotCurrency>, String> {
    let conn = db
        .get()
        .map_err(|e| format!("Database connection error: {e}"))?;

    let mut stmt = conn
        .prepare(
            "SELECT currency_key, amount
         FROM character_currencies
         WHERE snapshot_id = ?1
         ORDER BY currency_key",
        )
        .map_err(|e| format!("Failed to prepare query: {e}"))?;

    let rows = stmt
        .query_map([snapshot_id], |row| {
            Ok(SnapshotCurrency {
                currency_key: row.get(0)?,
                amount: row.get(1)?,
            })
        })
        .map_err(|e| format!("Query failed: {e}"))?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("Failed to read results: {e}"))
}

#[tauri::command]
pub fn get_snapshot_active_quests(
    db: State<'_, DbPool>,
    snapshot_id: i64,
) -> Result<Vec<SnapshotActiveQuest>, String> {
    let conn = db
        .get()
        .map_err(|e| format!("Database connection error: {e}"))?;

    let mut stmt = conn
        .prepare(
            "SELECT quest_key, category
         FROM character_active_quests
         WHERE snapshot_id = ?1
         ORDER BY category, quest_key",
        )
        .map_err(|e| format!("Failed to prepare query: {e}"))?;

    let rows = stmt
        .query_map([snapshot_id], |row| {
            Ok(SnapshotActiveQuest {
                quest_key: row.get(0)?,
                category: row.get(1)?,
            })
        })
        .map_err(|e| format!("Query failed: {e}"))?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("Failed to read results: {e}"))
}

#[tauri::command]
pub fn compare_snapshots(
    db: State<'_, DbPool>,
    snapshot_id_old: i64,
    snapshot_id_new: i64,
) -> Result<Vec<SkillDiff>, String> {
    let conn = db
        .get()
        .map_err(|e| format!("Database connection error: {e}"))?;

    // Get skills from both snapshots and compute diffs using UNION to emulate FULL OUTER JOIN
    let mut stmt = conn
        .prepare(
            "SELECT
            skill_name,
            MAX(CASE WHEN snapshot_id = ?1 THEN level ELSE 0 END) as old_level,
            MAX(CASE WHEN snapshot_id = ?2 THEN level ELSE 0 END) as new_level,
            MAX(CASE WHEN snapshot_id = ?2 THEN level ELSE 0 END) -
                MAX(CASE WHEN snapshot_id = ?1 THEN level ELSE 0 END) as level_change,
            MAX(CASE WHEN snapshot_id = ?1 THEN xp_toward_next ELSE 0 END) as old_xp,
            MAX(CASE WHEN snapshot_id = ?2 THEN xp_toward_next ELSE 0 END) as new_xp
         FROM character_skill_levels
         WHERE snapshot_id IN (?1, ?2)
         GROUP BY skill_name
         ORDER BY skill_name",
        )
        .map_err(|e| format!("Failed to prepare query: {e}"))?;

    let rows = stmt
        .query_map(rusqlite::params![snapshot_id_old, snapshot_id_new], |row| {
            Ok(SkillDiff {
                skill_name: row.get(0)?,
                old_level: row.get(1)?,
                new_level: row.get(2)?,
                level_change: row.get(3)?,
                old_xp: row.get(4)?,
                new_xp: row.get(5)?,
            })
        })
        .map_err(|e| format!("Query failed: {e}"))?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("Failed to read results: {e}"))
}

// ── Game State Seeding ──────────────────────────────────────────────────────

/// Seed game state tables from a character snapshot.
/// Uses timestamp deconfliction: only overwrites if snapshot is newer than existing data.
fn seed_game_state_from_snapshot(
    conn: &rusqlite::Connection,
    report: &CharacterReport,
    game_data: &GameData,
) -> Result<(), String> {
    let character = &report.character;
    let server = &report.server_name;
    let ts = &report.timestamp;

    // Auto-create server record if not exists
    conn.execute(
        "INSERT INTO servers (server_name) VALUES (?1) ON CONFLICT DO NOTHING",
        rusqlite::params![server],
    )
    .map_err(|e| format!("Failed to upsert server: {e}"))?;

    // Seed skills — resolve internal names to canonical IDs + display names
    let mut skill_stmt = conn.prepare(
        "INSERT INTO game_state_skills (character_name, server_name, skill_id, skill_name, level, base_level, bonus_levels, xp, tnl, max_level, last_confirmed_at, source)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, 0, ?10, 'snapshot')
         ON CONFLICT(character_name, server_name, skill_id) DO UPDATE SET
            skill_name = excluded.skill_name,
            level = excluded.level,
            base_level = excluded.base_level,
            bonus_levels = excluded.bonus_levels,
            xp = excluded.xp,
            tnl = excluded.tnl,
            last_confirmed_at = excluded.last_confirmed_at,
            source = excluded.source
         WHERE excluded.last_confirmed_at > game_state_skills.last_confirmed_at"
    ).map_err(|e| format!("Failed to prepare game state skill upsert: {e}"))?;

    for (skill_name, skill_data) in &report.skills {
        let (skill_id, display_name) = match game_data.resolve_skill(skill_name) {
            Some(info) => (info.id as i64, info.name.clone()),
            None => (0i64, skill_name.clone()),
        };
        let base_level = skill_data.level - skill_data.bonus_levels;
        skill_stmt
            .execute(rusqlite::params![
                character,
                server,
                skill_id,
                display_name,
                skill_data.level, // level (total — JSON's "Level" already includes bonuses)
                base_level,       // base_level (total minus bonuses)
                skill_data.bonus_levels,
                skill_data.xp_toward_next_level,
                skill_data.xp_needed_for_next_level,
                ts,
            ])
            .map_err(|e| format!("Failed to upsert skill: {e}"))?;
    }

    // Seed recipes — snapshot uses string keys like "Recipe_12345", extract numeric ID
    let mut recipe_stmt = conn.prepare(
        "INSERT INTO game_state_recipes (character_name, server_name, recipe_id, completion_count, last_confirmed_at, source)
         VALUES (?1, ?2, ?3, ?4, ?5, 'snapshot')
         ON CONFLICT(character_name, server_name, recipe_id) DO UPDATE SET
            completion_count = excluded.completion_count,
            last_confirmed_at = excluded.last_confirmed_at,
            source = excluded.source
         WHERE excluded.last_confirmed_at > game_state_recipes.last_confirmed_at"
    ).map_err(|e| format!("Failed to prepare game state recipe upsert: {e}"))?;

    for (recipe_key, completions) in &report.recipe_completions {
        // Snapshot keys are InternalName strings (e.g., "Butter", "OrcishFlour").
        // Look up the numeric recipe ID from the CDN internal name index.
        if let Some(&recipe_id) = game_data.recipe_internal_name_index.get(recipe_key) {
            recipe_stmt
                .execute(rusqlite::params![
                    character,
                    server,
                    recipe_id as i64,
                    completions,
                    ts,
                ])
                .map_err(|e| format!("Failed to upsert recipe: {e}"))?;
        }
    }

    // Seed NPC favor — snapshot provides tier names; preserve existing cumulative_delta
    // (snapshots only have tier names, not point totals, so zeroing delta destroys tracked gifting progress)
    let mut favor_stmt = conn.prepare(
        "INSERT INTO game_state_favor (character_name, server_name, npc_key, npc_name, favor_tier, cumulative_delta, last_confirmed_at, source)
         VALUES (?1, ?2, ?3, ?4, ?5, 0, ?6, 'snapshot')
         ON CONFLICT(character_name, server_name, npc_key) DO UPDATE SET
            npc_name = excluded.npc_name,
            favor_tier = excluded.favor_tier,
            last_confirmed_at = excluded.last_confirmed_at,
            source = 'snapshot'
         WHERE excluded.last_confirmed_at > game_state_favor.last_confirmed_at"
    ).map_err(|e| format!("Failed to prepare game state favor upsert: {e}"))?;

    for (npc_key, favor_data) in &report.npcs {
        let display_name = game_data
            .npcs
            .get(npc_key)
            .map(|info| info.name.clone())
            .unwrap_or_else(|| npc_key.clone());
        favor_stmt
            .execute(rusqlite::params![
                character,
                server,
                npc_key,
                display_name,
                favor_data.favor_level,
                ts,
            ])
            .map_err(|e| format!("Failed to upsert favor: {e}"))?;
    }

    // Seed currencies
    let mut currency_stmt = conn.prepare(
        "INSERT INTO game_state_currencies (character_name, server_name, currency_name, amount, last_confirmed_at, source)
         VALUES (?1, ?2, ?3, ?4, ?5, 'snapshot')
         ON CONFLICT(character_name, server_name, currency_name) DO UPDATE SET
            amount = excluded.amount,
            last_confirmed_at = excluded.last_confirmed_at,
            source = excluded.source
         WHERE excluded.last_confirmed_at > game_state_currencies.last_confirmed_at"
    ).map_err(|e| format!("Failed to prepare game state currency upsert: {e}"))?;

    for (currency_name, amount) in &report.currencies {
        currency_stmt
            .execute(rusqlite::params![
                character,
                server,
                currency_name,
                *amount as f64,
                ts,
            ])
            .map_err(|e| format!("Failed to upsert currency: {e}"))?;
    }

    // Re-anchor the council-wallet estimate (migration v56) on this export's
    // GOLD balance, resetting the running `delta_since` to 0 — live chat deltas
    // accrue from here (see coordinator::accrue_currency_delta). Only re-anchors
    // when this snapshot post-dates the stored anchor, so importing an older
    // export can't clobber a fresher anchor. `anchor_at` drops the trailing 'Z'
    // so it string-compares cleanly against the UTC "%Y-%m-%d %H:%M:%S" event
    // timestamps used by the accrual guard.
    if let Some(gold) = report.currencies.get("GOLD") {
        let anchor_at = ts.trim_end_matches('Z');
        conn.execute(
            "INSERT INTO currency_estimate
                (character_name, server_name, currency_name, anchor_amount, anchor_at, delta_since, updated_at)
             VALUES (?1, ?2, 'GOLD', ?3, ?4, 0, datetime('now'))
             ON CONFLICT(character_name, server_name, currency_name) DO UPDATE SET
                anchor_amount = excluded.anchor_amount,
                anchor_at = excluded.anchor_at,
                delta_since = 0,
                updated_at = excluded.updated_at
             WHERE currency_estimate.anchor_at IS NULL
                OR excluded.anchor_at > currency_estimate.anchor_at",
            rusqlite::params![character, server, *gold, anchor_at],
        )
        .map_err(|e| format!("Failed to re-anchor currency estimate: {e}"))?;
    }

    // Seed storage vault contents from the latest item snapshot
    // Full replacement — snapshot is authoritative for storage state
    conn.execute(
        "DELETE FROM game_state_storage WHERE character_name = ?1 AND server_name = ?2",
        rusqlite::params![character, server],
    )
    .map_err(|e| format!("Failed to clear storage: {e}"))?;

    // Find the latest item snapshot for this character+server
    let latest_snapshot_id: Option<i64> = conn
        .query_row(
            "SELECT cis.id FROM character_item_snapshots cis
         WHERE cis.character_name = ?1 AND cis.server_name = ?2
         ORDER BY cis.snapshot_timestamp DESC LIMIT 1",
            rusqlite::params![character, server],
            |row| row.get(0),
        )
        .ok();

    if let Some(snapshot_id) = latest_snapshot_id {
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

        // Snapshot items don't have real instance IDs, so generate synthetic ones per vault
        let rows: Vec<(String, i64, String, i64)> = storage_query
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

        for (i, (vault_key, type_id, item_name, stack_size)) in rows.iter().enumerate() {
            // Use negative synthetic instance IDs to avoid collision with real ones
            let synthetic_id = -(i as i64 + 1);
            storage_insert
                .execute(rusqlite::params![
                    character,
                    server,
                    vault_key,
                    synthetic_id,
                    item_name,
                    type_id,
                    stack_size,
                    ts,
                ])
                .map_err(|e| format!("Failed to insert storage item: {e}"))?;
        }
    }

    eprintln!("[game_state] Seeded game state from snapshot for {character} on {server} at {ts}");
    Ok(())
}
