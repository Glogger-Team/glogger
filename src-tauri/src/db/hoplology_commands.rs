use super::DbPool;
use serde::Serialize;
use tauri::State;

// ── Output types ────────────────────────────────────────────────────────────

#[derive(Serialize, specta::Type)]
pub struct HoplologyStudy {
    #[specta(type = u32)]
    pub id: i64,
    pub character_name: String,
    pub server_name: String,
    pub item_name: String,
    pub studied_at: String,
    pub source: String,
}

#[derive(Serialize, specta::Type)]
pub struct HoplologyStats {
    pub total_studied: u32,
    pub last_studied_at: Option<String>,
    pub last_studied_item: Option<String>,
}

// ── Commands ────────────────────────────────────────────────────────────────

#[tauri::command]
pub fn get_hoplology_studies(
    db: State<'_, DbPool>,
    character_name: String,
    server_name: String,
) -> Result<Vec<HoplologyStudy>, String> {
    let conn = db.get().map_err(|e| e.to_string())?;
    let mut stmt = conn
        .prepare(
            "SELECT id, character_name, server_name, item_name, studied_at, source
             FROM hoplology_studies
             WHERE character_name = ?1 AND server_name = ?2
             ORDER BY studied_at DESC",
        )
        .map_err(|e| e.to_string())?;

    let rows = stmt
        .query_map([&character_name, &server_name], |row| {
            Ok(HoplologyStudy {
                id: row.get(0)?,
                character_name: row.get(1)?,
                server_name: row.get(2)?,
                item_name: row.get(3)?,
                studied_at: row.get(4)?,
                source: row.get(5)?,
            })
        })
        .map_err(|e| e.to_string())?;

    let mut studies = Vec::new();
    for row in rows {
        studies.push(row.map_err(|e| e.to_string())?);
    }
    Ok(studies)
}

#[tauri::command]
pub fn get_hoplology_stats(
    db: State<'_, DbPool>,
    character_name: String,
    server_name: String,
) -> Result<HoplologyStats, String> {
    let conn = db.get().map_err(|e| e.to_string())?;

    let total_studied: u32 = conn
        .query_row(
            "SELECT COUNT(*) FROM hoplology_studies WHERE character_name = ?1 AND server_name = ?2",
            [&character_name, &server_name],
            |row| row.get(0),
        )
        .map_err(|e| e.to_string())?;

    // Only use live study events (source='chat') for cooldown tracking.
    // Report backfills (source='report') are not actual study actions.
    let last = conn
        .query_row(
            "SELECT studied_at, item_name FROM hoplology_studies
             WHERE character_name = ?1 AND server_name = ?2 AND source = 'chat'
             ORDER BY studied_at DESC LIMIT 1",
            [&character_name, &server_name],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        )
        .ok();

    Ok(HoplologyStats {
        total_studied,
        last_studied_at: last.as_ref().map(|(ts, _)| ts.clone()),
        last_studied_item: last.map(|(_, name)| name),
    })
}

// ── Internal helpers (called from coordinator, not Tauri commands) ──────────

/// Insert a hoplology study from a live chat event (source='chat').
/// If the item was previously known only from a report, upgrades the
/// source to 'chat' and updates the timestamp.
pub fn insert_hoplology_study_from_chat(
    conn: &rusqlite::Connection,
    character_name: &str,
    server_name: &str,
    item_name: &str,
    studied_at: &str,
) -> Result<Option<i64>, rusqlite::Error> {
    // Try inserting; on conflict update source/timestamp if the existing row
    // was only from a report — a live study is more authoritative.
    let changed = conn.execute(
        "INSERT INTO hoplology_studies (character_name, server_name, item_name, studied_at, source)
         VALUES (?1, ?2, ?3, ?4, 'chat')
         ON CONFLICT(character_name, server_name, item_name) DO UPDATE SET
             studied_at = excluded.studied_at,
             source = 'chat'
         WHERE source = 'report'",
        rusqlite::params![character_name, server_name, item_name, studied_at],
    )?;

    if changed > 0 {
        Ok(Some(conn.last_insert_rowid()))
    } else {
        Ok(None) // Already known from a live study
    }
}

/// Insert a hoplology study from a skill report (source='report').
/// Only inserts if the item is not already known at all.
pub fn insert_hoplology_study_from_report(
    conn: &rusqlite::Connection,
    character_name: &str,
    server_name: &str,
    item_name: &str,
    studied_at: &str,
) -> Result<Option<i64>, rusqlite::Error> {
    let changed = conn.execute(
        "INSERT OR IGNORE INTO hoplology_studies (character_name, server_name, item_name, studied_at, source)
         VALUES (?1, ?2, ?3, ?4, 'report')",
        rusqlite::params![character_name, server_name, item_name, studied_at],
    )?;

    if changed > 0 {
        Ok(Some(conn.last_insert_rowid()))
    } else {
        Ok(None) // Already known
    }
}

// ── Specta schema export (TODO.md:113 shared schema contract) ───────────────
// `hoplology` IPC payload shapes derive `specta::Type` so
// `src/types/hoplology.specta.ts` can be generated from them. Test-only plumbing:
// the exporters live in `[dev-dependencies]`, so no new runtime dependency
// ships beyond `specta` itself, and this fn compiles out of non-test builds.
// Regenerate with `OVERWRITE_BINDINGS=1 cargo test hoplology_specta`.
#[cfg(test)]
pub fn export_bindings() -> Result<String, Box<dyn std::error::Error>> {
    let types = specta::Types::default()
        .register::<HoplologyStudy>()
        .register::<HoplologyStats>()
        ;
    Ok(specta_typescript::Typescript::default().export(&types, specta_serde::Format)?)
}

#[cfg(test)]
mod specta_bindings_tests {
    use super::export_bindings;

    /// Freshness pin: the checked-in `src/types/hoplology.specta.ts` must equal
    /// what `export_bindings` derives from the `hoplology` payload shapes. Any
    /// shape change (field add/remove/rename, serde attribute) fails here until the
    /// bindings are regenerated — that failure IS the drift signal. Regenerate with
    /// `OVERWRITE_BINDINGS=1 cargo test hoplology_specta`, inspect the diff, and
    /// confirm it matches the hand-mirrored frontend types before committing.
    #[test]
    fn test_hoplology_specta_bindings_are_fresh() {
        let fresh = export_bindings().expect("specta export failed");
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../src/types/hoplology.specta.ts",
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