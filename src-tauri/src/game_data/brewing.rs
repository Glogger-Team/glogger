use serde::Serialize;
use std::collections::HashMap;

use super::items::ItemInfo;
use super::recipes::RecipeInfo;

// ── Brewing-specific structs (derived from existing recipe/item data) ────────

/// The category of a brewing recipe.
#[derive(Debug, Serialize, Clone, PartialEq, Eq)]
pub enum BrewingCategory {
    Beer,
    BeerKeg,
    Wine,
    LiquorUnaged,
    LiquorFinished,
    Utility,
}

/// A parsed `BrewItem(...)` ResultEffect string.
#[derive(Debug, Serialize, Clone)]
pub struct BrewItemEffect {
    /// Tier number (1-5 for beers, 1001+ for liquors, 2001+ for wines)
    pub tier: u32,
    /// Skill level associated with the effect
    pub skill_level: u32,
    /// Keyword slots that determine the effect (e.g., ["BrewingMushroomC4", "BrewingGarnishC3"])
    pub ingredient_slots: Vec<String>,
    /// Effect pool categories (e.g., ["Partying4", "RacialBonuses48"])
    pub effect_pools: Vec<String>,
}

/// A single variable ingredient slot in a brewing recipe.
#[derive(Debug, Serialize, Clone)]
pub struct BrewingVariableSlot {
    /// The keyword that items must have to fill this slot (e.g., "BrewingMushroomC4")
    pub keyword: String,
    /// Human-readable description from the recipe (e.g., "Coral Mushroom Powder, Groxmax, ...")
    pub description: Option<String>,
    /// Item IDs that can fill this slot (resolved from items with this keyword)
    pub valid_item_ids: Vec<u32>,
    /// Stack size required
    pub stack_size: f32,
}

/// A fixed ingredient in a brewing recipe (specific item required).
#[derive(Debug, Serialize, Clone)]
pub struct BrewingFixedIngredient {
    pub item_id: u32,
    pub stack_size: f32,
    pub chance_to_consume: Option<f32>,
}

/// A brewing recipe with all data needed for the Brewery tab.
#[derive(Debug, Serialize, Clone)]
pub struct BrewingRecipe {
    pub recipe_id: u32,
    pub name: String,
    pub internal_name: Option<String>,
    pub description: Option<String>,
    pub icon_id: Option<u32>,
    pub category: BrewingCategory,
    pub skill_level_req: u32,
    pub xp: u32,
    pub xp_first_time: Option<u32>,
    pub xp_drop_off_level: Option<u32>,
    pub usage_delay_message: Option<String>,
    pub fixed_ingredients: Vec<BrewingFixedIngredient>,
    pub variable_slots: Vec<BrewingVariableSlot>,
    pub brew_item_effect: Option<BrewItemEffect>,
    pub result_item_id: Option<u32>,
}

/// An item that can be used as a brewing ingredient, with its brewing keywords.
#[derive(Debug, Serialize, Clone)]
pub struct BrewingIngredient {
    pub item_id: u32,
    pub name: String,
    pub internal_name: Option<String>,
    pub icon_id: Option<u32>,
    pub brewing_keywords: Vec<String>,
}

// ── BrewItem parser ─────────────────────────────────────────────────────────

/// Parse a `BrewItem(tier,skillLevel,slot1+slot2+...=pool1+pool2+...)` string.
pub fn parse_brew_item(s: &str) -> Option<BrewItemEffect> {
    // Strip "BrewItem(" prefix and ")" suffix
    let inner = s.strip_prefix("BrewItem(")?.strip_suffix(')')?;

    // Split on first comma → tier
    let (tier_str, rest) = inner.split_once(',')?;
    let tier: u32 = tier_str.trim().parse().ok()?;

    // Split on second comma → skill level, then the slots=pools part
    let (level_str, slots_pools) = rest.split_once(',')?;
    let skill_level: u32 = level_str.trim().parse().ok()?;

    // Split on '=' → ingredient slots vs effect pools
    let (slots_part, pools_part) = slots_pools.trim().split_once('=')?;

    let ingredient_slots: Vec<String> = slots_part
        .split('+')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();

    let effect_pools: Vec<String> = pools_part
        .split('+')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();

    Some(BrewItemEffect {
        tier,
        skill_level,
        ingredient_slots,
        effect_pools,
    })
}

// ── Build brewing data from existing recipes + items ────────────────────────

/// Classify a recipe's brewing category from its properties.
fn classify_recipe(recipe: &RecipeInfo) -> Option<BrewingCategory> {
    let internal = recipe.internal_name.as_deref().unwrap_or("");
    let action = recipe.action_label.as_deref().unwrap_or("");

    // Check if it's a brewing recipe
    if recipe.skill.as_deref() != Some("Brewing") {
        return None;
    }

    // Utility recipes: reuse keg, reuse barrel, tap keg
    if internal.starts_with("Reuse") || internal == "TapAlcoholKeg" {
        return Some(BrewingCategory::Utility);
    }

    // Un-aged liquor: "Prepare Cask" action, internal name starts with "UnAged"
    if internal.starts_with("UnAged") {
        return Some(BrewingCategory::LiquorUnaged);
    }

    // Finished liquor: has BrewItem effect with tier >= 1001 and < 2001
    if let Some(effect) = recipe
        .result_effects
        .first()
        .and_then(|e| parse_brew_item(e))
    {
        if effect.tier >= 2001 {
            return Some(BrewingCategory::Wine);
        }
        if effect.tier >= 1001 {
            return Some(BrewingCategory::LiquorFinished);
        }
    }

    // Keg vs glass beer: check if name contains "Keg" or internal name ends with "Keg"
    if internal.ends_with("Keg") || action == "Brew Keg" {
        return Some(BrewingCategory::BeerKeg);
    }

    // If it has a BrewItem effect and we get here, it's a beer
    if recipe
        .result_effects
        .iter()
        .any(|e| e.starts_with("BrewItem("))
    {
        return Some(BrewingCategory::Beer);
    }

    // Fallback: if it's a Brewing skill recipe but doesn't match above, treat as utility
    Some(BrewingCategory::Utility)
}

/// Build the full brewing dataset from existing parsed recipes and items.
pub fn build_brewing_data(
    recipes: &HashMap<u32, RecipeInfo>,
    items: &HashMap<u32, ItemInfo>,
) -> (
    Vec<BrewingRecipe>,
    Vec<BrewingIngredient>,
    HashMap<String, Vec<u32>>,
) {
    // Build keyword → item IDs index for all brewing ingredients
    let mut keyword_to_items: HashMap<String, Vec<u32>> = HashMap::new();
    let mut brewing_ingredients: Vec<BrewingIngredient> = Vec::new();
    let mut seen_ingredient_ids = std::collections::HashSet::new();

    for (id, item) in items {
        let brewing_kws: Vec<String> = item
            .keywords
            .iter()
            .filter(|kw| kw.starts_with("Brewing"))
            .cloned()
            .collect();

        if brewing_kws.is_empty() {
            continue;
        }

        for kw in &brewing_kws {
            keyword_to_items.entry(kw.clone()).or_default().push(*id);
        }

        if seen_ingredient_ids.insert(*id) {
            brewing_ingredients.push(BrewingIngredient {
                item_id: *id,
                name: item.name.clone(),
                internal_name: item.internal_name.clone(),
                icon_id: item.icon_id,
                brewing_keywords: brewing_kws,
            });
        }
    }

    // Sort each keyword's item list for deterministic output
    for items_list in keyword_to_items.values_mut() {
        items_list.sort();
        items_list.dedup();
    }

    // Build brewing recipes
    let mut brewing_recipes: Vec<BrewingRecipe> = Vec::new();

    for (id, recipe) in recipes {
        let category = match classify_recipe(recipe) {
            Some(c) => c,
            None => continue,
        };

        // Parse BrewItem effect if present
        let brew_item_effect = recipe
            .result_effects
            .iter()
            .find(|e| e.starts_with("BrewItem("))
            .and_then(|e| parse_brew_item(e));

        // Separate fixed vs variable ingredients
        let mut fixed_ingredients = Vec::new();
        let mut variable_slots = Vec::new();

        for ingredient in &recipe.ingredients {
            if !ingredient.item_keys.is_empty() {
                // Variable slot — keyword-based
                let keyword = ingredient.item_keys[0].clone();
                let valid_ids = keyword_to_items.get(&keyword).cloned().unwrap_or_default();

                variable_slots.push(BrewingVariableSlot {
                    keyword,
                    description: ingredient.description.clone(),
                    valid_item_ids: valid_ids,
                    stack_size: ingredient.stack_size,
                });
            } else if let Some(item_id) = ingredient.item_id {
                // Fixed ingredient
                fixed_ingredients.push(BrewingFixedIngredient {
                    item_id,
                    stack_size: ingredient.stack_size,
                    chance_to_consume: ingredient.chance_to_consume,
                });
            }
        }

        let result_item_id = recipe.result_item_ids.first().copied();

        brewing_recipes.push(BrewingRecipe {
            recipe_id: *id,
            name: recipe.name.clone(),
            internal_name: recipe.internal_name.clone(),
            description: recipe.description.clone(),
            icon_id: recipe.icon_id,
            category,
            skill_level_req: recipe.skill_level_req.map(|v| v as u32).unwrap_or(0),
            xp: recipe.reward_skill_xp.map(|v| v as u32).unwrap_or(0),
            xp_first_time: recipe.reward_skill_xp_first_time.map(|v| v as u32),
            xp_drop_off_level: recipe.reward_skill_xp_drop_off_level,
            usage_delay_message: recipe
                .raw_json
                .get("UsageDelayMessage")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
            fixed_ingredients,
            variable_slots,
            brew_item_effect,
            result_item_id,
        });
    }

    // Sort recipes by category then skill level
    brewing_recipes.sort_by(|a, b| {
        let cat_order = |c: &BrewingCategory| match c {
            BrewingCategory::Beer => 0,
            BrewingCategory::BeerKeg => 1,
            BrewingCategory::Wine => 2,
            BrewingCategory::LiquorUnaged => 3,
            BrewingCategory::LiquorFinished => 4,
            BrewingCategory::Utility => 5,
        };
        cat_order(&a.category)
            .cmp(&cat_order(&b.category))
            .then(a.skill_level_req.cmp(&b.skill_level_req))
            .then(a.name.cmp(&b.name))
    });

    brewing_ingredients.sort_by(|a, b| a.name.cmp(&b.name));

    (brewing_recipes, brewing_ingredients, keyword_to_items)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_brew_item_basic() {
        let result = parse_brew_item("BrewItem(1,0,BrewingFruitA4=Partying4)");
        assert!(result.is_some());
        let r = result.unwrap();
        assert_eq!(r.tier, 1);
        assert_eq!(r.skill_level, 0);
        assert_eq!(r.ingredient_slots, vec!["BrewingFruitA4"]);
        assert_eq!(r.effect_pools, vec!["Partying4"]);
    }

    #[test]
    fn test_parse_brew_item_complex() {
        let result = parse_brew_item(
            "BrewItem(5,175,BrewingVegetableB4+BrewingFruitC3+BrewingMushroomC4+BrewingGarnishC3=Partying4+Gathering4+SkillSpecificPowerCosts12+Endurance12+RacialBonuses48+RacialBonuses48+Endurance12+Partying4)"
        );
        assert!(result.is_some());
        let r = result.unwrap();
        assert_eq!(r.tier, 5);
        assert_eq!(r.skill_level, 175);
        assert_eq!(r.ingredient_slots.len(), 4);
        assert_eq!(r.ingredient_slots[0], "BrewingVegetableB4");
        assert_eq!(r.effect_pools.len(), 8);
    }

    #[test]
    fn test_parse_brew_item_liquor() {
        let result = parse_brew_item(
            "BrewItem(1001,0,BrewingFruitA3+BrewingMushroomA4+BrewingAnimalPartA5+BrewingGarnishA4=Endurance12+Partying4+BasicMitigation18+DamageVsAnatomy32+Gathering4+DirectDamageBoosts15+EliteFighting4+Endurance12+Partying4+BasicMitigation18+DamageVsAnatomy32+Gathering4+DirectDamageBoosts15+EliteFighting4+RacialBonuses48+Endurance12+Endurance2)"
        );
        assert!(result.is_some());
        let r = result.unwrap();
        assert_eq!(r.tier, 1001);
        assert_eq!(r.ingredient_slots.len(), 4);
        assert!(r
            .ingredient_slots
            .contains(&"BrewingAnimalPartA5".to_string()));
    }

    #[test]
    fn test_parse_brew_item_wine() {
        let result = parse_brew_item(
            "BrewItem(2005,85,BrewingHerbsX2+BrewingAdditiveW3+BrewingFlowersW4+BrewingCrunchX4=Partying4+Endurance4+FishAndGame6+SkillBaseDamage32+BasicMitigation18+DamageVsAnatomy32)"
        );
        assert!(result.is_some());
        let r = result.unwrap();
        assert_eq!(r.tier, 2005);
        assert_eq!(r.skill_level, 85);
        assert_eq!(r.ingredient_slots.len(), 4);
    }

    #[test]
    fn test_parse_brew_item_invalid() {
        assert!(parse_brew_item("NotBrewItem").is_none());
        assert!(parse_brew_item("BrewItem()").is_none());
        assert!(parse_brew_item("BrewItem(abc,0,X=Y)").is_none());
    }
}
