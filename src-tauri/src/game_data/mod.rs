use chrono::Local;
use serde::{Deserialize, Serialize};
/// Deserialization of Project: Gorgon CDN JSON files into typed structs.
///
/// The CDN uses object maps keyed by "Item_1", "Ability_2", etc.
/// We parse those into HashMaps keyed by integer ID for fast lookup.
use std::collections::HashMap;
use std::path::Path;
use tokio::fs;

/// Timestamped log line for startup diagnostics.
macro_rules! startup_log {
    ($($arg:tt)*) => {
        eprintln!("[{}] {}", Local::now().format("%H:%M:%S%.3f"), format!($($arg)*));
    };
}

// ── Module declarations ──────────────────────────────────────────────────────
mod abilities;
pub mod ability_stats;
mod ability_dynamic;
mod ability_keywords;
mod advancement_tables;
mod ai;
mod areas;
mod attributes;
pub mod brewing;
mod directed_goals;
mod effects;
pub mod food_sources;
mod item_uses;
mod items;
mod landmarks;
mod lorebooks;
mod npcs;
mod player_titles;
mod quests;
mod recipes;
mod skills;
mod sources;
mod storage_vaults;
mod tsys;
mod xp_tables;

// ── Re-exports so cdn_commands.rs doesn't need updating ──────────────────────
pub use abilities::AbilityFamily;
pub use abilities::AbilityInfo;
pub use abilities::{CombatStats, DotEffect};
pub use areas::AreaInfo;
pub use effects::EffectInfo;
pub use item_uses::ItemUseInfo;
// Re-exported primarily for test harnesses that want to load real CDN
// `items.json` snapshots without depending on the items module being public.
// Renamed to make the call site self-documenting at use locations.
#[allow(unused_imports)]
pub use items::{parse as parse_items_json, ItemInfo, SurveyKind};
pub use lorebooks::LorebookCategoryInfo;
pub use lorebooks::LorebookEntry;
pub use npcs::NpcInfo;
// Re-exported so the gift-cap rule (`game_state.rs`'s `FavorChanged` and
// cap-note arms), the backfill's stored-entry check
// (`db::statehelm_commands.rs`), and test harnesses that seed `npcs` can reach
// them without making the whole module public — same shape as
// `parse_items_json` above. `GIFT_CAPPED_AREAS` is deliberately not
// re-exported: callers use `is_gift_capped_area`, never the list itself.
pub use npcs::{has_gift_tastes, is_gift_capped_area, parse as parse_npcs_json};
pub use player_titles::PlayerTitleInfo;
pub use quests::QuestInfo;
pub use recipes::RecipeInfo;
pub use skills::SkillInfo;
pub use sources::SourceEntry;
pub use ai::AiSummary;
pub use brewing::{BrewingIngredient, BrewingRecipe};
pub use tsys::TsysClientInfo;
pub use tsys::TsysTierInfo;
pub use xp_tables::XpTableInfo;

// ── Shared utilities ─────────────────────────────────────────────────────────

/// Parse a CDN-style map file where keys are like "Item_1", "Skill_42", etc.
/// Returns a HashMap<u32, T> keyed by the integer portion.
/// `file_name` is used only for error messages.
pub fn parse_id_map<T: for<'de> Deserialize<'de>>(
    json: &str,
    file_name: &str,
) -> Result<HashMap<u32, T>, String> {
    let raw: HashMap<String, T> = serde_json::from_str(json).map_err(|e| {
        format!(
            "{file_name}: parse error at line {}, col {}: {e}",
            e.line(),
            e.column()
        )
    })?;

    let mut out = HashMap::with_capacity(raw.len());
    let mut skipped = 0;
    for (key, value) in raw {
        // Keys are like "Item_1", "Ability_42", "Level_1" etc.
        if let Some(id_str) = key.split('_').last() {
            if let Ok(id) = id_str.parse::<u32>() {
                out.insert(id, value);
            } else {
                skipped += 1;
            }
        } else {
            skipped += 1;
        }
    }
    if skipped > 0 {
        eprintln!("{file_name}: Warning: skipped {skipped} entries with invalid keys");
    }
    Ok(out)
}

pub fn parse_string_map<T: for<'de> Deserialize<'de>>(
    json: &str,
    file_name: &str,
) -> Result<HashMap<String, T>, String> {
    serde_json::from_str(json).map_err(|e| {
        format!(
            "{file_name}: parse error at line {}, col {}: {e}",
            e.line(),
            e.column()
        )
    })
}

pub async fn read_file(path: &Path) -> Result<String, String> {
    fs::read_to_string(path)
        .await
        .map_err(|e| format!("Failed to read {}: {e}", path.display()))
}

// ── GameData struct ──────────────────────────────────────────────────────────

/// All CDN data loaded into memory. Held in Tauri managed state.
/// Built once on startup; replaced on CDN refresh.
#[allow(dead_code)]
pub struct GameData {
    pub version: u32,

    // ── Already implemented ────────────────────────────────────────────
    pub items: HashMap<u32, items::ItemInfo>,
    pub skills: HashMap<u32, skills::SkillInfo>,
    pub abilities: HashMap<u32, abilities::AbilityInfo>,
    pub recipes: HashMap<u32, recipes::RecipeInfo>,
    pub npcs: HashMap<String, npcs::NpcInfo>,

    // ── New stub fields (populated by load_from_cache) ─────────────────
    pub effects: HashMap<u32, effects::EffectInfo>,
    pub areas: HashMap<String, areas::AreaInfo>,
    pub attributes: HashMap<String, attributes::AttributeInfo>,
    pub xp_tables: HashMap<u32, xp_tables::XpTableInfo>,
    pub advancement_tables: HashMap<String, advancement_tables::AdvancementTableInfo>,
    pub ability_keywords: HashMap<String, ability_keywords::AbilityKeywordInfo>,
    pub ability_dynamic: ability_dynamic::AbilityDynamicData,
    pub ai: HashMap<String, ai::AiInfo>,
    pub directed_goals: HashMap<String, directed_goals::DirectedGoalInfo>,
    pub item_uses: HashMap<String, item_uses::ItemUseInfo>,
    pub landmarks: HashMap<String, landmarks::LandmarkInfo>,
    pub lorebooks: lorebooks::LorebookData,
    pub player_titles: HashMap<u32, player_titles::PlayerTitleInfo>,
    pub quests: HashMap<String, quests::QuestInfo>,
    pub sources: sources::SourcesData,
    pub storage_vaults: HashMap<String, storage_vaults::StorageVaultInfo>,
    pub tsys: tsys::TsysData,

    // ── Brewing (derived from recipes + items) ────────────────────────
    pub brewing_recipes: Vec<brewing::BrewingRecipe>,
    pub brewing_ingredients: Vec<brewing::BrewingIngredient>,
    pub brewing_keyword_to_items: HashMap<String, Vec<u32>>,

    // ── Cross-type indices ─────────────────────────────────────────────
    pub item_name_index: HashMap<String, u32>,
    pub item_internal_name_index: HashMap<String, u32>,
    pub skill_name_index: HashMap<String, u32>,
    pub recipes_by_skill: HashMap<String, Vec<u32>>,
    pub recipes_producing_item: HashMap<u32, Vec<u32>>,
    pub recipes_using_item: HashMap<u32, Vec<u32>>,
    pub recipe_name_index: HashMap<String, u32>,
    pub recipe_internal_name_index: HashMap<String, u32>,
    pub npcs_by_skill: HashMap<String, Vec<String>>,
    pub quest_internal_name_index: HashMap<String, String>,

    // ── Entity resolution indices ────────────────────────────────────────
    pub skill_internal_name_index: HashMap<String, u32>,
    pub npc_name_index: HashMap<String, String>,
    pub area_name_index: HashMap<String, String>,
    /// Ability display name → ability ID (for combat log resolution)
    pub ability_name_index: HashMap<String, u32>,
    /// Base internal name → AbilityFamily (groups tiers under their base ability)
    pub ability_families: HashMap<String, abilities::AbilityFamily>,
    /// Ability ID → base internal name (reverse lookup: which family does this tier belong to?)
    pub ability_to_family: HashMap<u32, String>,

    // ── Source cross-reference indices ───────────────────────────────────
    /// ability key (e.g. "ability_1002") → item IDs that bestow it
    pub items_bestowing_ability: HashMap<String, Vec<u32>>,
    /// recipe key string from BestowRecipes → item IDs that bestow it
    pub items_bestowing_recipe: HashMap<String, Vec<u32>>,
    /// quest key → item IDs that bestow it
    pub items_bestowing_quest: HashMap<String, Vec<u32>>,
    /// item key (e.g. "item_12345") → quest keys that reward it
    pub quests_rewarding_item: HashMap<String, Vec<String>>,

    // ── Cross-reference indices for data browser linking ──────────────
    /// item name (lowercase) → Vec<(npc_key, desire, pref_value)>
    pub npc_favor_by_item_name: HashMap<String, Vec<(String, String, f32)>>,
    /// item keyword → Vec<(npc_key, desire, pref_value)>
    pub npc_favor_by_keyword: HashMap<String, Vec<(String, String, f32)>>,
    /// npc key → quest keys (via FavorNpc field)
    pub quests_by_npc: HashMap<String, Vec<String>>,
    /// skill internal name → quest keys (work order quests)
    pub quests_by_work_order_skill: HashMap<String, Vec<String>>,
    /// item keyword → recipe IDs that accept this keyword as ingredient
    pub recipes_by_ingredient_keyword: HashMap<String, Vec<u32>>,
    /// NPC key → item IDs sold by that NPC (from Vendor/Barter source entries)
    pub vendor_items_by_npc: HashMap<String, Vec<u32>>,
    /// item ID → NPC keys that sell/barter it
    pub vendors_for_item: HashMap<u32, Vec<String>>,

    // ── Precomputed TSys ↔ Ability cross-reference ──────────────────────
    /// TSys key → Vec of ability IDs that this mod affects (deduped by family, highest tier)
    pub tsys_to_abilities: HashMap<String, Vec<u32>>,
    /// Ability ID → Vec of TSys keys that affect this ability
    pub ability_to_tsys: HashMap<u32, Vec<String>>,
    /// TSys internal_name → CDN key (for resolving power_name from presetMods)
    pub tsys_internal_name_index: HashMap<String, String>,
}

/// Serializable container for all derived indices. Cached to disk so they
/// only need rebuilding when the CDN version changes.
#[derive(Serialize, Deserialize)]
struct GameDataIndices {
    item_name_index: HashMap<String, u32>,
    item_internal_name_index: HashMap<String, u32>,
    skill_name_index: HashMap<String, u32>,
    recipes_by_skill: HashMap<String, Vec<u32>>,
    recipes_producing_item: HashMap<u32, Vec<u32>>,
    recipes_using_item: HashMap<u32, Vec<u32>>,
    recipe_name_index: HashMap<String, u32>,
    recipe_internal_name_index: HashMap<String, u32>,
    npcs_by_skill: HashMap<String, Vec<String>>,
    quest_internal_name_index: HashMap<String, String>,
    skill_internal_name_index: HashMap<String, u32>,
    npc_name_index: HashMap<String, String>,
    area_name_index: HashMap<String, String>,
    ability_name_index: HashMap<String, u32>,
    ability_families: HashMap<String, abilities::AbilityFamily>,
    ability_to_family: HashMap<u32, String>,
    items_bestowing_ability: HashMap<String, Vec<u32>>,
    items_bestowing_recipe: HashMap<String, Vec<u32>>,
    items_bestowing_quest: HashMap<String, Vec<u32>>,
    quests_rewarding_item: HashMap<String, Vec<String>>,
    npc_favor_by_item_name: HashMap<String, Vec<(String, String, f32)>>,
    npc_favor_by_keyword: HashMap<String, Vec<(String, String, f32)>>,
    quests_by_npc: HashMap<String, Vec<String>>,
    quests_by_work_order_skill: HashMap<String, Vec<String>>,
    recipes_by_ingredient_keyword: HashMap<String, Vec<u32>>,
    vendor_items_by_npc: HashMap<String, Vec<u32>>,
    vendors_for_item: HashMap<u32, Vec<String>>,
    tsys_to_abilities: HashMap<String, Vec<u32>>,
    ability_to_tsys: HashMap<u32, Vec<String>>,
    tsys_internal_name_index: HashMap<String, String>,
}

impl GameData {
    pub fn empty() -> Self {
        Self {
            version: 0,
            items: HashMap::new(),
            skills: HashMap::new(),
            abilities: HashMap::new(),
            recipes: HashMap::new(),
            npcs: HashMap::new(),
            effects: HashMap::new(),
            areas: HashMap::new(),
            attributes: HashMap::new(),
            xp_tables: HashMap::new(),
            advancement_tables: HashMap::new(),
            ability_keywords: HashMap::new(),
            ability_dynamic: ability_dynamic::AbilityDynamicData::empty(),
            ai: HashMap::new(),
            directed_goals: HashMap::new(),
            item_uses: HashMap::new(),
            landmarks: HashMap::new(),
            lorebooks: lorebooks::LorebookData::empty(),
            player_titles: HashMap::new(),
            quests: HashMap::new(),
            sources: sources::SourcesData::empty(),
            storage_vaults: HashMap::new(),
            tsys: tsys::TsysData::empty(),
            brewing_recipes: Vec::new(),
            brewing_ingredients: Vec::new(),
            brewing_keyword_to_items: HashMap::new(),
            item_name_index: HashMap::new(),
            item_internal_name_index: HashMap::new(),
            skill_name_index: HashMap::new(),
            recipes_by_skill: HashMap::new(),
            recipes_producing_item: HashMap::new(),
            recipes_using_item: HashMap::new(),
            recipe_name_index: HashMap::new(),
            recipe_internal_name_index: HashMap::new(),
            npcs_by_skill: HashMap::new(),
            quest_internal_name_index: HashMap::new(),
            skill_internal_name_index: HashMap::new(),
            npc_name_index: HashMap::new(),
            area_name_index: HashMap::new(),
            ability_name_index: HashMap::new(),
            ability_families: HashMap::new(),
            ability_to_family: HashMap::new(),
            items_bestowing_ability: HashMap::new(),
            items_bestowing_recipe: HashMap::new(),
            items_bestowing_quest: HashMap::new(),
            quests_rewarding_item: HashMap::new(),
            npc_favor_by_item_name: HashMap::new(),
            npc_favor_by_keyword: HashMap::new(),
            quests_by_npc: HashMap::new(),
            quests_by_work_order_skill: HashMap::new(),
            recipes_by_ingredient_keyword: HashMap::new(),
            vendor_items_by_npc: HashMap::new(),
            vendors_for_item: HashMap::new(),
            tsys_to_abilities: HashMap::new(),
            ability_to_tsys: HashMap::new(),
            tsys_internal_name_index: HashMap::new(),
        }
    }

    pub fn item_by_name(&self, name: &str) -> Option<&items::ItemInfo> {
        let id = self.item_name_index.get(name)?;
        self.items.get(id)
    }

    pub fn item_by_internal_name(&self, name: &str) -> Option<&items::ItemInfo> {
        let id = self.item_internal_name_index.get(name)?;
        self.items.get(id)
    }

    pub fn quest_by_internal_name(&self, name: &str) -> Option<&quests::QuestInfo> {
        let key = self.quest_internal_name_index.get(name)?;
        self.quests.get(key)
    }

    pub fn skill_by_name(&self, name: &str) -> Option<&skills::SkillInfo> {
        let id = self.skill_name_index.get(name)?;
        self.skills.get(id)
    }

    pub fn recipe_by_name(&self, name: &str) -> Option<&recipes::RecipeInfo> {
        let id = self.recipe_name_index.get(name)?;
        self.recipes.get(id)
    }

    // ── Unified entity resolvers ─────────────────────────────────────────
    // Each accepts any known reference form and returns the entity.

    /// Resolve any string reference to an ItemInfo.
    /// Tries: numeric ID → display name → internal name → TSys prefix/suffix stripping.
    pub fn resolve_item(&self, reference: &str) -> Option<&items::ItemInfo> {
        if let Ok(id) = reference.parse::<u32>() {
            if let Some(item) = self.items.get(&id) {
                return Some(item);
            }
        }
        if let Some(item) = self.item_by_name(reference) {
            return Some(item);
        }
        if let Some(item) = self.item_by_internal_name(reference) {
            return Some(item);
        }
        // Fallback: try stripping known TSys mod prefixes/suffixes from the name.
        // Items with mods appear as e.g. "Amazing Iron Sword" but CDN only has "Iron Sword".
        self.resolve_item_with_mod_stripping(reference)
    }

    /// Try to resolve an item by stripping known TSys prefixes and suffixes.
    fn resolve_item_with_mod_stripping(&self, reference: &str) -> Option<&items::ItemInfo> {
        for info in self.tsys.client_info.values() {
            if let Some(prefix) = &info.prefix {
                if let Some(stripped) = reference.strip_prefix(prefix.as_str()) {
                    let stripped = stripped.trim_start();
                    if !stripped.is_empty() {
                        if let Some(item) = self.item_by_name(stripped) {
                            return Some(item);
                        }
                    }
                }
            }
            if let Some(suffix) = &info.suffix {
                if let Some(stripped) = reference.strip_suffix(suffix.as_str()) {
                    let stripped = stripped.trim_end();
                    if !stripped.is_empty() {
                        if let Some(item) = self.item_by_name(stripped) {
                            return Some(item);
                        }
                    }
                }
            }
        }
        None
    }

    /// Find the base equipment name within a potentially-modified item name.
    ///
    /// Crafted items include TSys prefixes and suffixes (e.g. "Hailin' Thorian
    /// Kilt of Deathspark") but hoplology only cares about the base item
    /// ("Thorian Kilt"). This performs longest-first case-insensitive substring
    /// matching against all known equipment item names (items with an equip_slot).
    pub fn find_equipment_base_name(&self, crafted_name: &str) -> Option<String> {
        let lower = crafted_name.to_lowercase();

        // Build a sorted list of (lowercase_name, original_name) for equipment items.
        // Sorted by descending length so longest match wins.
        let mut equipment_names: Vec<(&str, &str)> = self
            .items
            .values()
            .filter(|item| item.equip_slot.is_some())
            .map(|item| (item.name.as_str(), item.name.as_str()))
            .collect();
        equipment_names.sort_by(|a, b| b.0.len().cmp(&a.0.len()));

        for (_, name) in &equipment_names {
            if lower.contains(&name.to_lowercase()) {
                return Some(name.to_string());
            }
        }
        None
    }

    /// Resolve any string reference to a SkillInfo.
    /// Tries: numeric ID → display name → internal name.
    pub fn resolve_skill(&self, reference: &str) -> Option<&skills::SkillInfo> {
        if let Ok(id) = reference.parse::<u32>() {
            if let Some(skill) = self.skills.get(&id) {
                return Some(skill);
            }
        }
        if let Some(skill) = self.skill_by_name(reference) {
            return Some(skill);
        }
        let id = self.skill_internal_name_index.get(reference)?;
        self.skills.get(id)
    }

    /// Resolve any string reference to a RecipeInfo.
    /// Tries: numeric ID → display name → internal name.
    pub fn resolve_recipe(&self, reference: &str) -> Option<&recipes::RecipeInfo> {
        if let Ok(id) = reference.parse::<u32>() {
            if let Some(recipe) = self.recipes.get(&id) {
                return Some(recipe);
            }
        }
        if let Some(recipe) = self.recipe_by_name(reference) {
            return Some(recipe);
        }
        let id = self.recipe_internal_name_index.get(reference)?;
        self.recipes.get(id)
    }

    /// Resolve any string reference to a QuestInfo.
    /// Tries: CDN key → internal name.
    pub fn resolve_quest(&self, reference: &str) -> Option<&quests::QuestInfo> {
        if let Some(quest) = self.quests.get(reference) {
            return Some(quest);
        }
        self.quest_by_internal_name(reference)
    }

    /// Resolve any string reference to an NpcInfo.
    /// Tries: CDN key → display name.
    pub fn resolve_npc(&self, reference: &str) -> Option<&npcs::NpcInfo> {
        if let Some(npc) = self.npcs.get(reference) {
            return Some(npc);
        }
        let key = self.npc_name_index.get(reference)?;
        self.npcs.get(key)
    }

    /// Resolve any string reference to an AreaInfo.
    /// Tries: CDN key → friendly name → short friendly name.
    pub fn resolve_area(&self, reference: &str) -> Option<&areas::AreaInfo> {
        if let Some(area) = self.areas.get(reference) {
            return Some(area);
        }
        let key = self.area_name_index.get(reference)?;
        self.areas.get(key)
    }

    /// Resolve any string reference to an AbilityInfo.
    /// Tries: numeric ID → display name.
    pub fn resolve_ability(&self, reference: &str) -> Option<&abilities::AbilityInfo> {
        if let Ok(id) = reference.parse::<u32>() {
            if let Some(ability) = self.abilities.get(&id) {
                return Some(ability);
            }
        }
        let id = self.ability_name_index.get(reference)?;
        self.abilities.get(id)
    }
}

// ── Loading from disk ─────────────────────────────────────────────────────────

/// Load all cached JSON files from disk into a `GameData` instance.
pub async fn load_from_cache(cache_dir: &Path, version: u32) -> Result<GameData, String> {
    load_from_cache_with_progress(cache_dir, version, |_| {}).await
}

/// Load cached JSON files with a progress callback for sub-status updates.
/// The callback fires between async boundaries so events can be delivered.
pub async fn load_from_cache_with_progress(
    cache_dir: &Path,
    version: u32,
    on_progress: impl Fn(&str),
) -> Result<GameData, String> {
    on_progress("Reading cached files...");

    // Read all existing files
    let items_json = read_file(&cache_dir.join("items.json")).await?;
    let skills_json = read_file(&cache_dir.join("skills.json")).await?;
    let abilities_json = read_file(&cache_dir.join("abilities.json")).await?;
    let recipes_json = read_file(&cache_dir.join("recipes.json")).await?;

    // Read new stub files with graceful fallback
    let npcs_json = read_file(&cache_dir.join("npcs.json"))
        .await
        .unwrap_or_else(|e| {
            eprintln!("Warning: {e}");
            String::from("{}")
        });
    let effects_json = read_file(&cache_dir.join("effects.json"))
        .await
        .unwrap_or_else(|e| {
            eprintln!("Warning: {e}");
            String::from("{}")
        });
    let areas_json = read_file(&cache_dir.join("areas.json"))
        .await
        .unwrap_or_else(|e| {
            eprintln!("Warning: {e}");
            String::from("{}")
        });
    let attributes_json = read_file(&cache_dir.join("attributes.json"))
        .await
        .unwrap_or_else(|e| {
            eprintln!("Warning: {e}");
            String::from("{}")
        });
    let xp_tables_json = read_file(&cache_dir.join("xptables.json"))
        .await
        .unwrap_or_else(|e| {
            eprintln!("Warning: {e}");
            String::from("{}")
        });
    let advancement_tables_json = read_file(&cache_dir.join("advancementtables.json"))
        .await
        .unwrap_or_else(|e| {
            eprintln!("Warning: {e}");
            String::from("{}")
        });
    let ability_keywords_json = read_file(&cache_dir.join("abilitykeywords.json"))
        .await
        .unwrap_or_else(|e| {
            eprintln!("Warning: {e}");
            String::from("{}")
        });
    let ability_dynamic_dots_json = read_file(&cache_dir.join("abilitydynamicdots.json"))
        .await
        .unwrap_or_else(|e| {
            eprintln!("Warning: {e}");
            String::from("{}")
        });
    let ability_dynamic_special_json =
        read_file(&cache_dir.join("abilitydynamicspecialvalues.json"))
            .await
            .unwrap_or_else(|e| {
                eprintln!("Warning: {e}");
                String::from("{}")
            });
    let ai_json = read_file(&cache_dir.join("ai.json"))
        .await
        .unwrap_or_else(|e| {
            eprintln!("Warning: {e}");
            String::from("{}")
        });
    let directed_goals_json = read_file(&cache_dir.join("directedgoals.json"))
        .await
        .unwrap_or_else(|e| {
            eprintln!("Warning: {e}");
            String::from("{}")
        });
    let item_uses_json = read_file(&cache_dir.join("itemuses.json"))
        .await
        .unwrap_or_else(|e| {
            eprintln!("Warning: {e}");
            String::from("{}")
        });
    let landmarks_json = read_file(&cache_dir.join("landmarks.json"))
        .await
        .unwrap_or_else(|e| {
            eprintln!("Warning: {e}");
            String::from("{}")
        });
    let lorebooks_json = read_file(&cache_dir.join("lorebooks.json"))
        .await
        .unwrap_or_else(|e| {
            eprintln!("Warning: {e}");
            String::from("{}")
        });
    let lorebook_info_json = read_file(&cache_dir.join("lorebookinfo.json"))
        .await
        .unwrap_or_else(|e| {
            eprintln!("Warning: {e}");
            String::from("{}")
        });
    let player_titles_json = read_file(&cache_dir.join("playertitles.json"))
        .await
        .unwrap_or_else(|e| {
            eprintln!("Warning: {e}");
            String::from("{}")
        });
    let quests_json = read_file(&cache_dir.join("quests.json"))
        .await
        .unwrap_or_else(|e| {
            eprintln!("Warning: {e}");
            String::from("{}")
        });
    let sources_abilities_json = read_file(&cache_dir.join("sources_abilities.json"))
        .await
        .unwrap_or_else(|e| {
            eprintln!("Warning: {e}");
            String::from("{}")
        });
    let sources_items_json = read_file(&cache_dir.join("sources_items.json"))
        .await
        .unwrap_or_else(|e| {
            eprintln!("Warning: {e}");
            String::from("{}")
        });
    let sources_recipes_json = read_file(&cache_dir.join("sources_recipes.json"))
        .await
        .unwrap_or_else(|e| {
            eprintln!("Warning: {e}");
            String::from("{}")
        });
    let storage_vaults_json = read_file(&cache_dir.join("storagevaults.json"))
        .await
        .unwrap_or_else(|e| {
            eprintln!("Warning: {e}");
            String::from("{}")
        });
    let tsys_client_info_json = read_file(&cache_dir.join("tsysclientinfo.json"))
        .await
        .unwrap_or_else(|e| {
            eprintln!("Warning: {e}");
            String::from("{}")
        });
    let tsys_profiles_json = read_file(&cache_dir.join("tsysprofiles.json"))
        .await
        .unwrap_or_else(|e| {
            eprintln!("Warning: {e}");
            String::from("{}")
        });

    // Try to load cached indices
    let indices_path = cache_dir.join("indices.json");
    let cached_indices: Option<GameDataIndices> = match fs::read_to_string(&indices_path).await {
        Ok(json) => match serde_json::from_str(&json) {
            Ok(idx) => {
                startup_log!("Loaded cached indices from disk");
                Some(idx)
            }
            Err(e) => {
                startup_log!("Cached indices invalid, will rebuild: {e}");
                None
            }
        },
        Err(_) => None,
    };

    let has_cached_indices = cached_indices.is_some();

    // Parse all data + optionally build indices in a blocking thread
    // so we don't starve the async runtime (and events can be delivered).
    if has_cached_indices {
        on_progress("Parsing game data...");
    } else {
        on_progress("Parsing and indexing game data (one-time setup)...");
    }

    let indices_path_owned = indices_path.clone();
    let game_data = tokio::task::spawn_blocking(move || -> Result<GameData, String> {
        startup_log!("Parsing game data files...");

        let items = items::parse(&items_json)?;
        startup_log!("  items: {} entries", items.len());

        let skills = skills::parse(&skills_json)?;
        startup_log!("  skills: {} entries", skills.len());

        let abilities = abilities::parse(&abilities_json)?;
        startup_log!("  abilities: {} entries", abilities.len());

        let recipes = recipes::parse(&recipes_json)?;
        startup_log!("  recipes: {} entries", recipes.len());

        let npcs = npcs::parse(&npcs_json)?;
        startup_log!("  npcs: {} entries", npcs.len());

        let effects = effects::parse(&effects_json)?;
        startup_log!("  effects: {} entries", effects.len());

        let areas = areas::parse(&areas_json)?;
        startup_log!("  areas: {} entries", areas.len());

        let attributes = attributes::parse(&attributes_json)?;
        startup_log!("  attributes: {} entries", attributes.len());

        let xp_tables = xp_tables::parse(&xp_tables_json)?;
        startup_log!("  xp_tables: {} entries", xp_tables.len());

        let advancement_tables = advancement_tables::parse(&advancement_tables_json)?;
        startup_log!("  advancement_tables: {} entries", advancement_tables.len());

        let ability_keywords = ability_keywords::parse(&ability_keywords_json)?;
        startup_log!("  ability_keywords: {} entries", ability_keywords.len());

        let ability_dynamic = ability_dynamic::AbilityDynamicData::parse(
            &ability_dynamic_dots_json,
            &ability_dynamic_special_json,
        )?;
        startup_log!("  ability_dynamic: parsed");

        let ai = ai::parse(&ai_json)?;
        startup_log!("  ai: {} entries", ai.len());

        let directed_goals = directed_goals::parse(&directed_goals_json)?;
        startup_log!("  directed_goals: {} entries", directed_goals.len());

        let item_uses = item_uses::parse(&item_uses_json)?;
        startup_log!("  item_uses: {} entries", item_uses.len());

        let landmarks = landmarks::parse(&landmarks_json)?;
        startup_log!("  landmarks: {} entries", landmarks.len());

        let lorebooks = lorebooks::LorebookData::parse(&lorebooks_json, &lorebook_info_json)?;
        startup_log!("  lorebooks: {} books, {} categories", lorebooks.books.len(), lorebooks.categories.len());

        let player_titles = player_titles::parse(&player_titles_json)?;
        startup_log!("  player_titles: {} entries", player_titles.len());

        let quests = quests::parse(&quests_json)?;
        startup_log!("  quests: {} entries", quests.len());

        let sources = sources::SourcesData::parse(
            &sources_abilities_json,
            &sources_items_json,
            &sources_recipes_json,
        )?;
        startup_log!("  sources: parsed");

        let storage_vaults = storage_vaults::parse(&storage_vaults_json)?;
        startup_log!("  storage_vaults: {} entries", storage_vaults.len());

        let tsys = tsys::TsysData::parse(&tsys_client_info_json, &tsys_profiles_json)?;
        startup_log!("  tsys: parsed");

        // Use cached indices if available, otherwise build them
        let indices = if let Some(idx) = cached_indices {
            startup_log!("Using cached indices (skipping index build)");
            idx
        } else {
            startup_log!("Building indices from scratch...");
            let idx = build_all_indices(
                &items, &skills, &abilities, &recipes, &npcs, &quests,
                &sources, &areas, &tsys,
            );
            // Save indices to disk for next startup
            match serde_json::to_string(&idx) {
                Ok(json) => {
                    if let Err(e) = std::fs::write(&indices_path_owned, json) {
                        startup_log!("Failed to cache indices: {e}");
                    } else {
                        startup_log!("Indices cached to disk");
                    }
                }
                Err(e) => { startup_log!("Failed to serialize indices: {e}"); }
            }
            idx
        };

        // Build brewing data (derived from recipes + items)
        let (brewing_recipes, brewing_ingredients, brewing_keyword_to_items) =
            brewing::build_brewing_data(&recipes, &items);
        startup_log!("  brewing: {} recipes, {} ingredients", brewing_recipes.len(), brewing_ingredients.len());

        Ok(GameData {
            version,
            items,
            skills,
            abilities,
            recipes,
            npcs,
            effects,
            areas,
            attributes,
            xp_tables,
            advancement_tables,
            ability_keywords,
            ability_dynamic,
            ai,
            directed_goals,
            item_uses,
            landmarks,
            lorebooks,
            player_titles,
            quests,
            sources,
            storage_vaults,
            tsys,
            brewing_recipes,
            brewing_ingredients,
            brewing_keyword_to_items,
            item_name_index: indices.item_name_index,
            item_internal_name_index: indices.item_internal_name_index,
            skill_name_index: indices.skill_name_index,
            recipes_by_skill: indices.recipes_by_skill,
            recipes_producing_item: indices.recipes_producing_item,
            recipes_using_item: indices.recipes_using_item,
            recipe_name_index: indices.recipe_name_index,
            recipe_internal_name_index: indices.recipe_internal_name_index,
            npcs_by_skill: indices.npcs_by_skill,
            quest_internal_name_index: indices.quest_internal_name_index,
            skill_internal_name_index: indices.skill_internal_name_index,
            npc_name_index: indices.npc_name_index,
            area_name_index: indices.area_name_index,
            ability_name_index: indices.ability_name_index,
            ability_families: indices.ability_families,
            ability_to_family: indices.ability_to_family,
            items_bestowing_ability: indices.items_bestowing_ability,
            items_bestowing_recipe: indices.items_bestowing_recipe,
            items_bestowing_quest: indices.items_bestowing_quest,
            quests_rewarding_item: indices.quests_rewarding_item,
            npc_favor_by_item_name: indices.npc_favor_by_item_name,
            npc_favor_by_keyword: indices.npc_favor_by_keyword,
            quests_by_npc: indices.quests_by_npc,
            quests_by_work_order_skill: indices.quests_by_work_order_skill,
            recipes_by_ingredient_keyword: indices.recipes_by_ingredient_keyword,
            vendor_items_by_npc: indices.vendor_items_by_npc,
            vendors_for_item: indices.vendors_for_item,
            tsys_to_abilities: indices.tsys_to_abilities,
            ability_to_tsys: indices.ability_to_tsys,
            tsys_internal_name_index: indices.tsys_internal_name_index,
        })
    }).await.map_err(|e| format!("Parsing task panicked: {e}"))??;

    if !has_cached_indices {
        on_progress("Indices built and cached");
    }

    Ok(game_data)
}

// ── Index builder ──────────────────────────────────────────────────────────

/// Build the display-name → ability-id index used to resolve ability names in combat logs and
/// in prose gear-mod descs ("Withering Shroud deals 164 Psychic damage over 8 seconds").
///
/// Monster copies of player abilities share display names (the Orc/Ratkin "Wave of Darkness",
/// Vampirism's "Withering Shroud" monster shroud), so a blind name → id collect can resolve a
/// prose mod to the wrong family and silently drop it. Two rules fix that:
/// 1. Player abilities win name collisions with `Lint_MonsterAbility` abilities.
/// 2. Each player family's tier-stripped base name points at its first tier when no player
///    ability owns the bare name (Vampirism's tiers run "Withering Shroud 1"–"13", so the bare
///    name would otherwise resolve only to the monster ability).
fn build_ability_name_index(
    abilities: &HashMap<u32, abilities::AbilityInfo>,
    ability_families: &HashMap<String, abilities::AbilityFamily>,
) -> HashMap<String, u32> {
    let is_monster = |a: &abilities::AbilityInfo| {
        a.keywords.iter().any(|k| k == "Lint_MonsterAbility")
    };
    let mut index: HashMap<String, u32> = HashMap::new();
    for (id, ability) in abilities {
        match index.entry(ability.name.clone()) {
            std::collections::hash_map::Entry::Vacant(e) => {
                e.insert(*id);
            }
            std::collections::hash_map::Entry::Occupied(mut e) => {
                let existing_is_monster = abilities.get(e.get()).map(is_monster).unwrap_or(true);
                if existing_is_monster && !is_monster(ability) {
                    e.insert(*id);
                }
            }
        }
    }

    for family in ability_families.values() {
        if family.is_monster_ability {
            continue;
        }
        let Some(&first_id) = family.tier_ids.first() else { continue };
        let claimable = match index.get(&family.base_name) {
            None => true,
            Some(existing) => abilities.get(existing).map(is_monster).unwrap_or(true),
        };
        if claimable {
            index.insert(family.base_name.clone(), first_id);
        }
    }
    index
}

/// Display name → NPC key, with a deterministic pick when several NPCs share a
/// display name (the CDN does: eight "Work Orders", for example).
///
/// `npcs` is a `HashMap`, so a plain `.collect()` picks whichever duplicate the
/// hash order happened to visit last — arbitrary per run. Keep the lowest key,
/// which is also what the DB-backed resolver does
/// (`SELECT key FROM npcs WHERE name = ?1 ORDER BY key LIMIT 1` in
/// `db::statehelm_commands`). Those two resolve names for the *same* line (the
/// live `GiftCountObserved` arm and the log backfill), and the gift tracker
/// keys rows by NPC key, so a disagreement would split one NPC's weekly count
/// across two rows.
pub(crate) fn build_npc_name_index<'a, I>(npcs: I) -> HashMap<String, String>
where
    I: IntoIterator<Item = (&'a String, &'a String)>,
{
    let mut index: HashMap<String, String> = HashMap::new();
    for (key, name) in npcs {
        match index.get(name) {
            Some(existing) if existing.as_str() <= key.as_str() => {}
            _ => {
                index.insert(name.clone(), key.clone());
            }
        }
    }
    index
}

/// Build all derived indices from parsed game data.
fn build_all_indices(
    items: &HashMap<u32, items::ItemInfo>,
    skills: &HashMap<u32, skills::SkillInfo>,
    abilities: &HashMap<u32, abilities::AbilityInfo>,
    recipes: &HashMap<u32, recipes::RecipeInfo>,
    npcs: &HashMap<String, npcs::NpcInfo>,
    quests: &HashMap<String, quests::QuestInfo>,
    sources: &sources::SourcesData,
    areas: &HashMap<String, areas::AreaInfo>,
    tsys: &tsys::TsysData,
) -> GameDataIndices {
    let item_name_index: HashMap<String, u32> = items
        .iter()
        .map(|(id, item)| (item.name.clone(), *id))
        .collect();

    let item_internal_name_index: HashMap<String, u32> = items
        .iter()
        .filter_map(|(id, item)| item.internal_name.as_ref().map(|n| (n.clone(), *id)))
        .collect();

    let skill_name_index: HashMap<String, u32> = skills
        .iter()
        .map(|(id, skill)| (skill.name.clone(), *id))
        .collect();

    // Map internal skill names (e.g. "JewelryCrafting") to display names (e.g. "Jewelry Crafting")
    let skill_internal_to_display: HashMap<String, String> = skills
        .values()
        .map(|skill| (skill.internal_name.clone(), skill.name.clone()))
        .collect();

    let mut recipes_by_skill: HashMap<String, Vec<u32>> = HashMap::new();
    let mut recipes_producing_item: HashMap<u32, Vec<u32>> = HashMap::new();
    let mut recipes_using_item: HashMap<u32, Vec<u32>> = HashMap::new();

    for (recipe_id, recipe) in recipes {
        if let Some(skill) = &recipe.skill {
            // Use display name if available, fall back to raw skill name
            let display_name = skill_internal_to_display
                .get(skill.as_str())
                .cloned()
                .unwrap_or_else(|| skill.clone());
            recipes_by_skill
                .entry(display_name)
                .or_default()
                .push(*recipe_id);
        }
        for item_id in &recipe.result_item_ids {
            recipes_producing_item
                .entry(*item_id)
                .or_default()
                .push(*recipe_id);
        }
        for item_id in &recipe.ingredient_item_ids {
            recipes_using_item
                .entry(*item_id)
                .or_default()
                .push(*recipe_id);
        }
    }

    let recipe_name_index: HashMap<String, u32> = recipes
        .iter()
        .map(|(id, recipe)| (recipe.name.clone(), *id))
        .collect();

    let recipe_internal_name_index: HashMap<String, u32> = recipes
        .iter()
        .filter_map(|(id, recipe)| {
            recipe
                .internal_name
                .as_ref()
                .map(|iname| (iname.clone(), *id))
        })
        .collect();

    let mut npcs_by_skill: HashMap<String, Vec<String>> = HashMap::new();
    for (npc_key, npc) in npcs {
        for skill in &npc.trains_skills {
            npcs_by_skill
                .entry(skill.clone())
                .or_default()
                .push(npc_key.clone());
        }
    }

    // Build quest internal name → key index
    let mut quest_internal_name_index: HashMap<String, String> = HashMap::new();
    for (quest_key, quest) in quests {
        if !quest.internal_name.is_empty() {
            quest_internal_name_index.insert(quest.internal_name.clone(), quest_key.clone());
        }
    }

    // Build entity resolution indices
    let skill_internal_name_index: HashMap<String, u32> = skills
        .iter()
        .map(|(id, skill)| (skill.internal_name.clone(), *id))
        .collect();

    let npc_name_index = build_npc_name_index(npcs.iter().map(|(key, npc)| (key, &npc.name)));

    let mut area_name_index: HashMap<String, String> = HashMap::new();
    for (key, area) in areas {
        if let Some(ref name) = area.friendly_name {
            area_name_index.insert(name.clone(), key.clone());
        }
        if let Some(ref name) = area.short_friendly_name {
            area_name_index.insert(name.clone(), key.clone());
        }
    }

    // Build source cross-reference indices
    let mut items_bestowing_ability: HashMap<String, Vec<u32>> = HashMap::new();
    let mut items_bestowing_recipe: HashMap<String, Vec<u32>> = HashMap::new();
    let mut items_bestowing_quest: HashMap<String, Vec<u32>> = HashMap::new();

    for (item_id, item) in items {
        if let Some(ref ability_key) = item.bestow_ability {
            items_bestowing_ability
                .entry(ability_key.clone())
                .or_default()
                .push(*item_id);
        }
        if let Some(ref quest_key) = item.bestow_quest {
            items_bestowing_quest
                .entry(quest_key.clone())
                .or_default()
                .push(*item_id);
        }
        if let Some(ref bestow_recipes) = item.bestow_recipes {
            for recipe_val in bestow_recipes {
                // BestowRecipes entries can be strings like "recipe_1234"
                if let Some(recipe_key) = recipe_val.as_str() {
                    items_bestowing_recipe
                        .entry(recipe_key.to_string())
                        .or_default()
                        .push(*item_id);
                }
            }
        }
    }

    let mut quests_rewarding_item: HashMap<String, Vec<String>> = HashMap::new();
    for (quest_key, quest) in quests {
        if let Some(reward_items) = quest.raw.get("Rewards_Items").and_then(|v| v.as_array()) {
            for reward in reward_items {
                if let Some(item_key) = reward.get("Item").and_then(|v| v.as_str()) {
                    quests_rewarding_item
                        .entry(item_key.to_string())
                        .or_default()
                        .push(quest_key.clone());
                }
            }
        }
    }

    // Build NPC favor reverse indices: item name → NPCs, keyword → NPCs
    let mut npc_favor_by_item_name: HashMap<String, Vec<(String, String, f32)>> = HashMap::new();
    let mut npc_favor_by_keyword: HashMap<String, Vec<(String, String, f32)>> = HashMap::new();
    for (npc_key, npc) in npcs {
        for pref in &npc.preferences {
            let entry = (npc_key.clone(), pref.desire.clone(), pref.pref);
            if let Some(ref name) = pref.name {
                npc_favor_by_item_name
                    .entry(name.to_lowercase())
                    .or_default()
                    .push(entry.clone());
            }
            for keyword in &pref.keywords {
                npc_favor_by_keyword
                    .entry(keyword.clone())
                    .or_default()
                    .push(entry.clone());
            }
        }
    }

    // Build quest reverse indices: NPC → quests, skill → work order quests
    let mut quests_by_npc: HashMap<String, Vec<String>> = HashMap::new();
    let mut quests_by_work_order_skill: HashMap<String, Vec<String>> = HashMap::new();
    for (quest_key, quest) in quests {
        if let Some(favor_npc) = quest.raw.get("FavorNpc").and_then(|v| v.as_str()) {
            // FavorNpc is a path like "AreaName/NPC_Foo" — extract the NPC key
            if let Some(npc_key) = favor_npc.split('/').last() {
                quests_by_npc
                    .entry(npc_key.to_string())
                    .or_default()
                    .push(quest_key.clone());
            }
        }
        if let Some(skill) = quest.raw.get("WorkOrderSkill").and_then(|v| v.as_str()) {
            quests_by_work_order_skill
                .entry(skill.to_string())
                .or_default()
                .push(quest_key.clone());
        }
    }

    // Build recipe keyword ingredient index: keyword → recipe IDs
    let mut recipes_by_ingredient_keyword: HashMap<String, Vec<u32>> = HashMap::new();
    for (recipe_id, recipe) in recipes {
        for ingredient in &recipe.ingredients {
            if ingredient.item_id.is_none() {
                for keyword in &ingredient.item_keys {
                    recipes_by_ingredient_keyword
                        .entry(keyword.clone())
                        .or_default()
                        .push(*recipe_id);
                }
            }
        }
    }

    // Build NPC vendor inventory index: NPC key → item IDs (from Vendor/Barter sources)
    let mut vendor_items_by_npc: HashMap<String, Vec<u32>> = HashMap::new();
    for (&item_id, source_info) in &sources.items {
        for entry in &source_info.entries {
            if entry.source_type == "Vendor" || entry.source_type == "Barter" {
                if let Some(ref npc_key) = entry.npc {
                    vendor_items_by_npc
                        .entry(npc_key.clone())
                        .or_default()
                        .push(item_id);
                }
            }
        }
    }
    // Sort each NPC's item list for consistent ordering
    for items in vendor_items_by_npc.values_mut() {
        items.sort_unstable();
        items.dedup();
    }

    // Build reverse index: item ID → NPC keys that sell it
    let mut vendors_for_item: HashMap<u32, Vec<String>> = HashMap::new();
    for (npc_key, item_ids) in &vendor_items_by_npc {
        for &item_id in item_ids {
            vendors_for_item
                .entry(item_id)
                .or_default()
                .push(npc_key.clone());
        }
    }
    for npc_keys in vendors_for_item.values_mut() {
        npc_keys.sort();
        npc_keys.dedup();
    }

    // ── Build ability family index ──────────────────────────────────────
    // Map internal_name → ability_id for lookup
    let ability_internal_name_map: HashMap<&str, u32> = abilities
        .iter()
        .filter_map(|(id, a)| a.internal_name.as_deref().map(|n| (n, *id)))
        .collect();

    // Collect which base internal names have upgrades pointing to them.
    // Exclude monster-only abilities (Lint_MonsterAbility) so they don't get
    // pulled into player families — they'd skew tier numbering and metadata.
    let mut family_tiers: HashMap<String, Vec<u32>> = HashMap::new();
    for (id, ability) in abilities {
        if let Some(ref upgrade_of) = ability.upgrade_of {
            let is_monster = ability.keywords.iter().any(|k| k == "Lint_MonsterAbility");
            if !is_monster {
                family_tiers
                    .entry(upgrade_of.clone())
                    .or_default()
                    .push(*id);
            }
        }
    }

    let mut ability_families: HashMap<String, abilities::AbilityFamily> = HashMap::new();
    let mut ability_to_family: HashMap<u32, String> = HashMap::new();

    // Build families for abilities that have upgrades
    for (base_internal_name, mut tier_ids) in family_tiers {
        // Add the base ability itself
        if let Some(&base_id) = ability_internal_name_map.get(base_internal_name.as_str()) {
            if !tier_ids.contains(&base_id) {
                tier_ids.push(base_id);
            }
        }

        // Sort tiers by level ascending
        tier_ids.sort_by(|a, b| {
            let level_a = abilities.get(a).and_then(|ab| ab.level).unwrap_or(0.0);
            let level_b = abilities.get(b).and_then(|ab| ab.level).unwrap_or(0.0);
            level_a
                .partial_cmp(&level_b)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        // Get base ability info for shared properties
        let base_ability = tier_ids
            .first()
            .and_then(|id| abilities.get(id));

        if let Some(base) = base_ability {
            let base_name = base
                .name
                .trim_end_matches(|c: char| c.is_ascii_digit())
                .trim()
                .to_string();

            // Mark as monster ability if ALL tiers have the tag.
            let is_monster_ability = tier_ids.iter().all(|tid| {
                abilities
                    .get(tid)
                    .map(|a| a.keywords.iter().any(|k| k == "Lint_MonsterAbility"))
                    .unwrap_or(false)
            });

            let family = abilities::AbilityFamily {
                base_internal_name: base_internal_name.clone(),
                base_name,
                icon_id: base.icon_id,
                skill: base.skill.clone(),
                damage_type: base.damage_type.clone(),
                is_monster_ability,
                tier_ids: tier_ids.clone(),
            };

            for &tid in &tier_ids {
                ability_to_family.insert(tid, base_internal_name.clone());
            }

            ability_families.insert(base_internal_name, family);
        }
    }

    // Create single-tier families for standalone abilities (no upgrade_of, nothing upgrades to them)
    for (id, ability) in abilities {
        if !ability_to_family.contains_key(id) {
            let internal_name = ability
                .internal_name
                .clone()
                .unwrap_or_else(|| format!("ability_{id}"));

            let base_name = ability
                .name
                .trim_end_matches(|c: char| c.is_ascii_digit())
                .trim()
                .to_string();

            let is_monster_ability = ability.keywords.iter().any(|k| k == "Lint_MonsterAbility");

            let family = abilities::AbilityFamily {
                base_internal_name: internal_name.clone(),
                base_name,
                icon_id: ability.icon_id,
                skill: ability.skill.clone(),
                damage_type: ability.damage_type.clone(),
                is_monster_ability,
                tier_ids: vec![*id],
            };

            ability_to_family.insert(*id, internal_name.clone());
            ability_families.insert(internal_name, family);
        }
    }

    let ability_name_index = build_ability_name_index(abilities, &ability_families);

    startup_log!(
        "Ability families built: {} families from {} abilities",
        ability_families.len(),
        abilities.len()
    );

    // ── Build TSys internal_name → CDN key index ─────────────────────────
    let mut tsys_internal_name_index: HashMap<String, String> = HashMap::new();
    for (key, info) in &tsys.client_info {
        if let Some(name) = &info.internal_name {
            tsys_internal_name_index.insert(name.clone(), key.clone());
        }
    }

    // ── Build TSys ↔ Ability cross-reference index ──────────────────────
    let (tsys_to_abilities, ability_to_tsys) = build_tsys_ability_index(
        tsys, abilities, &ability_families, &ability_to_family,
    );
    startup_log!(
        "TSys↔Ability index built: {} TSys entries mapped, {} abilities mapped",
        tsys_to_abilities.len(),
        ability_to_tsys.len()
    );

    startup_log!("Game data indices built");

    GameDataIndices {
        item_name_index,
        item_internal_name_index,
        skill_name_index,
        recipes_by_skill,
        recipes_producing_item,
        recipes_using_item,
        recipe_name_index,
        recipe_internal_name_index,
        npcs_by_skill,
        quest_internal_name_index,
        skill_internal_name_index,
        npc_name_index,
        area_name_index,
        ability_name_index,
        ability_families,
        ability_to_family,
        items_bestowing_ability,
        items_bestowing_recipe,
        items_bestowing_quest,
        quests_rewarding_item,
        npc_favor_by_item_name,
        npc_favor_by_keyword,
        quests_by_npc,
        quests_by_work_order_skill,
        recipes_by_ingredient_keyword,
        vendor_items_by_npc,
        vendors_for_item,
        tsys_to_abilities,
        ability_to_tsys,
        tsys_internal_name_index,
    }
}

// ── TSys ↔ Ability index builder ────────────────────────────────────────────

/// Extract `{TOKEN}` keys from effect_desc strings (format: `{TOKEN}{VALUE}`).
fn extract_effect_tokens(effect_descs: &[String]) -> Vec<String> {
    let mut tokens = Vec::new();
    for desc in effect_descs {
        if !desc.starts_with('{') { continue; }
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
        if desc.starts_with('{') { continue; }
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

/// Strip `<icon=XXXX>` tags from text.
fn strip_icon_tags(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let mut remaining = text;
    while let Some(start) = remaining.find("<icon=") {
        result.push_str(&remaining[..start]);
        if let Some(end) = remaining[start..].find('>') {
            remaining = &remaining[start + end + 1..];
        } else {
            remaining = &remaining[start + 6..];
        }
    }
    result.push_str(remaining);
    result
}

/// Collect all attribute tokens from an ability's PvE/PvP combat stats.
fn collect_ability_attribute_tokens(ability: &abilities::AbilityInfo) -> std::collections::HashSet<String> {
    let mut tokens = std::collections::HashSet::new();
    for stats in [&ability.pve, &ability.pvp].into_iter().flatten() {
        for t in &stats.attributes_that_delta_damage { tokens.insert(t.clone()); }
        for t in &stats.attributes_that_mod_base_damage { tokens.insert(t.clone()); }
        for t in &stats.attributes_that_mod_damage { tokens.insert(t.clone()); }
        for t in &stats.attributes_that_mod_crit_damage { tokens.insert(t.clone()); }
        for t in &stats.attributes_that_delta_power_cost { tokens.insert(t.clone()); }
        for t in &stats.attributes_that_mod_power_cost { tokens.insert(t.clone()); }
        for t in &stats.attributes_that_delta_rage { tokens.insert(t.clone()); }
        for t in &stats.attributes_that_mod_rage { tokens.insert(t.clone()); }
        for t in &stats.attributes_that_delta_taunt { tokens.insert(t.clone()); }
        for t in &stats.attributes_that_mod_taunt { tokens.insert(t.clone()); }
        if let Some(dots) = stats.extra.get("DoTs").and_then(|v| v.as_array()) {
            for dot in dots {
                for key in ["AttributesThatDelta", "AttributesThatMod", "AttributesThatDeltaDamage",
                            "AttributesThatModDamage", "AttributesThatModBaseDamage"] {
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

/// Check if `text` contains `name` not as a prefix of a longer known ability name.
fn text_contains_ability_name(text: &str, name: &str, longer_names: &[&str]) -> bool {
    let mut search_from = 0;
    while let Some(idx) = text[search_from..].find(name) {
        let abs_idx = search_from + idx;
        let is_prefix_of_longer = longer_names.iter().any(|longer| text[abs_idx..].starts_with(longer));
        if !is_prefix_of_longer {
            return true;
        }
        search_from = abs_idx + name.len();
    }
    false
}

/// Build precomputed bidirectional TSys ↔ Ability mapping.
/// Runs once at CDN load time. Uses three matching strategies:
/// 1. Attribute token overlap (structural, from combat stats)
/// 2. Icon ID matching (ability icon in effect desc `<icon=N>` tags)
/// 3. Ability name text matching (with prefix disambiguation)
fn build_tsys_ability_index(
    tsys: &tsys::TsysData,
    abilities: &HashMap<u32, abilities::AbilityInfo>,
    ability_families: &HashMap<String, abilities::AbilityFamily>,
    ability_to_family: &HashMap<u32, String>,
) -> (HashMap<String, Vec<u32>>, HashMap<u32, Vec<String>>) {
    let mut tsys_to_abilities: HashMap<String, Vec<u32>> = HashMap::new();
    let mut ability_to_tsys: HashMap<u32, Vec<String>> = HashMap::new();

    // Pre-collect ability data for matching
    struct AbilityMatchData {
        id: u32,
        tokens: std::collections::HashSet<String>,
        icon_id: Option<u32>,
        base_name: String,
    }

    // Build base name → list of all base names that start with it (for prefix disambiguation)
    let mut base_names: Vec<String> = Vec::new();
    let mut ability_match_data: Vec<AbilityMatchData> = Vec::new();

    // For each ability family, use the highest-level tier as representative
    for family in ability_families.values() {
        if family.is_monster_ability { continue; }

        // Use highest tier for matching
        let best_id = family.tier_ids.last().copied();
        let Some(best_id) = best_id else { continue };
        let Some(ability) = abilities.get(&best_id) else { continue };

        let tokens = collect_ability_attribute_tokens(ability);
        let base_name = family.base_name.clone();

        if base_name.len() >= 4 {
            base_names.push(base_name.clone());
        }

        ability_match_data.push(AbilityMatchData {
            id: best_id,
            tokens,
            icon_id: ability.icon_id,
            base_name,
        });
    }

    // Sort base names by length desc for prefix disambiguation
    base_names.sort_by(|a, b| b.len().cmp(&a.len()));

    // For each TSys entry, find matching abilities
    for (tsys_key, info) in &tsys.client_info {
        if info.is_unavailable == Some(true) { continue; }

        let mut matched_family_ids: Vec<u32> = Vec::new();

        for amd in &ability_match_data {
            let mut matches = false;

            // 1. Token matching: check all tiers
            'token_check: for tier in info.tiers.values() {
                let tier_tokens = extract_effect_tokens(&tier.effect_descs);
                if tier_tokens.iter().any(|t| amd.tokens.contains(t)) {
                    matches = true;
                    break 'token_check;
                }
            }

            if !matches {
                // 2. Icon ID matching (check first tier only)
                if let Some(tier) = info.tiers.values().next() {
                    if let Some(icon_id) = amd.icon_id {
                        let tier_icons = extract_icon_ids(&tier.effect_descs);
                        if tier_icons.contains(&icon_id) {
                            matches = true;
                        }
                    }

                    // 3. Text name matching
                    if !matches && amd.base_name.len() >= 4 {
                        let longer: Vec<&str> = base_names.iter()
                            .filter(|n| n.len() > amd.base_name.len() && n.starts_with(&amd.base_name))
                            .map(|s| s.as_str())
                            .collect();

                        for desc in &tier.effect_descs {
                            if desc.starts_with('{') { continue; }
                            let clean = strip_icon_tags(desc);
                            if text_contains_ability_name(&clean, &amd.base_name, &longer) {
                                matches = true;
                                break;
                            }
                        }
                    }
                }
            }

            if matches {
                matched_family_ids.push(amd.id);
            }
        }

        if !matched_family_ids.is_empty() {
            // Store all tier IDs for matched families (not just the representative)
            let mut all_ability_ids: Vec<u32> = Vec::new();
            for &rep_id in &matched_family_ids {
                if let Some(family_key) = ability_to_family.get(&rep_id) {
                    if let Some(family) = ability_families.get(family_key) {
                        all_ability_ids.extend(&family.tier_ids);
                    }
                }
            }
            all_ability_ids.sort();
            all_ability_ids.dedup();

            // Build reverse index
            for &aid in &all_ability_ids {
                ability_to_tsys.entry(aid).or_default().push(tsys_key.clone());
            }

            tsys_to_abilities.insert(tsys_key.clone(), all_ability_ids);
        }
    }

    (tsys_to_abilities, ability_to_tsys)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Duplicate display names must resolve to one key on every run, and to the
    /// same key the DB-backed resolver picks (`ORDER BY key LIMIT 1`). The live
    /// gift arm and the log backfill resolve the *same* note's display name
    /// through these two paths, and the gift tracker keys rows by NPC key — a
    /// disagreement splits one NPC's weekly count across two rows. A plain
    /// `.collect()` over the CDN `HashMap` picks whichever duplicate it visits
    /// last, which is why this is built by hand.
    #[test]
    fn npc_name_index_picks_the_lowest_key_for_duplicate_display_names() {
        // Insertion order puts the higher key last, so a `.collect()` would keep
        // that one.
        let pairs: Vec<(String, String)> = vec![
            ("NPC_WorkOrdersA".into(), "Work Orders".into()),
            ("NPC_WorkOrdersB".into(), "Work Orders".into()),
            ("NPC_Unique".into(), "Solo".into()),
        ];

        let index = build_npc_name_index(pairs.iter().map(|(key, name)| (key, name)));

        assert_eq!(
            index.get("Work Orders").map(String::as_str),
            Some("NPC_WorkOrdersA"),
            "the lowest key wins, deterministically"
        );
        assert_eq!(index.get("Solo").map(String::as_str), Some("NPC_Unique"));
    }

    fn ability(id: u32, name: &str, monster: bool) -> abilities::AbilityInfo {
        abilities::AbilityInfo {
            id,
            name: name.to_string(),
            internal_name: None,
            description: None,
            icon_id: None,
            skill: None,
            level: None,
            keywords: if monster { vec!["Lint_MonsterAbility".to_string()] } else { Vec::new() },
            damage_type: None,
            reset_time: None,
            target: None,
            prerequisite: None,
            upgrade_of: None,
            is_harmless: None,
            animation: None,
            special_info: None,
            works_underwater: None,
            works_while_falling: None,
            pve: None,
            pvp: None,
            mana_cost: None,
            power_cost: None,
            armor_cost: None,
            health_cost: None,
            range: None,
            raw_json: serde_json::Value::Null,
        }
    }

    fn family(base: &str, base_name: &str, monster: bool, tiers: Vec<u32>) -> abilities::AbilityFamily {
        abilities::AbilityFamily {
            base_internal_name: base.to_string(),
            base_name: base_name.to_string(),
            icon_id: None,
            skill: None,
            damage_type: None,
            is_monster_ability: monster,
            tier_ids: tiers,
        }
    }

    /// Wave of Darkness: the player base tier shares its exact name with two monster copies.
    /// The player ability must win the collision regardless of HashMap iteration order.
    #[test]
    fn name_index_prefers_player_ability_on_collision() {
        let abilities: HashMap<u32, abilities::AbilityInfo> = [
            (12334, ability(12334, "Wave of Darkness", true)),
            (3231, ability(3231, "Wave of Darkness", false)),
            (13624, ability(13624, "Wave of Darkness", true)),
        ]
        .into();
        let families = HashMap::from([(
            "WaveOfDarkness".to_string(),
            family("WaveOfDarkness", "Wave of Darkness", false, vec![3231]),
        )]);
        let index = build_ability_name_index(&abilities, &families);
        assert_eq!(index.get("Wave of Darkness"), Some(&3231));
    }

    /// Withering Shroud: no player ability is named bare "Withering Shroud" (tiers run
    /// "Withering Shroud 1"–"13") but a monster ability is. The family's base name must
    /// resolve to the player family's first tier so DoT prose mods land on it.
    #[test]
    fn name_index_maps_family_base_name_over_monster_squatter() {
        let abilities: HashMap<u32, abilities::AbilityInfo> = [
            (12257, ability(12257, "Withering Shroud", true)),
            (4926, ability(4926, "Withering Shroud 1", false)),
            (4937, ability(4937, "Withering Shroud 12", false)),
        ]
        .into();
        let families = HashMap::from([(
            "WitheringShroud1".to_string(),
            family("WitheringShroud1", "Withering Shroud", false, vec![4926, 4937]),
        )]);
        let index = build_ability_name_index(&abilities, &families);
        assert_eq!(index.get("Withering Shroud"), Some(&4926));
        // Tier names still resolve to themselves.
        assert_eq!(index.get("Withering Shroud 12"), Some(&4937));
    }

    /// A monster-only family must not hijack a name owned by a real player ability.
    #[test]
    fn name_index_keeps_player_ability_over_monster_family() {
        let abilities: HashMap<u32, abilities::AbilityInfo> = [
            (100, ability(100, "Screech", false)),
            (200, ability(200, "Screech 1", true)),
        ]
        .into();
        let families = HashMap::from([
            ("Screech".to_string(), family("Screech", "Screech", false, vec![100])),
            ("MonsterScreech".to_string(), family("MonsterScreech", "Screech", true, vec![200])),
        ]);
        let index = build_ability_name_index(&abilities, &families);
        assert_eq!(index.get("Screech"), Some(&100));
    }
}
