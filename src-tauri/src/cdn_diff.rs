/// CDN version diff logic.
///
/// Compares two versions of CDN JSON data files and produces structured diffs
/// at the key level (added/removed/changed entries) and, for changed entries,
/// at the field level (which top-level fields differ).
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::BTreeMap;
use std::path::Path;
use tokio::fs;

use crate::cdn::{DATA_FILES, TRANSLATION_FILES};

// ── Types ────────────────────────────────────────────────────────────────────

/// Per-file summary: how many entries were added, removed, or changed.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileDiffSummary {
    pub file_name: String,
    pub added_count: usize,
    pub removed_count: usize,
    pub changed_count: usize,
}

/// A single field-level change within an entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldChange {
    pub field: String,
    pub old_value: Value,
    pub new_value: Value,
}

/// Diff for a single entry (one key in a data file).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntryDiff {
    pub key: String,
    /// Human-readable label extracted from Name/InternalName if available.
    pub label: Option<String>,
    /// For changed entries, the fields that differ.
    /// For added/removed entries, this is empty (use `data` instead).
    pub field_changes: Vec<FieldChange>,
    /// The full JSON value (new for added, old for removed, new for changed).
    pub data: Option<Value>,
    /// For removed entries, the old JSON value.
    pub old_data: Option<Value>,
}

/// Full diff for a single data file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileDiff {
    pub file_name: String,
    pub added: Vec<EntryDiff>,
    pub removed: Vec<EntryDiff>,
    pub changed: Vec<EntryDiff>,
}

// ── Summary computation ──────────────────────────────────────────────────────

/// Compute diff summaries for all DATA_FILES and TRANSLATION_FILES by comparing
/// old_dir vs new_dir. Data files live directly in the dir; translation files
/// live under a `translation/` subdirectory.
pub async fn compute_summary(
    old_dir: &Path,
    new_dir: &Path,
) -> Result<Vec<FileDiffSummary>, String> {
    let mut summaries = Vec::new();

    // Data files: {dir}/{name}.json
    for &name in DATA_FILES {
        let old_path = old_dir.join(format!("{name}.json"));
        let new_path = new_dir.join(format!("{name}.json"));
        summaries.push(summarize_file(name, &old_path, &new_path).await);
    }

    // Translation string files: {dir}/translation/{name}.json
    for &name in TRANSLATION_FILES {
        let old_path = old_dir.join("translation").join(format!("{name}.json"));
        let new_path = new_dir.join("translation").join(format!("{name}.json"));
        summaries.push(summarize_file(name, &old_path, &new_path).await);
    }

    Ok(summaries)
}

/// Compute add/remove/change counts for a single JSON file pair.
async fn summarize_file(name: &str, old_path: &Path, new_path: &Path) -> FileDiffSummary {
    // If either file is missing, report zero changes
    if !old_path.exists() || !new_path.exists() {
        return FileDiffSummary {
            file_name: name.to_string(),
            added_count: 0,
            removed_count: 0,
            changed_count: 0,
        };
    }

    let (old_map, new_map) = match (
        load_json_keys(old_path).await,
        load_json_keys(new_path).await,
    ) {
        (Ok(o), Ok(n)) => (o, n),
        _ => {
            return FileDiffSummary {
                file_name: name.to_string(),
                added_count: 0,
                removed_count: 0,
                changed_count: 0,
            };
        }
    };

    let mut added = 0usize;
    let mut removed = 0usize;
    let mut changed = 0usize;

    for (key, new_val) in &new_map {
        match old_map.get(key) {
            None => added += 1,
            Some(old_val) => {
                if old_val != new_val {
                    changed += 1;
                }
            }
        }
    }

    for key in old_map.keys() {
        if !new_map.contains_key(key) {
            removed += 1;
        }
    }

    FileDiffSummary {
        file_name: name.to_string(),
        added_count: added,
        removed_count: removed,
        changed_count: changed,
    }
}

// ── File-level diff ──────────────────────────────────────────────────────────

/// Compute a detailed diff for a single data or translation file.
pub async fn compute_file_diff(
    file_name: &str,
    old_dir: &Path,
    new_dir: &Path,
) -> Result<FileDiff, String> {
    let is_translation = TRANSLATION_FILES.contains(&file_name);
    let sub = if is_translation { "translation/" } else { "" };
    let old_path = old_dir.join(format!("{sub}{file_name}.json"));
    let new_path = new_dir.join(format!("{sub}{file_name}.json"));

    let old_map = load_json_keys(&old_path).await?;
    let new_map = load_json_keys(&new_path).await?;

    let mut added = Vec::new();
    let mut removed = Vec::new();
    let mut changed = Vec::new();

    // Added and changed
    for (key, new_val) in &new_map {
        match old_map.get(key) {
            None => {
                added.push(EntryDiff {
                    key: key.clone(),
                    label: extract_label(new_val),
                    field_changes: Vec::new(),
                    data: Some(new_val.clone()),
                    old_data: None,
                });
            }
            Some(old_val) => {
                if old_val != new_val {
                    let field_changes = diff_top_level_fields(old_val, new_val);
                    changed.push(EntryDiff {
                        key: key.clone(),
                        label: extract_label(new_val),
                        field_changes,
                        data: Some(new_val.clone()),
                        old_data: Some(old_val.clone()),
                    });
                }
            }
        }
    }

    // Removed
    for (key, old_val) in &old_map {
        if !new_map.contains_key(key) {
            removed.push(EntryDiff {
                key: key.clone(),
                label: extract_label(old_val),
                field_changes: Vec::new(),
                data: None,
                old_data: Some(old_val.clone()),
            });
        }
    }

    // Sort for stable output
    added.sort_by(|a, b| a.key.cmp(&b.key));
    removed.sort_by(|a, b| a.key.cmp(&b.key));
    changed.sort_by(|a, b| a.key.cmp(&b.key));

    Ok(FileDiff {
        file_name: file_name.to_string(),
        added,
        removed,
        changed,
    })
}

// ── Helpers ──────────────────────────────────────────────────────────────────

/// Load a JSON file and return its top-level keys as a BTreeMap.
/// Most CDN data files are objects like `{ "Item_12345": { ... }, ... }`.
/// Some (abilitykeywords, abilitydynamicdots, etc.) are arrays — for those
/// we use the array index as the key.
async fn load_json_keys(path: &Path) -> Result<BTreeMap<String, Value>, String> {
    let bytes = fs::read(path)
        .await
        .map_err(|e| format!("Failed to read {}: {e}", path.display()))?;

    let val: Value = serde_json::from_slice(&bytes)
        .map_err(|e| format!("Failed to parse {}: {e}", path.display()))?;

    match val {
        Value::Object(map) => Ok(map.into_iter().collect()),
        Value::Array(arr) => Ok(arr
            .into_iter()
            .enumerate()
            .map(|(i, v)| (i.to_string(), v))
            .collect()),
        _ => Err(format!(
            "{} is neither a JSON object nor array",
            path.display()
        )),
    }
}

/// Try to extract a human-readable label from a JSON entry.
/// Checks common PG CDN fields: Name, InternalName, DisplayName.
fn extract_label(val: &Value) -> Option<String> {
    for field in &["Name", "InternalName", "DisplayName"] {
        if let Some(Value::String(s)) = val.get(field) {
            if !s.is_empty() {
                return Some(s.clone());
            }
        }
    }
    None
}

/// Compare two JSON values at the top level and return which fields differ.
fn diff_top_level_fields(old: &Value, new: &Value) -> Vec<FieldChange> {
    let empty = Map::new();
    let old_obj = old.as_object().unwrap_or(&empty);
    let new_obj = new.as_object().unwrap_or(&empty);
    let mut changes = Vec::new();

    // Fields in new that differ from old
    for (key, new_val) in new_obj {
        let old_val = old_obj.get(key).unwrap_or(&Value::Null);
        if old_val != new_val {
            changes.push(FieldChange {
                field: key.clone(),
                old_value: old_val.clone(),
                new_value: new_val.clone(),
            });
        }
    }

    // Fields removed in new
    for (key, old_val) in old_obj {
        if !new_obj.contains_key(key) {
            changes.push(FieldChange {
                field: key.clone(),
                old_value: old_val.clone(),
                new_value: Value::Null,
            });
        }
    }

    changes.sort_by(|a, b| a.field.cmp(&b.field));
    changes
}
