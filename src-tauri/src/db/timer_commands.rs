use super::DbPool;
use serde::{Deserialize, Serialize};
use tauri::State;

// ── Output types ────────────────────────────────────────────────────────────

#[derive(Serialize, Clone, Debug, specta::Type)]
pub struct UserTimer {
    #[specta(type = u32)]
    pub id: i64,
    pub character_name: String,
    pub server_name: String,
    pub label: String,
    #[specta(type = u32)]
    pub duration_seconds: i64,
    pub started_at: String,
    pub paused_at: Option<String>,
    pub area_key: Option<String>,
    pub source: String,
}

// ── Input types ─────────────────────────────────────────────────────────────

#[derive(Deserialize, specta::Type)]
pub struct SaveTimerInput {
    #[specta(type = Option<u32>)]
    pub id: Option<i64>,
    pub label: String,
    #[specta(type = u32)]
    pub duration_seconds: i64,
    pub started_at: String,
    pub paused_at: Option<String>,
    pub area_key: Option<String>,
    pub source: Option<String>,
}

// ── Commands ────────────────────────────────────────────────────────────────

#[tauri::command]
pub fn get_user_timers(
    db: State<'_, DbPool>,
    character_name: String,
    server_name: String,
) -> Result<Vec<UserTimer>, String> {
    let conn = db.get().map_err(|e| e.to_string())?;
    let mut stmt = conn
        .prepare(
            "SELECT id, character_name, server_name, label, duration_seconds, started_at, paused_at, area_key, source
             FROM user_timers
             WHERE character_name = ?1 AND server_name = ?2
             ORDER BY started_at DESC",
        )
        .map_err(|e| e.to_string())?;

    let rows = stmt
        .query_map([&character_name, &server_name], |row| {
            Ok(UserTimer {
                id: row.get(0)?,
                character_name: row.get(1)?,
                server_name: row.get(2)?,
                label: row.get(3)?,
                duration_seconds: row.get(4)?,
                started_at: row.get(5)?,
                paused_at: row.get(6)?,
                area_key: row.get(7)?,
                source: row.get(8)?,
            })
        })
        .map_err(|e| e.to_string())?;

    let mut timers = Vec::new();
    for row in rows {
        timers.push(row.map_err(|e| e.to_string())?);
    }
    Ok(timers)
}

#[tauri::command]
pub fn save_user_timer(
    db: State<'_, DbPool>,
    character_name: String,
    server_name: String,
    timer: SaveTimerInput,
) -> Result<UserTimer, String> {
    let conn = db.get_write().map_err(|e| e.to_string())?;
    let source = timer.source.as_deref().unwrap_or("manual");

    if let Some(id) = timer.id {
        // Update existing timer
        conn.execute(
            "UPDATE user_timers SET label = ?1, duration_seconds = ?2, started_at = ?3, paused_at = ?4, area_key = ?5, source = ?6
             WHERE id = ?7 AND character_name = ?8 AND server_name = ?9",
            rusqlite::params![
                &timer.label,
                timer.duration_seconds,
                &timer.started_at,
                &timer.paused_at,
                &timer.area_key,
                source,
                id,
                &character_name,
                &server_name,
            ],
        )
        .map_err(|e| e.to_string())?;

        Ok(UserTimer {
            id,
            character_name,
            server_name,
            label: timer.label,
            duration_seconds: timer.duration_seconds,
            started_at: timer.started_at,
            paused_at: timer.paused_at,
            area_key: timer.area_key,
            source: source.to_string(),
        })
    } else {
        // Insert new timer
        conn.execute(
            "INSERT INTO user_timers (character_name, server_name, label, duration_seconds, started_at, paused_at, area_key, source)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            rusqlite::params![
                &character_name,
                &server_name,
                &timer.label,
                timer.duration_seconds,
                &timer.started_at,
                &timer.paused_at,
                &timer.area_key,
                source,
            ],
        )
        .map_err(|e| e.to_string())?;

        let id = conn.last_insert_rowid();
        Ok(UserTimer {
            id,
            character_name,
            server_name,
            label: timer.label,
            duration_seconds: timer.duration_seconds,
            started_at: timer.started_at,
            paused_at: timer.paused_at,
            area_key: timer.area_key,
            source: source.to_string(),
        })
    }
}

#[tauri::command]
pub fn delete_user_timer(db: State<'_, DbPool>, id: i64) -> Result<(), String> {
    let conn = db.get_write().map_err(|e| e.to_string())?;
    conn.execute("DELETE FROM user_timers WHERE id = ?1", [id])
        .map_err(|e| e.to_string())?;
    Ok(())
}

// ── Specta schema export (TODO.md:113 shared schema contract) ───────────────
// `timer` IPC payload shapes derive `specta::Type` so
// `src/types/timer.specta.ts` can be generated from them. Test-only plumbing:
// the exporters live in `[dev-dependencies]`, so no new runtime dependency
// ships beyond `specta` itself, and this fn compiles out of non-test builds.
// Regenerate with `OVERWRITE_BINDINGS=1 cargo test timer_specta`.
#[cfg(test)]
pub fn export_bindings() -> Result<String, Box<dyn std::error::Error>> {
    let types = specta::Types::default()
        .register::<UserTimer>()
        .register::<SaveTimerInput>()
        ;
    Ok(specta_typescript::Typescript::default().export(&types, specta_serde::Format)?)
}

#[cfg(test)]
mod specta_bindings_tests {
    use super::export_bindings;

    /// Freshness pin: the checked-in `src/types/timer.specta.ts` must equal
    /// what `export_bindings` derives from the `timer` payload shapes. Any
    /// shape change (field add/remove/rename, serde attribute) fails here until the
    /// bindings are regenerated — that failure IS the drift signal. Regenerate with
    /// `OVERWRITE_BINDINGS=1 cargo test timer_specta`, inspect the diff, and
    /// confirm it matches the hand-mirrored frontend types before committing.
    #[test]
    fn test_timer_specta_bindings_are_fresh() {
        let fresh = export_bindings().expect("specta export failed");
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../src/types/timer.specta.ts",
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