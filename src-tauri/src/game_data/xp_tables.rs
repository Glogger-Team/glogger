use serde::Serialize;
use serde_json::Value;
use std::collections::HashMap;

// ── Parsed structs (app shape) ───────────────────────────────────────────────

#[derive(Debug, Serialize, Clone)]
pub struct XpTableInfo {
    pub internal_name: Option<String>,
    pub xp_amounts: Vec<u64>,

    // Full raw JSON
    pub raw_json: Value,
}

// ── Parse function ───────────────────────────────────────────────────────────

pub fn parse(json: &str) -> Result<HashMap<u32, XpTableInfo>, String> {
    let raw: HashMap<String, Value> = serde_json::from_str(json).map_err(|e| {
        format!(
            "xptables.json: parse error at line {}, col {}: {e}",
            e.line(),
            e.column()
        )
    })?;

    let mut tables = HashMap::with_capacity(raw.len());
    let mut skipped = 0;

    for (key, value) in raw {
        let id_str = match key.rsplit('_').next() {
            Some(s) => s.to_string(),
            None => {
                skipped += 1;
                continue;
            }
        };
        let id: u32 = match id_str.parse() {
            Ok(id) => id,
            Err(_) => {
                skipped += 1;
                continue;
            }
        };

        let internal_name = value
            .get("InternalName")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let xp_amounts = value
            .get("XpAmounts")
            .and_then(|v| v.as_array())
            .map(|arr| arr.iter().filter_map(|v| v.as_u64()).collect())
            .unwrap_or_default();

        tables.insert(
            id,
            XpTableInfo {
                internal_name,
                xp_amounts,
                raw_json: value,
            },
        );
    }

    if skipped > 0 {
        eprintln!("xptables.json: Warning: skipped {skipped} entries with invalid keys");
    }

    Ok(tables)
}
