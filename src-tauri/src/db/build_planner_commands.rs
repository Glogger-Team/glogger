use super::DbPool;
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use serde::{Deserialize, Serialize};
/// Build planner persistence commands
use tauri::State;

// ── Input types ─────────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct CreateBuildPresetInput {
    pub character_id: String,
    pub name: String,
    pub skill_primary: Option<String>,
    pub skill_secondary: Option<String>,
    pub target_level: Option<i32>,
    pub target_rarity: Option<String>,
}

#[derive(Deserialize)]
pub struct UpdateBuildPresetInput {
    pub id: i64,
    pub name: String,
    pub skill_primary: Option<String>,
    pub skill_secondary: Option<String>,
    pub target_level: i32,
    pub target_rarity: String,
    pub notes: Option<String>,
}

#[derive(Deserialize)]
pub struct BuildPresetModInput {
    pub equip_slot: String,
    pub power_name: String,
    pub tier: Option<i32>,
    pub is_augment: bool,
    pub sort_order: i32,
}

#[derive(Deserialize)]
pub struct SetSlotItemInput {
    pub preset_id: i64,
    pub equip_slot: String,
    pub item_id: i64,
    pub item_name: Option<String>,
    pub slot_level: Option<i32>,
    pub slot_rarity: Option<String>,
    pub is_crafted: Option<bool>,
    pub is_masterwork: Option<bool>,
}

#[derive(Deserialize)]
pub struct BuildPresetAbilityInput {
    pub bar: String,
    pub slot_position: i32,
    pub ability_id: i64,
    pub ability_name: Option<String>,
}

#[derive(Deserialize)]
pub struct BuildPresetCpRecipeInput {
    pub equip_slot: String,
    pub recipe_id: i64,
    pub recipe_name: Option<String>,
    pub cp_cost: i32,
    pub effect_type: String,
    pub effect_key: String,
    pub sort_order: i32,
}

// ── Output types ────────────────────────────────────────────────────────────

#[derive(Serialize)]
pub struct BuildPresetAbility {
    pub id: i64,
    pub preset_id: i64,
    pub bar: String,
    pub slot_position: i32,
    pub ability_id: i64,
    pub ability_name: Option<String>,
}

#[derive(Serialize)]
pub struct BuildPresetSlotItem {
    pub preset_id: i64,
    pub equip_slot: String,
    pub item_id: i64,
    pub item_name: Option<String>,
    pub slot_level: i32,
    pub slot_rarity: String,
    pub is_crafted: bool,
    pub is_masterwork: bool,
    pub slot_skill_primary: Option<String>,
    pub slot_skill_secondary: Option<String>,
}

#[derive(Serialize)]
pub struct BuildPreset {
    pub id: i64,
    pub character_id: String,
    pub name: String,
    pub skill_primary: Option<String>,
    pub skill_secondary: Option<String>,
    pub target_level: i32,
    pub target_rarity: String,
    pub notes: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Serialize)]
pub struct BuildPresetMod {
    pub id: i64,
    pub preset_id: i64,
    pub equip_slot: String,
    pub power_name: String,
    pub tier: Option<i32>,
    pub is_augment: bool,
    pub sort_order: i32,
}

#[derive(Serialize)]
pub struct BuildPresetCpRecipe {
    pub id: i64,
    pub preset_id: i64,
    pub equip_slot: String,
    pub recipe_id: i64,
    pub recipe_name: Option<String>,
    pub cp_cost: i32,
    pub effect_type: String,
    pub effect_key: String,
    pub sort_order: i32,
}

// ── Commands ────────────────────────────────────────────────────────────────

#[tauri::command]
pub fn create_build_preset(
    db: State<'_, DbPool>,
    input: CreateBuildPresetInput,
) -> Result<i64, String> {
    let conn = db
        .get_write()
        .map_err(|e| format!("Database connection error: {e}"))?;

    conn.execute(
        "INSERT INTO build_presets (character_id, name, skill_primary, skill_secondary, target_level, target_rarity)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        rusqlite::params![
            input.character_id,
            input.name,
            input.skill_primary,
            input.skill_secondary,
            input.target_level.unwrap_or(90),
            input.target_rarity.as_deref().unwrap_or("Epic"),
        ],
    ).map_err(|e| format!("Failed to create build preset: {e}"))?;

    Ok(conn.last_insert_rowid())
}

#[tauri::command]
pub fn get_build_presets(
    db: State<'_, DbPool>,
    character_id: String,
) -> Result<Vec<BuildPreset>, String> {
    let conn = db
        .get()
        .map_err(|e| format!("Database connection error: {e}"))?;

    let mut stmt = conn
        .prepare(
            "SELECT id, character_id, name, skill_primary, skill_secondary,
                target_level, target_rarity, notes,
                datetime(created_at), datetime(updated_at)
         FROM build_presets
         WHERE character_id = ?1
         ORDER BY updated_at DESC",
        )
        .map_err(|e| format!("Failed to prepare query: {e}"))?;

    let rows = stmt
        .query_map([&character_id], |row| {
            Ok(BuildPreset {
                id: row.get(0)?,
                character_id: row.get(1)?,
                name: row.get(2)?,
                skill_primary: row.get(3)?,
                skill_secondary: row.get(4)?,
                target_level: row.get(5)?,
                target_rarity: row.get(6)?,
                notes: row.get(7)?,
                created_at: row.get(8)?,
                updated_at: row.get(9)?,
            })
        })
        .map_err(|e| format!("Query failed: {e}"))?;

    let mut presets = Vec::new();
    for row in rows {
        presets.push(row.map_err(|e| format!("Row parse error: {e}"))?);
    }
    Ok(presets)
}

#[tauri::command]
pub fn update_build_preset(
    db: State<'_, DbPool>,
    input: UpdateBuildPresetInput,
) -> Result<(), String> {
    let conn = db
        .get_write()
        .map_err(|e| format!("Database connection error: {e}"))?;

    conn.execute(
        "UPDATE build_presets
         SET name = ?1, skill_primary = ?2, skill_secondary = ?3,
             target_level = ?4, target_rarity = ?5, notes = ?6,
             updated_at = datetime('now')
         WHERE id = ?7",
        rusqlite::params![
            input.name,
            input.skill_primary,
            input.skill_secondary,
            input.target_level,
            input.target_rarity,
            input.notes,
            input.id,
        ],
    )
    .map_err(|e| format!("Failed to update build preset: {e}"))?;

    Ok(())
}

#[tauri::command]
pub fn delete_build_preset(db: State<'_, DbPool>, preset_id: i64) -> Result<(), String> {
    let conn = db
        .get_write()
        .map_err(|e| format!("Database connection error: {e}"))?;

    conn.execute("DELETE FROM build_presets WHERE id = ?1", [preset_id])
        .map_err(|e| format!("Failed to delete build preset: {e}"))?;

    Ok(())
}

#[tauri::command]
pub fn get_build_preset_mods(
    db: State<'_, DbPool>,
    preset_id: i64,
) -> Result<Vec<BuildPresetMod>, String> {
    let conn = db
        .get()
        .map_err(|e| format!("Database connection error: {e}"))?;

    let mut stmt = conn
        .prepare(
            "SELECT id, preset_id, equip_slot, power_name, tier, is_augment, sort_order
         FROM build_preset_mods
         WHERE preset_id = ?1
         ORDER BY equip_slot, sort_order",
        )
        .map_err(|e| format!("Failed to prepare query: {e}"))?;

    let rows = stmt
        .query_map([preset_id], |row| {
            Ok(BuildPresetMod {
                id: row.get(0)?,
                preset_id: row.get(1)?,
                equip_slot: row.get(2)?,
                power_name: row.get(3)?,
                tier: row.get(4)?,
                is_augment: row.get::<_, i32>(5).map(|v| v != 0)?,
                sort_order: row.get(6)?,
            })
        })
        .map_err(|e| format!("Query failed: {e}"))?;

    let mut mods = Vec::new();
    for row in rows {
        mods.push(row.map_err(|e| format!("Row parse error: {e}"))?);
    }
    Ok(mods)
}

/// Replace all mods for a build preset (simpler than individual CRUD).
#[tauri::command]
pub fn set_build_preset_mods(
    db: State<'_, DbPool>,
    preset_id: i64,
    mods: Vec<BuildPresetModInput>,
) -> Result<(), String> {
    let mut conn = db
        .get_write()
        .map_err(|e| format!("Database connection error: {e}"))?;
    let conn = conn
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .map_err(|e| format!("Failed to begin transaction: {e}"))?;

    // Delete existing mods
    conn.execute(
        "DELETE FROM build_preset_mods WHERE preset_id = ?1",
        [preset_id],
    )
    .map_err(|e| format!("Failed to clear existing mods: {e}"))?;

    // Insert new mods
    let mut stmt = conn.prepare(
        "INSERT INTO build_preset_mods (preset_id, equip_slot, power_name, tier, is_augment, sort_order)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)"
    ).map_err(|e| format!("Failed to prepare insert: {e}"))?;

    for m in &mods {
        stmt.execute(rusqlite::params![
            preset_id,
            m.equip_slot,
            m.power_name,
            m.tier,
            m.is_augment as i32,
            m.sort_order,
        ])
        .map_err(|e| format!("Failed to insert mod: {e}"))?;
    }
    drop(stmt);

    // Touch preset updated_at
    conn.execute(
        "UPDATE build_presets SET updated_at = datetime('now') WHERE id = ?1",
        [preset_id],
    )
    .map_err(|e| format!("Failed to touch preset: {e}"))?;

    conn.commit()
        .map_err(|e| format!("Failed to commit transaction: {e}"))?;
    Ok(())
}

// ── Slot item commands ─────────────────────────────────────────────────────

/// Set or replace the base item for a specific slot in a build preset.
#[tauri::command]
pub fn set_build_preset_slot_item(
    db: State<'_, DbPool>,
    input: SetSlotItemInput,
) -> Result<(), String> {
    let mut conn = db
        .get_write()
        .map_err(|e| format!("Database connection error: {e}"))?;
    let conn = conn
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .map_err(|e| format!("Failed to begin transaction: {e}"))?;

    conn.execute(
        "INSERT INTO build_preset_slot_items (preset_id, equip_slot, item_id, item_name, slot_level, slot_rarity, is_crafted, is_masterwork)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
         ON CONFLICT(preset_id, equip_slot) DO UPDATE SET item_id = ?3, item_name = ?4, slot_level = ?5, slot_rarity = ?6, is_crafted = ?7, is_masterwork = ?8",
        rusqlite::params![
            input.preset_id,
            input.equip_slot,
            input.item_id,
            input.item_name,
            input.slot_level.unwrap_or(90),
            input.slot_rarity.as_deref().unwrap_or("Epic"),
            input.is_crafted.unwrap_or(false) as i32,
            input.is_masterwork.unwrap_or(false) as i32,
        ],
    )
    .map_err(|e| format!("Failed to set slot item: {e}"))?;

    conn.execute(
        "UPDATE build_presets SET updated_at = datetime('now') WHERE id = ?1",
        [input.preset_id],
    )
    .map_err(|e| format!("Failed to touch preset: {e}"))?;

    conn.commit()
        .map_err(|e| format!("Failed to commit transaction: {e}"))?;
    Ok(())
}

/// Update slot properties (level, rarity, crafted, masterwork, skills) without changing the item.
#[tauri::command]
pub fn update_build_preset_slot_props(
    db: State<'_, DbPool>,
    preset_id: i64,
    equip_slot: String,
    slot_level: Option<i32>,
    slot_rarity: Option<String>,
    is_crafted: Option<bool>,
    is_masterwork: Option<bool>,
    slot_skill_primary: Option<String>,
    slot_skill_secondary: Option<String>,
) -> Result<(), String> {
    let mut conn = db
        .get_write()
        .map_err(|e| format!("Database connection error: {e}"))?;

    // Build dynamic SET clause for provided fields
    let mut sets = Vec::new();
    let mut params: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();

    if let Some(level) = slot_level {
        sets.push("slot_level = ?");
        params.push(Box::new(level));
    }
    if let Some(rarity) = &slot_rarity {
        sets.push("slot_rarity = ?");
        params.push(Box::new(rarity.clone()));
    }
    if let Some(crafted) = is_crafted {
        sets.push("is_crafted = ?");
        params.push(Box::new(crafted as i32));
    }
    if let Some(mw) = is_masterwork {
        sets.push("is_masterwork = ?");
        params.push(Box::new(mw as i32));
    }
    if let Some(ref skill) = slot_skill_primary {
        sets.push("slot_skill_primary = ?");
        params.push(Box::new(skill.clone()));
    }
    if let Some(ref skill) = slot_skill_secondary {
        sets.push("slot_skill_secondary = ?");
        params.push(Box::new(skill.clone()));
    }

    if sets.is_empty() {
        return Ok(());
    }

    let sql = format!(
        "UPDATE build_preset_slot_items SET {} WHERE preset_id = ? AND equip_slot = ?",
        sets.join(", ")
    );
    params.push(Box::new(preset_id));
    params.push(Box::new(equip_slot));

    let tx = conn
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .map_err(|e| format!("Failed to begin transaction: {e}"))?;

    tx.execute(&sql, rusqlite::params_from_iter(params.iter()))
        .map_err(|e| format!("Failed to update slot props: {e}"))?;

    tx.execute(
        "UPDATE build_presets SET updated_at = datetime('now') WHERE id = ?1",
        [preset_id],
    )
    .map_err(|e| format!("Failed to touch preset: {e}"))?;

    tx.commit()
        .map_err(|e| format!("Failed to commit transaction: {e}"))?;

    Ok(())
}

/// Clear the base item for a specific slot in a build preset.
#[tauri::command]
pub fn clear_build_preset_slot_item(
    db: State<'_, DbPool>,
    preset_id: i64,
    equip_slot: String,
) -> Result<(), String> {
    let conn = db
        .get_write()
        .map_err(|e| format!("Database connection error: {e}"))?;

    conn.execute(
        "DELETE FROM build_preset_slot_items WHERE preset_id = ?1 AND equip_slot = ?2",
        rusqlite::params![preset_id, equip_slot],
    )
    .map_err(|e| format!("Failed to clear slot item: {e}"))?;

    Ok(())
}

/// Get all slot items for a build preset.
#[tauri::command]
pub fn get_build_preset_slot_items(
    db: State<'_, DbPool>,
    preset_id: i64,
) -> Result<Vec<BuildPresetSlotItem>, String> {
    let conn = db
        .get()
        .map_err(|e| format!("Database connection error: {e}"))?;

    let mut stmt = conn
        .prepare(
            "SELECT preset_id, equip_slot, item_id, item_name, slot_level, slot_rarity, is_crafted, is_masterwork, slot_skill_primary, slot_skill_secondary
         FROM build_preset_slot_items
         WHERE preset_id = ?1",
        )
        .map_err(|e| format!("Failed to prepare query: {e}"))?;

    let rows = stmt
        .query_map([preset_id], |row| {
            Ok(BuildPresetSlotItem {
                preset_id: row.get(0)?,
                equip_slot: row.get(1)?,
                item_id: row.get(2)?,
                item_name: row.get(3)?,
                slot_level: row.get(4)?,
                slot_rarity: row.get(5)?,
                is_crafted: row.get::<_, i32>(6).map(|v| v != 0)?,
                is_masterwork: row.get::<_, i32>(7).map(|v| v != 0)?,
                slot_skill_primary: row.get(8)?,
                slot_skill_secondary: row.get(9)?,
            })
        })
        .map_err(|e| format!("Query failed: {e}"))?;

    let mut items = Vec::new();
    for row in rows {
        items.push(row.map_err(|e| format!("Row parse error: {e}"))?);
    }
    Ok(items)
}

// ── Import equipped gear ────────────────────────────────────────────────────

/// A mod pulled from an equipped item's TSys powers (or its imbued augment).
#[derive(Serialize)]
pub struct EquippedGearMod {
    pub power_name: String,
    pub tier: Option<i32>,
    pub is_augment: bool,
}

/// One candidate item that could be worn in a given planner slot.
///
/// The Storage report has no "currently equipped" flag — worn gear and spare
/// equippables are both just inventory items with an equip slot. We surface every
/// candidate per slot (best-first) so the user confirms the loadout, and expose the
/// two signals that hint at which is actually worn: `is_worn` (the item has a
/// durability value, i.e. it has been used) and `is_attuned` (bound to this character).
#[derive(Serialize)]
pub struct EquippedCandidate {
    pub item_id: i64,
    pub item_name: Option<String>,
    pub slot_level: i32,
    pub slot_rarity: String,
    pub is_crafted: bool,
    pub is_worn: bool,
    pub is_attuned: bool,
    pub mods: Vec<EquippedGearMod>,
}

/// All candidate items for one build-planner slot, ordered best-guess first.
#[derive(Serialize)]
pub struct EquippedSlotCandidates {
    /// Build-planner slot id (e.g. "Belt", not the game's "Waist").
    pub equip_slot: String,
    pub candidates: Vec<EquippedCandidate>,
}

/// Map a game equipment-slot name to the build planner's slot id.
/// Slots the planner doesn't model (mount, banner, saddle…) return None.
fn planner_slot_for(game_slot: &str) -> Option<&'static str> {
    Some(match game_slot {
        "Head" => "Head",
        "Chest" => "Chest",
        "Legs" => "Legs",
        "Hands" => "Hands",
        "Feet" => "Feet",
        "MainHand" => "MainHand",
        "OffHand" => "OffHand",
        "Ring" => "Ring",
        "Necklace" => "Necklace",
        "Waist" => "Belt",
        "Racial" => "Racial",
        _ => return None,
    })
}

fn rarity_rank(rarity: &str) -> i32 {
    match rarity {
        "Uncommon" => 1,
        "Rare" => 2,
        "Exceptional" => 3,
        "Epic" => 4,
        "Legendary" => 5,
        _ => 0, // Common / unknown
    }
}

/// Collect every equippable item from the character's latest items snapshot,
/// grouped by build-planner slot, for the "Import Equipped" loadout picker.
///
/// Equipped/equippable items are those with an equip slot and no storage vault
/// (worn or carried, not stashed in a chest). The report can't tell worn gear from
/// spares, so we return all candidates per slot ordered best-guess first
/// (worn → attuned → level → rarity → mod count) and let the user confirm.
#[tauri::command]
pub fn get_equipped_gear_candidates(
    db: State<'_, DbPool>,
    character_name: String,
    server_name: Option<String>,
) -> Result<Vec<EquippedSlotCandidates>, String> {
    use super::inventory_commands::TsysPower;
    use std::collections::HashMap;

    let conn = db
        .get()
        .map_err(|e| format!("Database connection error: {e}"))?;

    // Latest snapshot id for this character (optionally scoped to a server).
    let snapshot_id: Option<i64> = if let Some(ref server) = server_name {
        conn.query_row(
            "SELECT id FROM character_item_snapshots
             WHERE character_name = ?1 AND server_name = ?2
             ORDER BY snapshot_timestamp DESC LIMIT 1",
            rusqlite::params![character_name, server],
            |row| row.get(0),
        )
    } else {
        conn.query_row(
            "SELECT id FROM character_item_snapshots
             WHERE character_name = ?1
             ORDER BY snapshot_timestamp DESC LIMIT 1",
            rusqlite::params![character_name],
            |row| row.get(0),
        )
    }
    .ok();

    let Some(snapshot_id) = snapshot_id else {
        return Ok(Vec::new());
    };

    let mut stmt = conn
        .prepare(
            "SELECT type_id, item_name, rarity, slot, level, is_crafted,
                    tsys_powers, tsys_imbue_power, tsys_imbue_power_tier,
                    durability, attuned_to
             FROM character_snapshot_items
             WHERE item_snapshot_id = ?1
               AND slot IS NOT NULL
               AND (storage_vault IS NULL OR storage_vault = '')",
        )
        .map_err(|e| format!("Failed to prepare query: {e}"))?;

    struct RawRow {
        type_id: i64,
        item_name: Option<String>,
        rarity: Option<String>,
        slot: String,
        level: Option<i64>,
        is_crafted: bool,
        tsys_powers: Option<String>,
        imbue_power: Option<String>,
        imbue_tier: Option<i64>,
        durability: Option<f64>,
        attuned_to: Option<String>,
    }

    let rows = stmt
        .query_map([snapshot_id], |row| {
            Ok(RawRow {
                type_id: row.get(0)?,
                item_name: row.get(1)?,
                rarity: row.get(2)?,
                slot: row.get(3)?,
                level: row.get(4)?,
                is_crafted: row.get::<_, i32>(5).map(|v| v != 0)?,
                tsys_powers: row.get(6)?,
                imbue_power: row.get(7)?,
                imbue_tier: row.get(8)?,
                durability: row.get(9)?,
                attuned_to: row.get(10)?,
            })
        })
        .map_err(|e| format!("Query failed: {e}"))?;

    let mut by_slot: HashMap<&'static str, Vec<EquippedCandidate>> = HashMap::new();

    for row in rows {
        let row = row.map_err(|e| format!("Row parse error: {e}"))?;
        let Some(planner_slot) = planner_slot_for(&row.slot) else {
            continue;
        };

        let slot_rarity = row.rarity.clone().unwrap_or_else(|| "Epic".to_string());
        let slot_level = row.level.unwrap_or(90) as i32;
        let is_attuned = row
            .attuned_to
            .as_deref()
            .is_some_and(|a| a.eq_ignore_ascii_case(&character_name));

        let mut mods: Vec<EquippedGearMod> = Vec::new();
        if let Some(ref json) = row.tsys_powers {
            if let Ok(powers) = serde_json::from_str::<Vec<TsysPower>>(json) {
                for p in powers {
                    mods.push(EquippedGearMod {
                        power_name: p.power,
                        tier: Some(p.tier as i32),
                        is_augment: false,
                    });
                }
            }
        }
        if let Some(imbue) = row.imbue_power.clone() {
            if !imbue.is_empty() {
                mods.push(EquippedGearMod {
                    power_name: imbue,
                    tier: row.imbue_tier.map(|t| t as i32),
                    is_augment: true,
                });
            }
        }

        by_slot.entry(planner_slot).or_default().push(EquippedCandidate {
            item_id: row.type_id,
            item_name: row.item_name,
            slot_level,
            slot_rarity,
            is_crafted: row.is_crafted,
            is_worn: row.durability.is_some(),
            is_attuned,
            mods,
        });
    }

    // Order candidates best-guess first so the picker can default to candidates[0]:
    // worn items first, then attuned, then higher level / rarity / mod count.
    let mut out: Vec<EquippedSlotCandidates> = by_slot
        .into_iter()
        .map(|(slot, mut candidates)| {
            candidates.sort_by(|a, b| {
                let key = |c: &EquippedCandidate| {
                    (
                        c.is_worn,
                        c.is_attuned,
                        c.slot_level,
                        rarity_rank(&c.slot_rarity),
                        c.mods.len() as i32,
                    )
                };
                key(b).cmp(&key(a))
            });
            EquippedSlotCandidates {
                equip_slot: slot.to_string(),
                candidates,
            }
        })
        .collect();
    out.sort_by(|a, b| a.equip_slot.cmp(&b.equip_slot));
    Ok(out)
}

// ── Ability bar commands ───────────────────────────────────────────────────

/// Replace all abilities for a specific bar in a build preset.
#[tauri::command]
pub fn set_build_preset_abilities(
    db: State<'_, DbPool>,
    preset_id: i64,
    bar: String,
    abilities: Vec<BuildPresetAbilityInput>,
) -> Result<(), String> {
    let mut conn = db
        .get_write()
        .map_err(|e| format!("Database connection error: {e}"))?;
    let conn = conn
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .map_err(|e| format!("Failed to begin transaction: {e}"))?;

    // Delete existing abilities for this bar
    conn.execute(
        "DELETE FROM build_preset_abilities WHERE preset_id = ?1 AND bar = ?2",
        rusqlite::params![preset_id, bar],
    )
    .map_err(|e| format!("Failed to clear abilities: {e}"))?;

    // Insert new abilities
    let mut stmt = conn.prepare(
        "INSERT INTO build_preset_abilities (preset_id, bar, slot_position, ability_id, ability_name)
         VALUES (?1, ?2, ?3, ?4, ?5)"
    ).map_err(|e| format!("Failed to prepare insert: {e}"))?;

    for a in &abilities {
        stmt.execute(rusqlite::params![
            preset_id,
            a.bar,
            a.slot_position,
            a.ability_id,
            a.ability_name,
        ])
        .map_err(|e| format!("Failed to insert ability: {e}"))?;
    }
    drop(stmt);

    conn.execute(
        "UPDATE build_presets SET updated_at = datetime('now') WHERE id = ?1",
        [preset_id],
    )
    .map_err(|e| format!("Failed to touch preset: {e}"))?;

    conn.commit()
        .map_err(|e| format!("Failed to commit transaction: {e}"))?;
    Ok(())
}

/// Get all abilities for a build preset across all bars.
#[tauri::command]
pub fn get_build_preset_abilities(
    db: State<'_, DbPool>,
    preset_id: i64,
) -> Result<Vec<BuildPresetAbility>, String> {
    let conn = db
        .get()
        .map_err(|e| format!("Database connection error: {e}"))?;

    let mut stmt = conn
        .prepare(
            "SELECT id, preset_id, bar, slot_position, ability_id, ability_name
         FROM build_preset_abilities
         WHERE preset_id = ?1
         ORDER BY bar, slot_position",
        )
        .map_err(|e| format!("Failed to prepare query: {e}"))?;

    let rows = stmt
        .query_map([preset_id], |row| {
            Ok(BuildPresetAbility {
                id: row.get(0)?,
                preset_id: row.get(1)?,
                bar: row.get(2)?,
                slot_position: row.get(3)?,
                ability_id: row.get(4)?,
                ability_name: row.get(5)?,
            })
        })
        .map_err(|e| format!("Query failed: {e}"))?;

    let mut abilities = Vec::new();
    for row in rows {
        abilities.push(row.map_err(|e| format!("Row parse error: {e}"))?);
    }
    Ok(abilities)
}

// ── CP Recipe Commands ─────────────────────────────────────────────────────

#[tauri::command]
pub fn get_build_preset_cp_recipes(
    db: State<'_, DbPool>,
    preset_id: i64,
) -> Result<Vec<BuildPresetCpRecipe>, String> {
    let conn = db
        .get()
        .map_err(|e| format!("Database connection error: {e}"))?;

    let mut stmt = conn
        .prepare(
            "SELECT id, preset_id, equip_slot, recipe_id, recipe_name, cp_cost, effect_type, effect_key, sort_order
             FROM build_preset_cp_recipes
             WHERE preset_id = ?1
             ORDER BY equip_slot, sort_order",
        )
        .map_err(|e| format!("Failed to prepare query: {e}"))?;

    let rows = stmt
        .query_map([preset_id], |row| {
            Ok(BuildPresetCpRecipe {
                id: row.get(0)?,
                preset_id: row.get(1)?,
                equip_slot: row.get(2)?,
                recipe_id: row.get(3)?,
                recipe_name: row.get(4)?,
                cp_cost: row.get(5)?,
                effect_type: row.get(6)?,
                effect_key: row.get(7)?,
                sort_order: row.get(8)?,
            })
        })
        .map_err(|e| format!("Query failed: {e}"))?;

    let mut recipes = Vec::new();
    for row in rows {
        recipes.push(row.map_err(|e| format!("Row parse error: {e}"))?);
    }
    Ok(recipes)
}

/// Clone a build preset and all its associated data (mods, slot items, abilities, CP recipes).
#[tauri::command]
pub fn clone_build_preset(
    db: State<'_, DbPool>,
    preset_id: i64,
    new_name: String,
) -> Result<i64, String> {
    let mut conn = db
        .get_write()
        .map_err(|e| format!("Database connection error: {e}"))?;
    let conn = conn
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .map_err(|e| format!("Failed to begin transaction: {e}"))?;

    // Get the source preset
    let source = conn
        .query_row(
            "SELECT character_id, skill_primary, skill_secondary, target_level, target_rarity, notes
             FROM build_presets WHERE id = ?1",
            [preset_id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, i32>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, Option<String>>(5)?,
                ))
            },
        )
        .map_err(|e| format!("Source preset not found: {e}"))?;

    let (character_id, skill_primary, skill_secondary, target_level, target_rarity, notes) = source;

    // Create the new preset
    conn.execute(
        "INSERT INTO build_presets (character_id, name, skill_primary, skill_secondary, target_level, target_rarity, notes)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        rusqlite::params![character_id, new_name, skill_primary, skill_secondary, target_level, target_rarity, notes],
    )
    .map_err(|e| format!("Failed to create cloned preset: {e}"))?;

    let new_id = conn.last_insert_rowid();

    // Clone mods
    conn.execute(
        "INSERT INTO build_preset_mods (preset_id, equip_slot, power_name, tier, is_augment, sort_order)
         SELECT ?1, equip_slot, power_name, tier, is_augment, sort_order
         FROM build_preset_mods WHERE preset_id = ?2",
        rusqlite::params![new_id, preset_id],
    )
    .map_err(|e| format!("Failed to clone mods: {e}"))?;

    // Clone slot items
    conn.execute(
        "INSERT INTO build_preset_slot_items (preset_id, equip_slot, item_id, item_name, slot_level, slot_rarity, is_crafted, is_masterwork, slot_skill_primary, slot_skill_secondary)
         SELECT ?1, equip_slot, item_id, item_name, slot_level, slot_rarity, is_crafted, is_masterwork, slot_skill_primary, slot_skill_secondary
         FROM build_preset_slot_items WHERE preset_id = ?2",
        rusqlite::params![new_id, preset_id],
    )
    .map_err(|e| format!("Failed to clone slot items: {e}"))?;

    // Clone abilities
    conn.execute(
        "INSERT INTO build_preset_abilities (preset_id, bar, slot_position, ability_id, ability_name)
         SELECT ?1, bar, slot_position, ability_id, ability_name
         FROM build_preset_abilities WHERE preset_id = ?2",
        rusqlite::params![new_id, preset_id],
    )
    .map_err(|e| format!("Failed to clone abilities: {e}"))?;

    // Clone CP recipes
    conn.execute(
        "INSERT INTO build_preset_cp_recipes (preset_id, equip_slot, recipe_id, recipe_name, cp_cost, effect_type, effect_key, sort_order)
         SELECT ?1, equip_slot, recipe_id, recipe_name, cp_cost, effect_type, effect_key, sort_order
         FROM build_preset_cp_recipes WHERE preset_id = ?2",
        rusqlite::params![new_id, preset_id],
    )
    .map_err(|e| format!("Failed to clone CP recipes: {e}"))?;

    conn.commit()
        .map_err(|e| format!("Failed to commit transaction: {e}"))?;
    Ok(new_id)
}

#[tauri::command]
pub fn set_build_preset_cp_recipes(
    db: State<'_, DbPool>,
    preset_id: i64,
    equip_slot: String,
    recipes: Vec<BuildPresetCpRecipeInput>,
) -> Result<(), String> {
    let mut conn = db
        .get_write()
        .map_err(|e| format!("Database connection error: {e}"))?;
    let conn = conn
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .map_err(|e| format!("Failed to begin transaction: {e}"))?;

    // Delete existing CP recipes for this slot
    conn.execute(
        "DELETE FROM build_preset_cp_recipes WHERE preset_id = ?1 AND equip_slot = ?2",
        rusqlite::params![preset_id, equip_slot],
    )
    .map_err(|e| format!("Failed to clear existing CP recipes: {e}"))?;

    // Insert new CP recipes
    let mut stmt = conn.prepare(
        "INSERT INTO build_preset_cp_recipes (preset_id, equip_slot, recipe_id, recipe_name, cp_cost, effect_type, effect_key, sort_order)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)"
    ).map_err(|e| format!("Failed to prepare insert: {e}"))?;

    for r in &recipes {
        stmt.execute(rusqlite::params![
            preset_id,
            r.equip_slot,
            r.recipe_id,
            r.recipe_name,
            r.cp_cost,
            r.effect_type,
            r.effect_key,
            r.sort_order,
        ])
        .map_err(|e| format!("Failed to insert CP recipe: {e}"))?;
    }
    drop(stmt);

    // Touch preset updated_at
    conn.execute(
        "UPDATE build_presets SET updated_at = datetime('now') WHERE id = ?1",
        [preset_id],
    )
    .map_err(|e| format!("Failed to touch preset: {e}"))?;

    conn.commit()
        .map_err(|e| format!("Failed to commit transaction: {e}"))?;
    Ok(())
}

// ── Import/Export Commands ─────────────────────────────────────────────────

/// Portable build format for sharing between players
#[derive(Serialize, Deserialize)]
struct ExportedBuild {
    version: u32,
    name: String,
    skill_primary: Option<String>,
    skill_secondary: Option<String>,
    target_level: i32,
    target_rarity: String,
    notes: Option<String>,
    mods: Vec<ExportedMod>,
    slot_items: Vec<ExportedSlotItem>,
    abilities: Vec<ExportedAbility>,
    cp_recipes: Vec<ExportedCpRecipe>,
}

#[derive(Serialize, Deserialize)]
struct ExportedMod {
    equip_slot: String,
    power_name: String,
    tier: Option<i32>,
    is_augment: bool,
    sort_order: i32,
}

#[derive(Serialize, Deserialize)]
struct ExportedSlotItem {
    equip_slot: String,
    item_id: i64,
    item_name: Option<String>,
    slot_level: i32,
    slot_rarity: String,
    is_crafted: bool,
    is_masterwork: bool,
    slot_skill_primary: Option<String>,
    slot_skill_secondary: Option<String>,
}

#[derive(Serialize, Deserialize)]
struct ExportedAbility {
    bar: String,
    slot_position: i32,
    ability_id: i64,
    ability_name: Option<String>,
}

#[derive(Serialize, Deserialize)]
struct ExportedCpRecipe {
    equip_slot: String,
    recipe_id: i64,
    recipe_name: Option<String>,
    cp_cost: i32,
    effect_type: String,
    effect_key: String,
    sort_order: i32,
}

#[tauri::command]
pub fn export_build_preset(
    db: State<'_, DbPool>,
    preset_id: i64,
) -> Result<String, String> {
    let conn = db
        .get()
        .map_err(|e| format!("Database connection error: {e}"))?;

    let (name, skill_primary, skill_secondary, target_level, target_rarity, notes) = conn
        .query_row(
            "SELECT name, skill_primary, skill_secondary, target_level, target_rarity, notes
             FROM build_presets WHERE id = ?1",
            [preset_id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, i32>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, Option<String>>(5)?,
                ))
            },
        )
        .map_err(|e| format!("Preset not found: {e}"))?;

    let mut stmt = conn
        .prepare("SELECT equip_slot, power_name, tier, is_augment, sort_order FROM build_preset_mods WHERE preset_id = ?1 ORDER BY equip_slot, sort_order")
        .map_err(|e| format!("Failed to query mods: {e}"))?;
    let mods: Vec<ExportedMod> = stmt
        .query_map([preset_id], |row| {
            Ok(ExportedMod {
                equip_slot: row.get(0)?,
                power_name: row.get(1)?,
                tier: row.get(2)?,
                is_augment: row.get::<_, i32>(3).map(|v| v != 0)?,
                sort_order: row.get(4)?,
            })
        })
        .map_err(|e| format!("Mod query failed: {e}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("Mod parse error: {e}"))?;

    let mut stmt = conn
        .prepare("SELECT equip_slot, item_id, item_name, slot_level, slot_rarity, is_crafted, is_masterwork, slot_skill_primary, slot_skill_secondary FROM build_preset_slot_items WHERE preset_id = ?1")
        .map_err(|e| format!("Failed to query slot items: {e}"))?;
    let slot_items: Vec<ExportedSlotItem> = stmt
        .query_map([preset_id], |row| {
            Ok(ExportedSlotItem {
                equip_slot: row.get(0)?,
                item_id: row.get(1)?,
                item_name: row.get(2)?,
                slot_level: row.get(3)?,
                slot_rarity: row.get(4)?,
                is_crafted: row.get::<_, i32>(5).map(|v| v != 0)?,
                is_masterwork: row.get::<_, i32>(6).map(|v| v != 0)?,
                slot_skill_primary: row.get(7)?,
                slot_skill_secondary: row.get(8)?,
            })
        })
        .map_err(|e| format!("Slot item query failed: {e}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("Slot item parse error: {e}"))?;

    let mut stmt = conn
        .prepare("SELECT bar, slot_position, ability_id, ability_name FROM build_preset_abilities WHERE preset_id = ?1 ORDER BY bar, slot_position")
        .map_err(|e| format!("Failed to query abilities: {e}"))?;
    let abilities: Vec<ExportedAbility> = stmt
        .query_map([preset_id], |row| {
            Ok(ExportedAbility {
                bar: row.get(0)?,
                slot_position: row.get(1)?,
                ability_id: row.get(2)?,
                ability_name: row.get(3)?,
            })
        })
        .map_err(|e| format!("Ability query failed: {e}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("Ability parse error: {e}"))?;

    let mut stmt = conn
        .prepare("SELECT equip_slot, recipe_id, recipe_name, cp_cost, effect_type, effect_key, sort_order FROM build_preset_cp_recipes WHERE preset_id = ?1 ORDER BY equip_slot, sort_order")
        .map_err(|e| format!("Failed to query CP recipes: {e}"))?;
    let cp_recipes: Vec<ExportedCpRecipe> = stmt
        .query_map([preset_id], |row| {
            Ok(ExportedCpRecipe {
                equip_slot: row.get(0)?,
                recipe_id: row.get(1)?,
                recipe_name: row.get(2)?,
                cp_cost: row.get(3)?,
                effect_type: row.get(4)?,
                effect_key: row.get(5)?,
                sort_order: row.get(6)?,
            })
        })
        .map_err(|e| format!("CP recipe query failed: {e}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("CP recipe parse error: {e}"))?;

    let exported = ExportedBuild {
        version: 1,
        name,
        skill_primary,
        skill_secondary,
        target_level,
        target_rarity,
        notes,
        mods,
        slot_items,
        abilities,
        cp_recipes,
    };

    let json = serde_json::to_string(&exported).map_err(|e| format!("Serialization error: {e}"))?;
    Ok(BASE64.encode(json.as_bytes()))
}

#[tauri::command]
pub fn import_build_preset(
    db: State<'_, DbPool>,
    character_id: String,
    encoded: String,
) -> Result<i64, String> {
    let json_bytes = BASE64
        .decode(encoded.trim())
        .map_err(|e| format!("Invalid build code: {e}"))?;
    let json_str = String::from_utf8(json_bytes).map_err(|e| format!("Invalid UTF-8: {e}"))?;
    let build: ExportedBuild =
        serde_json::from_str(&json_str).map_err(|e| format!("Invalid build data: {e}"))?;

    if build.version != 1 {
        return Err(format!(
            "Unsupported build version {} (this app supports version 1)",
            build.version
        ));
    }

    let mut conn = db
        .get_write()
        .map_err(|e| format!("Database connection error: {e}"))?;
    let conn = conn
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .map_err(|e| format!("Failed to begin transaction: {e}"))?;

    conn.execute(
        "INSERT INTO build_presets (character_id, name, skill_primary, skill_secondary, target_level, target_rarity, notes)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        rusqlite::params![
            character_id,
            build.name,
            build.skill_primary,
            build.skill_secondary,
            build.target_level,
            build.target_rarity,
            build.notes,
        ],
    )
    .map_err(|e| format!("Failed to create preset: {e}"))?;

    let new_id = conn.last_insert_rowid();

    {
        let mut stmt = conn
            .prepare("INSERT INTO build_preset_mods (preset_id, equip_slot, power_name, tier, is_augment, sort_order) VALUES (?1, ?2, ?3, ?4, ?5, ?6)")
            .map_err(|e| format!("Failed to prepare mod insert: {e}"))?;
        for m in &build.mods {
            stmt.execute(rusqlite::params![new_id, m.equip_slot, m.power_name, m.tier, m.is_augment as i32, m.sort_order])
                .map_err(|e| format!("Failed to insert mod: {e}"))?;
        }
    }

    {
        let mut stmt = conn
            .prepare("INSERT INTO build_preset_slot_items (preset_id, equip_slot, item_id, item_name, slot_level, slot_rarity, is_crafted, is_masterwork, slot_skill_primary, slot_skill_secondary) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)")
            .map_err(|e| format!("Failed to prepare slot item insert: {e}"))?;
        for si in &build.slot_items {
            stmt.execute(rusqlite::params![new_id, si.equip_slot, si.item_id, si.item_name, si.slot_level, si.slot_rarity, si.is_crafted as i32, si.is_masterwork as i32, si.slot_skill_primary, si.slot_skill_secondary])
                .map_err(|e| format!("Failed to insert slot item: {e}"))?;
        }
    }

    {
        let mut stmt = conn
            .prepare("INSERT INTO build_preset_abilities (preset_id, bar, slot_position, ability_id, ability_name) VALUES (?1, ?2, ?3, ?4, ?5)")
            .map_err(|e| format!("Failed to prepare ability insert: {e}"))?;
        for a in &build.abilities {
            stmt.execute(rusqlite::params![new_id, a.bar, a.slot_position, a.ability_id, a.ability_name])
                .map_err(|e| format!("Failed to insert ability: {e}"))?;
        }
    }

    {
        let mut stmt = conn
            .prepare("INSERT INTO build_preset_cp_recipes (preset_id, equip_slot, recipe_id, recipe_name, cp_cost, effect_type, effect_key, sort_order) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)")
            .map_err(|e| format!("Failed to prepare CP recipe insert: {e}"))?;
        for r in &build.cp_recipes {
            stmt.execute(rusqlite::params![new_id, r.equip_slot, r.recipe_id, r.recipe_name, r.cp_cost, r.effect_type, r.effect_key, r.sort_order])
                .map_err(|e| format!("Failed to insert CP recipe: {e}"))?;
        }
    }

    conn.commit()
        .map_err(|e| format!("Failed to commit transaction: {e}"))?;
    Ok(new_id)
}

#[cfg(test)]
mod equipped_gear_tests {
    use super::*;
    use crate::db::inventory_commands::TsysPower;

    #[test]
    fn maps_game_slots_to_planner_slots() {
        assert_eq!(planner_slot_for("Waist"), Some("Belt"));
        assert_eq!(planner_slot_for("MainHand"), Some("MainHand"));
        assert_eq!(planner_slot_for("Ring"), Some("Ring"));
        assert_eq!(planner_slot_for("Racial"), Some("Racial"));
        // Slots the planner doesn't model are skipped.
        assert_eq!(planner_slot_for("MountGlamour"), None);
        assert_eq!(planner_slot_for("Banner"), None);
        assert_eq!(planner_slot_for("Saddle"), None);
    }

    #[test]
    fn rarity_rank_orders_ascending() {
        assert!(rarity_rank("Legendary") > rarity_rank("Epic"));
        assert!(rarity_rank("Epic") > rarity_rank("Rare"));
        assert!(rarity_rank("Rare") > rarity_rank("Uncommon"));
        assert_eq!(rarity_rank("Common"), 0);
        assert_eq!(rarity_rank("nonsense"), 0);
    }

    /// The tsys_powers column is written by the inventory importer via
    /// serde_json::to_string(&Vec<TsysPower>). Guard that the equipped-gear
    /// reader parses that exact PascalCase shape back.
    #[test]
    fn parses_stored_tsys_powers_json() {
        let stored = serde_json::to_string(&vec![
            TsysPower { tier: 4, power: "AgonizeShortenReset".to_string() },
            TsysPower { tier: 9, power: "GenericBoostAcid".to_string() },
        ])
        .unwrap();
        assert!(stored.contains("\"Tier\":4"));
        assert!(stored.contains("\"Power\":\"AgonizeShortenReset\""));

        let parsed: Vec<TsysPower> = serde_json::from_str(&stored).unwrap();
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].power, "AgonizeShortenReset");
        assert_eq!(parsed[1].tier, 9);
    }
}
