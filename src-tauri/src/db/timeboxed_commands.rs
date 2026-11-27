/// Time-boxed analytics over the existing timestamped rows.
///
/// No new tables, no write paths, no migrations: every query below reads the
/// same ground truth the unbounded getters read, scoped to an inclusive
/// `[start_ts, end_ts]` window.
///
/// UTC contract: bounds are space-format naive UTC (`"YYYY-MM-DD HH:MM:SS"`),
/// the same shape the chat pipeline stores (`killed_at`, `timestamp`,
/// `awarded_at`) and the frontend sends. Comparison is lexicographic, which
/// orders contract stamps chronologically. Non-contract families (legacy
/// time-only `killed_at`, `T`-suffixed stamps) sort outside a contract window
/// and never match — documented, not normalized.
///
/// Identity rule (mirrors `get_farming_sessions_impl`): a `None` filter
/// disables that predicate; a `Some` filter matches the requested value OR
/// NULL, so legacy unattributed rows stay visible to every character. An
/// inverted window (`start_ts > end_ts`) returns an empty vec, never an
/// error — the selector can emit one transiently while typing a custom range.
///
/// Councils are read elsewhere: the v103 `council_events` ledger keeps the
/// per-event timestamped deltas `currency_estimate` never had, and
/// `get_council_history` answers "councils in window W" off it (see the
/// Council Wealth tab). These timeboxed queries stay kills/loot/XP only.
use super::farming_commands::{FarmingItemRecord, FarmingKillRecord, FarmingSkillRecord};
use super::{DbPool, DbRead};
use tauri::State;

/// Empty window (start sorts after end) → `true`: callers return `Ok(vec![])`.
fn is_empty_window(start_ts: &str, end_ts: &str) -> bool {
    start_ts > end_ts
}

fn require_bounds(start_ts: &str, end_ts: &str) -> Result<(), String> {
    if start_ts.is_empty() || end_ts.is_empty() {
        return Err("start_ts and end_ts are required (space-format naive UTC)".to_string());
    }
    Ok(())
}

/// Kills in `[start_ts, end_ts]` per enemy, most-killed first.
/// Shape reuses [`FarmingKillRecord`] so views share the session tables.
fn get_timeboxed_kills_impl<C: DbRead + ?Sized>(
    conn: &C,
    start_ts: &str,
    end_ts: &str,
    character_name: Option<&str>,
    server_name: Option<&str>,
) -> Result<Vec<FarmingKillRecord>, String> {
    require_bounds(start_ts, end_ts)?;
    if is_empty_window(start_ts, end_ts) {
        return Ok(Vec::new());
    }
    let mut stmt = conn
        .prepare(
            "SELECT enemy_name, COUNT(*) FROM enemy_kills
             WHERE killed_at >= ?1 AND killed_at <= ?2
               AND (?3 IS NULL OR character_name IS NULL OR character_name = ?3)
               AND (?4 IS NULL OR server_name IS NULL OR server_name = ?4)
             GROUP BY enemy_name
             ORDER BY COUNT(*) DESC, enemy_name ASC",
        )
        .map_err(|e| format!("Failed to prepare time-boxed kills query: {e}"))?;
    let rows = stmt
        .query_map(
            rusqlite::params![start_ts, end_ts, character_name, server_name],
            |row| {
                Ok(FarmingKillRecord {
                    enemy_name: row.get(0)?,
                    kill_count: row.get(1)?,
                })
            },
        )
        .map_err(|e| format!("Time-boxed kills query failed: {e}"))?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row.map_err(|e| format!("Time-boxed kill row error: {e}"))?);
    }
    Ok(out)
}

/// Loot gains in `[start_ts, end_ts]` per item, largest haul first.
/// Gain rows only (`context = 'loot'`, `quantity > 0`): losses/vendor-sale
/// rows are not loot. Shape reuses [`FarmingItemRecord`] (`net_quantity` =
/// summed gains in the window).
fn get_timeboxed_loot_impl<C: DbRead + ?Sized>(
    conn: &C,
    start_ts: &str,
    end_ts: &str,
    character_name: Option<&str>,
    server_name: Option<&str>,
) -> Result<Vec<FarmingItemRecord>, String> {
    require_bounds(start_ts, end_ts)?;
    if is_empty_window(start_ts, end_ts) {
        return Ok(Vec::new());
    }
    let mut stmt = conn
        .prepare(
            "SELECT item_name, SUM(quantity) FROM item_transactions
             WHERE context = 'loot' AND quantity > 0
               AND timestamp >= ?1 AND timestamp <= ?2
               AND (?3 IS NULL OR character_name IS NULL OR character_name = ?3)
               AND (?4 IS NULL OR server_name IS NULL OR server_name = ?4)
             GROUP BY item_name
             ORDER BY SUM(quantity) DESC, item_name ASC",
        )
        .map_err(|e| format!("Failed to prepare time-boxed loot query: {e}"))?;
    let rows = stmt
        .query_map(
            rusqlite::params![start_ts, end_ts, character_name, server_name],
            |row| {
                Ok(FarmingItemRecord {
                    item_name: row.get(0)?,
                    net_quantity: row.get(1)?,
                })
            },
        )
        .map_err(|e| format!("Time-boxed loot query failed: {e}"))?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row.map_err(|e| format!("Time-boxed loot row error: {e}"))?);
    }
    Ok(out)
}

/// XP awards in `[start_ts, end_ts]` per skill, largest gain first, read off
/// the v101 per-award ledger (`chat_xp_awards`). Names join
/// `game_state_skills`; a skill with no snapshot row yet falls back to
/// `"Skill <id>"`. `levels_gained` is 0: the ledger carries the level at
/// each award, and deriving ding transitions needs the anchor math — out of
/// scope for a read-only window. Shape reuses [`FarmingSkillRecord`].
fn get_timeboxed_xp_impl<C: DbRead + ?Sized>(
    conn: &C,
    start_ts: &str,
    end_ts: &str,
    character_name: Option<&str>,
    server_name: Option<&str>,
) -> Result<Vec<FarmingSkillRecord>, String> {
    require_bounds(start_ts, end_ts)?;
    if is_empty_window(start_ts, end_ts) {
        return Ok(Vec::new());
    }
    let mut stmt = conn
        .prepare(
            "SELECT a.skill_id, COALESCE(g.skill_name, 'Skill ' || a.skill_id), SUM(a.xp)
             FROM chat_xp_awards a
             LEFT JOIN game_state_skills g
               ON g.character_name = a.character_name
              AND g.server_name = a.server_name
              AND g.skill_id = a.skill_id
             WHERE a.awarded_at >= ?1 AND a.awarded_at <= ?2
               AND (?3 IS NULL OR a.character_name = ?3)
               AND (?4 IS NULL OR a.server_name = ?4)
             GROUP BY a.skill_id
             ORDER BY SUM(a.xp) DESC, a.skill_id ASC",
        )
        .map_err(|e| format!("Failed to prepare time-boxed XP query: {e}"))?;
    let rows = stmt
        .query_map(
            rusqlite::params![start_ts, end_ts, character_name, server_name],
            |row| {
                Ok(FarmingSkillRecord {
                    skill_id: row.get(0)?,
                    skill_name: row.get(1)?,
                    xp_gained: row.get(2)?,
                    levels_gained: 0,
                })
            },
        )
        .map_err(|e| format!("Time-boxed XP query failed: {e}"))?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row.map_err(|e| format!("Time-boxed XP row error: {e}"))?);
    }
    Ok(out)
}

#[tauri::command]
pub fn get_timeboxed_kills(
    db: State<'_, DbPool>,
    start_ts: String,
    end_ts: String,
    character_name: Option<String>,
    server_name: Option<String>,
) -> Result<Vec<FarmingKillRecord>, String> {
    let conn = db
        .get()
        .map_err(|e| format!("Database connection error: {e}"))?;
    get_timeboxed_kills_impl(
        &conn,
        &start_ts,
        &end_ts,
        character_name.as_deref(),
        server_name.as_deref(),
    )
}

#[tauri::command]
pub fn get_timeboxed_loot(
    db: State<'_, DbPool>,
    start_ts: String,
    end_ts: String,
    character_name: Option<String>,
    server_name: Option<String>,
) -> Result<Vec<FarmingItemRecord>, String> {
    let conn = db
        .get()
        .map_err(|e| format!("Database connection error: {e}"))?;
    get_timeboxed_loot_impl(
        &conn,
        &start_ts,
        &end_ts,
        character_name.as_deref(),
        server_name.as_deref(),
    )
}

#[tauri::command]
pub fn get_timeboxed_xp(
    db: State<'_, DbPool>,
    start_ts: String,
    end_ts: String,
    character_name: Option<String>,
    server_name: Option<String>,
) -> Result<Vec<FarmingSkillRecord>, String> {
    let conn = db
        .get()
        .map_err(|e| format!("Database connection error: {e}"))?;
    get_timeboxed_xp_impl(
        &conn,
        &start_ts,
        &end_ts,
        character_name.as_deref(),
        server_name.as_deref(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::migrations::run_migrations;
    use rusqlite::Connection;

    fn setup() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        run_migrations(&conn, None).unwrap();
        conn
    }

    /// Same-day fixture: two kills + receipts + XP awards for the hero,
    /// one out-of-window row per domain, one legacy NULL-identity kill.
    fn seed(conn: &Connection) {
        conn.execute_batch(
            "INSERT INTO enemy_kills (enemy_name, enemy_entity_id, killing_ability, killed_at, character_name, server_name) VALUES
                ('Rat', 'e1', 'Sword', '2026-06-15 10:00:00', 'Hero', 'Dreva'),
                ('Rat', 'e2', 'Sword', '2026-06-15 10:05:00', 'Hero', 'Dreva'),
                ('Bat', 'e3', 'Sword', '2026-06-15 11:00:00', 'Hero', 'Dreva'),
                ('Rat', 'e4', 'Sword', '2026-06-14 10:00:00', 'Hero', 'Dreva'),
                ('Spider', 'e5', 'Sword', '2026-06-15 12:00:00', NULL, NULL);
             INSERT INTO item_transactions (timestamp, character_name, server_name, item_name, quantity, context, source) VALUES
                ('2026-06-15 10:01:00', 'Hero', 'Dreva', 'Rat Tail', 2, 'loot', 'chat_status'),
                ('2026-06-15 10:06:00', 'Hero', 'Dreva', 'Rat Tail', 1, 'loot', 'chat_status'),
                ('2026-06-15 11:01:00', 'Hero', 'Dreva', 'Bat Wing', 1, 'loot', 'chat_status'),
                ('2026-06-14 10:01:00', 'Hero', 'Dreva', 'Rat Tail', 9, 'loot', 'chat_status'),
                ('2026-06-15 10:02:00', 'Hero', 'Dreva', 'Rat Tail', -1, 'loot', 'chat_status');
             INSERT INTO game_state_skills (character_name, server_name, skill_id, skill_name, level, base_level, bonus_levels, xp, tnl, max_level, last_confirmed_at, source) VALUES
                ('Hero', 'Dreva', 1, 'Sword', 10, 10, 0, 100, 200, 0, '2026-06-15 09:00:00', 'chat');
             INSERT INTO chat_xp_awards (character_name, server_name, skill_id, awarded_at, xp, level) VALUES
                ('Hero', 'Dreva', 1, '2026-06-15 10:00:30', 50, 10),
                ('Hero', 'Dreva', 1, '2026-06-15 11:00:30', 70, 10),
                ('Hero', 'Dreva', 1, '2026-06-14 10:00:30', 500, 10);",
        )
        .unwrap();
    }

    #[test]
    fn tonight_window_matches_unbounded_filtered_to_today() {
        // Equivalence proof: the range query over 2026-06-15 must equal the
        // unbounded table filtered to that day.
        let conn = setup();
        seed(&conn);
        let day_start = "2026-06-15 00:00:00";
        let day_end = "2026-06-15 23:59:59";

        let kills =
            get_timeboxed_kills_impl(&conn, day_start, day_end, Some("Hero"), Some("Dreva"))
                .unwrap();
        let unbounded_kills: Vec<(String, i64)> = conn
            .prepare(
                "SELECT enemy_name, COUNT(*) FROM enemy_kills
                 WHERE substr(killed_at, 1, 10) = '2026-06-15'
                   AND (character_name IS NULL OR character_name = 'Hero')
                   AND (server_name IS NULL OR server_name = 'Dreva')
                 GROUP BY enemy_name ORDER BY COUNT(*) DESC, enemy_name ASC",
            )
            .unwrap()
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap()
            .map(|r| r.unwrap())
            .collect();
        assert_eq!(kills.len(), unbounded_kills.len(), "kill groups match");
        for (got, want) in kills.iter().zip(unbounded_kills.iter()) {
            assert_eq!(&got.enemy_name, &want.0);
            assert_eq!(got.kill_count as i64, want.1);
        }
        // 2 Rat + 1 Bat today, plus the legacy NULL-identity Spider visible
        // to every character.
        assert_eq!(
            kills.iter().map(|k| k.kill_count).sum::<i32>(),
            4,
            "tonight kills total"
        );

        let loot =
            get_timeboxed_loot_impl(&conn, day_start, day_end, Some("Hero"), Some("Dreva")).unwrap();
        assert_eq!(loot.len(), 2, "Rat Tail + Bat Wing groups");
        let rat = loot.iter().find(|l| l.item_name == "Rat Tail").unwrap();
        assert_eq!(rat.net_quantity, 3, "2 + 1 gains; the -1 loss row excluded");
        let unbounded_loot: i64 = conn
            .query_row(
                "SELECT COALESCE(SUM(quantity), 0) FROM item_transactions
                 WHERE context = 'loot' AND quantity > 0
                   AND substr(timestamp, 1, 10) = '2026-06-15'
                   AND character_name = 'Hero' AND server_name = 'Dreva'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(
            loot.iter().map(|l| l.net_quantity as i64).sum::<i64>(),
            unbounded_loot,
            "loot totals equal unbounded-filtered-to-today"
        );

        let xp =
            get_timeboxed_xp_impl(&conn, day_start, day_end, Some("Hero"), Some("Dreva")).unwrap();
        assert_eq!(xp.len(), 1);
        assert_eq!(xp[0].skill_name, "Sword");
        assert_eq!(xp[0].xp_gained, 120, "50 + 70; the 500 from 06-14 excluded");
    }

    #[test]
    fn inclusive_bounds_cover_exact_edge_stamps() {
        let conn = setup();
        seed(&conn);
        // Window exactly on the two Rat kill stamps: both included.
        let kills = get_timeboxed_kills_impl(
            &conn,
            "2026-06-15 10:00:00",
            "2026-06-15 10:05:00",
            Some("Hero"),
            Some("Dreva"),
        )
        .unwrap();
        assert_eq!(kills.len(), 1);
        assert_eq!(kills[0].kill_count, 2, "both edge stamps inclusive");
        // A window excluding a known row drops it.
        let narrow = get_timeboxed_kills_impl(
            &conn,
            "2026-06-15 10:01:00",
            "2026-06-15 10:04:59",
            Some("Hero"),
            Some("Dreva"),
        )
        .unwrap();
        assert!(narrow.is_empty(), "window excluding both stamps drops them");
    }

    #[test]
    fn empty_and_inverted_windows_return_empty_not_error() {
        let conn = setup();
        seed(&conn);
        // Inverted window: empty vec, never an error (transient while typing
        // a custom range).
        assert!(
            get_timeboxed_kills_impl(
                &conn,
                "2026-06-15 12:00:00",
                "2026-06-15 10:00:00",
                Some("Hero"),
                Some("Dreva")
            )
            .unwrap()
            .is_empty()
        );
        assert!(
            get_timeboxed_loot_impl(
                &conn,
                "2026-06-15 12:00:00",
                "2026-06-15 10:00:00",
                Some("Hero"),
                Some("Dreva")
            )
            .unwrap()
            .is_empty()
        );
        assert!(
            get_timeboxed_xp_impl(
                &conn,
                "2026-06-15 12:00:00",
                "2026-06-15 10:00:00",
                Some("Hero"),
                Some("Dreva")
            )
            .unwrap()
            .is_empty()
        );
        // A window with no rows is empty, not an error.
        assert!(
            get_timeboxed_kills_impl(
                &conn,
                "2020-01-01 00:00:00",
                "2020-01-01 23:59:59",
                Some("Hero"),
                Some("Dreva")
            )
            .unwrap()
            .is_empty()
        );
        // Missing bounds are a caller error.
        assert!(
            get_timeboxed_kills_impl(&conn, "", "2026-06-15 23:59:59", None, None).is_err()
        );
    }

    #[test]
    fn identity_filter_keeps_legacy_null_rows_visible() {
        let conn = setup();
        seed(&conn);
        let day = ("2026-06-15 00:00:00", "2026-06-15 23:59:59");
        // Another character still sees the NULL-identity Spider kill.
        let other = get_timeboxed_kills_impl(&conn, day.0, day.1, Some("Alt"), Some("Dreva")).unwrap();
        assert_eq!(other.len(), 1, "only the legacy NULL row is visible to Alt");
        assert_eq!(other[0].enemy_name, "Spider");
        // No identity filter: global view sees everything in the window.
        let all = get_timeboxed_kills_impl(&conn, day.0, day.1, None, None).unwrap();
        assert_eq!(
            all.iter().map(|k| k.kill_count).sum::<i32>(),
            4,
            "2 Rat + 1 Bat + 1 legacy Spider"
        );
    }

    #[test]
    fn xp_name_falls_back_without_snapshot_row() {
        let conn = setup();
        conn.execute(
            "INSERT INTO chat_xp_awards (character_name, server_name, skill_id, awarded_at, xp, level)
             VALUES ('Hero', 'Dreva', 99, '2026-06-15 13:00:00', 10, 1)",
            [],
        )
        .unwrap();
        let xp = get_timeboxed_xp_impl(
            &conn,
            "2026-06-15 00:00:00",
            "2026-06-15 23:59:59",
            Some("Hero"),
            Some("Dreva"),
        )
        .unwrap();
        let fallback = xp.iter().find(|s| s.skill_id == 99).unwrap();
        assert_eq!(fallback.skill_name, "Skill 99");
        assert_eq!(fallback.levels_gained, 0, "levels never derived in a window");
    }
}
