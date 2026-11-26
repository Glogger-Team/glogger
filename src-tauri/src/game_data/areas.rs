use serde::Serialize;
use serde_json::Value;
use std::collections::HashMap;

// ── Parsed structs (app shape) ───────────────────────────────────────────────

#[derive(Debug, Serialize, Clone, specta::Type)]
pub struct AreaInfo {
    pub friendly_name: Option<String>,
    pub short_friendly_name: Option<String>,

    // Full raw JSON
    #[specta(skip)]
    pub raw_json: Value,
}

// ── Parse function ───────────────────────────────────────────────────────────

pub fn parse(json: &str) -> Result<HashMap<String, AreaInfo>, String> {
    let raw: HashMap<String, Value> = serde_json::from_str(json).map_err(|e| {
        format!(
            "areas.json: parse error at line {}, col {}: {e}",
            e.line(),
            e.column()
        )
    })?;

    Ok(raw
        .into_iter()
        .map(|(key, value)| {
            let friendly_name = value
                .get("FriendlyName")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            let short_friendly_name = value
                .get("ShortFriendlyName")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());

            (
                key,
                AreaInfo {
                    friendly_name,
                    short_friendly_name,
                    raw_json: value,
                },
            )
        })
        .collect())
}
