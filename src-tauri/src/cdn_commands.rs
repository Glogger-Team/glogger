use chrono::{Datelike, Local};
use once_cell::sync::Lazy;
/// Tauri commands for CDN data management and game data queries.
/// These are the invoke() endpoints the Vue frontend calls.
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager, State};
use tokio::sync::RwLock;

use serde_json::Value;

use crate::cdn;
use crate::db::DbPool;
use crate::game_data::{
    self, AbilityFamily, AbilityInfo, AiSummary, AreaInfo, BrewingIngredient, BrewingRecipe,
    EffectInfo, GameData, ItemInfo, LorebookCategoryInfo, LorebookEntry, NpcInfo, PlayerTitleInfo,
    QuestInfo, RecipeInfo, SkillInfo, SourceEntry, TsysClientInfo, TsysTierInfo,
};

/// Timestamped log line for startup diagnostics.
macro_rules! startup_log {
    ($($arg:tt)*) => {
        eprintln!("[{}] {}", Local::now().format("%H:%M:%S%.3f"), format!($($arg)*));
    };
}

// ── Managed state ─────────────────────────────────────────────────────────────

/// Shared, hot-swappable game data. Wrapped in Arc<RwLock<>> so commands can
/// read concurrently and the refresh command can swap in a new GameData.
pub type GameDataState = Arc<RwLock<GameData>>;

// ── Directory helpers ─────────────────────────────────────────────────────────

fn data_cache_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let base = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("Cannot resolve app data dir: {e}"))?;
    let dir = base.join("data");
    std::fs::create_dir_all(&dir).map_err(|e| format!("Cannot create data dir: {e}"))?;
    Ok(dir)
}

fn icon_cache_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let base = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("Cannot resolve app data dir: {e}"))?;
    let dir = base.join("icons");
    std::fs::create_dir_all(&dir).map_err(|e| format!("Cannot create icons dir: {e}"))?;
    Ok(dir)
}

// ── Init (called once at startup, not exposed as a command) ───────────────────

/// Check the CDN version, download if stale, load into managed state.
/// Runs at app startup. Non-fatal: on network failure, loads from cache if available.
pub async fn init_game_data(app: &AppHandle, state: &GameDataState) -> Result<(), String> {
    let data_dir = data_cache_dir(app)?;

    let cached_version = cdn::read_cached_version(&data_dir).await;

    // Try to fetch remote version; fall back to cache-only if offline
    startup_log!("Checking CDN version...");
    emit_startup_detail(app, 0, "Checking for updates...");
    let remote_version = match cdn::fetch_remote_version().await {
        Ok(v) => v,
        Err(e) => {
            startup_log!("CDN version check failed (offline?): {e}");
            if let Some(cached) = cached_version {
                startup_log!("Loading game data from cache (v{cached})");
                emit_startup_detail(app, 0, "Loading cached data...");
                match game_data::load_from_cache_with_progress(&data_dir, cached, |detail| {
                    emit_startup_detail(app, 0, detail);
                })
                .await
                {
                    Ok(data) => {
                        *state.write().await = data;
                        return Ok(());
                    }
                    Err(e2) => return Err(format!("Offline and cache load failed: {e2}")),
                }
            } else {
                return Err(format!("Offline and no cache available: {e}"));
            }
        }
    };

    let needs_download = cached_version.map_or(true, |cv| cv != remote_version);

    if needs_download {
        startup_log!("Downloading CDN data v{remote_version}...");
        emit_startup_detail(app, 0, &format!("Downloading update v{remote_version}..."));
        cdn::download_all_data_files(remote_version, &data_dir).await?;
        // Download translation strings (non-fatal — don't block startup)
        emit_startup_detail(app, 0, "Downloading translation strings...");
        if let Err(e) = cdn::download_translation_zip(remote_version, &data_dir).await {
            startup_log!("Translation download failed (non-fatal): {e}");
        }
        cdn::write_cached_version(&data_dir, remote_version).await?;
        // Remove stale cached indices so they'll be rebuilt from new data
        let _ = tokio::fs::remove_file(data_dir.join("indices.json")).await;
        startup_log!("CDN download complete");
    } else {
        startup_log!("CDN data up to date (v{remote_version}), loading from cache");
        // Backfill translation strings if they're missing (e.g. first launch after
        // this feature was added while CDN version hasn't changed)
        let translation_dir = data_dir.join("translation");
        if !translation_dir.exists() {
            startup_log!("Translation strings missing, downloading...");
            emit_startup_detail(app, 0, "Downloading translation strings...");
            if let Err(e) = cdn::download_translation_zip(remote_version, &data_dir).await {
                startup_log!("Translation download failed (non-fatal): {e}");
            }
        }
    }

    let data = game_data::load_from_cache_with_progress(&data_dir, remote_version, |detail| {
        emit_startup_detail(app, 0, detail);
    })
    .await?;
    *state.write().await = data;

    Ok(())
}

/// Emit a sub-status detail to the frontend loading screen.
pub fn emit_startup_detail(app: &AppHandle, task: u32, detail: &str) {
    #[derive(serde::Serialize, Clone)]
    struct StartupDetail {
        task: u32,
        detail: String,
    }
    app.emit(
        "startup-detail",
        StartupDetail {
            task,
            detail: detail.to_string(),
        },
    )
    .ok();
}

// ── CDN management commands ───────────────────────────────────────────────────

#[derive(serde::Serialize)]
pub struct CacheStatus {
    pub cached_version: Option<u32>,
    pub remote_version: Option<u32>,
    pub up_to_date: bool,
    pub item_count: usize,
    pub skill_count: usize,
}

#[derive(serde::Serialize)]
pub struct CdnVersionCheck {
    pub update_available: bool,
    pub current_version: u32,
    pub remote_version: u32,
}

/// Lightweight check: is there a newer CDN version than what we currently have loaded?
/// Only fetches the single-integer version file, not the full cache status.
#[tauri::command]
pub async fn check_cdn_version(state: State<'_, GameDataState>) -> Result<CdnVersionCheck, String> {
    let current_version = state.read().await.version;
    let remote_version = cdn::fetch_remote_version()
        .await
        .map_err(|e| format!("CDN version check failed: {e}"))?;

    Ok(CdnVersionCheck {
        update_available: remote_version != current_version && current_version > 0,
        current_version,
        remote_version,
    })
}

/// Returns current cache status and whether a CDN update is available.
#[tauri::command]
pub async fn get_cache_status(
    app: AppHandle,
    state: State<'_, GameDataState>,
) -> Result<CacheStatus, String> {
    let data_dir = data_cache_dir(&app)?;
    let cached_version = cdn::read_cached_version(&data_dir).await;
    let remote_version: Option<u32> = cdn::fetch_remote_version().await.ok();

    let data = state.read().await;
    let up_to_date = match (cached_version, remote_version) {
        (Some(c), Some(r)) => c == r,
        _ => false,
    };

    Ok(CacheStatus {
        cached_version,
        remote_version,
        up_to_date,
        item_count: data.items.len(),
        skill_count: data.skills.len(),
    })
}

/// Force a full re-download of CDN data, regardless of version.
#[tauri::command]
pub async fn force_refresh_cdn(
    app: AppHandle,
    state: State<'_, GameDataState>,
) -> Result<CacheStatus, String> {
    let data_dir = data_cache_dir(&app)?;

    let remote_version = cdn::fetch_remote_version().await?;
    cdn::download_all_data_files(remote_version, &data_dir).await?;
    // Download translation strings (non-fatal)
    let _ = cdn::download_translation_zip(remote_version, &data_dir).await;
    cdn::write_cached_version(&data_dir, remote_version).await?;
    let _ = tokio::fs::remove_file(data_dir.join("indices.json")).await;

    let data = game_data::load_from_cache(&data_dir, remote_version).await?;
    let item_count = data.items.len();
    let skill_count = data.skills.len();
    *state.write().await = data;

    Ok(CacheStatus {
        cached_version: Some(remote_version),
        remote_version: Some(remote_version),
        up_to_date: true,
        item_count,
        skill_count,
    })
}

// ── Unified entity resolve commands ──────────────────────────────────────────
// Each accepts any known reference form (numeric ID, display name, internal
// name, CDN key) and resolves it to the canonical entity.

/// Resolve any reference to an item (numeric ID, display name, or internal name).
#[tauri::command]
pub async fn resolve_item(
    reference: String,
    state: State<'_, GameDataState>,
) -> Result<Option<ItemInfo>, String> {
    Ok(state.read().await.resolve_item(&reference).cloned())
}

/// Resolve a batch of item references.
#[tauri::command]
pub async fn resolve_items_batch(
    references: Vec<String>,
    state: State<'_, GameDataState>,
) -> Result<std::collections::HashMap<String, ItemInfo>, String> {
    let data = state.read().await;
    let mut result = std::collections::HashMap::new();
    for ref_str in references {
        if let Some(item) = data.resolve_item(&ref_str) {
            result.insert(ref_str, item.clone());
        }
    }
    Ok(result)
}

/// Resolve any reference to a skill.
#[tauri::command]
pub async fn resolve_skill(
    reference: String,
    state: State<'_, GameDataState>,
) -> Result<Option<SkillInfo>, String> {
    Ok(state.read().await.resolve_skill(&reference).cloned())
}

/// Resolve any reference to a recipe.
#[tauri::command]
pub async fn resolve_recipe(
    reference: String,
    state: State<'_, GameDataState>,
) -> Result<Option<RecipeInfo>, String> {
    Ok(state.read().await.resolve_recipe(&reference).cloned())
}

/// Resolve any reference to a quest.
#[tauri::command]
pub async fn resolve_quest(
    reference: String,
    state: State<'_, GameDataState>,
) -> Result<Option<QuestInfo>, String> {
    Ok(state.read().await.resolve_quest(&reference).cloned())
}

/// Resolve any reference to an NPC.
#[tauri::command]
pub async fn resolve_npc(
    reference: String,
    state: State<'_, GameDataState>,
) -> Result<Option<NpcInfo>, String> {
    Ok(state.read().await.resolve_npc(&reference).cloned())
}

/// Resolve any reference to an ability (numeric ID or display name).
#[tauri::command]
pub async fn resolve_ability(
    reference: String,
    state: State<'_, GameDataState>,
) -> Result<Option<AbilityInfo>, String> {
    Ok(state.read().await.resolve_ability(&reference).cloned())
}

/// Resolve any reference to an area.
#[tauri::command]
pub async fn resolve_area(
    reference: String,
    state: State<'_, GameDataState>,
) -> Result<Option<AreaInfo>, String> {
    Ok(state.read().await.resolve_area(&reference).cloned())
}

/// Search items whose name contains the query string (case-insensitive).
/// Optional filters: equip_slot (exact match), min/max crafting_target_level.
#[tauri::command]
pub async fn search_items(
    query: String,
    equip_slot: Option<String>,
    level_min: Option<u32>,
    level_max: Option<u32>,
    armor_type: Option<String>,
    effect_text: Option<String>,
    state: State<'_, GameDataState>,
) -> Result<Vec<ItemInfo>, String> {
    let q = query.to_lowercase();
    let effect_q = effect_text.as_ref().map(|s| s.to_lowercase());
    let data = state.read().await;
    let mut results: Vec<ItemInfo> = data
        .items
        .values()
        .filter(|item| {
            // Name filter (always applied, but empty query matches all)
            if !q.is_empty() && !item.name.to_lowercase().contains(&q) {
                return false;
            }
            // Equip slot filter (maps build planner slot IDs to CDN equip_slot values)
            if let Some(ref slot) = equip_slot {
                match &item.equip_slot {
                    Some(s) => {
                        let matches = match slot.as_str() {
                            "OffHand" => s == "OffHand" || s == "OffHandShield",
                            "Belt" => s == "Waist",
                            _ => s == slot,
                        };
                        if !matches {
                            return false;
                        }
                    }
                    _ => return false,
                }
            }
            // Level range filter (on crafting_target_level)
            if let Some(min) = level_min {
                match item.crafting_target_level {
                    Some(lvl) if lvl >= min => {}
                    _ => return false,
                }
            }
            if let Some(max) = level_max {
                match item.crafting_target_level {
                    Some(lvl) if lvl <= max => {}
                    _ => return false,
                }
            }
            // Armor type filter (keyword-based: ClothArmor, LeatherArmor, MetalArmor, OrganicArmor)
            if let Some(ref at) = armor_type {
                let keyword = match at.as_str() {
                    "Cloth" => "ClothArmor",
                    "Leather" => "LeatherArmor",
                    "Metal" => "MetalArmor",
                    "Organic" => "OrganicArmor",
                    _ => return false,
                };
                if !item.keywords.iter().any(|k| k == keyword) {
                    return false;
                }
            }
            // Effect text filter (search within effect descriptions)
            if let Some(ref eq) = effect_q {
                if !eq.is_empty() {
                    let has_match = item.effect_descs.iter().any(|desc| {
                        // Check raw effect desc string
                        if desc.to_lowercase().contains(eq) {
                            return true;
                        }
                        // Also try resolving the effect for a human-readable match
                        if let Some(resolved) = resolve_single_effect(desc, &data) {
                            if resolved.formatted.to_lowercase().contains(eq)
                                || resolved.label.to_lowercase().contains(eq)
                            {
                                return true;
                            }
                        }
                        false
                    });
                    if !has_match {
                        // Also check the item description as fallback
                        let desc_match = item
                            .description
                            .as_ref()
                            .map(|d| d.to_lowercase().contains(eq))
                            .unwrap_or(false);
                        if !desc_match {
                            return false;
                        }
                    }
                }
            }
            true
        })
        .cloned()
        .collect();
    results.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(results)
}

/// Return all items whose keywords list contains the given keyword.
#[tauri::command]
pub async fn get_items_by_keyword(
    keyword: String,
    state: State<'_, GameDataState>,
) -> Result<Vec<ItemInfo>, String> {
    let kw_lower = keyword.to_lowercase();
    let data = state.read().await;
    let mut results: Vec<ItemInfo> = data
        .items
        .values()
        .filter(|item| item.keywords.iter().any(|k| k.to_lowercase() == kw_lower))
        .cloned()
        .collect();
    results.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(results)
}

/// Info about a keyword used as a recipe ingredient slot.
#[derive(Debug, serde::Serialize, Clone)]
pub struct IngredientKeywordInfo {
    pub keyword: String,
    pub description: String,
}

/// Return all distinct keywords used as wildcard ingredient slots in recipes,
/// along with their player-facing descriptions (from the CDN `Desc` field).
#[tauri::command]
pub async fn get_recipe_ingredient_keywords(
    state: State<'_, GameDataState>,
) -> Result<Vec<IngredientKeywordInfo>, String> {
    let data = state.read().await;
    let mut keyword_descs: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();

    // Iterate recipes to collect keyword → first description mapping
    for recipe in data.recipes.values() {
        for ingredient in &recipe.ingredients {
            if ingredient.item_id.is_none() && !ingredient.item_keys.is_empty() {
                let desc = ingredient
                    .description
                    .clone()
                    .unwrap_or_else(|| ingredient.item_keys.join(", "));
                for keyword in &ingredient.item_keys {
                    keyword_descs
                        .entry(keyword.clone())
                        .or_insert_with(|| desc.clone());
                }
            }
        }
    }

    let mut results: Vec<IngredientKeywordInfo> = keyword_descs
        .into_iter()
        .map(|(keyword, description)| IngredientKeywordInfo {
            keyword,
            description,
        })
        .collect();
    results.sort_by(|a, b| a.description.cmp(&b.description));
    Ok(results)
}

/// Return a sorted list of all distinct keyword values across all items.
#[tauri::command]
pub async fn get_all_item_keywords(state: State<'_, GameDataState>) -> Result<Vec<String>, String> {
    let data = state.read().await;
    let mut keywords: Vec<String> = data
        .items
        .values()
        .flat_map(|item| item.keywords.iter().cloned())
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect();
    keywords.sort();
    Ok(keywords)
}

/// Return a sorted list of all distinct equip_slot values across all items.
#[tauri::command]
pub async fn get_equip_slots(state: State<'_, GameDataState>) -> Result<Vec<String>, String> {
    let data = state.read().await;
    let mut slots: Vec<String> = data
        .items
        .values()
        .filter_map(|item| item.equip_slot.clone())
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect();
    slots.sort();
    Ok(slots)
}

// ── Skill query commands ──────────────────────────────────────────────────────

/// Get all skills as a flat list.
#[tauri::command]
pub async fn get_all_skills(state: State<'_, GameDataState>) -> Result<Vec<SkillInfo>, String> {
    let data = state.read().await;
    let mut skills: Vec<SkillInfo> = data.skills.values().cloned().collect();
    skills.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(skills)
}

/// Get the XP amounts array for a skill's XP table.
/// Returns the cumulative XP needed per level, or empty if not found.
#[tauri::command]
pub async fn get_xp_table_for_skill(
    skill_name: String,
    state: State<'_, GameDataState>,
) -> Result<Vec<u64>, String> {
    let data = state.read().await;

    // Find the skill
    let skill = data
        .skill_by_name(&skill_name)
        .ok_or_else(|| format!("Skill not found: {skill_name}"))?;

    // Get the XP table name
    let table_name = skill
        .xp_table
        .as_deref()
        .ok_or_else(|| format!("Skill {skill_name} has no XP table"))?;

    // Find the matching XP table by internal name
    for table in data.xp_tables.values() {
        if table.internal_name.as_deref() == Some(table_name) {
            return Ok(table.xp_amounts.clone());
        }
    }

    Err(format!(
        "XP table '{table_name}' not found for skill {skill_name}"
    ))
}

// ── Ability query commands ────────────────────────────────────────────────────

/// Get all abilities for a given skill name.
#[tauri::command]
pub async fn get_abilities_for_skill(
    skill: String,
    state: State<'_, GameDataState>,
) -> Result<Vec<AbilityInfo>, String> {
    let data = state.read().await;

    // Abilities store skill as internal name (e.g. "FireMagic"), but callers
    // may pass the display name ("Fire Magic"). Try both.
    let internal_name = data
        .skills
        .values()
        .find(|s| s.name == skill)
        .map(|s| s.internal_name.clone());
    let match_name = internal_name.as_deref().unwrap_or(&skill);

    let mut abilities: Vec<AbilityInfo> = data
        .abilities
        .values()
        .filter(|a| a.skill.as_deref() == Some(match_name))
        .cloned()
        .collect();
    abilities.sort_by(|a, b| {
        a.level
            .unwrap_or(0.0)
            .partial_cmp(&b.level.unwrap_or(0.0))
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    Ok(abilities)
}

/// Get all ability families for a given skill, sorted by base ability level.
#[tauri::command]
pub async fn get_ability_families_for_skill(
    skill: String,
    include_monster: Option<bool>,
    state: State<'_, GameDataState>,
) -> Result<Vec<AbilityFamily>, String> {
    let data = state.read().await;
    let show_monster = include_monster.unwrap_or(false);

    // Resolve display name → internal name
    let internal_name = data
        .skills
        .values()
        .find(|s| s.name == skill)
        .map(|s| s.internal_name.clone());
    let match_name = internal_name.as_deref().unwrap_or(&skill);

    let mut families: Vec<AbilityFamily> = data
        .ability_families
        .values()
        .filter(|f| {
            f.skill.as_deref() == Some(match_name) && (show_monster || !f.is_monster_ability)
        })
        .cloned()
        .collect();

    // Sort by the level of the first (lowest) tier
    families.sort_by(|a, b| {
        let level_a = a
            .tier_ids
            .first()
            .and_then(|id| data.abilities.get(id))
            .and_then(|ab| ab.level)
            .unwrap_or(0.0);
        let level_b = b
            .tier_ids
            .first()
            .and_then(|id| data.abilities.get(id))
            .and_then(|ab| ab.level)
            .unwrap_or(0.0);
        level_a
            .partial_cmp(&level_b)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    Ok(families)
}

/// Given any tier's ability ID, return the family it belongs to.
#[tauri::command]
pub async fn get_ability_family(
    ability_id: u32,
    state: State<'_, GameDataState>,
) -> Result<Option<AbilityFamily>, String> {
    let data = state.read().await;
    let family = data
        .ability_to_family
        .get(&ability_id)
        .and_then(|key| data.ability_families.get(key))
        .cloned();
    Ok(family)
}

/// Search ability families by name/description across all skills (or within one skill).
/// Returns up to `limit` matching families, sorted by base name.
#[tauri::command]
pub async fn search_ability_families(
    query: String,
    skill: Option<String>,
    limit: Option<usize>,
    include_monster: Option<bool>,
    state: State<'_, GameDataState>,
) -> Result<Vec<AbilityFamily>, String> {
    let data = state.read().await;
    let lower = query.to_lowercase();
    let max = limit.unwrap_or(50);
    let show_monster = include_monster.unwrap_or(false);

    // Resolve skill display name → internal name if provided
    let skill_internal = skill.as_ref().and_then(|s| {
        data.skills
            .values()
            .find(|sk| sk.name == *s)
            .map(|sk| sk.internal_name.clone())
    });
    let skill_match = skill_internal.as_deref().or(skill.as_deref());

    let mut results: Vec<AbilityFamily> = data
        .ability_families
        .values()
        .filter(|f| {
            // Filter out monster abilities unless requested
            if !show_monster && f.is_monster_ability {
                return false;
            }
            // Filter by skill if specified
            if let Some(sm) = skill_match {
                if f.skill.as_deref() != Some(sm) {
                    return false;
                }
            }
            // Match on family base_name first (fast path)
            if f.base_name.to_lowercase().contains(&lower) {
                return true;
            }
            // Fall back to checking individual tier names and descriptions
            for tier_id in &f.tier_ids {
                if let Some(ability) = data.abilities.get(tier_id) {
                    if ability.name.to_lowercase().contains(&lower) {
                        return true;
                    }
                    if let Some(desc) = &ability.description {
                        if desc.to_lowercase().contains(&lower) {
                            return true;
                        }
                    }
                }
            }
            false
        })
        .take(max)
        .cloned()
        .collect();

    results.sort_by(|a, b| a.base_name.to_lowercase().cmp(&b.base_name.to_lowercase()));
    if results.len() > max {
        results.truncate(max);
    }
    Ok(results)
}

/// Return a map of skill display name → number of ability families for that skill.
/// Used to populate the skill filter dropdown without N+1 IPC calls.
#[tauri::command]
pub async fn get_skills_with_ability_counts(
    include_monster: Option<bool>,
    state: State<'_, GameDataState>,
) -> Result<Vec<(String, usize)>, String> {
    let data = state.read().await;
    let show_monster = include_monster.unwrap_or(false);

    // Build internal_name → display_name map
    let internal_to_display: HashMap<&str, &str> = data
        .skills
        .values()
        .map(|s| (s.internal_name.as_str(), s.name.as_str()))
        .collect();

    // Count families per skill internal name
    let mut counts: HashMap<&str, usize> = HashMap::new();
    for family in data.ability_families.values() {
        if !show_monster && family.is_monster_ability {
            continue;
        }
        if let Some(skill) = family.skill.as_deref() {
            *counts.entry(skill).or_insert(0) += 1;
        }
    }

    // Convert to display names and sort
    let mut result: Vec<(String, usize)> = counts
        .into_iter()
        .filter_map(|(internal, count)| {
            let display = internal_to_display.get(internal).unwrap_or(&internal);
            Some((display.to_string(), count))
        })
        .filter(|(_, count)| *count > 0)
        .collect();
    result.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(result)
}

// ── Recipe query commands ─────────────────────────────────────────────────────

/// Get all recipes that produce a given item ID.
#[tauri::command]
pub async fn get_recipes_for_item(
    item_id: u32,
    state: State<'_, GameDataState>,
) -> Result<Vec<RecipeInfo>, String> {
    let data = state.read().await;
    let results: Vec<RecipeInfo> = data
        .recipes_producing_item
        .get(&item_id)
        .map(|ids| {
            ids.iter()
                .filter_map(|id| data.recipes.get(id).cloned())
                .collect()
        })
        .unwrap_or_default();
    Ok(results)
}

/// Get all recipes that require a given item ID as an ingredient.
#[tauri::command]
pub async fn get_recipes_using_item(
    item_id: u32,
    state: State<'_, GameDataState>,
) -> Result<Vec<RecipeInfo>, String> {
    let data = state.read().await;
    let results: Vec<RecipeInfo> = data
        .recipes_using_item
        .get(&item_id)
        .map(|ids| {
            ids.iter()
                .filter_map(|id| data.recipes.get(id).cloned())
                .collect()
        })
        .unwrap_or_default();
    Ok(results)
}

/// Search recipes by name.
#[tauri::command]
pub async fn search_recipes(
    query: String,
    limit: Option<usize>,
    state: State<'_, GameDataState>,
) -> Result<Vec<RecipeInfo>, String> {
    let data = state.read().await;
    let q = query.to_lowercase();
    let limit = limit.unwrap_or(50);

    let mut results: Vec<RecipeInfo> = data
        .recipes
        .values()
        .filter(|r| r.name.to_lowercase().contains(&q))
        .cloned()
        .collect();

    results.sort_by(|a, b| a.name.cmp(&b.name));
    results.truncate(limit);
    Ok(results)
}

/// Get all recipes for a given skill.
#[tauri::command]
pub async fn get_recipes_for_skill(
    skill: String,
    state: State<'_, GameDataState>,
) -> Result<Vec<RecipeInfo>, String> {
    let data = state.read().await;

    let recipe_ids = data.recipes_by_skill.get(&skill);
    let results: Vec<RecipeInfo> = match recipe_ids {
        Some(ids) => ids
            .iter()
            .filter_map(|id| data.recipes.get(id))
            .cloned()
            .collect(),
        None => vec![],
    };

    Ok(results)
}

/// Get all recipes that produce any of the given item IDs.
/// Used by Cook's Helper to find all food-producing recipes without hardcoding skill names.
#[tauri::command]
pub async fn get_recipes_producing_items(
    item_ids: Vec<u32>,
    state: State<'_, GameDataState>,
) -> Result<Vec<RecipeInfo>, String> {
    let data = state.read().await;
    let mut seen = std::collections::HashSet::new();
    let mut results = Vec::new();

    for item_id in &item_ids {
        if let Some(recipe_ids) = data.recipes_producing_item.get(item_id) {
            for recipe_id in recipe_ids {
                if seen.insert(*recipe_id) {
                    if let Some(recipe) = data.recipes.get(recipe_id) {
                        results.push(recipe.clone());
                    }
                }
            }
        }
    }

    Ok(results)
}

// ── Moon phase commands ──────────────────────────────────────────────────────

#[derive(serde::Serialize)]
pub struct MoonPhaseResult {
    /// Game phase name (e.g. "FullMoon", "WaningGibbousMoon")
    pub game_phase: String,
    /// Human-readable label (e.g. "Full Moon", "Waning Gibbous")
    pub label: String,
    /// 0-7 index into the phase cycle (0 = New Moon)
    pub phase_index: u8,
    /// Days until each future phase, in cycle order starting from the next phase
    pub days_until: Vec<DaysUntilPhase>,
}

#[derive(serde::Serialize)]
pub struct DaysUntilPhase {
    pub game_phase: String,
    pub label: String,
    pub days: u32,
}

const GAME_PHASE_NAMES: &[(&str, &str)] = &[
    ("NewMoon", "New Moon"),
    ("WaxingCrescentMoon", "Waxing Crescent"),
    ("QuarterMoon", "First Quarter"),
    ("WaxingGibbousMoon", "Waxing Gibbous"),
    ("FullMoon", "Full Moon"),
    ("WaningGibbousMoon", "Waning Gibbous"),
    ("LastQuarterMoon", "Last Quarter"),
    ("WaningCrescentMoon", "Waning Crescent"),
];

// ── Meeus lunar algorithms (ported from the game server's Moon.java) ─────────

/// Mean elongation of the moon (Meeus 47.2).
fn meeus_d(t: f64) -> f64 {
    297.8501921 + t * (445267.1114034 + t * (-0.0018819 + t * (1.0 / 545868.0 - t / 113065000.0)))
}

/// Sun's mean anomaly (Meeus 47.3).
fn meeus_m(t: f64) -> f64 {
    357.5291092 + t * (35999.0502909 + t * (-0.0001536 + t / 24490000.0))
}

/// Moon's mean anomaly (Meeus 47.4).
fn meeus_m_prime(t: f64) -> f64 {
    134.9633964 + t * (477198.8675055 + t * (0.0087414 + t * (1.0 / 69699.0 - t / 14712000.0)))
}

/// Phase angle between sun, moon, and earth (Meeus 48.4).
fn meeus_phase_angle(t: f64) -> f64 {
    let d = meeus_d(t);
    let m = meeus_m(t);
    let mp = meeus_m_prime(t);
    180.0 - d - 6.289 * mp.to_radians().sin() + 2.100 * m.to_radians().sin()
        - 1.274 * (2.0 * d - mp).to_radians().sin()
        - 0.658 * (2.0 * d).to_radians().sin()
        - 0.214 * (2.0 * mp).to_radians().sin()
        - 0.110 * d.to_radians().sin()
}

fn moon_is_waning(phase_angle: f64) -> bool {
    let s = phase_angle.to_radians().sin();
    if s < 0.0 {
        return true;
    }
    if s > 0.0 {
        return false;
    }
    phase_angle.to_radians().cos() > 0.0
}

fn moon_illuminated_fraction(phase_angle: f64) -> f64 {
    (1.0 + phase_angle.to_radians().cos()) * 0.5
}

/// Compute Julian Day number for a calendar date at midnight UTC (Meeus ch. 7).
fn calendar_to_jd(year: i32, month: u32, day: u32) -> f64 {
    let (y, m) = if month <= 2 {
        (year - 1, month + 12)
    } else {
        (year, month)
    };
    let a = (y as f64 / 100.0).floor() as i32;
    let b = 2 - a + (a as f64 / 4.0).floor() as i32;
    (365.25 * (y + 4716) as f64).floor()
        + (30.6001 * (m + 1) as f64).floor()
        + day as f64
        + b as f64
        - 1524.5
}

struct MoonDay {
    illuminated: f64,
    waning: bool,
}

/// Get the moon's illumination and waning state for a calendar date at midnight UTC.
fn moon_for_date(year: i32, month: u32, day: u32) -> MoonDay {
    let jd = calendar_to_jd(year, month, day);
    let t = (jd - 2451545.0) / 36525.0; // Meeus 22.1 — centuries since J2000.0
    let i = meeus_phase_angle(t);
    MoonDay {
        illuminated: moon_illuminated_fraction(i),
        waning: moon_is_waning(i),
    }
}

/// Get the current moon phase using the game server's exact algorithm.
///
/// The server (MoonPhaseCalculator.java) builds a 31-day window of daily
/// illumination data, then assigns the four major phases (Full, New, First
/// Quarter, Last Quarter) as exactly 3-day spans centered on the transition
/// day. Remaining days are filled with crescent/gibbous based on illumination
/// and waning state.
#[tauri::command]
pub async fn get_current_moon_phase() -> Result<MoonPhaseResult, String> {
    // Get today's date in Eastern timezone (game server time)
    let now = chrono::Utc::now();
    let eastern = chrono_tz::US::Eastern;
    let eastern_now = now.with_timezone(&eastern);
    let today = eastern_now.date_naive();

    // Build 31-day window starting from yesterday, matching the server's
    // calcPhasesForDate which uses `date.plusDays(loop - 1)` for loop 0..31.
    // phases[1] = today's phase.
    let window_start = today - chrono::Duration::days(1);
    let mut days: Vec<(chrono::NaiveDate, MoonDay)> = Vec::with_capacity(31);
    for i in 0..31 {
        let date = window_start + chrono::Duration::days(i);
        let md = moon_for_date(date.year(), date.month(), date.day());
        days.push((date, md));
    }

    // Assign major phases (exactly 3 days each), matching the server's algorithm.
    let mut assigned: Vec<Option<u8>> = vec![None; 31];

    // Full Moon: transition from non-waning to waning
    for i in 0..30 {
        if !days[i].1.waning && days[i + 1].1.waning {
            if i > 0 {
                assigned[i - 1] = Some(4);
            }
            assigned[i] = Some(4);
            if i + 1 < 31 {
                assigned[i + 1] = Some(4);
            }
            break;
        }
    }

    // New Moon: transition from waning to non-waning
    for i in 0..30 {
        if days[i].1.waning && !days[i + 1].1.waning {
            if i > 0 {
                assigned[i - 1] = Some(0);
            }
            assigned[i] = Some(0);
            if i + 1 < 31 {
                assigned[i + 1] = Some(0);
            }
            break;
        }
    }

    // First Quarter: illumination crosses 0.5 upward while waxing
    for i in 0..30 {
        if !days[i].1.waning
            && !days[i + 1].1.waning
            && days[i].1.illuminated <= 0.5
            && days[i + 1].1.illuminated > 0.5
        {
            if i > 0 {
                assigned[i - 1] = Some(2);
            }
            assigned[i] = Some(2);
            if i + 1 < 31 {
                assigned[i + 1] = Some(2);
            }
            break;
        }
    }

    // Last Quarter: illumination crosses 0.5 downward while waning
    for i in 0..30 {
        if days[i].1.waning
            && days[i + 1].1.waning
            && days[i].1.illuminated >= 0.5
            && days[i + 1].1.illuminated < 0.5
        {
            if i > 0 {
                assigned[i - 1] = Some(6);
            }
            assigned[i] = Some(6);
            if i + 1 < 31 {
                assigned[i + 1] = Some(6);
            }
            break;
        }
    }

    // Fill remaining days with crescent/gibbous based on illumination
    for i in 0..31 {
        if assigned[i].is_none() {
            assigned[i] = Some(if days[i].1.waning {
                if days[i].1.illuminated >= 0.5 {
                    5
                } else {
                    7
                }
            } else if days[i].1.illuminated <= 0.5 {
                1
            } else {
                3
            });
        }
    }

    // phases[1] = today's phase
    let current_index = assigned[1].unwrap();
    let (game_name, label) = GAME_PHASE_NAMES[current_index as usize];

    // Calculate days until each future phase by scanning forward from today (index 2+)
    let mut days_until = Vec::new();
    let mut seen_phases = std::collections::HashSet::new();
    seen_phases.insert(current_index);

    // Walk forward through the window, then extend if needed
    // First pass: use the 31-day window (indices 2..31 = tomorrow through day+29)
    for i in 2..31 {
        let phase_idx = assigned[i].unwrap();
        if !seen_phases.contains(&phase_idx) {
            seen_phases.insert(phase_idx);
            let (gn, lb) = GAME_PHASE_NAMES[phase_idx as usize];
            let day_offset = (i as u32) - 1; // days from today
            days_until.push(DaysUntilPhase {
                game_phase: gn.to_string(),
                label: lb.to_string(),
                days: day_offset,
            });
        }
    }

    // If we haven't found all 7 remaining phases in the window, extend the search.
    // This handles the rare case where a phase change falls beyond 29 days out.
    if seen_phases.len() < 8 {
        for extra_day in 30..=60u32 {
            let date = today + chrono::Duration::days(extra_day as i64);
            let md = moon_for_date(date.year(), date.month(), date.day());
            // Simple classification for extended days (no 3-day span logic needed
            // since we only need the first occurrence of each missing phase)
            let phase_idx = if md.waning {
                if md.illuminated >= 0.5 {
                    5
                } else {
                    7
                }
            } else if md.illuminated <= 0.5 {
                1
            } else {
                3
            };
            if !seen_phases.contains(&phase_idx) {
                seen_phases.insert(phase_idx);
                let (gn, lb) = GAME_PHASE_NAMES[phase_idx as usize];
                days_until.push(DaysUntilPhase {
                    game_phase: gn.to_string(),
                    label: lb.to_string(),
                    days: extra_day,
                });
            }
            if seen_phases.len() >= 8 {
                break;
            }
        }
    }

    Ok(MoonPhaseResult {
        game_phase: game_name.to_string(),
        label: label.to_string(),
        phase_index: current_index,
        days_until,
    })
}

// ── Quest query commands ──────────────────────────────────────────────────────

/// Get all quests.
#[tauri::command]
pub async fn get_all_quests(state: State<'_, GameDataState>) -> Result<Vec<QuestInfo>, String> {
    let data = state.read().await;
    let mut results: Vec<QuestInfo> = data.quests.values().cloned().collect();
    results.sort_by(|a, b| {
        // Sort by internal_name for now since we don't have parsed display names yet
        let a_name = a
            .raw
            .get("DisplayName")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let b_name = b
            .raw
            .get("DisplayName")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        a_name.cmp(b_name)
    });
    Ok(results)
}

/// Search quests by name.
#[tauri::command]
pub async fn search_quests(
    query: String,
    state: State<'_, GameDataState>,
) -> Result<Vec<QuestInfo>, String> {
    let data = state.read().await;
    let q = query.to_lowercase();

    let mut results: Vec<QuestInfo> = data
        .quests
        .values()
        .filter(|quest| {
            // Search in DisplayName, InternalName, or Description
            let display_name = quest
                .raw
                .get("DisplayName")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_lowercase();
            let description = quest
                .raw
                .get("Description")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_lowercase();

            display_name.contains(&q) || description.contains(&q)
        })
        .cloned()
        .collect();

    results.sort_by(|a, b| {
        let a_name = a
            .raw
            .get("DisplayName")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let b_name = b
            .raw
            .get("DisplayName")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        a_name.cmp(b_name)
    });

    Ok(results)
}

// ── NPC query commands ────────────────────────────────────────────────────────

/// Get all NPCs.
#[tauri::command]
pub async fn get_all_npcs(state: State<'_, GameDataState>) -> Result<Vec<NpcInfo>, String> {
    let data = state.read().await;
    let mut results: Vec<NpcInfo> = data.npcs.values().cloned().collect();
    results.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(results)
}

/// Search NPCs by name or description.
#[tauri::command]
pub async fn search_npcs(
    query: String,
    state: State<'_, GameDataState>,
) -> Result<Vec<NpcInfo>, String> {
    let data = state.read().await;
    let q = query.to_lowercase();

    let mut results: Vec<NpcInfo> = data
        .npcs
        .values()
        .filter(|npc| {
            let name_match = npc.name.to_lowercase().contains(&q);
            let desc_match = npc
                .desc
                .as_ref()
                .map(|d| d.to_lowercase().contains(&q))
                .unwrap_or(false);
            name_match || desc_match
        })
        .cloned()
        .collect();

    results.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(results)
}

// ── Achievement query commands ──────────────────────────────────────────────

/// Get all achievements, sorted by name.
#[tauri::command]
pub async fn get_all_achievements(
    state: State<'_, GameDataState>,
) -> Result<Vec<crate::game_data::AchievementInfo>, String> {
    let data = state.read().await;
    let mut results: Vec<crate::game_data::AchievementInfo> =
        data.achievements.values().cloned().collect();
    results.sort_by(|a, b| {
        a.name
            .as_deref()
            .unwrap_or(&a.id)
            .cmp(b.name.as_deref().unwrap_or(&b.id))
    });
    Ok(results)
}

/// Search achievements by name, description, or id (case-insensitive contains).
#[tauri::command]
pub async fn search_achievements(
    query: String,
    state: State<'_, GameDataState>,
) -> Result<Vec<crate::game_data::AchievementInfo>, String> {
    let data = state.read().await;
    let q = query.to_lowercase();

    let mut results: Vec<crate::game_data::AchievementInfo> = data
        .achievements
        .values()
        .filter(|ach| {
            let name_match = ach
                .name
                .as_ref()
                .is_some_and(|n| n.to_lowercase().contains(&q));
            let desc_match = ach
                .description
                .as_ref()
                .is_some_and(|d| d.to_lowercase().contains(&q));
            let id_match = ach.id.to_lowercase().contains(&q);
            name_match || desc_match || id_match
        })
        .cloned()
        .collect();

    results.sort_by(|a, b| {
        a.name
            .as_deref()
            .unwrap_or(&a.id)
            .cmp(b.name.as_deref().unwrap_or(&b.id))
    });
    Ok(results)
}

/// Get all NPCs in a specific area.
#[tauri::command]
pub async fn get_npcs_in_area(
    area: String,
    state: State<'_, GameDataState>,
) -> Result<Vec<NpcInfo>, String> {
    let data = state.read().await;

    let mut results: Vec<NpcInfo> = data
        .npcs
        .values()
        .filter(|npc| npc.area_name.as_ref().map(|a| a == &area).unwrap_or(false))
        .cloned()
        .collect();

    results.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(results)
}

/// Summary info for one area, used by the area browser list.
#[derive(serde::Serialize, Clone)]
pub struct AreaSummary {
    pub key: String,
    pub friendly_name: String,
    pub short_friendly_name: Option<String>,
    pub npc_count: usize,
    pub monster_count: usize,
}

/// Return all areas with NPC and monster counts for the browser list.
#[tauri::command]
pub async fn get_all_areas(
    app: AppHandle,
    state: State<'_, GameDataState>,
) -> Result<Vec<AreaSummary>, String> {
    let data = state.read().await;

    // Count NPCs per area
    let mut npc_counts: HashMap<String, usize> = HashMap::new();
    for npc in data.npcs.values() {
        if let Some(area) = &npc.area_name {
            *npc_counts.entry(area.clone()).or_default() += 1;
        }
    }

    // Count unique monsters per area from translation strings
    let monster_counts = load_monster_counts_by_area(&app);

    let mut results: Vec<AreaSummary> = data
        .areas
        .iter()
        .map(|(key, info)| {
            let friendly_name = info
                .friendly_name
                .clone()
                .unwrap_or_else(|| key.replace("Area", ""));
            AreaSummary {
                key: key.clone(),
                friendly_name,
                short_friendly_name: info.short_friendly_name.clone(),
                npc_count: npc_counts.get(key).copied().unwrap_or(0),
                monster_count: monster_counts.get(key).copied().unwrap_or(0),
            }
        })
        .collect();

    results.sort_by(|a, b| a.friendly_name.cmp(&b.friendly_name));
    Ok(results)
}

/// Parse translation strings once and return unique monster counts per area key.
fn load_monster_counts_by_area(app: &AppHandle) -> HashMap<String, usize> {
    let data_dir = match data_cache_dir(app) {
        Ok(d) => d,
        Err(_) => return HashMap::new(),
    };
    let path = data_dir.join("translation").join("strings_requested.json");
    let bytes = match std::fs::read_to_string(&path) {
        Ok(b) => b,
        Err(_) => return HashMap::new(),
    };
    let map: HashMap<String, String> = match serde_json::from_str(&bytes) {
        Ok(m) => m,
        Err(_) => return HashMap::new(),
    };

    let mut area_monsters: HashMap<String, std::collections::HashSet<String>> = HashMap::new();
    for (key, name) in &map {
        if !key.starts_with("monster_Area") || !key.ends_with("_Name") {
            continue;
        }
        // Key format: monster_{AreaKey}/{MonsterId}_Name
        let without_prefix = &key["monster_".len()..];
        if let Some(slash) = without_prefix.find('/') {
            let area_key = &without_prefix[..slash];
            area_monsters
                .entry(area_key.to_string())
                .or_default()
                .insert(name.clone());
        }
    }

    area_monsters
        .into_iter()
        .map(|(k, v)| (k, v.len()))
        .collect()
}

/// Get monsters that spawn in a specific area, extracted from translation strings.
/// Returns a sorted list of unique monster display names.
#[tauri::command]
pub async fn get_monsters_in_area(app: AppHandle, area: String) -> Result<Vec<String>, String> {
    let data_dir = data_cache_dir(&app)?;
    let path = data_dir.join("translation").join("strings_requested.json");

    if !path.exists() {
        return Ok(Vec::new());
    }

    let json = tokio::fs::read_to_string(&path)
        .await
        .map_err(|e| format!("Failed to read translation strings: {e}"))?;

    let map: HashMap<String, String> = serde_json::from_str(&json)
        .map_err(|e| format!("Failed to parse translation strings: {e}"))?;

    // Keys look like: monster_AreaSerbule/GiantRat_Name = "Giant Rat"
    // We match keys starting with "monster_{area}/" and ending with "_Name"
    let prefix = format!("monster_{area}/");
    let mut names: Vec<String> = map
        .iter()
        .filter(|(k, _)| k.starts_with(&prefix) && k.ends_with("_Name"))
        .map(|(_, v)| v.clone())
        .collect();

    names.sort();
    names.dedup();
    Ok(names)
}

// ── Effect query commands ────────────────────────────────────────────────────

/// Search effects by name or description.
#[tauri::command]
pub async fn search_effects(
    query: String,
    limit: Option<usize>,
    state: State<'_, GameDataState>,
) -> Result<Vec<EffectInfo>, String> {
    let data = state.read().await;
    let q = query.to_lowercase();
    let limit = limit.unwrap_or(50);

    let mut results: Vec<EffectInfo> = data
        .effects
        .values()
        .filter(|e| {
            let name_match = e
                .name
                .as_ref()
                .map(|n| n.to_lowercase().contains(&q))
                .unwrap_or(false);
            let desc_match = e
                .desc
                .as_ref()
                .map(|d| d.to_lowercase().contains(&q))
                .unwrap_or(false);
            name_match || desc_match
        })
        .cloned()
        .collect();

    results.sort_by(|a, b| {
        let a_name = a.name.as_deref().unwrap_or("");
        let b_name = b.name.as_deref().unwrap_or("");
        a_name.cmp(b_name)
    });
    results.truncate(limit);
    Ok(results)
}

/// Get a single effect by ID.
#[tauri::command]
pub async fn get_effect(
    id: u32,
    state: State<'_, GameDataState>,
) -> Result<Option<EffectInfo>, String> {
    Ok(state.read().await.effects.get(&id).cloned())
}

// ── Player Title query commands ─────────────────────────────────────────────

/// Get all player titles.
#[tauri::command]
pub async fn get_all_player_titles(
    state: State<'_, GameDataState>,
) -> Result<Vec<PlayerTitleInfo>, String> {
    let data = state.read().await;
    let mut results: Vec<PlayerTitleInfo> = data.player_titles.values().cloned().collect();
    results.sort_by(|a, b| {
        let a_title = a.title.as_deref().unwrap_or("");
        let b_title = b.title.as_deref().unwrap_or("");
        a_title.cmp(b_title)
    });
    Ok(results)
}

/// Search player titles by title text or tooltip.
#[tauri::command]
pub async fn search_player_titles(
    query: String,
    state: State<'_, GameDataState>,
) -> Result<Vec<PlayerTitleInfo>, String> {
    let data = state.read().await;
    let q = query.to_lowercase();

    let mut results: Vec<PlayerTitleInfo> = data
        .player_titles
        .values()
        .filter(|t| {
            let title_match = t
                .title
                .as_ref()
                .map(|n| n.to_lowercase().contains(&q))
                .unwrap_or(false);
            let tooltip_match = t
                .tooltip
                .as_ref()
                .map(|d| d.to_lowercase().contains(&q))
                .unwrap_or(false);
            title_match || tooltip_match
        })
        .cloned()
        .collect();

    results.sort_by(|a, b| {
        let a_title = a.title.as_deref().unwrap_or("");
        let b_title = b.title.as_deref().unwrap_or("");
        a_title.cmp(b_title)
    });
    Ok(results)
}

// ── Icon commands ─────────────────────────────────────────────────────────────

/// Returns the local filesystem path to an icon, fetching it from CDN if not cached.
/// The frontend can use this path with Tauri's asset protocol.
#[tauri::command]
pub async fn get_icon_path(
    icon_id: u32,
    app: AppHandle,
    state: State<'_, GameDataState>,
) -> Result<String, String> {
    let icon_dir = icon_cache_dir(&app)?;
    let version = state.read().await.version;

    if version == 0 {
        return Err("Game data not loaded yet".to_string());
    }

    let path: std::path::PathBuf = cdn::get_or_fetch_icon(version, icon_id, &icon_dir).await?;
    Ok(path.to_string_lossy().to_string())
}

// ── Source query commands ────────────────────────────────────────────────────

#[derive(serde::Serialize)]
pub struct QuestSummary {
    pub key: String,
    pub name: String,
}

#[derive(serde::Serialize)]
pub struct EntitySources {
    pub cdn_sources: Vec<SourceEntry>,
    pub bestowed_by_items: Vec<ItemInfo>,
    pub rewarded_by_quests: Vec<QuestSummary>,
}

/// Get all known sources for an ability.
#[tauri::command]
pub async fn get_ability_sources(
    id: u32,
    state: State<'_, GameDataState>,
) -> Result<EntitySources, String> {
    let data = state.read().await;

    let cdn_sources = data
        .sources
        .abilities
        .get(&id)
        .map(|s| s.entries.clone())
        .unwrap_or_default();

    // Find items that bestow this ability via bestow_ability field.
    // bestow_ability stores the internal name like "ability_1002"
    let ability_key = format!("ability_{id}");
    let bestowed_by_items = data
        .items_bestowing_ability
        .get(&ability_key)
        .map(|item_ids| {
            item_ids
                .iter()
                .filter_map(|iid| data.items.get(iid))
                .cloned()
                .collect()
        })
        .unwrap_or_default();

    Ok(EntitySources {
        cdn_sources,
        bestowed_by_items,
        rewarded_by_quests: vec![],
    })
}

/// Get all known sources for an item.
#[tauri::command]
pub async fn get_item_sources(
    id: u32,
    state: State<'_, GameDataState>,
) -> Result<EntitySources, String> {
    let data = state.read().await;

    let cdn_sources = data
        .sources
        .items
        .get(&id)
        .map(|s| s.entries.clone())
        .unwrap_or_default();

    // Find quests that reward this item
    let item_key = format!("item_{id}");
    let rewarded_by_quests = data
        .quests_rewarding_item
        .get(&item_key)
        .map(|quest_keys| {
            quest_keys
                .iter()
                .filter_map(|qk| {
                    let quest = data.quests.get(qk)?;
                    let name = quest
                        .raw
                        .get("Name")
                        .and_then(|v| v.as_str())
                        .unwrap_or(qk.as_str())
                        .to_string();
                    Some(QuestSummary {
                        key: qk.clone(),
                        name,
                    })
                })
                .collect()
        })
        .unwrap_or_default();

    Ok(EntitySources {
        cdn_sources,
        bestowed_by_items: vec![],
        rewarded_by_quests,
    })
}

/// Get all known sources for a recipe.
#[tauri::command]
pub async fn get_recipe_sources(
    id: u32,
    state: State<'_, GameDataState>,
) -> Result<EntitySources, String> {
    let data = state.read().await;

    let cdn_sources = data
        .sources
        .recipes
        .get(&id)
        .map(|s| s.entries.clone())
        .unwrap_or_default();

    // Find items that bestow this recipe
    let recipe_key = format!("recipe_{id}");
    let bestowed_by_items = data
        .items_bestowing_recipe
        .get(&recipe_key)
        .map(|item_ids| {
            item_ids
                .iter()
                .filter_map(|iid| data.items.get(iid))
                .cloned()
                .collect()
        })
        .unwrap_or_default();

    Ok(EntitySources {
        cdn_sources,
        bestowed_by_items,
        rewarded_by_quests: vec![],
    })
}

/// Get all known sources for a quest (items that bestow it).
#[tauri::command]
pub async fn get_quest_sources(
    key: String,
    state: State<'_, GameDataState>,
) -> Result<EntitySources, String> {
    let data = state.read().await;

    let bestowed_by_items = data
        .items_bestowing_quest
        .get(&key)
        .map(|item_ids| {
            item_ids
                .iter()
                .filter_map(|iid| data.items.get(iid))
                .cloned()
                .collect()
        })
        .unwrap_or_default();

    Ok(EntitySources {
        cdn_sources: vec![],
        bestowed_by_items,
        rewarded_by_quests: vec![],
    })
}

/// Return the set of item IDs that are purchasable from NPCs (Barter or Vendor sources).
/// Used by crafting projects to accurately split shopping lists.
#[tauri::command]
pub async fn get_vendor_purchasable_item_ids(
    state: State<'_, GameDataState>,
) -> Result<Vec<u32>, String> {
    let data = state.read().await;
    let mut ids = Vec::new();

    for (&item_id, source_info) in &data.sources.items {
        let is_vendor = source_info
            .entries
            .iter()
            .any(|e| e.source_type == "Barter" || e.source_type == "Vendor");
        if is_vendor {
            ids.push(item_id);
        }
    }

    Ok(ids)
}

/// A compact item summary for vendor inventory display.
#[derive(serde::Serialize, Clone)]
pub struct VendorItemSummary {
    pub item_id: u32,
    pub name: String,
    pub value: Option<f32>,
    pub icon_id: Option<u32>,
}

/// Get the items sold by a specific NPC (Vendor + Barter sources).
/// Returns resolved item summaries for display.
#[tauri::command]
pub async fn get_npc_vendor_items(
    npc_key: String,
    state: State<'_, GameDataState>,
) -> Result<Vec<VendorItemSummary>, String> {
    let data = state.read().await;

    let item_ids = match data.vendor_items_by_npc.get(&npc_key) {
        Some(ids) => ids,
        None => return Ok(vec![]),
    };

    let mut results: Vec<VendorItemSummary> = item_ids
        .iter()
        .filter_map(|&id| {
            let item = data.items.get(&id)?;
            Some(VendorItemSummary {
                item_id: id,
                name: item.name.clone(),
                value: item.value,
                icon_id: item.icon_id,
            })
        })
        .collect();

    results.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(results)
}

/// Get the count of vendor items per NPC (for showing counts in NPC list).
#[tauri::command]
pub async fn get_vendor_item_counts(
    state: State<'_, GameDataState>,
) -> Result<std::collections::HashMap<String, usize>, String> {
    let data = state.read().await;
    let counts: std::collections::HashMap<String, usize> = data
        .vendor_items_by_npc
        .iter()
        .map(|(npc_key, items)| (npc_key.clone(), items.len()))
        .collect();
    Ok(counts)
}

/// Get the NPC keys and names that sell/barter a given item (for item tooltips).
#[derive(serde::Serialize, Clone)]
pub struct VendorNpcSummary {
    pub npc_key: String,
    pub name: String,
}

#[tauri::command]
pub async fn get_vendors_for_item(
    item_id: u32,
    state: State<'_, GameDataState>,
) -> Result<Vec<VendorNpcSummary>, String> {
    let data = state.read().await;
    let npc_keys = match data.vendors_for_item.get(&item_id) {
        Some(keys) => keys,
        None => return Ok(vec![]),
    };
    let mut results: Vec<VendorNpcSummary> = npc_keys
        .iter()
        .filter_map(|key| {
            let npc = data.npcs.get(key)?;
            Some(VendorNpcSummary {
                npc_key: key.clone(),
                name: npc.name.clone(),
            })
        })
        .collect();
    results.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(results)
}

// ── Effect description resolution ───────────────────────────────────────────

#[derive(serde::Serialize, Clone)]
pub struct ResolvedEffect {
    pub label: String,
    pub value: String,
    pub display_type: String,
    pub formatted: String,
    pub icon_id: Option<u32>,
}

/// Resolve `{TOKEN}{VALUE}` style effect descriptions into human-readable text.
#[tauri::command]
pub async fn resolve_effect_descs(
    descs: Vec<String>,
    state: State<'_, GameDataState>,
) -> Result<Vec<ResolvedEffect>, String> {
    let data = state.read().await;
    let mut results = Vec::new();

    for desc in &descs {
        if let Some(resolved) = resolve_single_effect(desc, &data) {
            results.push(resolved);
        } else {
            // Fallback: return the raw string
            results.push(ResolvedEffect {
                label: desc.clone(),
                value: String::new(),
                display_type: String::new(),
                formatted: desc.clone(),
                icon_id: None,
            });
        }
    }

    Ok(results)
}

/// Strip `<icon=XXXX>` tags from text and return (cleaned_text, first_icon_id)
fn strip_icon_tags(text: &str) -> (String, Option<u32>) {
    let mut first_icon: Option<u32> = None;
    let mut result = String::with_capacity(text.len());
    let mut remaining = text;

    while let Some(start) = remaining.find("<icon=") {
        result.push_str(&remaining[..start]);
        if let Some(end) = remaining[start..].find('>') {
            let icon_str = &remaining[start + 6..start + end];
            if first_icon.is_none() {
                first_icon = icon_str.parse::<u32>().ok();
            }
            remaining = &remaining[start + end + 1..];
        } else {
            // Malformed tag, just keep it
            result.push_str(&remaining[start..start + 6]);
            remaining = &remaining[start + 6..];
        }
    }
    result.push_str(remaining);

    (result, first_icon)
}

/// Public wrapper for resolve_single_effect (used by brewing_commands)
pub fn resolve_single_effect_public(desc: &str, data: &GameData) -> Option<ResolvedEffect> {
    resolve_single_effect(desc, data)
}

fn resolve_single_effect(desc: &str, data: &GameData) -> Option<ResolvedEffect> {
    // Parse "{TOKEN}{VALUE}" format
    let parts: Vec<&str> = desc.split('{').filter(|s| !s.is_empty()).collect();
    if parts.len() >= 2 {
        let token = parts[0].trim_end_matches('}');
        let value_str = parts[1].trim_end_matches('}');

        if let Some(attr) = data.attributes.get(token) {
            if let Some(label) = attr.raw.get("Label").and_then(|v| v.as_str()) {
                let label = label.to_string();
                let display_type = attr
                    .raw
                    .get("DisplayType")
                    .and_then(|v| v.as_str())
                    .unwrap_or("AsInt")
                    .to_string();
                let icon_id = attr
                    .raw
                    .get("IconIds")
                    .and_then(|v| v.as_array())
                    .and_then(|arr| arr.first())
                    .and_then(|v| v.as_u64())
                    .map(|v| v as u32);

                let value: f64 = value_str.parse().unwrap_or(0.0);
                let formatted = format_effect_value(&label, value, &display_type);

                return Some(ResolvedEffect {
                    label,
                    value: value_str.to_string(),
                    display_type,
                    formatted,
                    icon_id,
                });
            }
        }
    }

    // Handle "<icon=XXXX>text" format (pre-rendered effect descriptions)
    if desc.contains("<icon=") {
        let (cleaned, icon_id) = strip_icon_tags(desc);
        let cleaned = cleaned.trim().to_string();
        return Some(ResolvedEffect {
            label: cleaned.clone(),
            value: String::new(),
            display_type: String::new(),
            formatted: cleaned,
            icon_id,
        });
    }

    None
}

fn format_effect_value(label: &str, value: f64, display_type: &str) -> String {
    match display_type {
        "AsPercent" => format!("{} +{}%", label, (value * 100.0).round() as i64),
        "AsBuffDelta" => format!("{} +{}", label, value.round() as i64),
        "AsDebuffDelta" => format!("{} {}", label, value.round() as i64),
        "AsBuffMod" => format!("{} +{}%", label, (value * 100.0).round() as i64),
        "AsDebuffMod" => format!("{} -{}%", label, (value * 100.0).abs().round() as i64),
        "AsInt" => format!("{} +{}", label, value.round() as i64),
        "AsDouble" | "AsDoubleTimes100" => {
            if value.fract() == 0.0 {
                format!("{} +{}", label, value as i64)
            } else {
                format!("{} +{:.2}", label, value)
            }
        }
        "AsBool" => label.to_string(),
        _ => format!("{} {}", label, value),
    }
}

// ── TSys power lookup ───────────────────────────────────────────────────────

#[derive(serde::Serialize)]
pub struct TsysPowerInfo {
    pub internal_name: String,
    pub skill: Option<String>,
    pub prefix: Option<String>,
    pub suffix: Option<String>,
    pub slots: Vec<String>,
    pub tier_effects: Vec<String>,
    /// Structured effect data for aggregation (label, numeric value, display_type)
    pub tier_effects_structured: Vec<ResolvedEffect>,
    /// Icon ID from the first effect's attribute definition
    pub icon_id: Option<u32>,
}

/// Look up a TSys power by internal name and tier, returning human-readable info.
#[tauri::command]
pub async fn get_tsys_power_info(
    power_name: String,
    tier: i64,
    state: State<'_, GameDataState>,
) -> Result<Option<TsysPowerInfo>, String> {
    let data = state.read().await;

    // Find the power in tsys client_info by internal_name
    let entry = data
        .tsys
        .client_info
        .values()
        .find(|info| info.internal_name.as_deref() == Some(&power_name));

    let Some(info) = entry else {
        return Ok(None);
    };

    // Get the effect descriptions for the requested tier
    let tier_key = format!("id_{}", tier);
    let mut tier_effects = Vec::new();
    let mut tier_effects_structured = Vec::new();
    let mut icon_id: Option<u32> = None;

    if let Some(tier_info) = info.tiers.get(&tier_key) {
        for desc in &tier_info.effect_descs {
            if let Some(resolved) = resolve_single_effect(desc, &data) {
                if icon_id.is_none() {
                    icon_id = resolved.icon_id;
                }
                tier_effects.push(resolved.formatted.clone());
                tier_effects_structured.push(resolved);
            } else {
                tier_effects.push(desc.to_string());
                tier_effects_structured.push(ResolvedEffect {
                    label: desc.to_string(),
                    value: String::new(),
                    display_type: String::new(),
                    formatted: desc.to_string(),
                    icon_id: None,
                });
            }
        }
    }

    // Resolve skill internal name to display name
    let resolved_skill = info
        .skill
        .as_deref()
        .and_then(|s| data.resolve_skill(s).map(|si| si.name.clone()))
        .or_else(|| info.skill.clone());

    Ok(Some(TsysPowerInfo {
        internal_name: power_name,
        skill: resolved_skill,
        prefix: info.prefix.clone(),
        suffix: info.suffix.clone(),
        slots: info.slots.clone(),
        tier_effects,
        tier_effects_structured,
        icon_id,
    }))
}

/// Batch-resolve multiple TSys powers at once.
/// Input: array of [powerName, tier] pairs.
/// Returns a map of "powerName:tier" → TsysPowerInfo.
#[tauri::command]
pub async fn get_tsys_power_info_batch(
    powers: Vec<(String, i64)>,
    state: State<'_, GameDataState>,
) -> Result<HashMap<String, TsysPowerInfo>, String> {
    let data = state.read().await;
    let mut result = HashMap::new();

    for (power_name, tier) in powers {
        let entry = data
            .tsys
            .client_info
            .values()
            .find(|info| info.internal_name.as_deref() == Some(&power_name));

        let Some(info) = entry else {
            continue;
        };

        let tier_key = format!("id_{}", tier);
        let mut tier_effects = Vec::new();
        let mut tier_effects_structured = Vec::new();
        let mut icon_id: Option<u32> = None;

        if let Some(tier_info) = info.tiers.get(&tier_key) {
            for desc in &tier_info.effect_descs {
                if let Some(resolved) = resolve_single_effect(desc, &data) {
                    if icon_id.is_none() {
                        icon_id = resolved.icon_id;
                    }
                    tier_effects.push(resolved.formatted.clone());
                    tier_effects_structured.push(resolved);
                } else {
                    tier_effects.push(desc.to_string());
                    tier_effects_structured.push(ResolvedEffect {
                        label: desc.to_string(),
                        value: String::new(),
                        display_type: String::new(),
                        formatted: desc.to_string(),
                        icon_id: None,
                    });
                }
            }
        }

        let resolved_skill = info
            .skill
            .as_deref()
            .and_then(|s| data.resolve_skill(s).map(|si| si.name.clone()))
            .or_else(|| info.skill.clone());

        let key = format!("{}:{}", power_name, tier);
        result.insert(
            key,
            TsysPowerInfo {
                internal_name: power_name,
                skill: resolved_skill,
                prefix: info.prefix.clone(),
                suffix: info.suffix.clone(),
                slots: info.slots.clone(),
                tier_effects,
                tier_effects_structured,
                icon_id,
            },
        );
    }

    Ok(result)
}

// ── Effective ability stats (build planner) ─────────────────────────────────

/// Parse the `{TOKEN}{VALUE}` head of an effect desc into (raw token, numeric value).
/// Parse a tsys/item effect desc in `{TOKEN}{VALUE}` form, keeping the token the display
/// resolver discards. A 3rd `{SKILL}` segment (e.g. `{MOD_NATURE_INDIRECT}{0.1}{Druid}`) marks
/// the effect as conditional on that skill being equipped — returned as the third tuple element
/// so the caller can gate it. (Every 3rd segment in the CDN data is a valid skill name.)
fn parse_token_value(desc: &str) -> Option<(String, f64, Option<String>)> {
    let segs: Vec<&str> = desc
        .split('{')
        .map(|s| s.trim().trim_end_matches('}').trim())
        .filter(|s| !s.is_empty())
        .collect();
    let token = (*segs.first()?).to_string();
    let value: f64 = segs.get(1)?.parse().ok()?;
    let condition = segs.get(2).map(|s| s.to_string());
    Some((token, value, condition))
}

/// Strip the leading run of `<icon=NNN>` tags most tsys prose descs open with, so the
/// anchored prose parsers see the sentence itself.
fn strip_leading_icons(desc: &str) -> String {
    static ICON: Lazy<regex::Regex> = Lazy::new(|| regex::Regex::new(r"^(?:<icon=\d+>)+").unwrap());
    ICON.replace(desc.trim(), "").trim().to_string()
}

/// Some conditional indirect-damage mods are stored as English prose instead of `{TOKEN}{VALUE}`.
/// Three phrasings exist in the data, differing only in wording:
/// ```text
/// Indirect Psychic damage is +53% per tick while Vampirism skill active
/// Indirect Nature and Indirect Trauma damage +30% while Cow skill active
/// Indirect Poison Damage, Indirect Trauma Damage, and Indirect Psychic Damage +30% while Sword skill active
/// ```
/// Each is parsed into the equivalent `MOD_<TYPE>_INDIRECT` percent tokens, one per damage type
/// named, conditional on the named skill. Returns `(token, fraction, skill)` triples.
fn parse_conditional_indirect_text(desc: &str) -> Vec<(String, f64, String)> {
    // The percent + skill tail, shared by all three phrasings ("is +53% per tick while …",
    // "damage +30% while …"). Anchored at the end so temporal/conditional riders don't match.
    static RE_TAIL: Lazy<regex::Regex> = Lazy::new(|| {
        regex::Regex::new(r"\+(\d+)% (?:per tick )?while (\w+) skill active\.?$").unwrap()
    });
    static RE_TYPE: Lazy<regex::Regex> =
        Lazy::new(|| regex::Regex::new(r"Indirect (\w+)").unwrap());

    let body = strip_leading_icons(desc);
    // Only the "Indirect <type> damage" family; mitigation, vulnerability and proc prose that
    // merely mentions indirect damage opens with something else.
    if !body.starts_with("Indirect ") || !body.to_lowercase().contains("damage") {
        return Vec::new();
    }
    let Some(tail) = RE_TAIL.captures(&body) else {
        return Vec::new();
    };
    let pct = tail[1].parse::<f64>().unwrap_or(0.0) / 100.0;
    let skill = tail[2].to_string();
    let head = &body[..tail.get(0).unwrap().start()];
    RE_TYPE
        .captures_iter(head)
        .filter(|c| {
            crate::game_data::ability_stats::DAMAGE_TYPES
                .iter()
                .any(|t| t.eq_ignore_ascii_case(&c[1]))
        })
        .map(|c| {
            (
                format!("MOD_{}_INDIRECT", c[1].to_uppercase()),
                pct,
                skill.clone(),
            )
        })
        .collect()
}

/// The *direct* counterparts of [`parse_conditional_indirect_text`], also stored as prose. Two
/// syntaxes exist in the data:
/// ```text
/// Direct Poison and Acid Damage +9 and Indirect Poison and Acid +5 (per tick) while Archery skill active.
/// Direct Electricity Damage, Direct Fire Damage, and Direct Cold Damage +11% while Hammer skill active
/// ```
/// The first grants flat `BOOST_<TYPE>_DIRECT` + `BOOST_<TYPE>_INDIRECT` per named type; the second
/// grants percent `MOD_<TYPE>_DIRECT` per named type. Returns `(token, value, skill)` triples, each
/// conditional on the named skill.
fn parse_conditional_direct_text(desc: &str) -> Vec<(String, f64, String)> {
    static RE_PAIR: Lazy<regex::Regex> = Lazy::new(|| {
        regex::Regex::new(
            r"^Direct (\w+)(?: and (\w+))? [Dd]amage \+(\d+) and Indirect (\w+)(?: and (\w+))? \+(\d+) \(per tick\) while (\w+) skill active",
        )
        .unwrap()
    });
    static RE_PCT_TAIL: Lazy<regex::Regex> =
        Lazy::new(|| regex::Regex::new(r"\+(\d+)% while (\w+) skill active").unwrap());
    static RE_DIRECT_TYPE: Lazy<regex::Regex> =
        Lazy::new(|| regex::Regex::new(r"Direct (\w+) [Dd]amage").unwrap());

    let d = strip_leading_icons(desc);
    let d = d.as_str();
    let mut out = Vec::new();
    if let Some(caps) = RE_PAIR.captures(d) {
        let flat_direct: f64 = caps[3].parse().unwrap_or(0.0);
        let flat_indirect: f64 = caps[6].parse().unwrap_or(0.0);
        let skill = caps[7].to_string();
        for i in [1usize, 2] {
            if let Some(t) = caps.get(i) {
                out.push((
                    format!("BOOST_{}_DIRECT", t.as_str().to_uppercase()),
                    flat_direct,
                    skill.clone(),
                ));
            }
        }
        for i in [4usize, 5] {
            if let Some(t) = caps.get(i) {
                out.push((
                    format!("BOOST_{}_INDIRECT", t.as_str().to_uppercase()),
                    flat_indirect,
                    skill.clone(),
                ));
            }
        }
        return out;
    }
    if d.starts_with("Direct ") {
        if let Some(tail) = RE_PCT_TAIL.captures(d) {
            let pct = tail[1].parse::<f64>().unwrap_or(0.0) / 100.0;
            let skill = tail[2].to_string();
            for t in RE_DIRECT_TYPE.captures_iter(d) {
                out.push((
                    format!("MOD_{}_DIRECT", t[1].to_uppercase()),
                    pct,
                    skill.clone(),
                ));
            }
        }
    }
    out
}

/// Pure regex extraction of a DoT-add prose mod's clause: the stated total damage, the damage type
/// (None when the prose omits it), and the duration in seconds. Handles the `+N` / `N` /
/// `an additional N` value forms and the optional `to health` / `to armor` qualifier. Deliberately
/// matches as a *substring* so trailing side-effect clauses are ignored rather than blocking a
/// match, but the value must be immediately followed by `[type] damage [to X] over M seconds`.
fn parse_dot_clause(desc: &str) -> Option<(f64, Option<String>, f64)> {
    static RE: Lazy<regex::Regex> = Lazy::new(|| {
        regex::Regex::new(
            r"(?:an additional )?\+?(\d+) (?:(\w+) )?damage(?: to (?:health|armor))? over (\d+) seconds",
        )
        .unwrap()
    });
    let caps = RE.captures(desc)?;
    Some((
        caps[1].parse().ok()?,
        caps.get(2).map(|m| m.as_str().to_string()),
        caps[3].parse().ok()?,
    ))
}

/// Strip leading `<icon=NNN>` tags, then greedily consume the run of known ability names that opens
/// a DoT-add prose mod ("Screech", "Hacking Blade and Debilitating Blow", "Screech, Sonic Burst,
/// and Deathscream"), returning their resolved ability ids. Stops at the first word that isn't part
/// of a known ability name, so descriptive verbs ("…coats the target in insects that deal…") end the
/// list. Returns empty for skill-wide prose that names no ability ("Archery attacks that crit…").
fn extract_leading_abilities(desc: &str, data: &GameData) -> Vec<u32> {
    extract_leading_abilities_rest(desc, data).0
}

/// Like [`extract_leading_abilities`], but also returns the text remaining after the consumed
/// ability names (whitespace-normalized), so callers can parse the clause that follows them
/// (e.g. `"Damage +39"` in "Many Cuts and Debilitating Blow Damage +39").
fn extract_leading_abilities_rest(desc: &str, data: &GameData) -> (Vec<u32>, String) {
    static ICON: Lazy<regex::Regex> = Lazy::new(|| regex::Regex::new(r"^(?:<icon=\d+>)+").unwrap());
    let body = ICON.replace(desc.trim(), "");
    let words: Vec<&str> = body.split_whitespace().collect();
    let mut ids = Vec::new();
    let mut i = 0;
    while i < words.len() {
        let mut matched: Option<(usize, u32)> = None;
        let max = (words.len() - i).min(4);
        for len in (1..=max).rev() {
            let cand = words[i..i + len].join(" ");
            let cand = cand.trim_end_matches([',', '.', ':']).trim();
            // Possessive form too ("Fairy Fire's damage type becomes Fire").
            let id = data.ability_name_index.get(cand).or_else(|| {
                cand.strip_suffix("'s")
                    .and_then(|c| data.ability_name_index.get(c))
            });
            if let Some(&id) = id {
                matched = Some((len, id));
                break;
            }
        }
        let Some((len, id)) = matched else { break };
        ids.push(id);
        i += len;
        // Skip an explicit "and"/"or" separator; comma separators are consumed with the name word.
        if words
            .get(i)
            .map(|w| {
                let w = w.trim_end_matches([',', '.']).to_lowercase();
                w == "and" || w == "or"
            })
            .unwrap_or(false)
        {
            i += 1;
        }
    }
    (ids, words[i.min(words.len())..].join(" "))
}

/// True when the text immediately following a parsed damage clause makes it conditional or
/// temporal ("… when you have low Armor", "… for 10 seconds", "… after a 3-second delay",
/// "… to Elite enemies") — such bonuses are excluded from the unconditional tooltip numbers.
/// Riders that don't qualify the damage itself ("and taunts +40", ". Non-Elite targets …",
/// ", and there's a 30% chance …") are fine.
fn conditional_tail(rest: &str) -> bool {
    static RE: Lazy<regex::Regex> = Lazy::new(|| {
        regex::Regex::new(r"^\s*(?:when|if|while|for \d|after|versus|vs\.?|against|to |per )")
            .unwrap()
    });
    RE.is_match(rest)
}

/// Parse the damage-amount clause that follows the leading ability names of a named-ability
/// direct-damage prose mod. Syntaxes in the data:
/// ```text
/// Damage +39            ("Many Cuts and Debilitating Blow Damage +39 …")
/// damage is +45         ("Pixie Flare's damage is +45 and its damage becomes Fire …")
/// deals +25% damage     ("Aimed Shot deals +25% damage and boosts …")
/// deal 30 armor damage  (bare value allowed only with an explicit type word)
/// deals Fire damage (instead of Darkness) and damage is +2   (conversion clause first)
/// ```
/// Returns `(value, is_percent)`, or `None` when the clause is absent or conditional/temporal.
fn parse_named_damage_rest(rest: &str) -> Option<(f64, bool)> {
    static RE_DAMAGE: Lazy<regex::Regex> =
        Lazy::new(|| regex::Regex::new(r"^[Dd]amage (?:is )?\+(\d+)(%)?").unwrap());
    static RE_DEAL: Lazy<regex::Regex> = Lazy::new(|| {
        regex::Regex::new(
            r"^deals? (\+)?(\d+)(%)?(?: ([A-Za-z]+))? [Dd]amage( to (?:[Aa]rmor|[Hh]ealth))?",
        )
        .unwrap()
    });
    // A leading type-conversion clause ("deals Fire damage (instead of Darkness) and damage is
    // +2") hides the damage clause from the anchored patterns below — skip past it. The
    // conversion itself is harvested separately by `harvest_type_rules`.
    static RE_CONV_LEAD: Lazy<regex::Regex> = Lazy::new(|| {
        regex::Regex::new(r"^deals? \w+ damage \(instead of \w+\),? and (?:its )?").unwrap()
    });
    let stripped = RE_CONV_LEAD.replace(rest, "");
    let rest: &str = &stripped;
    if let Some(caps) = RE_DAMAGE.captures(rest) {
        let value: f64 = caps[1].parse().ok()?;
        let pct = caps.get(2).is_some();
        if conditional_tail(&rest[caps.get(0).unwrap().end()..]) {
            return None;
        }
        return Some(if pct {
            (value / 100.0, true)
        } else {
            (value, false)
        });
    }
    if let Some(caps) = RE_DEAL.captures(rest) {
        let has_plus = caps.get(1).is_some();
        let value: f64 = caps[2].parse().ok()?;
        let pct = caps.get(3).is_some();
        let type_word = caps.get(4).map(|m| m.as_str());
        // A bare value ("deal 30 armor damage") is only additive when a type word is present;
        // otherwise require the explicit "+".
        if !has_plus && type_word.is_none() {
            return None;
        }
        if conditional_tail(&rest[caps.get(0).unwrap().end()..]) {
            return None;
        }
        return Some(if pct {
            (value / 100.0, true)
        } else {
            (value, false)
        });
    }
    None
}

/// Parse a skill- or keyword-wide "X attacks deal +N[%] damage" prose mod (e.g. "Hammer attacks
/// deal +33% damage but generate +25% Rage", "Bite attacks deal +11% damage and hasten …").
/// Returns `(subject_word, value, is_percent)`; the caller gates the subject against the target
/// ability's skill and keywords. Conditional variants ("… when you have …", "… damage to Elite
/// enemies") are rejected.
fn parse_attacks_deal(body: &str) -> Option<(String, f64, bool)> {
    static RE: Lazy<regex::Regex> = Lazy::new(|| {
        regex::Regex::new(r"^(?:Your )?([A-Za-z]+) attacks deal \+(\d+)(%)? damage").unwrap()
    });
    let caps = RE.captures(body)?;
    let value: f64 = caps[2].parse().ok()?;
    let pct = caps.get(3).is_some();
    let tail = &body[caps.get(0).unwrap().end()..];
    if conditional_tail(tail) {
        return None;
    }
    let value = if pct { value / 100.0 } else { value };
    Some((caps[1].to_string(), value, pct))
}

/// Parse a skill-wide "<Skill> Base Damage +N[%]" prose mod — the primary per-skill damage boost
/// some mods grant as prose rather than a `{MOD_<SKILL>}` token (e.g. the Necromancer's mod's
/// "Necromancy Base Damage +60%; Summoned Skeletons deal +12% direct damage"). Returns
/// `(skill_word, value, is_percent)`; the caller gates the skill against the target ability.
/// Conditional/temporal variants never match: they don't *begin* with the "<Skill> Base Damage"
/// clause — they're prefixed by their trigger ("Shadow Feint raises your Lycanthropy Base Damage
/// +79% until …", "Combo: … boosts Giant Bat Base Damage +60% for 10 seconds") — and a trailing
/// conditional rider is rejected outright.
fn parse_base_damage(body: &str) -> Option<(String, f64, bool)> {
    static RE: Lazy<regex::Regex> =
        Lazy::new(|| regex::Regex::new(r"^([A-Za-z]+) Base Damage \+(\d+)(%)?").unwrap());
    let caps = RE.captures(body)?;
    let value: f64 = caps[2].parse().ok()?;
    let pct = caps.get(3).is_some();
    if conditional_tail(&body[caps.get(0).unwrap().end()..]) {
        return None;
    }
    let value = if pct { value / 100.0 } else { value };
    Some((caps[1].to_string(), value, pct))
}

/// Pick the token to feed a prose-granted damage bonus through: prefer the ability-specific
/// token (`*_ABILITY_*`) so the breakdown label reads naturally, falling back to the bucket's
/// first entry. Any token from the bucket is arithmetically equivalent — the effects list is
/// computed per ability, so the grant can't leak to other abilities.
fn pick_bucket_token(arr: &[String]) -> Option<&String> {
    arr.iter()
        .find(|t| t.contains("_ABILITY_"))
        .or_else(|| arr.first())
}

/// Parse a skill-scoped damage-type conversion ("Staff abilities that normally deal Crushing
/// damage instead deal Slashing damage"). Returns `(skill, from_type, to_type)`.
fn parse_skill_type_conversion(body: &str) -> Option<(String, String, String)> {
    static RE: Lazy<regex::Regex> = Lazy::new(|| {
        regex::Regex::new(
            r"^(\w+) abilities that normally deal (\w+) damage instead deal (\w+) damage",
        )
        .unwrap()
    });
    let caps = RE.captures(body)?;
    Some((
        caps[1].to_string(),
        caps[2].to_string(),
        caps[3].to_string(),
    ))
}

/// Parse a named-ability damage-type conversion from the text remaining after the leading
/// ability names. The data phrases these several ways:
/// ```text
/// deals +30 damage, and the damage type becomes Fire.
/// deals +45 damage, and damage type is changed to Electricity
/// direct damage becomes Darkness damage.                       (Horns of Hate / Deadly Emission)
/// damage is +45 and its damage becomes Fire instead of Electricity   (Pixie Flare)
/// damage +19. Damage becomes Trauma instead of Crushing              (Infuriating Fist)
/// becomes a Sonic ability and deals Nature damage instead of Psychic (Embrace of Despair)
/// deals Fire damage (instead of Darkness) and ignites the target …   (SpiritFox fire mods)
/// ```
/// The "X, Y and Z deal direct Fire damage" item form is handled separately by
/// [`parse_names_deal_direct`] — its name lists use spellings the ability index can't always
/// resolve.
fn parse_named_type_conversion(rest: &str) -> Option<String> {
    static RE_BECOMES: Lazy<regex::Regex> = Lazy::new(|| {
        regex::Regex::new(r"[Dd]amage (?:type )?(?:becomes|is changed to) (\w+)").unwrap()
    });
    static RE_DEALS_INSTEAD: Lazy<regex::Regex> =
        Lazy::new(|| regex::Regex::new(r"deals? (\w+) damage \(?instead of").unwrap());
    if let Some(caps) = RE_BECOMES.captures(rest) {
        return Some(caps[1].to_string());
    }
    RE_DEALS_INSTEAD
        .captures(rest)
        .map(|caps| caps[1].to_string())
}

/// Parse the "X, Y and Z deal direct Fire damage" conversion form (Vanquisher's Blade, Flaming
/// Gazluk Sword). The item texts spell some ability names without spaces ("Windstrike" for
/// "Wind Strike"), which breaks index-based leading-name extraction — one unresolvable name
/// stops the walk and hides the clause from every ability in the list. So instead the comma /
/// "and"-separated name segments before the clause are compared against the target ability's
/// tier-stripped name, both normalized to lowercase alphanumerics (spacing-insensitive).
/// Conditional prose ("… causes your next attack to deal +50 damage if it deals direct Psychic
/// …") can't match: its pre-clause text is not a bare name segment.
fn parse_names_deal_direct(body: &str, ability_name: &str) -> Option<String> {
    static RE: Lazy<regex::Regex> =
        Lazy::new(|| regex::Regex::new(r"^(.{0,200}?) deals? direct (\w+) damage").unwrap());
    let caps = RE.captures(body)?;
    let to = caps[2].to_string();
    fn norm(s: &str) -> String {
        s.chars()
            .filter(|c| c.is_ascii_alphanumeric())
            .collect::<String>()
            .to_lowercase()
    }
    let want = norm(
        ability_name
            .trim_end_matches(|c: char| c.is_ascii_digit())
            .trim_end(),
    );
    if want.is_empty() {
        return None;
    }
    caps.get(1)?
        .as_str()
        .split(',')
        .flat_map(|seg| seg.split(" and "))
        .any(|seg| norm(seg) == want)
        .then_some(to)
}

/// Parse a type- or skill-scoped DoT rider ("All slashing abilities apply Poison Slice damage
/// for 10 seconds", "Sword abilities apply Psychic scream damage for 10 seconds"). Unanchored —
/// these often follow a conversion sentence in the same desc. Returns
/// `(subject_word, dot_name, seconds)`; the subject is matched against the ability's (possibly
/// converted) damage type or its skill.
fn parse_dot_apply(body: &str) -> Option<(String, String, f64)> {
    static RE: Lazy<regex::Regex> = Lazy::new(|| {
        regex::Regex::new(r"(?:All )?([A-Za-z]+) abilities apply (.+?) damage for (\d+) seconds")
            .unwrap()
    });
    let caps = RE.captures(body)?;
    Some((
        caps[1].to_string(),
        caps[2].to_string(),
        caps[3].parse().ok()?,
    ))
}

/// Best-effort damage type of a prose-named DoT ("Poison Slice", "Psychic scream", "Flaming
/// Sword") from its name and granting attribute's label, using the engine's damage-type table
/// plus common flavor aliases. `None` → the DoT gets universal indirect modifiers only.
fn sniff_damage_type(text: &str) -> Option<String> {
    let lower = text.to_lowercase();
    for t in crate::game_data::ability_stats::DAMAGE_TYPES {
        if lower.contains(&t.to_lowercase()) {
            return Some((*t).to_string());
        }
    }
    if lower.contains("flaming") || lower.contains("burning") {
        return Some("Fire".to_string());
    }
    if lower.contains("freezing") || lower.contains("frost") {
        return Some("Cold".to_string());
    }
    None
}

/// One "apply <DoT> damage for N seconds" rider, paired with the `BOOST_DOT_*` token(s) its
/// granting source also carries (which state the per-second damage).
struct DotApplyRule {
    subject: String,
    dot_name: String,
    seconds: f64,
    tokens: Vec<String>,
}

/// Harvest damage-type conversions and DoT riders from one effect desc. Token-form descs only
/// contribute their `BOOST_DOT_*` token (stashed per source so it can be paired with a rider
/// from the same mod/item); prose descs contribute conversion rules (evaluated against this
/// ability immediately) and pending DoT riders.
fn harvest_type_rules(
    desc: &str,
    ability: &crate::game_data::AbilityInfo,
    target_family: Option<&str>,
    data: &GameData,
    conversions: &mut Vec<String>,
    src_rules: &mut Vec<(String, String, f64)>,
    src_tokens: &mut Vec<String>,
) {
    static ICON: Lazy<regex::Regex> = Lazy::new(|| regex::Regex::new(r"^(?:<icon=\d+>)+").unwrap());
    if let Some((token, _, _)) = parse_token_value(desc) {
        if token.starts_with("BOOST_DOT_") {
            src_tokens.push(token);
        }
        return;
    }
    let body = ICON.replace(desc.trim(), "").to_string();

    if let Some((_skill, from, to)) = parse_skill_type_conversion(&body) {
        // Despite the prose naming the weapon's skill ("Staff abilities that normally deal
        // Crushing damage…"), in-game these equip conversions apply to EVERY ability of the
        // from-type regardless of skill (verified against live tooltips with a Viper Halberd:
        // the second combat skill's Crushing attacks convert too). Gate on damage type only.
        let from_match = ability
            .damage_type
            .as_deref()
            .is_some_and(|d| d.eq_ignore_ascii_case(&from));
        if from_match {
            conversions.push(to);
        }
    } else if let Some(to) = parse_names_deal_direct(&body, &ability.name) {
        conversions.push(to);
    } else if let Some(tf) = target_family {
        let (ids, rest) = extract_leading_abilities_rest(desc, data);
        if !ids.is_empty()
            && ids
                .iter()
                .any(|id| data.ability_to_family.get(id).map(|s| s.as_str()) == Some(tf))
        {
            if let Some(to) = parse_named_type_conversion(&rest) {
                conversions.push(to);
            }
        }
    }

    if let Some((subject, dot_name, seconds)) = parse_dot_apply(&body) {
        src_rules.push((subject, dot_name, seconds));
    }
}

/// Look up an attribute's display label + display type for the breakdown UI, falling back to the
/// raw token when the attribute isn't in the table.
fn attr_label(data: &GameData, token: &str) -> (String, String) {
    data.attributes
        .get(token)
        .map(|attr| {
            (
                attr.raw
                    .get("Label")
                    .and_then(|v| v.as_str())
                    .unwrap_or(token)
                    .to_string(),
                attr.raw
                    .get("DisplayType")
                    .and_then(|v| v.as_str())
                    .unwrap_or("AsInt")
                    .to_string(),
            )
        })
        .unwrap_or_else(|| (token.to_string(), String::new()))
}

/// Resolve one effect desc (token or conditional-indirect prose) into gated [`ModEffect`]s, pushed
/// into `out`. Effects conditional on a skill not present in `equipped` (lowercased skill names) are
/// dropped; surviving effects are labeled from the attribute table for the breakdown UI. DoT-add
/// prose is handled separately by [`resolve_dot_add`] (it needs the target ability).
fn resolve_mod_effects(
    desc: &str,
    source: &str,
    data: &GameData,
    equipped: &std::collections::HashSet<String>,
    out: &mut Vec<crate::game_data::ability_stats::ModEffect>,
) {
    let candidates: Vec<(String, f64, Option<String>)> = match parse_token_value(desc) {
        Some(t) => vec![t],
        None => {
            let mut prose = parse_conditional_indirect_text(desc);
            if prose.is_empty() {
                prose = parse_conditional_direct_text(desc);
            }
            prose
                .into_iter()
                .map(|(token, value, skill)| (token, value, Some(skill)))
                .collect()
        }
    };

    for (token, value, condition) in candidates {
        if let Some(skill) = &condition {
            if !equipped.contains(&skill.to_lowercase()) {
                continue;
            }
        }
        let (label, display_type) = attr_label(data, &token);
        out.push(crate::game_data::ability_stats::ModEffect {
            token,
            value,
            source: source.to_string(),
            label,
            display_type,
        });
    }
}

/// Target-aware handling of DoT-adding prose mods ("X deals N Type damage over M seconds"). When the
/// mod names an ability in the *target's* family it either feeds the target's matching dormant DoT
/// (via that DoT's own `BOOST_ABILITYDOT_*` token, scaling the stated total to per-tick) or — when
/// the target has no such base DoT — synthesizes a new DoT for it. A no-op for token-form descs and
/// for prose that doesn't name the target's family.
#[allow(clippy::too_many_arguments)]
fn resolve_dot_add(
    desc: &str,
    source: &str,
    target_family: Option<&str>,
    stats: &crate::game_data::CombatStats,
    data: &GameData,
    effects: &mut Vec<crate::game_data::ability_stats::ModEffect>,
    synthetic: &mut Vec<crate::game_data::DotEffect>,
) {
    if desc.contains('{') {
        return; // token form — handled elsewhere
    }
    let Some(target_family) = target_family else {
        return;
    };
    let Some((total, dtype, seconds)) = parse_dot_clause(desc) else {
        return;
    };
    let ids = extract_leading_abilities(desc, data);
    if ids.is_empty() {
        return;
    }
    // Only applies when the ability being computed shares a family with one of the named abilities.
    let affected = ids
        .iter()
        .any(|id| data.ability_to_family.get(id).map(|s| s.as_str()) == Some(target_family));
    if !affected {
        return;
    }

    let dtype_l = dtype.as_deref().map(str::to_lowercase);
    let type_matches = |d: &&crate::game_data::DotEffect| {
        dtype_l
            .as_deref()
            .map(|t| d.damage_type.as_deref().map(str::to_lowercase).as_deref() == Some(t))
            .unwrap_or(false)
    };
    // Prefer a DoT matching both type and duration, then type, then duration, then a lone DoT.
    let dot = stats
        .dots
        .iter()
        .find(|d| type_matches(d) && d.duration == Some(seconds as f32))
        .or_else(|| stats.dots.iter().find(type_matches))
        .or_else(|| {
            stats
                .dots
                .iter()
                .find(|d| d.duration == Some(seconds as f32))
        })
        .or_else(|| {
            if stats.dots.len() == 1 {
                stats.dots.first()
            } else {
                None
            }
        });

    match dot {
        // Feed an existing dormant DoT through its own per-tick token.
        Some(d) if !d.attributes_that_delta.is_empty() && d.num_ticks > 0.0 => {
            let token = d.attributes_that_delta[0].clone();
            let (label, display_type) = attr_label(data, &token);
            effects.push(crate::game_data::ability_stats::ModEffect {
                token,
                value: total / d.num_ticks as f64,
                source: source.to_string(),
                label,
                display_type,
            });
        }
        // No base DoT to feed — synthesize one. PG's "over N seconds" DoTs tick every 2 seconds.
        _ => {
            let ticks = (seconds / 2.0).round().max(1.0);
            synthetic.push(crate::game_data::DotEffect {
                damage_per_tick: (total / ticks) as f32,
                num_ticks: ticks as f32,
                duration: Some(seconds as f32),
                damage_type: dtype.clone(),
                preface: None,
                attributes_that_delta: Vec::new(),
                attributes_that_mod: Vec::new(),
            });
        }
    }
}

/// Target-aware handling of direct-damage prose mods, the counterpart of [`resolve_dot_add`] for
/// up-front hits. Covers the two big prose families the tsys data stores without tokens:
///
/// 1. **Named-ability**: "Many Cuts and Debilitating Blow Damage +39", "Aimed Shot deals +25%
///    damage and boosts …" — applies when a named ability shares the target's family. The bonus is
///    fed through one of the target's own direct-damage bucket tokens (flat → delta bucket,
///    percent → all-damage mod bucket). A trailing "and Power Cost -N" clause is folded too.
/// 2. **Skill/keyword-wide**: "Hammer attacks deal +33% damage but generate …", "Bite attacks deal
///    +11% damage and hasten …" — applies when the subject word matches the target's skill or one
///    of its keywords.
///
/// Conditional/temporal bonuses are rejected by the parsers. A no-op for token-form descs and for
/// DoT-add prose (owned by [`resolve_dot_add`]).
fn resolve_direct_add(
    desc: &str,
    source: &str,
    target_family: Option<&str>,
    ability: &crate::game_data::AbilityInfo,
    stats: &crate::game_data::CombatStats,
    data: &GameData,
    effects: &mut Vec<crate::game_data::ability_stats::ModEffect>,
) {
    static ICON: Lazy<regex::Regex> = Lazy::new(|| regex::Regex::new(r"^(?:<icon=\d+>)+").unwrap());
    static RE_POWER_COST: Lazy<regex::Regex> =
        Lazy::new(|| regex::Regex::new(r"(?:[Aa]nd |, )Power Cost -(\d+)").unwrap());

    if desc.contains('{') {
        return; // token form — handled by resolve_mod_effects
    }
    if parse_dot_clause(desc).is_some() {
        return; // DoT-add prose — owned by resolve_dot_add
    }

    let push = |token: &String,
                value: f64,
                effects: &mut Vec<crate::game_data::ability_stats::ModEffect>| {
        let (label, display_type) = attr_label(data, token);
        effects.push(crate::game_data::ability_stats::ModEffect {
            token: token.clone(),
            value,
            source: source.to_string(),
            label,
            display_type,
        });
    };

    let body = ICON.replace(desc.trim(), "").to_string();

    // Skill/keyword-wide boosts, gated on the target ability's skill or keyword. Two prose shapes:
    //   "<Skill> attacks deal +N[%] damage"  (Kick/Hammer/Unarmed…)
    //   "<Skill> Base Damage +N[%]"          (Necromancy — the per-skill boost some mods grant as
    //                                          prose instead of a {MOD_<SKILL>} token)
    // Checked before the named-ability path so a leading skill word ("Kick", "Necromancy") isn't
    // mis-consumed as an ability name.
    let apply_skill_wide =
        |subject: String,
         value: f64,
         pct: bool,
         effects: &mut Vec<crate::game_data::ability_stats::ModEffect>| {
            let subj = subject.to_lowercase();
            let applies = ability
                .skill
                .as_deref()
                .is_some_and(|s| s.to_lowercase() == subj)
                || ability.keywords.iter().any(|k| k.to_lowercase() == subj);
            if applies {
                let bucket = if pct {
                    &stats.attributes_that_mod_damage
                } else {
                    &stats.attributes_that_delta_damage
                };
                if let Some(token) = pick_bucket_token(bucket) {
                    push(&token.clone(), value, effects);
                }
            }
        };
    if let Some((subject, value, pct)) = parse_attacks_deal(&body) {
        apply_skill_wide(subject, value, pct, effects);
        return;
    }
    if let Some((subject, value, pct)) = parse_base_damage(&body) {
        apply_skill_wide(subject, value, pct, effects);
        return;
    }

    // Named-ability form, gated on the target's family like resolve_dot_add.
    let Some(target_family) = target_family else {
        return;
    };
    let (ids, rest) = extract_leading_abilities_rest(desc, data);
    if ids.is_empty() {
        return;
    }
    let affected = ids
        .iter()
        .any(|id| data.ability_to_family.get(id).map(|s| s.as_str()) == Some(target_family));
    if !affected {
        return;
    }

    if let Some((value, pct)) = parse_named_damage_rest(&rest) {
        let bucket = if pct {
            &stats.attributes_that_mod_damage
        } else {
            &stats.attributes_that_delta_damage
        };
        if let Some(token) = pick_bucket_token(bucket) {
            push(&token.clone(), value, effects);
        }
    }
    if let Some(caps) = RE_POWER_COST.captures(&rest) {
        if let (Ok(v), Some(token)) = (
            caps[1].parse::<f64>(),
            stats.attributes_that_delta_power_cost.first(),
        ) {
            push(&token.clone(), -v, effects);
        }
    }
}

/// One assigned build mod, as the frontend knows it (preset mods + slot label).
#[derive(serde::Deserialize)]
pub struct AbilityModRef {
    pub power_name: String,
    pub tier: i64,
    /// Pre-formatted equipment slot label (e.g. "Hands") for source attribution.
    pub slot_label: String,
}

/// One equipped slot item, for folding its innate `{TOKEN}{VALUE}` bonuses (e.g. a focus's flat
/// Direct/Indirect damage) into the calculation alongside the assigned gear mods.
#[derive(serde::Deserialize)]
pub struct AbilityItemRef {
    pub item_id: u32,
    /// Pre-formatted equipment slot label (e.g. "Head") for source attribution.
    pub slot_label: String,
}

/// Attack-category keywords whose flat/percent damage bonuses ("+N Core Attack Damage", etc.) gear
/// and mods grant but abilities never list in their own modifier arrays. Each maps to the
/// `BOOST_ABILITY_<KW>` (flat) / `MOD_ABILITY_<KW>` (percent) attribute the in-game engine applies to
/// every attack of that category. Type-specific variants always co-occur with their general
/// category keyword in the data, so listing both lets an ability pick up both bonuses.
const ATTACK_CATEGORY_KEYWORDS: &[&str] = &[
    "CoreAttack",
    "NiceAttack",
    "EpicAttack",
    "SignatureDebuff",
    "CorePoisonAttack",
    "NiceAcidAttack",
    "NicePoisonAttack",
    // Attack-shape keyword groups whose BOOST_/MOD_ABILITY_<KW> tokens gear actually grants
    // ("Kick Damage +5", "Bard Blast-Ability Damage +76", "Melee Attack Damage +87",
    // "Animal Bite-Attack Damage +26%"):
    "Kick",
    "BardBlast",
    "Melee",
    "Bite",
];

/// Build the direct-damage delta/mod tokens an ability inherits from its attack-category keywords
/// (Core/Nice/Epic/Signature). Returns `(delta_tokens, mod_tokens)` to extend the direct buckets so
/// that "+N Core Attack Damage" gear/mods land on every attack of that category. No ability lists
/// these in its own arrays, so this is purely additive.
fn category_attack_tokens(keywords: &[String]) -> (Vec<String>, Vec<String>) {
    let mut delta = Vec::new();
    let mut mods = Vec::new();
    for kw in keywords {
        if ATTACK_CATEGORY_KEYWORDS.contains(&kw.as_str()) {
            let up = kw.to_uppercase();
            delta.push(format!("BOOST_ABILITY_{up}"));
            mods.push(format!("MOD_ABILITY_{up}"));
        }
    }
    (delta, mods)
}

/// Compute an ability's effective combat stats under a build's assigned gear mods and equipped
/// items.
///
/// Resolves each mod tier's and item's `{TOKEN}{VALUE}` effects to raw tokens (folding in the
/// equipped items' own innate bonuses), gating skill-conditional mods on `equipped_skills`, then
/// defers all game-formula math to `game_data::ability_stats` (which classifies each token by exact
/// membership in the ability's attribute buckets). Returns `None` only if the ability id is unknown.
#[tauri::command]
pub async fn compute_ability_build_stats(
    ability_id: u32,
    mods: Vec<AbilityModRef>,
    items: Vec<AbilityItemRef>,
    equipped_skills: Vec<String>,
    pvp: bool,
    state: State<'_, GameDataState>,
) -> Result<Option<crate::game_data::ability_stats::AbilityBuildStats>, String> {
    use crate::game_data::ability_stats::{compute, AbilityBuildStats, ModEffect};

    let data = state.read().await;
    let Some(ability) = data.abilities.get(&ability_id) else {
        return Ok(None);
    };
    let stats = if pvp {
        ability.pvp.as_ref()
    } else {
        ability.pve.as_ref()
    };
    let Some(stats) = stats else {
        // No combat stats for this side — nothing to compute, but the ability exists.
        return Ok(Some(AbilityBuildStats::default()));
    };

    // Skill-conditional mods ("while <skill> active") apply only when that skill is one of the
    // build's equipped skills. Match case-insensitively.
    let equipped: std::collections::HashSet<String> =
        equipped_skills.iter().map(|s| s.to_lowercase()).collect();

    let mut effects: Vec<ModEffect> = Vec::new();
    // DoTs that a prose mod creates on an ability with no base DoT (e.g. "Poison Arrow deals +600
    // Poison damage over 12 seconds"). Appended to the ability's DoT list before the formula runs.
    let mut synthetic_dots: Vec<crate::game_data::DotEffect> = Vec::new();
    // Damage-type conversions that apply to this ability ("Staff abilities that normally deal
    // Crushing damage instead deal Slashing damage", "… damage type becomes Fire") and pending
    // "apply <DoT> damage for N seconds" riders, harvested from all sources below.
    let mut conversions: Vec<String> = Vec::new();
    let mut dot_apply_rules: Vec<DotApplyRule> = Vec::new();
    let target_family = data.ability_to_family.get(&ability_id).map(|s| s.as_str());

    // Gear mods assigned to slots (tsys powers).
    for mref in &mods {
        // O(1) internal-name → CDN key → client_info lookup.
        let info = data
            .tsys_internal_name_index
            .get(&mref.power_name)
            .and_then(|cdn_key| data.tsys.client_info.get(cdn_key));
        let Some(info) = info else { continue };

        let display_name = info
            .prefix
            .clone()
            .or_else(|| info.suffix.clone())
            .unwrap_or_else(|| mref.power_name.clone());
        let source = format!("{} ({})", display_name, mref.slot_label);

        let tier_key = format!("id_{}", mref.tier);
        let Some(tier_info) = info.tiers.get(&tier_key) else {
            continue;
        };
        let mut src_rules: Vec<(String, String, f64)> = Vec::new();
        let mut src_tokens: Vec<String> = Vec::new();
        for desc in &tier_info.effect_descs {
            resolve_mod_effects(desc, &source, &data, &equipped, &mut effects);
            resolve_dot_add(
                desc,
                &source,
                target_family,
                stats,
                &data,
                &mut effects,
                &mut synthetic_dots,
            );
            resolve_direct_add(
                desc,
                &source,
                target_family,
                ability,
                stats,
                &data,
                &mut effects,
            );
            harvest_type_rules(
                desc,
                ability,
                target_family,
                &data,
                &mut conversions,
                &mut src_rules,
                &mut src_tokens,
            );
        }
        for (subject, dot_name, seconds) in src_rules {
            dot_apply_rules.push(DotApplyRule {
                subject,
                dot_name,
                seconds,
                tokens: src_tokens.clone(),
            });
        }
    }

    // Equipped items' own innate bonuses (e.g. a focus's flat Direct/Indirect Psychic damage),
    // which the in-game tooltip folds in the same way as gear mods.
    for iref in &items {
        let Some(item) = data.items.get(&iref.item_id) else {
            continue;
        };
        let source = format!("{} ({})", item.name, iref.slot_label);
        let mut src_rules: Vec<(String, String, f64)> = Vec::new();
        let mut src_tokens: Vec<String> = Vec::new();
        for desc in &item.effect_descs {
            resolve_mod_effects(desc, &source, &data, &equipped, &mut effects);
            resolve_dot_add(
                desc,
                &source,
                target_family,
                stats,
                &data,
                &mut effects,
                &mut synthetic_dots,
            );
            resolve_direct_add(
                desc,
                &source,
                target_family,
                ability,
                stats,
                &data,
                &mut effects,
            );
            harvest_type_rules(
                desc,
                ability,
                target_family,
                &data,
                &mut conversions,
                &mut src_rules,
                &mut src_tokens,
            );
        }
        for (subject, dot_name, seconds) in src_rules {
            dot_apply_rules.push(DotApplyRule {
                subject,
                dot_name,
                seconds,
                tokens: src_tokens.clone(),
            });
        }
    }

    // Damage-type conversion: the first applicable rule wins. This changes both the displayed
    // type and which generic typed tokens the engine injects — e.g. with a Viper Halberd
    // (Staff Crushing → Slashing), "Direct Crushing Damage" gear stops applying to Headcracker
    // and Slashing-typed gear starts.
    let damage_type = conversions
        .into_iter()
        .next()
        .or_else(|| ability.damage_type.clone());

    // "Apply <DoT> damage for N seconds" riders: gate on the (converted) damage type or the
    // ability's skill, then synthesize a DoT fed by the granting source's own BOOST_DOT_* token
    // (labeled "(Per Second)" — so one tick per second at the token's value).
    for rule in &dot_apply_rules {
        let applies = damage_type
            .as_deref()
            .is_some_and(|t| t.eq_ignore_ascii_case(&rule.subject))
            || ability
                .skill
                .as_deref()
                .is_some_and(|s| s.eq_ignore_ascii_case(&rule.subject));
        if !applies || rule.tokens.is_empty() {
            continue;
        }
        let label_text = rule
            .tokens
            .iter()
            .map(|t| attr_label(&data, t).0)
            .collect::<Vec<_>>()
            .join(" ");
        synthetic_dots.push(crate::game_data::DotEffect {
            damage_per_tick: 0.0,
            num_ticks: rule.seconds as f32,
            duration: Some(rule.seconds as f32),
            damage_type: sniff_damage_type(&format!("{} {label_text}", rule.dot_name)),
            preface: None,
            attributes_that_delta: rule.tokens.clone(),
            attributes_that_mod: Vec::new(),
        });
    }

    // Attack-category damage tokens (Core/Nice/Epic/Signature) the ability inherits from its
    // keywords but never lists itself — so "+N Core Attack Damage" gear/mods can land on it.
    let (cat_delta, cat_mods) = category_attack_tokens(&ability.keywords);

    // Fold the synthesized DoTs and category tokens into the ability's stats so the formula sees
    // them. Cloning is cheap (one ability) and keeps the cached game data immutable.
    if synthetic_dots.is_empty() && cat_delta.is_empty() {
        Ok(Some(compute(stats, damage_type, &effects)))
    } else {
        let mut stats = stats.clone();
        stats.dots.extend(synthetic_dots);
        stats.attributes_that_delta_damage.extend(cat_delta);
        stats.attributes_that_mod_damage.extend(cat_mods);
        Ok(Some(compute(&stats, damage_type, &effects)))
    }
}

// ── Storage Vault query commands ─────────────────────────────────────────────

#[derive(serde::Serialize)]
pub struct StorageVaultZoneInfo {
    pub vault_key: String,
    pub area_key: Option<String>,
    pub area_name: Option<String>,
    pub npc_friendly_name: Option<String>,
    pub num_slots: Option<u32>,
}

/// Get zone/area mappings for all storage vaults.
#[tauri::command]
pub async fn get_storage_vault_zones(
    state: State<'_, GameDataState>,
) -> Result<Vec<StorageVaultZoneInfo>, String> {
    let data = state.read().await;
    let mut results: Vec<StorageVaultZoneInfo> = data
        .storage_vaults
        .iter()
        .map(|(key, vault)| {
            let area_key = vault.area.clone();
            let area_name = area_key
                .as_ref()
                .and_then(|ak| data.areas.get(ak))
                .and_then(|a| a.short_friendly_name.clone().or(a.friendly_name.clone()));

            StorageVaultZoneInfo {
                vault_key: key.clone(),
                area_key,
                area_name,
                npc_friendly_name: vault.npc_friendly_name.clone(),
                num_slots: vault.num_slots,
            }
        })
        .collect();
    results.sort_by(|a, b| a.vault_key.cmp(&b.vault_key));
    Ok(results)
}

/// Vault summary for a specific area (used by AreaBrowser).
#[derive(serde::Serialize)]
pub struct AreaVaultSummary {
    pub key: String,
    pub npc_friendly_name: String,
    pub max_slots: u32,
}

/// Get storage vaults in a specific area.
#[tauri::command]
pub async fn get_storage_vaults_in_area(
    area: String,
    state: State<'_, GameDataState>,
) -> Result<Vec<AreaVaultSummary>, String> {
    let data = state.read().await;
    let mut results: Vec<AreaVaultSummary> = data
        .storage_vaults
        .iter()
        .filter(|(_, v)| v.area.as_ref().map(|a| a == &area).unwrap_or(false))
        .map(|(key, v)| {
            let max_slots = v
                .levels
                .as_ref()
                .and_then(|lvls| lvls.values().copied().max())
                .or(v.num_slots)
                .unwrap_or(0);
            AreaVaultSummary {
                key: key.clone(),
                npc_friendly_name: v.npc_friendly_name.clone().unwrap_or_else(|| key.clone()),
                max_slots,
            }
        })
        .collect();
    results.sort_by(|a, b| a.npc_friendly_name.cmp(&b.npc_friendly_name));
    Ok(results)
}

#[derive(serde::Serialize)]
pub struct StorageVaultDetail {
    pub key: String,
    pub id: u32,
    pub npc_friendly_name: Option<String>,
    pub area: Option<String>,
    pub area_name: Option<String>,
    pub grouping: Option<String>,
    pub grouping_name: Option<String>,
    pub num_slots: Option<u32>,
    pub levels: Option<std::collections::HashMap<String, u32>>,
    pub slot_attribute: Option<String>,
    pub required_item_keywords: Option<Vec<String>>,
    pub requirement_description: Option<String>,
    pub num_slots_script_atomic_max: Option<u32>,
}

/// Get full metadata for all storage vaults (for the Storage Tracker feature).
#[tauri::command]
pub async fn get_storage_vault_metadata(
    state: State<'_, GameDataState>,
) -> Result<Vec<StorageVaultDetail>, String> {
    let data = state.read().await;
    let mut results: Vec<StorageVaultDetail> = data
        .storage_vaults
        .iter()
        .map(|(key, vault)| {
            let area_name = vault
                .area
                .as_ref()
                .and_then(|ak| data.areas.get(ak))
                .and_then(|a| a.short_friendly_name.clone().or(a.friendly_name.clone()));
            let grouping_area = vault.grouping.as_ref().or(vault.area.as_ref());
            let grouping_name = grouping_area
                .and_then(|gk| data.areas.get(gk))
                .and_then(|a| a.short_friendly_name.clone().or(a.friendly_name.clone()));

            StorageVaultDetail {
                key: key.clone(),
                id: vault.id,
                npc_friendly_name: vault.npc_friendly_name.clone(),
                area: vault.area.clone(),
                area_name,
                grouping: vault.grouping.clone(),
                grouping_name,
                num_slots: vault.num_slots,
                levels: vault.levels.clone(),
                slot_attribute: vault.slot_attribute.clone(),
                required_item_keywords: vault.required_item_keywords.clone(),
                requirement_description: vault.requirement_description.clone(),
                num_slots_script_atomic_max: vault.num_slots_script_atomic_max,
            }
        })
        .collect();
    results.sort_by(|a, b| a.key.cmp(&b.key));
    Ok(results)
}

// ── Build Planner CDN queries ───────────────────────────────────────────────

/// Get all combat skills (where combat == true in CDN data).
#[tauri::command]
pub async fn get_combat_skills(state: State<'_, GameDataState>) -> Result<Vec<SkillInfo>, String> {
    let data = state.read().await;
    let mut skills: Vec<SkillInfo> = data
        .skills
        .values()
        .filter(|s| s.combat == Some(true))
        .cloned()
        .collect();
    skills.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(skills)
}

/// Summary of a single tier within a TSys power.
#[derive(serde::Serialize, Clone)]
pub struct TsysTierSummary {
    pub tier_id: String,
    pub min_level: i64,
    pub max_level: i64,
    pub min_rarity: Option<String>,
    pub skill_level_prereq: Option<i64>,
    pub effects: Vec<String>,
    pub icon_id: Option<u32>,
}

/// A single TSys power eligible for a specific equipment slot, with resolved effect text.
#[derive(serde::Serialize)]
pub struct SlotTsysPower {
    /// The CDN key for this power (e.g. "power_12345")
    pub key: String,
    /// Internal name (e.g. "SwordBoost")
    pub internal_name: Option<String>,
    /// Which skill this mod belongs to, or None for generic
    pub skill: Option<String>,
    /// Display prefix (e.g. "Sword Damage")
    pub prefix: Option<String>,
    /// Display suffix
    pub suffix: Option<String>,
    /// The tier index that matched the target level
    pub tier_id: Option<String>,
    /// Resolved effect descriptions at this tier
    pub effects: Vec<String>,
    /// Raw effect descriptions (unresolved tokens)
    pub raw_effects: Vec<String>,
    /// Minimum rarity required for this tier
    pub min_rarity: Option<String>,
    /// Skill level prerequisite for this tier
    pub skill_level_prereq: Option<i64>,
    /// Icon ID from the first effect's attribute definition
    pub icon_id: Option<u32>,
    /// All available tiers for this power (for tier selection UI)
    pub available_tiers: Vec<TsysTierSummary>,
    /// Equipment slots this power can appear on
    pub slots: Vec<String>,
}

/// Choose the default tier for a mod at a given target level.
///
/// Tier level ranges overlap heavily (e.g. 5–20, 10–25, 15–30 …), so several
/// tiers can contain the target level at once. Pick deterministically: the
/// highest tier whose `min_level` is at or below the target — i.e. the strongest
/// tier you qualify for at that level. If the target is below every tier's
/// `min_level`, fall back to the lowest tier (a higher-level mod can still be
/// applied to lower-level gear; it just raises the equip requirement).
///
/// Returns the tier key (e.g. "id_9"), or `None` if `tiers` is empty. Iterating
/// the `tiers` HashMap and taking the first range match instead would be
/// non-deterministic, which made the default tier appear random across launches.
/// The `min_level` tiebreak on the tier key keeps the result stable even if two
/// tiers shared a `min_level`.
fn select_default_tier_key(
    tiers: &HashMap<String, TsysTierInfo>,
    target_level: i64,
) -> Option<String> {
    // Strongest tier at or below the target level.
    let at_or_below = tiers
        .iter()
        .filter(|(_, t)| t.min_level.unwrap_or(0) as i64 <= target_level)
        .max_by(|a, b| {
            a.1.min_level
                .unwrap_or(0)
                .cmp(&b.1.min_level.unwrap_or(0))
                .then_with(|| a.0.cmp(b.0))
        });
    if let Some((key, _)) = at_or_below {
        return Some(key.clone());
    }
    // Target is below every tier: use the lowest tier available.
    tiers
        .iter()
        .min_by(|a, b| {
            a.1.min_level
                .unwrap_or(0)
                .cmp(&b.1.min_level.unwrap_or(0))
                .then_with(|| a.0.cmp(b.0))
        })
        .map(|(key, _)| key.clone())
}

/// Get all eligible TSys powers for a given equipment slot, filtered by skills and target level.
/// Returns powers belonging to skill_primary, skill_secondary, or generic (no skill).
#[tauri::command]
pub async fn get_tsys_powers_for_slot(
    _skill_primary: Option<String>,
    _skill_secondary: Option<String>,
    equip_slot: String,
    target_level: i64,
    state: State<'_, GameDataState>,
) -> Result<Vec<SlotTsysPower>, String> {
    let data = state.read().await;
    let mut results: Vec<SlotTsysPower> = Vec::new();

    for (key, info) in &data.tsys.client_info {
        // Skip unavailable powers
        if info.is_unavailable == Some(true) {
            continue;
        }

        // Must include this equipment slot (map build planner IDs to CDN slot names)
        let slot_matches = info.slots.iter().any(|s| match equip_slot.as_str() {
            "OffHand" => s == "OffHand" || s == "OffHandShield",
            "Belt" => s == "Waist",
            _ => s == &equip_slot,
        });
        if !slot_matches {
            continue;
        }

        // No skill filter — return all mods for this slot.
        // The frontend filters by skill per-column.

        // Find the best tier for the target level
        if info.tiers.is_empty() {
            continue;
        }
        let tiers = &info.tiers;

        // Collect all available tiers for this power
        let mut available_tiers: Vec<TsysTierSummary> = Vec::new();
        for (tier_key, tier) in tiers {
            let min_level = tier.min_level.unwrap_or(0) as i64;
            let max_level = tier.max_level.unwrap_or(999) as i64;
            let min_rarity = tier.min_rarity.clone();
            let skill_level_prereq = tier.skill_level_prereq.map(|v| v as i64);

            let mut tier_icon_id: Option<u32> = None;
            let effects: Vec<String> = tier
                .effect_descs
                .iter()
                .map(|desc| {
                    if let Some(resolved) = resolve_single_effect(desc, &data) {
                        if tier_icon_id.is_none() {
                            tier_icon_id = resolved.icon_id;
                        }
                        resolved.formatted
                    } else {
                        desc.clone()
                    }
                })
                .collect();

            available_tiers.push(TsysTierSummary {
                tier_id: tier_key.clone(),
                min_level,
                max_level,
                min_rarity,
                skill_level_prereq,
                effects,
                icon_id: tier_icon_id,
            });
        }
        // Sort tiers by min_level ascending
        available_tiers.sort_by_key(|t| t.min_level);

        let mut best_effects: Vec<String> = Vec::new();
        let mut best_raw_effects: Vec<String> = Vec::new();
        let mut best_min_rarity: Option<String> = None;
        let mut best_skill_prereq: Option<i64> = None;
        let mut best_icon_id: Option<u32> = None;

        // Helper to resolve effects from a tier
        let resolve_tier =
            |tier: &TsysTierInfo, data: &GameData| -> (Vec<String>, Vec<String>, Option<u32>) {
                let raw = tier.effect_descs.clone();
                let mut icon: Option<u32> = None;
                let resolved: Vec<String> = raw
                    .iter()
                    .map(|desc| {
                        if let Some(r) = resolve_single_effect(desc, data) {
                            if icon.is_none() {
                                icon = r.icon_id;
                            }
                            r.formatted
                        } else {
                            desc.clone()
                        }
                    })
                    .collect();
                (resolved, raw, icon)
            };

        // Default the mod to the tier the target level qualifies for (deterministic
        // — see select_default_tier_key). This is what feeds `tier_id` below, which
        // the frontend uses as the default in the mod/augment tier picker.
        let best_tier_id = select_default_tier_key(tiers, target_level);
        if let Some(tier) = best_tier_id.as_ref().and_then(|k| tiers.get(k)) {
            best_min_rarity = tier.min_rarity.clone();
            best_skill_prereq = tier.skill_level_prereq.map(|v| v as i64);
            let (eff, raw, icon) = resolve_tier(tier, &data);
            best_effects = eff;
            best_raw_effects = raw;
            best_icon_id = icon;
        }

        if best_tier_id.is_some() {
            // Resolve skill internal name to display name (e.g. "FireMagic" -> "Fire Magic")
            let resolved_skill = info
                .skill
                .as_deref()
                .and_then(|s| data.resolve_skill(s).map(|si| si.name.clone()))
                .or_else(|| info.skill.clone());

            results.push(SlotTsysPower {
                key: key.clone(),
                internal_name: info.internal_name.clone(),
                skill: resolved_skill,
                prefix: info.prefix.clone(),
                suffix: info.suffix.clone(),
                tier_id: best_tier_id,
                effects: best_effects,
                raw_effects: best_raw_effects,
                min_rarity: best_min_rarity,
                skill_level_prereq: best_skill_prereq,
                icon_id: best_icon_id,
                available_tiers: if available_tiers.len() > 1 {
                    available_tiers
                } else {
                    vec![]
                },
                slots: info
                    .slots
                    .iter()
                    .map(|s| {
                        // Map CDN slot names back to display names
                        match s.as_str() {
                            "Waist" => "Belt".to_string(),
                            "OffHandShield" => "Off Hand".to_string(),
                            "OffHand" => "Off Hand".to_string(),
                            "MainHand" => "Main Hand".to_string(),
                            _ => s.clone(),
                        }
                    })
                    .collect(),
            });
        }
    }

    // Sort: skill-specific first (grouped by skill), then generic, then by name
    results.sort_by(|a, b| {
        let a_is_generic = a.skill.is_none();
        let b_is_generic = b.skill.is_none();
        if a_is_generic != b_is_generic {
            return a_is_generic.cmp(&b_is_generic);
        }
        let skill_cmp = a.skill.cmp(&b.skill);
        if skill_cmp != std::cmp::Ordering::Equal {
            return skill_cmp;
        }
        let a_name = a
            .prefix
            .as_deref()
            .or(a.suffix.as_deref())
            .or(a.internal_name.as_deref())
            .unwrap_or("");
        let b_name = b
            .prefix
            .as_deref()
            .or(b.suffix.as_deref())
            .or(b.internal_name.as_deref())
            .unwrap_or("");
        a_name.cmp(b_name)
    });

    Ok(results)
}

// ── TSys browser commands ────────────────────────────────────────────────────

/// Flattened TSys entry for the frontend browser (includes the CDN key).
#[derive(Debug, serde::Serialize, Clone)]
pub struct TsysBrowserEntry {
    pub key: String,
    pub internal_name: Option<String>,
    pub skill: Option<String>,
    pub slots: Vec<String>,
    pub prefix: Option<String>,
    pub suffix: Option<String>,
    pub tiers: HashMap<String, TsysTierInfo>,
    pub is_unavailable: Option<bool>,
    pub is_hidden_from_transmutation: Option<bool>,
    pub tier_count: usize,
    pub raw_json: Value,
}

impl TsysBrowserEntry {
    fn from_entry(key: &str, info: &TsysClientInfo) -> Self {
        let tier_count = info.tiers.len();
        Self {
            key: key.to_string(),
            internal_name: info.internal_name.clone(),
            skill: info.skill.clone(),
            slots: info.slots.clone(),
            prefix: info.prefix.clone(),
            suffix: info.suffix.clone(),
            tiers: info.tiers.clone(),
            is_unavailable: info.is_unavailable,
            is_hidden_from_transmutation: info.is_hidden_from_transmutation,
            tier_count,
            raw_json: info.raw_json.clone(),
        }
    }
}

/// Get all TSys client info entries for the browser.
#[tauri::command]
pub async fn get_all_tsys(
    state: State<'_, GameDataState>,
) -> Result<Vec<TsysBrowserEntry>, String> {
    let data = state.read().await;
    let mut results: Vec<TsysBrowserEntry> = data
        .tsys
        .client_info
        .iter()
        .map(|(key, info)| TsysBrowserEntry::from_entry(key, info))
        .collect();
    results.sort_by(|a, b| {
        let a_name = a.internal_name.as_deref().unwrap_or(&a.key);
        let b_name = b.internal_name.as_deref().unwrap_or(&b.key);
        a_name.cmp(b_name)
    });
    Ok(results)
}

/// Search TSys entries by name, skill, prefix, suffix, or key.
#[tauri::command]
pub async fn search_tsys(
    query: String,
    limit: Option<usize>,
    state: State<'_, GameDataState>,
) -> Result<Vec<TsysBrowserEntry>, String> {
    let data = state.read().await;
    let q = query.to_lowercase();
    let limit = limit.unwrap_or(100);

    let mut results: Vec<TsysBrowserEntry> = data
        .tsys
        .client_info
        .iter()
        .filter(|(key, info)| {
            let key_match = key.to_lowercase().contains(&q);
            let name_match = info
                .internal_name
                .as_ref()
                .map(|n| n.to_lowercase().contains(&q))
                .unwrap_or(false);
            let skill_match = info
                .skill
                .as_ref()
                .map(|s| s.to_lowercase().contains(&q))
                .unwrap_or(false);
            let prefix_match = info
                .prefix
                .as_ref()
                .map(|p| p.to_lowercase().contains(&q))
                .unwrap_or(false);
            let suffix_match = info
                .suffix
                .as_ref()
                .map(|s| s.to_lowercase().contains(&q))
                .unwrap_or(false);
            let slot_match = info.slots.iter().any(|s| s.to_lowercase().contains(&q));
            key_match || name_match || skill_match || prefix_match || suffix_match || slot_match
        })
        .map(|(key, info)| TsysBrowserEntry::from_entry(key, info))
        .collect();

    results.sort_by(|a, b| {
        let a_name = a.internal_name.as_deref().unwrap_or(&a.key);
        let b_name = b.internal_name.as_deref().unwrap_or(&b.key);
        a_name.cmp(b_name)
    });
    results.truncate(limit);
    Ok(results)
}

/// Get TSys profiles (raw JSON).
#[tauri::command]
pub async fn get_tsys_profiles(state: State<'_, GameDataState>) -> Result<Value, String> {
    let data = state.read().await;
    Ok(data.tsys.profiles.clone())
}

// ── Cross-reference commands for data browser linking ──────────────────────

/// NPC favor entry returned to the frontend.
#[derive(serde::Serialize, Clone)]
pub struct NpcFavorEntry {
    pub npc_key: String,
    pub npc_name: String,
    pub desire: String,
    pub pref: f32,
    pub match_type: String,
}

/// Get all NPCs that want a given item (by name match or keyword match).
#[tauri::command]
pub async fn get_npcs_wanting_item(
    item_id: u32,
    state: State<'_, GameDataState>,
) -> Result<Vec<NpcFavorEntry>, String> {
    let data = state.read().await;
    let item = data.items.get(&item_id).ok_or("Item not found")?;

    let mut results: Vec<NpcFavorEntry> = Vec::new();
    let mut seen_npc_keys = std::collections::HashSet::new();

    // Check name-based matches
    let name_lower = item.name.to_lowercase();
    if let Some(entries) = data.npc_favor_by_item_name.get(&name_lower) {
        for (npc_key, desire, pref) in entries {
            if seen_npc_keys.insert(npc_key.clone()) {
                let npc_name = data
                    .npcs
                    .get(npc_key)
                    .map(|n| n.name.clone())
                    .unwrap_or_default();
                results.push(NpcFavorEntry {
                    npc_key: npc_key.clone(),
                    npc_name,
                    desire: desire.clone(),
                    pref: *pref,
                    match_type: "name".to_string(),
                });
            }
        }
    }

    // Check keyword-based matches
    for keyword in &item.keywords {
        if let Some(entries) = data.npc_favor_by_keyword.get(keyword) {
            for (npc_key, desire, pref) in entries {
                if seen_npc_keys.insert(npc_key.clone()) {
                    let npc_name = data
                        .npcs
                        .get(npc_key)
                        .map(|n| n.name.clone())
                        .unwrap_or_default();
                    results.push(NpcFavorEntry {
                        npc_key: npc_key.clone(),
                        npc_name,
                        desire: desire.clone(),
                        pref: *pref,
                        match_type: format!("keyword:{}", keyword),
                    });
                }
            }
        }
    }

    // Sort by pref value descending
    results.sort_by(|a, b| {
        b.pref
            .partial_cmp(&a.pref)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    Ok(results)
}

/// Get all NPCs that train a given skill.
#[tauri::command]
pub async fn get_npcs_training_skill(
    skill: String,
    state: State<'_, GameDataState>,
) -> Result<Vec<NpcInfo>, String> {
    let data = state.read().await;

    // npcs_by_skill is keyed by skill internal name from training data
    let npc_keys = data.npcs_by_skill.get(&skill).cloned().unwrap_or_default();
    let results: Vec<NpcInfo> = npc_keys
        .iter()
        .filter_map(|key| data.npcs.get(key).cloned())
        .collect();
    Ok(results)
}

/// Get all quests associated with a given NPC (via FavorNpc field).
#[tauri::command]
pub async fn get_quests_for_npc(
    npc_key: String,
    state: State<'_, GameDataState>,
) -> Result<Vec<QuestInfo>, String> {
    let data = state.read().await;
    let quest_keys = data
        .quests_by_npc
        .get(&npc_key)
        .cloned()
        .unwrap_or_default();
    let results: Vec<QuestInfo> = quest_keys
        .iter()
        .filter_map(|key| data.quests.get(key).cloned())
        .collect();
    Ok(results)
}

/// Get all work order quests for a given skill.
#[tauri::command]
pub async fn get_quests_for_skill(
    skill: String,
    state: State<'_, GameDataState>,
) -> Result<Vec<QuestInfo>, String> {
    let data = state.read().await;
    let quest_keys = data
        .quests_by_work_order_skill
        .get(&skill)
        .cloned()
        .unwrap_or_default();
    let results: Vec<QuestInfo> = quest_keys
        .iter()
        .filter_map(|key| data.quests.get(key).cloned())
        .collect();
    Ok(results)
}

/// Get all quests that require a specific moon phase (via RequirementsToSustain).
#[tauri::command]
pub async fn get_quests_by_moon_phase(
    moon_phase: String,
    state: State<'_, GameDataState>,
) -> Result<Vec<QuestInfo>, String> {
    let data = state.read().await;

    fn has_moon_phase(req: &serde_json::Value, phase: &str) -> bool {
        if let Some(t) = req.get("T").and_then(|v| v.as_str()) {
            if t == "MoonPhase" {
                if let Some(mp) = req.get("MoonPhase").and_then(|v| v.as_str()) {
                    return mp == phase;
                }
            }
        }
        false
    }

    let mut results: Vec<QuestInfo> = data
        .quests
        .values()
        .filter(|quest| {
            if let Some(reqs) = quest.raw.get("RequirementsToSustain") {
                // Can be a single object or an array
                if let Some(arr) = reqs.as_array() {
                    arr.iter().any(|r| has_moon_phase(r, &moon_phase))
                } else {
                    has_moon_phase(reqs, &moon_phase)
                }
            } else {
                false
            }
        })
        .cloned()
        .collect();

    results.sort_by(|a, b| {
        let a_name = a
            .raw
            .get("DisplayName")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let b_name = b
            .raw
            .get("DisplayName")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        a_name.cmp(b_name)
    });

    Ok(results)
}

/// Get all recipes that accept a given keyword as an ingredient.
#[tauri::command]
pub async fn get_recipes_for_keyword(
    keyword: String,
    state: State<'_, GameDataState>,
) -> Result<Vec<RecipeInfo>, String> {
    let data = state.read().await;
    let recipe_ids = data
        .recipes_by_ingredient_keyword
        .get(&keyword)
        .cloned()
        .unwrap_or_default();
    let results: Vec<RecipeInfo> = recipe_ids
        .iter()
        .filter_map(|id| data.recipes.get(id).cloned())
        .collect();
    Ok(results)
}

// ── CP-consuming recipe queries ───────────────────────────────────────────

#[derive(serde::Serialize)]
pub struct CpRecipeOption {
    pub recipe_id: u32,
    pub recipe_name: String,
    pub icon_id: Option<u32>,
    pub skill: Option<String>,
    pub skill_level_req: Option<f32>,
    pub cp_cost: u32,
    /// "shamanic_infusion" or "crafting_enhancement"
    pub effect_type: String,
    /// For shamanic: TSys power name; for enhancements: the enhancement function name
    pub effect_key: String,
    /// Human-readable description of the effect
    pub effect_description: String,
}

/// Get all CP-consuming recipes (shamanic infusion + crafting enhancements) applicable to a slot.
#[tauri::command]
pub async fn get_cp_recipes_for_slot(
    equip_slot: String,
    state: State<'_, GameDataState>,
) -> Result<Vec<CpRecipeOption>, String> {
    let data = state.read().await;
    let mut results = Vec::new();

    // Map frontend slot name to CDN slot name
    let cdn_slot = match equip_slot.as_str() {
        "Belt" => "Waist",
        "OffHand" => "OffHand",
        _ => &equip_slot,
    };

    for recipe in data.recipes.values() {
        for effect in &recipe.result_effects {
            // Shamanic Infusion: "AddItemTSysPower(PowerName,Tier)"
            if let Some(inner) = effect
                .strip_prefix("AddItemTSysPower(")
                .and_then(|s| s.strip_suffix(')'))
            {
                let parts: Vec<&str> = inner.split(',').collect();
                if parts.len() != 2 {
                    continue;
                }
                let power_name = parts[0].trim();

                // Look up the TSys power to check skill and slot compatibility
                let tsys_entry = data
                    .tsys
                    .client_info
                    .values()
                    .find(|info| info.internal_name.as_deref() == Some(power_name));

                let Some(tsys_info) = tsys_entry else {
                    continue;
                };

                // Must be a ShamanicInfusion power
                if tsys_info.skill.as_deref() != Some("ShamanicInfusion") {
                    continue;
                }

                // Must be compatible with this equipment slot
                let slot_matches = tsys_info.slots.iter().any(|s| match cdn_slot {
                    "OffHand" => s == "OffHand" || s == "OffHandShield",
                    _ => s == cdn_slot,
                });
                if !slot_matches {
                    continue;
                }

                // Resolve effect description from the power's tier data
                let tier_str = parts[1].trim();
                let tier_key = format!("id_{}", tier_str);
                let description = tsys_info
                    .tiers
                    .get(&tier_key)
                    .map(|tier| {
                        tier.effect_descs
                            .iter()
                            .filter_map(|desc| resolve_single_effect(desc, &data))
                            .map(|r| r.formatted)
                            .collect::<Vec<_>>()
                            .join(", ")
                    })
                    .unwrap_or_else(|| recipe.description.clone().unwrap_or_default());

                results.push(CpRecipeOption {
                    recipe_id: recipe.id,
                    recipe_name: recipe.name.clone(),
                    icon_id: recipe.icon_id,
                    skill: recipe.skill.clone(),
                    skill_level_req: recipe.skill_level_req,
                    cp_cost: 100, // All shamanic infusions cost 100 CP
                    effect_type: "shamanic_infusion".to_string(),
                    effect_key: power_name.to_string(),
                    effect_description: description,
                });
            }

            // Crafting Enhancement: "CraftingEnhanceItemXxx(value,cpCost)"
            if effect.starts_with("CraftingEnhanceItem") {
                if let Some(inner) = effect
                    .find('(')
                    .and_then(|start| effect.strip_suffix(')').map(|s| &s[start + 1..]))
                {
                    let parts: Vec<&str> = inner.split(',').collect();
                    if parts.len() != 2 {
                        continue;
                    }
                    let cp_cost: u32 = match parts[1].trim().parse() {
                        Ok(v) => v,
                        Err(_) => continue,
                    };

                    // Crafting enhancements apply to armor slots only
                    let armor_slots = ["Head", "Chest", "Legs", "Hands", "Feet"];
                    if !armor_slots.contains(&cdn_slot) {
                        continue;
                    }

                    // Filter by item type in recipe name — recipes target specific
                    // body parts via name patterns like "...Cloth Pants", "...Leather Shirt"
                    let name_lower = recipe.name.to_lowercase();
                    let slot_matches = match cdn_slot {
                        "Head" => name_lower.contains("helmet") || name_lower.contains("head"),
                        "Chest" => name_lower.contains("shirt") || name_lower.contains("chest"),
                        "Legs" => name_lower.contains("pants") || name_lower.contains("legs"),
                        "Hands" => {
                            name_lower.contains("gloves")
                                || name_lower.contains("hands")
                                || name_lower.contains("gauntlets")
                        }
                        "Feet" => {
                            name_lower.contains("boots")
                                || name_lower.contains("feet")
                                || name_lower.contains("shoes")
                        }
                        _ => false,
                    };
                    if !slot_matches {
                        continue;
                    }

                    // Use recipe description as the effect description
                    let description = recipe
                        .description
                        .clone()
                        .unwrap_or_else(|| recipe.name.clone());

                    results.push(CpRecipeOption {
                        recipe_id: recipe.id,
                        recipe_name: recipe.name.clone(),
                        icon_id: recipe.icon_id,
                        skill: recipe.skill.clone(),
                        skill_level_req: recipe.skill_level_req,
                        cp_cost,
                        effect_type: "crafting_enhancement".to_string(),
                        effect_key: effect.clone(),
                        effect_description: description,
                    });
                }
            }
        }
    }

    // Sort by effect_type (shamanic first), then by name
    results.sort_by(|a, b| {
        a.effect_type
            .cmp(&b.effect_type)
            .then(a.recipe_name.cmp(&b.recipe_name))
    });

    Ok(results)
}

// ── Precomputed TSys ↔ Ability lookups ─────────────────────────────────────

/// Batch lookup: given a list of TSys keys or internal names, return the ability IDs each one affects.
/// Accepts either CDN keys (e.g., "power_12345") or internal names (e.g., "SwordDamageBoost").
/// Uses the precomputed index built at CDN load time — O(1) per key.
#[tauri::command]
pub async fn get_tsys_ability_map(
    tsys_keys: Vec<String>,
    state: State<'_, GameDataState>,
) -> Result<HashMap<String, Vec<u32>>, String> {
    let data = state.read().await;
    let mut result = HashMap::new();
    for key in &tsys_keys {
        // Try direct CDN key lookup first
        if let Some(ids) = data.tsys_to_abilities.get(key) {
            result.insert(key.clone(), ids.clone());
            continue;
        }
        // Try as internal_name via precomputed index — O(1)
        if let Some(cdn_key) = data.tsys_internal_name_index.get(key) {
            if let Some(ids) = data.tsys_to_abilities.get(cdn_key) {
                result.insert(key.clone(), ids.clone());
            }
        }
    }
    Ok(result)
}

// ── Ability ↔ TSys cross-reference commands (legacy, still used by data browser) ──

/// Extract all `{TOKEN}` keys from a list of effect_desc strings.
fn extract_effect_tokens(effect_descs: &[String]) -> Vec<String> {
    let mut tokens = Vec::new();
    for desc in effect_descs {
        if !desc.starts_with('{') {
            continue;
        }
        // Format: "{TOKEN}{VALUE}" — extract the TOKEN part
        let parts: Vec<&str> = desc.split('{').filter(|s| !s.is_empty()).collect();
        if let Some(first) = parts.first() {
            let token = first.trim_end_matches('}');
            if !token.is_empty() {
                tokens.push(token.to_string());
            }
        }
    }
    tokens
}

/// Extract `<icon=NNNN>` IDs from text-format effect descriptions.
fn extract_icon_ids(effect_descs: &[String]) -> std::collections::HashSet<u32> {
    let mut icons = std::collections::HashSet::new();
    for desc in effect_descs {
        // Only process text-format (non-token) descriptions
        if desc.starts_with('{') {
            continue;
        }
        let mut remaining = desc.as_str();
        while let Some(start) = remaining.find("<icon=") {
            let after = &remaining[start + 6..];
            if let Some(end) = after.find('>') {
                if let Ok(id) = after[..end].parse::<u32>() {
                    icons.insert(id);
                }
            }
            remaining = &remaining[start + 6..];
        }
    }
    icons
}

/// Collect all attribute tokens referenced by an ability's PvE/PvP combat stats,
/// including DoT attribute arrays from the raw JSON.
fn collect_ability_attribute_tokens(ability: &AbilityInfo) -> std::collections::HashSet<String> {
    let mut tokens = std::collections::HashSet::new();

    for stats in [&ability.pve, &ability.pvp].into_iter().flatten() {
        for t in &stats.attributes_that_delta_damage {
            tokens.insert(t.clone());
        }
        for t in &stats.attributes_that_mod_base_damage {
            tokens.insert(t.clone());
        }
        for t in &stats.attributes_that_mod_damage {
            tokens.insert(t.clone());
        }
        for t in &stats.attributes_that_mod_crit_damage {
            tokens.insert(t.clone());
        }
        for t in &stats.attributes_that_delta_power_cost {
            tokens.insert(t.clone());
        }
        for t in &stats.attributes_that_mod_power_cost {
            tokens.insert(t.clone());
        }
        for t in &stats.attributes_that_delta_rage {
            tokens.insert(t.clone());
        }
        for t in &stats.attributes_that_mod_rage {
            tokens.insert(t.clone());
        }
        for t in &stats.attributes_that_delta_taunt {
            tokens.insert(t.clone());
        }
        for t in &stats.attributes_that_mod_taunt {
            tokens.insert(t.clone());
        }

        // Also check DoTs in the extra field (e.g., DoTs[].AttributesThatDelta)
        if let Some(dots) = stats.extra.get("DoTs").and_then(|v| v.as_array()) {
            for dot in dots {
                for key in [
                    "AttributesThatDelta",
                    "AttributesThatMod",
                    "AttributesThatDeltaDamage",
                    "AttributesThatModDamage",
                    "AttributesThatModBaseDamage",
                ] {
                    if let Some(arr) = dot.get(key).and_then(|v| v.as_array()) {
                        for val in arr {
                            if let Some(s) = val.as_str() {
                                tokens.insert(s.to_string());
                            }
                        }
                    }
                }
            }
        }
    }

    tokens
}

/// Check if `text` contains `name` as an exact ability reference, not as a prefix
/// of a longer ability name. `longer_names` contains all known ability base names
/// that start with `name + " "`.
fn text_contains_ability_name(text: &str, name: &str, longer_names: &[&str]) -> bool {
    let mut search_from = 0;
    while let Some(idx) = text[search_from..].find(name) {
        let abs_idx = search_from + idx;
        // Check: does a longer ability name also match at this position?
        let is_prefix_of_longer = longer_names
            .iter()
            .any(|longer| text[abs_idx..].starts_with(longer));
        if !is_prefix_of_longer {
            return true;
        }
        // Move past this match and keep looking — the short name might appear
        // elsewhere in the text on its own
        search_from = abs_idx + name.len();
    }
    false
}

/// Check if a TSys mod matches an ability via any of:
/// 1. Attribute token overlap (PvE/PvP stats ↔ {TOKEN}{VALUE} effect descs)
/// 2. Icon ID match (ability icon_id in <icon=NNN> tags in text effect descs)
/// 3. Ability display name appears in text effect descs (with prefix disambiguation)
fn tsys_matches_ability(
    info: &TsysClientInfo,
    ability_tokens: &std::collections::HashSet<String>,
    ability_name: &str,
    ability_icon_id: Option<u32>,
    longer_ability_names: &[&str],
) -> bool {
    // Collect all effect_descs across tiers (check one tier for text match, all for tokens)
    for tier in info.tiers.values() {
        // 1. Token matching
        let tier_tokens = extract_effect_tokens(&tier.effect_descs);
        if tier_tokens.iter().any(|t| ability_tokens.contains(t)) {
            return true;
        }
    }

    // For text matching, only check one representative tier (they all reference the same abilities)
    if let Some(tier) = info.tiers.values().next() {
        // 2. Icon ID matching
        if let Some(icon_id) = ability_icon_id {
            let tier_icons = extract_icon_ids(&tier.effect_descs);
            if tier_icons.contains(&icon_id) {
                return true;
            }
        }

        // 3. Ability display name matching in text descriptions
        // Only match names that are 4+ chars to avoid false positives on short names
        if ability_name.len() >= 4 {
            for desc in &tier.effect_descs {
                if desc.starts_with('{') {
                    continue;
                }
                let (clean, _) = strip_icon_tags(desc);
                if text_contains_ability_name(&clean, ability_name, longer_ability_names) {
                    return true;
                }
            }
        }
    }

    false
}

/// Lightweight TSys summary returned for ability cross-references.
#[derive(Debug, serde::Serialize, Clone)]
pub struct TsysAbilityXref {
    pub key: String,
    pub internal_name: Option<String>,
    pub skill: Option<String>,
    pub prefix: Option<String>,
    pub suffix: Option<String>,
    pub slots: Vec<String>,
    pub tier_count: usize,
    /// Resolved effect descriptions from the highest tier
    pub top_tier_effects: Vec<String>,
}

fn resolve_top_tier_effects(info: &TsysClientInfo, data: &GameData) -> Vec<String> {
    let top_tier = info
        .tiers
        .iter()
        .max_by_key(|(k, _)| {
            k.strip_prefix("id_")
                .and_then(|n| n.parse::<u32>().ok())
                .unwrap_or(0)
        })
        .map(|(_, tier)| tier);

    top_tier
        .map(|tier| {
            tier.effect_descs
                .iter()
                .map(|desc| {
                    resolve_single_effect(desc, data)
                        .map(|r| r.formatted)
                        .unwrap_or_else(|| strip_icon_tags(desc).0)
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Get all TSys mods that affect a given ability.
/// Matches via attribute tokens, icon IDs, and ability name text matching.
#[tauri::command]
pub async fn get_tsys_for_ability(
    ability_id: u32,
    state: State<'_, GameDataState>,
) -> Result<Vec<TsysAbilityXref>, String> {
    let data = state.read().await;
    let ability = data.abilities.get(&ability_id).ok_or("Ability not found")?;

    // Monster abilities don't have treasure system effects
    if ability.keywords.iter().any(|k| k == "Lint_MonsterAbility") {
        return Ok(Vec::new());
    }

    let ability_tokens = collect_ability_attribute_tokens(ability);

    // Get the base ability name (without trailing number) for text matching
    // e.g., "Reckless Slam 3" → "Reckless Slam"
    let base_display_name = ability
        .name
        .trim_end_matches(|c: char| c.is_ascii_digit())
        .trim()
        .to_string();

    // Build list of longer ability names that start with this one + space
    // e.g., for "Pound" → ["Pound To Slag"]
    // This prevents "Pound" from matching text about "Pound To Slag"
    let all_base_names: Vec<String> = data
        .abilities
        .values()
        .map(|a| {
            a.name
                .trim_end_matches(|c: char| c.is_ascii_digit())
                .trim()
                .to_string()
        })
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect();
    let longer_names: Vec<&str> = all_base_names
        .iter()
        .filter(|n| {
            n.len() > base_display_name.len() && n.starts_with(&format!("{} ", base_display_name))
        })
        .map(|n| n.as_str())
        .collect();

    let mut results: Vec<TsysAbilityXref> = Vec::new();

    for (key, info) in &data.tsys.client_info {
        if tsys_matches_ability(
            info,
            &ability_tokens,
            &base_display_name,
            ability.icon_id,
            &longer_names,
        ) {
            let top_tier_effects = resolve_top_tier_effects(info, &data);

            results.push(TsysAbilityXref {
                key: key.clone(),
                internal_name: info.internal_name.clone(),
                skill: info.skill.clone(),
                prefix: info.prefix.clone(),
                suffix: info.suffix.clone(),
                slots: info.slots.clone(),
                tier_count: info.tiers.len(),
                top_tier_effects,
            });
        }
    }

    results.sort_by(|a, b| {
        let a_name = a.internal_name.as_deref().unwrap_or(&a.key);
        let b_name = b.internal_name.as_deref().unwrap_or(&b.key);
        a_name.cmp(b_name)
    });

    Ok(results)
}

/// Lightweight ability summary returned for TSys cross-references.
#[derive(Debug, serde::Serialize, Clone)]
pub struct AbilityTsysXref {
    pub id: u32,
    pub name: String,
    pub icon_id: Option<u32>,
    pub skill: Option<String>,
    pub level: Option<f32>,
    pub internal_name: Option<String>,
}

/// Get all abilities affected by a given TSys mod.
/// Matches via attribute tokens, icon IDs, and ability name text matching.
#[tauri::command]
pub async fn get_abilities_for_tsys(
    tsys_key: String,
    state: State<'_, GameDataState>,
) -> Result<Vec<AbilityTsysXref>, String> {
    let data = state.read().await;
    let info = data
        .tsys
        .client_info
        .get(&tsys_key)
        .ok_or("TSys entry not found")?;

    // Collect attribute tokens from {TOKEN}{VALUE} format effect descs
    let mut tsys_tokens = std::collections::HashSet::new();
    for tier in info.tiers.values() {
        for token in extract_effect_tokens(&tier.effect_descs) {
            tsys_tokens.insert(token);
        }
    }

    // Collect icon IDs from text-format effect descs
    let mut tsys_icon_ids = std::collections::HashSet::new();
    for tier in info.tiers.values() {
        tsys_icon_ids.extend(extract_icon_ids(&tier.effect_descs));
    }

    // Collect ability display names from text-format effect descs
    // Build a set of all known ability base names for matching
    let mut ability_base_names: std::collections::HashMap<String, Vec<u32>> =
        std::collections::HashMap::new();
    for ab in data.abilities.values() {
        let base = ab
            .name
            .trim_end_matches(|c: char| c.is_ascii_digit())
            .trim()
            .to_string();
        if base.len() >= 4 {
            ability_base_names.entry(base).or_default().push(ab.id);
        }
    }

    let mut matched_ability_ids = std::collections::HashSet::new();

    // 1. Token matching: find abilities whose attribute tokens overlap
    if !tsys_tokens.is_empty() {
        for ability in data.abilities.values() {
            let ability_tokens = collect_ability_attribute_tokens(ability);
            if ability_tokens.iter().any(|t| tsys_tokens.contains(t)) {
                matched_ability_ids.insert(ability.id);
            }
        }
    }

    // 2. Icon ID matching: find abilities whose icon_id appears in text descs
    if !tsys_icon_ids.is_empty() {
        for ability in data.abilities.values() {
            if let Some(icon_id) = ability.icon_id {
                if tsys_icon_ids.contains(&icon_id) {
                    matched_ability_ids.insert(ability.id);
                }
            }
        }
    }

    // 3. Text name matching: find ability names in text effect descs
    // Sort names longest-first so longer names get priority over shorter prefixes
    let mut sorted_names: Vec<&String> = ability_base_names.keys().collect();
    sorted_names.sort_by(|a, b| b.len().cmp(&a.len()));

    if let Some(tier) = info.tiers.values().next() {
        for desc in &tier.effect_descs {
            if desc.starts_with('{') {
                continue;
            }
            let (clean, _) = strip_icon_tags(desc);
            for base_name in &sorted_names {
                // Build longer-names list for this specific name
                let longer: Vec<&str> = sorted_names
                    .iter()
                    .filter(|n| {
                        n.len() > base_name.len() && n.starts_with(&format!("{} ", base_name))
                    })
                    .map(|n| n.as_str())
                    .collect();
                if text_contains_ability_name(&clean, base_name, &longer) {
                    if let Some(ids) = ability_base_names.get(base_name.as_str()) {
                        for id in ids {
                            matched_ability_ids.insert(*id);
                        }
                    }
                }
            }
        }
    }

    // Deduplicate by base ability name, keep highest-level version
    let mut results: Vec<AbilityTsysXref> = Vec::new();
    let mut seen_base_names = std::collections::HashSet::new();

    for &ability_id in &matched_ability_ids {
        if let Some(ability) = data.abilities.get(&ability_id) {
            let base_name = ability
                .internal_name
                .as_deref()
                .unwrap_or(&ability.name)
                .trim_end_matches(char::is_numeric)
                .to_string();

            if seen_base_names.contains(&base_name) {
                continue;
            }

            // Find the highest-level version of this ability
            let best = data
                .abilities
                .values()
                .filter(|a| {
                    let a_base = a
                        .internal_name
                        .as_deref()
                        .unwrap_or(&a.name)
                        .trim_end_matches(char::is_numeric)
                        .to_string();
                    a_base == base_name
                })
                .max_by(|a, b| {
                    a.level
                        .unwrap_or(0.0)
                        .partial_cmp(&b.level.unwrap_or(0.0))
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
                .unwrap_or(ability);

            seen_base_names.insert(base_name);
            results.push(AbilityTsysXref {
                id: best.id,
                name: best.name.clone(),
                icon_id: best.icon_id,
                skill: best.skill.clone(),
                level: best.level,
                internal_name: best.internal_name.clone(),
            });
        }
    }

    results.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(results)
}

// ── Recipe Item Finder ────────────────────────────────────────────────────────

/// A recipe item found in the player's inventory, with context about whether
/// the recipes it teaches are already known or if the player meets the skill
/// requirements to learn them.
#[derive(Debug, Clone, serde::Serialize)]
pub struct RecipeItemMatch {
    /// The item's type ID from CDN
    pub item_id: u32,
    /// Display name of the item
    pub item_name: String,
    /// Icon ID for display
    pub icon_id: Option<u32>,
    /// How many copies are in inventory
    pub stack_size: i32,
    /// Recipe keys this item bestows (e.g., "recipe_1234")
    pub bestow_recipe_keys: Vec<String>,
    /// Display names of the recipes this item bestows
    pub bestow_recipe_names: Vec<String>,
    /// Whether ALL bestowed recipes are already known
    pub all_known: bool,
    /// Whether the player meets skill requirements to use this item
    pub meets_requirements: bool,
    /// Skill requirements that are NOT met (skill_name -> (required, current))
    pub unmet_requirements: Vec<UnmetRequirement>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct UnmetRequirement {
    pub skill_name: String,
    pub required: u32,
    pub current: u32,
}

/// Find recipe items in the player's inventory and classify them.
/// Returns items that bestow recipes, indicating whether recipes are already
/// known and whether the player meets the skill requirements.
#[tauri::command]
pub async fn find_recipe_items_in_inventory(
    character_name: String,
    server_name: String,
    db: State<'_, DbPool>,
    state: State<'_, GameDataState>,
) -> Result<Vec<RecipeItemMatch>, String> {
    let conn = db.get().map_err(|e| format!("Database error: {e}"))?;
    let data = state.read().await;

    // Get all inventory items
    let mut inv_stmt = conn
        .prepare(
            "SELECT item_name, item_type_id, stack_size
         FROM game_state_inventory
         WHERE character_name = ?1 AND server_name = ?2",
        )
        .map_err(|e| format!("Query error: {e}"))?;

    let inv_items: Vec<(String, Option<i64>, i32)> = inv_stmt
        .query_map(
            rusqlite::params![character_name, server_name],
            |row: &rusqlite::Row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, Option<i64>>(1)?,
                    row.get::<_, i32>(2)?,
                ))
            },
        )
        .map_err(|e| format!("Query error: {e}"))?
        .filter_map(|r: Result<_, rusqlite::Error>| r.ok())
        .collect();

    // Get all storage items
    let mut storage_stmt = conn
        .prepare(
            "SELECT item_name, item_type_id, stack_size
         FROM game_state_storage
         WHERE character_name = ?1 AND server_name = ?2",
        )
        .map_err(|e| format!("Query error: {e}"))?;

    let storage_items: Vec<(String, Option<i64>, i32)> = storage_stmt
        .query_map(
            rusqlite::params![character_name, server_name],
            |row: &rusqlite::Row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, Option<i64>>(1)?,
                    row.get::<_, i32>(2)?,
                ))
            },
        )
        .map_err(|e| format!("Query error: {e}"))?
        .filter_map(|r: Result<_, rusqlite::Error>| r.ok())
        .collect();

    // Get known recipes
    let mut recipe_stmt = conn
        .prepare(
            "SELECT recipe_id FROM game_state_recipes
         WHERE character_name = ?1 AND server_name = ?2 AND completion_count > 0",
        )
        .map_err(|e| format!("Query error: {e}"))?;

    let known_recipe_ids: std::collections::HashSet<u32> = recipe_stmt
        .query_map(
            rusqlite::params![character_name, server_name],
            |row: &rusqlite::Row| row.get::<_, i64>(0).map(|id| id as u32),
        )
        .map_err(|e| format!("Query error: {e}"))?
        .filter_map(|r: Result<_, rusqlite::Error>| r.ok())
        .collect();

    // Get current skill levels
    let mut skill_stmt = conn
        .prepare(
            "SELECT skill_name, level FROM game_state_skills
         WHERE character_name = ?1 AND server_name = ?2",
        )
        .map_err(|e| format!("Query error: {e}"))?;

    let skill_levels: std::collections::HashMap<String, u32> = skill_stmt
        .query_map(
            rusqlite::params![character_name, server_name],
            |row: &rusqlite::Row| {
                let name: String = row.get(0)?;
                let level: i32 = row.get(1)?;
                Ok((name, level as u32))
            },
        )
        .map_err(|e| format!("Query error: {e}"))?
        .filter_map(|r: Result<_, rusqlite::Error>| r.ok())
        .collect();

    // Aggregate by item type — sum stack sizes across inventory and storage
    let mut type_stacks = std::collections::HashMap::<u32, (String, i32)>::new();
    for entry in inv_items.iter().chain(storage_items.iter()) {
        if let Some(type_id) = entry.1 {
            let id = type_id as u32;
            let ts = type_stacks
                .entry(id)
                .or_insert_with(|| (entry.0.clone(), 0));
            ts.1 += entry.2;
        }
    }

    let mut results: Vec<RecipeItemMatch> = Vec::new();

    for (type_id, (_item_name, total_stack)) in &type_stacks {
        let item = match data.items.get(type_id) {
            Some(item) => item,
            None => continue,
        };

        let bestow_recipes = match &item.bestow_recipes {
            Some(recipes) if !recipes.is_empty() => recipes,
            _ => continue,
        };

        let mut recipe_keys: Vec<String> = Vec::new();
        let mut recipe_names: Vec<String> = Vec::new();
        let mut all_known = true;

        for recipe_val in bestow_recipes {
            if let Some(recipe_key) = recipe_val.as_str() {
                recipe_keys.push(recipe_key.to_string());
                // BestowRecipes values are internal names (e.g. "BasicBeetleSkewer"),
                // resolve via the unified lookup to get the numeric ID
                if let Some(recipe_info) = data.resolve_recipe(recipe_key) {
                    if !known_recipe_ids.contains(&recipe_info.id) {
                        all_known = false;
                    }
                    recipe_names.push(recipe_info.name.clone());
                } else {
                    // Unknown recipe — treat as not known
                    all_known = false;
                    recipe_names.push(recipe_key.to_string());
                }
            }
        }

        // Check skill requirements
        let mut unmet_requirements: Vec<UnmetRequirement> = Vec::new();
        if let Some(ref skill_reqs) = item.skill_reqs {
            if let Some(obj) = skill_reqs.as_object() {
                for (skill_ref, level_val) in obj {
                    let required = level_val.as_u64().unwrap_or(0) as u32;
                    // Resolve skill name
                    let skill_name = data
                        .resolve_skill(skill_ref)
                        .map(|s| s.name.clone())
                        .unwrap_or_else(|| skill_ref.clone());
                    let current = skill_levels.get(&skill_name).copied().unwrap_or(0);
                    if current < required {
                        unmet_requirements.push(UnmetRequirement {
                            skill_name,
                            required,
                            current,
                        });
                    }
                }
            }
        }

        let meets_requirements = unmet_requirements.is_empty();

        results.push(RecipeItemMatch {
            item_id: *type_id,
            item_name: item.name.clone(),
            icon_id: item.icon_id,
            stack_size: *total_stack,
            bestow_recipe_keys: recipe_keys,
            bestow_recipe_names: recipe_names,
            all_known,
            meets_requirements,
            unmet_requirements,
        });
    }

    // Sort: known duplicates first, then learnable, then others
    results.sort_by(|a, b| {
        b.all_known
            .cmp(&a.all_known)
            .then(b.meets_requirements.cmp(&a.meets_requirements))
            .then(a.item_name.cmp(&b.item_name))
    });

    Ok(results)
}

// ── Lorebook query commands ─────────────────────────────────────────────────

/// Get all lorebook entries, sorted by title.
#[tauri::command]
pub async fn get_all_lorebooks(
    state: State<'_, GameDataState>,
) -> Result<Vec<LorebookEntry>, String> {
    let data = state.read().await;
    let mut results: Vec<LorebookEntry> = data.lorebooks.books.values().cloned().collect();
    results.sort_by(|a, b| {
        let a_title = a.title.as_deref().unwrap_or("");
        let b_title = b.title.as_deref().unwrap_or("");
        a_title.cmp(b_title)
    });
    Ok(results)
}

/// Get lorebook category info.
#[tauri::command]
pub async fn get_lorebook_categories(
    state: State<'_, GameDataState>,
) -> Result<Vec<LorebookCategoryInfo>, String> {
    let data = state.read().await;
    let mut results: Vec<LorebookCategoryInfo> =
        data.lorebooks.categories.values().cloned().collect();
    results.sort_by(|a, b| {
        let a_sort = a
            .sort_title
            .as_deref()
            .unwrap_or(a.title.as_deref().unwrap_or(""));
        let b_sort = b
            .sort_title
            .as_deref()
            .unwrap_or(b.title.as_deref().unwrap_or(""));
        a_sort.cmp(b_sort)
    });
    Ok(results)
}

/// Search lorebook entries by title, text, or category.
#[tauri::command]
pub async fn search_lorebooks(
    query: String,
    category: Option<String>,
    state: State<'_, GameDataState>,
) -> Result<Vec<LorebookEntry>, String> {
    let data = state.read().await;
    let q = query.to_lowercase();

    let mut results: Vec<LorebookEntry> = data
        .lorebooks
        .books
        .values()
        .filter(|b| {
            // Category filter
            if let Some(ref cat) = category {
                if b.category.as_deref() != Some(cat.as_str()) {
                    return false;
                }
            }
            // Text query filter
            if q.is_empty() {
                return true;
            }
            let title_match = b
                .title
                .as_ref()
                .map(|t| t.to_lowercase().contains(&q))
                .unwrap_or(false);
            let text_match = b
                .text
                .as_ref()
                .map(|t| t.to_lowercase().contains(&q))
                .unwrap_or(false);
            let location_match = b
                .location_hint
                .as_ref()
                .map(|l| l.to_lowercase().contains(&q))
                .unwrap_or(false);
            title_match || text_match || location_match
        })
        .cloned()
        .collect();

    results.sort_by(|a, b| {
        let a_title = a.title.as_deref().unwrap_or("");
        let b_title = b.title.as_deref().unwrap_or("");
        a_title.cmp(b_title)
    });
    Ok(results)
}

// ── Brewing commands ────────────────────────────────────────────────────────

#[tauri::command]
pub async fn get_brewing_recipes(
    state: State<'_, GameDataState>,
) -> Result<Vec<BrewingRecipe>, String> {
    let data = state.read().await;
    Ok(data.brewing_recipes.clone())
}

#[tauri::command]
pub async fn get_brewing_ingredients(
    state: State<'_, GameDataState>,
) -> Result<Vec<BrewingIngredient>, String> {
    let data = state.read().await;
    Ok(data.brewing_ingredients.clone())
}

// ── CDN Diff commands ────────────────────────────────────────────────────────

use crate::cdn_diff::{self, FileDiff, FileDiffSummary};

/// Download the prior CDN version's data files and compute a diff summary.
/// The downloaded files are kept in `_diff_old/` and reused across sessions.
/// They are only re-downloaded when the current cached version changes
/// (i.e. we track which old version we downloaded via `_diff_old/version.txt`).
#[tauri::command]
pub async fn cdn_diff_summary(app: AppHandle) -> Result<Vec<FileDiffSummary>, String> {
    let data_dir = data_cache_dir(&app)?;
    let cached_version = cdn::read_cached_version(&data_dir)
        .await
        .ok_or("No cached CDN version found")?;

    if cached_version == 0 {
        return Err("Cannot diff: cached version is 0".into());
    }

    let old_version = cached_version - 1;
    let old_dir = data_dir.join("_diff_old");
    std::fs::create_dir_all(&old_dir).map_err(|e| format!("Failed to create diff dir: {e}"))?;

    // Only re-download if the cached old version doesn't match what we need
    let old_cached = cdn::read_cached_version(&old_dir).await;
    if old_cached != Some(old_version) {
        cdn::download_all_data_files(old_version, &old_dir).await?;
        // Also grab old translation files for string diffing
        let _ = cdn::download_translation_zip(old_version, &old_dir).await;
        cdn::write_cached_version(&old_dir, old_version).await?;
    }

    cdn_diff::compute_summary(&old_dir, &data_dir).await
}

/// Compute a detailed diff for a single CDN data file.
/// Reads the prior version from the `_diff_old/` directory that was populated
/// by `cdn_diff_summary` — no additional download needed.
#[tauri::command]
pub async fn cdn_diff_file(app: AppHandle, file_name: String) -> Result<FileDiff, String> {
    if !cdn::DATA_FILES.contains(&file_name.as_str())
        && !cdn::TRANSLATION_FILES.contains(&file_name.as_str())
    {
        return Err(format!("Unknown data file: {file_name}"));
    }

    let data_dir = data_cache_dir(&app)?;
    let old_dir = data_dir.join("_diff_old");

    if !old_dir.exists() {
        return Err("No diff data available — run CDN Diff Summary first".into());
    }

    cdn_diff::compute_file_diff(&file_name, &old_dir, &data_dir).await
}

// ── Enemy (AI) query commands ─────────────────────────────────────────────────

/// Get all AI/enemy entries as summaries.
#[tauri::command]
pub async fn get_all_enemies(state: State<'_, GameDataState>) -> Result<Vec<AiSummary>, String> {
    let data = state.read().await;
    let mut results: Vec<AiSummary> = data
        .ai
        .iter()
        .map(|(key, info)| info.to_summary(key))
        .collect();
    results.sort_by(|a, b| a.key.cmp(&b.key));
    Ok(results)
}

/// Search AI/enemy entries by key name.
#[tauri::command]
pub async fn search_enemies(
    query: String,
    state: State<'_, GameDataState>,
) -> Result<Vec<AiSummary>, String> {
    let data = state.read().await;
    let q = query.to_lowercase();

    let mut results: Vec<AiSummary> = data
        .ai
        .iter()
        .filter(|(key, info)| {
            let key_match = key.to_lowercase().contains(&q);
            let comment_match = info
                .raw
                .get("Comment")
                .and_then(|v| v.as_str())
                .map(|c| c.to_lowercase().contains(&q))
                .unwrap_or(false);
            let strategy_match = info
                .raw
                .get("Strategy")
                .and_then(|v| v.as_str())
                .map(|s| s.to_lowercase().contains(&q))
                .unwrap_or(false);
            let ability_match = info
                .raw
                .get("Abilities")
                .and_then(|v| v.as_object())
                .map(|m| m.keys().any(|k| k.to_lowercase().contains(&q)))
                .unwrap_or(false);
            key_match || comment_match || strategy_match || ability_match
        })
        .map(|(key, info)| info.to_summary(key))
        .collect();
    results.sort_by(|a, b| a.key.cmp(&b.key));
    Ok(results)
}

/// Get a single AI/enemy entry with full raw data.
#[tauri::command]
pub async fn get_enemy(
    key: String,
    state: State<'_, GameDataState>,
) -> Result<Option<AiSummary>, String> {
    let data = state.read().await;
    Ok(data.ai.get(&key).map(|info| info.to_summary(&key)))
}

// ── Monster list from translation strings ────────────────────────────────────

/// A monster entry derived from translation strings, enriched with AI data
/// when available. This is the primary data source for the Enemies browser.
#[derive(serde::Serialize, Clone)]
pub struct MonsterEntry {
    /// Internal identifier (e.g. "AreaSerbule/GiantRat")
    pub key: String,
    /// Display name from translation strings (e.g. "Giant Rat")
    pub name: String,
    /// Area key if the monster is area-specific (e.g. "AreaSerbule")
    pub area_key: Option<String>,
    /// Friendly area name
    pub area_name: Option<String>,
    // AI-enriched fields (None if no matching AI template)
    pub strategy: Option<String>,
    pub mobility_type: Option<String>,
    pub comment: Option<String>,
    pub swimming: Option<bool>,
    pub uncontrolled_pet: Option<bool>,
    pub ability_count: Option<usize>,
    pub ability_names: Option<Vec<String>>,
}

/// Build the full monster list from translation strings, enriched with AI data.
#[tauri::command]
pub async fn get_all_monsters(
    app: AppHandle,
    state: State<'_, GameDataState>,
) -> Result<Vec<MonsterEntry>, String> {
    let data_dir = data_cache_dir(&app)?;
    let path = data_dir.join("translation").join("strings_requested.json");

    if !path.exists() {
        // Fall back: return AI-only entries if translation strings aren't available
        let data = state.read().await;
        return Ok(data
            .ai
            .iter()
            .map(|(key, info)| {
                let summary = info.to_summary(key);
                MonsterEntry {
                    key: key.clone(),
                    name: format_ai_key(key),
                    area_key: None,
                    area_name: None,
                    strategy: summary.strategy,
                    mobility_type: summary.mobility_type,
                    comment: summary.comment,
                    swimming: Some(summary.swimming),
                    uncontrolled_pet: Some(summary.uncontrolled_pet),
                    ability_count: Some(summary.ability_count),
                    ability_names: Some(summary.ability_names),
                }
            })
            .collect());
    }

    let json = tokio::fs::read_to_string(&path)
        .await
        .map_err(|e| format!("Failed to read translation strings: {e}"))?;

    let map: HashMap<String, String> = serde_json::from_str(&json)
        .map_err(|e| format!("Failed to parse translation strings: {e}"))?;

    let data = state.read().await;

    // Deduplicate by display name (many monster variants share the same name)
    // Track which names we've seen along with their first key and area
    let mut seen: HashMap<String, MonsterEntry> = HashMap::new();

    for (key, name) in &map {
        if !key.starts_with("monster_") || !key.ends_with("_Name") {
            continue;
        }

        let monster_id = &key["monster_".len()..key.len() - "_Name".len()];

        // Extract area key if present (format: Area{Name}/{MonsterPart})
        let (area_key, ai_part) = if let Some(slash) = monster_id.find('/') {
            let area = &monster_id[..slash];
            let monster_part = &monster_id[slash + 1..];
            (Some(area.to_string()), monster_part.to_string())
        } else {
            (None, monster_id.to_string())
        };

        // Use display name as dedup key; prefer entries with area info
        if let Some(existing) = seen.get(name) {
            // If existing has no area but this one does, replace
            if existing.area_key.is_none() && area_key.is_some() {
                // keep going to replace
            } else {
                continue;
            }
        }

        let area_name = area_key
            .as_ref()
            .and_then(|ak| data.areas.get(ak))
            .and_then(|a| a.friendly_name.clone());

        // Try to find matching AI data: strip trailing numbers/letters
        let ai_base = ai_part
            .trim_end_matches(|c: char| c.is_ascii_digit())
            .trim_end_matches(|c: char| {
                c.is_ascii_uppercase() && c != ai_part.chars().next().unwrap_or('_')
            });
        let ai_info = data.ai.get(&ai_part).or_else(|| data.ai.get(ai_base));

        let entry = if let Some(info) = ai_info {
            let summary = info.to_summary(&ai_part);
            MonsterEntry {
                key: monster_id.to_string(),
                name: name.clone(),
                area_key,
                area_name,
                strategy: summary.strategy,
                mobility_type: summary.mobility_type,
                comment: summary.comment,
                swimming: Some(summary.swimming),
                uncontrolled_pet: Some(summary.uncontrolled_pet),
                ability_count: Some(summary.ability_count),
                ability_names: Some(summary.ability_names),
            }
        } else {
            MonsterEntry {
                key: monster_id.to_string(),
                name: name.clone(),
                area_key,
                area_name,
                strategy: None,
                mobility_type: None,
                comment: None,
                swimming: None,
                uncontrolled_pet: None,
                ability_count: None,
                ability_names: None,
            }
        };

        seen.insert(name.clone(), entry);
    }

    let mut results: Vec<MonsterEntry> = seen.into_values().collect();
    results.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(results)
}

/// Format an AI key into a display name (e.g. "GiantRat" → "Giant Rat")
fn format_ai_key(key: &str) -> String {
    let mut result = String::with_capacity(key.len() + 10);
    for (i, c) in key.chars().enumerate() {
        if i > 0 && c.is_uppercase() {
            let prev = key.chars().nth(i - 1).unwrap_or('_');
            if prev.is_lowercase() {
                result.push(' ');
            }
        }
        result.push(c);
    }
    result.replace('_', " ")
}

// ── Gardening product chain ─────────────────────────────────────────────────

/// A compact item summary for gardening chain display.
#[derive(serde::Serialize, Clone)]
pub struct GardeningChainItem {
    pub id: u32,
    pub name: String,
    pub icon_id: Option<u32>,
    pub role: String, // "seedling", "plant", or "product"
}

/// A recipe in the gardening chain that connects a plant to its products.
#[derive(serde::Serialize, Clone)]
pub struct GardeningChainRecipe {
    pub id: u32,
    pub name: String,
    pub skill: Option<String>,
    pub skill_level_req: Option<f32>,
}

/// The full gardening product chain for a seedling or plant item.
#[derive(serde::Serialize, Clone)]
pub struct GardeningProductChain {
    /// The seedling item (if found or if the input was a seedling)
    pub seedling: Option<GardeningChainItem>,
    /// The plant/vegetable item produced by planting the seedling
    pub plant: Option<GardeningChainItem>,
    /// Recipes that use the plant as an ingredient, and their products
    pub products: Vec<GardeningChainProduct>,
    /// The gardening level required to plant (from seedling skill_reqs)
    pub gardening_level: Option<u32>,
}

/// A single product produced from a plant via a recipe.
#[derive(serde::Serialize, Clone)]
pub struct GardeningChainProduct {
    pub recipe: GardeningChainRecipe,
    pub items: Vec<GardeningChainItem>,
}

/// Suffixes that indicate an item is a seedling/seed variant.
const SEEDLING_SUFFIXES: &[&str] = &["Seedling", "Leafling", "Seeds"];

/// Strip known seedling suffixes from an internal name to get the base plant name.
fn strip_seedling_suffix(internal_name: &str) -> Option<&str> {
    for suffix in SEEDLING_SUFFIXES {
        if internal_name.ends_with(suffix) && internal_name.len() > suffix.len() {
            return Some(&internal_name[..internal_name.len() - suffix.len()]);
        }
    }
    None
}

/// Check if an item is a seedling (has Seedling=XX keyword).
fn is_seedling(item: &ItemInfo) -> bool {
    item.keywords.iter().any(|kw| kw.starts_with("Seedling="))
}

/// Find the plant item that a seedling produces, by matching internal names.
fn find_plant_for_seedling<'a>(data: &'a GameData, seedling: &ItemInfo) -> Option<&'a ItemInfo> {
    let internal = seedling.internal_name.as_deref()?;
    let base = strip_seedling_suffix(internal)?;

    // Look up by internal name
    if let Some(item) = data.resolve_item(base) {
        return Some(item);
    }

    None
}

/// Find the seedling that produces a given plant item.
fn find_seedling_for_plant<'a>(data: &'a GameData, plant: &ItemInfo) -> Option<&'a ItemInfo> {
    let plant_internal = plant.internal_name.as_deref()?;

    for suffix in SEEDLING_SUFFIXES {
        let candidate = format!("{}{}", plant_internal, suffix);
        if let Some(item) = data.resolve_item(&candidate) {
            if is_seedling(item) {
                return Some(item);
            }
        }
    }

    None
}

fn item_to_chain_item(item: &ItemInfo, role: &str) -> GardeningChainItem {
    GardeningChainItem {
        id: item.id,
        name: item.name.clone(),
        icon_id: item.icon_id,
        role: role.to_string(),
    }
}

/// Get the full gardening product chain for a seedling or plant item.
///
/// If given a seedling, walks: seedling → plant → recipes using plant → products.
/// If given a plant (vegetable), walks: find seedling → plant → recipes → products.
/// Returns None if the item isn't part of a gardening chain.
#[tauri::command]
pub async fn get_gardening_product_chain(
    item_id: u32,
    state: State<'_, GameDataState>,
) -> Result<Option<GardeningProductChain>, String> {
    let data = state.read().await;

    let item = match data.items.get(&item_id) {
        Some(i) => i,
        None => return Ok(None),
    };

    let (seedling_item, plant_item) = if is_seedling(item) {
        // Input is a seedling — find its plant
        let plant = find_plant_for_seedling(&data, item);
        (Some(item), plant)
    } else {
        // Input might be a plant — try to find its seedling
        let seedling = find_seedling_for_plant(&data, item);
        if seedling.is_some() {
            (seedling, Some(item))
        } else {
            // Not a gardening item
            return Ok(None);
        }
    };

    // Extract gardening level from seedling's skill_reqs
    let gardening_level = seedling_item.and_then(|s| {
        s.skill_reqs.as_ref().and_then(|reqs: &Value| {
            reqs.get("Gardening")
                .and_then(|v: &Value| v.as_u64())
                .map(|v| v as u32)
        })
    });

    // Build product list from recipes that use the plant item
    let mut products = Vec::new();
    if let Some(plant) = plant_item {
        if let Some(recipe_ids) = data.recipes_using_item.get(&plant.id) {
            for recipe_id in recipe_ids {
                if let Some(recipe) = data.recipes.get(recipe_id) {
                    let product_items: Vec<GardeningChainItem> = recipe
                        .result_item_ids
                        .iter()
                        .filter_map(|rid| data.items.get(rid))
                        .map(|i| item_to_chain_item(i, "product"))
                        .collect();

                    if !product_items.is_empty() {
                        products.push(GardeningChainProduct {
                            recipe: GardeningChainRecipe {
                                id: recipe.id,
                                name: recipe.name.clone(),
                                skill: recipe.skill.clone(),
                                skill_level_req: recipe.skill_level_req,
                            },
                            items: product_items,
                        });
                    }
                }
            }
        }
    }

    // Sort products by skill level requirement
    products.sort_by(|a, b| {
        let a_lvl = a.recipe.skill_level_req.unwrap_or(0.0);
        let b_lvl = b.recipe.skill_level_req.unwrap_or(0.0);
        a_lvl
            .partial_cmp(&b_lvl)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    Ok(Some(GardeningProductChain {
        seedling: seedling_item.map(|s| item_to_chain_item(s, "seedling")),
        plant: plant_item.map(|p| item_to_chain_item(p, "plant")),
        products,
        gardening_level,
    }))
}

#[cfg(test)]
mod build_stats_parsing_tests {
    use super::{
        category_attack_tokens, parse_attacks_deal, parse_base_damage,
        parse_conditional_direct_text, parse_conditional_indirect_text, parse_dot_apply,
        parse_dot_clause, parse_named_damage_rest, parse_named_type_conversion,
        parse_names_deal_direct, parse_skill_type_conversion, parse_token_value,
        select_default_tier_key, sniff_damage_type, TsysTierInfo,
    };
    use std::collections::HashMap;

    fn tier(min: u32, max: u32) -> TsysTierInfo {
        TsysTierInfo {
            effect_descs: vec![],
            min_level: Some(min),
            max_level: Some(max),
            min_rarity: None,
            skill_level_prereq: None,
        }
    }

    #[test]
    fn category_attack_tokens_maps_keywords_to_tokens() {
        // A core crushing attack (e.g. Cobra Strike: Attack, BodypartAttack, CoreAttack, ...).
        let kws = ["Attack", "BodypartAttack", "CoreAttack", "CrushingAttack"].map(String::from);
        let (delta, mods) = category_attack_tokens(&kws);
        assert_eq!(delta, vec!["BOOST_ABILITY_COREATTACK"]);
        assert_eq!(mods, vec!["MOD_ABILITY_COREATTACK"]);

        // A nice poison attack carries both the general and type-specific category tokens.
        let kws = ["NiceAttack", "NicePoisonAttack"].map(String::from);
        let (delta, _) = category_attack_tokens(&kws);
        assert_eq!(
            delta,
            vec!["BOOST_ABILITY_NICEATTACK", "BOOST_ABILITY_NICEPOISONATTACK"]
        );

        // Attack-shape keyword groups (gear grants e.g. "Kick Damage +5" / "Melee Attack Damage").
        let (delta, mods) = category_attack_tokens(&["Kick".to_string(), "Melee".to_string()]);
        assert_eq!(delta, vec!["BOOST_ABILITY_KICK", "BOOST_ABILITY_MELEE"]);
        assert_eq!(mods, vec!["MOD_ABILITY_KICK", "MOD_ABILITY_MELEE"]);

        // Non-category keywords contribute nothing.
        let (delta, mods) = category_attack_tokens(&["Ranged".to_string(), "Attack".to_string()]);
        assert!(delta.is_empty() && mods.is_empty());
    }

    #[test]
    fn parse_conditional_direct_pair_form() {
        // Archery's flat direct+indirect typed pair.
        let out = parse_conditional_direct_text(
            "Direct Poison and Acid Damage +9 and Indirect Poison and Acid +5 (per tick) while Archery skill active.",
        );
        assert_eq!(
            out,
            vec![
                (
                    "BOOST_POISON_DIRECT".to_string(),
                    9.0,
                    "Archery".to_string()
                ),
                ("BOOST_ACID_DIRECT".to_string(), 9.0, "Archery".to_string()),
                (
                    "BOOST_POISON_INDIRECT".to_string(),
                    5.0,
                    "Archery".to_string()
                ),
                (
                    "BOOST_ACID_INDIRECT".to_string(),
                    5.0,
                    "Archery".to_string()
                ),
            ]
        );
        // Single-type variant.
        let out = parse_conditional_direct_text(
            "Direct Fire Damage +12 and Indirect Fire +6 (per tick) while Archery skill active.",
        );
        assert_eq!(
            out,
            vec![
                ("BOOST_FIRE_DIRECT".to_string(), 12.0, "Archery".to_string()),
                (
                    "BOOST_FIRE_INDIRECT".to_string(),
                    6.0,
                    "Archery".to_string()
                ),
            ]
        );
    }

    #[test]
    fn parse_conditional_direct_percent_list_form() {
        // Hammer's percent list form.
        let out = parse_conditional_direct_text(
            "Direct Electricity Damage, Direct Fire Damage, and Direct Cold Damage +11% while Hammer skill active",
        );
        assert_eq!(
            out,
            vec![
                (
                    "MOD_ELECTRICITY_DIRECT".to_string(),
                    0.11,
                    "Hammer".to_string()
                ),
                ("MOD_FIRE_DIRECT".to_string(), 0.11, "Hammer".to_string()),
                ("MOD_COLD_DIRECT".to_string(), 0.11, "Hammer".to_string()),
            ]
        );
        // The same text as the CDN actually stores it, behind a leading icon tag.
        assert_eq!(
            parse_conditional_direct_text(
                "<icon=108>Direct Electricity Damage, Direct Fire Damage, and Direct Cold Damage +11% while Hammer skill active",
            ),
            out
        );
        // Unrelated prose stays empty.
        assert!(
            parse_conditional_direct_text("Max Armor +75 while Shield skill active").is_empty()
        );
        assert!(parse_conditional_direct_text(
            "Indirect Psychic damage is +53% per tick while Vampirism skill active"
        )
        .is_empty());
    }

    #[test]
    fn parse_named_damage_rest_flat_and_percent() {
        // "Many Cuts and Debilitating Blow Damage +39" → rest after names.
        assert_eq!(parse_named_damage_rest("Damage +39"), Some((39.0, false)));
        // Lowercase variant ("Finishing Blow and Decapitate damage +47").
        assert_eq!(parse_named_damage_rest("damage +47"), Some((47.0, false)));
        // Percent with rider ("Parry and Riposte Damage +25% and Power Cost -3").
        assert_eq!(
            parse_named_damage_rest("Damage +25% and Power Cost -3"),
            Some((0.25, true))
        );
        // "damage is +N" phrasing (Pixie Flare).
        assert_eq!(
            parse_named_damage_rest(
                "damage is +45 and its damage becomes Fire instead of Electricity"
            ),
            Some((45.0, false))
        );
        // "deals +N% damage" with a benign rider.
        assert_eq!(
            parse_named_damage_rest("deals +25% damage and boosts your Accuracy +5 for 10 seconds"),
            Some((0.25, true))
        );
        // "deals +N damage" with a sentence rider ("Seismic Impact deals +55 Damage. If target …" —
        // the conditional part is a separate clause and must not block the unconditional bonus).
        assert_eq!(
            parse_named_damage_rest(
                "deals +55 Damage. If target is Knocked Down it deals a further +10% damage"
            ),
            Some((55.0, false))
        );
        // Bare value allowed with an explicit type word ("Wind Strike and Heart Piercer deal 30 armor damage").
        assert_eq!(
            parse_named_damage_rest("deal 30 armor damage"),
            Some((30.0, false))
        );
        // "to Armor" qualifier is part of the hit ("Acid Arrow deals +20 damage to Armor").
        assert_eq!(
            parse_named_damage_rest("deals +20 damage to Armor"),
            Some((20.0, false))
        );
        // Conversion clause first (SoulBiteFire: "Soul Bite deals Fire damage (instead of
        // Darkness) and damage is +2") — the damage clause behind it must still parse.
        assert_eq!(
            parse_named_damage_rest("deals Fire damage (instead of Darkness) and damage is +2"),
            Some((2.0, false))
        );
        // Same conversion clause followed by a DoT rider (SpiritBoltFireDoT /
        // DimensionalSnareDoT) has no direct-damage clause — must NOT parse as one.
        assert_eq!(
            parse_named_damage_rest(
                "deals Fire damage (instead of Darkness) and ignites the target, dealing 54 Fire damage over 12 seconds"
            ),
            None
        );
    }

    #[test]
    fn parse_named_damage_rest_rejects_conditional_and_nondamage() {
        // Delayed bonus damage is not part of the up-front hit.
        assert_eq!(
            parse_named_damage_rest("deals 60 Poison damage after a 3-second delay"),
            None
        );
        // Bare value without a type word is ambiguous.
        assert_eq!(parse_named_damage_rest("deals 60 damage"), None);
        // Temporal buff.
        assert_eq!(parse_named_damage_rest("Damage +30 for 10 seconds"), None);
        // Target-conditional.
        assert_eq!(
            parse_named_damage_rest("Damage +25 versus Elite enemies"),
            None
        );
        // Not a damage clause at all.
        assert_eq!(
            parse_named_damage_rest("boosts your Nice Attack Damage +30 for 10 seconds"),
            None
        );
        assert_eq!(
            parse_named_damage_rest("hastens the current reuse timer by 1 second"),
            None
        );
    }

    #[test]
    fn parse_attacks_deal_skill_and_keyword_wide() {
        // Skill-wide with a benign rider.
        assert_eq!(
            parse_attacks_deal("Hammer attacks deal +33% damage but generate +25% Rage"),
            Some(("Hammer".to_string(), 0.33, true))
        );
        // Keyword-wide flat.
        assert_eq!(
            parse_attacks_deal("Bomb attacks deal +50 damage and hasten the current reuse timer of Healing Mist by 1 second"),
            Some(("Bomb".to_string(), 50.0, false))
        );
        // "Your" prefix.
        assert_eq!(
            parse_attacks_deal("Your Unarmed attacks deal +7 damage and have +5 Accuracy"),
            Some(("Unarmed".to_string(), 7.0, false))
        );
    }

    #[test]
    fn parse_base_damage_skill_wide_and_rejects_conditional() {
        // Necromancer's mod: percent boost; the benign "; Summoned Skeletons …" pet-damage tail
        // is ignored (that clause is intentionally excluded).
        assert_eq!(
            parse_base_damage(
                "Necromancy Base Damage +60%; Summoned Skeletons deal +12% direct damage"
            ),
            Some(("Necromancy".to_string(), 0.6, true))
        );
        // Flat low tier.
        assert_eq!(
            parse_base_damage(
                "Necromancy Base Damage +5; Summoned Skeletons deal +1% direct damage"
            ),
            Some(("Necromancy".to_string(), 5.0, false))
        );
        // Trailing temporal rider rejected.
        assert_eq!(
            parse_base_damage("Bat Base Damage +60% for 10 seconds"),
            None
        );
        // Trigger-prefixed conditional never begins with the clause, so it can't match.
        assert_eq!(
            parse_base_damage(
                "Shadow Feint raises your Lycanthropy Base Damage +79% until you trigger the teleport"
            ),
            None
        );
    }

    #[test]
    fn parse_skill_type_conversion_matches_instead_deal() {
        // Viper Halberd / Tactician's Halberd (with and without the trailing DoT-rider sentence).
        assert_eq!(
            parse_skill_type_conversion(
                "Staff abilities that normally deal Crushing damage instead deal Slashing damage. All slashing abilities apply Poison Slice damage for 10 seconds."
            ),
            Some(("Staff".to_string(), "Crushing".to_string(), "Slashing".to_string()))
        );
        assert_eq!(
            parse_skill_type_conversion(
                "Knife abilities that normally deal Slashing damage instead deal Cold damage"
            ),
            Some((
                "Knife".to_string(),
                "Slashing".to_string(),
                "Cold".to_string()
            ))
        );
        assert_eq!(parse_skill_type_conversion("Staff Base Damage % +13"), None);
    }

    #[test]
    fn parse_named_type_conversion_variants() {
        // Rests after the leading ability names, across all data variants.
        assert_eq!(
            parse_named_type_conversion("deals +30 damage, and the damage type becomes Fire."),
            Some("Fire".to_string())
        );
        assert_eq!(
            parse_named_type_conversion(
                "deals +45 damage, and damage type is changed to Electricity"
            ),
            Some("Electricity".to_string())
        );
        // Sentence-start variant ("Lethal Force deals +N damage. Damage type becomes Fire").
        assert_eq!(
            parse_named_type_conversion("deals +40 damage. Damage type becomes Fire"),
            Some("Fire".to_string())
        );
        // Possessive form rest ("Fairy Fire's damage type becomes Fire, and it deals …").
        assert_eq!(
            parse_named_type_conversion("damage type becomes Fire, and it deals an additional 100 Fire damage over 10 seconds"),
            Some("Fire".to_string())
        );
        // "direct damage becomes <Type> damage" (Horns of Hate → Deadly Emission).
        assert_eq!(
            parse_named_type_conversion("direct damage becomes Darkness damage."),
            Some("Darkness".to_string())
        );
        // "its damage becomes <Type> instead of <Type>" (Pixie Flare).
        assert_eq!(
            parse_named_type_conversion(
                "damage is +45 and its damage becomes Fire instead of Electricity"
            ),
            Some("Fire".to_string())
        );
        // Sentence-start "Damage becomes <Type>" (Infuriating Fist).
        assert_eq!(
            parse_named_type_conversion("damage +19. Damage becomes Trauma instead of Crushing"),
            Some("Trauma".to_string())
        );
        // "deals <Type> damage instead of <Type>" (Embrace of Despair — becomes a Sonic ability
        // but its DAMAGE type is Nature).
        assert_eq!(
            parse_named_type_conversion("becomes a Sonic ability and deals Nature damage instead of Psychic. Sonic Ability Damage +15% while Vampirism skill active"),
            Some("Nature".to_string())
        );
        // Parenthesized "deals <Type> damage (instead of <Type>)" (SpiritFox's SoulBiteFire /
        // SpiritBoltFireDoT / DimensionalSnareDoT).
        assert_eq!(
            parse_named_type_conversion("deals Fire damage (instead of Darkness) and damage is +2"),
            Some("Fire".to_string())
        );
        assert_eq!(
            parse_named_type_conversion(
                "deals Fire damage (instead of Darkness) and ignites the target, dealing 54 Fire damage over 12 seconds"
            ),
            Some("Fire".to_string())
        );
        // Keyword changes are NOT damage-type conversions ("Delerium becomes a 15m Burst attack
        // that deals +30% damage to targets that are not focused on you").
        assert_eq!(
            parse_named_type_conversion("becomes a 15m Burst attack that deals +30% damage to targets that are not focused on you"),
            None
        );
        // The "deal direct <Type> damage" item form is handled by parse_names_deal_direct, not here.
        assert_eq!(
            parse_named_type_conversion("causes your next attack to deal +50 damage if it deals direct Psychic, Trauma, or Poison damage"),
            None
        );
    }

    #[test]
    fn parse_names_deal_direct_matches_listed_abilities() {
        // Vanquisher's Blade / Flaming Gazluk Sword text — note "Windstrike" is spelled without
        // the space the ability name ("Wind Strike") has, so matching must be spacing-insensitive.
        const DESC: &str = "Sword Slash, Riposte, Windstrike, Finishing Blow, Debilitating Blow and Heart Piercer deal direct Fire damage. All Sword abilities apply Flaming Sword damage for 10 seconds.";
        for name in [
            "Sword Slash 6", // tier suffixes are stripped
            "Riposte",
            "Wind Strike 4", // spelled "Windstrike" in the item text
            "Finishing Blow 8",
            "Debilitating Blow 3",
            "Heart Piercer 5",
        ] {
            assert_eq!(
                parse_names_deal_direct(DESC, name),
                Some("Fire".to_string()),
                "{name}"
            );
        }
        // Sword abilities NOT in the list stay unconverted.
        assert_eq!(parse_names_deal_direct(DESC, "Parry 4"), None);
        assert_eq!(parse_names_deal_direct(DESC, "Many Cuts 5"), None);
        // "Slash" must not substring-match "Sword Slash".
        assert_eq!(parse_names_deal_direct(DESC, "Slash"), None);
    }

    #[test]
    fn parse_names_deal_direct_rejects_conditional_prose() {
        // "Finishing Blow causes your next attack to deal +50 damage if it deals direct Psychic,
        // Trauma, or Poison damage" — the pre-clause text isn't a bare name segment.
        const DESC: &str = "Finishing Blow causes your next attack to deal +50 damage if it deals direct Psychic, Trauma, or Poison damage";
        assert_eq!(parse_names_deal_direct(DESC, "Finishing Blow 8"), None);
    }

    #[test]
    fn parse_dot_apply_type_and_skill_scoped() {
        // Type-scoped rider following a conversion sentence in the same desc.
        assert_eq!(
            parse_dot_apply(
                "Staff abilities that normally deal Crushing damage instead deal Slashing damage. All slashing abilities apply Poison Slice damage for 10 seconds."
            ),
            Some(("slashing".to_string(), "Poison Slice".to_string(), 10.0))
        );
        // Skill-scoped rider (Rapier Wit).
        assert_eq!(
            parse_dot_apply("Sword abilities apply Psychic scream damage for 10 seconds."),
            Some(("Sword".to_string(), "Psychic scream".to_string(), 10.0))
        );
        assert_eq!(parse_dot_apply("Staff Base Damage % +13"), None);
    }

    #[test]
    fn sniff_damage_type_from_dot_names() {
        assert_eq!(
            sniff_damage_type("Poison Slice Poison Slice Damage over Time (Per Second)"),
            Some("Poison".to_string())
        );
        assert_eq!(
            sniff_damage_type("Psychic scream"),
            Some("Psychic".to_string())
        );
        // Flavor alias: "Flaming Sword" carries no literal type name.
        assert_eq!(
            sniff_damage_type("Flaming Sword Flaming Sword Damage over Time (Per Second)"),
            Some("Fire".to_string())
        );
        assert_eq!(sniff_damage_type("Mystery Goo"), None);
    }

    #[test]
    fn parse_attacks_deal_rejects_conditionals() {
        // Armor-gated.
        assert_eq!(
            parse_attacks_deal(
                "Kick attacks deal +30% damage when you have 33% or less of your Armor left"
            ),
            None
        );
        // Target-species/Elite conditional (Gourmand food buffs).
        assert_eq!(
            parse_attacks_deal("Your melee attacks deal +45 damage to Elite enemies"),
            None
        );
        assert_eq!(
            parse_attacks_deal("Your attacks deal +12 damage to Elves"),
            None
        );
        // Not an attacks-deal clause.
        assert_eq!(
            parse_attacks_deal("Many Cuts and Debilitating Blow Damage +39"),
            None
        );
    }

    #[test]
    fn parse_token_value_two_and_three_segments() {
        // Plain item/mod token: no skill condition.
        assert_eq!(
            parse_token_value("{BOOST_PSYCHIC_DIRECT}{104}"),
            Some(("BOOST_PSYCHIC_DIRECT".to_string(), 104.0, None))
        );
        // Conditional gear mod: the 3rd segment is the required skill.
        assert_eq!(
            parse_token_value("{MOD_NATURE_INDIRECT}{0.1}{Druid}"),
            Some((
                "MOD_NATURE_INDIRECT".to_string(),
                0.1,
                Some("Druid".to_string())
            ))
        );
        // Fractional value with trailing whitespace.
        assert_eq!(
            parse_token_value("{MOD_PSYCHIC_INDIRECT}{0.05}{Rabbit} "),
            Some((
                "MOD_PSYCHIC_INDIRECT".to_string(),
                0.05,
                Some("Rabbit".to_string())
            ))
        );
    }

    #[test]
    fn parse_token_value_rejects_prose_and_malformed() {
        assert_eq!(
            parse_token_value(
                "Indirect Psychic damage is +53% per tick while Vampirism skill active"
            ),
            None
        );
        assert_eq!(parse_token_value("{ONLY_ONE_SEGMENT}"), None);
        assert_eq!(parse_token_value("{TOKEN}{not_a_number}"), None);
    }

    #[test]
    fn parse_conditional_text_single_type() {
        // The user's example: VampirismPsychMod tier text.
        let out = parse_conditional_indirect_text(
            "Indirect Psychic damage is +53% per tick while Vampirism skill active",
        );
        assert_eq!(
            out,
            vec![(
                "MOD_PSYCHIC_INDIRECT".to_string(),
                0.53,
                "Vampirism".to_string()
            )]
        );
    }

    #[test]
    fn parse_conditional_text_two_types_and_trailing_period() {
        // Warden-family two-type variant, with a trailing period.
        let out = parse_conditional_indirect_text(
            "Indirect Poison and Indirect Trauma damage is +5% per tick while Warden skill active.",
        );
        assert_eq!(
            out,
            vec![
                (
                    "MOD_POISON_INDIRECT".to_string(),
                    0.05,
                    "Warden".to_string()
                ),
                (
                    "MOD_TRAUMA_INDIRECT".to_string(),
                    0.05,
                    "Warden".to_string()
                ),
            ]
        );
    }

    #[test]
    fn parse_conditional_text_three_types_with_icon() {
        // Sword "of Body Damage" (SwordBoostBodyDamage): leading icon tag, comma-separated list of
        // three types, and no "is …/per tick" wording.
        let out = parse_conditional_indirect_text(
            "<icon=108>Indirect Poison Damage, Indirect Trauma Damage, and Indirect Psychic Damage +30% while Sword skill active",
        );
        assert_eq!(
            out,
            vec![
                ("MOD_POISON_INDIRECT".to_string(), 0.30, "Sword".to_string()),
                ("MOD_TRAUMA_INDIRECT".to_string(), 0.30, "Sword".to_string()),
                (
                    "MOD_PSYCHIC_INDIRECT".to_string(),
                    0.30,
                    "Sword".to_string()
                ),
            ]
        );
    }

    #[test]
    fn parse_conditional_text_no_per_tick_wording_with_icon() {
        // Cow-family variant: icon tag, two types, "damage +N%" with no "is …/per tick".
        let out = parse_conditional_indirect_text(
            "<icon=108>Indirect Nature and Indirect Trauma damage +30% while Cow skill active",
        );
        assert_eq!(
            out,
            vec![
                ("MOD_NATURE_INDIRECT".to_string(), 0.30, "Cow".to_string()),
                ("MOD_TRAUMA_INDIRECT".to_string(), 0.30, "Cow".to_string()),
            ]
        );
    }

    #[test]
    fn parse_conditional_text_ignores_unrelated_prose() {
        // Mitigation / proc prose must not be misread as a damage boost.
        assert!(
            parse_conditional_indirect_text("Max Armor +75 while Shield skill active").is_empty()
        );
        // Names indirect damage, but as a debuff on the target rather than an "Indirect <type>
        // damage +N%" grant — and the tail is temporal.
        assert!(parse_conditional_indirect_text(
            "<icon=3481>Heart Thorn causes target to suffer +17% damage from all Indirect Damage sources for 15 seconds"
        )
        .is_empty());
        assert!(parse_conditional_indirect_text(
            "<icon=108>Direct and indirect Psychic and Nature Mitigation +2 while Deer skill active"
        )
        .is_empty());
    }

    #[test]
    fn parse_dot_clause_handles_value_and_type_variants() {
        // "+N" with explicit type (AgonizeDoT family).
        assert_eq!(
            parse_dot_clause("Agonize deals +900 Psychic damage over 12 seconds"),
            Some((900.0, Some("Psychic".to_string()), 12.0))
        );
        // Bare "N" (no plus), "to health" qualifier (Giant Bat VirulentBiteBleed family).
        assert_eq!(
            parse_dot_clause(
                "<icon=3548>Virulent Bite deals 24 Trauma damage to health over 12 seconds"
            ),
            Some((24.0, Some("Trauma".to_string()), 12.0))
        );
        // "an additional N" (Knife/Archery families).
        assert_eq!(
            parse_dot_clause("Fire Arrow deals an additional 375 Fire damage over 10 seconds"),
            Some((375.0, Some("Fire".to_string()), 10.0))
        );
        // No damage type stated (FireMagic "deal +45 damage over 10 seconds").
        assert_eq!(
            parse_dot_clause("Fire Breath and Super Fireball deal +45 damage over 10 seconds"),
            Some((45.0, None, 10.0))
        );
    }

    #[test]
    fn parse_dot_clause_extracts_dot_from_compound_prose() {
        // The DoT clause is taken even when a side-effect clause trails it, and the immediate-damage
        // figure before it ("+144 direct damage plus") is skipped in favor of the over-time clause.
        assert_eq!(
            parse_dot_clause(
                "Squeal deals +144 direct damage plus 576 Trauma damage over 8 seconds"
            ),
            Some((576.0, Some("Trauma".to_string()), 8.0))
        );
        assert_eq!(
            parse_dot_clause(
                "Stunning Bash causes the target to take 318 Trauma damage over 12 seconds"
            ),
            Some((318.0, Some("Trauma".to_string()), 12.0))
        );
    }

    #[test]
    fn parse_dot_clause_rejects_non_dot_prose() {
        // %-damage and instant-only forms have no "over M seconds" clause.
        assert!(
            parse_dot_clause("Agonize deals +40% damage and conjures a magical field").is_none()
        );
        assert!(parse_dot_clause("Drink Blood steals 24 more Health over 12 seconds").is_none());
    }

    #[test]
    fn select_default_tier_key_is_deterministic_across_overlapping_ranges() {
        // Mod tier level ranges overlap: id_1 5–20, id_2 10–25, id_3 15–30, id_4 20–35.
        // At target level 20 all four ranges contain the level; the strongest tier
        // whose min_level is <= 20 is id_4. The choice must not depend on HashMap
        // iteration order (each fresh map gets a new random seed) — this is the bug
        // that made augment/mod defaults appear random across launches.
        let build = || {
            let mut t = HashMap::new();
            t.insert("id_1".to_string(), tier(5, 20));
            t.insert("id_2".to_string(), tier(10, 25));
            t.insert("id_3".to_string(), tier(15, 30));
            t.insert("id_4".to_string(), tier(20, 35));
            t
        };
        for _ in 0..50 {
            assert_eq!(
                select_default_tier_key(&build(), 20).as_deref(),
                Some("id_4")
            );
        }
    }

    #[test]
    fn select_default_tier_key_picks_strongest_qualifying_tier() {
        // ArcheryBoost-style ladder (min increments by 10). At level 90 the intended
        // default is the tier whose min_level == 90, not a lower overlapping one.
        let mut t = HashMap::new();
        t.insert("id_7".to_string(), tier(70, 90));
        t.insert("id_8".to_string(), tier(80, 100));
        t.insert("id_9".to_string(), tier(90, 110));
        t.insert("id_10".to_string(), tier(100, 120));
        assert_eq!(select_default_tier_key(&t, 90).as_deref(), Some("id_9"));
    }

    #[test]
    fn select_default_tier_key_below_all_uses_lowest_tier() {
        // Target under every tier's min_level: fall back to the lowest tier.
        let mut t = HashMap::new();
        t.insert("id_2".to_string(), tier(10, 25));
        t.insert("id_3".to_string(), tier(15, 30));
        assert_eq!(select_default_tier_key(&t, 3).as_deref(), Some("id_2"));
    }

    #[test]
    fn select_default_tier_key_above_all_uses_strongest_tier() {
        // Target above every tier's max_level: still pick the highest min_level tier.
        let mut t = HashMap::new();
        t.insert("id_1".to_string(), tier(5, 20));
        t.insert("id_2".to_string(), tier(10, 25));
        assert_eq!(select_default_tier_key(&t, 200).as_deref(), Some("id_2"));
    }

    #[test]
    fn select_default_tier_key_empty_is_none() {
        let t: HashMap<String, TsysTierInfo> = HashMap::new();
        assert!(select_default_tier_key(&t, 90).is_none());
    }
}
