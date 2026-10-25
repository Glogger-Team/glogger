use super::DbPool;
use serde::Serialize;
use tauri::State;

#[derive(Serialize)]
pub struct PlayerMessageRow {
    pub id: i64,
    pub character_name: String,
    pub server_name: String,
    pub timestamp: String,
    pub message_type: String,
    pub direction: String,
    pub other_player: String,
    pub body: String,
    pub item_name: Option<String>,
}

#[tauri::command(rename_all = "camelCase")]
pub fn get_player_messages(
    db: State<'_, DbPool>,
    character_name: String,
    server_name: String,
    limit: Option<i32>,
) -> Result<Vec<PlayerMessageRow>, String> {
    let conn = db.get().map_err(|e| e.to_string())?;
    let limit = limit.unwrap_or(50);

    let mut stmt = conn
        .prepare(
            "SELECT id, character_name, server_name, timestamp, message_type, direction, other_player, body, item_name
             FROM player_messages
             WHERE character_name = ?1 AND server_name = ?2
             ORDER BY timestamp DESC
             LIMIT ?3",
        )
        .map_err(|e| e.to_string())?;

    let rows = stmt
        .query_map(
            rusqlite::params![&character_name, &server_name, limit],
            |row| {
                Ok(PlayerMessageRow {
                    id: row.get(0)?,
                    character_name: row.get(1)?,
                    server_name: row.get(2)?,
                    timestamp: row.get(3)?,
                    message_type: row.get(4)?,
                    direction: row.get(5)?,
                    other_player: row.get(6)?,
                    body: row.get(7)?,
                    item_name: row.get(8)?,
                })
            },
        )
        .map_err(|e| e.to_string())?;

    let mut messages = Vec::new();
    for row in rows {
        messages.push(row.map_err(|e| e.to_string())?);
    }
    Ok(messages)
}
