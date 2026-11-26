use super::DbPool;
use serde::Serialize;
use tauri::State;

#[derive(Debug, Clone, Serialize, specta::Type)]
pub struct CharacterDeath {
    #[specta(type = u32)]
    pub id: i64,
    pub character_name: String,
    pub server_name: String,
    pub died_at: String,
    pub killer_name: String,
    pub killer_entity_id: Option<String>,
    pub killing_ability: String,
    #[specta(type = u32)]
    pub health_damage: i64,
    #[specta(type = u32)]
    pub armor_damage: i64,
    pub area: Option<String>,
    pub damage_type: Option<String>,
}

#[tauri::command]
pub fn get_character_deaths(
    db: State<'_, DbPool>,
    character_name: String,
    server_name: String,
) -> Result<Vec<CharacterDeath>, String> {
    let conn = db
        .get()
        .map_err(|e| format!("Database connection error: {e}"))?;

    let mut stmt = conn
        .prepare(
            "SELECT id, character_name, server_name, died_at, killer_name,
                    killer_entity_id, killing_ability, health_damage, armor_damage,
                    area, damage_type
             FROM character_deaths
             WHERE character_name = ?1 AND server_name = ?2
             ORDER BY died_at DESC",
        )
        .map_err(|e| format!("Failed to prepare query: {e}"))?;

    let rows = stmt
        .query_map(rusqlite::params![character_name, server_name], |row| {
            Ok(CharacterDeath {
                id: row.get(0)?,
                character_name: row.get(1)?,
                server_name: row.get(2)?,
                died_at: row.get(3)?,
                killer_name: row.get(4)?,
                killer_entity_id: row.get(5)?,
                killing_ability: row.get(6)?,
                health_damage: row.get(7)?,
                armor_damage: row.get(8)?,
                area: row.get(9)?,
                damage_type: row.get(10)?,
            })
        })
        .map_err(|e| format!("Query failed: {e}"))?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("Failed to read results: {e}"))
}

#[derive(Debug, Clone, Serialize, specta::Type)]
pub struct DeathDamageSource {
    #[specta(type = u32)]
    pub id: i64,
    #[specta(type = u32)]
    pub death_id: i64,
    #[specta(type = u32)]
    pub event_order: i64,
    pub timestamp: String,
    pub attacker_name: String,
    pub attacker_entity_id: Option<String>,
    pub ability_name: String,
    #[specta(type = u32)]
    pub health_damage: i64,
    #[specta(type = u32)]
    pub armor_damage: i64,
    pub is_crit: bool,
}

#[tauri::command]
pub fn get_death_damage_sources(
    db: State<'_, DbPool>,
    death_id: i64,
) -> Result<Vec<DeathDamageSource>, String> {
    let conn = db
        .get()
        .map_err(|e| format!("Database connection error: {e}"))?;

    let mut stmt = conn
        .prepare(
            "SELECT id, death_id, event_order, timestamp, attacker_name,
                    attacker_entity_id, ability_name, health_damage, armor_damage, is_crit
             FROM death_damage_sources
             WHERE death_id = ?1
             ORDER BY event_order ASC",
        )
        .map_err(|e| format!("Failed to prepare query: {e}"))?;

    let rows = stmt
        .query_map(rusqlite::params![death_id], |row| {
            Ok(DeathDamageSource {
                id: row.get(0)?,
                death_id: row.get(1)?,
                event_order: row.get(2)?,
                timestamp: row.get(3)?,
                attacker_name: row.get(4)?,
                attacker_entity_id: row.get(5)?,
                ability_name: row.get(6)?,
                health_damage: row.get(7)?,
                armor_damage: row.get(8)?,
                is_crit: row.get(9)?,
            })
        })
        .map_err(|e| format!("Query failed: {e}"))?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("Failed to read results: {e}"))
}

// ── Specta schema export (TODO.md:113 shared schema contract) ───────────────
// `death` IPC payload shapes derive `specta::Type` so
// `src/types/death.specta.ts` can be generated from them. Test-only plumbing:
// the exporters live in `[dev-dependencies]`, so no new runtime dependency
// ships beyond `specta` itself, and this fn compiles out of non-test builds.
// Regenerate with `OVERWRITE_BINDINGS=1 cargo test death_specta`.
#[cfg(test)]
pub fn export_bindings() -> Result<String, Box<dyn std::error::Error>> {
    let types = specta::Types::default()
        .register::<CharacterDeath>()
        .register::<DeathDamageSource>()
        ;
    Ok(specta_typescript::Typescript::default().export(&types, specta_serde::Format)?)
}

#[cfg(test)]
mod specta_bindings_tests {
    use super::export_bindings;

    /// Freshness pin: the checked-in `src/types/death.specta.ts` must equal
    /// what `export_bindings` derives from the `death` payload shapes. Any
    /// shape change (field add/remove/rename, serde attribute) fails here until the
    /// bindings are regenerated — that failure IS the drift signal. Regenerate with
    /// `OVERWRITE_BINDINGS=1 cargo test death_specta`, inspect the diff, and
    /// confirm it matches the hand-mirrored frontend types before committing.
    #[test]
    fn test_death_specta_bindings_are_fresh() {
        let fresh = export_bindings().expect("specta export failed");
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../src/types/death.specta.ts",
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