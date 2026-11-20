use crate::cdn_commands::GameDataState;
use crate::db::DbPool;
/// Game State Manager — persists derived game state from chat and report data to SQLite.
///
/// Follows the SurveySessionTracker pattern: lightweight struct that receives
/// &DbPool per call, called synchronously from the coordinator's event loop.
/// Maintains "last known value" tables, not event logs.
use chrono::{Local, Utc};
use rusqlite::Connection;

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
    /// When true, login events will clear transient state (inventory, equipment, etc.)
    /// Starts false — set to true after the first poll cycle completes (catch-up is done).
    live_mode: bool,
    /// Reference to loaded CDN game data for entity resolution
    game_data: GameDataState,
}

impl GameStateManager {
    pub fn new(game_data: GameDataState) -> Self {
        Self {
            active_character: None,
            active_server: None,
            live_mode: false,
            game_data,
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

    /// Update the active character.
    /// During live mode, clears transient state so the login burst can repopulate.
    /// During replay/catch-up, skips clearing to preserve data built for each character.
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
                tx.execute(
                    "DELETE FROM game_state_inventory WHERE character_name = ?1 AND server_name = ?2",
                    rusqlite::params![name, server],
                )?;
                tx.execute(
                    "DELETE FROM game_state_equipment WHERE character_name = ?1 AND server_name = ?2",
                    rusqlite::params![name, server],
                )?;
                tx.execute(
                    "DELETE FROM game_state_combat WHERE character_name = ?1 AND server_name = ?2",
                    rusqlite::params![name, server],
                )?;
                tx.execute(
                    "DELETE FROM game_state_mount WHERE character_name = ?1 AND server_name = ?2",
                    rusqlite::params![name, server],
                )?;

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

    /// Session-keyed stack correction: corrects the named `(character, server)`
    /// slice instead of the active slice. Catch-up applies staged chat gains
    /// through this so corrections land on the session that produced the rows,
    /// not whatever identity is active at drain time.
    pub fn correct_stack_for_session(
        &self,
        character: &str,
        server: &str,
        display_name: &str,
        quantity: u32,
        db: &DbPool,
    ) -> Vec<&'static str> {
        if quantity <= 1 {
            return vec![];
        }

        let conn = match db.get_write() {
            Ok(c) => c,
            Err(_) => return vec![],
        };

        // Resolve display name to internal name via CDN data
        let internal_name = self.game_data.try_read().ok().and_then(|gd| {
            gd.resolve_item(display_name)
                .and_then(|info| info.internal_name.clone())
        });

        let mut domains = Vec::new();

        // Try to correct game_state_inventory — match by internal_name or display_name.
        // NOTE: `base_where` uses ?2/?3 because ?1 is the SET value and ?4 the item name.
        let inv_corrected = correct_stack_in_table(
            &conn,
            &StackCorrection {
                table: "game_state_inventory",
                base_where: "character_name = ?2 AND server_name = ?3",
                character,
                server,
                internal_name: internal_name.as_deref(),
                display_name,
                quantity,
            },
        );
        if inv_corrected {
            domains.push("inventory");
        }

        // Try to correct game_state_storage — same logic, any vault
        let storage_corrected = correct_stack_in_table(
            &conn,
            &StackCorrection {
                table: "game_state_storage",
                base_where: "character_name = ?2 AND server_name = ?3",
                character,
                server,
                internal_name: internal_name.as_deref(),
                display_name,
                quantity,
            },
        );
        if storage_corrected {
            domains.push("storage");
        }

        domains
    }

    /// Correct inventory/storage stack sizes using chat status data.
    ///
    /// Player.log's ProcessAddItem always records stack_size=1. The chat log's
    /// "[Status] Item x5 added to inventory." gives us the real quantity.
    /// This finds recent rows with stack_size=1 for the matching item and
    /// updates them, returning which domains were corrected.
    pub fn correct_stack_from_chat(
        &self,
        display_name: &str,
        quantity: u32,
        db: &DbPool,
    ) -> Vec<&'static str> {
        let (character, server) = match (&self.active_character, &self.active_server) {
            (Some(c), Some(s)) => (c.as_str(), s.as_str()),
            _ => return vec![],
        };
        self.correct_stack_for_session(character, server, display_name, quantity, db)
    }

}
/// Scan-callable session-keyed stack correction. Same targeting as
/// `GameStateManager::correct_stack_for_session` but without the manager:
/// scan paths (`run_scan_dispatch` in `chat_commands.rs`) have no manager
/// instance, only the line's own session identity — so the caller resolves
/// the internal name via its `GameDataState` handle and passes it in.
/// Matches internal-first with display fallback (same as the manager path);
/// idempotent — only `stack_size = 1` rows match.
pub fn correct_stack_in_table_for(
    conn: &rusqlite::Connection,
    character: &str,
    server: &str,
    display_name: &str,
    internal_name: Option<&str>,
    quantity: u32,
) -> Vec<&'static str> {
    if quantity <= 1 {
        return vec![];
    }
    let mut domains = Vec::new();
    if correct_stack_in_table(
        conn,
        &StackCorrection {
            table: "game_state_inventory",
            base_where: "character_name = ?2 AND server_name = ?3",
            character,
            server,
            internal_name,
            display_name,
            quantity,
        },
    ) {
        domains.push("inventory");
    }
    if correct_stack_in_table(
        conn,
        &StackCorrection {
            table: "game_state_storage",
            base_where: "character_name = ?2 AND server_name = ?3",
            character,
            server,
            internal_name,
            display_name,
            quantity,
        },
    ) {
        domains.push("storage");
    }
    domains
}
/// Targeting inputs for one stack-correction UPDATE. Groups the
/// `correct_stack_in_table` args so the fn stays under the arity lint.
/// (`vault_key` is gone: every caller passed `None` — no vault filter.)
#[derive(Clone, Copy)]
struct StackCorrection<'a> {
    table: &'a str,
    base_where: &'a str,
    character: &'a str,
    server: &'a str,
    internal_name: Option<&'a str>,
    display_name: &'a str,
    quantity: u32,
}
/// Returns true if a row was updated.
fn correct_stack_in_table(conn: &Connection, target: &StackCorrection<'_>) -> bool {
    let StackCorrection {
        table,
        base_where,
        character,
        server,
        internal_name,
        display_name,
        quantity,
    } = *target;
    // Build the item name match: prefer internal_name, fall back to display_name
    let name_to_match = internal_name.unwrap_or(display_name);

    // Find the most recent row with stack_size=1 for this item
    // (rowid ordering gives us recency since rows are inserted in order)
    let query = format!(
        "UPDATE {table} SET stack_size = ?1
         WHERE rowid = (
             SELECT rowid FROM {table}
             WHERE {base_where} AND item_name = ?4 AND stack_size = 1
             ORDER BY rowid DESC LIMIT 1
         )"
    );

    let updated = conn
        .execute(
            &query,
            rusqlite::params![quantity, character, server, name_to_match],
        )
        .unwrap_or(0);

    if updated > 0 {
        eprintln!(
            "[game-state] Corrected {table} stack: {display_name} → {quantity} (matched {name_to_match})"
        );
        return true;
    }

    // If internal_name didn't match and we have a display_name to try as fallback
    if internal_name.is_some() {
        let updated = conn
            .execute(
                &format!(
                    "UPDATE {table} SET stack_size = ?1
                     WHERE rowid = (
                         SELECT rowid FROM {table}
                         WHERE {base_where} AND item_name = ?4 AND stack_size = 1
                         ORDER BY rowid DESC LIMIT 1
                     )"
                ),
                rusqlite::params![quantity, character, server, display_name],
            )
            .unwrap_or(0);
        if updated > 0 {
            eprintln!(
                "[game-state] Corrected {table} stack: {display_name} → {quantity} (matched by display name)"
            );
            return true;
        }
    }

    false
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

    /// Two Player.log sessions in one catch-up: each session's full-state
    /// `SkillsLoaded` dump must land under the identity in force when it was
    /// replayed. `SkillsLoaded` deletes the character+server slice before
    /// inserting, so a session replayed under the *other* character's identity
    /// silently replaced its rows (measured before the per-session
    /// attribution: TestPlayer ended up with OtherAlt's skill load).
    ///
    /// Contract guard, not a regression pin: this drives `GameStateManager`
    /// directly, and none of those calls changed — it passes on the pre-change
    /// code too. What routes a session to its own identity is the coordinator's
    /// `adopt_player_log_session` (covered by the catch-up smoke); what this
    /// pins is that the pair-scoped dump keeps two sessions' rows apart once the
    /// identity is right.

    #[test]
    fn session_scoped_stack_correction_keys_by_session() {
        use std::sync::Arc;

        let db_path = unique_temp_db("glogger_session_stack_correct_test");
        let pool = crate::db::init_pool(db_path.clone(), Some(0)).expect("init pool");

        let mut manager = GameStateManager::new(Arc::new(tauri::async_runtime::RwLock::new(
            crate::game_data::GameData::empty(),
        )));

        let seed = |character: &str, instance_id: i64, item_name: &str| {
            pool.get_write()
                .unwrap()
                .execute(
                    "INSERT INTO game_state_inventory
                     (character_name, server_name, instance_id, item_name, stack_size, last_confirmed_at, source)
                     VALUES (?1, 'Dreva', ?2, ?3, 1, '2026-10-19 12:00:00', 'log')",
                    rusqlite::params![character, instance_id, item_name],
                )
                .unwrap()
        };
        // Internal names, matching the player-log rows the coordinator drains
        // before the staged chat corrections run.
        seed("TestPlayer", 1, "Fluorite");
        seed("OtherAlt", 2, "Fluorite");

        let stack = |character: &str| -> i64 {
            pool.get()
                .unwrap()
                .query_row(
                    "SELECT stack_size FROM game_state_inventory
                     WHERE character_name = ?1 AND server_name = 'Dreva' AND item_name = 'Fluorite'",
                    rusqlite::params![character],
                    |row| row.get(0),
                )
                .unwrap()
        };

        // Session-keyed: only OtherAlt's slice is corrected.
        let domains = manager.correct_stack_for_session("OtherAlt", "Dreva", "Fluorite", 5, &pool);
        assert!(domains.contains(&"inventory"));
        assert_eq!(stack("OtherAlt"), 5, "staged session corrected");
        assert_eq!(stack("TestPlayer"), 1, "other session untouched");

        // Live wrapper targets the active slice.
        manager.set_active_character_name("TestPlayer");
        manager.set_active_server_name("Dreva");
        let domains = manager.correct_stack_from_chat("Fluorite", 3, &pool);
        assert!(domains.contains(&"inventory"));
        assert_eq!(stack("TestPlayer"), 3, "active slice corrected");
        assert_eq!(stack("OtherAlt"), 5, "staged correction survived");

        // Drop the pool first: Windows won't delete a file with an open handle.
        drop(pool);
        let _ = std::fs::remove_file(&db_path);
    }
}
