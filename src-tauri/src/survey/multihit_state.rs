//! DB-backed tracking of open multihit nodes.
//!
//! A multihit node is a mining target spawned by a `Multihit`-kind survey map
//! that survives multiple swings (2–20 hits, varies per node, no explicit
//! "depleted" log signal). The window for attributing mining gains to the
//! originating survey use stays open until either:
//!
//! - the player starts a mining interaction with a different `entity_id`, or
//! - 30 minutes have elapsed since the last hit on this node.
//!
//! The 30-minute window is long — an app restart, a server disconnect, or a
//! coffee break can all happen within it. Losing that state would lose real
//! loot attribution, so we persist it. See
//! `docs/architecture/survey-mechanics.md` for the full mechanics.
use crate::db::DbRead;
use rusqlite::{params, Connection, OptionalExtension, Result};
use serde::{Deserialize, Serialize};

/// One row from `open_multihit_nodes`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenMultihitNode {
    pub node_entity_id: i64,
    pub character_name: String,
    pub server_name: String,
    pub survey_use_id: i64,
    pub opened_at: String,
    pub last_hit_at: String,
}

/// Create or refresh an open-multihit-node row.
///
/// `INSERT OR REPLACE` semantics: if a row already exists for this
/// (character, server, node_entity_id), the new `survey_use_id` and `opened_at`
/// overwrite. This handles the rare case where the same `entity_id` gets
/// recycled by the game after the previous owner closed.
pub fn open_node(
    conn: &Connection,
    character: &str,
    server: &str,
    node_entity_id: i64,
    survey_use_id: i64,
    opened_at: &str,
) -> Result<()> {
    conn.execute(
        "INSERT INTO open_multihit_nodes
            (node_entity_id, character_name, server_name, survey_use_id, opened_at, last_hit_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?5)
         ON CONFLICT(character_name, server_name, node_entity_id) DO UPDATE SET
             survey_use_id = excluded.survey_use_id,
             opened_at = excluded.opened_at,
             last_hit_at = excluded.last_hit_at",
        params![node_entity_id, character, server, survey_use_id, opened_at],
    )?;
    Ok(())
}

/// Update `last_hit_at` for an existing open node. Caller should look up the
/// node first to confirm it's tracked; this is a no-op if the row doesn't
/// exist.
pub fn touch_node(
    conn: &Connection,
    character: &str,
    server: &str,
    node_entity_id: i64,
    last_hit_at: &str,
) -> Result<()> {
    conn.execute(
        "UPDATE open_multihit_nodes
         SET last_hit_at = ?4
         WHERE character_name = ?1 AND server_name = ?2 AND node_entity_id = ?3",
        params![character, server, node_entity_id, last_hit_at],
    )?;
    Ok(())
}

/// Close (remove) a single open node row by its key.
pub fn close_node(
    conn: &Connection,
    character: &str,
    server: &str,
    node_entity_id: i64,
) -> Result<()> {
    conn.execute(
        "DELETE FROM open_multihit_nodes
         WHERE character_name = ?1 AND server_name = ?2 AND node_entity_id = ?3",
        params![character, server, node_entity_id],
    )?;
    Ok(())
}

/// Fetch the open-multihit row for one node, if tracked.
pub fn get_node<C: DbRead + ?Sized>(
    conn: &C,
    character: &str,
    server: &str,
    node_entity_id: i64,
) -> Result<Option<OpenMultihitNode>> {
    conn.query_row(
        "SELECT node_entity_id, character_name, server_name, survey_use_id, opened_at, last_hit_at
         FROM open_multihit_nodes
         WHERE character_name = ?1 AND server_name = ?2 AND node_entity_id = ?3",
        params![character, server, node_entity_id],
        row_to_node,
    )
    .optional()
}

/// All open nodes for a character/server pair, oldest `last_hit_at` first.
/// Used by the per-character timeout sweep.
pub fn list_nodes<C: DbRead + ?Sized>(
    conn: &C,
    character: &str,
    server: &str,
) -> Result<Vec<OpenMultihitNode>> {
    let mut stmt = conn.prepare(
        "SELECT node_entity_id, character_name, server_name, survey_use_id, opened_at, last_hit_at
         FROM open_multihit_nodes
         WHERE character_name = ?1 AND server_name = ?2
         ORDER BY last_hit_at ASC",
    )?;
    let rows = stmt.query_map(params![character, server], row_to_node)?;
    rows.collect()
}

/// Find and return all rows whose `last_hit_at` is older than the cutoff
/// timestamp, then delete them in the same transaction. Returns the rows so
/// the caller can mark their associated `survey_uses` as completed/aborted.
///
/// `cutoff_ts` should be a "HH:MM:SS"- or "YYYY-MM-DD HH:MM:SS"-shaped string
/// that compares lexicographically against the stored `last_hit_at` (use the
/// same format consistently). The aggregator typically computes this as
/// `now_utc - 30 minutes`.
pub fn sweep_expired(
    conn: &Connection,
    character: &str,
    server: &str,
    cutoff_ts: &str,
) -> Result<Vec<OpenMultihitNode>> {
    // Collect first, then delete by primary key. Two-step keeps the API
    // simple at the cost of one extra round-trip per expired row; multihit
    // sweeps are infrequent so this is fine.
    let mut stmt = conn.prepare(
        "SELECT node_entity_id, character_name, server_name, survey_use_id, opened_at, last_hit_at
         FROM open_multihit_nodes
         WHERE character_name = ?1 AND server_name = ?2 AND last_hit_at < ?3",
    )?;
    let rows: Vec<OpenMultihitNode> = stmt
        .query_map(params![character, server, cutoff_ts], row_to_node)?
        .collect::<Result<Vec<_>>>()?;

    for r in &rows {
        conn.execute(
            "DELETE FROM open_multihit_nodes
             WHERE character_name = ?1 AND server_name = ?2 AND node_entity_id = ?3",
            params![r.character_name, r.server_name, r.node_entity_id],
        )?;
    }
    Ok(rows)
}

fn row_to_node(row: &rusqlite::Row<'_>) -> rusqlite::Result<OpenMultihitNode> {
    Ok(OpenMultihitNode {
        node_entity_id: row.get(0)?,
        character_name: row.get(1)?,
        server_name: row.get(2)?,
        survey_use_id: row.get(3)?,
        opened_at: row.get(4)?,
        last_hit_at: row.get(5)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::migrations::run_migrations;
    use crate::survey::persistence;
    use crate::survey::types::{SessionStartTrigger, SurveyUseKind};

    fn fresh_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        run_migrations(&conn, None).unwrap();
        conn
    }

    /// Create a session + use so the foreign key on open_multihit_nodes is satisfied.
    fn make_use(conn: &Connection) -> i64 {
        let s = persistence::insert_session(
            conn,
            "Zenith",
            "Dreva",
            "2026-04-15 12:00:00",
            SessionStartTrigger::Manual,
            None,
        )
        .unwrap();
        persistence::insert_use(
            conn,
            Some(s),
            "Zenith",
            "Dreva",
            "2026-04-15 12:05:00",
            "MiningSurveyPovus7Y",
            "Povus Astounding Mining Survey",
            SurveyUseKind::Multihit,
            Some("Povus"),
        )
        .unwrap()
    }

    #[test]
    fn test_open_then_get() {
        let conn = fresh_db();
        let use_id = make_use(&conn);
        open_node(&conn, "Zenith", "Dreva", 12345, use_id, "12:30:00").unwrap();

        let n = get_node(&conn, "Zenith", "Dreva", 12345)
            .unwrap()
            .expect("present");
        assert_eq!(n.survey_use_id, use_id);
        assert_eq!(n.opened_at, "12:30:00");
        assert_eq!(n.last_hit_at, "12:30:00");
    }

    #[test]
    fn test_open_overwrites_existing() {
        // If the game recycles an entity_id (rare but possible across long sessions),
        // the second open should replace the first cleanly rather than fail.
        let conn = fresh_db();
        let u1 = make_use(&conn);
        let u2 = make_use(&conn);
        open_node(&conn, "Zenith", "Dreva", 12345, u1, "12:30:00").unwrap();
        open_node(&conn, "Zenith", "Dreva", 12345, u2, "13:00:00").unwrap();

        let n = get_node(&conn, "Zenith", "Dreva", 12345).unwrap().unwrap();
        assert_eq!(n.survey_use_id, u2);
        assert_eq!(n.opened_at, "13:00:00");
    }

    #[test]
    fn test_touch_and_close() {
        let conn = fresh_db();
        let use_id = make_use(&conn);
        open_node(&conn, "Zenith", "Dreva", 12345, use_id, "12:30:00").unwrap();

        touch_node(&conn, "Zenith", "Dreva", 12345, "12:32:00").unwrap();
        let n = get_node(&conn, "Zenith", "Dreva", 12345).unwrap().unwrap();
        assert_eq!(n.last_hit_at, "12:32:00");
        // opened_at must NOT change on touch
        assert_eq!(n.opened_at, "12:30:00");

        close_node(&conn, "Zenith", "Dreva", 12345).unwrap();
        assert!(get_node(&conn, "Zenith", "Dreva", 12345).unwrap().is_none());
    }

    #[test]
    fn test_touch_missing_node_is_noop() {
        let conn = fresh_db();
        // Should not error — UPDATE simply matches zero rows
        touch_node(&conn, "Zenith", "Dreva", 99999, "12:32:00").unwrap();
    }

    #[test]
    fn test_sweep_expired_returns_and_deletes() {
        let conn = fresh_db();
        let u1 = make_use(&conn);
        let u2 = make_use(&conn);
        let u3 = make_use(&conn);

        // Three nodes with different last_hit_at timestamps
        open_node(&conn, "Zenith", "Dreva", 100, u1, "12:00:00").unwrap();
        open_node(&conn, "Zenith", "Dreva", 200, u2, "12:30:00").unwrap();
        open_node(&conn, "Zenith", "Dreva", 300, u3, "13:00:00").unwrap();

        // Sweep with cutoff of 12:45:00 — should grab the first two
        let expired = sweep_expired(&conn, "Zenith", "Dreva", "12:45:00").unwrap();
        let mut ids: Vec<i64> = expired.iter().map(|r| r.node_entity_id).collect();
        ids.sort();
        assert_eq!(ids, vec![100, 200]);

        // Survivor remains
        let remaining = list_nodes(&conn, "Zenith", "Dreva").unwrap();
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].node_entity_id, 300);
    }

    #[test]
    fn test_list_nodes_orders_by_last_hit_asc() {
        let conn = fresh_db();
        let u1 = make_use(&conn);
        let u2 = make_use(&conn);

        open_node(&conn, "Zenith", "Dreva", 100, u1, "13:00:00").unwrap();
        open_node(&conn, "Zenith", "Dreva", 200, u2, "12:30:00").unwrap();

        let nodes = list_nodes(&conn, "Zenith", "Dreva").unwrap();
        assert_eq!(nodes.len(), 2);
        // Oldest first — node 200 was hit at 12:30
        assert_eq!(nodes[0].node_entity_id, 200);
        assert_eq!(nodes[1].node_entity_id, 100);
    }

    #[test]
    fn test_per_character_isolation() {
        let conn = fresh_db();
        let u1 = make_use(&conn);

        // Same node_entity_id, different character — must not collide
        open_node(&conn, "Zenith", "Dreva", 100, u1, "12:00:00").unwrap();
        // Need a use scoped to the other character for FK reasons.
        let s2 = persistence::insert_session(
            &conn,
            "Other",
            "Dreva",
            "2026-04-15 12:00:00",
            SessionStartTrigger::Manual,
            None,
        )
        .unwrap();
        let u2 = persistence::insert_use(
            &conn,
            Some(s2),
            "Other",
            "Dreva",
            "2026-04-15 12:05:00",
            "MiningSurveyPovus7Y",
            "Povus Astounding Mining Survey",
            SurveyUseKind::Multihit,
            None,
        )
        .unwrap();
        open_node(&conn, "Other", "Dreva", 100, u2, "12:00:00").unwrap();

        let zenith_nodes = list_nodes(&conn, "Zenith", "Dreva").unwrap();
        let other_nodes = list_nodes(&conn, "Other", "Dreva").unwrap();
        assert_eq!(zenith_nodes.len(), 1);
        assert_eq!(other_nodes.len(), 1);
        assert_eq!(zenith_nodes[0].survey_use_id, u1);
        assert_eq!(other_nodes[0].survey_use_id, u2);
    }
}
