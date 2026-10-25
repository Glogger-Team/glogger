use super::DbPool;
use serde::{Deserialize, Serialize};
/// Farming session persistence commands
use tauri::State;

// ── Input types ─────────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct FarmingSkillInput {
    pub skill_id: i64,
    pub skill_name: String,
    pub xp_gained: i64,
    pub levels_gained: i32,
}

#[derive(Deserialize)]
pub struct FarmingItemInput {
    pub item_name: String,
    pub net_quantity: i32,
}

#[derive(Deserialize)]
pub struct FarmingFavorInput {
    pub npc_key: String,
    pub npc_name: String,
    pub delta: f64,
}

#[derive(Deserialize)]
pub struct FarmingKillInput {
    pub enemy_name: String,
    pub kill_count: i32,
}

#[derive(Deserialize)]
pub struct SaveFarmingSessionInput {
    /// When set, update this existing session row (and replace its child rows)
    /// instead of inserting a new one. Used by the periodic auto-save so an
    /// in-progress session keeps a single row that is refreshed in place.
    #[serde(default)]
    pub session_id: Option<i64>,
    pub name: String,
    pub notes: String,
    pub start_time: String,
    pub end_time: Option<String>,
    pub elapsed_seconds: i64,
    pub total_paused_seconds: i64,
    pub vendor_gold: i64,
    pub skills: Vec<FarmingSkillInput>,
    pub items: Vec<FarmingItemInput>,
    pub favors: Vec<FarmingFavorInput>,
    pub kills: Vec<FarmingKillInput>,
}

// ── Output types ────────────────────────────────────────────────────────────

#[derive(Serialize)]
pub struct FarmingSkillRecord {
    pub skill_id: i64,
    pub skill_name: String,
    pub xp_gained: i64,
    pub levels_gained: i32,
}

#[derive(Serialize)]
pub struct FarmingItemRecord {
    pub item_name: String,
    pub net_quantity: i32,
}

#[derive(Serialize)]
pub struct FarmingFavorRecord {
    pub npc_key: String,
    pub npc_name: String,
    pub delta: f64,
}

#[derive(Serialize)]
pub struct FarmingKillRecord {
    pub enemy_name: String,
    pub kill_count: i32,
}

#[derive(Serialize)]
pub struct HistoricalFarmingSession {
    pub id: i64,
    pub name: String,
    pub notes: String,
    pub start_time: String,
    pub end_time: Option<String>,
    pub elapsed_seconds: i64,
    pub total_paused_seconds: i64,
    pub vendor_gold: i64,
    pub created_at: String,
    pub skills: Vec<FarmingSkillRecord>,
    pub items: Vec<FarmingItemRecord>,
    pub favors: Vec<FarmingFavorRecord>,
    pub kills: Vec<FarmingKillRecord>,
}

// ── Commands ────────────────────────────────────────────────────────────────

#[tauri::command]
pub fn save_farming_session(
    db: State<'_, DbPool>,
    input: SaveFarmingSessionInput,
) -> Result<i64, String> {
    let mut conn = db
        .get_write()
        .map_err(|e| format!("Database connection error: {e}"))?;
    let conn = conn
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .map_err(|e| format!("Failed to begin transaction: {e}"))?;
    let id = save_farming_session_impl(&conn, &input)?;
    conn.commit()
        .map_err(|e| format!("Failed to commit transaction: {e}"))?;
    Ok(id)
}

/// Core save/upsert logic, separated from Tauri managed state so it can be
/// unit-tested against an in-memory connection.
fn save_farming_session_impl(
    conn: &rusqlite::Connection,
    input: &SaveFarmingSessionInput,
) -> Result<i64, String> {
    // Upsert the session row. When updating an existing row (periodic auto-save
    // of an active session) we refresh its columns and clear the child rows so
    // they can be rewritten from the current snapshot — no double-counting.
    let session_id = if let Some(existing_id) = input.session_id {
        let affected = conn
            .execute(
                "UPDATE farming_sessions
                 SET name = ?1, notes = ?2, start_time = ?3, end_time = ?4,
                     elapsed_seconds = ?5, total_paused_seconds = ?6, vendor_gold = ?7
                 WHERE id = ?8",
                rusqlite::params![
                    input.name,
                    input.notes,
                    input.start_time,
                    input.end_time,
                    input.elapsed_seconds,
                    input.total_paused_seconds,
                    input.vendor_gold,
                    existing_id,
                ],
            )
            .map_err(|e| format!("Failed to update farming session: {e}"))?;

        if affected == 0 {
            return Err(format!("Farming session {existing_id} not found"));
        }

        for table in [
            "farming_session_skills",
            "farming_session_items",
            "farming_session_favors",
            "farming_session_kills",
        ] {
            conn.execute(
                &format!("DELETE FROM {table} WHERE session_id = ?1"),
                rusqlite::params![existing_id],
            )
            .map_err(|e| format!("Failed to clear {table}: {e}"))?;
        }

        existing_id
    } else {
        conn.execute(
            "INSERT INTO farming_sessions (name, notes, start_time, end_time, elapsed_seconds, total_paused_seconds, vendor_gold)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            rusqlite::params![
                input.name,
                input.notes,
                input.start_time,
                input.end_time,
                input.elapsed_seconds,
                input.total_paused_seconds,
                input.vendor_gold,
            ],
        ).map_err(|e| format!("Failed to save farming session: {e}"))?;

        conn.last_insert_rowid()
    };

    // Insert skill records
    for skill in &input.skills {
        conn.execute(
            "INSERT INTO farming_session_skills (session_id, skill_id, skill_name, xp_gained, levels_gained)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![session_id, skill.skill_id, skill.skill_name, skill.xp_gained, skill.levels_gained],
        ).map_err(|e| format!("Failed to save farming skill: {e}"))?;
    }

    // Insert item records
    for item in &input.items {
        conn.execute(
            "INSERT INTO farming_session_items (session_id, item_name, net_quantity)
             VALUES (?1, ?2, ?3)",
            rusqlite::params![session_id, item.item_name, item.net_quantity],
        )
        .map_err(|e| format!("Failed to save farming item: {e}"))?;
    }

    // Insert favor records
    for favor in &input.favors {
        conn.execute(
            "INSERT INTO farming_session_favors (session_id, npc_key, npc_name, delta)
             VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![session_id, favor.npc_key, favor.npc_name, favor.delta],
        )
        .map_err(|e| format!("Failed to save farming favor: {e}"))?;
    }

    // Insert kill records
    for kill in &input.kills {
        conn.execute(
            "INSERT INTO farming_session_kills (session_id, enemy_name, kill_count)
             VALUES (?1, ?2, ?3)",
            rusqlite::params![session_id, kill.enemy_name, kill.kill_count],
        )
        .map_err(|e| format!("Failed to save farming kill: {e}"))?;
    }

    Ok(session_id)
}

#[tauri::command]
pub fn get_farming_sessions(
    db: State<'_, DbPool>,
    limit: Option<usize>,
) -> Result<Vec<HistoricalFarmingSession>, String> {
    let conn = db
        .get()
        .map_err(|e| format!("Database connection error: {e}"))?;
    let limit = limit.unwrap_or(50) as i64;

    // Fetch session rows
    let mut stmt = conn
        .prepare(
            "SELECT id, name, notes, start_time, end_time, elapsed_seconds,
                total_paused_seconds, vendor_gold, datetime(created_at) as created_at
         FROM farming_sessions
         ORDER BY created_at DESC
         LIMIT ?1",
        )
        .map_err(|e| format!("Failed to prepare query: {e}"))?;

    let session_rows = stmt
        .query_map([limit], |row| {
            Ok(HistoricalFarmingSession {
                id: row.get(0)?,
                name: row.get(1)?,
                notes: row.get(2)?,
                start_time: row.get(3)?,
                end_time: row.get(4)?,
                elapsed_seconds: row.get(5)?,
                total_paused_seconds: row.get(6)?,
                vendor_gold: row.get(7)?,
                created_at: row.get(8)?,
                skills: Vec::new(),
                items: Vec::new(),
                favors: Vec::new(),
                kills: Vec::new(),
            })
        })
        .map_err(|e| format!("Query failed: {e}"))?;

    let mut sessions: Vec<HistoricalFarmingSession> = Vec::new();
    for row in session_rows {
        sessions.push(row.map_err(|e| format!("Row parse error: {e}"))?);
    }

    // Fill in children for each session
    for session in &mut sessions {
        // Skills
        let mut skill_stmt = conn.prepare(
            "SELECT skill_id, skill_name, xp_gained, levels_gained FROM farming_session_skills WHERE session_id = ?1"
        ).map_err(|e| format!("Failed to prepare skill query: {e}"))?;

        let skill_rows = skill_stmt
            .query_map([session.id], |row| {
                Ok(FarmingSkillRecord {
                    skill_id: row.get(0)?,
                    skill_name: row.get(1)?,
                    xp_gained: row.get(2)?,
                    levels_gained: row.get(3)?,
                })
            })
            .map_err(|e| format!("Skill query failed: {e}"))?;

        for row in skill_rows {
            session
                .skills
                .push(row.map_err(|e| format!("Skill row error: {e}"))?);
        }

        // Items
        let mut item_stmt = conn.prepare(
            "SELECT item_name, net_quantity FROM farming_session_items WHERE session_id = ?1 ORDER BY net_quantity DESC"
        ).map_err(|e| format!("Failed to prepare item query: {e}"))?;

        let item_rows = item_stmt
            .query_map([session.id], |row| {
                Ok(FarmingItemRecord {
                    item_name: row.get(0)?,
                    net_quantity: row.get(1)?,
                })
            })
            .map_err(|e| format!("Item query failed: {e}"))?;

        for row in item_rows {
            session
                .items
                .push(row.map_err(|e| format!("Item row error: {e}"))?);
        }

        // Favors
        let mut favor_stmt = conn
            .prepare(
                "SELECT npc_key, npc_name, delta FROM farming_session_favors WHERE session_id = ?1",
            )
            .map_err(|e| format!("Failed to prepare favor query: {e}"))?;

        let favor_rows = favor_stmt
            .query_map([session.id], |row| {
                Ok(FarmingFavorRecord {
                    npc_key: row.get(0)?,
                    npc_name: row.get(1)?,
                    delta: row.get(2)?,
                })
            })
            .map_err(|e| format!("Favor query failed: {e}"))?;

        for row in favor_rows {
            session
                .favors
                .push(row.map_err(|e| format!("Favor row error: {e}"))?);
        }

        // Kills
        let mut kill_stmt = conn
            .prepare(
                "SELECT enemy_name, kill_count FROM farming_session_kills WHERE session_id = ?1 ORDER BY kill_count DESC",
            )
            .map_err(|e| format!("Failed to prepare kill query: {e}"))?;

        let kill_rows = kill_stmt
            .query_map([session.id], |row| {
                Ok(FarmingKillRecord {
                    enemy_name: row.get(0)?,
                    kill_count: row.get(1)?,
                })
            })
            .map_err(|e| format!("Kill query failed: {e}"))?;

        for row in kill_rows {
            session
                .kills
                .push(row.map_err(|e| format!("Kill row error: {e}"))?);
        }
    }

    Ok(sessions)
}

#[tauri::command]
pub fn update_farming_session(
    db: State<'_, DbPool>,
    session_id: i64,
    name: String,
    notes: String,
) -> Result<(), String> {
    let conn = db
        .get_write()
        .map_err(|e| format!("Database connection error: {e}"))?;

    conn.execute(
        "UPDATE farming_sessions SET name = ?1, notes = ?2 WHERE id = ?3",
        rusqlite::params![name, notes, session_id],
    )
    .map_err(|e| format!("Failed to update farming session: {e}"))?;

    Ok(())
}

#[tauri::command]
pub fn delete_farming_session(db: State<'_, DbPool>, session_id: i64) -> Result<(), String> {
    let conn = db
        .get_write()
        .map_err(|e| format!("Database connection error: {e}"))?;

    conn.execute("DELETE FROM farming_sessions WHERE id = ?1", [session_id])
        .map_err(|e| format!("Failed to delete farming session: {e}"))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::migrations::run_migrations;
    use rusqlite::Connection;

    fn setup() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys=ON;").unwrap();
        run_migrations(&conn, None).unwrap();
        conn
    }

    fn input_with(
        session_id: Option<i64>,
        kills: Vec<(&str, i32)>,
        items: Vec<(&str, i32)>,
    ) -> SaveFarmingSessionInput {
        SaveFarmingSessionInput {
            session_id,
            name: "Test Session".into(),
            notes: String::new(),
            start_time: "12:00:00".into(),
            end_time: None,
            elapsed_seconds: 600,
            total_paused_seconds: 0,
            vendor_gold: 0,
            skills: Vec::new(),
            items: items
                .into_iter()
                .map(|(item_name, net_quantity)| FarmingItemInput {
                    item_name: item_name.into(),
                    net_quantity,
                })
                .collect(),
            favors: Vec::new(),
            kills: kills
                .into_iter()
                .map(|(enemy_name, kill_count)| FarmingKillInput {
                    enemy_name: enemy_name.into(),
                    kill_count,
                })
                .collect(),
        }
    }

    fn session_count(conn: &Connection) -> i64 {
        conn.query_row("SELECT COUNT(*) FROM farming_sessions", [], |r| r.get(0))
            .unwrap()
    }

    fn kill_count(conn: &Connection, id: i64, enemy: &str) -> i64 {
        conn.query_row(
            "SELECT COALESCE(SUM(kill_count), 0) FROM farming_session_kills WHERE session_id = ?1 AND enemy_name = ?2",
            rusqlite::params![id, enemy],
            |r| r.get(0),
        )
        .unwrap()
    }

    fn item_row_count(conn: &Connection, id: i64) -> i64 {
        conn.query_row(
            "SELECT COUNT(*) FROM farming_session_items WHERE session_id = ?1",
            [id],
            |r| r.get(0),
        )
        .unwrap()
    }

    /// The periodic auto-save must update the same row in place and *replace*
    /// child rows from the fresh snapshot — never insert a duplicate session and
    /// never double-count progressively growing tallies.
    #[test]
    fn upsert_updates_in_place_and_replaces_children() {
        let conn = setup();

        // First auto-save (no id) inserts a new row.
        let id = save_farming_session_impl(
            &conn,
            &input_with(None, vec![("Goblin", 5)], vec![("Gold Coin", 10)]),
        )
        .unwrap();
        assert_eq!(session_count(&conn), 1);
        assert_eq!(kill_count(&conn, id, "Goblin"), 5);
        assert_eq!(item_row_count(&conn, id), 1);

        // Second auto-save (same id) with grown tallies updates the SAME row.
        let id2 = save_farming_session_impl(
            &conn,
            &input_with(
                Some(id),
                vec![("Goblin", 8)],
                vec![("Gold Coin", 10), ("Bone", 3)],
            ),
        )
        .unwrap();

        assert_eq!(id2, id, "upsert must return the same row id");
        assert_eq!(session_count(&conn), 1, "no duplicate session row created");
        assert_eq!(
            kill_count(&conn, id, "Goblin"),
            8,
            "kills replaced with the fresh snapshot, not summed (no double-count)"
        );
        assert_eq!(
            item_row_count(&conn, id),
            2,
            "items replaced with the fresh snapshot"
        );
    }

    /// Auto-saving against an id that no longer exists (e.g. the row was deleted)
    /// must error rather than silently create a mismatched row.
    #[test]
    fn upsert_with_unknown_id_errors() {
        let conn = setup();
        let result =
            save_farming_session_impl(&conn, &input_with(Some(99999), vec![("Goblin", 1)], vec![]));
        assert!(
            result.is_err(),
            "updating a non-existent session should error"
        );
    }
}
