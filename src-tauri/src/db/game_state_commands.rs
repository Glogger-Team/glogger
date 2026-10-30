use super::DbPool;
use rusqlite::OptionalExtension;
use serde::{Deserialize, Serialize};
/// Tauri commands for querying persisted game state
use tauri::State;

// ── Response Types ──────────────────────────────────────────────────────────

#[derive(Serialize)]
pub struct GameStateSkill {
    pub skill_id: i64,
    pub skill_name: String,
    pub level: i32,
    pub base_level: i32,
    pub bonus_levels: i32,
    pub xp: i64,
    pub tnl: i64,
    pub max_level: i32,
    pub last_confirmed_at: String,
    pub source: String,
}

#[derive(Serialize)]
pub struct GameStateAttribute {
    pub attribute_name: String,
    pub value: f64,
    pub last_confirmed_at: String,
}

#[derive(Serialize)]
pub struct GameStateActiveSkills {
    pub skill1_id: i64,
    pub skill1_name: String,
    pub skill2_id: i64,
    pub skill2_name: String,
    pub last_confirmed_at: String,
}

#[derive(Serialize)]
pub struct GameStateWorld {
    pub weather: Option<GameStateWeather>,
    pub combat: Option<GameStateCombat>,
    pub mount: Option<GameStateMount>,
    pub area: Option<GameStateArea>,
}

#[derive(Serialize)]
pub struct GameStateArea {
    pub area_name: String,
    pub last_confirmed_at: String,
}

#[derive(Serialize)]
pub struct GameStateWeather {
    pub weather_name: String,
    pub is_active: bool,
    pub last_confirmed_at: String,
}

#[derive(Serialize)]
pub struct GameStateCombat {
    pub in_combat: bool,
    pub last_confirmed_at: String,
}

#[derive(Serialize)]
pub struct GameStateMount {
    pub is_mounted: bool,
    pub last_confirmed_at: String,
}

#[derive(Serialize)]
pub struct GameStateInventoryItem {
    pub instance_id: i64,
    pub item_name: String,
    pub item_type_id: Option<i32>,
    pub stack_size: i32,
    pub slot_index: i32,
    pub last_confirmed_at: String,
    pub source: String,
}

#[derive(Serialize)]
pub struct GameStateRecipe {
    pub recipe_id: i32,
    pub completion_count: i32,
    pub last_confirmed_at: String,
    pub source: String,
}

#[derive(Serialize)]
pub struct GameStateEquipmentSlot {
    pub slot: String,
    pub appearance_key: String,
    pub last_confirmed_at: String,
}

#[derive(Serialize)]
pub struct GameStateFavor {
    pub npc_key: String,
    pub npc_name: String,
    pub cumulative_delta: f64,
    pub favor_tier: Option<String>,
    pub last_confirmed_at: String,
    pub source: String,
}

#[derive(Serialize)]
pub struct GameStateCurrency {
    pub currency_name: String,
    pub amount: f64,
    pub last_confirmed_at: String,
    pub source: String,
}

/// Running council-wallet (`GOLD`) estimate (migration v56): the last export's
/// balance (`anchor_amount` at `anchor_at`) plus live deltas (`delta_since`)
/// parsed from chat. `estimated = anchor_amount + delta_since`. `has_anchor` is
/// false until a character export seeds the anchor — until then the estimate is
/// not meaningful and the UI should prompt for an export.
#[derive(Serialize)]
pub struct CurrencyEstimate {
    pub currency_name: String,
    pub anchor_amount: i64,
    pub anchor_at: Option<String>,
    pub delta_since: i64,
    pub estimated: i64,
    pub has_anchor: bool,
}

#[derive(Serialize)]
pub struct GameStateEffect {
    pub effect_instance_id: i64,
    pub effect_name: Option<String>,
    pub source_entity_id: i64,
    pub last_confirmed_at: String,
}

#[derive(Serialize)]
pub struct GameStateStorageItem {
    pub vault_key: String,
    pub instance_id: i64,
    pub item_name: String,
    pub item_type_id: Option<i32>,
    pub stack_size: i32,
    pub slot_index: i32,
    pub last_confirmed_at: String,
    pub source: String,
}

// ── Commands ────────────────────────────────────────────────────────────────

#[tauri::command]
pub fn get_game_state_skills(
    db: State<'_, DbPool>,
    character_name: String,
    server_name: String,
) -> Result<Vec<GameStateSkill>, String> {
    let conn = db.get().map_err(|e| format!("Database error: {e}"))?;
    let mut stmt = conn.prepare(
        "SELECT skill_id, skill_name, level, base_level, bonus_levels, xp, tnl, max_level, last_confirmed_at, source
         FROM game_state_skills WHERE character_name = ?1 AND server_name = ?2
         ORDER BY skill_name"
    ).map_err(|e| format!("Query error: {e}"))?;

    let rows = stmt
        .query_map(rusqlite::params![character_name, server_name], |row| {
            Ok(GameStateSkill {
                skill_id: row.get(0)?,
                skill_name: row.get(1)?,
                level: row.get(2)?,
                base_level: row.get(3)?,
                bonus_levels: row.get(4)?,
                xp: row.get(5)?,
                tnl: row.get(6)?,
                max_level: row.get(7)?,
                last_confirmed_at: row.get(8)?,
                source: row.get(9)?,
            })
        })
        .map_err(|e| format!("Query error: {e}"))?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("Row error: {e}"))
}

#[tauri::command]
pub fn get_game_state_attributes(
    db: State<'_, DbPool>,
    character_name: String,
    server_name: String,
) -> Result<Vec<GameStateAttribute>, String> {
    let conn = db.get().map_err(|e| format!("Database error: {e}"))?;
    let mut stmt = conn
        .prepare(
            "SELECT attribute_name, value, last_confirmed_at
         FROM game_state_attributes WHERE character_name = ?1 AND server_name = ?2
         ORDER BY attribute_name",
        )
        .map_err(|e| format!("Query error: {e}"))?;

    let rows = stmt
        .query_map(rusqlite::params![character_name, server_name], |row| {
            Ok(GameStateAttribute {
                attribute_name: row.get(0)?,
                value: row.get(1)?,
                last_confirmed_at: row.get(2)?,
            })
        })
        .map_err(|e| format!("Query error: {e}"))?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("Row error: {e}"))
}

#[tauri::command]
pub fn get_game_state_active_skills(
    db: State<'_, DbPool>,
    character_name: String,
    server_name: String,
) -> Result<Option<GameStateActiveSkills>, String> {
    let conn = db.get().map_err(|e| format!("Database error: {e}"))?;
    let result = conn.query_row(
        "SELECT skill1_id, skill1_name, skill2_id, skill2_name, last_confirmed_at
         FROM game_state_active_skills WHERE character_name = ?1 AND server_name = ?2",
        rusqlite::params![character_name, server_name],
        |row| {
            Ok(GameStateActiveSkills {
                skill1_id: row.get(0)?,
                skill1_name: row.get(1)?,
                skill2_id: row.get(2)?,
                skill2_name: row.get(3)?,
                last_confirmed_at: row.get(4)?,
            })
        },
    );

    match result {
        Ok(r) => Ok(Some(r)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(format!("Query error: {e}")),
    }
}

#[tauri::command]
pub fn get_game_state_world(
    db: State<'_, DbPool>,
    character_name: String,
    server_name: String,
) -> Result<GameStateWorld, String> {
    let conn = db.get().map_err(|e| format!("Database error: {e}"))?;

    // Weather (singleton, not per-character)
    let weather = conn.query_row(
        "SELECT weather_name, is_active, last_confirmed_at FROM game_state_weather WHERE id = 1",
        [],
        |row| {
            Ok(GameStateWeather {
                weather_name: row.get(0)?,
                is_active: row.get::<_, i32>(1)? != 0,
                last_confirmed_at: row.get(2)?,
            })
        },
    ).optional().map_err(|e| format!("Query error: {e}"))?;

    // Combat
    let combat = conn.query_row(
        "SELECT in_combat, last_confirmed_at FROM game_state_combat WHERE character_name = ?1 AND server_name = ?2",
        rusqlite::params![character_name, server_name],
        |row| {
            Ok(GameStateCombat {
                in_combat: row.get::<_, i32>(0)? != 0,
                last_confirmed_at: row.get(1)?,
            })
        },
    ).optional().map_err(|e| format!("Query error: {e}"))?;

    // Mount
    let mount = conn.query_row(
        "SELECT is_mounted, last_confirmed_at FROM game_state_mount WHERE character_name = ?1 AND server_name = ?2",
        rusqlite::params![character_name, server_name],
        |row| {
            Ok(GameStateMount {
                is_mounted: row.get::<_, i32>(0)? != 0,
                last_confirmed_at: row.get(1)?,
            })
        },
    ).optional().map_err(|e| format!("Query error: {e}"))?;

    // Area
    let area = conn.query_row(
        "SELECT area_name, last_confirmed_at FROM game_state_area WHERE character_name = ?1 AND server_name = ?2",
        rusqlite::params![character_name, server_name],
        |row| {
            Ok(GameStateArea {
                area_name: row.get(0)?,
                last_confirmed_at: row.get(1)?,
            })
        },
    ).optional().map_err(|e| format!("Query error: {e}"))?;

    Ok(GameStateWorld {
        weather,
        combat,
        mount,
        area,
    })
}

#[tauri::command]
pub fn get_game_state_inventory(
    db: State<'_, DbPool>,
    character_name: String,
    server_name: String,
) -> Result<Vec<GameStateInventoryItem>, String> {
    let conn = db.get().map_err(|e| format!("Database error: {e}"))?;
    let mut stmt = conn.prepare(
        "SELECT instance_id, item_name, item_type_id, stack_size, slot_index, last_confirmed_at, source
         FROM game_state_inventory WHERE character_name = ?1 AND server_name = ?2
         ORDER BY slot_index"
    ).map_err(|e| format!("Query error: {e}"))?;

    let rows = stmt
        .query_map(rusqlite::params![character_name, server_name], |row| {
            Ok(GameStateInventoryItem {
                instance_id: row.get(0)?,
                item_name: row.get(1)?,
                item_type_id: row.get(2)?,
                stack_size: row.get(3)?,
                slot_index: row.get(4)?,
                last_confirmed_at: row.get(5)?,
                source: row.get(6)?,
            })
        })
        .map_err(|e| format!("Query error: {e}"))?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("Row error: {e}"))
}

#[tauri::command]
pub fn get_game_state_recipes(
    db: State<'_, DbPool>,
    character_name: String,
    server_name: String,
) -> Result<Vec<GameStateRecipe>, String> {
    let conn = db.get().map_err(|e| format!("Database error: {e}"))?;
    let mut stmt = conn
        .prepare(
            "SELECT recipe_id, completion_count, last_confirmed_at, source
         FROM game_state_recipes WHERE character_name = ?1 AND server_name = ?2
         ORDER BY recipe_id",
        )
        .map_err(|e| format!("Query error: {e}"))?;

    let rows = stmt
        .query_map(rusqlite::params![character_name, server_name], |row| {
            Ok(GameStateRecipe {
                recipe_id: row.get(0)?,
                completion_count: row.get(1)?,
                last_confirmed_at: row.get(2)?,
                source: row.get(3)?,
            })
        })
        .map_err(|e| format!("Query error: {e}"))?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("Row error: {e}"))
}

#[tauri::command]
pub fn get_game_state_equipment(
    db: State<'_, DbPool>,
    character_name: String,
    server_name: String,
) -> Result<Vec<GameStateEquipmentSlot>, String> {
    let conn = db.get().map_err(|e| format!("Database error: {e}"))?;
    let mut stmt = conn
        .prepare(
            "SELECT slot, appearance_key, last_confirmed_at
         FROM game_state_equipment WHERE character_name = ?1 AND server_name = ?2
         ORDER BY slot",
        )
        .map_err(|e| format!("Query error: {e}"))?;

    let rows = stmt
        .query_map(rusqlite::params![character_name, server_name], |row| {
            Ok(GameStateEquipmentSlot {
                slot: row.get(0)?,
                appearance_key: row.get(1)?,
                last_confirmed_at: row.get(2)?,
            })
        })
        .map_err(|e| format!("Query error: {e}"))?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("Row error: {e}"))
}

#[tauri::command]
pub fn get_game_state_favor(
    db: State<'_, DbPool>,
    character_name: String,
    server_name: String,
) -> Result<Vec<GameStateFavor>, String> {
    let conn = db.get().map_err(|e| format!("Database error: {e}"))?;
    let mut stmt = conn
        .prepare(
            "SELECT npc_key, npc_name, cumulative_delta, favor_tier, last_confirmed_at, source
         FROM game_state_favor WHERE character_name = ?1 AND server_name = ?2
         ORDER BY npc_name",
        )
        .map_err(|e| format!("Query error: {e}"))?;

    let rows = stmt
        .query_map(rusqlite::params![character_name, server_name], |row| {
            Ok(GameStateFavor {
                npc_key: row.get(0)?,
                npc_name: row.get(1)?,
                cumulative_delta: row.get(2)?,
                favor_tier: row.get(3)?,
                last_confirmed_at: row.get(4)?,
                source: row.get(5)?,
            })
        })
        .map_err(|e| format!("Query error: {e}"))?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("Row error: {e}"))
}

#[tauri::command]
pub fn get_game_state_currencies(
    db: State<'_, DbPool>,
    character_name: String,
    server_name: String,
) -> Result<Vec<GameStateCurrency>, String> {
    let conn = db.get().map_err(|e| format!("Database error: {e}"))?;
    let mut stmt = conn
        .prepare(
            "SELECT currency_name, amount, last_confirmed_at, source
         FROM game_state_currencies WHERE character_name = ?1 AND server_name = ?2
         ORDER BY currency_name",
        )
        .map_err(|e| format!("Query error: {e}"))?;

    let rows = stmt
        .query_map(rusqlite::params![character_name, server_name], |row| {
            Ok(GameStateCurrency {
                currency_name: row.get(0)?,
                amount: row.get(1)?,
                last_confirmed_at: row.get(2)?,
                source: row.get(3)?,
            })
        })
        .map_err(|e| format!("Query error: {e}"))?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("Row error: {e}"))
}

/// Council-wallet estimate for the active character+server (migration v56).
/// Returns `None` when no row exists yet (no export imported for this
/// character). The frontend reconciles its optimistic live counter against this.
#[tauri::command]
pub fn get_currency_estimate(
    db: State<'_, DbPool>,
    character_name: String,
    server_name: String,
) -> Result<Option<CurrencyEstimate>, String> {
    let conn = db.get().map_err(|e| format!("Database error: {e}"))?;
    get_currency_estimate_for(&conn, &character_name, &server_name)
}

/// Split out from the command so tests can exercise it without Tauri state.
/// Generic over `DbRead` (same convention as the other read helpers here).
///
/// Timestamp-normalization audit (2026-10-26, corrected 2026-10-27):
/// `anchor_at` is written by `import_character_report_internal` as the ISO
/// export timestamp with the trailing `Z` stripped AND the `T` separator
/// replaced with a space (`character_commands.rs`), i.e. space-format
/// `"YYYY-MM-DD HH:MM:SS"` UTC — the same shape `parse_status_message`
/// stamps into chat events (migration v80 repairs pre-existing `T`-shaped
/// anchors, which sorted after same-day space stamps and dropped intraday
/// deltas). The accrual guard (`accrue_currency_delta`,
/// `event_ts > anchor_at`) therefore string-compares like with like;
/// zero-padded fixed-width stamps sort chronologically. The one asymmetry is
/// documented on `apply_chat_level_up`: snapshot `"…Z"` stamps sort *after*
/// same-day space-format chat stamps, but that arm doesn't touch this table.
///
/// `fetchCurrencyEstimate` audit (2026-10-26): the frontend
/// (`gameStateStore.ts`) reconciles its optimistic counter against this on
/// every load and no-ops `applyWalletDelta` until `has_anchor` is true —
/// mirroring the backend accrual guard (`anchor_at IS NOT NULL`), so a
/// pre-export session can neither double-count nor fabricate a balance.
/// `traversal`/`report_kind_for_path` audit: no path handling here — the
/// only path-shaped input in this file's orbit is the VIP-report path, which
/// is classified by `report_kind_for_path` in `coordinator.rs` (5 tests there
/// cover Character/Items shapes, non-Reports parents, and `..` traversal).
pub fn get_currency_estimate_for<C: crate::db::DbRead + ?Sized>(
    conn: &C,
    character_name: &str,
    server_name: &str,
) -> Result<Option<CurrencyEstimate>, String> {
    use rusqlite::OptionalExtension;
    conn.query_row(
        "SELECT currency_name, anchor_amount, anchor_at, delta_since
         FROM currency_estimate
         WHERE character_name = ?1 AND server_name = ?2 AND currency_name = 'GOLD'",
        rusqlite::params![character_name, server_name],
        |row| {
            let currency_name: String = row.get(0)?;
            let anchor_amount: i64 = row.get(1)?;
            let anchor_at: Option<String> = row.get(2)?;
            let delta_since: i64 = row.get(3)?;
            Ok(CurrencyEstimate {
                has_anchor: anchor_at.is_some(),
                estimated: anchor_amount + delta_since,
                currency_name,
                anchor_amount,
                anchor_at,
                delta_since,
            })
        },
    )
    .optional()
    .map_err(|e| format!("Query error: {e}"))
}

#[tauri::command]
pub fn get_game_state_effects(
    db: State<'_, DbPool>,
    character_name: String,
    server_name: String,
) -> Result<Vec<GameStateEffect>, String> {
    let conn = db.get().map_err(|e| format!("Database error: {e}"))?;
    let mut stmt = conn
        .prepare(
            "SELECT effect_instance_id, effect_name, source_entity_id, last_confirmed_at
         FROM game_state_effects WHERE character_name = ?1 AND server_name = ?2
         ORDER BY effect_instance_id",
        )
        .map_err(|e| format!("Query error: {e}"))?;

    let rows = stmt
        .query_map(rusqlite::params![character_name, server_name], |row| {
            Ok(GameStateEffect {
                effect_instance_id: row.get(0)?,
                effect_name: row.get(1)?,
                source_entity_id: row.get(2)?,
                last_confirmed_at: row.get(3)?,
            })
        })
        .map_err(|e| format!("Query error: {e}"))?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("Row error: {e}"))
}

#[tauri::command]
pub fn get_game_state_storage(
    db: State<'_, DbPool>,
    character_name: String,
    server_name: String,
) -> Result<Vec<GameStateStorageItem>, String> {
    let conn = db.get().map_err(|e| format!("Database error: {e}"))?;
    let mut stmt = conn.prepare(
        "SELECT vault_key, instance_id, item_name, item_type_id, stack_size, slot_index, last_confirmed_at, source
         FROM game_state_storage WHERE character_name = ?1 AND server_name = ?2
         ORDER BY vault_key, item_name"
    ).map_err(|e| format!("Query error: {e}"))?;

    let rows = stmt
        .query_map(rusqlite::params![character_name, server_name], |row| {
            Ok(GameStateStorageItem {
                vault_key: row.get(0)?,
                instance_id: row.get(1)?,
                item_name: row.get(2)?,
                item_type_id: row.get(3)?,
                stack_size: row.get(4)?,
                slot_index: row.get(5)?,
                last_confirmed_at: row.get(6)?,
                source: row.get(7)?,
            })
        })
        .map_err(|e| format!("Query error: {e}"))?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("Row error: {e}"))
}

// ── Tracked Skills ─────────────────────────────────────────────────────────

#[derive(Serialize)]
pub struct TrackedSkill {
    pub skill_name: String,
    pub sort_order: i32,
}

#[derive(Deserialize)]
pub struct TrackedSkillEntry {
    pub skill_name: String,
    pub sort_order: i32,
}

#[tauri::command]
pub fn get_tracked_skills(
    db: State<'_, DbPool>,
    character_name: String,
    server_name: String,
) -> Result<Vec<TrackedSkill>, String> {
    let conn = db.get().map_err(|e| format!("Database error: {e}"))?;
    let mut stmt = conn
        .prepare(
            "SELECT skill_name, sort_order FROM tracked_skills
         WHERE character_name = ?1 AND server_name = ?2
         ORDER BY sort_order, skill_name",
        )
        .map_err(|e| format!("Query error: {e}"))?;

    let rows = stmt
        .query_map(rusqlite::params![character_name, server_name], |row| {
            Ok(TrackedSkill {
                skill_name: row.get(0)?,
                sort_order: row.get(1)?,
            })
        })
        .map_err(|e| format!("Query error: {e}"))?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("Row error: {e}"))
}

#[tauri::command]
pub fn set_tracked_skills(
    db: State<'_, DbPool>,
    character_name: String,
    server_name: String,
    skills: Vec<TrackedSkillEntry>,
) -> Result<(), String> {
    let mut conn = db.get_write().map_err(|e| format!("Database error: {e}"))?;
    let conn = conn
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .map_err(|e| format!("Failed to start transaction: {e}"))?;

    conn.execute(
        "DELETE FROM tracked_skills WHERE character_name = ?1 AND server_name = ?2",
        rusqlite::params![character_name, server_name],
    )
    .map_err(|e| format!("Delete error: {e}"))?;

    let mut stmt = conn
        .prepare(
            "INSERT INTO tracked_skills (character_name, server_name, skill_name, sort_order)
         VALUES (?1, ?2, ?3, ?4)",
        )
        .map_err(|e| format!("Prepare error: {e}"))?;

    for entry in &skills {
        stmt.execute(rusqlite::params![
            character_name,
            server_name,
            entry.skill_name,
            entry.sort_order
        ])
        .map_err(|e| format!("Insert error: {e}"))?;
    }
    drop(stmt);

    conn.commit()
        .map_err(|e| format!("Failed to commit transaction: {e}"))?;
    Ok(())
}

// ── Gift Log ────────────────────────────────────────────────────────────────

/// Reconcile the `week_start`-onward gift-log rows for one NPC to `target`
/// (the game's authoritative weekly count). Adds rows stamped `at` (favor 0)
/// when short, trims the newest extras when long. Returns true when the row
/// count changed. Same semantics as the live `GiftCountObserved` arm.
/// Inputs for [`reconcile_gift_count`]; groups the args so the fn stays
/// under the arity lint.
pub(crate) struct GiftReconcile<'a> {
    pub conn: &'a rusqlite::Connection,
    pub character: &'a str,
    pub server: &'a str,
    pub npc_key: &'a str,
    pub npc_name: &'a str,
    pub at: &'a str,
    pub target: u32,
    pub week_start: &'a str,
}

pub(crate) fn reconcile_gift_count(g: &GiftReconcile<'_>) -> Result<bool, rusqlite::Error> {
    let GiftReconcile {
        conn,
        character,
        server,
        npc_key,
        npc_name,
        at,
        target,
        week_start,
    } = *g;
    let target = target as i64;
    // `replace(gifted_at, 'T', ' ')`: rows written by the manual `+` button (and
    // older builds) carry ISO `YYYY-MM-DDTHH:MM:SSZ` stamps, and 'T' sorts after
    // ' ', so an untrimmed comparison counts an early-Monday gift into the new
    // week. The live/backfill rows are already in this format.
    let existing: i64 = conn.query_row(
        "SELECT COUNT(*) FROM game_state_gift_log
         WHERE character_name = ?1 AND server_name = ?2 AND npc_key = ?3
           AND replace(gifted_at, 'T', ' ') >= ?4",
        rusqlite::params![character, server, npc_key, week_start],
        |row| row.get(0),
    )?;

    if target > existing {
        for _ in 0..(target - existing) {
            conn.execute(
                "INSERT INTO game_state_gift_log (character_name, server_name, npc_key, npc_name, gifted_at, favor_delta)
                 VALUES (?1, ?2, ?3, ?4, ?5, 0)",
                rusqlite::params![character, server, npc_key, npc_name, at],
            )?;
        }
    } else if target < existing {
        // Trim the newest extras (e.g. a manual over-count) down to the game's value.
        conn.execute(
            "DELETE FROM game_state_gift_log WHERE id IN (
                SELECT id FROM game_state_gift_log
                WHERE character_name = ?1 AND server_name = ?2 AND npc_key = ?3
                  AND replace(gifted_at, 'T', ' ') >= ?4
                ORDER BY replace(gifted_at, 'T', ' ') DESC, id DESC LIMIT ?5)",
            rusqlite::params![character, server, npc_key, week_start, existing - target],
        )?;
    }

    Ok(target != existing)
}

// ── Chat Level-Ups ────────────────────────────────────────────────────────────

/// Upsert one skill row from a chat `[Status]` level-up line
/// ("You earned N XP and reached level L in Skill!").
/// Post-10/21/2026 this is the only live absolute skill signal
/// (`ProcessUpdateSkill` lines are gone from Player.log).
/// Chat carries only level + this-level XP, so the upsert preserves
/// `base_level/bonus_levels/tnl/max_level` on conflict and always refreshes
/// `skill_name` to the canonical display name (a fresh insert seeds
/// `base_level = level`, `tnl/max = 0`). Same recency-gated shape as the
/// snapshot seeder — except the comparator is lexicographic over two
/// different formats: the coordinator UTC-converts chat stamps before parsing
/// (`"YYYY-MM-DD HH:MM:SS"`), while snapshots carry `"…Z"` UTC. Space
/// (`0x20`) sorts before `T` (`0x54`), so a same-day chat stamp is always
/// lexicographically older than its snapshot — the timestamp guard only
/// orders same-format stamps. The unconditional winner is the genuine
/// level-up (`excluded.level >` stored level always wins — chat is
/// authoritative that the player *reached* that level, even under clock
/// skew); a same-level cross-format newer line is dropped.
/// `tnl` from an older level can diverge from the new level; the widgets key
/// persisted progress off columns they own, never this mixed pair.
/// A level win never moves the clock backward or downward: `last_confirmed_at`
/// is the max of stored and incoming (a skewed-older genuine level-up keeps
/// the newer stamp), and a newer stamp never downgrades the level (same-level
/// newer stamps refresh xp; lower levels are always dropped).
/// Returns true when the row was inserted or updated (changed the table).
/// Inputs for [`apply_chat_level_up`]; groups the args so the fn stays
/// under the arity lint.
pub(crate) struct ChatLevelUp<'a> {
    pub conn: &'a rusqlite::Connection,
    pub character: &'a str,
    pub server: &'a str,
    pub skill_id: i64,
    pub skill_name: &'a str,
    pub level: i64,
    pub xp: i64,
    pub confirmed_at: &'a str,
}

pub(crate) fn apply_chat_level_up(l: &ChatLevelUp<'_>) -> Result<bool, String> {
    let ChatLevelUp {
        conn,
        character,
        server,
        skill_id,
        skill_name,
        level,
        xp,
        confirmed_at,
    } = *l;
    let changed = conn.execute(
        "INSERT INTO game_state_skills
           (character_name, server_name, skill_id, skill_name, level, base_level, bonus_levels, xp, tnl, max_level, last_confirmed_at, source)
         VALUES (?1, ?2, ?3, ?4, ?5, ?5, 0, ?6, 0, 0, ?7, 'chat')
         ON CONFLICT(character_name, server_name, skill_id) DO UPDATE SET
            skill_name = excluded.skill_name,
            level = excluded.level,
            xp = excluded.xp,
            last_confirmed_at = CASE WHEN excluded.last_confirmed_at > game_state_skills.last_confirmed_at THEN excluded.last_confirmed_at ELSE game_state_skills.last_confirmed_at END,
            source = excluded.source
         WHERE (excluded.last_confirmed_at > game_state_skills.last_confirmed_at AND excluded.level >= game_state_skills.level)
            OR excluded.level > game_state_skills.level",
        rusqlite::params![character, server, skill_id, skill_name, level, xp, confirmed_at],
    )
    .map_err(|e| format!("Failed to apply chat level-up: {e}"))?;
    Ok(changed > 0)
}

#[derive(Serialize)]
pub struct GiftLogEntry {
    pub npc_key: String,
    pub npc_name: String,
    pub gifted_at: String,
    pub favor_delta: f64,
}

#[tauri::command]
pub fn get_gift_log(
    db: State<'_, DbPool>,
    character_name: String,
    server_name: String,
) -> Result<Vec<GiftLogEntry>, String> {
    let conn = db.get().map_err(|e| format!("Database error: {e}"))?;
    let mut stmt = conn
        .prepare(
            "SELECT npc_key, npc_name, gifted_at, favor_delta
             FROM game_state_gift_log
             WHERE character_name = ?1 AND server_name = ?2
             ORDER BY gifted_at DESC",
        )
        .map_err(|e| format!("Query error: {e}"))?;

    let rows = stmt
        .query_map(rusqlite::params![character_name, server_name], |row| {
            Ok(GiftLogEntry {
                npc_key: row.get(0)?,
                npc_name: row.get(1)?,
                gifted_at: row.get(2)?,
                favor_delta: row.get(3)?,
            })
        })
        .map_err(|e| format!("Query error: {e}"))?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("Row error: {e}"))
}

#[tauri::command]
pub fn add_manual_gift(
    db: State<'_, DbPool>,
    character_name: String,
    server_name: String,
    npc_key: String,
    npc_name: String,
) -> Result<(), String> {
    let conn = db.get_write().map_err(|e| format!("Database error: {e}"))?;
    // Same storage format as the live/backfill rows (`YYYY-MM-DD HH:MM:SS`):
    // the weekly cutoff compares these stamps lexicographically, and an ISO stamp
    // sorts after every same-date row ('T' > ' '), which would tally an
    // early-Monday gift into the new week.
    let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
    conn.execute(
        "INSERT INTO game_state_gift_log (character_name, server_name, npc_key, npc_name, gifted_at, favor_delta)
         VALUES (?1, ?2, ?3, ?4, ?5, 0.0)",
        rusqlite::params![character_name, server_name, npc_key, npc_name, now],
    )
    .map_err(|e| format!("Insert error: {e}"))?;
    Ok(())
}

#[tauri::command]
pub fn remove_last_gift(
    db: State<'_, DbPool>,
    character_name: String,
    server_name: String,
    npc_key: String,
    week_start: String,
) -> Result<(), String> {
    let conn = db.get_write().map_err(|e| format!("Database error: {e}"))?;
    conn.execute(
        "DELETE FROM game_state_gift_log WHERE id = (
            SELECT id FROM game_state_gift_log
            WHERE character_name = ?1 AND server_name = ?2 AND npc_key = ?3
              AND replace(gifted_at, 'T', ' ') >= ?4
            ORDER BY replace(gifted_at, 'T', ' ') DESC LIMIT 1
        )",
        rusqlite::params![character_name, server_name, npc_key, week_start],
    )
    .map_err(|e| format!("Delete error: {e}"))?;
    Ok(())
}

// ── Vendor State ───────────────────────────────────────────────────────────

#[derive(Serialize)]
pub struct GameStateVendor {
    pub npc_key: String,
    pub vendor_gold_available: Option<i64>,
    pub vendor_gold_max: Option<i64>,
    pub vendor_gold_timer_start: Option<String>,
    pub last_interaction_at: Option<String>,
    pub last_sell_at: Option<String>,
    pub last_confirmed_at: String,
    pub councils_earned_current: Option<i64>,
    pub councils_earned_lifetime: Option<i64>,
}

#[tauri::command]
pub fn get_game_state_vendor(
    db: State<'_, DbPool>,
    character_name: String,
    server_name: String,
) -> Result<Vec<GameStateVendor>, String> {
    let conn = db.get().map_err(|e| format!("Database error: {e}"))?;
    let mut stmt = conn
        .prepare(
            "SELECT npc_key, vendor_gold_available, vendor_gold_max, vendor_gold_timer_start, last_interaction_at, last_sell_at, last_confirmed_at, councils_earned_current, councils_earned_lifetime
             FROM game_state_npc_vendor WHERE character_name = ?1 AND server_name = ?2
             ORDER BY npc_key",
        )
        .map_err(|e| format!("Query error: {e}"))?;

    let rows = stmt
        .query_map(rusqlite::params![character_name, server_name], |row| {
            Ok(GameStateVendor {
                npc_key: row.get(0)?,
                vendor_gold_available: row.get(1)?,
                vendor_gold_max: row.get(2)?,
                vendor_gold_timer_start: row.get(3)?,
                last_interaction_at: row.get(4)?,
                last_sell_at: row.get(5)?,
                last_confirmed_at: row.get(6)?,
                councils_earned_current: row.get(7)?,
                councils_earned_lifetime: row.get(8)?,
            })
        })
        .map_err(|e| format!("Query error: {e}"))?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("Row error: {e}"))
}

#[tauri::command]
pub fn set_manual_vendor_gold(
    db: State<'_, DbPool>,
    character_name: String,
    server_name: String,
    npc_key: String,
    gold_available: i64,
    gold_max: i64,
    reset_hours_remaining: Option<f64>,
) -> Result<(), String> {
    let conn = db.get_write().map_err(|e| format!("Database error: {e}"))?;
    let now = chrono::Utc::now();
    let now_str = now.format("%Y-%m-%dT%H:%M:%SZ").to_string();
    let timer_start: Option<String> = if gold_available >= gold_max {
        None // at cap, no timer needed
    } else if let Some(hours) = reset_hours_remaining {
        // Back-calculate timer start: if N hours remain, timer started at (now - (168 - N) hours)
        let elapsed_hours = 168.0 - hours.clamp(0.0, 168.0);
        let elapsed_secs = (elapsed_hours * 3600.0) as i64;
        let start = now - chrono::Duration::seconds(elapsed_secs);
        Some(start.format("%Y-%m-%dT%H:%M:%SZ").to_string())
    } else {
        Some(now_str.clone()) // default: timer starts now
    };
    conn.execute(
        "INSERT INTO game_state_npc_vendor (character_name, server_name, npc_key, vendor_gold_available, vendor_gold_max, vendor_gold_timer_start, last_confirmed_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
         ON CONFLICT(character_name, server_name, npc_key) DO UPDATE SET
            vendor_gold_available = excluded.vendor_gold_available,
            vendor_gold_max = excluded.vendor_gold_max,
            vendor_gold_timer_start = excluded.vendor_gold_timer_start,
            last_confirmed_at = excluded.last_confirmed_at",
        rusqlite::params![character_name, server_name, npc_key, gold_available, gold_max, timer_start, now_str],
    )
    .map_err(|e| format!("Upsert error: {e}"))?;
    Ok(())
}

// ── Book reports ────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize)]
pub struct GameStateBook {
    pub book_type: String,
    pub title: String,
    pub content: String,
    pub captured_at: String,
}

#[tauri::command]
pub fn get_game_state_books(
    db: State<'_, DbPool>,
    character_name: String,
    server_name: String,
    book_type: Option<String>,
) -> Result<Vec<GameStateBook>, String> {
    let conn = db.get().map_err(|e| format!("Database error: {e}"))?;

    if let Some(bt) = book_type {
        let mut stmt = conn
            .prepare(
                "SELECT book_type, title, content, captured_at
                 FROM game_state_books
                 WHERE character_name = ?1 AND server_name = ?2 AND book_type = ?3
                 ORDER BY captured_at DESC",
            )
            .map_err(|e| format!("Query error: {e}"))?;

        let rows = stmt
            .query_map(rusqlite::params![character_name, server_name, bt], |row| {
                Ok(GameStateBook {
                    book_type: row.get(0)?,
                    title: row.get(1)?,
                    content: row.get(2)?,
                    captured_at: row.get(3)?,
                })
            })
            .map_err(|e| format!("Query error: {e}"))?;

        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("Row error: {e}"))
    } else {
        let mut stmt = conn
            .prepare(
                "SELECT book_type, title, content, captured_at
                 FROM game_state_books
                 WHERE character_name = ?1 AND server_name = ?2
                 ORDER BY captured_at DESC",
            )
            .map_err(|e| format!("Query error: {e}"))?;

        let rows = stmt
            .query_map(rusqlite::params![character_name, server_name], |row| {
                Ok(GameStateBook {
                    book_type: row.get(0)?,
                    title: row.get(1)?,
                    content: row.get(2)?,
                    captured_at: row.get(3)?,
                })
            })
            .map_err(|e| format!("Query error: {e}"))?;

        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("Row error: {e}"))
    }
}

// ── Milking timers ──────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize)]
pub struct MilkingTimer {
    pub cow_name: String,
    pub zone: String,
    /// For a known milk: when it happened. For `cooldown_unknown` rows: the
    /// **earliest observed failed attempt**, which is what bounds the cooldown
    /// (`expiry ≤ last_milked_at + 1h`) — see `record_cow_cooldown_bound`.
    pub last_milked_at: String,
    /// The game only said "milked within the past hour", so this cow's exact
    /// milk time is unknown: the widget must render an upper bound, not a countdown.
    pub cooldown_unknown: bool,
}

#[tauri::command]
pub fn get_milking_timers(
    db: State<'_, DbPool>,
    character_name: String,
    server_name: String,
) -> Result<Vec<MilkingTimer>, String> {
    let conn = db.get().map_err(|e| format!("Database error: {e}"))?;
    get_milking_timers_for(&conn, &character_name, &server_name)
}

/// Split out from the command so tests can exercise it without Tauri state.
pub fn get_milking_timers_for<C: crate::db::DbRead + ?Sized>(
    conn: &C,
    character_name: &str,
    server_name: &str,
) -> Result<Vec<MilkingTimer>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT cow_name, zone, last_milked_at, cooldown_unknown
             FROM milking_timers
             WHERE character_name = ?1 AND server_name = ?2
             ORDER BY zone, cow_name",
        )
        .map_err(|e| format!("Query error: {e}"))?;

    let rows = stmt
        .query_map(rusqlite::params![character_name, server_name], |row| {
            Ok(MilkingTimer {
                cow_name: row.get(0)?,
                zone: row.get(1)?,
                last_milked_at: row.get(2)?,
                cooldown_unknown: row.get(3)?,
            })
        })
        .map_err(|e| format!("Query error: {e}"))?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("Row error: {e}"))
}

/// Record a **known** cow milk: the cooldown starts at `milked_at` (the milk
/// event's own UTC time) and the "unknown" flag is cleared, because this is the
/// truth the bound was only approximating.
///
/// The stamp only ever moves forward (`MAX`), so replaying an old chat log can't
/// hand out a fresh hour; the zone merge still learns a previously-unknown area.
/// Lives here rather than inline in the coordinator so the statement is testable.
pub fn record_cow_milk(
    conn: &rusqlite::Connection,
    character: &str,
    server: &str,
    cow_name: &str,
    zone: &str,
    milked_at: &str,
) -> Result<(), String> {
    conn.execute(
        "INSERT INTO milking_timers
             (character_name, server_name, cow_name, zone, last_milked_at, cooldown_unknown)
         VALUES (?1, ?2, ?3, ?4, ?5, 0)
         ON CONFLICT(character_name, server_name, cow_name) DO UPDATE SET
            last_milked_at = MAX(excluded.last_milked_at, milking_timers.last_milked_at),
            cooldown_unknown = 0,
            zone = CASE WHEN excluded.zone != '' THEN excluded.zone ELSE milking_timers.zone END",
        rusqlite::params![character, server, cow_name, zone, milked_at],
    )
    .map_err(|e| format!("Failed to record cow milk: {e}"))?;
    Ok(())
}

/// Record that a cow was milked *somewhere within the hour before `attempt_at`
/// (the game's "You've already milked X in the past hour." error).
///
/// The exact milk time is unknowable, so the row keeps a **bound** instead of a
/// fabricated stamp: `last_milked_at` = the *earliest* observed failed attempt
/// (each accepted attempt can only tighten it, hence `MIN`), which makes the
/// cooldown expiry `≤ last_milked_at + 1h` — an upper bound the widget renders as
/// "≤ Xm" and which becomes a certain "Ready!" at zero.
///
/// A row that already holds a *known* milk time is never degraded by this: its
/// stamp and flag are left alone (a real time is strictly better information),
/// only the zone merge still applies.
pub fn record_cow_cooldown_bound(
    conn: &rusqlite::Connection,
    character: &str,
    server: &str,
    cow_name: &str,
    zone: &str,
    attempt_at: &str,
) -> Result<(), String> {
    conn.execute(
        "INSERT INTO milking_timers
             (character_name, server_name, cow_name, zone, last_milked_at, cooldown_unknown)
         VALUES (?1, ?2, ?3, ?4, ?5, 1)
         ON CONFLICT(character_name, server_name, cow_name) DO UPDATE SET
            last_milked_at = CASE
                WHEN milking_timers.cooldown_unknown = 1
                  THEN MIN(excluded.last_milked_at, milking_timers.last_milked_at)
                ELSE milking_timers.last_milked_at
            END,
            cooldown_unknown = milking_timers.cooldown_unknown,
            zone = CASE WHEN excluded.zone != '' THEN excluded.zone ELSE milking_timers.zone END",
        rusqlite::params![character, server, cow_name, zone, attempt_at],
    )
    .map_err(|e| format!("Failed to record cow cooldown bound: {e}"))?;
    Ok(())
}

// ── Character stats ─────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize)]
pub struct CharacterReportStat {
    pub category: String,
    pub stat_name: String,
    pub stat_value: String,
    pub updated_at: String,
}

#[tauri::command]
pub fn get_character_report_stats(
    db: State<'_, DbPool>,
    character_name: String,
    server_name: String,
    category: Option<String>,
) -> Result<Vec<CharacterReportStat>, String> {
    let conn = db.get().map_err(|e| format!("Database error: {e}"))?;

    if let Some(cat) = category {
        let mut stmt = conn
            .prepare(
                "SELECT category, stat_name, stat_value, updated_at
                 FROM character_report_stats
                 WHERE character_name = ?1 AND server_name = ?2 AND category = ?3
                 ORDER BY category, stat_name",
            )
            .map_err(|e| format!("Query error: {e}"))?;

        let rows = stmt
            .query_map(rusqlite::params![character_name, server_name, cat], |row| {
                Ok(CharacterReportStat {
                    category: row.get(0)?,
                    stat_name: row.get(1)?,
                    stat_value: row.get(2)?,
                    updated_at: row.get(3)?,
                })
            })
            .map_err(|e| format!("Query error: {e}"))?;

        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("Row error: {e}"))
    } else {
        let mut stmt = conn
            .prepare(
                "SELECT category, stat_name, stat_value, updated_at
                 FROM character_report_stats
                 WHERE character_name = ?1 AND server_name = ?2
                 ORDER BY category, stat_name",
            )
            .map_err(|e| format!("Query error: {e}"))?;

        let rows = stmt
            .query_map(rusqlite::params![character_name, server_name], |row| {
                Ok(CharacterReportStat {
                    category: row.get(0)?,
                    stat_name: row.get(1)?,
                    stat_value: row.get(2)?,
                    updated_at: row.get(3)?,
                })
            })
            .map_err(|e| format!("Query error: {e}"))?;

        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("Row error: {e}"))
    }
}

// ── Player Milking Leaderboard ──────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct PlayerMilkingEntry {
    pub player_name: String,
    pub count: i64,
}

#[tauri::command]
pub fn get_player_milking_leaderboard(
    db: State<'_, DbPool>,
    character_name: String,
    server_name: String,
    direction: String,
) -> Result<Vec<PlayerMilkingEntry>, String> {
    let conn = db.get().map_err(|e| format!("Database error: {e}"))?;
    let mut stmt = conn
        .prepare(
            "SELECT other_player, COUNT(*) as cnt
             FROM player_milking_log
             WHERE character_name = ?1 AND server_name = ?2 AND direction = ?3
             GROUP BY other_player
             ORDER BY cnt DESC, other_player ASC",
        )
        .map_err(|e| format!("Query error: {e}"))?;

    let rows = stmt
        .query_map(
            rusqlite::params![character_name, server_name, direction],
            |row| {
                Ok(PlayerMilkingEntry {
                    player_name: row.get(0)?,
                    count: row.get(1)?,
                })
            },
        )
        .map_err(|e| format!("Query error: {e}"))?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("Row error: {e}"))
}

// ── Milking log ────────────────────────────────────────────────────────────

/// Record one milking event. `milked_at` is RFC3339 UTC **event time**.
///
/// Idempotent — `v72` makes `(character, server, direction, other_player,
/// milked_at)` unique, so the same line seen twice (live tail + Chat.log
/// backfill, or a catch-up replay) leaves one row and the widget's counter can't
/// double-count. Returns `false` when that key was already present.
///
/// The conflict target is spelled out rather than `INSERT OR IGNORE` so *other*
/// constraint failures (a bad `direction` against the v38 CHECK, a NULL) still
/// raise instead of being silently reported as a duplicate.
pub fn record_milking_event(
    conn: &rusqlite::Connection,
    character: &str,
    server: &str,
    other_player: &str,
    direction: &str,
    milked_at: &str,
) -> Result<bool, String> {
    let inserted = conn
        .execute(
            "INSERT INTO player_milking_log
                 (character_name, server_name, other_player, direction, milked_at)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(character_name, server_name, direction, other_player, milked_at)
                 DO NOTHING",
            rusqlite::params![character, server, other_player, direction, milked_at],
        )
        .map_err(|e| format!("Failed to record milking event: {e}"))?;
    Ok(inserted > 0)
}

// ── Pooping (Dungcrafting) ─────────────────────────────────────────────────

/// Lifetime pooping stats for one character.
///
/// `total_poops` / `days_pooped` / first / last come from `player_pooping_log`
/// (fed by the live Combat-channel hook and the Chat.log backfill). The
/// `skill_*` fields mirror the character's Dungcrafting skill row — the game
/// names the skill "Dungcrafting" while its skill type/xp_table is "Pooping".
#[derive(Debug, Clone, Serialize)]
pub struct PoopingStats {
    pub total_poops: i64,
    pub days_pooped: i64,
    pub first_poop: Option<String>,
    pub last_poop: Option<String>,
    pub skill_level: Option<i64>,
    pub skill_max_level: Option<i64>,
}

/// Record one poop. `pooped_at` is the chat log's LOCAL wall-clock timestamp
/// (`YYYY-MM-DD HH:MM:SS`) — the live tail and the chat-log backfill must agree
/// on the dedup key, and the backfill never converts to UTC. Idempotent:
/// returns `false` when that key was already present (the conflict target is
/// spelled out so other constraint failures still raise).
pub fn record_poop(
    conn: &rusqlite::Connection,
    character: &str,
    server: &str,
    target: &str,
    pooped_at: &str,
) -> Result<bool, String> {
    let inserted = conn
        .execute(
            "INSERT INTO player_pooping_log
                 (character_name, server_name, target_name, pooped_at)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(character_name, server_name, pooped_at, target_name)
                 DO NOTHING",
            rusqlite::params![character, server, target, pooped_at],
        )
        .map_err(|e| format!("Failed to record poop: {e}"))?;
    Ok(inserted > 0)
}

#[tauri::command]
pub fn get_pooping_stats(
    db: State<'_, DbPool>,
    character_name: String,
    server_name: String,
) -> Result<PoopingStats, String> {
    let conn = db.get().map_err(|e| format!("Database error: {e}"))?;
    get_pooping_stats_for(&conn, &character_name, &server_name)
}

/// Split out from the command so tests can exercise it without Tauri state.
/// Generic over `DbRead` so it accepts read connections, write connections and
/// transactions alike (same convention as the other read helpers here).
pub fn get_pooping_stats_for<C: crate::db::DbRead + ?Sized>(
    conn: &C,
    character_name: &str,
    server_name: &str,
) -> Result<PoopingStats, String> {
    let (total_poops, days_pooped, first_poop, last_poop) = conn
        .query_row(
            "SELECT COUNT(*), COUNT(DISTINCT substr(pooped_at, 1, 10)), MIN(pooped_at), MAX(pooped_at)
               FROM player_pooping_log
              WHERE character_name = ?1 AND server_name = ?2",
            rusqlite::params![character_name, server_name],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, Option<String>>(3)?,
                ))
            },
        )
        .map_err(|e| format!("Query error: {e}"))?;

    // Accept either spelling: the game's display name is "Dungcrafting", the
    // skill type in Player.log is "Pooping". A rename on either side still
    // resolves as long as one of the names matches.
    let skill = conn
        .query_row(
            "SELECT level, max_level FROM game_state_skills
              WHERE character_name = ?1 AND server_name = ?2
                AND (skill_name = 'Dungcrafting' OR skill_name = 'Pooping')
              ORDER BY level DESC LIMIT 1",
            rusqlite::params![character_name, server_name],
            |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?)),
        )
        .optional()
        .map_err(|e| format!("Query error: {e}"))?;

    let (skill_level, skill_max_level) = match skill {
        Some((level, max_level)) => (Some(level), Some(max_level)),
        None => (None, None),
    };

    Ok(PoopingStats {
        total_poops,
        days_pooped,
        first_poop,
        last_poop,
        skill_level,
        skill_max_level,
    })
}

// ── Harvest Almanac ────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct HarvestAlmanacEntry {
    pub monster_name: String,
    pub description: Option<String>,
    pub event_start: Option<String>,
    pub event_end: Option<String>,
    pub is_current: bool,
    pub captured_at: String,
}

/// The day's harvesting foci as of the last time this character read the
/// Emotion-Harvesting Almanac in Statehelm.
#[tauri::command]
pub fn get_harvest_almanac(
    db: State<'_, DbPool>,
    character_name: String,
    server_name: String,
) -> Result<Vec<HarvestAlmanacEntry>, String> {
    let conn = db.get().map_err(|e| format!("Database error: {e}"))?;
    let mut stmt = conn
        .prepare(
            "SELECT monster_name, description, event_start, event_end, is_current, captured_at
             FROM harvest_almanac
             WHERE character_name = ?1 AND server_name = ?2
             ORDER BY is_current DESC, event_start ASC",
        )
        .map_err(|e| format!("Query error: {e}"))?;

    let rows = stmt
        .query_map(rusqlite::params![character_name, server_name], |row| {
            Ok(HarvestAlmanacEntry {
                monster_name: row.get(0)?,
                description: row.get(1)?,
                event_start: row.get(2)?,
                event_end: row.get(3)?,
                is_current: row.get::<_, i32>(4)? != 0,
                captured_at: row.get(5)?,
            })
        })
        .map_err(|e| format!("Query error: {e}"))?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("Row error: {e}"))
}

// ── Garden Almanac ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct GardenAlmanacEntry {
    pub crop_name: String,
    pub zone_name: String,
    pub event_start: Option<String>,
    pub event_end: Option<String>,
    pub is_current: bool,
    pub captured_at: String,
}

#[tauri::command]
pub fn get_garden_almanac(
    db: State<'_, DbPool>,
    character_name: String,
    server_name: String,
) -> Result<Vec<GardenAlmanacEntry>, String> {
    let conn = db.get().map_err(|e| format!("Database error: {e}"))?;
    let mut stmt = conn
        .prepare(
            "SELECT crop_name, zone_name, event_start, event_end, is_current, captured_at
             FROM garden_almanac
             WHERE character_name = ?1 AND server_name = ?2
             ORDER BY is_current DESC, event_start ASC",
        )
        .map_err(|e| format!("Query error: {e}"))?;

    let rows = stmt
        .query_map(rusqlite::params![character_name, server_name], |row| {
            Ok(GardenAlmanacEntry {
                crop_name: row.get(0)?,
                zone_name: row.get(1)?,
                event_start: row.get(2)?,
                event_end: row.get(3)?,
                is_current: row.get::<_, i32>(4)? != 0,
                captured_at: row.get(5)?,
            })
        })
        .map_err(|e| format!("Query error: {e}"))?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("Row error: {e}"))
}

#[derive(Debug, Clone, Serialize)]
pub struct GardenAlmanacHistoryEntry {
    pub crop_name: String,
    pub zone_name: String,
    pub event_start: Option<String>,
    pub event_end: Option<String>,
    pub captured_at: String,
}

#[tauri::command]
pub fn get_garden_almanac_history(
    db: State<'_, DbPool>,
    character_name: String,
    server_name: String,
) -> Result<Vec<GardenAlmanacHistoryEntry>, String> {
    let conn = db.get().map_err(|e| format!("Database error: {e}"))?;
    let mut stmt = conn
        .prepare(
            "SELECT crop_name, zone_name, event_start, event_end, captured_at
             FROM garden_almanac_history
             WHERE character_name = ?1 AND server_name = ?2
             ORDER BY captured_at DESC
             LIMIT 100",
        )
        .map_err(|e| format!("Query error: {e}"))?;

    let rows = stmt
        .query_map(rusqlite::params![character_name, server_name], |row| {
            Ok(GardenAlmanacHistoryEntry {
                crop_name: row.get(0)?,
                zone_name: row.get(1)?,
                event_start: row.get(2)?,
                event_end: row.get(3)?,
                captured_at: row.get(4)?,
            })
        })
        .map_err(|e| format!("Query error: {e}"))?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("Row error: {e}"))
}

// ── Computed Stats ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct SkillCraftingBreakdown {
    pub skill_name: String,
    pub total_crafted: i64,
    pub crafting_seconds: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct AttributeExtreme {
    pub attribute_name: String,
    pub current_value: f64,
    pub min_value: f64,
    pub max_value: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ComputedStats {
    pub total_level: i64,
    pub total_base_level: i64,
    pub total_bonus_levels: i64,
    pub skill_count: i64,
    pub total_xp_earned: i64,
    pub items_crafted: i64,
    pub items_distilled: i64,
    pub items_deconstructed: i64,
    pub times_teleported: i64,
    pub items_dyed: i64,
    pub total_crafting_seconds: f64,
    // Rate stats (None if time_played not available from /age report)
    pub hours_played: Option<f64>,
    pub xp_per_hour: Option<f64>,
    pub kills_per_hour: Option<f64>,
    pub deaths_per_hour: Option<f64>,
    // Per-skill crafting breakdowns
    pub crafting_by_skill: Vec<SkillCraftingBreakdown>,
}

/// Parse a "X days Y hours" string into total hours.
/// Handles formats like: "40 days 13 hours", "5 hours", "120 days", "1 days 0 hours"
fn parse_time_played_hours(value: &str) -> Option<f64> {
    let mut total_hours: f64 = 0.0;
    let parts: Vec<&str> = value.split_whitespace().collect();
    let mut i = 0;
    while i + 1 < parts.len() {
        if let Ok(num) = parts[i].parse::<f64>() {
            let unit = parts[i + 1].to_lowercase();
            if unit.starts_with("day") {
                total_hours += num * 24.0;
            } else if unit.starts_with("hour") {
                total_hours += num;
            } else if unit.starts_with("minute") {
                total_hours += num / 60.0;
            }
            i += 2;
        } else {
            i += 1;
        }
    }
    if total_hours > 0.0 {
        Some(total_hours)
    } else {
        None
    }
}

#[tauri::command]
pub fn get_computed_stats(
    db: State<'_, DbPool>,
    character_name: String,
    server_name: String,
) -> Result<ComputedStats, String> {
    let conn = db.get().map_err(|e| format!("Database error: {e}"))?;

    // ── Skill totals ────────────────────────────────────────────────
    let (total_level, total_base_level, total_bonus_levels, skill_count, total_xp_earned): (
        i64,
        i64,
        i64,
        i64,
        i64,
    ) = conn
        .query_row(
            "SELECT
                COALESCE(SUM(level + bonus_levels), 0),
                COALESCE(SUM(level), 0),
                COALESCE(SUM(bonus_levels), 0),
                COUNT(*),
                COALESCE(SUM(xp), 0)
             FROM game_state_skills
             WHERE character_name = ?1 AND server_name = ?2",
            rusqlite::params![character_name, server_name],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                ))
            },
        )
        .map_err(|e| format!("Skill totals query error: {e}"))?;

    // ── Recipe-based stats ──────────────────────────────────────────

    // Items crafted: recipes whose skill is NOT a utility/knowledge skill
    let items_crafted: i64 = conn
        .query_row(
            "SELECT COALESCE(SUM(gr.completion_count), 0)
             FROM game_state_recipes gr
             JOIN recipes r ON r.id = gr.recipe_id
             WHERE gr.character_name = ?1 AND gr.server_name = ?2
               AND r.skill IS NOT NULL
               AND r.skill NOT IN ('Lore', 'Teleportation', 'Augmentation', 'Transmutation')",
            rusqlite::params![character_name, server_name],
            |row| row.get(0),
        )
        .map_err(|e| format!("Items crafted query error: {e}"))?;

    // Items distilled: Transmutation recipes that aren't repair crafts
    let items_distilled: i64 = conn
        .query_row(
            "SELECT COALESCE(SUM(gr.completion_count), 0)
             FROM game_state_recipes gr
             JOIN recipes r ON r.id = gr.recipe_id
             WHERE gr.character_name = ?1 AND gr.server_name = ?2
               AND r.skill = 'Transmutation'
               AND COALESCE(r.name, '') NOT LIKE '%Repair%'
               AND COALESCE(r.action_label, '') NOT LIKE '%Repair%'",
            rusqlite::params![character_name, server_name],
            |row| row.get(0),
        )
        .map_err(|e| format!("Items distilled query error: {e}"))?;

    // Items deconstructed: recipes with internal names like "DecomposeHead", "DecomposeChest", etc.
    let items_deconstructed: i64 = conn
        .query_row(
            "SELECT COALESCE(SUM(gr.completion_count), 0)
             FROM game_state_recipes gr
             JOIN recipes r ON r.id = gr.recipe_id
             WHERE gr.character_name = ?1 AND gr.server_name = ?2
               AND COALESCE(json_extract(r.raw_json, '$.InternalName'), '') LIKE 'Decompose%'",
            rusqlite::params![character_name, server_name],
            |row| row.get(0),
        )
        .map_err(|e| format!("Items deconstructed query error: {e}"))?;

    // Times teleported: Teleportation recipes that aren't binding-related
    let times_teleported: i64 = conn
        .query_row(
            "SELECT COALESCE(SUM(gr.completion_count), 0)
             FROM game_state_recipes gr
             JOIN recipes r ON r.id = gr.recipe_id
             WHERE gr.character_name = ?1 AND gr.server_name = ?2
               AND r.skill = 'Teleportation'
               AND COALESCE(r.name, '') NOT LIKE '%Bind%'
               AND COALESCE(r.name, '') NOT LIKE '%Binding%'",
            rusqlite::params![character_name, server_name],
            |row| row.get(0),
        )
        .map_err(|e| format!("Times teleported query error: {e}"))?;

    // Items dyed: recipes with internal names like "DyeLeatherArmor1", "DyeMetalArmor2", etc.
    let items_dyed: i64 = conn
        .query_row(
            "SELECT COALESCE(SUM(gr.completion_count), 0)
             FROM game_state_recipes gr
             JOIN recipes r ON r.id = gr.recipe_id
             WHERE gr.character_name = ?1 AND gr.server_name = ?2
               AND COALESCE(json_extract(r.raw_json, '$.InternalName'), '') LIKE 'Dye%Armor%'",
            rusqlite::params![character_name, server_name],
            |row| row.get(0),
        )
        .map_err(|e| format!("Items dyed query error: {e}"))?;

    // Total crafting time: sum of (completion_count * usage_delay) for all recipes
    let total_crafting_seconds: f64 = conn
        .query_row(
            "SELECT COALESCE(SUM(gr.completion_count * COALESCE(r.usage_delay, 0)), 0.0)
             FROM game_state_recipes gr
             JOIN recipes r ON r.id = gr.recipe_id
             WHERE gr.character_name = ?1 AND gr.server_name = ?2",
            rusqlite::params![character_name, server_name],
            |row| row.get(0),
        )
        .map_err(|e| format!("Crafting time query error: {e}"))?;

    // ── Per-skill crafting breakdowns ───────────────────────────────
    let crafting_by_skill = {
        let mut stmt = conn
            .prepare(
                "SELECT r.skill, SUM(gr.completion_count), SUM(gr.completion_count * COALESCE(r.usage_delay, 0))
                 FROM game_state_recipes gr
                 JOIN recipes r ON r.id = gr.recipe_id
                 WHERE gr.character_name = ?1 AND gr.server_name = ?2
                   AND r.skill IS NOT NULL
                 GROUP BY r.skill
                 ORDER BY SUM(gr.completion_count) DESC",
            )
            .map_err(|e| format!("Crafting breakdown query error: {e}"))?;

        let rows = stmt
            .query_map(rusqlite::params![character_name, server_name], |row| {
                Ok(SkillCraftingBreakdown {
                    skill_name: row.get(0)?,
                    total_crafted: row.get(1)?,
                    crafting_seconds: row.get(2)?,
                })
            })
            .map_err(|e| format!("Crafting breakdown query error: {e}"))?;

        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("Crafting breakdown row error: {e}"))?
    };

    // ── Rate stats from report stats ────────────────────────────────
    // Look up time_played, kills, deaths from character_report_stats (from /age report)
    let hours_played = {
        let time_str: Option<String> = conn
            .query_row(
                "SELECT stat_value FROM character_report_stats
                 WHERE character_name = ?1 AND server_name = ?2
                   AND category = 'age' AND stat_name = 'time_played'",
                rusqlite::params![character_name, server_name],
                |row| row.get(0),
            )
            .ok();
        time_str.and_then(|s| parse_time_played_hours(&s))
    };

    let report_kills: Option<f64> = conn
        .query_row(
            "SELECT stat_value FROM character_report_stats
             WHERE character_name = ?1 AND server_name = ?2
               AND category = 'age' AND stat_name = 'kills'",
            rusqlite::params![character_name, server_name],
            |row| {
                let val: String = row.get(0)?;
                Ok(val.replace(',', "").parse::<f64>().ok())
            },
        )
        .ok()
        .flatten();

    let report_deaths: Option<f64> = conn
        .query_row(
            "SELECT stat_value FROM character_report_stats
             WHERE character_name = ?1 AND server_name = ?2
               AND category = 'age' AND stat_name = 'deaths'",
            rusqlite::params![character_name, server_name],
            |row| {
                let val: String = row.get(0)?;
                Ok(val.replace(',', "").parse::<f64>().ok())
            },
        )
        .ok()
        .flatten();

    let xp_per_hour = hours_played.map(|h| total_xp_earned as f64 / h);
    let kills_per_hour = match (hours_played, report_kills) {
        (Some(h), Some(k)) if h > 0.0 => Some(k / h),
        _ => None,
    };
    let deaths_per_hour = match (hours_played, report_deaths) {
        (Some(h), Some(d)) if h > 0.0 => Some(d / h),
        _ => None,
    };

    Ok(ComputedStats {
        total_level,
        total_base_level,
        total_bonus_levels,
        skill_count,
        total_xp_earned,
        items_crafted,
        items_distilled,
        items_deconstructed,
        times_teleported,
        items_dyed,
        total_crafting_seconds,
        hours_played,
        xp_per_hour,
        kills_per_hour,
        deaths_per_hour,
        crafting_by_skill,
    })
}

#[tauri::command]
pub fn get_attribute_extremes(
    db: State<'_, DbPool>,
    character_name: String,
    server_name: String,
) -> Result<Vec<AttributeExtreme>, String> {
    let conn = db.get().map_err(|e| format!("Database error: {e}"))?;
    let mut stmt = conn
        .prepare(
            "SELECT attribute_name, value, COALESCE(min_value, value), COALESCE(max_value, value)
             FROM game_state_attributes
             WHERE character_name = ?1 AND server_name = ?2
             ORDER BY attribute_name",
        )
        .map_err(|e| format!("Attribute extremes query error: {e}"))?;

    let rows = stmt
        .query_map(rusqlite::params![character_name, server_name], |row| {
            Ok(AttributeExtreme {
                attribute_name: row.get(0)?,
                current_value: row.get(1)?,
                min_value: row.get(2)?,
                max_value: row.get(3)?,
            })
        })
        .map_err(|e| format!("Attribute extremes query error: {e}"))?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("Attribute extremes row error: {e}"))
}

// ── Teleportation bind queries ───────────────────────────────────────────────

#[derive(serde::Serialize)]
pub struct TeleportationBinds {
    pub primary_bind: Option<String>,
    pub secondary_bind: Option<String>,
    pub mushroom_circle_1: Option<String>,
    pub mushroom_circle_2: Option<String>,
    pub last_updated: Option<String>,
}

/// Get the player's known teleportation bind locations.
#[tauri::command]
pub async fn get_teleportation_binds(
    character: String,
    server: String,
    db: State<'_, DbPool>,
) -> Result<TeleportationBinds, String> {
    let conn = db.get().map_err(|e| e.to_string())?;
    let mut stmt = conn
        .prepare(
            "SELECT primary_bind, secondary_bind, mushroom_circle_1, mushroom_circle_2, last_updated
             FROM game_state_teleportation
             WHERE character_name = ?1 AND server_name = ?2",
        )
        .map_err(|e| e.to_string())?;

    let result = stmt
        .query_row(rusqlite::params![character, server], |row| {
            Ok(TeleportationBinds {
                primary_bind: row.get(0)?,
                secondary_bind: row.get(1)?,
                mushroom_circle_1: row.get(2)?,
                mushroom_circle_2: row.get(3)?,
                last_updated: row.get(4)?,
            })
        })
        .optional()
        .map_err(|e| e.to_string())?;

    Ok(result.unwrap_or(TeleportationBinds {
        primary_bind: None,
        secondary_bind: None,
        mushroom_circle_1: None,
        mushroom_circle_2: None,
        last_updated: None,
    }))
}

/// Manually set mushroom circle attunements (can't be parsed from logs).
#[tauri::command]
pub async fn set_mushroom_circles(
    character: String,
    server: String,
    circle_1: Option<String>,
    circle_2: Option<String>,
    db: State<'_, DbPool>,
) -> Result<(), String> {
    let conn = db.get_write().map_err(|e| e.to_string())?;
    let dt = chrono::Utc::now().to_rfc3339();
    conn.execute(
        "INSERT INTO game_state_teleportation
            (character_name, server_name, mushroom_circle_1, mushroom_circle_2, last_updated)
         VALUES (?1, ?2, ?3, ?4, ?5)
         ON CONFLICT(character_name, server_name) DO UPDATE SET
            mushroom_circle_1 = excluded.mushroom_circle_1,
            mushroom_circle_2 = excluded.mushroom_circle_2,
            last_updated = excluded.last_updated",
        rusqlite::params![character, server, circle_1, circle_2, dt],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

// ── Item ownership lookup ──────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct ItemOwnershipLocation {
    pub location: String,
    pub count: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ItemOwnershipInfo {
    pub total_count: i64,
    pub locations: Vec<ItemOwnershipLocation>,
}

#[tauri::command]
pub fn get_item_ownership(
    db: State<'_, DbPool>,
    character_name: String,
    server_name: String,
    item_name: String,
) -> Result<ItemOwnershipInfo, String> {
    let conn = db.get().map_err(|e| format!("Database error: {e}"))?;

    let mut locations: Vec<ItemOwnershipLocation> = Vec::new();

    // Inventory
    let inv_count: i64 = conn
        .query_row(
            "SELECT COALESCE(SUM(stack_size), 0) FROM game_state_inventory
             WHERE character_name = ?1 AND server_name = ?2 AND item_name = ?3",
            rusqlite::params![character_name, server_name, item_name],
            |row| row.get(0),
        )
        .map_err(|e| format!("Query error: {e}"))?;

    if inv_count > 0 {
        locations.push(ItemOwnershipLocation {
            location: "Inventory".to_string(),
            count: inv_count,
        });
    }

    // Storage vaults
    let mut stmt = conn
        .prepare(
            "SELECT vault_key, SUM(stack_size) FROM game_state_storage
             WHERE character_name = ?1 AND server_name = ?2 AND item_name = ?3
             GROUP BY vault_key ORDER BY vault_key",
        )
        .map_err(|e| format!("Query error: {e}"))?;

    let vault_rows = stmt
        .query_map(
            rusqlite::params![character_name, server_name, item_name],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)),
        )
        .map_err(|e| format!("Query error: {e}"))?;

    for row in vault_rows {
        let (vault_key, count) = row.map_err(|e| format!("Row error: {e}"))?;
        if count > 0 {
            locations.push(ItemOwnershipLocation {
                location: vault_key,
                count,
            });
        }
    }

    let total_count: i64 = locations.iter().map(|l| l.count).sum();

    Ok(ItemOwnershipInfo {
        total_count,
        locations,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_time_played_hours() {
        assert_eq!(
            parse_time_played_hours("40 days 13 hours"),
            Some(40.0 * 24.0 + 13.0)
        );
        assert_eq!(parse_time_played_hours("5 hours"), Some(5.0));
        assert_eq!(parse_time_played_hours("120 days"), Some(120.0 * 24.0));
        assert_eq!(parse_time_played_hours("1 days 0 hours"), Some(24.0));
        assert_eq!(parse_time_played_hours("0 days 0 hours"), None); // 0 total → None
        assert_eq!(parse_time_played_hours(""), None);
        assert_eq!(parse_time_played_hours("garbage"), None);
        assert_eq!(parse_time_played_hours("71 days"), Some(71.0 * 24.0));
    }

    fn setup() -> rusqlite::Connection {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        crate::db::migrations::run_migrations(&conn, None).unwrap();
        conn
    }

    #[test]
    fn poop_stats_dedup_per_char_and_read_skill() {
        let conn = setup();

        // The live tail and the Chat.log backfill both see the same line — the
        // second insert is a no-op, not a second poop.
        assert!(record_poop(&conn, "Zen", "Dreva", "Zen", "2026-08-10 20:42:54").unwrap());
        assert!(!record_poop(&conn, "Zen", "Dreva", "Zen", "2026-08-10 20:42:54").unwrap());
        assert!(record_poop(&conn, "Zen", "Dreva", "Zen", "2026-08-11 09:00:00").unwrap());
        // Another character on the same server must not leak into the count.
        record_poop(&conn, "Alt", "Dreva", "Alt", "2026-08-11 09:00:00").unwrap();

        conn.execute(
            "INSERT INTO game_state_skills
                 (character_name, server_name, skill_id, skill_name, level, bonus_levels, xp, tnl, max_level, last_confirmed_at, source)
             VALUES ('Zen','Dreva',7,'Dungcrafting',50,0,0,-1,50,'2026-08-11T00:00:00','log')",
            [],
        )
        .unwrap();

        let stats = get_pooping_stats_for(&conn, "Zen", "Dreva").unwrap();
        assert_eq!(stats.total_poops, 2);
        assert_eq!(stats.days_pooped, 2);
        assert_eq!(stats.first_poop.as_deref(), Some("2026-08-10 20:42:54"));
        assert_eq!(stats.last_poop.as_deref(), Some("2026-08-11 09:00:00"));
        assert_eq!(stats.skill_level, Some(50));
        assert_eq!(stats.skill_max_level, Some(50));

        // A character with poops but no Dungcrafting row still gets counts.
        let bare = get_pooping_stats_for(&conn, "Alt", "Dreva").unwrap();
        assert_eq!(bare.total_poops, 1);
        assert_eq!(bare.skill_level, None);
    }

    #[test]
    fn cow_cooldown_bound_is_honest_and_real_milk_wins() {
        let conn = setup();

        // A failed attempt records a *bound*: the flag is set and the stamp is the
        // attempt itself, so the widget can show "expiry ≤ attempt + 1h".
        record_cow_cooldown_bound(
            &conn,
            "Zen",
            "Dreva",
            "Homer",
            "Serbule",
            "2026-10-13T01:00:00+00:00",
        )
        .unwrap();
        let timers = get_milking_timers_for(&conn, "Zen", "Dreva").unwrap();
        assert_eq!(timers.len(), 1);
        assert!(timers[0].cooldown_unknown);
        assert_eq!(timers[0].last_milked_at, "2026-10-13T01:00:00+00:00");
        assert_eq!(timers[0].zone, "Serbule");

        // Later attempts keep the flag and tighten the bound to the *earliest*
        // attempt seen (a stricter statement about when the milk happened) — a
        // chronological replay therefore converges instead of drifting.
        record_cow_cooldown_bound(
            &conn,
            "Zen",
            "Dreva",
            "Homer",
            "Serbule",
            "2026-10-13T01:30:00+00:00",
        )
        .unwrap();
        record_cow_cooldown_bound(
            &conn,
            "Zen",
            "Dreva",
            "Homer",
            "",
            "2026-10-13T00:50:00+00:00",
        )
        .unwrap();
        let timers = get_milking_timers_for(&conn, "Zen", "Dreva").unwrap();
        assert!(timers[0].cooldown_unknown);
        assert_eq!(timers[0].last_milked_at, "2026-10-13T00:50:00+00:00");
        assert_eq!(
            timers[0].zone, "Serbule",
            "blank zone must not clobber a known one"
        );

        // A real milk is strictly better information: it clears the flag and
        // installs the true time.
        record_cow_milk(
            &conn,
            "Zen",
            "Dreva",
            "Homer",
            "Serbule",
            "2026-10-13T02:05:00+00:00",
        )
        .unwrap();
        let timers = get_milking_timers_for(&conn, "Zen", "Dreva").unwrap();
        assert!(!timers[0].cooldown_unknown);
        assert_eq!(timers[0].last_milked_at, "2026-10-13T02:05:00+00:00");

        // …and no later bound may degrade it (only the zone merge still applies).
        record_cow_cooldown_bound(
            &conn,
            "Zen",
            "Dreva",
            "Homer",
            "Eltibule",
            "2026-10-13T02:20:00+00:00",
        )
        .unwrap();
        let timers = get_milking_timers_for(&conn, "Zen", "Dreva").unwrap();
        assert!(!timers[0].cooldown_unknown);
        assert_eq!(timers[0].last_milked_at, "2026-10-13T02:05:00+00:00");
        assert_eq!(timers[0].zone, "Eltibule");

        // Replaying an older successful milk can't move the stamp back either.
        record_cow_milk(
            &conn,
            "Zen",
            "Dreva",
            "Homer",
            "",
            "2026-10-13T01:00:00+00:00",
        )
        .unwrap();
        let timers = get_milking_timers_for(&conn, "Zen", "Dreva").unwrap();
        assert_eq!(timers[0].last_milked_at, "2026-10-13T02:05:00+00:00");
        assert!(!timers[0].cooldown_unknown);
    }

    #[test]
    fn milking_events_dedup_on_the_v72_key_and_surface_bad_directions() {
        let conn = setup();

        // Same event twice (live tail + Chat.log backfill / a replayed catch-up):
        // exactly one row, and the second call reports "nothing inserted" so the
        // caller can skip its `milking` refresh event.
        assert!(record_milking_event(
            &conn,
            "Zen",
            "Dreva",
            "Zen",
            "self_milked",
            "2026-10-13T01:14:16+00:00"
        )
        .unwrap());
        assert!(!record_milking_event(
            &conn,
            "Zen",
            "Dreva",
            "Zen",
            "self_milked",
            "2026-10-13T01:14:16+00:00"
        )
        .unwrap());

        // Every other part of the key still separates rows: another second,
        // direction, player, or character.
        assert!(record_milking_event(
            &conn,
            "Zen",
            "Dreva",
            "Zen",
            "self_milked",
            "2026-10-13T01:14:17+00:00"
        )
        .unwrap());
        assert!(record_milking_event(
            &conn,
            "Zen",
            "Dreva",
            "Zen",
            "milked_by",
            "2026-10-13T01:14:16+00:00"
        )
        .unwrap());
        assert!(record_milking_event(
            &conn,
            "Zen",
            "Dreva",
            "Reyetta",
            "milked",
            "2026-10-13T01:14:16+00:00"
        )
        .unwrap());
        assert!(record_milking_event(
            &conn,
            "Alt",
            "Dreva",
            "Zen",
            "self_milked",
            "2026-10-13T01:14:16+00:00"
        )
        .unwrap());

        let rows: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM player_milking_log WHERE character_name = 'Zen'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(rows, 4);

        // The conflict target is explicit, so a value the v38 CHECK rejects is an
        // error rather than a silent no-op that looks like a duplicate.
        let err = record_milking_event(
            &conn,
            "Zen",
            "Dreva",
            "Zen",
            "bogus_direction",
            "2026-10-13T01:14:18+00:00",
        );
        assert!(err.is_err(), "CHECK violation must surface, got {err:?}");
    }
    #[test]
    fn chat_level_up_inserts_new_row() {
        let conn = setup();

        // Verbatim values from the fixture line
        // "You earned 192 XP and reached level 87 in Cartography!"
        let changed =
            apply_chat_level_up(&ChatLevelUp { conn: &conn, character: "TestPlayer", server: "Dreva", skill_id: 106, skill_name: "Cartography", level: 87, xp: 192, confirmed_at: "2026-04-16 14:23:16" })
                .expect("apply level-up");
        assert!(changed);

        let (level, xp, source, ts, base_level, tnl): (i64, i64, String, String, i64, i64) = conn
            .query_row(
                "SELECT level, xp, source, last_confirmed_at, base_level, tnl FROM game_state_skills
                 WHERE character_name = 'TestPlayer' AND server_name = 'Dreva' AND skill_id = 106",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?)),
            )
            .expect("row exists");
        assert_eq!(level, 87);
        assert_eq!(xp, 192);
        assert_eq!(source, "chat");
        assert_eq!(ts, "2026-04-16 14:23:16");
        assert_eq!(base_level, 87);
        assert_eq!(tnl, 0);
    }

    #[test]
    fn chat_level_up_preserves_unknown_columns() {
        let conn = setup();

        conn.execute(
            "INSERT INTO game_state_skills
                 (character_name, server_name, skill_id, skill_name, level, base_level, bonus_levels, xp, tnl, max_level, last_confirmed_at, source)
             VALUES ('TestPlayer','Dreva',106,'Old Name',86,80,3,100,5000,100,'2026-04-15 10:00:00','log')",
            [],
        )
        .unwrap();

        let changed =
            apply_chat_level_up(&ChatLevelUp { conn: &conn, character: "TestPlayer", server: "Dreva", skill_id: 106, skill_name: "Cartography", level: 87, xp: 192, confirmed_at: "2026-04-16 14:23:16" })
                .expect("apply level-up");
        assert!(changed);

        let (level, xp, source, ts, base_level, bonus, tnl, max, name): (i64, i64, String, String, i64, i64, i64, i64, String) = conn
            .query_row(
                "SELECT level, xp, source, last_confirmed_at, base_level, bonus_levels, tnl, max_level, skill_name FROM game_state_skills
                 WHERE character_name = 'TestPlayer' AND server_name = 'Dreva' AND skill_id = 106",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?, r.get(7)?, r.get(8)?)),
            )
            .expect("row exists");
        assert_eq!((level, xp), (87, 192));
        assert_eq!(source, "chat");
        assert_eq!(ts, "2026-04-16 14:23:16");
        assert_eq!((base_level, bonus, tnl, max), (80, 3, 5000, 100));
        assert_eq!(name, "Cartography");
    }

    #[test]
    fn chat_level_up_replay_is_idempotent() {
        let conn = setup();

        let first =
            apply_chat_level_up(&ChatLevelUp { conn: &conn, character: "TestPlayer", server: "Dreva", skill_id: 106, skill_name: "Cartography", level: 87, xp: 192, confirmed_at: "2026-04-16 14:23:16" })
                .expect("first apply");
        assert!(first);
        // Same values re-applied at a later timestamp: the recency guard lets
        // it through (only the timestamp moves).
        let replay =
            apply_chat_level_up(&ChatLevelUp { conn: &conn, character: "TestPlayer", server: "Dreva", skill_id: 106, skill_name: "Cartography", level: 87, xp: 192, confirmed_at: "2026-04-16 15:00:00" })
                .expect("replay apply");
        assert!(replay);

        let (level, xp, ts, count): (i64, i64, String, i64) = conn
            .query_row(
                "SELECT level, xp, last_confirmed_at, (SELECT COUNT(*) FROM game_state_skills
                 WHERE character_name = 'TestPlayer' AND server_name = 'Dreva' AND skill_id = 106)
                 FROM game_state_skills
                 WHERE character_name = 'TestPlayer' AND server_name = 'Dreva' AND skill_id = 106",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )
            .expect("row exists");
        assert_eq!((level, xp), (87, 192));
        assert_eq!(ts, "2026-04-16 15:00:00");
        assert_eq!(count, 1);
    }

    #[test]
    fn chat_level_up_stale_replay_is_dropped() {
        let conn = setup();

        apply_chat_level_up(&ChatLevelUp { conn: &conn, character: "TestPlayer", server: "Dreva", skill_id: 106, skill_name: "Cartography", level: 87, xp: 192, confirmed_at: "2026-04-16 15:00:00" })
            .expect("first apply");
        // A same-level line at an older timestamp replayed after newer state:
        // the guard drops it, and the timestamp must not move backward.
        // (Same-format stamps only: production chat stamps are UTC-converted
        // space-format vs `T…Z` snapshot stamps, so a same-day live level-up
        // always sorts older — only `excluded.level >` rescues it. This pins
        // the guard logic, not the wire formats.)
        let changed =
            apply_chat_level_up(&ChatLevelUp { conn: &conn, character: "TestPlayer", server: "Dreva", skill_id: 106, skill_name: "Cartography", level: 87, xp: 100, confirmed_at: "2026-04-16 14:23:16" })
                .expect("stale apply");
        assert!(!changed);

        let (level, xp, ts, source): (i64, i64, String, String) = conn
            .query_row(
                "SELECT level, xp, last_confirmed_at, source FROM game_state_skills
                 WHERE character_name = 'TestPlayer' AND server_name = 'Dreva' AND skill_id = 106",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )
            .expect("row exists");
        assert_eq!((level, xp), (87, 192));
        assert_eq!(ts, "2026-04-16 15:00:00");
        assert_eq!(source, "chat");
    }

    #[test]
    fn chat_level_up_wins_on_level_despite_older_clock() {
        let conn = setup();

        apply_chat_level_up(&ChatLevelUp { conn: &conn, character: "TestPlayer", server: "Dreva", skill_id: 106, skill_name: "Cartography", level: 87, xp: 192, confirmed_at: "2026-04-16 15:00:00" })
            .expect("first apply");
        // Genuine level-up with a skewed-older clock: chat is authoritative
        // that the level was reached, so the level wins anyway — but the
        // timestamp stays monotonic so a later stale replay can't downgrade.
        let changed =
            apply_chat_level_up(&ChatLevelUp { conn: &conn, character: "TestPlayer", server: "Dreva", skill_id: 106, skill_name: "Cartography", level: 88, xp: 10, confirmed_at: "2026-04-16 14:59:00" })
                .expect("level-up apply");
        assert!(changed);

        let (level, xp, ts): (i64, i64, String) = conn
            .query_row(
                "SELECT level, xp, last_confirmed_at FROM game_state_skills
                 WHERE character_name = 'TestPlayer' AND server_name = 'Dreva' AND skill_id = 106",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .expect("row exists");
        assert_eq!((level, xp), (88, 10));
        assert_eq!(ts, "2026-04-16 15:00:00");

        // A stale replay that would have won against the rewound clock is now
        // dropped: 14:59:30 is not newer than the monotonic 15:00:00.
        let stale =
            apply_chat_level_up(&ChatLevelUp { conn: &conn, character: "TestPlayer", server: "Dreva", skill_id: 106, skill_name: "Cartography", level: 87, xp: 100, confirmed_at: "2026-04-16 14:59:30" })
                .expect("stale apply");
        assert!(!stale);
    }

    #[test]
    fn currency_estimate_none_until_anchored_then_anchor_plus_delta() {
        let conn = setup();

        // No row yet (no export imported): None, so the UI prompts for one.
        assert!(get_currency_estimate_for(&conn, "Zen", "Dreva").unwrap().is_none());

        // Seeded row without an anchor (migration backfill shape): estimate
        // is not meaningful, flagged via has_anchor = false.
        conn.execute(
            "INSERT INTO currency_estimate (character_name, server_name, currency_name, anchor_amount, anchor_at, delta_since)
             VALUES ('Zen','Dreva','GOLD',0,NULL,150)",
            [],
        )
        .unwrap();
        let e = get_currency_estimate_for(&conn, "Zen", "Dreva").unwrap().unwrap();
        assert!(!e.has_anchor);
        assert_eq!(e.estimated, 150);

        // Anchored row: estimated = anchor_amount + delta_since, and another
        // character's row never leaks in.
        conn.execute(
            "UPDATE currency_estimate SET anchor_amount = 5000, anchor_at = '2026-10-26 12:00:00', delta_since = -320
             WHERE character_name = 'Zen' AND server_name = 'Dreva' AND currency_name = 'GOLD'",
            [],
        )
        .unwrap();
        let e = get_currency_estimate_for(&conn, "Zen", "Dreva").unwrap().unwrap();
        assert!(e.has_anchor);
        assert_eq!(e.anchor_at.as_deref(), Some("2026-10-26 12:00:00"));
        assert_eq!(e.estimated, 4680);
        assert!(get_currency_estimate_for(&conn, "Alt", "Dreva").unwrap().is_none());
    }
}
