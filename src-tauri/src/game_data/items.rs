use serde::Serialize;
use serde_json::Value;
use std::collections::HashMap;

// ── Survey kind classifier ───────────────────────────────────────────────────
//
// See docs/architecture/survey-mechanics.md for the full taxonomy and rules.
// CDN data alone cannot distinguish Basic from Multihit — only the
// `MotherlodeMap` keyword is reliable. Everything else falls back to a
// hardcoded area table.
//
// TODO: investigate whether other CDN files (recipes, skills, areas, etc.)
// carry a signal we could use to make this dynamic. Until then, any new area
// added by the game with multihit nodes must be added here manually.

/// Areas where mining surveys produce multihit nodes (non-motherlode nodes
/// that survive multiple swings). Hardcoded — no CDN field expresses this.
const MULTIHIT_AREAS: &[&str] = &["Povus", "Vidaria"];

/// The three behaviorally distinct survey-map kinds. See
/// docs/architecture/survey-mechanics.md for full mechanics per kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SurveyKind {
    /// Single-use map that drops loot immediately on collection. Speed bonus
    /// rewards (only this kind has them) appear when used quickly enough.
    Basic,
    /// Single-use map that spawns a one-hit motherlode node. Loot comes from
    /// the mining hit, not from using the map. Identified by the
    /// `MotherlodeMap` keyword in CDN.
    Motherlode,
    /// Single-use map that spawns a multi-swing node. Loot drips across
    /// 2–20 mining cycles. Identified by area (Povus/Vidaria currently).
    Multihit,
}

// ── Parsed structs (app shape) ───────────────────────────────────────────────

/// A single item definition, suitable for serialising to the frontend.
/// Contains typed fields for commonly-used data, plus the full raw JSON
/// so no CDN data is ever lost.
#[derive(Debug, Serialize, Clone)]
pub struct ItemInfo {
    pub id: u32,
    pub name: String,
    pub description: Option<String>,
    pub icon_id: Option<u32>,
    pub value: Option<f32>,
    pub max_stack_size: Option<f32>,
    pub keywords: Vec<String>,
    pub effect_descs: Vec<String>,

    // ── New typed fields (Phase 1) ──────────────────────────────────────
    pub internal_name: Option<String>,
    pub food_desc: Option<String>,
    pub equip_slot: Option<String>,
    pub num_uses: Option<u32>,
    pub skill_reqs: Option<Value>,
    pub behaviors: Option<Vec<Value>>,
    pub bestow_recipes: Option<Vec<Value>>,
    pub bestow_ability: Option<String>,
    pub bestow_quest: Option<String>,
    pub bestow_title: Option<u32>,
    pub craft_points: Option<u32>,
    pub crafting_target_level: Option<u32>,
    pub tsys_profile: Option<String>,

    // ── Full raw JSON (source of truth) ─────────────────────────────────
    pub raw_json: Value,
}

impl ItemInfo {
    /// Classify this item as a survey map kind, or `None` if it isn't a
    /// survey/motherlode map. See `SurveyKind` and
    /// docs/architecture/survey-mechanics.md.
    ///
    /// Classification precedence:
    /// 1. `MotherlodeMap` keyword → `Motherlode`
    /// 2. Has `MineralSurvey` or `MiningSurvey` keyword AND area is in
    ///    `MULTIHIT_AREAS` → `Multihit`
    /// 3. Has `MineralSurvey` or `MiningSurvey` keyword → `Basic`
    /// 4. Otherwise → `None` (not a survey map)
    pub fn survey_kind(&self) -> Option<SurveyKind> {
        let is_survey = self
            .keywords
            .iter()
            .any(|k| k == "MineralSurvey" || k == "MiningSurvey" || k == "MotherlodeMap");
        if !is_survey {
            return None;
        }

        if self.keywords.iter().any(|k| k == "MotherlodeMap") {
            return Some(SurveyKind::Motherlode);
        }

        // Basic vs Multihit hinges on area, which must be parsed from
        // internal_name. Display name carries area too but internal_name is
        // canonical and stable across game versions.
        let area = self
            .internal_name
            .as_deref()
            .and_then(parse_survey_area_from_internal_name);
        match area {
            Some(a) if MULTIHIT_AREAS.contains(&a.as_str()) => Some(SurveyKind::Multihit),
            _ => Some(SurveyKind::Basic),
        }
    }
}

/// Extract the area token from a survey internal name.
///
/// Examples:
///   `GeologySurveyEltibule2` → `"Eltibule"`
///   `MiningSurveySouthSerbule1X` → `"SouthSerbule"`
///   `GeologySurveyKurMountains3` → `"KurMountains"`
///   `MiningSurveyPovus7Y` → `"Povus"`
///
/// Returns `None` if the name doesn't follow the expected `(GeologySurvey|MiningSurvey)<Area><digits><suffix?>`
/// pattern.
fn parse_survey_area_from_internal_name(internal: &str) -> Option<String> {
    let rest = internal
        .strip_prefix("GeologySurvey")
        .or_else(|| internal.strip_prefix("MiningSurvey"))?;
    // Area is everything before the trailing digit(s) and optional X/Y suffix.
    // Find the first ASCII digit; everything before it is the area.
    let digit_idx = rest.find(|c: char| c.is_ascii_digit())?;
    if digit_idx == 0 {
        return None;
    }
    Some(rest[..digit_idx].to_string())
}

// ── Parse function ───────────────────────────────────────────────────────────

pub fn parse(json: &str) -> Result<HashMap<u32, ItemInfo>, String> {
    let raw: HashMap<String, Value> = serde_json::from_str(json).map_err(|e| {
        format!(
            "items.json: parse error at line {}, col {}: {e}",
            e.line(),
            e.column()
        )
    })?;

    let mut items = HashMap::with_capacity(raw.len());
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

        let item = ItemInfo {
            id,
            name: str_field(&value, "Name").unwrap_or_else(|| format!("Unknown Item {id}")),
            description: str_field(&value, "Description"),
            icon_id: u32_field(&value, "IconId"),
            value: f32_field(&value, "Value"),
            max_stack_size: f32_field(&value, "MaxStackSize"),
            keywords: str_array_field(&value, "Keywords"),
            effect_descs: str_array_field(&value, "EffectDescs"),

            // Phase 1 typed fields
            internal_name: str_field(&value, "InternalName"),
            food_desc: str_field(&value, "FoodDesc"),
            equip_slot: str_field(&value, "EquipSlot"),
            num_uses: u32_field(&value, "NumUses"),
            skill_reqs: value.get("SkillReqs").cloned(),
            behaviors: value.get("Behaviors").and_then(|v| v.as_array().cloned()),
            bestow_recipes: value
                .get("BestowRecipes")
                .and_then(|v| v.as_array().cloned()),
            bestow_ability: str_field(&value, "BestowAbility"),
            bestow_quest: str_field(&value, "BestowQuest"),
            bestow_title: u32_field(&value, "BestowTitle"),
            craft_points: u32_field(&value, "CraftPoints"),
            crafting_target_level: u32_field(&value, "CraftingTargetLevel"),
            tsys_profile: str_field(&value, "TSysProfile"),

            raw_json: value,
        };

        items.insert(id, item);
    }

    if skipped > 0 {
        eprintln!("items.json: Warning: skipped {skipped} entries with invalid keys");
    }

    Ok(items)
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

    fn make_item(internal: &str, keywords: &[&str]) -> ItemInfo {
        ItemInfo {
            id: 0,
            name: internal.to_string(),
            description: None,
            icon_id: None,
            value: None,
            max_stack_size: None,
            keywords: keywords.iter().map(|s| s.to_string()).collect(),
            effect_descs: vec![],
            internal_name: Some(internal.to_string()),
            food_desc: None,
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
            raw_json: Value::Null,
        }
    }

    #[test]
    fn test_survey_kind_motherlode_keyword_wins() {
        // Motherlode keyword takes precedence regardless of area
        let item = make_item(
            "MiningSurveyKurMountains1X",
            &["Document", "MiningSurvey", "MotherlodeMap"],
        );
        assert_eq!(item.survey_kind(), Some(SurveyKind::Motherlode));
    }

    #[test]
    fn test_survey_kind_basic_geology_serbule() {
        let item = make_item("GeologySurveySerbule1", &["Document", "MineralSurvey"]);
        assert_eq!(item.survey_kind(), Some(SurveyKind::Basic));
    }

    #[test]
    fn test_survey_kind_basic_eltibule_mining() {
        // Eltibule mining surveys are Basic despite the MiningSurvey keyword
        let item = make_item("MiningSurveyEltibule3", &["Document", "MiningSurvey"]);
        assert_eq!(item.survey_kind(), Some(SurveyKind::Basic));
    }

    #[test]
    fn test_survey_kind_multihit_povus_mining() {
        let item = make_item("MiningSurveyPovus7Y", &["Document", "MiningSurvey"]);
        assert_eq!(item.survey_kind(), Some(SurveyKind::Multihit));
    }

    #[test]
    fn test_survey_kind_multihit_vidaria_geology() {
        // The user-flagged case: Vidaria has GeologySurvey-prefixed multihit maps
        let item = make_item("GeologySurveyVidaria4", &["Document", "MineralSurvey"]);
        assert_eq!(item.survey_kind(), Some(SurveyKind::Multihit));
    }

    #[test]
    fn test_survey_kind_multihit_povus_geology() {
        let item = make_item("GeologySurveyPovus1", &["Document", "MineralSurvey"]);
        assert_eq!(item.survey_kind(), Some(SurveyKind::Multihit));
    }

    #[test]
    fn test_survey_kind_none_for_non_survey() {
        let item = make_item("Phlogiston7", &["Refined"]);
        assert_eq!(item.survey_kind(), None);
    }

    #[test]
    fn test_survey_kind_none_when_no_keyword() {
        // Survey-shaped name but no keywords — defensively reject
        let item = make_item("GeologySurveySerbule1", &[]);
        assert_eq!(item.survey_kind(), None);
    }

    #[test]
    fn test_parse_area_compound_names() {
        assert_eq!(
            parse_survey_area_from_internal_name("MiningSurveySouthSerbule1X"),
            Some("SouthSerbule".to_string())
        );
        assert_eq!(
            parse_survey_area_from_internal_name("GeologySurveyKurMountains3"),
            Some("KurMountains".to_string())
        );
    }

    #[test]
    fn test_parse_area_returns_none_for_garbage() {
        assert_eq!(parse_survey_area_from_internal_name("RandomItem99"), None);
        assert_eq!(parse_survey_area_from_internal_name("MiningSurvey"), None);
    }
}
