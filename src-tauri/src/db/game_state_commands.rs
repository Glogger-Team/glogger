use super::{DbPool, DbRead};
use rusqlite::OptionalExtension;
use serde::{Deserialize, Serialize};
/// Tauri commands for querying persisted game state
use tauri::State;

// ── Response Types ──────────────────────────────────────────────────────────

#[derive(Serialize, specta::Type)]
pub struct GameStateSkill {
    #[specta(type = u32)]
    pub skill_id: i64,
    pub skill_name: String,
    pub level: i32,
    pub base_level: i32,
    pub bonus_levels: i32,
    #[specta(type = u32)]
    pub xp: i64,
    #[specta(type = u32)]
    pub tnl: i64,
    pub max_level: i32,
    pub last_confirmed_at: String,
    pub source: String,
}

#[derive(Serialize, specta::Type)]
pub struct GameStateAttribute {
    pub attribute_name: String,
    pub value: f64,
    pub last_confirmed_at: String,
}

#[derive(Serialize, specta::Type)]
pub struct GameStateActiveSkills {
    #[specta(type = u32)]
    pub skill1_id: i64,
    pub skill1_name: String,
    #[specta(type = u32)]
    pub skill2_id: i64,
    pub skill2_name: String,
    pub last_confirmed_at: String,
}

#[derive(Serialize, specta::Type)]
pub struct GameStateWorld {
    pub weather: Option<GameStateWeather>,
    pub combat: Option<GameStateCombat>,
    pub mount: Option<GameStateMount>,
    pub area: Option<GameStateArea>,
}

#[derive(Serialize, specta::Type)]
pub struct GameStateArea {
    pub area_name: String,
    pub last_confirmed_at: String,
}

#[derive(Serialize, specta::Type)]
pub struct GameStateWeather {
    pub weather_name: String,
    pub is_active: bool,
    pub last_confirmed_at: String,
}

#[derive(Serialize, specta::Type)]
pub struct GameStateCombat {
    pub in_combat: bool,
    pub last_confirmed_at: String,
}

#[derive(Serialize, specta::Type)]
pub struct GameStateMount {
    pub is_mounted: bool,
    pub last_confirmed_at: String,
}

#[derive(Serialize, specta::Type)]
pub struct GameStateInventoryItem {
    #[specta(type = u32)]
    pub instance_id: i64,
    pub item_name: String,
    pub item_type_id: Option<i32>,
    pub stack_size: i32,
    pub slot_index: i32,
    pub last_confirmed_at: String,
    pub source: String,
}

#[derive(Serialize, specta::Type)]
pub struct GameStateRecipe {
    pub recipe_id: i32,
    pub completion_count: i32,
    pub last_confirmed_at: String,
    pub source: String,
}

#[derive(Serialize, specta::Type)]
pub struct GameStateEquipmentSlot {
    pub slot: String,
    pub appearance_key: String,
    pub last_confirmed_at: String,
}

#[derive(Serialize, specta::Type)]
pub struct GameStateFavor {
    pub npc_key: String,
    pub npc_name: String,
    pub session_delta: f64,
    pub favor_tier: Option<String>,
    pub last_confirmed_at: String,
    pub source: String,
}

#[derive(Serialize, specta::Type)]
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
#[derive(Serialize, specta::Type)]
pub struct CurrencyEstimate {
    pub currency_name: String,
    #[specta(type = u32)]
    pub anchor_amount: i64,
    pub anchor_at: Option<String>,
    #[specta(type = i32)]
    pub delta_since: i64,
    #[specta(type = i32)]
    pub estimated: i64,
    pub has_anchor: bool,
}

#[derive(Serialize, specta::Type)]
pub struct GameStateEffect {
    #[specta(type = u32)]
    pub effect_instance_id: i64,
    pub effect_name: Option<String>,
    #[specta(type = u32)]
    pub source_entity_id: i64,
    pub last_confirmed_at: String,
}

#[derive(Serialize, specta::Type)]
pub struct GameStateStorageItem {
    pub vault_key: String,
    #[specta(type = u32)]
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
    let session = session_favor_sums(&conn, &character_name, &server_name)?;
    let mut stmt = conn
        .prepare(
            "SELECT npc_key, npc_name, favor_tier, last_confirmed_at, source
         FROM game_state_favor WHERE character_name = ?1 AND server_name = ?2
         ORDER BY npc_name",
        )
        .map_err(|e| format!("Query error: {e}"))?;

    let rows = stmt
        .query_map(rusqlite::params![character_name, server_name], |row| {
            let npc_key: String = row.get(0)?;
            Ok(GameStateFavor {
                session_delta: session.get(&npc_key).copied().unwrap_or(0.0),
                npc_key,
                npc_name: row.get(1)?,
                favor_tier: row.get(2)?,
                last_confirmed_at: row.get(3)?,
                source: row.get(4)?,
            })
        })
        .map_err(|e| format!("Query error: {e}"))?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("Row error: {e}"))
}

/// Session favor sums: per-NPC total of `favor_events` deltas since the
/// active session started. Both sides are UTC: the live drain converts Chat
/// timestamps before dispatch and the scan path passes UTC messages, while
/// the session singleton stores `Utc::now()` — so the boundary binds
/// directly with no frame conversion. No session row, a row for a different
/// character (viewing a non-active character), or a missing events table
/// (minimal fixtures) → empty map, i.e. all zeros.
/// Covers capped (Statehelm) and uncapped NPCs alike — it sums every delta,
/// while gift *detection* stays exclusions-aware at write time.
pub fn session_favor_sums(
    conn: &impl DbRead,
    character_name: &str,
    server_name: &str,
) -> Result<std::collections::HashMap<String, f64>, String> {
    let has_events: bool = conn
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'favor_events')",
            [],
            |r| r.get(0),
        )
        .unwrap_or(false);
    if !has_events {
        return Ok(std::collections::HashMap::new());
    }
    let start: Option<String> = conn
        .query_row(
            "SELECT last_login_at FROM game_state_session
             WHERE id = 1 AND character_name = ?1 AND server_name = ?2",
            rusqlite::params![character_name, server_name],
            |r| r.get(0),
        )
        .optional()
        .map_err(|e| format!("Query error: {e}"))?;
    let Some(start) = start else {
        return Ok(std::collections::HashMap::new());
    };
    let mut stmt = conn
        .prepare(
            "SELECT npc_key, SUM(delta) FROM favor_events
             WHERE character_name = ?1 AND server_name = ?2 AND timestamp >= ?3
             GROUP BY npc_key",
        )
        .map_err(|e| format!("Query error: {e}"))?;
    let rows = stmt
        .query_map(
            rusqlite::params![character_name, server_name, start],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, f64>(1)?)),
        )
        .map_err(|e| format!("Query error: {e}"))?;
    rows.collect::<Result<std::collections::HashMap<_, _>, _>>()
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
/// zero-padded fixed-width stamps sort chronologically. The snapshot/chat
/// stamp asymmetry now lives in the XP anchor: the seeder normalizes the
/// export stamp to space-format (`xp_snapshot_at`), so the award-vs-anchor
/// compare is same-format — see the cross-format note on
/// `apply_chat_xp_award` (which doesn't touch this table).
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

#[derive(Serialize, specta::Type)]
pub struct TrackedSkill {
    pub skill_name: String,
    pub sort_order: i32,
}

#[derive(Deserialize, specta::Type)]
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

// ── Chat Level-Ups ────────────────────────────────────────────────────────────

/// Upsert one skill row from a chat `[Status]` level-up line
/// ("You earned N XP and reached level L in Skill!").
/// Post-10/21/2026 this is the only live skill signal
/// (`ProcessUpdateSkill` lines are gone from Player.log). The carried XP is
/// an awarded increment, not a total — same-level awards accumulate onto
/// the stored value, and an award that raises the level stores just the
/// award (no snapshot-seeded balance carries forward into the new level).
/// Basis rule: report snapshots seed `xp` as the absolute within-level
/// figure (`xp_toward_next_level`); chat carries only level + this-level XP,
/// so the upsert preserves `base_level/bonus_levels/tnl/max_level` on
/// conflict and always refreshes `skill_name` to the canonical display
/// name (a fresh insert seeds `base_level = level`, `tnl/max = 0`).
/// Identity rule (v101 `chat_xp_awards`): every applied award lands one
/// ledger row keyed on (character, server, skill, timestamp, xp, level).
/// A replay or twin finds its row and skips (no double-count); a
/// same-second or out-of-order award with a distinct identity applies —
/// the old recency guard dropped both (indistinguishable from replays
/// under a timestamp-only key). Same-timestamp same-xp same-level pairs
/// still collapse: second-granularity logs give no key separating a
/// genuine duplicate from its own replay, and the only same-content
/// same-second pairs observable are twin-or-replay (English non-level XP
/// lines carry no writer).
/// Anchor rule (`xp_snapshot_at`): a same-level award at or before the
/// snapshot anchor is already inside the seeded absolute and drops; newer
/// awards accumulate on top. A level-up clears the anchor (it described
/// the previous level's absolute — the new level has no snapshot yet),
/// so post-ding awards are ledger-gated only. The unconditional winner
/// is the genuine level-up (`excluded.level >` stored level always wins
/// — chat is authoritative that the player *reached* that level, even
/// under clock skew, and even at or before the anchor).
/// Cross-format note: `last_confirmed_at` keeps the raw export stamp
/// (space-format chat stamps and `…Z` snapshot stamps coexist), so its
/// max() only orders same-format stamps; the award-vs-anchor compare
/// stays same-format because the seeder normalizes the anchor.
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
    apply_chat_xp_award(&ChatXpAward {
        conn: l.conn,
        character: l.character,
        server: l.server,
        skill_id: l.skill_id,
        skill_name: l.skill_name,
        level: l.level,
        xp: l.xp,
        confirmed_at: l.confirmed_at,
    })
}

/// One chat-observed XP award (level-up or within-level): same shape as
/// [`ChatLevelUp`]. Both writers (`apply_chat_level_up` for level-ups,
/// `upsert_chat_xp` for within-level JSON awards) were byte-identical
/// recency-gated upserts; both now delegate here so the ledger + anchor
/// gates live in exactly one place.
pub(crate) struct ChatXpAward<'a> {
    pub conn: &'a rusqlite::Connection,
    pub character: &'a str,
    pub server: &'a str,
    pub skill_id: i64,
    pub skill_name: &'a str,
    pub level: i64,
    pub xp: i64,
    pub confirmed_at: &'a str,
}

/// Apply one chat XP award with the v101 identity + anchor gates.
/// Returns true when the skills row was inserted or updated.
pub(crate) fn apply_chat_xp_award(a: &ChatXpAward<'_>) -> Result<bool, String> {
    let stored: Option<(i64, i64, String, Option<String>)> = a
        .conn
        .query_row(
            "SELECT level, xp, last_confirmed_at, xp_snapshot_at FROM game_state_skills
             WHERE character_name = ?1 AND server_name = ?2 AND skill_id = ?3",
            rusqlite::params![a.character, a.server, a.skill_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .optional()
        .map_err(|e| format!("Failed to read chat XP row: {e}"))?;
    if let Some((stored_level, _, _, anchor)) = &stored {
        // Level gate (unchanged): a lower level never rewinds the row — a
        // newer-stamped first-XP award arriving after the skill is known,
        // or any older-level line, drops whole.
        if a.level < *stored_level {
            return Ok(false);
        }
        // Anchor gate: a same-level award at or before the snapshot anchor
        // is already inside the seeded absolute and drops. Level-ups skip
        // this gate (chat stays authoritative that the level was reached).
        if a.level == *stored_level
            && anchor.as_deref().is_some_and(|anc| a.confirmed_at <= anc)
        {
            return Ok(false);
        }
    }
    // Single write transaction for the ledger + skills unit: a failure
    // between them rolls back, so a later replay retries the whole award
    // instead of skipping an unapplied one (same unchecked-transaction
    // shape as the favor arm: the pool checkout is exclusive and no outer
    // transaction is open here).
    let tx = rusqlite::Transaction::new_unchecked(
        a.conn,
        rusqlite::TransactionBehavior::Immediate,
    )
    .map_err(|e| format!("Failed to begin chat XP award txn: {e}"))?;
    // Ledger: a replay or twin-second finds its identity row and skips
    // before touching the total — this is the no-double-count gate the
    // old recency WHERE clause used to be. Only applied awards land here:
    // gate-dropped lines above leave no row and re-evaluate the same
    // gates on replay.
    let fresh = tx
        .execute(
            "INSERT OR IGNORE INTO chat_xp_awards
                (character_name, server_name, skill_id, awarded_at, xp, level)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            rusqlite::params![
                a.character,
                a.server,
                a.skill_id,
                a.confirmed_at,
                a.xp,
                a.level
            ],
        )
        .map_err(|e| format!("Failed to ledger chat XP award: {e}"))?;
    if fresh == 0 {
        return Ok(false);
    }
    let Some((stored_level, stored_xp, _, _)) = stored else {
        // Fresh skill: seed the row around the award (anchor stays NULL —
        // no snapshot absolute exists yet to subsume anything).
        tx.execute(
            "INSERT INTO game_state_skills
               (character_name, server_name, skill_id, skill_name, level, base_level, bonus_levels, xp, tnl, max_level, last_confirmed_at, source)
             VALUES (?1, ?2, ?3, ?4, ?5, ?5, 0, ?6, 0, 0, ?7, 'chat')",
            rusqlite::params![
                a.character,
                a.server,
                a.skill_id,
                a.skill_name,
                a.level,
                a.xp,
                a.confirmed_at
            ],
        )
        .map_err(|e| format!("Failed to apply chat XP award: {e}"))?;
        tx.commit()
            .map_err(|e| format!("Failed to commit chat XP award: {e}"))?;
        return Ok(true);
    };
    // Level-up resets to the award (no prior-level progress carries
    // forward); same-level accumulates. The clock stays monotonic —
    // `last_confirmed_at` is the max of stored and incoming.
    let new_xp = if a.level > stored_level {
        a.xp
    } else {
        stored_xp + a.xp
    };
    tx.execute(
        "UPDATE game_state_skills SET skill_name = ?4, level = ?5, xp = ?6,
            last_confirmed_at = CASE WHEN ?7 > last_confirmed_at THEN ?7 ELSE last_confirmed_at END,
            xp_snapshot_at = CASE WHEN ?5 > level THEN NULL ELSE xp_snapshot_at END,
            source = 'chat'
         WHERE character_name = ?1 AND server_name = ?2 AND skill_id = ?3",
        rusqlite::params![
            a.character,
            a.server,
            a.skill_id,
            a.skill_name,
            a.level,
            new_xp,
            a.confirmed_at
        ],
    )
    .map_err(|e| format!("Failed to apply chat XP award: {e}"))?;
    tx.commit()
        .map_err(|e| format!("Failed to commit chat XP award: {e}"))?;
    Ok(true)
}

#[derive(Serialize, specta::Type)]
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

#[derive(Serialize, specta::Type)]
pub struct GameStateVendor {
    pub npc_key: String,
    #[specta(type = Option<u32>)]
    pub vendor_gold_available: Option<i64>,
    #[specta(type = Option<u32>)]
    pub vendor_gold_max: Option<i64>,
    pub vendor_gold_timer_start: Option<String>,
    pub last_interaction_at: Option<String>,
    pub last_sell_at: Option<String>,
    pub last_confirmed_at: String,
    #[specta(type = Option<u32>)]
    pub councils_earned_current: Option<i64>,
    #[specta(type = Option<u32>)]
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

/// Record a **known** cow milk: the cooldown starts at `milked_at` (the milk
/// event's own UTC time) and the "unknown" flag is cleared, because this is the
/// truth the bound was only approximating.
///
/// The stamp only ever moves forward (`MAX`), so replaying an old chat log can't
/// hand out a fresh hour; the zone merge still learns a previously-unknown area.
///
/// Returns `true` when the upsert advanced the row (a new row, a newer stamp,
/// a cleared `cooldown_unknown` flag, or a newly learned zone) — the caller
/// emits `milking` only then, so replays that move nothing stay silent. The
/// comparison mirrors the upsert below exactly (RFC3339 UTC stamps compare
/// lexicographically, same as SQL `MAX` on `TEXT`).
pub fn record_cow_milk(
    conn: &rusqlite::Connection,
    character: &str,
    server: &str,
    cow_name: &str,
    zone: &str,
    milked_at: &str,
) -> Result<bool, String> {
    let advanced: bool = match conn.query_row(
        "SELECT last_milked_at, cooldown_unknown, zone FROM milking_timers
         WHERE character_name = ?1 AND server_name = ?2 AND cow_name = ?3",
        rusqlite::params![character, server, cow_name],
        |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, String>(2)?,
            ))
        },
    ) {
        Err(rusqlite::Error::QueryReturnedNoRows) => true,
        Err(e) => return Err(format!("Failed to read cow milk row: {e}")),
        Ok((stamp, unknown, stored_zone)) => {
            milked_at > stamp.as_str() || (unknown == 1 && milked_at >= stamp.as_str()) || (!zone.is_empty() && zone != stored_zone)
        }
    };
    conn.execute(
        "INSERT INTO milking_timers
             (character_name, server_name, cow_name, zone, last_milked_at, cooldown_unknown)
         VALUES (?1, ?2, ?3, ?4, ?5, 0)
         ON CONFLICT(character_name, server_name, cow_name) DO UPDATE SET
            last_milked_at = MAX(excluded.last_milked_at, milking_timers.last_milked_at),
            cooldown_unknown = CASE WHEN excluded.last_milked_at >= milking_timers.last_milked_at THEN 0 ELSE milking_timers.cooldown_unknown END,
            zone = CASE WHEN excluded.zone != '' THEN excluded.zone ELSE milking_timers.zone END",
        rusqlite::params![character, server, cow_name, zone, milked_at],
    )
    .map_err(|e| format!("Failed to record cow milk: {e}"))?;
    Ok(advanced)
}

// ── Character stats ─────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize, specta::Type)]
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

#[derive(Debug, Clone, Serialize, specta::Type)]
pub struct PlayerMilkingEntry {
    pub player_name: String,
    #[specta(type = u32)]
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
#[derive(Debug, Clone, Serialize, specta::Type)]
pub struct PoopingStats {
    #[specta(type = u32)]
    pub total_poops: i64,
    #[specta(type = u32)]
    pub days_pooped: i64,
    pub first_poop: Option<String>,
    pub last_poop: Option<String>,
    #[specta(type = Option<u32>)]
    pub skill_level: Option<i64>,
    #[specta(type = Option<u32>)]
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

// ── Timestamp contract: space-format naive UTC ─────────────────────────────
///
/// Every timestamp this module writes or compares is `"YYYY-MM-DD HH:MM:SS"`
/// naive UTC (the chat pipeline's shape — what all SQL predicates assume).
/// Two legacy families reach these writers: RFC3339 (`2026-05-02T03:59:25.322`
/// `+00:00`, chrono convenience in later modules) and time-only (`19:29:10`,
/// Player.log era). `normalize_contract_ts` maps the T-family onto the
/// contract (fraction + offset truncated — the contract is
/// second-granularity and every RFC3339 writer is chrono-`Utc`, so the
/// offset is always `+00:00`/`Z`); anything else passes through
/// byte-identical rather than being dropped or invented — a time-only stamp
/// has no recoverable date, and inventing one would silently corrupt
/// ordering. Migration v98 backfills the stored T-family rows; time-only
/// rows stay until a date source exists, and the window predicates that
/// must match them are family-aware (`kill_tracking_commands.rs`).
///
/// `parse_contract_ts` is the read side: fixed-format Rust parses must
/// accept both live families explicitly — never silently drop the other one.
pub(crate) fn parse_contract_ts(s: &str) -> Option<chrono::NaiveDateTime> {
    if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S") {
        return Some(dt);
    }
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(s) {
        return Some(dt.naive_utc());
    }
    chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S").ok()
}

pub(crate) fn normalize_contract_ts(s: &str) -> String {
    match parse_contract_ts(s) {
        Some(dt) => dt.format("%Y-%m-%d %H:%M:%S").to_string(),
        None => s.to_string(),
    }
}

// ── Harvest Almanac ────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, specta::Type)]
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

/// One harvesting focus parsed from an Emotion-Harvesting Almanac body.
pub struct HarvestFocus {
    pub monster: String,
    pub description: Option<String>,
    pub is_current: bool,
    pub event_start: Option<String>,
    pub event_end: Option<String>,
}

/// Snapshot-comparison key for one harvest focus row.
type HarvestFocusKey = (String, Option<String>, i64, Option<String>, Option<String>);

/// Strip `<...>` markup from an almanac block, leaving the prose.
fn strip_almanac_tags(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_tag = false;
    for c in s.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Parse an Emotion-Harvesting Almanac `JSON:Book` body (observed 2026-11-24)
/// into per-monster focuses. Current/upcoming split comes from the section
/// headers; `event_start`/`event_end` are absolute (`%Y-%m-%d %H:%M:%S`)
/// computed from `captured_at` plus the relative text (NULL when unparseable).
/// `captured_at` accepts both contract families (space-format and RFC3339) —
/// a T-family capture must still yield event math, never silent NULLs.
pub fn parse_harvest_almanac_body(body: &str, captured_at: &str) -> Vec<HarvestFocus> {
    let upcoming_at = body.find("Upcoming Harvesting Focuses:").unwrap_or(usize::MAX);
    let base = parse_contract_ts(captured_at);
    let mut out = Vec::new();
    let mut search_from = 0usize;
    while let Some(rel) = body[search_from..].find("<h2>") {
        let h2 = search_from + rel;
        let title_start = h2 + "<h2>".len();
        let Some(end_rel) = body[title_start..].find("</h2>") else {
            break;
        };
        let title = &body[title_start..title_start + end_rel];
        let rest_start = title_start + end_rel + "</h2>".len();
        // Next <h2> (or end of body) bounds this focus's block.
        let block_end = body[rest_start..]
            .find("<h2>")
            .map(|r| rest_start + r)
            .unwrap_or(body.len());
        let block = &body[rest_start..block_end];
        search_from = block_end;
        let monster = title.trim();
        if monster.is_empty() {
            continue;
        }
        let timing = block
            .find("<i>")
            .and_then(|s| {
                block[s + "<i>".len()..]
                    .find("</i>")
                    .map(|e| block[s + "<i>".len()..s + "<i>".len() + e].trim().to_string())
            })
            .filter(|s| !s.is_empty());
        // Description is the prose before the timing note, tags stripped.
        let prose_end = block.find("<i>").unwrap_or(block.len());
        let prose = strip_almanac_tags(&block[..prose_end]);
        let description = if prose.is_empty() { None } else { Some(prose) };
        let is_current = h2 < upcoming_at;
        let (event_start, event_end) = match timing.as_deref() {
            Some(t) if t.starts_with("Starts in ") => (
                relative_to(base, t.trim_start_matches("Starts in ")),
                None,
            ),
            Some(t) if t.starts_with("Ends in ") => (
                None,
                relative_to(base, t.trim_start_matches("Ends in ")),
            ),
            _ => (None, None),
        };
        out.push(HarvestFocus {
            monster: monster.to_string(),
            description,
            is_current,
            event_start,
            event_end,
        });
    }
    out
}

/// Replace a character's harvest snapshot from an almanac body. Returns the
/// number of rows written: identical re-imports (rescans) and unparseable
/// bodies are pure no-ops returning 0. There is no harvest history table
/// (unlike the garden path), so only the snapshot is replaced.
pub fn import_harvest_almanac(
    conn: &rusqlite::Connection,
    character: &str,
    server: &str,
    body: &str,
    captured_at: &str,
) -> Result<usize, String> {
    // Contract: persist space-format naive UTC. A T-family capture normalizes
    // here so the snapshot, the recency guard below, and the event math all
    // share one shape (migration v98 backfills the stored rows).
    let normalized = normalize_contract_ts(captured_at);
    let captured_at: &str = &normalized;
    // A truncated (unclosed) <h2> means a partial parse — refuse to replace
    // the existing snapshot (no-op, Ok(0)) rather than deleting omitted focuses.
    if body.rfind("<h2>").map(|h| !body[h..].contains("</h2>")).unwrap_or(false) {
        return Ok(0);
    }
    let focuses = parse_harvest_almanac_body(body, captured_at);
    if focuses.is_empty() {
        // Never wipe a good snapshot on an unparseable body. Tradeoff: a
        // genuinely empty almanac (never observed) would also no-op instead
        // of clearing — clearing needs a distinct parsed-empty signal.
        return Ok(0);
    }
    // Identical re-import (rescan) is a pure no-op: no snapshot churn.
    let current: Vec<HarvestFocusKey> = conn
        .prepare(
            "SELECT monster_name, description, is_current, event_start, event_end
             FROM harvest_almanac WHERE character_name = ?1 AND server_name = ?2 ORDER BY monster_name",
        )
        .map_err(|e| format!("Failed to read harvest snapshot: {e}"))?
        .query_map(rusqlite::params![character, server], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?))
        })
        .map_err(|e| format!("Failed to read harvest snapshot: {e}"))?
        .collect::<Result<_, _>>()
        .map_err(|e| format!("Failed to read harvest snapshot: {e}"))?;
    let mut fresh: Vec<HarvestFocusKey> = focuses
        .iter()
        .map(|f| {
            (
                f.monster.clone(),
                f.description.clone(),
                f.is_current as i64,
                f.event_start.clone(),
                f.event_end.clone(),
            )
        })
        .collect();
    fresh.sort();
    if current == fresh {
        return Ok(0);
    }
    // Recency: a rescan can redeliver an OLDER identical Book after a newer
    // import (newest-first scans). Its content differs (older focuses), so
    // the check above passes — but replacing the snapshot would move the
    // harvest clock backward. Captures sort lexicographically (naive-UTC
    // stamps on both sides), so compare as text. Equal captures stay
    // eligible (a failed import of the current observation can still be
    // retried).
    let stored_newest: Option<String> = conn
        .query_row(
            "SELECT MAX(captured_at) FROM harvest_almanac
             WHERE character_name = ?1 AND server_name = ?2",
            rusqlite::params![character, server],
            |r| r.get(0),
        )
        .map_err(|e| format!("Failed to read harvest capture time: {e}"))?;
    if stored_newest.as_deref().is_some_and(|s| captured_at < s) {
        return Ok(0);
    }
    // One transaction: a crash or error mid-replace must not leave a partial
    // snapshot (the pool is the only lock, so BEGIN IMMEDIATE is enough).
    conn.execute_batch("BEGIN IMMEDIATE")
        .map_err(|e| format!("Failed to begin harvest import: {e}"))?;
    let replaced = (|| -> Result<usize, String> {
        conn.execute(
            "DELETE FROM harvest_almanac WHERE character_name = ?1 AND server_name = ?2",
            rusqlite::params![character, server],
        )
        .map_err(|e| format!("Failed to clear harvest snapshot: {e}"))?;
        let mut n = 0;
        for f in &focuses {
            conn.execute(
                "INSERT INTO harvest_almanac
                    (character_name, server_name, monster_name, description, event_start, event_end, is_current, captured_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                rusqlite::params![
                    character,
                    server,
                    f.monster,
                    f.description,
                    f.event_start,
                    f.event_end,
                    f.is_current as i64,
                    captured_at
                ],
            )
            .map_err(|e| format!("Failed to insert harvest focus: {e}"))?;
            n += 1;
        }
        Ok(n)
    })();
    match replaced {
        Ok(n) => conn
            .execute_batch("COMMIT")
            .map_err(|e| {
                // A failed COMMIT leaves the transaction active; roll back
                // deterministically instead of relying on connection teardown.
                let _ = conn.execute_batch("ROLLBACK");
                format!("Failed to commit harvest import: {e}")
            })
            .map(|_| n),
        Err(e) => {
            let _ = conn.execute_batch("ROLLBACK");
            Err(e)
        }
    }
}

/// One gardening season parsed from a Gardening Almanac body.
pub struct GardenSeason {
    pub crop: String,
    pub zone: String,
    pub is_current: bool,
    pub event_start: Option<String>,
    pub event_end: Option<String>,
}

/// Snapshot-comparison key for one garden season row.
type GardenSeasonKey = (String, String, i64, Option<String>, Option<String>);

/// Parse a Gardening Almanac `JSON:Book` body (observed 2026-11-17) into
/// per-crop seasons. Current/upcoming split comes from the section headers;
/// `event_start`/`event_end` are absolute (`%Y-%m-%d %H:%M:%S`) computed from
/// `captured_at` plus the relative text (NULL when unparseable).
/// `captured_at` accepts both contract families (space-format and RFC3339) —
/// a T-family capture must still yield event math, never silent NULLs.
pub fn parse_garden_almanac_body(body: &str, captured_at: &str) -> Vec<GardenSeason> {
    let upcoming_at = body.find("Upcoming Gardening Events:").unwrap_or(usize::MAX);
    let base = parse_contract_ts(captured_at);
    let mut out = Vec::new();
    let mut search_from = 0usize;
    while let Some(rel) = body[search_from..].find("<h2>") {
        let h2 = search_from + rel;
        let title_start = h2 + "<h2>".len();
        let Some(end_rel) = body[title_start..].find("</h2>") else {
            break;
        };
        let title = &body[title_start..title_start + end_rel];
        let rest_start = title_start + end_rel + "</h2>".len();
        // Next <h2> (or end of body) bounds this event's block.
        let block_end = body[rest_start..]
            .find("<h2>")
            .map(|r| rest_start + r)
            .unwrap_or(body.len());
        let block = &body[rest_start..block_end];
        search_from = block_end;
        let Some((crop, zone)) = title.split_once(" Season in ") else {
            continue;
        };
        let timing = block
            .find("<i>")
            .and_then(|s| {
                block[s + "<i>".len()..]
                    .find("</i>")
                    .map(|e| block[s + "<i>".len()..s + "<i>".len() + e].trim().to_string())
            })
            .filter(|s| !s.is_empty());
        let is_current = h2 < upcoming_at;
        let (event_start, event_end) = match timing.as_deref() {
            Some(t) if t.starts_with("Starts in ") => (
                relative_to(base, t.trim_start_matches("Starts in ")),
                None,
            ),
            Some(t) if t.starts_with("Ends in ") => (
                None,
                relative_to(base, t.trim_start_matches("Ends in ")),
            ),
            _ => (None, None),
        };
        out.push(GardenSeason {
            crop: crop.trim().to_string(),
            zone: zone.trim().to_string(),
            is_current,
            event_start,
            event_end,
        });
    }
    out
}

/// Add a parsed relative duration ("6 hours 12 minutes", "1 day 6 hours")
/// to a base timestamp. Returns `None` when either side is unparseable.
/// `base` is the contract-family parse of the capture stamp (`None` when the
/// capture itself is unparseable — the callers then yield NULL event bounds,
/// never a silently wrong absolute time).
fn relative_to(base: Option<chrono::NaiveDateTime>, rel: &str) -> Option<String> {
    let base = base?;
    let mut secs: i64 = 0;
    let mut num: Option<i64> = None;
    for tok in rel.split_whitespace() {
        if let Ok(n) = tok.parse::<i64>() {
            num = Some(n);
            continue;
        }
        let unit = tok.trim_end_matches('s');
        let mult = match unit {
            "day" => 86_400,
            "hour" => 3_600,
            "minute" => 60,
            _ => {
                num = None;
                continue;
            }
        };
        if let Some(n) = num.take() {
            let add = n.checked_mul(mult)?;
            secs = secs.checked_add(add)?;
        }
    }
    if secs <= 0 {
        return None;
    }
    let dt = base.checked_add_signed(chrono::Duration::try_seconds(secs)?)?;
    Some(dt.format("%Y-%m-%d %H:%M:%S").to_string())
}

/// Replace a character's garden snapshot from an almanac body. Returns the
/// number of rows written: identical re-imports (rescans) and unparseable
/// bodies are pure no-ops returning 0. There is no garden history table
/// (v102 dropped the dead `garden_almanac_history` surface), so only the
/// snapshot is replaced — mirroring the harvest path.
pub fn import_garden_almanac(
    conn: &rusqlite::Connection,
    character: &str,
    server: &str,
    body: &str,
    captured_at: &str,
) -> Result<usize, String> {
    // Contract: persist space-format naive UTC. A T-family capture normalizes
    // here so the snapshot, the history rows, and the recency guard below all
    // share one shape (migration v98 backfills the stored rows). Without this
    // a T-family capture sorts *after* same-day space rows at position 10
    // (`T` > ` `) and the recency guard below misfires.
    let normalized = normalize_contract_ts(captured_at);
    let captured_at: &str = &normalized;
    // A truncated (unclosed) <h2> means a partial parse — refuse to replace
    // the existing snapshot (no-op, Ok(0)) rather than deleting omitted seasons.
    if body.rfind("<h2>").map(|h| !body[h..].contains("</h2>")).unwrap_or(false) {
        return Ok(0);
    }
    let seasons = parse_garden_almanac_body(body, captured_at);
    if seasons.is_empty() {
        // Never wipe a good snapshot on an unparseable body. Tradeoff: a
        // genuinely empty almanac (never observed) would also no-op instead
        // of clearing — clearing needs a distinct parsed-empty signal.
        return Ok(0);
    }
    // Identical re-import (rescan) is a pure no-op: no snapshot churn, no
    // duplicate history rows.
    let current: Vec<GardenSeasonKey> = conn
        .prepare(
            "SELECT crop_name, zone_name, is_current, event_start, event_end
             FROM garden_almanac WHERE character_name = ?1 AND server_name = ?2 ORDER BY crop_name, zone_name",
        )
        .map_err(|e| format!("Failed to read garden snapshot: {e}"))?
        .query_map(rusqlite::params![character, server], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?))
        })
        .map_err(|e| format!("Failed to read garden snapshot: {e}"))?
        .collect::<Result<_, _>>()
        .map_err(|e| format!("Failed to read garden snapshot: {e}"))?;
    let mut fresh: Vec<GardenSeasonKey> = seasons
        .iter()
        .map(|s| {
            (
                s.crop.clone(),
                s.zone.clone(),
                s.is_current as i64,
                s.event_start.clone(),
                s.event_end.clone(),
            )
        })
        .collect();
    fresh.sort();
    if current == fresh {
        return Ok(0);
    }
    // Recency: a rescan can redeliver an OLDER identical Book after a newer
    // import (newest-first scans). Its content differs (older seasons), so
    // the check above passes — but replacing the snapshot would move the
    // garden clock backward. Captures sort
    // lexicographically (naive-UTC stamps on both sides), so compare as
    // text. Equal captures stay eligible (a failed import of the current
    // observation can still be retried).
    let stored_newest: Option<String> = conn
        .query_row(
            "SELECT MAX(captured_at) FROM garden_almanac
             WHERE character_name = ?1 AND server_name = ?2",
            rusqlite::params![character, server],
            |r| r.get(0),
        )
        .map_err(|e| format!("Failed to read garden capture time: {e}"))?;
    if stored_newest.as_deref().is_some_and(|s| captured_at < s) {
        return Ok(0);
    }
    // One transaction: a crash or error mid-replace must not leave a partial
    // snapshot (the pool is the only lock, so BEGIN IMMEDIATE is enough).
    conn.execute_batch("BEGIN IMMEDIATE")
        .map_err(|e| format!("Failed to begin garden import: {e}"))?;
    let replaced = (|| -> Result<usize, String> {
        conn.execute(
            "DELETE FROM garden_almanac WHERE character_name = ?1 AND server_name = ?2",
            rusqlite::params![character, server],
        )
        .map_err(|e| format!("Failed to clear garden snapshot: {e}"))?;
        let mut n = 0;
        for s in &seasons {
            conn.execute(
                "INSERT INTO garden_almanac
                    (character_name, server_name, crop_name, zone_name, event_start, event_end, is_current, captured_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                rusqlite::params![
                    character, server, s.crop, s.zone, s.event_start, s.event_end,
                    s.is_current as i64, captured_at
                ],
            )
            .map_err(|e| format!("Failed to insert garden season: {e}"))?;
            n += 1;
        }
        Ok(n)
    })();
    match replaced {
        Ok(n) => conn
            .execute_batch("COMMIT")
            .map_err(|e| {
                // A failed COMMIT leaves the transaction active; roll back
                // deterministically instead of relying on connection teardown.
                let _ = conn.execute_batch("ROLLBACK");
                format!("Failed to commit garden import: {e}")
            })
            .map(|_| n),
        Err(e) => {
            let _ = conn.execute_batch("ROLLBACK");
            Err(e)
        }
    }
}
// ── Garden Almanac ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, specta::Type)]
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

// ── Computed Stats ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, specta::Type)]
pub struct SkillCraftingBreakdown {
    pub skill_name: String,
    #[specta(type = u32)]
    pub total_crafted: i64,
    pub crafting_seconds: f64,
}

#[derive(Debug, Clone, Serialize, specta::Type)]
pub struct AttributeExtreme {
    pub attribute_name: String,
    pub current_value: f64,
    pub min_value: f64,
    pub max_value: f64,
}

#[derive(Debug, Clone, Serialize, specta::Type)]
pub struct ComputedStats {
    #[specta(type = u32)]
    pub total_level: i64,
    #[specta(type = u32)]
    pub total_base_level: i64,
    #[specta(type = u32)]
    pub total_bonus_levels: i64,
    #[specta(type = u32)]
    pub skill_count: i64,
    #[specta(type = u32)]
    pub total_xp_earned: i64,
    #[specta(type = u32)]
    pub items_crafted: i64,
    #[specta(type = u32)]
    pub items_distilled: i64,
    #[specta(type = u32)]
    pub items_deconstructed: i64,
    #[specta(type = u32)]
    pub times_teleported: i64,
    #[specta(type = u32)]
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

#[derive(serde::Serialize, specta::Type)]
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

#[derive(Debug, Clone, Serialize, specta::Type)]
pub struct ItemOwnershipLocation {
    pub location: String,
    #[specta(type = u32)]
    pub count: i64,
}

#[derive(Debug, Clone, Serialize, specta::Type)]
pub struct ItemOwnershipInfo {
    #[specta(type = u32)]
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
    fn collect_milk_arm_records_named_cow_and_leaves_other_cows_alone() {
        // The Collect Milk arm takes &DbPool, so this drives the dispatch
        // path with a temp-file pool instead of the in-memory `setup()`
        // conn. Unique path per call: parallel tests share the temp dir.
        use std::sync::atomic::{AtomicU64, Ordering};
        static SEQ: AtomicU64 = AtomicU64::new(0);
        let n = SEQ.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "glogger_cow_milk_test_{}_{n}.db",
            std::process::id()
        ));
        let pool = crate::db::init_pool(path.clone(), Some(0)).expect("init pool");

        // A second cow with an approximated (unknown-cooldown) bound: milking
        // Daisy must not touch Bessie's row.
        pool.get_write()
            .unwrap()
            .execute(
                "INSERT INTO milking_timers
                     (character_name, server_name, cow_name, zone, last_milked_at, cooldown_unknown)
                 VALUES ('Alice', 'Dreva', 'Bessie', 'Serbule', '2026-04-01T05:00:00+00:00', 1)",
                [],
            )
            .unwrap();

        let game_data: crate::cdn_commands::GameDataState = std::sync::Arc::new(
            tauri::async_runtime::RwLock::new(crate::game_data::GameData::empty()),
        );
        let identity = crate::chat_status_dispatch::DispatchIdentity {
            character: "Alice".to_string(),
            server: "Dreva".to_string(),
        };
        let mut arena = crate::arena_parser::ArenaTracker::new();
        let mut dmg: Vec<crate::chat_combat_parser::ChatCombatEvent> = Vec::new();
        let mut milk = |cow: &str, ts: &str, area: Option<&str>| {
            let line = format!("[Combat] Alice: Collect Milk on {cow}!");
            let line = format!("{ts}\t{line}");
            let msg = crate::chat_parser::parse_chat_line(&line).unwrap();
            let mut st = crate::chat_status_dispatch::DispatchState {
                arena_tracker: &mut arena,
                recent_damage: &mut dmg,
                current_area: area.map(|s| s.to_string()),
                count_currency: true,
            };
            crate::chat_status_dispatch::dispatch_chat_message(
                crate::chat_status_dispatch::DispatchCtx {
                    db: &pool,
                    game_data: &game_data,
                    identity: Some(&identity),
                    msg: &msg,
                    local_ts: "2026-04-01 05:30:53",
                    watch_rules: &[],
                    // FORCED (out-of-slice): new `DispatchCtx` field.
                    source_line_id: None,
                    is_scan: false,
                    is_live: true,
                },
                None,
                &mut st,
            )
        };
        let row = |cow: &str| -> (String, i64, String) {
            pool.get_write()
                .unwrap()
                .query_row(
                    "SELECT last_milked_at, cooldown_unknown, zone FROM milking_timers
                     WHERE character_name = 'Alice' AND server_name = 'Dreva' AND cow_name = ?1",
                    rusqlite::params![cow],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
                )
                .unwrap()
        };

        // Fresh milk for Daisy in an unknown area: sets the stamp, clears
        // the unknown flag, keeps the blank zone — and reports `milking`.
        let out = milk("Daisy", "26-04-01 05:30:53", None);
        assert!(out.domains.contains(&"milking".to_string()));
        assert_eq!(
            row("Daisy"),
            (
                "2026-04-01T05:30:53+00:00".to_string(),
                0,
                String::new()
            )
        );
        // Bessie's approximated bound is untouched.
        assert_eq!(
            row("Bessie"),
            (
                "2026-04-01T05:00:00+00:00".to_string(),
                1,
                "Serbule".to_string()
            )
        );

        // An older replay with a now-known area must not move the stamp
        // backwards (MAX newer-wins) while the zone merge still learns it.
        milk("Daisy", "26-03-01 05:30:53", Some("Serbule"));
        assert_eq!(
            row("Daisy"),
            (
                "2026-04-01T05:30:53+00:00".to_string(),
                0,
                "Serbule".to_string()
            )
        );

        // A newer milk with an unknown area advances the stamp without
        // clobbering the learned zone.
        milk("Daisy", "26-05-01 05:30:53", None);
        assert_eq!(
            row("Daisy"),
            (
                "2026-05-01T05:30:53+00:00".to_string(),
                0,
                "Serbule".to_string()
            )
        );

        // Drop the pool first: Windows won't delete a file with an open handle.
        drop(pool);
        let _ = std::fs::remove_file(&path);
    }
    #[test]
    fn record_cow_milk_keeps_unknown_flag_on_older_milk() {
        // Pre-VIP-JSON bound row: the exact milk time is unknown, so the
        // widget renders an upper bound. An older known milk replayed after
        // the bound must not clear the flag into a false exact expiry.
        let conn = setup();
        conn.execute(
            "INSERT INTO milking_timers
                 (character_name, server_name, cow_name, zone, last_milked_at, cooldown_unknown)
             VALUES ('Alice', 'Dreva', 'Bessie', 'Serbule', '2026-04-01T05:00:00+00:00', 1)",
            [],
        )
        .unwrap();
        let row = || -> (String, i64) {
            conn.query_row(
                "SELECT last_milked_at, cooldown_unknown FROM milking_timers
                 WHERE character_name = 'Alice' AND server_name = 'Dreva' AND cow_name = 'Bessie'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap()
        };

        // Older known milk: stamp stays (MAX newer-wins), flag stays set,
        // and nothing advanced so the caller stays silent.
        assert!(!record_cow_milk(
            &conn,
            "Alice",
            "Dreva",
            "Bessie",
            "Serbule",
            "2026-04-01T04:00:00+00:00"
        )
        .unwrap());
        assert_eq!(
            row(),
            ("2026-04-01T05:00:00+00:00".to_string(), 1)
        );

        // Newer known milk: stamp advances and the bound resolves to exact.
        assert!(record_cow_milk(
            &conn,
            "Alice",
            "Dreva",
            "Bessie",
            "Serbule",
            "2026-04-01T06:00:00+00:00"
        )
        .unwrap());
        assert_eq!(
            row(),
            ("2026-04-01T06:00:00+00:00".to_string(), 0)
        );
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
    fn chat_level_up_resets_within_level_progress() {
        // A report-seeded within-level balance plus a later level-up award
        // must not sum into the new level: the award is the whole story.
        let conn = setup();
        conn.execute(
            "INSERT INTO game_state_skills
                 (character_name, server_name, skill_id, skill_name, level, base_level, bonus_levels, xp, tnl, max_level, last_confirmed_at, source)
             VALUES ('TestPlayer','Dreva',106,'Cartography',86,86,0,4900,0,0,'2026-04-16 12:00:00','report')",
            [],
        )
        .unwrap();
        let changed =
            apply_chat_level_up(&ChatLevelUp { conn: &conn, character: "TestPlayer", server: "Dreva", skill_id: 106, skill_name: "Cartography", level: 87, xp: 192, confirmed_at: "2026-04-16 14:23:16" })
                .expect("apply level-up");
        assert!(changed);
        let (level, xp): (i64, i64) = conn
            .query_row(
                "SELECT level, xp FROM game_state_skills
                 WHERE character_name = 'TestPlayer' AND server_name = 'Dreva' AND skill_id = 106",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .expect("row exists");
        assert_eq!((level, xp), (87, 192));
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
        // Byte-identical resend (same timestamp): the ledger identity drops
        // it, so the increment is not double-counted.
        let replay =
            apply_chat_level_up(&ChatLevelUp { conn: &conn, character: "TestPlayer", server: "Dreva", skill_id: 106, skill_name: "Cartography", level: 87, xp: 192, confirmed_at: "2026-04-16 14:23:16" })
                .expect("replay apply");
        assert!(!replay);
        // Same values at a LATER timestamp are indistinguishable from a
        // second identical award, so they accumulate (documented limit of
        // second-granularity logs).
        let second =
            apply_chat_level_up(&ChatLevelUp { conn: &conn, character: "TestPlayer", server: "Dreva", skill_id: 106, skill_name: "Cartography", level: 87, xp: 192, confirmed_at: "2026-04-16 15:00:00" })
                .expect("second apply");
        assert!(second);

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
        assert_eq!((level, xp), (87, 384));
        assert_eq!(ts, "2026-04-16 15:00:00");
        assert_eq!(count, 1);
    }

    #[test]
    fn chat_level_up_older_award_accumulates() {
        // v101: the per-award ledger replaces the recency guard, so a
        // same-level award with an older timestamp is a distinct award, not
        // a replay — it accumulates instead of dropping. The clock still
        // stays monotonic (`last_confirmed_at` is the max).
        let conn = setup();

        apply_chat_level_up(&ChatLevelUp { conn: &conn, character: "TestPlayer", server: "Dreva", skill_id: 106, skill_name: "Cartography", level: 87, xp: 192, confirmed_at: "2026-04-16 15:00:00" })
            .expect("first apply");
        let changed =
            apply_chat_level_up(&ChatLevelUp { conn: &conn, character: "TestPlayer", server: "Dreva", skill_id: 106, skill_name: "Cartography", level: 87, xp: 100, confirmed_at: "2026-04-16 14:23:16" })
                .expect("older apply");
        assert!(changed);

        let (level, xp, ts, source): (i64, i64, String, String) = conn
            .query_row(
                "SELECT level, xp, last_confirmed_at, source FROM game_state_skills
                 WHERE character_name = 'TestPlayer' AND server_name = 'Dreva' AND skill_id = 106",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )
            .expect("row exists");
        assert_eq!((level, xp), (87, 292));
        assert_eq!(ts, "2026-04-16 15:00:00");
        assert_eq!(source, "chat");
        // The ledger holds both award identities ...
        let n: i64 = conn
            .query_row("SELECT COUNT(*) FROM chat_xp_awards", [], |r| r.get(0))
            .expect("ledger count");
        assert_eq!(n, 2);
        // ... so replaying the older award now skips instead of re-adding.
        let replay =
            apply_chat_level_up(&ChatLevelUp { conn: &conn, character: "TestPlayer", server: "Dreva", skill_id: 106, skill_name: "Cartography", level: 87, xp: 100, confirmed_at: "2026-04-16 14:23:16" })
                .expect("replay apply");
        assert!(!replay);
        let xp: i64 = conn
            .query_row(
                "SELECT xp FROM game_state_skills
                 WHERE character_name = 'TestPlayer' AND server_name = 'Dreva' AND skill_id = 106",
                [],
                |r| r.get(0),
            )
            .expect("row exists");
        assert_eq!(xp, 292);
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
    fn chat_xp_anchor_drops_subsumed_same_level_award() {
        // Snapshot-shaped row: absolute xp seeded by an export, anchor at
        // the export stamp. Same-level awards at or before the anchor are
        // already inside the absolute and drop; newer ones accumulate.
        let conn = setup();
        conn.execute(
            "INSERT INTO game_state_skills
                (character_name, server_name, skill_id, skill_name, level, base_level, bonus_levels, xp, tnl, max_level, last_confirmed_at, xp_snapshot_at, source)
             VALUES ('TestPlayer','Dreva',106,'Cartography',87,87,0,4900,0,0,'2026-04-16 12:00:00','2026-04-16 12:00:00','snapshot')",
            [],
        )
        .unwrap();
        // At the anchor second: subsumed, drops.
        let subsumed =
            apply_chat_level_up(&ChatLevelUp { conn: &conn, character: "TestPlayer", server: "Dreva", skill_id: 106, skill_name: "Cartography", level: 87, xp: 100, confirmed_at: "2026-04-16 12:00:00" })
                .expect("subsumed apply");
        assert!(!subsumed);
        // Before the anchor, never seen: still subsumed (the export
        // absolute already includes it), drops.
        let older =
            apply_chat_level_up(&ChatLevelUp { conn: &conn, character: "TestPlayer", server: "Dreva", skill_id: 106, skill_name: "Cartography", level: 87, xp: 100, confirmed_at: "2026-04-16 11:00:00" })
                .expect("older apply");
        assert!(!older);
        // After the anchor: genuinely new, accumulates.
        let newer =
            apply_chat_level_up(&ChatLevelUp { conn: &conn, character: "TestPlayer", server: "Dreva", skill_id: 106, skill_name: "Cartography", level: 87, xp: 100, confirmed_at: "2026-04-16 13:00:00" })
                .expect("newer apply");
        assert!(newer);
        let (level, xp, ts, anchor): (i64, i64, String, String) = conn
            .query_row(
                "SELECT level, xp, last_confirmed_at, xp_snapshot_at FROM game_state_skills
                 WHERE character_name = 'TestPlayer' AND server_name = 'Dreva' AND skill_id = 106",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )
            .expect("row exists");
        assert_eq!((level, xp), (87, 5000));
        assert_eq!(ts, "2026-04-16 13:00:00");
        assert_eq!(anchor, "2026-04-16 12:00:00");
        // Dropped awards ledger nothing ... only the newer award applied.
        let n: i64 = conn
            .query_row("SELECT COUNT(*) FROM chat_xp_awards", [], |r| r.get(0))
            .expect("ledger count");
        assert_eq!(n, 1);
        // ... so the newer award replays skip.
        let replay =
            apply_chat_level_up(&ChatLevelUp { conn: &conn, character: "TestPlayer", server: "Dreva", skill_id: 106, skill_name: "Cartography", level: 87, xp: 100, confirmed_at: "2026-04-16 13:00:00" })
                .expect("replay apply");
        assert!(!replay);
    }

    #[test]
    fn chat_xp_level_up_wins_despite_anchor() {
        // A genuine level-up at or before the anchor still applies: chat
        // stays authoritative that the level was reached (existing
        // priority, unchanged by the anchor gate).
        let conn = setup();
        conn.execute(
            "INSERT INTO game_state_skills
                (character_name, server_name, skill_id, skill_name, level, base_level, bonus_levels, xp, tnl, max_level, last_confirmed_at, xp_snapshot_at, source)
             VALUES ('TestPlayer','Dreva',106,'Cartography',87,87,0,4900,0,0,'2026-04-16 12:00:00','2026-04-16 12:00:00','snapshot')",
            [],
        )
        .unwrap();
        let changed =
            apply_chat_level_up(&ChatLevelUp { conn: &conn, character: "TestPlayer", server: "Dreva", skill_id: 106, skill_name: "Cartography", level: 88, xp: 10, confirmed_at: "2026-04-16 11:00:00" })
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
        assert_eq!(ts, "2026-04-16 12:00:00");
    }

    #[test]
    fn chat_xp_level_up_clears_stale_anchor() {
        // F1: the anchor describes the previous level's absolute — after a
        // level-up resets xp it must clear, or genuine new-level awards
        // stamped at or before the old anchor drop as already-counted.
        let conn = setup();
        conn.execute(
            "INSERT INTO game_state_skills
                (character_name, server_name, skill_id, skill_name, level, base_level, bonus_levels, xp, tnl, max_level, last_confirmed_at, xp_snapshot_at, source)
             VALUES ('TestPlayer','Dreva',106,'Cartography',87,87,0,4900,0,0,'2026-04-16 12:00:00','2026-04-16 12:00:00','snapshot')",
            [],
        )
        .unwrap();
        let leveled =
            apply_chat_level_up(&ChatLevelUp { conn: &conn, character: "TestPlayer", server: "Dreva", skill_id: 106, skill_name: "Cartography", level: 88, xp: 10, confirmed_at: "2026-04-16 11:00:00" })
                .expect("level-up apply");
        assert!(leveled);
        let anchor: Option<String> = conn
            .query_row(
                "SELECT xp_snapshot_at FROM game_state_skills
                 WHERE character_name = 'TestPlayer' AND server_name = 'Dreva' AND skill_id = 106",
                [],
                |r| r.get(0),
            )
            .expect("row exists");
        assert_eq!(anchor, None, "level-up must clear the previous level's anchor");
        // Older same-level award at the NEW level (stamped before the old
        // anchor): genuinely new progress, accumulates.
        let older =
            apply_chat_level_up(&ChatLevelUp { conn: &conn, character: "TestPlayer", server: "Dreva", skill_id: 106, skill_name: "Cartography", level: 88, xp: 5, confirmed_at: "2026-04-16 10:00:00" })
                .expect("older apply");
        assert!(older);
        let (level, xp, ts): (i64, i64, String) = conn
            .query_row(
                "SELECT level, xp, last_confirmed_at FROM game_state_skills
                 WHERE character_name = 'TestPlayer' AND server_name = 'Dreva' AND skill_id = 106",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .expect("row exists");
        assert_eq!((level, xp), (88, 15));
        assert_eq!(ts, "2026-04-16 12:00:00");
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

    #[test]
    fn garden_almanac_body_parses_seasons() {
        let body = include_str!("../../../test_data/vip_json/chat-book.log")
            .lines()
            .find(|l| l.contains("Gardening Almanac"))
            .expect("almanac fixture line");
        let json: serde_json::Value =
            serde_json::from_str(body.split_once('\t').unwrap().1.trim_start_matches("[JSON:Book] ")).unwrap();
        let seasons = parse_garden_almanac_body(
            json["Body"].as_str().unwrap(),
            "2026-11-17 16:47:31",
        );
        assert_eq!(seasons.len(), 5);
        assert!(seasons.iter().filter(|s| s.is_current).count() == 2);
        assert!(seasons.iter().filter(|s| !s.is_current).count() == 3);
        let barley = seasons.iter().find(|s| s.crop == "Barley").unwrap();
        assert_eq!(barley.zone, "Gazluk");
        assert!(barley.is_current);
        // 16:47:31 + 6h12m.
        assert_eq!(barley.event_end.as_deref(), Some("2026-11-17 22:59:31"));
        let carrot = seasons.iter().find(|s| s.crop == "Carrot").unwrap();
        assert!(!carrot.is_current);
        assert_eq!(carrot.event_start.as_deref(), Some("2026-11-17 22:59:31"));
        let potato = seasons.iter().find(|s| s.crop == "Potato").unwrap();
        // 16:47:31 + 1d6h.
        assert_eq!(potato.event_start.as_deref(), Some("2026-11-18 22:47:31"));
    }

    #[test]
    fn garden_almanac_import_replaces_snapshot_without_history() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        crate::db::migrations::run_migrations(&conn, None).unwrap();
        let body = include_str!("../../../test_data/vip_json/chat-book.log")
            .lines()
            .find(|l| l.contains("Gardening Almanac"))
            .expect("almanac fixture line");
        let json: serde_json::Value =
            serde_json::from_str(body.split_once('\t').unwrap().1.trim_start_matches("[JSON:Book] ")).unwrap();
        let n = import_garden_almanac(
            &conn,
            "T",
            "Dreva",
            json["Body"].as_str().unwrap(),
            "2026-11-17 16:47:31",
        )
        .unwrap();
        assert_eq!(n, 5);
        let snap: i64 = conn
            .query_row("SELECT COUNT(*) FROM garden_almanac WHERE character_name = 'T'", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(snap, 5);
        // Re-import (rescan): pure no-op — returns 0, snapshot still 5.
        // Returning 0 (not the parsed count) is what keeps the coordinator
        // from re-emitting the `garden_almanac` domain.
        let n = import_garden_almanac(
            &conn,
            "T",
            "Dreva",
            json["Body"].as_str().unwrap(),
            "2026-11-17 16:47:31",
        )
        .unwrap();
        assert_eq!(n, 0);
        let snap: i64 = conn
            .query_row("SELECT COUNT(*) FROM garden_almanac WHERE character_name = 'T'", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(snap, 5);
    }

    #[test]
    fn garden_almanac_import_empty_body_is_noop_and_returns_zero() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        crate::db::migrations::run_migrations(&conn, None).unwrap();
        // Seed a good snapshot, then feed garbage: the snapshot must survive.
        conn.execute(
            "INSERT INTO garden_almanac
                (character_name, server_name, crop_name, zone_name, event_start, event_end, is_current, captured_at)
             VALUES ('T', 'Dreva', 'Barley', 'Gazluk', NULL, '2026-11-17 22:59:31', 1, '2026-11-17 16:47:31')",
            [],
        )
        .unwrap();
        let n = import_garden_almanac(&conn, "T", "Dreva", "not an almanac", "2026-11-17 16:47:31")
            .unwrap();
        assert_eq!(n, 0);
        let snap: i64 = conn
            .query_row("SELECT COUNT(*) FROM garden_almanac WHERE character_name = 'T'", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(snap, 1);
    }
    #[test]
    fn garden_almanac_import_truncated_body_preserves_snapshot() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        crate::db::migrations::run_migrations(&conn, None).unwrap();
        // Seed a good snapshot, then feed one complete season followed by an
        // unclosed <h2>: the partial parse must not replace the snapshot.
        conn.execute(
            "INSERT INTO garden_almanac
                (character_name, server_name, crop_name, zone_name, event_start, event_end, is_current, captured_at)
             VALUES ('T', 'Dreva', 'OldCrop', 'OldZone', NULL, NULL, 1, '2026-11-17 16:47:31')",
            [],
        )
        .unwrap();
        let body = "Upcoming Gardening Events:<h2>Barley Season in Gazluk</h2><i>Ends in 6 hours 12 minutes</i><h2>Carrot Season in";
        let n = import_garden_almanac(&conn, "T", "Dreva", body, "2026-11-17 16:47:31").unwrap();
        assert_eq!(n, 0);
        let kept: String = conn
            .query_row("SELECT crop_name FROM garden_almanac WHERE character_name = 'T'", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(kept, "OldCrop");
    }
    #[test]
    fn garden_almanac_import_older_capture_preserves_snapshot() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        crate::db::migrations::run_migrations(&conn, None).unwrap();
        // A rescan redelivering an OLDER Book after a newer import must not
        // move the snapshot backward: different content passes the identical
        // check, so the captured_at guard is the only defense.
        conn.execute(
            "INSERT INTO garden_almanac
                (character_name, server_name, crop_name, zone_name, event_start, event_end, is_current, captured_at)
             VALUES ('T', 'Dreva', 'NewCrop', 'NewZone', NULL, NULL, 1, '2026-11-17 16:47:31')",
            [],
        )
        .unwrap();
        let body = "Upcoming Gardening Events:<h2>Old Season in OldZone</h2><i>Ends in 6 hours 12 minutes</i>";
        let n = import_garden_almanac(&conn, "T", "Dreva", body, "2026-11-16 16:47:31").unwrap();
        assert_eq!(n, 0);
        let kept: String = conn
            .query_row("SELECT crop_name FROM garden_almanac WHERE character_name = 'T'", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(kept, "NewCrop");
        // Equal capture stays eligible: a retry of the current observation
        // with different content still replaces (rescan heals failed imports).
        let n = import_garden_almanac(&conn, "T", "Dreva", body, "2026-11-17 16:47:31").unwrap();
        assert_eq!(n, 1);
    }

    #[test]
    fn harvest_almanac_body_parses_focuses() {
        let body = "<h1>Current Harvesting Focuses:</h1>\n<indent=20><h2>Ghouls</h2></indent><indent=20>Seek out ghouls. Knock them senseless and steal their fear and greed!</indent>\n<indent=20><i>Ends in 1 hour 42 minutes</i></indent>\n\n\n<h1>Upcoming Harvesting Focuses:</h1>\n<indent=20><h2>Corporeal Non-Skeleton Undead</h2></indent><indent=20>Today you should hunt non-skeleton, non-ghost undead. Their vile emotions can be repurposed!</indent>\n<indent=20><i>Starts in 1 hour 42 minutes</i></indent>";
        let focuses = parse_harvest_almanac_body(body, "2026-11-24 21:17:37");
        assert_eq!(focuses.len(), 2);
        let current: Vec<_> = focuses.iter().filter(|f| f.is_current).collect();
        let upcoming: Vec<_> = focuses.iter().filter(|f| !f.is_current).collect();
        assert_eq!(current.len(), 1);
        assert_eq!(upcoming.len(), 1);
        let ghouls = &current[0];
        assert_eq!(ghouls.monster, "Ghouls");
        assert!(ghouls.description.as_deref().unwrap().contains("steal their fear and greed"));
        // 21:17:37 + 1h42m.
        assert_eq!(ghouls.event_end.as_deref(), Some("2026-11-24 22:59:37"));
        assert_eq!(ghouls.event_start, None);
        let undead = &upcoming[0];
        assert_eq!(undead.monster, "Corporeal Non-Skeleton Undead");
        assert!(undead.description.as_deref().unwrap().contains("non-skeleton, non-ghost undead"));
        assert_eq!(undead.event_start.as_deref(), Some("2026-11-24 22:59:37"));
        assert_eq!(undead.event_end, None);
    }

    #[test]
    fn harvest_almanac_import_replaces_snapshot_and_round_trips() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        crate::db::migrations::run_migrations(&conn, None).unwrap();
        let body = "<h1>Current Harvesting Focuses:</h1>\n<indent=20><h2>Ghouls</h2></indent><indent=20>Seek out ghouls. Knock them senseless and steal their fear and greed!</indent>\n<indent=20><i>Ends in 1 hour 42 minutes</i></indent>\n\n\n<h1>Upcoming Harvesting Focuses:</h1>\n<indent=20><h2>Corporeal Non-Skeleton Undead</h2></indent><indent=20>Today you should hunt non-skeleton, non-ghost undead. Their vile emotions can be repurposed!</indent>\n<indent=20><i>Starts in 1 hour 42 minutes</i></indent>";
        let n = import_harvest_almanac(&conn, "T", "Dreva", body, "2026-11-24 21:17:37").unwrap();
        assert_eq!(n, 2);
        let snap: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM harvest_almanac WHERE character_name = 'T'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(snap, 2);
        // Re-import (rescan): pure no-op — returns 0, snapshot still 2.
        // Returning 0 (not the parsed count) is what keeps the coordinator
        // from re-emitting the `harvest_almanac` domain.
        let n = import_harvest_almanac(&conn, "T", "Dreva", body, "2026-11-24 21:17:37").unwrap();
        assert_eq!(n, 0);
        let snap: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM harvest_almanac WHERE character_name = 'T'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(snap, 2);
        // Garbage body never wipes a good snapshot.
        let n =
            import_harvest_almanac(&conn, "T", "Dreva", "not an almanac", "2026-11-24 21:17:37")
                .unwrap();
        assert_eq!(n, 0);
        let snap: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM harvest_almanac WHERE character_name = 'T'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(snap, 2);
        // Truncated body preserves the snapshot.
        let truncated = "<h2>Ghouls</h2><i>Ends in 1 hour</i><h2>Undead";
        let n = import_harvest_almanac(&conn, "T", "Dreva", truncated, "2026-11-24 21:17:37")
            .unwrap();
        assert_eq!(n, 0);
    }

    #[test]
    fn relative_to_rejects_absurd_durations_without_panic() {
        let base = parse_contract_ts("2026-11-17 16:47:31");
        // Valid duration still resolves.
        assert_eq!(
            relative_to(base, "1 day 6 hours").as_deref(),
            Some("2026-11-18 22:47:31")
        );
        // Absurd inputs return None instead of panicking on overflow.
        assert_eq!(relative_to(base, "1000000000 days"), None);
        assert_eq!(relative_to(base, "999999999999 hours"), None);
        // chrono stores millis: 7.8e17 s passes i64 seconds math but exceeds
        // i64::MAX/1000, so Duration::seconds would panic — try_seconds wins.
        assert_eq!(relative_to(base, "9000000000000 days"), None);
    }

    #[test]
    fn session_favor_sums_ranges_by_login_and_covers_capped() {
        let conn = setup();
        // Both sides UTC: session started 2026-11-16 12:00Z, so the 11-17
        // lines below are in session and the 11-15 line is out — no frame
        // conversion anywhere.
        conn.execute(
            "INSERT INTO game_state_session (id, character_name, server_name, last_login_at, updated_at)
             VALUES (1, 'Alice', 'Dreva', '2026-11-16 12:00:00', '2026-11-16 12:00:00')",
            [],
        )
        .unwrap();
        conn.execute_batch(
            "INSERT INTO favor_events (character_name, server_name, npc_key, timestamp, delta) VALUES
             ('Alice', 'Dreva', 'NPC_Elune', '2026-11-17 08:32:34', 18.06948),
             ('Alice', 'Dreva', 'NPC_Elune', '2026-11-15 08:00:00', -52.65),
             ('Alice', 'Dreva', 'NPC_Goss', '2026-11-17 08:33:00', 5.0),
             ('Bob', 'Dreva', 'NPC_Elune', '2026-11-17 08:32:34', 100.0)",
        )
        .unwrap();
        let sums = session_favor_sums(&conn, "Alice", "Dreva").unwrap();
        assert!((sums["NPC_Elune"] - 18.06948).abs() < 1e-9);
        // Capped (Statehelm) deltas count — only gift *detection* excludes them.
        assert!((sums["NPC_Goss"] - 5.0).abs() < 1e-9);
        // A session row for another character is no session at all.
        assert!(session_favor_sums(&conn, "Bob", "Dreva").unwrap().is_empty());
    }
    #[test]
    fn contract_ts_normalizes_t_family_and_passes_the_rest_through() {
        // Space-format is already the contract: byte-identical.
        assert_eq!(
            normalize_contract_ts("2026-04-13 22:01:42"),
            "2026-04-13 22:01:42"
        );
        // RFC3339 with fraction + offset collapses to contract seconds.
        assert_eq!(
            normalize_contract_ts("2026-05-02T03:59:25.322+00:00"),
            "2026-05-02 03:59:25"
        );
        assert_eq!(
            normalize_contract_ts("2026-05-02T03:59:25Z"),
            "2026-05-02 03:59:25"
        );
        // Bare-T (no offset) still parses — the separator is the signal.
        assert_eq!(
            normalize_contract_ts("2026-05-02T03:59:25"),
            "2026-05-02 03:59:25"
        );
        // Time-only has no recoverable date: pass through, never invent one.
        assert_eq!(normalize_contract_ts("19:29:10"), "19:29:10");
        // Unparseable input passes through rather than becoming NULL/empty —
        // writers must never silently destroy a stamp they don't understand.
        assert_eq!(normalize_contract_ts("Fri Apr 24 04:46"), "Fri Apr 24 04:46");
        assert_eq!(normalize_contract_ts(""), "");
    }

    #[test]
    fn garden_almanac_rfc3339_capture_yields_same_event_math() {
        // The census mixed family: a T-family capture must produce the same
        // absolute event bounds as its space twin — the old fixed-format
        // parse dropped the T-family to NULL event bounds (silent no-math).
        let body = "Upcoming Gardening Events:\
            <h2>Barley Season in Gazluk</h2><i>Ends in 6 hours 12 minutes</i>";
        let space = parse_garden_almanac_body(body, "2026-11-17 16:47:31");
        let rfc = parse_garden_almanac_body(body, "2026-11-17T16:47:31.000+00:00");
        assert_eq!(space.len(), 1);
        assert_eq!(space[0].event_end.as_deref(), Some("2026-11-17 22:59:31"));
        assert_eq!(rfc.len(), space.len());
        assert_eq!(rfc[0].event_end, space[0].event_end);
        assert_eq!(rfc[0].event_start, space[0].event_start);
    }

    #[test]
    fn garden_import_normalizes_capture_and_rescans_converge_across_families() {
        let conn = setup();
        let body = "Upcoming Gardening Events:\
            <h2>Barley Season in Gazluk</h2><i>Ends in 6 hours 12 minutes</i>";
        // T-family capture persists as space-format contract time.
        let n = import_garden_almanac(&conn, "T", "Dreva", body, "2026-11-17T16:47:31+00:00")
            .unwrap();
        assert_eq!(n, 1);
        let stored: String = conn
            .query_row(
                "SELECT captured_at FROM garden_almanac WHERE character_name = 'T'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(stored, "2026-11-17 16:47:31");
        // The space-format twin of the same observation is an identical
        // re-import (rescan): no-op — the families converge on one stored
        // shape instead of churning the snapshot.
        let n = import_garden_almanac(&conn, "T", "Dreva", body, "2026-11-17 16:47:31")
            .unwrap();
        assert_eq!(n, 0);
    }
}

// ── Specta schema export (TODO.md:113 shared schema contract) ───────────────
// `game_state` IPC payload shapes derive `specta::Type` so
// `src/types/gameState.specta.ts` can be generated from them. Test-only plumbing:
// the exporters live in `[dev-dependencies]`, so no new runtime dependency
// ships beyond `specta` itself, and this fn compiles out of non-test builds.
// Regenerate with `OVERWRITE_BINDINGS=1 cargo test game_state_specta`.
#[cfg(test)]
pub fn export_bindings() -> Result<String, Box<dyn std::error::Error>> {
    let types = specta::Types::default()
        .register::<GameStateSkill>()
        .register::<GameStateAttribute>()
        .register::<GameStateActiveSkills>()
        .register::<GameStateWorld>()
        .register::<GameStateArea>()
        .register::<GameStateWeather>()
        .register::<GameStateCombat>()
        .register::<GameStateMount>()
        .register::<GameStateInventoryItem>()
        .register::<GameStateRecipe>()
        .register::<GameStateEquipmentSlot>()
        .register::<GameStateFavor>()
        .register::<GameStateCurrency>()
        .register::<CurrencyEstimate>()
        .register::<GameStateEffect>()
        .register::<GameStateStorageItem>()
        .register::<TrackedSkill>()
        .register::<TrackedSkillEntry>()
        .register::<GiftLogEntry>()
        .register::<GameStateVendor>()
        .register::<CharacterReportStat>()
        .register::<PlayerMilkingEntry>()
        .register::<PoopingStats>()
        .register::<HarvestAlmanacEntry>()
        .register::<GardenAlmanacEntry>()
        .register::<SkillCraftingBreakdown>()
        .register::<AttributeExtreme>()
        .register::<ComputedStats>()
        .register::<TeleportationBinds>()
        .register::<ItemOwnershipLocation>()
        .register::<ItemOwnershipInfo>()
        ;
    Ok(specta_typescript::Typescript::default().export(&types, specta_serde::Format)?)
}

#[cfg(test)]
mod specta_bindings_tests {
    use super::export_bindings;

    /// Freshness pin: the checked-in `src/types/gameState.specta.ts` must equal
    /// what `export_bindings` derives from the `game_state` payload shapes. Any
    /// shape change (field add/remove/rename, serde attribute) fails here until the
    /// bindings are regenerated — that failure IS the drift signal. Regenerate with
    /// `OVERWRITE_BINDINGS=1 cargo test game_state_specta`, inspect the diff, and
    /// confirm it matches the hand-mirrored frontend types before committing.
    #[test]
    fn test_game_state_specta_bindings_are_fresh() {
        let fresh = export_bindings().expect("specta export failed");
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../src/types/gameState.specta.ts",
        );
        if std::env::var("OVERWRITE_BINDINGS").is_ok() {
            std::fs::write(path, &fresh).expect("bindings write failed");
            return;
        }
        let checked_in =
            std::fs::read_to_string(path).expect("checked-in specta bindings missing");
        assert_eq!(
            checked_in, fresh,
            "specta bindings drift: regenerate with OVERWRITE_BINDINGS=1",
        );
    }
}