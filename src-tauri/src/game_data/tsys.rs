use serde::Serialize;
use serde_json::Value;
use std::collections::HashMap;

// ── Parsed structs (app shape) ───────────────────────────────────────────────

/// A single tier within a TSys power.
#[derive(Debug, Serialize, Clone, specta::Type)]
pub struct TsysTierInfo {
    pub effect_descs: Vec<String>,
    pub min_level: Option<u32>,
    pub max_level: Option<u32>,
    pub min_rarity: Option<String>,
    pub skill_level_prereq: Option<u32>,
}

/// A single TSys (crafting system) client info entry.
#[derive(Debug, Serialize, Clone)]
pub struct TsysClientInfo {
    pub internal_name: Option<String>,
    pub skill: Option<String>,
    pub slots: Vec<String>,
    pub prefix: Option<String>,
    pub suffix: Option<String>,
    pub tiers: HashMap<String, TsysTierInfo>,
    pub is_unavailable: Option<bool>,
    pub is_hidden_from_transmutation: Option<bool>,

    // Full raw JSON
    pub raw_json: Value,
}

/// Aggregated TSys data from multiple CDN files.
#[derive(Debug, Serialize, Clone, Default)]
pub struct TsysData {
    pub client_info: HashMap<String, TsysClientInfo>,
    pub profiles: Value,
}

impl TsysData {
    pub fn empty() -> Self {
        Self {
            client_info: HashMap::new(),
            profiles: Value::Null,
        }
    }

    pub fn parse(client_info_json: &str, profiles_json: &str) -> Result<Self, String> {
        let raw_client: HashMap<String, Value> = serde_json::from_str(client_info_json)
            .map_err(|e| format!("tsysclientinfo.json: {e}"))?;

        let client_info: HashMap<String, TsysClientInfo> = raw_client
            .into_iter()
            .map(|(key, value)| {
                let info = TsysClientInfo {
                    internal_name: str_field(&value, "InternalName"),
                    skill: str_field(&value, "Skill"),
                    slots: str_array_field(&value, "Slots"),
                    prefix: str_field(&value, "Prefix"),
                    suffix: str_field(&value, "Suffix"),
                    tiers: parse_tiers(value.get("Tiers")),
                    is_unavailable: bool_field(&value, "IsUnavailable"),
                    is_hidden_from_transmutation: bool_field(&value, "IsHiddenFromTransmutation"),
                    raw_json: value,
                };
                (key, info)
            })
            .collect();

        let profiles: Value =
            serde_json::from_str(profiles_json).map_err(|e| format!("tsysprofiles.json: {e}"))?;

        Ok(Self {
            client_info,
            profiles,
        })
    }
}

// ── Tier parsing ────────────────────────────────────────────────────────────

fn parse_tiers(value: Option<&Value>) -> HashMap<String, TsysTierInfo> {
    let Some(obj) = value.and_then(|v| v.as_object()) else {
        return HashMap::new();
    };

    obj.iter()
        .map(|(key, tier)| {
            let info = TsysTierInfo {
                effect_descs: str_array_field(tier, "EffectDescs"),
                min_level: u32_field(tier, "MinLevel"),
                max_level: u32_field(tier, "MaxLevel"),
                min_rarity: str_field(tier, "MinRarity"),
                skill_level_prereq: u32_field(tier, "SkillLevelPrereq"),
            };
            (key.clone(), info)
        })
        .collect()
}

// ── Field extraction helpers ─────────────────────────────────────────────────

fn str_field(value: &Value, key: &str) -> Option<String> {
    value.get(key)?.as_str().map(|s| s.to_string())
}

fn u32_field(value: &Value, key: &str) -> Option<u32> {
    value.get(key)?.as_u64().map(|n| n as u32)
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
