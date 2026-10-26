use r2d2_sqlite::SqliteConnectionManager;
use rusqlite::{Connection, Result};
use std::path::PathBuf;

pub mod admin_commands;
pub mod aggregate_commands;
pub mod arena_commands;
pub mod brewing_commands;
pub mod build_planner_commands;
pub mod cdn_persistence;
pub mod character_commands;
pub mod chat_commands;
pub mod combat_wisdom_commands;
pub mod crafting_commands;
pub mod death_commands;
pub mod farming_commands;
pub mod game_state_commands;
pub mod gourmand_commands;
pub mod inventory_commands;
pub mod kill_tracking_commands;
pub mod market_commands;
pub mod message_commands;
pub mod migrations;
pub mod player_commands;
pub mod poem_commands;
#[allow(dead_code)]
pub mod price_helper_commands;
pub mod queries;
pub mod resuscitate_commands;
pub mod roulette_commands;
pub mod stall_commands;
pub mod stall_tracker_commands;
// Reads survey_types (CDN-populated reference table) — useful raw material
// for the future Analytics rebuild. No active consumer right now since the
// legacy survey screen was removed; allow dead_code until that work lands.
pub mod hoplology_commands;
pub mod statehelm_commands;
#[allow(dead_code)]
pub mod survey_commands;
pub mod timer_commands;
pub mod word_of_power_catalog;
pub mod words_of_power_commands;

/// The pooled single write connection. Writers only.
pub type WriteConn = r2d2::PooledConnection<SqliteConnectionManager>;

/// Pooled read-only connection. Deliberately exposes no write API: there is no
/// `execute`, no `execute_batch`, and `prepare` hands back a `ReadStatement`
/// that likewise cannot execute. A write routed through this type is a
/// compile error, not a runtime `SQLITE_READONLY`.
pub struct ReadConn(r2d2::PooledConnection<SqliteConnectionManager>);

/// Statement borrowed from a `ReadConn`, exposing query methods only.
pub struct ReadStatement<'conn>(rusqlite::Statement<'conn>);

/// Read-only SQLite API. Implemented by every connection type that may be
/// handed to a read helper, so read helpers work with read connections, write
/// connections and transactions alike.
pub trait DbRead {
    fn prepare(&self, sql: &str) -> Result<ReadStatement<'_>>;
    fn query_row<T, P, F>(&self, sql: &str, params: P, f: F) -> Result<T>
    where
        P: rusqlite::Params,
        F: FnOnce(&rusqlite::Row<'_>) -> Result<T>;
}

impl ReadConn {
    /// Prepare a read-only statement. Mirrors `DbRead::prepare` as an inherent
    /// method so call sites do not need the trait in scope.
    pub fn prepare(&self, sql: &str) -> Result<ReadStatement<'_>> {
        Connection::prepare(&self.0, sql).map(|s| ReadStatement(s))
    }

    /// Query a single row. Mirrors `DbRead::query_row`.
    pub fn query_row<T, P, F>(&self, sql: &str, params: P, f: F) -> Result<T>
    where
        P: rusqlite::Params,
        F: FnOnce(&rusqlite::Row<'_>) -> Result<T>,
    {
        self.0.query_row(sql, params, f)
    }
}

impl<'conn> ReadStatement<'conn> {
    pub fn query_map<T, P, F>(&mut self, params: P, f: F) -> Result<rusqlite::MappedRows<'_, F>>
    where
        P: rusqlite::Params,
        F: FnMut(&rusqlite::Row<'_>) -> Result<T>,
    {
        self.0.query_map(params, f)
    }

    pub fn query_row<T, P, F>(&mut self, params: P, f: F) -> Result<T>
    where
        P: rusqlite::Params,
        F: FnOnce(&rusqlite::Row<'_>) -> Result<T>,
    {
        self.0.query_row(params, f)
    }

    pub fn column_names(&self) -> Vec<&str> {
        self.0.column_names()
    }
}

impl DbRead for ReadConn {
    fn prepare(&self, sql: &str) -> Result<ReadStatement<'_>> {
        Connection::prepare(&self.0, sql).map(|s| ReadStatement(s))
    }

    fn query_row<T, P, F>(&self, sql: &str, params: P, f: F) -> Result<T>
    where
        P: rusqlite::Params,
        F: FnOnce(&rusqlite::Row<'_>) -> Result<T>,
    {
        self.0.query_row(sql, params, f)
    }
}

impl DbRead for Connection {
    fn prepare(&self, sql: &str) -> Result<ReadStatement<'_>> {
        Connection::prepare(self, sql).map(|s| ReadStatement(s))
    }

    fn query_row<T, P, F>(&self, sql: &str, params: P, f: F) -> Result<T>
    where
        P: rusqlite::Params,
        F: FnOnce(&rusqlite::Row<'_>) -> Result<T>,
    {
        Connection::query_row(self, sql, params, f)
    }
}

// NOTE: `DbRead` is deliberately NOT implemented for `Transaction` or
// `PooledConnection`. Both deref to `Connection`, and a trait method on the
// receiver type would shadow the inherent `Connection::prepare` at the first
// autoderef step — turning every `tx.prepare(..).execute(..)` / writer
// `conn.prepare(..)` into a compile error. Call sites that hold a transaction
// or pooled write connection and want the read helper pass `&*tx` / `&*conn`
// (deref to `Connection`) instead.

/// A pool pair for a WAL-mode SQLite database: a read pool (multiple
/// connections — WAL readers never block the writer) and a single-connection
/// write pool.
///
/// INVARIANT — the pool is the lock. Do NOT add an app-level Mutex/RwLock
/// (StallOpsLock, operation_lock, or any successor) around writers to
/// "serialize" them: there is exactly one write connection, so writers
/// already serialize at checkout. A second lock only adds deadlock shape
/// (poll thread + command + watcher each holding one side) for zero extra
/// safety. Rules:
/// - `get()` for reads, `get_write()` for writes (one-word routing).
/// - Multi-statement writes use `BEGIN IMMEDIATE` (`TransactionBehavior::Immediate`).
/// - Never hold an app lock across a pool checkout; parse first, check out
///   the writer only around the transaction.
///
/// Both pools wait up to 15 s for a free connection (`connection_timeout`);
/// exceeding it yields an `r2d2::Error` whose `Display` is always
/// `timed out waiting for connection: …` (r2d2 0.8 has no error variants —
/// `pub struct Error(Option<String>)`).
#[derive(Clone)]
pub struct DbPool {
    reads: r2d2::Pool<SqliteConnectionManager>,
    writes: r2d2::Pool<SqliteConnectionManager>,
}

impl DbPool {
    /// Test/dev constructor: build a pool pair around existing pools.
    #[cfg(test)]
    pub fn from_pools(
        reads: r2d2::Pool<SqliteConnectionManager>,
        writes: r2d2::Pool<SqliteConnectionManager>,
    ) -> Self {
        Self { reads, writes }
    }

    /// Get a read connection (concurrent with the writer and other readers).
    pub fn get(&self) -> Result<ReadConn, r2d2::Error> {
        Ok(ReadConn(self.reads.get()?))
    }

    /// Get the dedicated write connection. Max one exists, so concurrent
    /// writers queue here in the app (fair FIFO via r2d2) rather than racing
    /// SQLite's busy handler.
    pub fn get_write(&self) -> Result<WriteConn, r2d2::Error> {
        self.writes.get()
    }
}

/// Initialize the database pool pair with the given path.
/// `tz_offset_seconds` is needed for one-time migration to fix historical timestamps.
pub fn init_pool(
    db_path: PathBuf,
    tz_offset_seconds: Option<i32>,
) -> Result<DbPool, Box<dyn std::error::Error>> {
    // Create parent directory if it doesn't exist
    if let Some(parent) = db_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let manager = SqliteConnectionManager::file(&db_path).with_init(|conn| {
        // rarray (array binding for IN-list queries) must be registered
        // per-connection; do it alongside the PRAGMA setup so every pooled
        // connection gets it.
        if let Err(e) = rusqlite::vtab::array::load_module(conn) {
            return Err(rusqlite::Error::ModuleError(format!(
                "Failed to load rarray module: {e}"
            )));
        }
        conn.execute_batch(
            "PRAGMA journal_mode=WAL;
                 PRAGMA busy_timeout=5000;
                 PRAGMA synchronous=NORMAL;
                 PRAGMA foreign_keys=ON;",
        )
    });

    // Writes: exactly one connection — app-side serialization of every writer.
    let writes = r2d2::Pool::builder()
        .max_size(1)
        .min_idle(Some(1))
        .connection_timeout(std::time::Duration::from_secs(15))
        .build(manager)?;

    // Reads: concurrent in WAL mode, never block the writer. query_only
    // fails fast (instead of silently racing the writer) if a future write
    // path ever grabs a read connection by mistake.
    let reads = r2d2::Pool::builder()
        .max_size(12)
        .connection_timeout(std::time::Duration::from_secs(15))
        .build(SqliteConnectionManager::file(&db_path).with_init(|conn| {
            if let Err(e) = rusqlite::vtab::array::load_module(conn) {
                return Err(rusqlite::Error::ModuleError(format!(
                    "Failed to load rarray module: {e}"
                )));
            }
            conn.execute_batch(
                "PRAGMA journal_mode=WAL;
                     PRAGMA busy_timeout=5000;
                     PRAGMA synchronous=NORMAL;
                     PRAGMA foreign_keys=ON;
                     PRAGMA query_only=ON;",
            )
        }))?;

    // Run migrations on the write connection (schema changes are writes).
    {
        let conn = writes.get()?;
        migrations::run_migrations(&conn, tz_offset_seconds)?;
    } // Released back to the write pool here.

    Ok(DbPool { reads, writes })
}

/// Get current schema version
pub fn get_schema_version(conn: &Connection) -> Result<i32> {
    match conn.query_row(
        "SELECT version FROM schema_migrations ORDER BY version DESC LIMIT 1",
        [],
        |row| row.get(0),
    ) {
        Ok(v) => Ok(v),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(0), // empty table = fresh DB
        Err(e) => Err(e),
    }
}

/// Record that a migration was applied
fn record_migration(conn: &Connection, version: i32) -> Result<()> {
    conn.execute(
        "INSERT INTO schema_migrations (version) VALUES (?1)",
        [version],
    )?;
    Ok(())
}

/// SQLITE_BUSY / SQLITE_LOCKED classification (retryable contention).
pub fn is_busy_error(e: &rusqlite::Error) -> bool {
    matches!(
        e,
        rusqlite::Error::SqliteFailure(err, _)
            if err.code == rusqlite::ErrorCode::DatabaseBusy
                || err.code == rusqlite::ErrorCode::DatabaseLocked
    )
}

/// VACUUM with SQLITE_BUSY retries. VACUUM needs an exclusive snapshot, so an
/// in-flight read transaction makes it fail after busy_timeout; retrying lets
/// short reads drain. `attempts` total tries, 1s apart.
pub fn vacuum_with_retry(conn: &rusqlite::Connection, attempts: u32) -> rusqlite::Result<()> {
    let attempts = attempts.max(1);
    for attempt in 0..attempts {
        match conn.execute_batch("VACUUM;") {
            Ok(()) => return Ok(()),
            Err(e) if is_busy_error(&e) && attempt + 1 < attempts => {
                std::thread::sleep(std::time::Duration::from_secs(1));
            }
            Err(e) => return Err(e),
        }
    }
    unreachable!("vacuum_with_retry loop always returns")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn get_schema_version_propagates_read_errors() {
        // A bare in-memory connection has no `schema_migrations` table. The old
        // `.unwrap_or(0)` silently reported "fresh DB" (0) on ANY error, which
        // would make the migration runner replay the whole history. It must now
        // surface the error instead.
        let conn = Connection::open_in_memory().unwrap();
        assert!(get_schema_version(&conn).is_err());
    }

    #[test]
    fn get_schema_version_zero_on_empty_table() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("CREATE TABLE schema_migrations (version INTEGER PRIMARY KEY)")
            .unwrap();
        assert_eq!(get_schema_version(&conn).unwrap(), 0);
    }
}
