use super::parse_string_map;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ── Raw CDN shapes ────────────────────────────────────────────────────────────

#[allow(dead_code)]
#[derive(Debug, Deserialize, Clone)]
pub struct RawAiInfo {
    #[serde(flatten)]
    pub extra: serde_json::Value,
}

// ── Parsed structs (app shape) ───────────────────────────────────────────────

#[derive(Debug, Serialize, Clone, Default)]
pub struct AiInfo {
    pub raw: serde_json::Value,
}

/// Lightweight summary sent to the frontend for enemy list display.
#[derive(Debug, Serialize, Clone)]
pub struct AiSummary {
    pub key: String,
    pub strategy: Option<String>,
    pub mobility_type: Option<String>,
    pub comment: Option<String>,
    pub swimming: bool,
    pub uncontrolled_pet: bool,
    pub ability_count: usize,
    pub ability_names: Vec<String>,
}

impl AiInfo {
    pub fn to_summary(&self, key: &str) -> AiSummary {
        let strategy = self
            .raw
            .get("Strategy")
            .and_then(|v| v.as_str())
            .map(String::from);
        let mobility_type = self
            .raw
            .get("MobilityType")
            .and_then(|v| v.as_str())
            .map(String::from);
        let comment = self
            .raw
            .get("Comment")
            .and_then(|v| v.as_str())
            .map(String::from);
        let swimming = self
            .raw
            .get("Swimming")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let uncontrolled_pet = self
            .raw
            .get("UncontrolledPet")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let abilities = self.raw.get("Abilities").and_then(|v| v.as_object());
        let ability_count = abilities.map(|m| m.len()).unwrap_or(0);
        let mut ability_names: Vec<String> = abilities
            .map(|m| m.keys().cloned().collect())
            .unwrap_or_default();
        ability_names.sort();

        AiSummary {
            key: key.to_string(),
            strategy,
            mobility_type,
            comment,
            swimming,
            uncontrolled_pet,
            ability_count,
            ability_names,
        }
    }
}

// ── Parse function ───────────────────────────────────────────────────────────

pub fn parse(json: &str) -> Result<HashMap<String, AiInfo>, String> {
    let raw: HashMap<String, serde_json::Value> = parse_string_map(json, "ai.json")?;
    Ok(raw
        .into_iter()
        .map(|(k, v)| (k, AiInfo { raw: v }))
        .collect())
}
