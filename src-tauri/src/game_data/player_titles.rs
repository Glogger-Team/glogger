use super::parse_id_map;
use serde::Serialize;
use std::collections::HashMap;

// ── Parsed structs (app shape) ───────────────────────────────────────────────

#[derive(Debug, Serialize, Clone, specta::Type)]
pub struct PlayerTitleInfo {
    pub id: u32,
    pub title: Option<String>,
    pub tooltip: Option<String>,
    pub keywords: Vec<String>,
    pub account_wide: Option<bool>,
    pub soul_wide: Option<bool>,
    #[specta(skip)]
    pub raw_json: serde_json::Value,
}

// ── Parse function ───────────────────────────────────────────────────────────

pub fn parse(json: &str) -> Result<HashMap<u32, PlayerTitleInfo>, String> {
    let raw: HashMap<u32, serde_json::Value> = parse_id_map(json, "playertitles.json")?;
    Ok(raw
        .into_iter()
        .map(|(id, value)| {
            let info = PlayerTitleInfo {
                id,
                title: str_field(&value, "Title"),
                tooltip: str_field(&value, "Tooltip"),
                keywords: str_array_field(&value, "Keywords"),
                account_wide: bool_field(&value, "AccountWide"),
                soul_wide: bool_field(&value, "SoulWide"),
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

fn bool_field(v: &serde_json::Value, key: &str) -> Option<bool> {
    v.get(key).and_then(|x| x.as_bool())
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
