use serde::Serialize;
use std::collections::HashMap;

// ── Parsed structs (app shape) ───────────────────────────────────────────────

#[derive(Debug, Serialize, Clone)]
pub struct AchievementInfo {
    pub id: String,
    pub name: Option<String>,
    pub description: Option<String>,
    pub icon_type: Option<String>,
    pub raw_json: serde_json::Value,
}

// ── Parse function ───────────────────────────────────────────────────────────

// Live CDN shape (verified v487) is a JSON array of objects keyed by `id`:
// `[{"description": ..., "iconType": "Combat", "id": "Combat_Any_25",
//   "name": "Neophyte Warrior"}, ...]`. Accept a string-keyed map too so a
// future CDN re-spelling still loads.
pub fn parse(json: &str) -> Result<HashMap<String, AchievementInfo>, String> {
    let value: serde_json::Value = serde_json::from_str(json).map_err(|e| {
        format!(
            "achievements.json: parse error at line {}, col {}: {e}",
            e.line(),
            e.column()
        )
    })?;

    let entries: Vec<serde_json::Value> = match value {
        serde_json::Value::Array(arr) => arr,
        serde_json::Value::Object(map) => map.into_values().collect(),
        _ => {
            return Err("achievements.json: expected a JSON array or object".to_string());
        }
    };

    let mut out = HashMap::with_capacity(entries.len());
    for entry in entries {
        let id = entry
            .get("id")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let Some(id) = id else {
            eprintln!("achievements.json: Warning: skipped entry without an id field");
            continue;
        };
        out.insert(
            id.clone(),
            AchievementInfo {
                id,
                name: str_field(&entry, "name"),
                description: str_field(&entry, "description"),
                icon_type: str_field(&entry, "iconType"),
                raw_json: entry,
            },
        );
    }
    Ok(out)
}

// ── Helpers ─────────────────────────────────────────────────────────────────

fn str_field(v: &serde_json::Value, key: &str) -> Option<String> {
    v.get(key).and_then(|x| x.as_str()).map(|s| s.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = r#"[
        {"description": "Reach level 25 in any combat skill.", "iconType": "Combat",
         "id": "Combat_Any_25", "name": "Neophyte Warrior"},
        {"description": "Get Smote by an Admin.", "iconType": "Book",
         "id": "CauseOfDeath_AdminSmite", "name": "Administrative Overreach"}
    ]"#;

    #[test]
    fn parses_live_array_shape() {
        let parsed = parse(FIXTURE).expect("fixture must parse");
        assert_eq!(parsed.len(), 2);
        let first = &parsed["Combat_Any_25"];
        assert_eq!(first.name.as_deref(), Some("Neophyte Warrior"));
        assert_eq!(
            first.description.as_deref(),
            Some("Reach level 25 in any combat skill.")
        );
        assert_eq!(first.icon_type.as_deref(), Some("Combat"));
        assert_eq!(parsed["CauseOfDeath_AdminSmite"].icon_type.as_deref(), Some("Book"));
    }

    #[test]
    fn tolerates_map_shape() {
        let map_shape = r#"{"Combat_Any_25":
            {"description": "Reach level 25 in any combat skill.", "iconType": "Combat",
             "id": "Combat_Any_25", "name": "Neophyte Warrior"}}"#;
        let parsed = parse(map_shape).expect("map shape must parse");
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed["Combat_Any_25"].name.as_deref(), Some("Neophyte Warrior"));
    }

    #[test]
    fn missing_file_fallback_parses_empty() {
        // The loader substitutes "{}" when achievements.json is absent; that
        // must parse to an empty map rather than erroring.
        let parsed = parse("{}").expect("empty map must parse");
        assert!(parsed.is_empty());
    }

    #[test]
    fn entries_without_id_are_skipped() {
        let parsed = parse(r#"[{"name": "Nameless"}]"#).expect("must parse");
        assert!(parsed.is_empty());
    }
}
