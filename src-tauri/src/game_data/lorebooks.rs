use super::parse_id_map;
use serde::Serialize;
use std::collections::HashMap;

// ── Parsed structs (app shape) ───────────────────────────────────────────────

#[derive(Debug, Serialize, Clone)]
pub struct LorebookEntry {
    pub id: u32,
    pub title: Option<String>,
    pub internal_name: Option<String>,
    pub category: Option<String>,
    pub text: Option<String>,
    pub location_hint: Option<String>,
    pub keywords: Vec<String>,
    pub visibility: Option<String>,
    pub is_client_local: Option<bool>,
}

#[derive(Debug, Serialize, Clone, Default)]
pub struct LorebookCategoryInfo {
    pub key: String,
    pub title: Option<String>,
    pub sub_title: Option<String>,
    pub sort_title: Option<String>,
}

#[derive(Debug, Serialize, Clone, Default)]
pub struct LorebookData {
    pub books: HashMap<u32, LorebookEntry>,
    pub categories: HashMap<String, LorebookCategoryInfo>,
}

impl LorebookData {
    pub fn empty() -> Self {
        Self::default()
    }

    pub fn parse(books_json: &str, info_json: &str) -> Result<Self, String> {
        // Parse books
        let raw_books: HashMap<u32, serde_json::Value> =
            parse_id_map(books_json, "lorebooks.json")?;

        let books: HashMap<u32, LorebookEntry> = raw_books
            .into_iter()
            .map(|(id, value)| {
                let entry = LorebookEntry {
                    id,
                    title: str_field(&value, "Title"),
                    internal_name: str_field(&value, "InternalName"),
                    category: str_field(&value, "Category"),
                    text: str_field(&value, "Text"),
                    location_hint: str_field(&value, "LocationHint"),
                    keywords: str_array_field(&value, "Keywords"),
                    visibility: str_field(&value, "Visibility"),
                    is_client_local: bool_field(&value, "IsClientLocal"),
                };
                (id, entry)
            })
            .collect();

        // Parse categories from lorebookinfo.json
        let info: serde_json::Value =
            serde_json::from_str(info_json).map_err(|e| format!("lorebookinfo.json: {e}"))?;

        let mut categories = HashMap::new();
        if let Some(cats) = info.get("Categories").and_then(|c| c.as_object()) {
            for (key, val) in cats {
                categories.insert(
                    key.clone(),
                    LorebookCategoryInfo {
                        key: key.clone(),
                        title: str_field(val, "Title"),
                        sub_title: str_field(val, "SubTitle"),
                        sort_title: str_field(val, "SortTitle"),
                    },
                );
            }
        }

        Ok(Self { books, categories })
    }
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
