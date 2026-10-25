use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

// ── Parsed structs (app shape) ───────────────────────────────────────────────

/// Typed PvE or PvP combat stats for an ability.
#[derive(Debug, Serialize, Clone, Default)]
pub struct CombatStats {
    pub damage: Option<f32>,
    /// Armor-bypassing "health-specific" up-front hit (e.g. Mindworm, Mamba Strike). Shares the
    /// ability's standard direct-damage modifier arrays — there is no HSD-specific mod array in
    /// the data — so it is computed with the same buckets as `damage`.
    pub health_specific_damage: Option<f32>,
    /// "Armor-specific" up-front hit (e.g. Acid Opener, Acid Spew's armor component). Also driven
    /// by the standard direct-damage modifier arrays.
    pub armor_specific_damage: Option<f32>,
    pub power_cost: Option<f32>,
    pub range: Option<f32>,
    pub rage_cost: Option<f32>,
    pub accuracy: Option<f32>,
    pub attributes_that_delta_damage: Vec<String>,
    pub attributes_that_mod_base_damage: Vec<String>,
    pub attributes_that_mod_damage: Vec<String>,
    pub attributes_that_mod_crit_damage: Vec<String>,
    pub attributes_that_delta_power_cost: Vec<String>,
    pub attributes_that_mod_power_cost: Vec<String>,
    pub attributes_that_delta_rage: Vec<String>,
    pub attributes_that_mod_rage: Vec<String>,
    pub attributes_that_delta_taunt: Vec<String>,
    pub attributes_that_mod_taunt: Vec<String>,
    /// Damage-over-time components (each with its own per-tick damage and mod arrays).
    pub dots: Vec<DotEffect>,
    /// Labeled non-damage values (heals, restores, etc.) with their own mod arrays.
    pub special_values: Vec<SpecialValue>,
    /// Any fields not explicitly typed above.
    pub extra: Value,
}

/// A single damage-over-time component of an ability.
#[derive(Debug, Serialize, Clone, Default)]
pub struct DotEffect {
    pub damage_per_tick: f32,
    pub num_ticks: f32,
    pub duration: Option<f32>,
    pub damage_type: Option<String>,
    pub preface: Option<String>,
    /// Flat additions to per-tick damage.
    pub attributes_that_delta: Vec<String>,
    /// Percentage multipliers on per-tick damage.
    pub attributes_that_mod: Vec<String>,
}

/// A labeled numeric effect of an ability (heal, restore, buff amount, etc.).
/// Shares the same base-value + delta/mod-attribute shape as damage.
#[derive(Debug, Serialize, Clone, Default)]
pub struct SpecialValue {
    pub label: Option<String>,
    pub suffix: Option<String>,
    pub value: f32,
    pub display_type: Option<String>,
    /// When true and the resolved value is zero, the game hides this line.
    pub skip_if_zero: bool,
    /// Flat additions to the base value.
    pub attributes_that_delta_base: Vec<String>,
    /// Flat additions applied after the base.
    pub attributes_that_delta: Vec<String>,
    /// Percentage multipliers.
    pub attributes_that_mod: Vec<String>,
}

/// A single ability definition.
#[derive(Debug, Serialize, Clone)]
pub struct AbilityInfo {
    pub id: u32,
    pub name: String,
    pub internal_name: Option<String>,
    pub description: Option<String>,
    pub icon_id: Option<u32>,
    pub skill: Option<String>,
    pub level: Option<f32>,
    pub keywords: Vec<String>,

    pub damage_type: Option<String>,
    pub reset_time: Option<f32>,
    pub target: Option<String>,
    pub prerequisite: Option<String>,
    pub upgrade_of: Option<String>,
    pub is_harmless: Option<bool>,
    pub animation: Option<String>,
    pub special_info: Option<String>,
    pub works_underwater: Option<bool>,
    pub works_while_falling: Option<bool>,
    pub pve: Option<CombatStats>,
    pub pvp: Option<CombatStats>,
    pub mana_cost: Option<u32>,
    pub power_cost: Option<u32>,
    pub armor_cost: Option<u32>,
    pub health_cost: Option<u32>,
    pub range: Option<f32>,

    // Full raw JSON
    pub raw_json: Value,
}

/// A group of ability tiers that represent the same base ability at different power levels.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AbilityFamily {
    /// InternalName of the base (tier 1) ability, used as the family key.
    pub base_internal_name: String,
    /// Display name of the base ability (without tier number).
    pub base_name: String,
    pub icon_id: Option<u32>,
    pub skill: Option<String>,
    pub damage_type: Option<String>,
    /// Whether this is a monster-only ability (has Lint_MonsterAbility keyword).
    pub is_monster_ability: bool,
    /// Ordered list of tier ability IDs (ascending by level).
    pub tier_ids: Vec<u32>,
}

// ── Parse function ───────────────────────────────────────────────────────────

pub fn parse(json: &str) -> Result<HashMap<u32, AbilityInfo>, String> {
    let raw: HashMap<String, Value> = serde_json::from_str(json).map_err(|e| {
        format!(
            "abilities.json: parse error at line {}, col {}: {e}",
            e.line(),
            e.column()
        )
    })?;

    let mut abilities = HashMap::with_capacity(raw.len());
    let mut skipped = 0;

    for (key, value) in raw {
        let id_str = match key.split('_').last() {
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

        abilities.insert(
            id,
            AbilityInfo {
                id,
                name: str_field(&value, "Name").unwrap_or_else(|| format!("Unknown Ability {id}")),
                internal_name: str_field(&value, "InternalName"),
                description: str_field(&value, "Description"),
                icon_id: u32_field(&value, "IconID"),
                skill: str_field(&value, "Skill"),
                level: f32_field(&value, "Level"),
                keywords: str_array_field(&value, "Keywords"),

                damage_type: str_field(&value, "DamageType"),
                reset_time: f32_field(&value, "ResetTime"),
                target: str_field(&value, "Target"),
                prerequisite: str_field(&value, "Prerequisite"),
                upgrade_of: str_field(&value, "UpgradeOf"),
                is_harmless: bool_field(&value, "IsHarmless"),
                animation: str_field(&value, "Animation"),
                special_info: str_field(&value, "SpecialInfo"),
                works_underwater: bool_field(&value, "WorksUnderwater"),
                works_while_falling: bool_field(&value, "WorksWhileFalling"),
                pve: value.get("PvE").map(parse_combat_stats),
                pvp: value.get("PvP").map(parse_combat_stats),
                mana_cost: u32_field(&value, "ManaCost"),
                power_cost: u32_field(&value, "PowerCost"),
                armor_cost: u32_field(&value, "ArmorCost"),
                health_cost: u32_field(&value, "HealthCost"),
                range: f32_field(&value, "Range"),

                raw_json: value,
            },
        );
    }

    if skipped > 0 {
        eprintln!("abilities.json: Warning: skipped {skipped} entries with invalid keys");
    }

    Ok(abilities)
}

// ── Combat stats parsing ────────────────────────────────────────────────────

/// Known keys that are extracted into typed CombatStats fields.
const COMBAT_STATS_KNOWN_KEYS: &[&str] = &[
    "Damage",
    "HealthSpecificDamage",
    "ArmorSpecificDamage",
    "PowerCost",
    "Range",
    "RageCost",
    "Accuracy",
    "AttributesThatDeltaDamage",
    "AttributesThatModBaseDamage",
    "AttributesThatModDamage",
    "AttributesThatModCritDamage",
    "AttributesThatDeltaPowerCost",
    "AttributesThatModPowerCost",
    "AttributesThatDeltaRage",
    "AttributesThatModRage",
    "AttributesThatDeltaTaunt",
    "AttributesThatModTaunt",
    "DoTs",
    "SpecialValues",
];

fn parse_combat_stats(value: &Value) -> CombatStats {
    // Build `extra` object from fields we don't explicitly type
    let extra = if let Some(obj) = value.as_object() {
        let filtered: serde_json::Map<String, Value> = obj
            .iter()
            .filter(|(k, _)| !COMBAT_STATS_KNOWN_KEYS.contains(&k.as_str()))
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        Value::Object(filtered)
    } else {
        Value::Object(serde_json::Map::new())
    };

    CombatStats {
        // Three distinct up-front damage shapes exist in the data and are kept separate so each
        // can be shown on its own line in the effective-stats tooltip:
        //   • `Damage`               — normal hit (armor first, then health).
        //   • `HealthSpecificDamage` — armor-bypassing hit (Mentalism/Psychology/Vampirism, ~235).
        //   • `ArmorSpecificDamage`  — armor-only hit (Acid* etc., ~188).
        // Some abilities carry two or three at once (e.g. Acid Spew = Damage + Armor, Rage Acid
        // Toss = Health + Armor). All three share the ability's single set of direct-damage
        // modifier arrays (there is no per-shape mod array), so the formula engine folds the same
        // mods into each component.
        damage: f32_field(value, "Damage"),
        health_specific_damage: f32_field(value, "HealthSpecificDamage"),
        armor_specific_damage: f32_field(value, "ArmorSpecificDamage"),
        power_cost: f32_field(value, "PowerCost"),
        range: f32_field(value, "Range"),
        rage_cost: f32_field(value, "RageCost"),
        accuracy: f32_field(value, "Accuracy"),
        attributes_that_delta_damage: str_array_field(value, "AttributesThatDeltaDamage"),
        attributes_that_mod_base_damage: str_array_field(value, "AttributesThatModBaseDamage"),
        attributes_that_mod_damage: str_array_field(value, "AttributesThatModDamage"),
        attributes_that_mod_crit_damage: str_array_field(value, "AttributesThatModCritDamage"),
        attributes_that_delta_power_cost: str_array_field(value, "AttributesThatDeltaPowerCost"),
        attributes_that_mod_power_cost: str_array_field(value, "AttributesThatModPowerCost"),
        attributes_that_delta_rage: str_array_field(value, "AttributesThatDeltaRage"),
        attributes_that_mod_rage: str_array_field(value, "AttributesThatModRage"),
        attributes_that_delta_taunt: str_array_field(value, "AttributesThatDeltaTaunt"),
        attributes_that_mod_taunt: str_array_field(value, "AttributesThatModTaunt"),
        dots: parse_dots(value),
        special_values: parse_special_values(value),
        extra,
    }
}

fn parse_dots(value: &Value) -> Vec<DotEffect> {
    value
        .get("DoTs")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .map(|d| DotEffect {
                    damage_per_tick: f32_field(d, "DamagePerTick").unwrap_or(0.0),
                    num_ticks: f32_field(d, "NumTicks").unwrap_or(0.0),
                    duration: f32_field(d, "Duration"),
                    damage_type: str_field(d, "DamageType"),
                    preface: str_field(d, "Preface"),
                    attributes_that_delta: str_array_field(d, "AttributesThatDelta"),
                    attributes_that_mod: str_array_field(d, "AttributesThatMod"),
                })
                .collect()
        })
        .unwrap_or_default()
}

fn parse_special_values(value: &Value) -> Vec<SpecialValue> {
    value
        .get("SpecialValues")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .map(|s| SpecialValue {
                    label: str_field(s, "Label"),
                    suffix: str_field(s, "Suffix"),
                    value: f32_field(s, "Value").unwrap_or(0.0),
                    display_type: str_field(s, "DisplayType"),
                    skip_if_zero: bool_field(s, "SkipIfZero").unwrap_or(false),
                    attributes_that_delta_base: str_array_field(s, "AttributesThatDeltaBase"),
                    attributes_that_delta: str_array_field(s, "AttributesThatDelta"),
                    attributes_that_mod: str_array_field(s, "AttributesThatMod"),
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

fn bool_field(value: &Value, key: &str) -> Option<bool> {
    value.get(key)?.as_bool()
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
    use serde_json::json;

    /// HealthSpecificDamage / ArmorSpecificDamage parse into their own typed fields (the plain
    /// `Damage` field stays empty when absent) and don't leak into `extra`.
    #[test]
    fn specific_damage_fields_are_typed() {
        let pve = json!({
            "HealthSpecificDamage": 369,
            "ArmorSpecificDamage": 45,
            "AttributesThatDeltaDamage": ["BOOST_ABILITY_MINDWORM"],
            "DoTs": [{ "DamagePerTick": 84, "NumTicks": 4, "DamageType": "Psychic" }],
        });
        let stats = parse_combat_stats(&pve);
        assert_eq!(stats.damage, None);
        assert_eq!(stats.health_specific_damage, Some(369.0));
        assert_eq!(stats.armor_specific_damage, Some(45.0));
        assert_eq!(stats.dots.len(), 1);
        assert!(
            stats.extra.get("HealthSpecificDamage").is_none(),
            "HSD should be typed, not in extra"
        );
        assert!(
            stats.extra.get("ArmorSpecificDamage").is_none(),
            "ASD should be typed, not in extra"
        );
    }

    /// When plain `Damage` and a specific-damage component coexist (e.g. Finishing Blow =
    /// Damage + Health, Acid Spew = Damage + Armor), each is preserved as its own field so both
    /// hits can be displayed.
    #[test]
    fn plain_and_specific_damage_coexist() {
        let pve = json!({ "Damage": 10, "HealthSpecificDamage": 50, "ArmorSpecificDamage": 10 });
        let stats = parse_combat_stats(&pve);
        assert_eq!(stats.damage, Some(10.0));
        assert_eq!(stats.health_specific_damage, Some(50.0));
        assert_eq!(stats.armor_specific_damage, Some(10.0));
    }
}
