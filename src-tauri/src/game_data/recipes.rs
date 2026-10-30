use serde::Serialize;
use serde_json::Value;
use std::collections::HashMap;

// ── Parsed structs (app shape) ───────────────────────────────────────────────

#[derive(Debug, Serialize, Clone)]
pub struct RecipeIngredient {
    pub item_id: Option<u32>,
    pub item_keys: Vec<String>,
    pub description: Option<String>,
    pub stack_size: f32,
    pub chance_to_consume: Option<f32>,
}

/// A non-item cost paid when using the recipe (Combat Wisdom, Fae Energy, …).
#[derive(Debug, Serialize, Clone)]
pub struct RecipeCost {
    pub currency: String,
    pub price: f32,
}

#[derive(Debug, Serialize, Clone)]
pub struct RecipeResultItem {
    pub item_id: u32,
    pub stack_size: f32,
    pub percent_chance: Option<f32>,
}

/// A single recipe definition.
#[derive(Debug, Serialize, Clone)]
pub struct RecipeInfo {
    pub id: u32,
    pub name: String,
    pub description: Option<String>,
    pub internal_name: Option<String>,
    pub icon_id: Option<u32>,
    pub skill: Option<String>,
    pub skill_level_req: Option<f32>,
    pub ingredients: Vec<RecipeIngredient>,
    pub costs: Vec<RecipeCost>,
    pub result_items: Vec<RecipeResultItem>,
    pub reward_skill: Option<String>,
    pub reward_skill_xp: Option<f32>,
    pub reward_skill_xp_first_time: Option<f32>,
    pub prereq_recipe: Option<String>,
    pub keywords: Vec<String>,
    pub ingredient_item_ids: Vec<u32>,
    pub result_item_ids: Vec<u32>,

    // Phase 2 typed fields
    pub result_effects: Vec<String>,
    pub usage_delay: Option<f32>,
    pub reward_skill_xp_drop_off_level: Option<u32>,
    pub sort_skill: Option<String>,
    pub action_label: Option<String>,
    pub shares_name_with_item_id: Option<u32>,

    // Full raw JSON
    pub raw_json: Value,
}

// ── Parse function ───────────────────────────────────────────────────────────

pub fn parse(json: &str) -> Result<HashMap<u32, RecipeInfo>, String> {
    let raw: HashMap<String, Value> = serde_json::from_str(json).map_err(|e| {
        format!(
            "recipes.json: parse error at line {}, col {}: {e}",
            e.line(),
            e.column()
        )
    })?;

    let mut recipes = HashMap::with_capacity(raw.len());
    let mut skipped = 0;

    for (key, value) in raw {
        let id_str = match key.rsplit('_').next() {
            Some(s) => s.to_string(),
            None => {
                skipped += 1;
                continue;
            }
        };
        let id: u32 = match id_str.parse() {
            Ok(id) => id,
            Err(_) => {
                skipped += 1;
                continue;
            }
        };

        let ingredients = parse_ingredients(&value);
        let costs = parse_costs(&value);
        let result_items = parse_result_items(&value, "ResultItems");
        let proto_result_items = parse_result_items(&value, "ProtoResultItems");

        let ingredient_item_ids: Vec<u32> = ingredients.iter().filter_map(|i| i.item_id).collect();
        let mut result_item_ids: Vec<u32> = result_items.iter().map(|r| r.item_id).collect();
        result_item_ids.extend(proto_result_items.iter().map(|r| r.item_id));

        recipes.insert(
            id,
            RecipeInfo {
                id,
                name: str_field(&value, "Name").unwrap_or_else(|| format!("Unknown Recipe {id}")),
                description: str_field(&value, "Description"),
                internal_name: str_field(&value, "InternalName"),
                icon_id: u32_field(&value, "IconId"),
                skill: str_field(&value, "Skill"),
                skill_level_req: f32_field(&value, "SkillLevelReq"),
                ingredients,
                costs,
                result_items,
                reward_skill: str_field(&value, "RewardSkill"),
                reward_skill_xp: f32_field(&value, "RewardSkillXp"),
                reward_skill_xp_first_time: f32_field(&value, "RewardSkillXpFirstTime"),
                prereq_recipe: str_field(&value, "PrereqRecipe"),
                keywords: str_array_field(&value, "Keywords"),
                ingredient_item_ids,
                result_item_ids,

                // Phase 2 typed fields
                result_effects: str_array_field(&value, "ResultEffects"),
                usage_delay: f32_field(&value, "UsageDelay"),
                reward_skill_xp_drop_off_level: u32_field(&value, "RewardSkillXpDropOffLevel"),
                sort_skill: str_field(&value, "SortSkill"),
                action_label: str_field(&value, "ActionLabel"),
                shares_name_with_item_id: u32_field(&value, "SharesNameWithItemId"),

                raw_json: value,
            },
        );
    }

    if skipped > 0 {
        eprintln!("recipes.json: Warning: skipped {skipped} entries with invalid keys");
    }

    Ok(recipes)
}

// ── Ingredient / result parsing ──────────────────────────────────────────────

fn parse_ingredients(value: &Value) -> Vec<RecipeIngredient> {
    value
        .get("Ingredients")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|item| {
                    let item_id = item
                        .get("ItemCode")
                        .and_then(|v| v.as_u64())
                        .map(|n| n as u32);
                    let item_keys = str_array_field(item, "ItemKeys");

                    // Skip entries that have neither ItemCode nor ItemKeys
                    if item_id.is_none() && item_keys.is_empty() {
                        return None;
                    }

                    Some(RecipeIngredient {
                        item_id,
                        item_keys,
                        description: str_field(item, "Desc"),
                        stack_size: item
                            .get("StackSize")
                            .and_then(|v| v.as_f64())
                            .map(|n| n as f32)
                            .unwrap_or(1.0),
                        chance_to_consume: item
                            .get("ChanceToConsume")
                            .and_then(|v| v.as_f64())
                            .map(|n| n as f32),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Parse the `Costs` array — currency prices paid on top of the item ingredients.
fn parse_costs(value: &Value) -> Vec<RecipeCost> {
    value
        .get("Costs")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|cost| {
                    Some(RecipeCost {
                        currency: str_field(cost, "Currency")?,
                        price: cost.get("Price").and_then(|v| v.as_f64())? as f32,
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

fn parse_result_items(value: &Value, field: &str) -> Vec<RecipeResultItem> {
    value
        .get(field)
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|item| {
                    let item_id = item.get("ItemCode")?.as_u64()? as u32;
                    Some(RecipeResultItem {
                        item_id,
                        stack_size: item
                            .get("StackSize")
                            .and_then(|v| v.as_f64())
                            .map(|n| n as f32)
                            .unwrap_or(1.0),
                        percent_chance: item
                            .get("PercentChance")
                            .and_then(|v| v.as_f64())
                            .map(|n| n as f32),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

// ── Field extraction helpers ─────────────────────────────────────────────────

fn str_field(value: &Value, key: &str) -> Option<String> {
    value.get(key)?.as_str().map(|s| s.to_string())
}

fn u32_field(value: &Value, key: &str) -> Option<u32> {
    value.get(key)?.as_u64().map(|n| n as u32)
}

fn f32_field(value: &Value, key: &str) -> Option<f32> {
    value.get(key).and_then(|v| v.as_f64()).map(|n| n as f32)
}

fn str_array_field(value: &Value, key: &str) -> Vec<String> {
    value
        .get(key)
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_currency_costs() {
        let json = r#"{
            "recipe_18111": {
                "Name": "Infuse Combat Wisdom into Prophesied Hammer",
                "Costs": [{ "Currency": "CombatWisdom", "Price": 1000 }],
                "Ingredients": [
                    { "Desc": "Foretold Hammer (Augmented)", "ItemKeys": ["Hammer"], "StackSize": 1 }
                ]
            }
        }"#;

        let recipes = parse(json).expect("parse");
        let recipe = &recipes[&18111];
        assert_eq!(recipe.costs.len(), 1);
        assert_eq!(recipe.costs[0].currency, "CombatWisdom");
        assert_eq!(recipe.costs[0].price, 1000.0);
        // Currency costs are separate from the item ingredient list
        assert_eq!(recipe.ingredients.len(), 1);
    }

    #[test]
    fn recipes_without_costs_get_an_empty_list() {
        let json = r#"{ "recipe_1": { "Name": "Plain Recipe" } }"#;
        assert!(parse(json).expect("parse")[&1].costs.is_empty());
    }
}
