use super::{DbRead, WriteConn};
use rusqlite::{params, OptionalExtension, Result};

/// Player data query functions
pub mod player_data {
    use super::*;

    /// Insert a market price observation
    pub fn insert_market_price(
        conn: &WriteConn,
        item_id: u32,
        price: f64,
        quantity: u32,
        vendor_type: &str,
        vendor_name: Option<&str>,
        notes: Option<&str>,
    ) -> Result<i64> {
        conn.query_row(
            "INSERT INTO market_prices (item_id, price, quantity, vendor_type, vendor_name, notes)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6) RETURNING id",
            params![item_id, price, quantity, vendor_type, vendor_name, notes],
            |row| row.get(0),
        )
    }

    /// Record a sale in sales history
    pub fn insert_sale(
        conn: &WriteConn,
        item_id: u32,
        quantity: u32,
        sale_price: f64,
        sale_method: &str,
        buyer_name: Option<&str>,
        notes: Option<&str>,
    ) -> Result<i64> {
        conn.query_row(
            "INSERT INTO sales_history (item_id, quantity, sale_price, sale_method, buyer_name, notes)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6) RETURNING id",
            params![item_id, quantity, sale_price, sale_method, buyer_name, notes],
            |row| row.get(0),
        )
    }
    /// Log a generic event
    pub fn log_event(
        conn: &WriteConn,
        event_type: &str,
        event_data: &str, // JSON string
    ) -> Result<i64> {
        conn.query_row(
            "INSERT INTO event_log (event_type, event_data) VALUES (?1, ?2) RETURNING id",
            params![event_type, event_data],
            |row| row.get(0),
        )
    }
}

/// Survey-related queries
/// CDN data persistence queries
#[allow(dead_code)]
pub mod cdn_data {
    use super::*;

    /// Check if CDN data is loaded and get version
    pub fn get_cdn_version<C: DbRead + ?Sized>(conn: &C) -> Result<Option<u32>> {
        let version: Option<u32> = conn
            .query_row("SELECT version FROM cdn_version WHERE id = 1", [], |row| {
                row.get(0)
            })
            .optional()?;
        Ok(version)
    }

    /// Update CDN version (upsert)
    pub fn set_cdn_version(conn: &WriteConn, version: u32) -> Result<()> {
        conn.execute(
            "INSERT OR REPLACE INTO cdn_version (id, version) VALUES (1, ?1)",
            params![version],
        )?;
        Ok(())
    }

    /// Clear all CDN data (for refresh)
    pub fn clear_cdn_data(conn: &WriteConn) -> Result<()> {
        conn.execute_batch(
            "DELETE FROM recipe_ingredients;
             DELETE FROM recipes;
             DELETE FROM abilities;
             DELETE FROM skills;
             DELETE FROM items;
             DELETE FROM npc_skills;
             DELETE FROM npcs;
             DELETE FROM quests;",
        )?;
        Ok(())
    }
}

/// Log file position tracking (unified for all log types)
pub mod log_positions {
    use super::*;

    /// Get the last processed position for a log file
    pub fn get_position<C: DbRead + ?Sized>(conn: &C, file_path: &str) -> Result<u64> {
        let position: i64 = conn
            .query_row(
                "SELECT last_position FROM log_file_positions WHERE file_path = ?1",
                params![file_path],
                |row| row.get(0),
            )
            .unwrap_or(0);

        Ok(position as u64)
    }

    /// Get the last processed position and player name for a log file.
    /// Returns (position, player_name) so we can restore identity on resume.
    pub fn get_position_with_player<C: DbRead + ?Sized>(
        conn: &C,
        file_path: &str,
    ) -> Result<(u64, Option<String>)> {
        let result = conn.query_row(
            "SELECT last_position, player_name FROM log_file_positions WHERE file_path = ?1",
            params![file_path],
            |row| Ok((row.get::<_, i64>(0)?, row.get::<_, Option<String>>(1)?)),
        );

        match result {
            Ok((pos, name)) => Ok((pos as u64, name)),
            Err(_) => Ok((0, None)),
        }
    }

    /// Update the last processed position for a log file
    pub fn update_position(
        conn: &rusqlite::Connection,
        file_path: &str,
        file_type: &str,
        position: u64,
        player_name: Option<&str>,
        metadata: Option<&str>,
    ) -> Result<()> {
        // Monotonic by default: never move a stored cursor BACKWARD. A stale
        // backfill racing the live tail reads an old offset, parses, then
        // commits last — without this guard it would overwrite the tail's
        // larger position, regressing the cursor and replaying lines the
        // tail already ingested. MAX() keeps the larger of old/new even if
        // the stale scan commits later.
        //
        // Legitimate resets (log truncation / rotation: the watcher saw the
        // file shrink and restarted from 0) must NOT be blocked by MAX —
        // use force_position() for those.
        conn.execute(
            "INSERT INTO log_file_positions (file_path, file_type, last_position, player_name, metadata, last_processed)
             VALUES (?1, ?2, ?3, ?4, ?5, CURRENT_TIMESTAMP)
             ON CONFLICT(file_path) DO UPDATE SET
                last_position = MAX(last_position, ?3),
                player_name = COALESCE(?4, player_name),
                metadata = COALESCE(?5, metadata),
                last_processed = CURRENT_TIMESTAMP",
            params![file_path, file_type, position as i64, player_name, metadata],
        )?;

        Ok(())
    }

    /// Force the stored position to an exact value — used ONLY for
    /// legitimate cursor resets (log truncation / rotation detected by the
    /// watcher: file size < saved position). Bypasses the monotonic MAX
    /// guard.
    pub fn force_position(
        conn: &rusqlite::Connection,
        file_path: &str,
        file_type: &str,
        position: u64,
        player_name: Option<&str>,
        metadata: Option<&str>,
    ) -> Result<()> {
        conn.execute(
            "INSERT INTO log_file_positions (file_path, file_type, last_position, player_name, metadata, last_processed)
             VALUES (?1, ?2, ?3, ?4, ?5, CURRENT_TIMESTAMP)
             ON CONFLICT(file_path) DO UPDATE SET
                last_position = ?3,
                player_name = COALESCE(?4, player_name),
                metadata = COALESCE(?5, metadata),
                last_processed = CURRENT_TIMESTAMP",
            params![file_path, file_type, position as i64, player_name, metadata],
        )?;

        Ok(())
    }

    /// Get all positions for a specific file type
    #[allow(dead_code)]
    pub fn get_positions_by_type<C: DbRead + ?Sized>(
        conn: &C,
        file_type: &str,
    ) -> Result<Vec<(String, u64)>> {
        let mut stmt = conn.prepare(
            "SELECT file_path, last_position FROM log_file_positions WHERE file_type = ?1 ORDER BY last_processed DESC"
        )?;

        let rows = stmt.query_map([file_type], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)? as u64))
        })?;

        let mut positions = Vec::new();
        for row in rows {
            positions.push(row?);
        }

        Ok(positions)
    }
}
