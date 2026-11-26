use super::parse_string_map;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ── Raw CDN shapes ────────────────────────────────────────────────────────────

#[allow(dead_code)]
#[derive(Debug, Deserialize, Clone)]
pub struct RawQuestInfo {
    #[serde(flatten)]
    pub extra: serde_json::Value,
}

// ── Parsed structs (app shape) ───────────────────────────────────────────────

#[derive(Debug, Serialize, Clone, Default, specta::Type)]
pub struct QuestInfo {
    pub internal_name: String,
    #[specta(skip)]
    pub raw: serde_json::Value,
}

// ── Parse function ───────────────────────────────────────────────────────────

pub fn parse(json: &str) -> Result<HashMap<String, QuestInfo>, String> {
    let raw: HashMap<String, serde_json::Value> = parse_string_map(json, "quests.json")?;
    Ok(raw
        .into_iter()
        .map(|(k, v)| {
            // Use InternalName from the raw JSON if available, otherwise fall back to the key
            let iname = v
                .get("InternalName")
                .and_then(|n| n.as_str())
                .unwrap_or(&k)
                .to_string();
            let info = QuestInfo {
                internal_name: iname,
                raw: v,
            };
            (k, info)
        })
        .collect())
}
