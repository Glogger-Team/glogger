use serde::Serialize;
use serde_json::Value;
use std::collections::HashMap;

// ── Parsed structs (app shape) ───────────────────────────────────────────────

/// A single skill definition.
#[derive(Debug, Serialize, Clone, specta::Type)]
pub struct SkillInfo {
    pub id: u32,
    pub name: String,
    pub internal_name: String,
    pub description: Option<String>,
    pub icon_id: Option<u32>,
    pub xp_table: Option<String>,
    pub keywords: Vec<String>,

    // Phase 2 typed fields
    pub combat: Option<bool>,
    pub max_bonus_levels: Option<u32>,
    #[specta(skip)]
    pub parents: Vec<Value>,
    pub advancement_table: Option<String>,
    pub guest_level_cap: Option<u32>,
    pub hide_when_zero: Option<bool>,
    #[specta(skip)]
    pub advancement_hints: Option<Value>,
    #[specta(skip)]
    pub rewards: Option<Value>,
    #[specta(skip)]
    pub reports: Option<Vec<Value>>,

    // Full raw JSON
    #[specta(skip)]
    pub raw_json: Value,
}

// ── Parse function ───────────────────────────────────────────────────────────

pub fn parse(json: &str) -> Result<HashMap<u32, SkillInfo>, String> {
    // Skills.json uses skill names as keys (e.g., "Alchemy", "Cooking")
    // Each skill has an "Id" field inside
    let raw: HashMap<String, Value> = serde_json::from_str(json).map_err(|e| {
        format!(
            "skills.json: parse error at line {}, col {}: {e}",
            e.line(),
            e.column()
        )
    })?;

    let mut skills = HashMap::with_capacity(raw.len());
    let mut skipped = 0;

    for (skill_name, value) in raw {
        let id = match value.get("Id").and_then(|v| v.as_u64()) {
            Some(id) => id as u32,
            None => {
                skipped += 1;
                continue;
            }
        };

        skills.insert(
            id,
            SkillInfo {
                id,
                name: str_field(&value, "Name").unwrap_or_else(|| skill_name.clone()),
                internal_name: skill_name,
                description: str_field(&value, "Description"),
                icon_id: u32_field(&value, "IconId"),
                xp_table: str_field(&value, "XpTable"),
                keywords: str_array_field(&value, "Keywords"),

                // Phase 2 typed fields
                combat: bool_field(&value, "Combat"),
                max_bonus_levels: u32_field(&value, "MaxBonusLevels"),
                parents: value
                    .get("Parents")
                    .and_then(|v| v.as_array().cloned())
                    .unwrap_or_default(),
                advancement_table: str_field(&value, "AdvancementTable"),
                guest_level_cap: u32_field(&value, "GuestLevelCap"),
                hide_when_zero: bool_field(&value, "HideWhenZero"),
                advancement_hints: value.get("AdvancementHints").cloned(),
                rewards: value.get("Rewards").cloned(),
                reports: value.get("Reports").and_then(|v| v.as_array().cloned()),

                raw_json: value,
            },
        );
    }

    if skipped > 0 {
        eprintln!("skills.json: Warning: skipped {skipped} entries without valid Id");
    }
    Ok(skills)
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
