use super::DbPool;
use serde::Serialize;
use tauri::State;

/// One item gain whose provenance is unresolved: `source_kind` is
/// `'unknown'` / `'uncertain'` (Player.log path couldn't attribute it) or
/// NULL (Chat.log path, never tagged — e.g. a survey hook that didn't fire).
#[derive(Debug, Clone, Serialize)]
pub struct UnknownSourceTransaction {
    pub id: i64,
    pub timestamp: String,
    pub item_name: String,
    pub quantity: i64,
    pub context: String,
    pub source: String,
    pub source_kind: Option<String>,
    pub source_details: Option<String>,
}

fn query_unknown_source(
    conn: &impl crate::db::DbRead,
    character_name: &str,
    server_name: &str,
    limit: i64,
) -> Result<Vec<UnknownSourceTransaction>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT id, timestamp, item_name, quantity, context, source, source_kind, source_details
             FROM item_transactions
             WHERE character_name = ?1 AND server_name = ?2
               AND (source_kind = 'unknown' OR source_kind = 'uncertain' OR source_kind IS NULL)
             ORDER BY timestamp DESC
             LIMIT ?3",
        )
        .map_err(|e| format!("Failed to prepare query: {e}"))?;

    let rows = stmt
        .query_map(rusqlite::params![character_name, server_name, limit], |row| {
            Ok(UnknownSourceTransaction {
                id: row.get(0)?,
                timestamp: row.get(1)?,
                item_name: row.get(2)?,
                quantity: row.get(3)?,
                context: row.get(4)?,
                source: row.get(5)?,
                source_kind: row.get(6)?,
                source_details: row.get(7)?,
            })
        })
        .map_err(|e| format!("Query failed: {e}"))?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("Failed to read results: {e}"))
}

/// Diagnostic report: every gain for this character/server whose source is
/// unresolved, newest first. Helps find drops the provenance pipeline missed
/// (bad taxonomy, untagged chat loot) instead of silently lumping them.
#[tauri::command]
pub fn get_unknown_source_transactions(
    db: State<'_, DbPool>,
    character_name: String,
    server_name: String,
    limit: Option<i64>,
) -> Result<Vec<UnknownSourceTransaction>, String> {
    let conn = db
        .get()
        .map_err(|e| format!("Database connection error: {e}"))?;
    let limit = limit.unwrap_or(100).clamp(1, 1000);
    query_unknown_source(&conn, &character_name, &server_name, limit)
}
