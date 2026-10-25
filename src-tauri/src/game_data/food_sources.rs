//! Where each food comes from — powers the Gourmand tracker's source filter.
//!
//! Every food is classified into a *set* of source kinds derived from
//! `sources_items.json` (a food can be both crafted and sold by a vendor), plus
//! the display names of the skills that can craft it. The kinds are emitted in
//! a fixed priority order so the frontend can treat `kinds[0]` as the primary
//! kind for badges and grouping.

use std::collections::{HashMap, HashSet};

use super::items::ItemInfo;
use super::recipes::RecipeInfo;
use super::sources::SourceInfo;

// ── Source kinds ─────────────────────────────────────────────────────────────
// These strings cross the Tauri boundary; keep them in sync with
// `FoodSourceKind` in src/types/gourmand.ts.

pub const KIND_UNOBTAINABLE: &str = "unobtainable";
pub const KIND_CRAFTED: &str = "crafted";
pub const KIND_EVENT: &str = "event";
pub const KIND_VENDOR: &str = "vendor";
pub const KIND_QUEST: &str = "quest";
pub const KIND_NPC_GIFT: &str = "npc-gift";
pub const KIND_HANGOUT: &str = "hangout";
pub const KIND_BARTER: &str = "barter";
pub const KIND_CONTAINER: &str = "container";
pub const KIND_OTHER: &str = "other";

/// The classification of a single food.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct FoodSources {
    /// Applicable source kinds, most-significant first. Never empty — a food
    /// with no known source gets `["other"]`.
    pub kinds: Vec<String>,
    /// Display names of the skills that can craft this food, alphabetical.
    /// Empty unless `kinds` contains `crafted`.
    pub craft_skills: Vec<String>,
}

// ── Curated event list ───────────────────────────────────────────────────────

/// Marks a food as handed out during a seasonal event or as a commemorative
/// gift from the devs.
///
/// The CDN has no event flag of any kind — no keyword, no field, no source
/// entry — so this list is curated by hand and needs a line added whenever
/// Project: Gorgon ships a new event food. Every entry below was verified to
/// have zero `sources_items.json` entries, with one deliberate exception: the
/// top-tier Halloween candy (`HalloweenCandy9`, Grape-Strawberry Candy) is a
/// quest reward, so it ends up classified as both `quest` and `event`.
///
/// Deliberately *not* listed: the pumpkin dishes (Pumpkin Pie, Pumpkin Soup,
/// …) are ordinary year-round Cooking recipes despite the seasonal theming,
/// and Jellybeans / Tiramisu of Delights have no evidence either way, so they
/// fall through to `other` rather than being guessed at.
fn is_event_food(item: &ItemInfo) -> bool {
    // Halloween — the candy ladder shares a keyword.
    if item.keywords.iter().any(|k| k == "HalloweenCandy") {
        return true;
    }

    let internal = match item.internal_name.as_deref() {
        Some(n) => n,
        None => return false,
    };

    // Winter — MintCandyCane, TartCandyCane, MagicalCandyCane, NormalCandyCane,
    // LemonCandyCane, SpearmintCandyCane, GrapeCandyCane, and any future cane.
    if internal.ends_with("CandyCane") {
        return true;
    }

    // Steam anniversary — SteamCake3 … SteamCake8 and onward.
    if internal.starts_with("SteamCake") {
        return true;
    }

    const EVENT_ITEMS: &[&str] = &[
        // Summer treats (item ids 6480-6485, one contiguous block).
        "SugarStick",
        "CandyDrops",
        "FrozenFruitPop",
        "SummerSucker",
        "SugarCaterpillar",
        "CandyPebbles",
        // Commemorative / promotional cakes.
        "CelebratoryCake",
        "WelcomeCake",
        "ApologyCake",
        "ApologyCakeAttuned",
        "ThankYouCake",
        "BackerSnackCake",
    ];

    EVENT_ITEMS.contains(&internal)
}

// ── Classification ───────────────────────────────────────────────────────────

/// Classify one food item.
///
/// `sources` is the item's entry from `sources_items.json` (absent for items
/// the CDN lists no source for), `recipes` resolves `Recipe` source entries to
/// their crafting skill, and `skill_display_names` maps a skill's internal name
/// (`"SushiPreparation"`) to its display name (`"Sushi Preparation"`).
pub fn classify(
    item: &ItemInfo,
    sources: Option<&SourceInfo>,
    recipes: &HashMap<u32, RecipeInfo>,
    skill_display_names: &HashMap<&str, &str>,
) -> FoodSources {
    let entries = sources.map(|s| s.entries.as_slice()).unwrap_or(&[]);

    let mut types: HashSet<&str> = HashSet::new();
    let mut craft_skills: HashSet<String> = HashSet::new();

    for entry in entries {
        types.insert(entry.source_type.as_str());

        if entry.source_type == "Recipe" {
            let skill = entry
                .recipe_id
                .and_then(|id| recipes.get(&id))
                .and_then(|r| r.skill.as_deref());

            if let Some(skill) = skill {
                let display = skill_display_names.get(skill).copied().unwrap_or(skill);
                craft_skills.insert(display.to_string());
            }
        }
    }

    let mut kinds: Vec<String> = Vec::new();

    // Priority order: the first kind is what the UI shows as the badge, and
    // what the "attainable only" progress filter tests against.
    if item.keywords.iter().any(|k| k == "Lint_NotObtainable") {
        kinds.push(KIND_UNOBTAINABLE.to_string());
    }
    if types.contains("Recipe") {
        kinds.push(KIND_CRAFTED.to_string());
    }
    if is_event_food(item) {
        kinds.push(KIND_EVENT.to_string());
    }
    if types.contains("Vendor") {
        kinds.push(KIND_VENDOR.to_string());
    }
    if types.contains("Quest") {
        kinds.push(KIND_QUEST.to_string());
    }
    if types.contains("NpcGift") {
        kinds.push(KIND_NPC_GIFT.to_string());
    }
    if types.contains("HangOut") {
        kinds.push(KIND_HANGOUT.to_string());
    }
    if types.contains("Barter") {
        kinds.push(KIND_BARTER.to_string());
    }
    if types.contains("Item") {
        kinds.push(KIND_CONTAINER.to_string());
    }
    if kinds.is_empty() {
        // Monster drops, foraged ingredients, butchered flesh — the CDN
        // records no source for any of them.
        kinds.push(KIND_OTHER.to_string());
    }

    let mut craft_skills: Vec<String> = craft_skills.into_iter().collect();
    craft_skills.sort();

    FoodSources {
        kinds,
        craft_skills,
    }
}

/// Build the `internal name → display name` lookup `classify` expects.
pub fn skill_display_names(skills: &HashMap<u32, super::skills::SkillInfo>) -> HashMap<&str, &str> {
    skills
        .values()
        .map(|s| (s.internal_name.as_str(), s.name.as_str()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game_data::sources::SourceEntry;
    use serde_json::json;

    fn item(internal: &str, keywords: &[&str]) -> ItemInfo {
        ItemInfo {
            id: 1,
            name: internal.to_string(),
            description: None,
            icon_id: None,
            value: None,
            max_stack_size: None,
            keywords: keywords.iter().map(|k| k.to_string()).collect(),
            effect_descs: vec![],
            internal_name: Some(internal.to_string()),
            food_desc: Some("Level 10 Snack".to_string()),
            equip_slot: None,
            num_uses: None,
            skill_reqs: None,
            behaviors: None,
            bestow_recipes: None,
            bestow_ability: None,
            bestow_quest: None,
            bestow_title: None,
            craft_points: None,
            crafting_target_level: None,
            tsys_profile: None,
            raw_json: json!({}),
        }
    }

    fn entry(source_type: &str, recipe_id: Option<u32>) -> SourceEntry {
        SourceEntry {
            source_type: source_type.to_string(),
            skill: None,
            npc: None,
            item_type_id: None,
            quest_id: None,
            recipe_id,
            hang_out_id: None,
            friendly_name: None,
            extra: json!({}),
        }
    }

    fn recipe(id: u32, skill: &str) -> RecipeInfo {
        RecipeInfo {
            id,
            name: format!("Recipe {id}"),
            description: None,
            internal_name: None,
            icon_id: None,
            skill: Some(skill.to_string()),
            skill_level_req: None,
            ingredients: vec![],
            costs: vec![],
            result_items: vec![],
            reward_skill: None,
            reward_skill_xp: None,
            reward_skill_xp_first_time: None,
            prereq_recipe: None,
            keywords: vec![],
            ingredient_item_ids: vec![],
            result_item_ids: vec![],
            result_effects: vec![],
            usage_delay: None,
            reward_skill_xp_drop_off_level: None,
            sort_skill: None,
            action_label: None,
            shares_name_with_item_id: None,
            raw_json: json!({}),
        }
    }

    #[test]
    fn crafted_food_reports_its_craft_skill() {
        let recipes = HashMap::from([(7, recipe(7, "SushiPreparation"))]);
        let names = HashMap::from([("SushiPreparation", "Sushi Preparation")]);
        let sources = SourceInfo {
            entries: vec![entry("Recipe", Some(7))],
        };

        let out = classify(&item("SalmonNigiri", &[]), Some(&sources), &recipes, &names);

        assert_eq!(out.kinds, vec![KIND_CRAFTED]);
        assert_eq!(out.craft_skills, vec!["Sushi Preparation"]);
    }

    #[test]
    fn skill_without_a_display_name_falls_back_to_its_key() {
        let recipes = HashMap::from([(1, recipe(1, "Cooking"))]);
        let sources = SourceInfo {
            entries: vec![entry("Recipe", Some(1))],
        };

        let out = classify(
            &item("Bread", &[]),
            Some(&sources),
            &recipes,
            &HashMap::new(),
        );

        assert_eq!(out.craft_skills, vec!["Cooking"]);
    }

    #[test]
    fn multiple_sources_are_all_recorded_in_priority_order() {
        let recipes = HashMap::from([(1, recipe(1, "Cooking"))]);
        let sources = SourceInfo {
            entries: vec![
                entry("Vendor", None),
                entry("NpcGift", None),
                entry("Recipe", Some(1)),
            ],
        };

        let out = classify(
            &item("Cheese", &[]),
            Some(&sources),
            &recipes,
            &HashMap::new(),
        );

        assert_eq!(out.kinds, vec![KIND_CRAFTED, KIND_VENDOR, KIND_NPC_GIFT]);
    }

    #[test]
    fn halloween_candy_is_an_event_food_via_keyword() {
        let out = classify(
            &item("HalloweenCandy0", &["HalloweenCandy"]),
            None,
            &HashMap::new(),
            &HashMap::new(),
        );

        assert_eq!(out.kinds, vec![KIND_EVENT]);
    }

    #[test]
    fn quest_given_halloween_candy_is_both_quest_and_event() {
        let sources = SourceInfo {
            entries: vec![entry("Quest", None)],
        };

        let out = classify(
            &item("HalloweenCandy9", &["HalloweenCandy"]),
            Some(&sources),
            &HashMap::new(),
            &HashMap::new(),
        );

        assert_eq!(out.kinds, vec![KIND_EVENT, KIND_QUEST]);
    }

    #[test]
    fn candy_canes_and_steam_cakes_match_by_name_pattern() {
        for internal in ["MintCandyCane", "GrapeCandyCane", "SteamCake8"] {
            let out = classify(&item(internal, &[]), None, &HashMap::new(), &HashMap::new());
            assert_eq!(out.kinds, vec![KIND_EVENT], "{internal}");
        }
    }

    #[test]
    fn unobtainable_outranks_everything_else() {
        let out = classify(
            &item("ApologyCake", &["Lint_NotObtainable"]),
            None,
            &HashMap::new(),
            &HashMap::new(),
        );

        assert_eq!(out.kinds, vec![KIND_UNOBTAINABLE, KIND_EVENT]);
    }

    #[test]
    fn seasonally_themed_cooking_recipes_are_not_event_foods() {
        let recipes = HashMap::from([(1, recipe(1, "Cooking"))]);
        let sources = SourceInfo {
            entries: vec![entry("Recipe", Some(1))],
        };

        let out = classify(
            &item("FancyPumpkinPie", &[]),
            Some(&sources),
            &recipes,
            &HashMap::new(),
        );

        assert_eq!(out.kinds, vec![KIND_CRAFTED]);
    }

    #[test]
    fn a_food_with_no_known_source_falls_back_to_other() {
        let out = classify(&item("Honey", &[]), None, &HashMap::new(), &HashMap::new());

        assert_eq!(out.kinds, vec![KIND_OTHER]);
        assert!(out.craft_skills.is_empty());
    }
}
