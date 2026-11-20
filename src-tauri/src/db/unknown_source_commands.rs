use super::DbPool;

/// Retention for unattributed gains, in hours. Rows older than this with no
/// source of any kind are provisional data whose link-up window has closed —
/// survey retro-claims look back seconds, kill links 30 minutes — so they
/// can never attribute and are purged. Kill-linked rows (`from_entity_id`),
/// survey/economic rows (`source_kind`), spends and vault moves are never
/// touched; neither are the legacy 'unknown'/'uncertain' classifications
/// (deliberate history, not provisional data).
pub const UNLINKED_GAIN_RETENTION_HOURS: i64 = 24;

/// Purge unattributed gains older than [`UNLINKED_GAIN_RETENTION_HOURS`].
/// Runs at startup and after every scan/tail dispatch; log-and-continue on
/// failure, never fatal. Returns rows deleted.
pub fn sweep_unlinked_gains(db_pool: &DbPool) -> Result<usize, String> {
    let conn = db_pool
        .get_write()
        .map_err(|e| format!("Database error: {e}"))?;
    let deleted = conn
        .execute(
            "DELETE FROM item_transactions
             WHERE source_kind IS NULL
               AND from_entity_id IS NULL
               AND quantity > 0
               AND context != 'storage_deposit'
               AND timestamp < datetime('now', '-' || ?1 || ' hours')",
            rusqlite::params![UNLINKED_GAIN_RETENTION_HOURS],
        )
        .map_err(|e| format!("Failed to sweep unlinked gains: {e}"))?;
    Ok(deleted)
}
