use super::{DbPool, WriteConn};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
/// Advanced database administration commands
use tauri::State;

// ── Database Statistics ───────────────────────────────────────────────────────

/// On-disk footprint of a single logical table (its own b-tree plus all of its
/// indexes, and — for an FTS table — its shadow tables). Surfaced so users can
/// see which tables actually dominate the file.
#[derive(Serialize)]
pub struct TableSize {
    pub name: String,
    pub size_bytes: i64,
}

#[derive(Serialize)]
pub struct DatabaseStats {
    pub total_size_bytes: i64,
    pub cdn_size_bytes: i64,
    pub player_data_size_bytes: i64,
    /// Space currently on the freelist — reclaimable by a VACUUM / Compact.
    pub free_bytes: i64,
    pub market_prices_count: i64,
    pub sales_history_count: i64,
    pub survey_sessions_count: i64,
    pub event_log_count: i64,
    pub item_transactions_count: i64,
    pub chat_messages_count: i64,
    /// Largest tables by on-disk size (indexes folded into their parent table),
    /// most to least. Lets users see exactly what's eating the file.
    pub largest_tables: Vec<TableSize>,
}

#[tauri::command]
pub fn get_database_stats(db: State<'_, DbPool>) -> Result<DatabaseStats, String> {
    let conn = db
        .get()
        .map_err(|e| format!("Database connection error: {e}"))?;

    // Get page count and page size to calculate total DB size
    let page_count: i64 = conn
        .query_row("PRAGMA page_count", [], |row| row.get(0))
        .map_err(|e| format!("Failed to get page count: {e}"))?;

    let page_size: i64 = conn
        .query_row("PRAGMA page_size", [], |row| row.get(0))
        .map_err(|e| format!("Failed to get page size: {e}"))?;

    let total_size_bytes = page_count * page_size;

    // Estimate CDN data size (items, skills, abilities, recipes, npcs, quests)
    let cdn_tables = vec![
        "items",
        "skills",
        "abilities",
        "recipes",
        "recipe_ingredients",
        "npcs",
        "npc_skills",
        "quests",
    ];
    let mut cdn_size_bytes = 0i64;

    for table in cdn_tables {
        let query = format!("SELECT SUM(pgsize) FROM dbstat WHERE name = '{}'", table);
        let size: Option<i64> = conn.query_row(&query, [], |row| row.get(0)).ok();
        cdn_size_bytes += size.unwrap_or(0);
    }

    // Get player data counts
    let market_prices_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM market_prices", [], |row| row.get(0))
        .unwrap_or(0);

    let sales_history_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM sales_history", [], |row| row.get(0))
        .unwrap_or(0);

    let survey_sessions_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM survey_sessions", [], |row| row.get(0))
        .unwrap_or(0);

    let event_log_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM event_log", [], |row| row.get(0))
        .unwrap_or(0);

    let item_transactions_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM item_transactions", [], |row| {
            row.get(0)
        })
        .unwrap_or(0);

    let chat_messages_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM chat_messages", [], |row| row.get(0))
        .unwrap_or(0);

    // Reclaimable space — pages on the freelist that a VACUUM would return.
    let free_pages: i64 = conn
        .query_row("PRAGMA freelist_count", [], |row| row.get(0))
        .unwrap_or(0);
    let free_bytes = free_pages * page_size;

    // Largest tables by total on-disk size. Index pages are folded into their
    // parent table via sqlite_schema.tbl_name so a row reads as
    // "item_transactions = data + all its indexes". This is the same diagnostic
    // a user would run by hand against dbstat.
    let largest_tables: Vec<TableSize> = conn
        .prepare(
            "SELECT COALESCE(m.tbl_name, d.name) AS tbl, SUM(d.pgsize) AS bytes
               FROM dbstat d
               LEFT JOIN sqlite_schema m ON m.name = d.name
              GROUP BY tbl
              ORDER BY bytes DESC
              LIMIT 15",
        )
        .and_then(|mut stmt| {
            let rows = stmt.query_map([], |row| {
                Ok(TableSize {
                    name: row.get(0)?,
                    size_bytes: row.get::<_, Option<i64>>(1)?.unwrap_or(0),
                })
            })?;
            rows.collect::<Result<Vec<_>, _>>()
        })
        .unwrap_or_default();

    // Estimate player data size
    let player_data_size_bytes = total_size_bytes - cdn_size_bytes;

    Ok(DatabaseStats {
        total_size_bytes,
        cdn_size_bytes,
        player_data_size_bytes,
        free_bytes,
        market_prices_count,
        sales_history_count,
        survey_sessions_count,
        event_log_count,
        item_transactions_count,
        chat_messages_count,
        largest_tables,
    })
}

// ── Force Rebuild CDN Tables ──────────────────────────────────────────────────

#[tauri::command]
pub async fn force_rebuild_cdn_tables(
    db: State<'_, DbPool>,
    cdn_state: State<'_, crate::cdn_commands::GameDataState>,
) -> Result<String, String> {
    // Check the in-memory CDN data before touching the writer — a version-0
    // bail must not queue behind (or hold) the single write connection.
    let data = cdn_state.read().await;

    if data.version == 0 {
        return Err(
            "No CDN data loaded. Please wait for initial data load or force refresh CDN."
                .to_string(),
        );
    }

    let mut conn = db
        .get_write()
        .map_err(|e| format!("Database connection error: {e}"))?;

    // Persist to database (this clears and rebuilds CDN tables)
    crate::db::cdn_persistence::persist_cdn_data(&mut conn, &data)
        .map_err(|e| format!("Failed to rebuild CDN tables: {e}"))?;

    Ok(format!(
        "CDN tables rebuilt successfully from version {}",
        data.version
    ))
}

// ── Purge Player Data ─────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct PurgeOptions {
    pub older_than_days: Option<u32>,
    pub purge_all: bool,
}

#[derive(Serialize, Default)]
pub struct PurgeResult {
    pub market_prices_deleted: usize,
    pub sales_deleted: usize,
    pub survey_sessions_deleted: usize,
    pub events_deleted: usize,
    pub item_transactions_deleted: usize,
    pub chat_messages_deleted: usize,
    /// File bytes returned to the OS by the post-purge VACUUM.
    pub bytes_reclaimed: i64,
}

/// Player-generated, time-stamped tables eligible for purge, paired with the
/// column to compare against the cutoff. CDN/reference tables are never listed
/// here. `item_transactions` and `chat_messages` are the dominant space users —
/// deleting a `chat_messages` row cascades to `chat_item_links` (FK) and the
/// FTS shadow tables (triggers), so the full-text index shrinks too.
const PURGE_TABLES: &[(&str, &str)] = &[
    ("market_prices", "observed_at"),
    ("sales_history", "sold_at"),
    ("survey_sessions", "started_at"),
    ("event_log", "created_at"),
    ("item_transactions", "timestamp"),
    ("chat_messages", "timestamp"),
    ("stall_price_observations", "observed_at"),
];

/// Logical-size of the database in bytes (page_count × page_size).
fn db_size_bytes(conn: &Connection) -> i64 {
    let page_count: i64 = conn
        .query_row("PRAGMA page_count", [], |r| r.get(0))
        .unwrap_or(0);
    let page_size: i64 = conn
        .query_row("PRAGMA page_size", [], |r| r.get(0))
        .unwrap_or(0);
    page_count * page_size
}

/// Delete rows from one table, tolerating a missing table/column so a single
/// failure can't abort the whole purge. `cutoff` is `Some("-90 days")` for an
/// age-bounded purge or `None` to delete every row. Returns rows removed.
fn purge_table(conn: &Connection, table: &str, time_col: &str, cutoff: Option<&str>) -> usize {
    let result = match cutoff {
        Some(c) => conn.execute(
            &format!("DELETE FROM {table} WHERE {time_col} < datetime('now', ?1)"),
            params![c],
        ),
        None => conn.execute(&format!("DELETE FROM {table}"), []),
    };
    match result {
        Ok(n) => n,
        Err(e) => {
            eprintln!("[purge] {table} delete failed (skipped): {e}");
            0
        }
    }
}

/// Run all configured purges against `conn`. Shared by the manual command and
/// the startup auto-purge so they can never drift apart.
fn run_purge(conn: &Connection, cutoff: Option<&str>) -> PurgeResult {
    let mut counts = std::collections::HashMap::new();
    for (table, time_col) in PURGE_TABLES {
        counts.insert(*table, purge_table(conn, table, time_col, cutoff));
    }
    let get = |t: &str| counts.get(t).copied().unwrap_or(0);
    PurgeResult {
        market_prices_deleted: get("market_prices"),
        sales_deleted: get("sales_history"),
        survey_sessions_deleted: get("survey_sessions"),
        events_deleted: get("event_log"),
        item_transactions_deleted: get("item_transactions"),
        chat_messages_deleted: get("chat_messages"),
        bytes_reclaimed: 0,
    }
}

#[tauri::command]
pub fn purge_player_data(
    db: State<'_, DbPool>,
    options: PurgeOptions,
) -> Result<PurgeResult, String> {
    let conn = db
        .get_write()
        .map_err(|e| format!("Database connection error: {e}"))?;

    let cutoff = if options.purge_all {
        None
    } else if let Some(days) = options.older_than_days {
        Some(format!("-{} days", days))
    } else {
        return Err("Must specify either purge_all or older_than_days".to_string());
    };

    let size_before = db_size_bytes(&conn);
    let mut result = run_purge(&conn, cutoff.as_deref());

    // Reclaim freed pages back to the OS. VACUUM can't run inside a transaction;
    // run_purge uses autocommit deletes, so this is safe here. Retried on
    // SQLITE_BUSY so a short-lived read snapshot doesn't fail the purge.
    super::vacuum_with_retry(&conn, 3)
        .map_err(|e| format!("Failed to vacuum database (busy after 3 attempts): {e}"))?;

    result.bytes_reclaimed = (size_before - db_size_bytes(&conn)).max(0);
    Ok(result)
}

// ── Compact (VACUUM only) ─────────────────────────────────────────────────────

#[derive(Serialize)]
pub struct CompactResult {
    pub bytes_before: i64,
    pub bytes_after: i64,
    pub bytes_reclaimed: i64,
}

/// Reclaim free space without deleting any data — rebuilds the file so freelist
/// pages (left behind by deletes, FTS churn, etc.) are returned to the OS. Safe
/// to run any time; the only cost is the rewrite.
#[tauri::command]
pub fn compact_database(db: State<'_, DbPool>) -> Result<CompactResult, String> {
    let conn = db
        .get_write()
        .map_err(|e| format!("Database connection error: {e}"))?;

    let bytes_before = db_size_bytes(&conn);
    super::vacuum_with_retry(&conn, 3)
        .map_err(|e| format!("Failed to vacuum database (busy after 3 attempts): {e}"))?;
    let bytes_after = db_size_bytes(&conn);

    Ok(CompactResult {
        bytes_before,
        bytes_after,
        bytes_reclaimed: (bytes_before - bytes_after).max(0),
    })
}

// ── Auto-Purge Settings ───────────────────────────────────────────────────────

/// Perform a startup auto-purge if enabled in settings. Deletes player data
/// older than `auto_purge_days` across all `PURGE_TABLES`, then VACUUMs only if
/// something was actually removed (so a no-op startup doesn't pay the rewrite).
/// Errors on individual tables are logged and skipped, never fatal.
pub fn check_auto_purge(
    conn: &WriteConn,
    auto_purge_days: Option<u32>,
) -> Result<PurgeResult, String> {
    let Some(days) = auto_purge_days else {
        return Ok(PurgeResult::default());
    };

    let cutoff = format!("-{} days", days);
    let size_before = db_size_bytes(conn);
    let mut result = run_purge(conn, Some(&cutoff));

    let deleted = result.market_prices_deleted
        + result.sales_deleted
        + result.survey_sessions_deleted
        + result.events_deleted
        + result.item_transactions_deleted
        + result.chat_messages_deleted;

    if deleted > 0 {
        eprintln!(
            "Auto-purge (older than {days}d): {} market prices, {} sales, {} surveys, {} events, {} item txns, {} chat msgs",
            result.market_prices_deleted,
            result.sales_deleted,
            result.survey_sessions_deleted,
            result.events_deleted,
            result.item_transactions_deleted,
            result.chat_messages_deleted,
        );
        if let Err(e) = super::vacuum_with_retry(conn, 3) {
            eprintln!("[auto-purge] VACUUM failed after 3 attempts: {e}");
        }
        result.bytes_reclaimed = (size_before - db_size_bytes(conn)).max(0);
    }

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    /// Two old rows (year 2000) + one current row in each of the two big tables.
    fn setup() -> Connection {
        let c = Connection::open_in_memory().unwrap();
        c.execute_batch(
            "CREATE TABLE item_transactions(timestamp TEXT);
             CREATE TABLE chat_messages(timestamp TEXT);",
        )
        .unwrap();
        for t in ["2000-01-01 00:00:00", "2000-01-02 00:00:00"] {
            c.execute(
                "INSERT INTO item_transactions(timestamp) VALUES (?1)",
                params![t],
            )
            .unwrap();
            c.execute(
                "INSERT INTO chat_messages(timestamp) VALUES (?1)",
                params![t],
            )
            .unwrap();
        }
        c.execute(
            "INSERT INTO item_transactions(timestamp) VALUES (datetime('now'))",
            [],
        )
        .unwrap();
        c.execute(
            "INSERT INTO chat_messages(timestamp) VALUES (datetime('now'))",
            [],
        )
        .unwrap();
        c
    }

    #[test]
    fn purge_table_respects_cutoff() {
        let c = setup();
        assert_eq!(
            purge_table(&c, "item_transactions", "timestamp", Some("-30 days")),
            2
        );
        let remaining: i64 = c
            .query_row("SELECT COUNT(*) FROM item_transactions", [], |r| r.get(0))
            .unwrap();
        assert_eq!(
            remaining, 1,
            "the current row survives an age-bounded purge"
        );
    }

    #[test]
    fn purge_table_tolerates_missing_table() {
        let c = setup();
        // Must not panic / propagate — a missing table reports 0 deleted.
        assert_eq!(
            purge_table(&c, "does_not_exist", "timestamp", Some("-30 days")),
            0
        );
    }

    #[test]
    fn run_purge_counts_present_and_skips_absent() {
        let c = setup();
        let r = run_purge(&c, Some("-30 days"));
        assert_eq!(r.item_transactions_deleted, 2);
        assert_eq!(r.chat_messages_deleted, 2);
        // The other PURGE_TABLES entries don't exist in this fixture → 0, no error.
        assert_eq!(r.market_prices_deleted, 0);
        assert_eq!(r.sales_deleted, 0);
        assert_eq!(r.survey_sessions_deleted, 0);
        assert_eq!(r.events_deleted, 0);
    }

    #[test]
    fn run_purge_all_deletes_every_row() {
        let c = setup();
        let r = run_purge(&c, None);
        assert_eq!(r.item_transactions_deleted, 3);
        assert_eq!(r.chat_messages_deleted, 3);
        let remaining: i64 = c
            .query_row("SELECT COUNT(*) FROM chat_messages", [], |r| r.get(0))
            .unwrap();
        assert_eq!(remaining, 0);
    }
}
