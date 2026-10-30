use serde::Serialize;
use serde_json::Value;
use std::collections::HashMap;

// ── Parsed structs (app shape) ───────────────────────────────────────────────

/// A single source entry describing where an ability/item/recipe comes from.
#[derive(Debug, Serialize, Clone)]
pub struct SourceEntry {
    pub source_type: String,
    pub skill: Option<String>,
    pub npc: Option<String>,
    pub item_type_id: Option<u32>,
    pub quest_id: Option<u32>,
    pub recipe_id: Option<u32>,
    pub hang_out_id: Option<u32>,
    pub friendly_name: Option<String>,
    pub extra: Value,
}

/// All sources for a single entity.
#[derive(Debug, Serialize, Clone)]
pub struct SourceInfo {
    pub entries: Vec<SourceEntry>,
}

/// All CDN source data, parsed into typed maps keyed by entity ID.
#[derive(Debug, Clone, Default)]
pub struct SourcesData {
    pub abilities: HashMap<u32, SourceInfo>,
    pub items: HashMap<u32, SourceInfo>,
    pub recipes: HashMap<u32, SourceInfo>,
}

impl SourcesData {
    pub fn empty() -> Self {
        Self::default()
    }

    pub fn parse(
        abilities_json: &str,
        items_json: &str,
        recipes_json: &str,
    ) -> Result<Self, String> {
        let abilities = parse_source_map(abilities_json, "sources_abilities.json")?;
        let items = parse_source_map(items_json, "sources_items.json")?;
        let recipes = parse_source_map(recipes_json, "sources_recipes.json")?;
        Ok(Self {
            abilities,
            items,
            recipes,
        })
    }
}

// ── Parsing helpers ─────────────────────────────────────────────────────────

/// Parse a sources JSON file into a HashMap<u32, SourceInfo>.
/// Keys are like "ability_1002", "item_1", "recipe_42".
fn parse_source_map(json: &str, file_name: &str) -> Result<HashMap<u32, SourceInfo>, String> {
    let raw: HashMap<String, Value> = serde_json::from_str(json).map_err(|e| {
        format!(
            "{file_name}: parse error at line {}, col {}: {e}",
            e.line(),
            e.column()
        )
    })?;

    let mut out = HashMap::with_capacity(raw.len());
    let mut skipped = 0;

    for (key, value) in raw {
        // Extract numeric ID from keys like "ability_1002"
        let id: u32 = match key.rsplit('_').next().and_then(|s| s.parse().ok()) {
            Some(id) => id,
            None => {
                skipped += 1;
                continue;
            }
        };

        let entries = parse_source_entries(&value);
        out.insert(id, SourceInfo { entries });
    }

    if skipped > 0 {
        eprintln!("{file_name}: Warning: skipped {skipped} entries with invalid keys");
    }

    Ok(out)
}

/// Parse the "entries" array from a source value, extracting known fields.
fn parse_source_entries(value: &Value) -> Vec<SourceEntry> {
    let entries_arr = match value.get("entries").and_then(|v| v.as_array()) {
        Some(arr) => arr,
        None => return vec![],
    };

    entries_arr
        .iter()
        .map(|entry| {
            let source_type = entry
                .get("type")
                .and_then(|v| v.as_str())
                .unwrap_or("Unknown")
                .to_string();
            let skill = entry
                .get("skill")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            let npc = entry
                .get("npc")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            let item_type_id = entry
                .get("itemTypeId")
                .and_then(|v| v.as_u64())
                .map(|n| n as u32);
            let quest_id = entry
                .get("questId")
                .and_then(|v| v.as_u64())
                .map(|n| n as u32);
            let recipe_id = entry
                .get("recipeId")
                .and_then(|v| v.as_u64())
                .map(|n| n as u32);
            let hang_out_id = entry
                .get("hangOutId")
                .and_then(|v| v.as_u64())
                .map(|n| n as u32);
            let friendly_name = entry
                .get("friendlyName")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());

            // Build extra map excluding already-extracted fields
            let extra = if let Some(obj) = entry.as_object() {
                let known = [
                    "type",
                    "skill",
                    "npc",
                    "itemTypeId",
                    "questId",
                    "recipeId",
                    "hangOutId",
                    "friendlyName",
                ];
                let filtered: serde_json::Map<String, Value> = obj
                    .iter()
                    .filter(|(k, _)| !known.contains(&k.as_str()))
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect();
                Value::Object(filtered)
            } else {
                Value::Null
            };

            SourceEntry {
                source_type,
                skill,
                npc,
                item_type_id,
                quest_id,
                recipe_id,
                hang_out_id,
                friendly_name,
                extra,
            }
        })
        .collect()
}
