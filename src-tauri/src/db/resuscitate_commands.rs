use super::DbPool;
use serde::Serialize;
use tauri::State;

#[derive(Debug, Clone, Serialize, specta::Type)]
pub struct CharacterResuscitation {
    #[specta(type = u32)]
    pub id: i64,
    pub character_name: String,
    pub server_name: String,
    pub occurred_at: String,
    pub caster_name: String,
    pub target_name: String,
    pub success: bool,
    pub area: Option<String>,
}

#[tauri::command]
pub fn get_character_resuscitations(
    db: State<'_, DbPool>,
    character_name: String,
    server_name: String,
) -> Result<Vec<CharacterResuscitation>, String> {
    let conn = db
        .get()
        .map_err(|e| format!("Database connection error: {e}"))?;

    let mut stmt = conn
        .prepare(
            "SELECT id, character_name, server_name, occurred_at, caster_name,
                    target_name, success, area
             FROM character_resuscitations
             WHERE character_name = ?1 AND server_name = ?2
             ORDER BY occurred_at DESC",
        )
        .map_err(|e| format!("Failed to prepare query: {e}"))?;

    let rows = stmt
        .query_map(rusqlite::params![character_name, server_name], |row| {
            Ok(CharacterResuscitation {
                id: row.get(0)?,
                character_name: row.get(1)?,
                server_name: row.get(2)?,
                occurred_at: row.get(3)?,
                caster_name: row.get(4)?,
                target_name: row.get(5)?,
                success: row.get(6)?,
                area: row.get(7)?,
            })
        })
        .map_err(|e| format!("Query failed: {e}"))?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("Failed to read results: {e}"))
}

// ── Specta schema export (TODO.md:113 shared schema contract) ───────────────
// `resuscitate` IPC payload shapes derive `specta::Type` so
// `src/types/resuscitate.specta.ts` can be generated from them. Test-only plumbing:
// the exporters live in `[dev-dependencies]`, so no new runtime dependency
// ships beyond `specta` itself, and this fn compiles out of non-test builds.
// Regenerate with `OVERWRITE_BINDINGS=1 cargo test resuscitate_specta`.
#[cfg(test)]
pub fn export_bindings() -> Result<String, Box<dyn std::error::Error>> {
    let types = specta::Types::default()
        .register::<CharacterResuscitation>()
        ;
    Ok(specta_typescript::Typescript::default().export(&types, specta_serde::Format)?)
}

#[cfg(test)]
mod specta_bindings_tests {
    use super::export_bindings;

    /// Freshness pin: the checked-in `src/types/resuscitate.specta.ts` must equal
    /// what `export_bindings` derives from the `resuscitate` payload shapes. Any
    /// shape change (field add/remove/rename, serde attribute) fails here until the
    /// bindings are regenerated — that failure IS the drift signal. Regenerate with
    /// `OVERWRITE_BINDINGS=1 cargo test resuscitate_specta`, inspect the diff, and
    /// confirm it matches the hand-mirrored frontend types before committing.
    #[test]
    fn test_resuscitate_specta_bindings_are_fresh() {
        let fresh = export_bindings().expect("specta export failed");
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../src/types/resuscitate.specta.ts",
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