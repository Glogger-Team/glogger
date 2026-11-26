use super::DbPool;
use serde::Serialize;
use tauri::State;

#[derive(Serialize, specta::Type)]
pub struct PlayerMessageRow {
    #[specta(type = u32)]
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

// ── Specta schema export (TODO.md:113 shared schema contract) ───────────────
// `message` IPC payload shapes derive `specta::Type` so
// `src/types/message.specta.ts` can be generated from them. Test-only plumbing:
// the exporters live in `[dev-dependencies]`, so no new runtime dependency
// ships beyond `specta` itself, and this fn compiles out of non-test builds.
// Regenerate with `OVERWRITE_BINDINGS=1 cargo test message_specta`.
#[cfg(test)]
pub fn export_bindings() -> Result<String, Box<dyn std::error::Error>> {
    let types = specta::Types::default()
        .register::<PlayerMessageRow>()
        ;
    Ok(specta_typescript::Typescript::default().export(&types, specta_serde::Format)?)
}

#[cfg(test)]
mod specta_bindings_tests {
    use super::export_bindings;

    /// Freshness pin: the checked-in `src/types/message.specta.ts` must equal
    /// what `export_bindings` derives from the `message` payload shapes. Any
    /// shape change (field add/remove/rename, serde attribute) fails here until the
    /// bindings are regenerated — that failure IS the drift signal. Regenerate with
    /// `OVERWRITE_BINDINGS=1 cargo test message_specta`, inspect the diff, and
    /// confirm it matches the hand-mirrored frontend types before committing.
    #[test]
    fn test_message_specta_bindings_are_fresh() {
        let fresh = export_bindings().expect("specta export failed");
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../src/types/message.specta.ts",
        );
        if std::env::var("OVERWRITE_BINDINGS").is_ok() {
            std::fs::write(path, &fresh).expect("bindings write failed");
            return;
        }
        let checked_in =
            std::fs::read_to_string(path).expect("checked-in specta bindings missing");
        assert_eq!(
            checked_in, fresh,
            "specta bindings drift: regenerate with OVERWRITE_BINDINGS=1",
        );
    }
}