use crate::db::DbPool;
/// Game State Manager — persists derived game state from chat and report data to SQLite.
///
/// Follows the SurveySessionTracker pattern: lightweight struct that receives
/// &DbPool per call, called synchronously from the coordinator's event loop.
/// Maintains "last known value" tables, not event logs.
use chrono::{Local, Utc};

/// Timestamped log line for startup diagnostics.
macro_rules! startup_log {
    ($($arg:tt)*) => {
        eprintln!("[{}] {}", Local::now().format("%H:%M:%S%.3f"), format!($($arg)*));
    };
}

/// Tracks active character and writes derived game state to the database.
pub struct GameStateManager {
    active_character: Option<String>,
    active_server: Option<String>,
    /// When true, login events clear obsolete log-era inventory rows (snapshot rows survive).
    /// Starts false — set to true after the first poll cycle completes (catch-up is done).
    live_mode: bool,
}

impl GameStateManager {
    pub fn new() -> Self {
        Self {
            active_character: None,
            active_server: None,
            live_mode: false,
        }
    }

    /// Mark that catch-up replay is complete and future logins are live.
    pub fn set_live_mode(&mut self) {
        self.live_mode = true;
    }

    /// Check if we are in live mode (catch-up complete).
    pub fn is_live(&self) -> bool {
        self.live_mode
    }

    /// The ingest identity's character: the identity every log-derived write is
    /// keyed to (`DataIngestCoordinator::active_character_server`,
    /// `adopt_player_log_session`), and what the catch-up sync copies into
    /// settings. `None` only before any identity exists — the chat drain
    /// drops lines while it is.
    pub fn get_active_character(&self) -> Option<&str> {
        self.active_character.as_deref()
    }

    /// The ingest identity's server — the other half of every row's key.
    /// `None` until one is seeded from settings, adopted from a session, or
    /// learned from the chat log's marker.
    pub fn get_active_server(&self) -> Option<&str> {
        self.active_server.as_deref()
    }

    /// Update just the character name without touching the database or server.
    /// Used by Player.log handler which knows the character but not the server.
    pub fn set_active_character_name(&mut self, name: &str) {
        self.active_character = Some(name.to_string());
    }

    /// Update just the server name without touching the database.
    /// Used to seed from persisted settings at startup.
    pub fn set_active_server_name(&mut self, server: &str) {
        self.active_server = Some(server.to_string());
    }
    /// Clear the server half of the ingest identity. Used by the Player.log
    /// re-arm path, which knows the character but not the server: tagless
    /// lines skip dispatch without identity rather than keying to a stale
    /// server until the chat marker fires.
    pub fn clear_active_server_name(&mut self) {
        self.active_server = None;
    }

    /// Update the active character.
    /// During live mode, clears obsolete log-era inventory rows so the report
    /// snapshot slice (and equipment/combat/mount, which have no repopulating
    /// source) survives the login. During replay/catch-up, skips clearing to
    /// preserve data built for each character.
    pub fn set_active_character(&mut self, name: &str, server: &str, db: &DbPool, login_at: Option<&str>) {
        startup_log!(
            "Active character set: {} on {} (mode: {})",
            name,
            server,
            if self.live_mode { "live" } else { "replay" }
        );
        self.active_character = Some(name.to_string());
        self.active_server = Some(server.to_string());

        let mut conn = match db.get_write() {
            Ok(c) => c,
            Err(e) => {
                startup_log!("[game_state] DB error on set_active_character: {e}");
                return;
            }
        };

        let now = login_at
            .map(|s| s.to_string())
            .unwrap_or_else(|| Utc::now().format("%Y-%m-%d %H:%M:%S").to_string());
        let live_mode = self.live_mode;

        let tx = match conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate) {
            Ok(t) => t,
            Err(e) => {
                startup_log!("[game_state] set_active_character transaction failed: {e}");
                return;
            }
        };

        let result = (|| -> rusqlite::Result<()> {
            // Update session singleton
            tx.execute(
                "INSERT INTO game_state_session (id, character_name, server_name, last_login_at, updated_at)
                 VALUES (1, ?1, ?2, ?3, ?3)
                 ON CONFLICT(id) DO UPDATE SET
                    character_name = excluded.character_name,
                    server_name = excluded.server_name,
                    last_login_at = excluded.last_login_at,
                    updated_at = excluded.updated_at",
                rusqlite::params![name, server, now],
            )?;

            // Auto-create server record if not exists
            tx.execute(
                "INSERT INTO servers (server_name) VALUES (?1) ON CONFLICT DO NOTHING",
                rusqlite::params![server],
            )?;

            // Only clear transient state during live tailing — during replay/catch-up
            // we want to accumulate data for all characters, not nuke it on each login.
            if live_mode {
                // Inventory: only the retired Player.log slice. Report imports
                // delete-then-reinsert the inventory slice on every import,
                // and no production writer creates inventory rows besides
                // the snapshot import (chat gains write the item_transactions
                // ledger, not game_state_inventory) — so the scoped delete
                // only removes obsolete log-era leftovers. Same scoping as the
                tx.execute(
                    "DELETE FROM game_state_inventory WHERE character_name = ?1 AND server_name = ?2 AND source = 'log'",
                    rusqlite::params![name, server],
                )?;
                // Equipment/combat/mount deliberately untouched: none has a
                // `source` column to scope on, and none has a production
                // writer (no report import, no live/chat feed) that would
                // repopulate the slice after a wipe — deleting here is pure
                // data loss. Same as the adopt path, which clears only
                // log-scoped inventory for the same reason.

                // Favor needs no reset: the session label is a timestamp-filtered
                // sum over favor_events at read time, so a new login naturally
                // starts from zero without destroying anything.
            }
            Ok(())
        })();

        if let Err(e) = result {
            startup_log!("[game_state] set_active_character transaction failed: {e}");
            // tx dropped here → rollback
            return;
        }

        if let Err(e) = tx.commit() {
            startup_log!("[game_state] set_active_character transaction failed: {e}");
            return;
        }

        eprintln!(
            "[game_state] Active character set to {name} on {server}{}",
            if self.live_mode { "" } else { " (replay)" }
        );
    }

}

#[cfg(test)]
mod tests {
    use super::*;

    /// Unique temp DB path per call: `SystemTime` nanos repeat when parallel
    /// tests start in the same tick, so a bare-nanos name let two helpers open
    /// one DB — rows from one test leaked into another's assertion.
    fn unique_temp_db(prefix: &str) -> std::path::PathBuf {
        use std::sync::atomic::{AtomicU64, Ordering};
        static SEQ: AtomicU64 = AtomicU64::new(0);
        let n = SEQ.fetch_add(1, Ordering::Relaxed);
        // Nanos kept for human-sortable names; pid + counter carry uniqueness.
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        std::env::temp_dir().join(format!("{prefix}_{}_{nanos}_{n}.db", std::process::id()))
    }

    /// The login-event timestamp (not wall-clock now) opens the session: a
    /// favor gain stamped after the login event counts toward the new
    /// session's sums. Regression pin for threading the CharacterLogin
    /// timestamp into `set_active_character`.
    #[test]
    fn set_active_character_login_event_time_includes_later_favor_gain() {

        let db_path = unique_temp_db("glogger_session_login_include_test");
        let pool = crate::db::init_pool(db_path.clone(), Some(0)).expect("init pool");

        let mut manager = GameStateManager::new();

        // Favor gain lands 2s after the login event.
        pool.get_write()
            .unwrap()
            .execute(
                "INSERT INTO favor_events (character_name, server_name, npc_key, timestamp, delta)
                 VALUES ('Alice', 'Dreva', 'NPC_Test', '2026-11-16 12:00:02', 7.5)",
                [],
            )
            .unwrap();

        // Login-event timestamp — not now.
        manager.set_active_character("Alice", "Dreva", &pool, Some("2026-11-16 12:00:00"));

        let conn = pool.get().unwrap();
        let session_at: String = conn
            .query_row("SELECT last_login_at FROM game_state_session WHERE id = 1", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(session_at, "2026-11-16 12:00:00");

        let sums =
            crate::db::game_state_commands::session_favor_sums(&conn, "Alice", "Dreva").unwrap();
        assert!((sums["NPC_Test"] - 7.5).abs() < 1e-9);

        // Drop the pool first: Windows won't delete a file with an open handle.
        drop(pool);
        let _ = std::fs::remove_file(&db_path);
    }

    /// A gain from the previous session (before the login event) stays out
    /// of the new session's sums once the login boundary is the event time.
    #[test]
    fn set_active_character_login_event_time_excludes_prior_session_favor() {

        let db_path = unique_temp_db("glogger_session_login_exclude_test");
        let pool = crate::db::init_pool(db_path.clone(), Some(0)).expect("init pool");

        let mut manager = GameStateManager::new();

        // One gain from the previous session, one from the new session.
        pool.get_write()
            .unwrap()
            .execute_batch(
                "INSERT INTO favor_events (character_name, server_name, npc_key, timestamp, delta) VALUES
                 ('Alice', 'Dreva', 'NPC_Test', '2026-11-16 11:59:58', 100.0),
                 ('Alice', 'Dreva', 'NPC_Test', '2026-11-16 12:00:02', 7.5)",
            )
            .unwrap();

        manager.set_active_character("Alice", "Dreva", &pool, Some("2026-11-16 12:00:00"));

        let conn = pool.get().unwrap();
        let sums =
            crate::db::game_state_commands::session_favor_sums(&conn, "Alice", "Dreva").unwrap();
        assert_eq!(sums.len(), 1);
        assert!((sums["NPC_Test"] - 7.5).abs() < 1e-9);

        // Drop the pool first: Windows won't delete a file with an open handle.
        drop(pool);
        let _ = std::fs::remove_file(&db_path);
    }

    /// A live login clears only obsolete log-era inventory rows: snapshot
    /// rows (what report imports write) survive with no import following,
    /// and equipment/combat/mount — which have no `source` column and no
    /// production writer to repopulate them — survive untouched. Pins the
    /// per-table scoping of the live-login wipe (same as the adopt path).
    #[test]
    fn set_active_character_live_login_preserves_snapshot_inventory() {

        let db_path = unique_temp_db("glogger_session_login_snapshot_test");
        let pool = crate::db::init_pool(db_path.clone(), Some(0)).expect("init pool");

        let mut manager = GameStateManager::new();
        manager.set_live_mode();

        // Same slice, two provenances: an obsolete log-era row and a saved
        // snapshot row (what report imports write).
        pool.get_write()
            .unwrap()
            .execute_batch(
                "INSERT INTO game_state_inventory
                 (character_name, server_name, instance_id, item_name, stack_size, last_confirmed_at, source) VALUES
                 ('Alice', 'Dreva', 1, 'Fluorite', 1, '2026-11-16 12:00:00', 'log'),
                 ('Alice', 'Dreva', 2, 'Fluorite', 1, '2026-11-16 12:00:00', 'snapshot');
                 INSERT INTO game_state_equipment (character_name, server_name, slot, appearance_key, last_confirmed_at)
                 VALUES ('Alice', 'Dreva', 'MainHand', 'sword_key', '2026-11-16 12:00:00');
                 INSERT INTO game_state_combat (character_name, server_name, in_combat, last_confirmed_at)
                 VALUES ('Alice', 'Dreva', 0, '2026-11-16 12:00:00');
                 INSERT INTO game_state_mount (character_name, server_name, is_mounted, last_confirmed_at)
                 VALUES ('Alice', 'Dreva', 0, '2026-11-16 12:00:00')",
            )
            .unwrap();

        manager.set_active_character("Alice", "Dreva", &pool, Some("2026-11-16 12:00:00"));

        let conn = pool.get().unwrap();
        let remaining: Vec<String> = conn
            .prepare(
                "SELECT source FROM game_state_inventory
                 WHERE character_name = 'Alice' AND server_name = 'Dreva'",
            )
            .unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(
            remaining,
            vec!["snapshot".to_string()],
            "snapshot survives live login"
        );

        for table in [
            "game_state_equipment",
            "game_state_combat",
            "game_state_mount",
        ] {
            let count: i64 = conn
                .query_row(
                    &format!("SELECT COUNT(*) FROM {table} WHERE character_name = 'Alice' AND server_name = 'Dreva'"),
                    [],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(count, 1, "{table} survives live login");
        }

        // Drop the pool first: Windows won't delete a file with an open handle.
        drop(pool);
        let _ = std::fs::remove_file(&db_path);
    }
}
