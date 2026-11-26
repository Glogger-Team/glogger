use super::parse_id_map;
use serde::Serialize;
use std::collections::HashMap;

// ── Parsed structs (app shape) ───────────────────────────────────────────────

#[derive(Debug, Serialize, Clone, specta::Type)]
pub struct EffectInfo {
    pub id: u32,
    pub name: Option<String>,
    pub desc: Option<String>,
    pub icon_id: Option<u32>,
    pub display_mode: Option<String>,
    #[specta(skip)]
    pub duration: Option<serde_json::Value>,
    pub keywords: Vec<String>,
    pub ability_keywords: Vec<String>,
    pub stacking_type: Option<String>,
    pub stacking_priority: Option<u32>,
    pub particle: Option<String>,
    #[specta(skip)]
    pub raw_json: serde_json::Value,
}

// ── Parse function ───────────────────────────────────────────────────────────

pub fn parse(json: &str) -> Result<HashMap<u32, EffectInfo>, String> {
    let raw: HashMap<u32, serde_json::Value> = parse_id_map(json, "effects.json")?;
    Ok(raw
        .into_iter()
        .map(|(id, value)| {
            let info = EffectInfo {
                id,
                name: str_field(&value, "Name"),
                desc: str_field(&value, "Desc"),
                icon_id: u32_field(&value, "IconId"),
                display_mode: str_field(&value, "DisplayMode"),
                duration: value.get("Duration").cloned(),
                keywords: str_array_field(&value, "Keywords"),
                ability_keywords: str_array_field(&value, "AbilityKeywords"),
                stacking_type: str_field(&value, "StackingType"),
                stacking_priority: u32_field(&value, "StackingPriority"),
                particle: str_field(&value, "Particle"),
                raw_json: value,
            };
            (id, info)
        })
        .collect())
}

// ── Helpers ─────────────────────────────────────────────────────────────────

fn str_field(v: &serde_json::Value, key: &str) -> Option<String> {
    v.get(key).and_then(|x| x.as_str()).map(|s| s.to_string())
}

fn u32_field(v: &serde_json::Value, key: &str) -> Option<u32> {
    v.get(key).and_then(|x| x.as_u64()).map(|n| n as u32)
}

fn str_array_field(v: &serde_json::Value, key: &str) -> Vec<String> {
    v.get(key)
        .and_then(|x| x.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|x| x.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default()
}
