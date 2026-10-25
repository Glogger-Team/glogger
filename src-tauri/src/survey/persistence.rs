//! DB I/O for the survey tracker.
//!
//! Pure CRUD — no business logic, no state machines. The aggregator owns
//! decisions about *when* to call these; this layer just translates between
//! [`crate::survey::types`] structs and SQLite rows.
//!
//! All inserts/updates take a `&Connection` rather than a pool so callers
//! can group writes into a single transaction when they need to.

use crate::db::DbRead;
use crate::survey::types::{
    SessionStartTrigger, SurveySession, SurveyUse, SurveyUseKind, SurveyUseStatus,
};
use rusqlite::{params, Connection, OptionalExtension, Result};

// ============================================================
// Sessions
// ============================================================

/// Create a new session row. Returns the allocated `id`.
///
/// `crafted_count` is only used when `start_trigger == Crafting` — for the
/// other triggers it should be `None`.
pub fn insert_session(
    conn: &Connection,
    character: &str,
    server: &str,
    started_at: &str,
    start_trigger: SessionStartTrigger,
    crafted_count: Option<u32>,
) -> Result<i64> {
    conn.execute(
        "INSERT INTO survey_sessions (character_name, server_name, started_at, start_trigger, crafted_count)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![character, server, started_at, start_trigger.as_str(), crafted_count],
    )?;
    Ok(conn.last_insert_rowid())
}

/// Mark a session as ended. Idempotent — calling on an already-ended session
/// is a no-op rather than overwriting `ended_at`.
pub fn end_session(conn: &Connection, session_id: i64, ended_at: &str) -> Result<()> {
    conn.execute(
        "UPDATE survey_sessions SET ended_at = ?2 WHERE id = ?1 AND ended_at IS NULL",
        params![session_id, ended_at],
    )?;
    Ok(())
}

/// Fetch the active session (no `ended_at`) for a character/server pair, if any.
/// Multiple active sessions for the same character should be impossible by
/// construction — callers should treat that as a bug if it happens.
/// Column list for all SurveySession SELECT queries. Keep in sync with
/// `row_to_session` below and `SurveySession` in types.rs.
pub const SESSION_COLS: &str = "id, character_name, server_name, started_at, ended_at, \
     start_trigger, crafted_count, consumed_count, notes, \
     name, user_started_at, user_ended_at, \
     first_craft_at, last_craft_at, first_loot_at, last_loot_at";

pub fn active_session<C: DbRead + ?Sized>(
    conn: &C,
    character: &str,
    server: &str,
) -> Result<Option<SurveySession>> {
    conn.query_row(
        &format!(
            "SELECT {SESSION_COLS} FROM survey_sessions \
             WHERE character_name = ?1 AND server_name = ?2 AND ended_at IS NULL \
             ORDER BY id DESC LIMIT 1"
        ),
        params![character, server],
        row_to_session,
    )
    .optional()
}

/// Fetch a specific session by ID.
pub fn get_session<C: DbRead + ?Sized>(conn: &C, session_id: i64) -> Result<Option<SurveySession>> {
    conn.query_row(
        &format!("SELECT {SESSION_COLS} FROM survey_sessions WHERE id = ?1"),
        params![session_id],
        row_to_session,
    )
    .optional()
}

/// Atomically increment `crafted_count` for a session. Used when the
/// crafting-triggered session sees a new survey-map crafted.
pub fn increment_crafted_count(conn: &Connection, session_id: i64) -> Result<()> {
    conn.execute(
        "UPDATE survey_sessions SET crafted_count = COALESCE(crafted_count, 0) + 1
         WHERE id = ?1",
        params![session_id],
    )?;
    Ok(())
}

/// Atomically increment `consumed_count` for a session. Called by the
/// aggregator when a survey map use is recorded.
pub fn increment_consumed_count(conn: &Connection, session_id: i64) -> Result<()> {
    conn.execute(
        "UPDATE survey_sessions SET consumed_count = consumed_count + 1
         WHERE id = ?1",
        params![session_id],
    )?;
    Ok(())
}

// ── Session timestamp helpers ──────────────────────────────────────────
// These use conditional UPDATEs so callers can fire them on every event
// without worrying about overwriting earlier values. `first_*_at` only
// writes if currently NULL; `last_*_at` overwrites unconditionally.

pub fn update_first_craft_at(conn: &Connection, session_id: i64, ts: &str) -> Result<()> {
    conn.execute(
        "UPDATE survey_sessions SET first_craft_at = ?2
         WHERE id = ?1 AND first_craft_at IS NULL",
        params![session_id, ts],
    )?;
    Ok(())
}

pub fn update_last_craft_at(conn: &Connection, session_id: i64, ts: &str) -> Result<()> {
    conn.execute(
        "UPDATE survey_sessions SET last_craft_at = ?2 WHERE id = ?1",
        params![session_id, ts],
    )?;
    Ok(())
}

pub fn update_first_loot_at(conn: &Connection, session_id: i64, ts: &str) -> Result<()> {
    conn.execute(
        "UPDATE survey_sessions SET first_loot_at = ?2
         WHERE id = ?1 AND first_loot_at IS NULL",
        params![session_id, ts],
    )?;
    Ok(())
}

pub fn update_last_loot_at(conn: &Connection, session_id: i64, ts: &str) -> Result<()> {
    conn.execute(
        "UPDATE survey_sessions SET last_loot_at = ?2 WHERE id = ?1",
        params![session_id, ts],
    )?;
    Ok(())
}

/// Incrementally tighten `started_at` for an open session: if `ts` is
/// earlier than the current `started_at`, update it. This keeps the session
/// header accurate for live/open sessions without waiting for the full
/// `recompute_session_bounds_and_end` that runs at close time.
///
/// Only fires on open sessions (`ended_at IS NULL`) to avoid mutating
/// already-finalized bounds.
pub fn tighten_started_at(conn: &Connection, session_id: i64, ts: &str) -> Result<()> {
    conn.execute(
        "UPDATE survey_sessions SET started_at = ?2
         WHERE id = ?1 AND ended_at IS NULL AND started_at > ?2",
        params![session_id, ts],
    )?;
    Ok(())
}

/// Incrementally extend `ended_at` for an open session: sets `ended_at`
/// to `ts` if `ts` is later than the current latest timestamp. Unlike the
/// close-time recompute, this gives the frontend a rough "last activity"
/// timestamp for live sessions.
///
/// Only fires on open sessions (`ended_at IS NULL` is NOT checked here
/// because open sessions don't have an ended_at — instead we use a
/// separate column-free approach: the frontend already reads
/// `last_loot_at` / `last_craft_at` for the "last activity" display).
/// This is a no-op placeholder reserved for future use.

pub fn update_session_name(conn: &Connection, session_id: i64, name: &str) -> Result<()> {
    conn.execute(
        "UPDATE survey_sessions SET name = ?2 WHERE id = ?1",
        params![session_id, name],
    )?;
    Ok(())
}

pub fn update_session_user_times(
    conn: &Connection,
    session_id: i64,
    user_started_at: Option<&str>,
    user_ended_at: Option<&str>,
) -> Result<()> {
    conn.execute(
        "UPDATE survey_sessions SET user_started_at = ?2, user_ended_at = ?3 WHERE id = ?1",
        params![session_id, user_started_at, user_ended_at],
    )?;
    Ok(())
}

// ============================================================
// Uses
// ============================================================

/// Record a survey-map use. Returns the allocated `id`. The new row defaults
/// to `status = pending_loot` since loot may still arrive (motherlode mining
/// cycle, multihit window).
#[allow(clippy::too_many_arguments)]
pub fn insert_use(
    conn: &Connection,
    session_id: Option<i64>,
    character: &str,
    server: &str,
    used_at: &str,
    map_internal_name: &str,
    map_display_name: &str,
    kind: SurveyUseKind,
    area: Option<&str>,
) -> Result<i64> {
    conn.execute(
        "INSERT INTO survey_uses
            (session_id, character_name, server_name, used_at,
             map_internal_name, map_display_name, kind, area)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            session_id,
            character,
            server,
            used_at,
            map_internal_name,
            map_display_name,
            kind.as_str(),
            area
        ],
    )?;
    Ok(conn.last_insert_rowid())
}

/// Update the status of a use. Used to move from `pending_loot` to
/// `completed` / `aborted` when the per-kind window closes.
pub fn set_use_status(conn: &Connection, use_id: i64, status: SurveyUseStatus) -> Result<()> {
    conn.execute(
        "UPDATE survey_uses SET status = ?2 WHERE id = ?1",
        params![use_id, status.as_str()],
    )?;
    Ok(())
}

/// Add to a use's denormalized `loot_qty` total. Called by the aggregator
/// each time it attributes a gain to this use.
pub fn add_loot_qty(conn: &Connection, use_id: i64, delta: u32) -> Result<()> {
    conn.execute(
        "UPDATE survey_uses SET loot_qty = loot_qty + ?2 WHERE id = ?1",
        params![use_id, delta],
    )?;
    Ok(())
}

/// Mark `item_transactions` rows for a given survey use + item name as
/// speed-bonus drops. Patches `source_details` JSON with
/// `is_speed_bonus: true`. Returns the number of rows updated.
///
/// Called when a `ProcessScreenText` "(speed bonus!)" marker arrives
/// immediately after the gain transactions have been written; we identify
/// the rows by the `survey_use_id` (already stitched in via provenance) and
/// the display name of the bonus item parsed from the ScreenText.
pub fn mark_transactions_as_speed_bonus(
    conn: &Connection,
    survey_use_id: i64,
    item_name: &str,
) -> Result<usize> {
    let updated = conn.execute(
        "UPDATE item_transactions
            SET source_details = json_set(
                COALESCE(source_details, '{}'),
                '$.is_speed_bonus', json('true'))
          WHERE item_name = ?2
            AND survey_use_id = ?1",
        params![survey_use_id, item_name],
    )?;
    Ok(updated)
}

/// Fetch a use by ID.
pub fn get_use(conn: &Connection, use_id: i64) -> Result<Option<SurveyUse>> {
    conn.query_row(
        "SELECT id, session_id, character_name, server_name, used_at,
                map_internal_name, map_display_name, kind, area, status, loot_qty
         FROM survey_uses WHERE id = ?1",
        params![use_id],
        row_to_use,
    )
    .optional()
}

/// The most-recently-used survey use belonging to a session, if any.
///
/// Used by the chat-loot identity path: a gained item that is identifiable
/// as survey yield (Ore/MetalSlab keywords, …) attributes to whichever
/// survey map the player most recently consumed in the active session.
/// Ordered by `used_at` then `id` so the latest map wins even when two uses
/// share a second-resolution timestamp.
pub fn latest_use_id_for_session(conn: &Connection, session_id: i64) -> Result<Option<i64>> {
    conn.query_row(
        "SELECT id FROM survey_uses WHERE session_id = ?1
         ORDER BY used_at DESC, id DESC LIMIT 1",
        params![session_id],
        |row| row.get(0),
    )
    .optional()
}

/// The most recent Motherlode/Multihit use for a character, no older than
/// `max_age_secs` before `now_iso`.
///
/// Fallback binding for the chat-loot path: when a mining loop starts on a
/// node whose StartInteraction name marks it as survey-spawned (contains
/// "FromSurvey") but the originating use's 60s pending window has already
/// expired (long walk to the node), this recovers the use to attribute to.
pub fn latest_recent_nonbasic_use(
    conn: &Connection,
    character: &str,
    server: &str,
    now_iso: &str,
    max_age_secs: u32,
) -> Result<Option<(i64, SurveyUseKind)>> {
    conn.query_row(
        "SELECT id, kind FROM survey_uses
         WHERE character_name = ?1 AND server_name = ?2
           AND kind != 'basic'
           AND used_at >= datetime(?3, '-' || ?4 || ' seconds')
           AND used_at <= ?3
         ORDER BY used_at DESC, id DESC LIMIT 1",
        params![character, server, now_iso, max_age_secs],
        |row| {
            let id: i64 = row.get(0)?;
            let kind: String = row.get(1)?;
            Ok((
                id,
                SurveyUseKind::from_str(&kind).unwrap_or(SurveyUseKind::Multihit),
            ))
        },
    )
    .optional()
}

/// Claim untagged chat-status loot rows near a Basic survey use's `used_at`
/// and link them to the use. Returns the total quantity claimed.
///
/// Basic-survey loot lands in the same game tick as the map's DeleteItem,
/// but the chat and player logs are tailed independently — the chat rows may
/// already be in `item_transactions` (untagged) by the time the DeleteItem
/// is processed. This retroactively tags those rows as `survey_chat` so the
/// loot summary counts them, mirroring what the live gate in
/// `attribute_chat_gain` does when the ordering goes the other way.
///
/// Survey maps themselves are excluded by internal-name prefix — crafting a
/// map emits the same "added to inventory" chat line as loot, and players
/// commonly craft the next map within seconds of using one.
pub fn claim_unlinked_chat_loot_near(
    conn: &Connection,
    survey_use_id: i64,
    character: &str,
    server: &str,
    used_at_iso: &str,
    window_secs: u32,
) -> Result<u32> {
    const PREDICATE: &str = "character_name = ?2
           AND server_name = ?3
           AND source = 'chat_status'
           AND context = 'loot'
           AND source_kind IS NULL
           AND quantity > 0
           AND (internal_name IS NULL
                OR (internal_name NOT LIKE 'GeologySurvey%'
                    AND internal_name NOT LIKE 'MiningSurvey%'))
           AND timestamp BETWEEN datetime(?4, '-' || ?5 || ' seconds')
                             AND datetime(?4, '+' || ?5 || ' seconds')";

    let total: u32 = conn.query_row(
        &format!("SELECT COALESCE(SUM(quantity), 0) FROM item_transactions WHERE {PREDICATE}"),
        params![survey_use_id, character, server, used_at_iso, window_secs],
        |row| row.get(0),
    )?;
    if total == 0 {
        return Ok(0);
    }
    conn.execute(
        &format!(
            "UPDATE item_transactions
                SET source_kind = 'survey_chat',
                    source_details = json_set(COALESCE(source_details, '{{}}'),
                                              '$.survey_use_id', ?1)
              WHERE {PREDICATE}"
        ),
        params![survey_use_id, character, server, used_at_iso, window_secs],
    )?;
    Ok(total)
}

/// All uses belonging to a session, ordered by `used_at` ascending.
pub fn uses_for_session<C: DbRead + ?Sized>(conn: &C, session_id: i64) -> Result<Vec<SurveyUse>> {
    let mut stmt = conn.prepare(
        "SELECT id, session_id, character_name, server_name, used_at,
                map_internal_name, map_display_name, kind, area, status, loot_qty
         FROM survey_uses WHERE session_id = ?1 ORDER BY used_at ASC, id ASC",
    )?;
    let rows = stmt.query_map(params![session_id], row_to_use)?;
    rows.collect()
}

/// Computed bounds for a session's activity, derived from the actual
/// event timestamps rather than wall-clock at session-start/end time.
/// Both fields are `None` when the session has no uses yet.
#[derive(Debug, Clone)]
pub struct SessionTimeBounds {
    pub earliest: Option<String>,
    pub latest: Option<String>,
}

/// Compute the earliest/latest event timestamps associated with a session
/// by combining `survey_uses.used_at` and `item_transactions.timestamp`
/// for any transaction attributed via `source_details->>'survey_use_id'`.
///
/// Used by [`recompute_session_bounds_and_end`] to correct the session
/// header so start/end reflect when activity actually happened — important
/// for replayed / old-log data where wall-clock `Utc::now()` is wrong.
pub fn session_time_bounds(conn: &Connection, session_id: i64) -> Result<SessionTimeBounds> {
    // Earliest: MIN across crafting start, survey-use consumption, and
    // attributed item_transactions. Including first_craft_at ensures the
    // session start reflects when the user began crafting maps, not just
    // when the first map was consumed (which can be minutes later).
    let earliest: Option<String> = conn
        .query_row(
            "SELECT MIN(t) FROM (
                SELECT first_craft_at AS t FROM survey_sessions WHERE id = ?1 AND first_craft_at IS NOT NULL
                UNION ALL
                SELECT used_at AS t FROM survey_uses WHERE session_id = ?1
                UNION ALL
                SELECT it.timestamp AS t
                FROM item_transactions it
                JOIN survey_uses u ON u.id = it.survey_use_id
                WHERE u.session_id = ?1
             )",
            params![session_id],
            |row| row.get::<_, Option<String>>(0),
        )
        .unwrap_or(None);

    let latest: Option<String> = conn
        .query_row(
            "SELECT MAX(t) FROM (
                SELECT last_loot_at AS t FROM survey_sessions WHERE id = ?1 AND last_loot_at IS NOT NULL
                UNION ALL
                SELECT used_at AS t FROM survey_uses WHERE session_id = ?1
                UNION ALL
                SELECT it.timestamp AS t
                FROM item_transactions it
                JOIN survey_uses u ON u.id = it.survey_use_id
                WHERE u.session_id = ?1
             )",
            params![session_id],
            |row| row.get::<_, Option<String>>(0),
        )
        .unwrap_or(None);

    Ok(SessionTimeBounds { earliest, latest })
}

/// Correct a session's `started_at` and `ended_at` to match the earliest and
/// latest event timestamps actually attributed to it. Called when a session
/// closes so both live and replayed sessions store bounds that reflect the
/// real activity window instead of the wall-clock moment the end button (or
/// auto-end) fired.
///
/// `fallback_ended_at` is used when the session has no events at all
/// (nothing to derive an end from).
pub fn recompute_session_bounds_and_end(
    conn: &Connection,
    session_id: i64,
    fallback_ended_at: &str,
) -> Result<()> {
    let bounds = session_time_bounds(conn, session_id)?;
    let new_started = bounds.earliest.as_deref();
    let new_ended = bounds.latest.as_deref().unwrap_or(fallback_ended_at);

    if let Some(started) = new_started {
        conn.execute(
            "UPDATE survey_sessions SET started_at = ?2, ended_at = ?3 WHERE id = ?1",
            params![session_id, started, new_ended],
        )?;
    } else {
        // No events at all — just set ended_at, leave started_at alone.
        conn.execute(
            "UPDATE survey_sessions SET ended_at = ?2 WHERE id = ?1 AND ended_at IS NULL",
            params![session_id, new_ended],
        )?;
    }
    Ok(())
}

/// True if any use in the session is still `pending_loot`. Used by the
/// auto-end check for crafting-triggered sessions.
pub fn session_has_pending_uses(conn: &Connection, session_id: i64) -> Result<bool> {
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM survey_uses
         WHERE session_id = ?1 AND status = 'pending_loot'",
        params![session_id],
        |row| row.get(0),
    )?;
    Ok(count > 0)
}

// ============================================================
// Row mappers
// ============================================================

fn row_to_session(row: &rusqlite::Row<'_>) -> rusqlite::Result<SurveySession> {
    row_to_session_offset(row, 0)
}

/// Parse a `SurveySession` from a row where session columns start at the
/// given column offset. Shared by both the persistence layer (offset 0)
/// and the commands layer (offset varies in joined queries).
pub fn row_to_session_offset(row: &rusqlite::Row<'_>, o: usize) -> rusqlite::Result<SurveySession> {
    let trigger_str: String = row.get(o + 5)?;
    let start_trigger = SessionStartTrigger::from_str(&trigger_str).ok_or_else(|| {
        rusqlite::Error::FromSqlConversionFailure(
            o + 5,
            rusqlite::types::Type::Text,
            format!("unknown start_trigger value: {trigger_str}").into(),
        )
    })?;
    Ok(SurveySession {
        id: row.get(o)?,
        character_name: row.get(o + 1)?,
        server_name: row.get(o + 2)?,
        started_at: row.get(o + 3)?,
        ended_at: row.get(o + 4)?,
        start_trigger,
        crafted_count: row.get::<_, Option<i64>>(o + 6)?.map(|n| n as u32),
        consumed_count: row.get::<_, i64>(o + 7)? as u32,
        notes: row.get(o + 8)?,
        name: row.get(o + 9)?,
        user_started_at: row.get(o + 10)?,
        user_ended_at: row.get(o + 11)?,
        first_craft_at: row.get(o + 12)?,
        last_craft_at: row.get(o + 13)?,
        first_loot_at: row.get(o + 14)?,
        last_loot_at: row.get(o + 15)?,
    })
}

fn row_to_use(row: &rusqlite::Row<'_>) -> rusqlite::Result<SurveyUse> {
    let kind_str: String = row.get(7)?;
    let kind = SurveyUseKind::from_str(&kind_str).ok_or_else(|| {
        rusqlite::Error::FromSqlConversionFailure(
            7,
            rusqlite::types::Type::Text,
            format!("unknown kind value: {kind_str}").into(),
        )
    })?;
    let status_str: String = row.get(9)?;
    let status = SurveyUseStatus::from_str(&status_str).ok_or_else(|| {
        rusqlite::Error::FromSqlConversionFailure(
            9,
            rusqlite::types::Type::Text,
            format!("unknown status value: {status_str}").into(),
        )
    })?;
    Ok(SurveyUse {
        id: row.get(0)?,
        session_id: row.get(1)?,
        character_name: row.get(2)?,
        server_name: row.get(3)?,
        used_at: row.get(4)?,
        map_internal_name: row.get(5)?,
        map_display_name: row.get(6)?,
        kind,
        area: row.get(8)?,
        status,
        loot_qty: row.get::<_, i64>(10)? as u32,
    })
}

// ============================================================
// Tests
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::migrations::run_migrations;
    use rusqlite::Connection;

    /// Spin up an in-memory SQLite DB with the full migration history applied.
    /// Verifies v26 actually creates the tables this layer queries.
    fn fresh_db() -> Connection {
        let conn = Connection::open_in_memory().expect("open in-memory db");
        run_migrations(&conn, None).expect("run migrations");
        conn
    }

    #[test]
    fn test_insert_session_and_fetch_back() {
        let conn = fresh_db();
        let id = insert_session(
            &conn,
            "Zenith",
            "Dreva",
            "2026-04-15 12:00:00",
            SessionStartTrigger::Manual,
            None,
        )
        .unwrap();

        let s = get_session(&conn, id).unwrap().expect("session present");
        assert_eq!(s.id, id);
        assert_eq!(s.character_name, "Zenith");
        assert_eq!(s.start_trigger, SessionStartTrigger::Manual);
        assert!(s.ended_at.is_none());
        assert_eq!(s.consumed_count, 0);
    }

    #[test]
    fn test_active_session_returns_only_unended() {
        let conn = fresh_db();
        let s1 = insert_session(
            &conn,
            "Zenith",
            "Dreva",
            "2026-04-15 12:00:00",
            SessionStartTrigger::Manual,
            None,
        )
        .unwrap();
        end_session(&conn, s1, "2026-04-15 12:30:00").unwrap();

        // No active session now
        assert!(active_session(&conn, "Zenith", "Dreva").unwrap().is_none());

        // Start a new one
        let s2 = insert_session(
            &conn,
            "Zenith",
            "Dreva",
            "2026-04-15 13:00:00",
            SessionStartTrigger::FirstUse,
            None,
        )
        .unwrap();

        let active = active_session(&conn, "Zenith", "Dreva")
            .unwrap()
            .expect("should be active");
        assert_eq!(active.id, s2);
    }

    #[test]
    fn test_end_session_is_idempotent() {
        let conn = fresh_db();
        let id = insert_session(
            &conn,
            "Zenith",
            "Dreva",
            "2026-04-15 12:00:00",
            SessionStartTrigger::Manual,
            None,
        )
        .unwrap();
        end_session(&conn, id, "2026-04-15 12:30:00").unwrap();
        // Calling end_session again should not overwrite the original timestamp
        end_session(&conn, id, "2026-04-15 99:99:99").unwrap();

        let s = get_session(&conn, id).unwrap().unwrap();
        assert_eq!(s.ended_at.as_deref(), Some("2026-04-15 12:30:00"));
    }

    #[test]
    fn test_increment_counts() {
        let conn = fresh_db();
        let id = insert_session(
            &conn,
            "Zenith",
            "Dreva",
            "2026-04-15 12:00:00",
            SessionStartTrigger::Crafting,
            Some(0),
        )
        .unwrap();

        increment_crafted_count(&conn, id).unwrap();
        increment_crafted_count(&conn, id).unwrap();
        increment_consumed_count(&conn, id).unwrap();

        let s = get_session(&conn, id).unwrap().unwrap();
        assert_eq!(s.crafted_count, Some(2));
        assert_eq!(s.consumed_count, 1);
    }

    #[test]
    fn test_insert_use_and_loot_accumulation() {
        let conn = fresh_db();
        let session = insert_session(
            &conn,
            "Zenith",
            "Dreva",
            "2026-04-15 12:00:00",
            SessionStartTrigger::Manual,
            None,
        )
        .unwrap();

        let use_id = insert_use(
            &conn,
            Some(session),
            "Zenith",
            "Dreva",
            "2026-04-15 12:05:00",
            "GeologySurveySerbule1",
            "Serbule Blue Mineral Survey",
            SurveyUseKind::Basic,
            Some("Serbule"),
        )
        .unwrap();

        let u = get_use(&conn, use_id).unwrap().unwrap();
        assert_eq!(u.kind, SurveyUseKind::Basic);
        assert_eq!(u.status, SurveyUseStatus::PendingLoot);
        assert_eq!(u.loot_qty, 0);
        assert_eq!(u.area.as_deref(), Some("Serbule"));

        add_loot_qty(&conn, use_id, 3).unwrap();
        add_loot_qty(&conn, use_id, 5).unwrap();
        set_use_status(&conn, use_id, SurveyUseStatus::Completed).unwrap();

        let u = get_use(&conn, use_id).unwrap().unwrap();
        assert_eq!(u.loot_qty, 8);
        assert_eq!(u.status, SurveyUseStatus::Completed);
    }

    #[test]
    fn test_session_has_pending_uses() {
        let conn = fresh_db();
        let session = insert_session(
            &conn,
            "Zenith",
            "Dreva",
            "2026-04-15 12:00:00",
            SessionStartTrigger::Crafting,
            Some(2),
        )
        .unwrap();

        // No uses yet → no pending
        assert!(!session_has_pending_uses(&conn, session).unwrap());

        // Add one use, default pending
        let u1 = insert_use(
            &conn,
            Some(session),
            "Zenith",
            "Dreva",
            "2026-04-15 12:05:00",
            "GeologySurveySerbule1",
            "Serbule Blue Mineral Survey",
            SurveyUseKind::Basic,
            None,
        )
        .unwrap();
        assert!(session_has_pending_uses(&conn, session).unwrap());

        // Complete it → no pending
        set_use_status(&conn, u1, SurveyUseStatus::Completed).unwrap();
        assert!(!session_has_pending_uses(&conn, session).unwrap());

        // Two more uses, one completed, one still pending → still pending
        let _u2 = insert_use(
            &conn,
            Some(session),
            "Zenith",
            "Dreva",
            "2026-04-15 12:10:00",
            "MiningSurveyPovus7Y",
            "Povus Astounding Mining Survey",
            SurveyUseKind::Multihit,
            None,
        )
        .unwrap();
        let u3 = insert_use(
            &conn,
            Some(session),
            "Zenith",
            "Dreva",
            "2026-04-15 12:15:00",
            "MiningSurveyKurMountains1X",
            "Kur Mountains Simple Metal Motherlode Map",
            SurveyUseKind::Motherlode,
            None,
        )
        .unwrap();
        set_use_status(&conn, u3, SurveyUseStatus::Completed).unwrap();
        assert!(session_has_pending_uses(&conn, session).unwrap());
    }

    #[test]
    fn test_uses_for_session_orders_by_used_at() {
        let conn = fresh_db();
        let session = insert_session(
            &conn,
            "Zenith",
            "Dreva",
            "2026-04-15 12:00:00",
            SessionStartTrigger::Manual,
            None,
        )
        .unwrap();

        // Insert in non-chronological order
        let u_late = insert_use(
            &conn,
            Some(session),
            "Zenith",
            "Dreva",
            "2026-04-15 12:30:00",
            "GeologySurveySerbule1",
            "Serbule Blue Mineral Survey",
            SurveyUseKind::Basic,
            None,
        )
        .unwrap();
        let u_early = insert_use(
            &conn,
            Some(session),
            "Zenith",
            "Dreva",
            "2026-04-15 12:10:00",
            "GeologySurveySerbule2",
            "Serbule Green Mineral Survey",
            SurveyUseKind::Basic,
            None,
        )
        .unwrap();

        let uses = uses_for_session(&conn, session).unwrap();
        assert_eq!(uses.len(), 2);
        assert_eq!(uses[0].id, u_early);
        assert_eq!(uses[1].id, u_late);
    }

    #[test]
    fn test_mark_transactions_as_speed_bonus() {
        let conn = fresh_db();
        let session = insert_session(
            &conn,
            "Zenith",
            "Dreva",
            "2026-04-15 12:00:00",
            SessionStartTrigger::Manual,
            None,
        )
        .unwrap();
        let use_id = insert_use(
            &conn,
            Some(session),
            "Zenith",
            "Dreva",
            "2026-04-15 12:05:00",
            "GeologySurveySerbule1",
            "Serbule Blue Mineral Survey",
            SurveyUseKind::Basic,
            Some("Serbule"),
        )
        .unwrap();

        // Three transactions: primary + one bonus + one unrelated (different use_id).
        let details = format!(r#"{{"survey_use_id":{}}}"#, use_id);
        let other_details = r#"{"survey_use_id":999}"#;
        conn.execute(
            "INSERT INTO item_transactions (timestamp, character_name, server_name, item_name, quantity, context, source, source_kind, source_details)
             VALUES (?1,'Zenith','Dreva','Blue Spinel',1,'loot','player_log','survey_map_use',?2),
                    (?1,'Zenith','Dreva','Rubywall Crystal',2,'loot','player_log','survey_map_use',?2),
                    (?1,'Zenith','Dreva','Rubywall Crystal',1,'loot','player_log','survey_map_use',?3)",
            params!["2026-04-15 12:05:00", details, other_details],
        ).unwrap();

        let n = mark_transactions_as_speed_bonus(&conn, use_id, "Rubywall Crystal").unwrap();
        assert_eq!(
            n, 1,
            "only the matching (use_id, item_name) row should update"
        );

        // The matching Rubywall row has the flag; the Blue Spinel row and the
        // other-use Rubywall row do not.
        let flags: Vec<(String, Option<i64>)> = conn
            .prepare("SELECT item_name, json_extract(source_details, '$.is_speed_bonus') FROM item_transactions ORDER BY id")
            .unwrap()
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap()
            .collect::<Result<_>>()
            .unwrap();
        assert_eq!(flags[0].0, "Blue Spinel");
        assert_eq!(flags[0].1, None, "primary untouched");
        assert_eq!(flags[1].0, "Rubywall Crystal");
        assert_eq!(
            flags[1].1,
            Some(1),
            "matching bonus flagged true (JSON true -> 1)"
        );
        assert_eq!(flags[2].0, "Rubywall Crystal");
        assert_eq!(flags[2].1, None, "different use_id not touched");
    }
}
