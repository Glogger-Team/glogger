//! Dataset-scale chat replay guard: the only session-scale accumulation check
//! on the surviving chat ingestion path.
//!
//! The deleted `survey/replay_tests.rs` drove the dead `PlayerEvent` pipeline,
//! so this guard rebuilds only its chat half: the compact full-session fixture
//! `test_data/chat_replay_guard_session.log` (hand-recorded ground truth in
//! its header) replays through the real parse+dispatch path
//! (`parse_chat_line` → [`dispatch_chat_message`]) on a fresh migrated DB,
//! then asserts receipt counts and tallies equal ground truth — no
//! double-count on English/JSON twins or same-second resends, no
//! cross-session bleed — and that a second full pass (scan-equivalent)
//! converges without writing anything new.

#[cfg(test)]
mod tests {
    use crate::arena_parser::ArenaTracker;
    use crate::cdn_commands::GameDataState;
    use crate::chat_combat_parser::ChatCombatEvent;
    use crate::chat_parser::{parse_chat_line, parse_chat_login_line, ChatMessage};
    use crate::chat_status_dispatch::{
        dispatch_chat_message, DispatchCtx, DispatchIdentity, DispatchState,
    };
    use crate::chat_status_parser::ChatStatusEvent;
    use crate::db::{init_pool, DbPool};
    use std::collections::HashMap;

    const HERO: (&str, &str) = ("GuardHero", "Dreva");
    const ALT: (&str, &str) = ("OtherAlt", "OtherAltServer");

    fn test_pool() -> (DbPool, std::path::PathBuf) {
        use std::sync::atomic::{AtomicU64, Ordering};
        static SEQ: AtomicU64 = AtomicU64::new(0);
        let n = SEQ.fetch_add(1, Ordering::Relaxed);
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let path = std::env::temp_dir().join(format!(
            "glogger_replay_guard_test_{}_{nanos}_{n}.db",
            std::process::id()
        ));
        let pool = init_pool(path.clone(), None).expect("init pool");
        (pool, path)
    }

    fn game_data() -> GameDataState {
        std::sync::Arc::new(tauri::async_runtime::RwLock::new(crate::game_data::GameData::empty()))
    }

    fn seed_skill(gd: &GameDataState, name: &str, sid: u32) {
        let mut g = gd.blocking_write();
        g.skills.insert(
            sid,
            crate::game_data::SkillInfo {
                id: sid,
                name: name.to_string(),
                internal_name: name.to_string(),
                description: None,
                icon_id: None,
                xp_table: None,
                keywords: vec![],
                combat: None,
                max_bonus_levels: None,
                parents: vec![],
                advancement_table: None,
                guest_level_cap: None,
                hide_when_zero: None,
                advancement_hints: None,
                rewards: None,
                reports: None,
                raw_json: serde_json::Value::Null,
            },
        );
        g.skill_name_index.insert(name.to_string(), sid);
    }

    struct SessionLine {
        local_ts: String,
        msg: ChatMessage,
        identity: Option<DispatchIdentity>,
    }

    /// Parse the fixture into dispatch-ready lines, tracking the login-marker
    /// session identity exactly like the watcher poll does (markers switch
    /// the attribution; marker lines themselves are never dispatched).
    fn parse_fixture() -> Vec<SessionLine> {
        let fixture = include_str!("../../test_data/chat_replay_guard_session.log");
        let mut out = Vec::new();
        let mut identity: Option<DispatchIdentity> = None;
        for raw in fixture.lines() {
            let line = raw.trim_end_matches('\r');
            if line.trim().is_empty() || line.starts_with('#') {
                continue;
            }
            if let Some(info) = parse_chat_login_line(line) {
                identity = Some(DispatchIdentity {
                    character: info.character_name,
                    server: info.server_name,
                });
                continue;
            }
            let msg = parse_chat_line(line)
                .unwrap_or_else(|| panic!("fixture line must parse: {line}"));
            let local_ts = line.split('\t').next().unwrap_or("").to_string();
            out.push(SessionLine { local_ts, msg, identity: identity.clone() });
        }
        assert!(!out.is_empty(), "fixture parsed zero dispatch lines");
        out
    }

    struct PassTotals {
        staged_pushes: usize,
        staged_qty: u32,
        alt_staged: usize,
        xp_events: u32,
        xp_sum: u32,
        levelups: u32,
    }

    fn dispatch_pass(
        pool: &DbPool,
        gd: &GameDataState,
        lines: &[SessionLine],
        is_scan: bool,
        count_currency: bool,
    ) -> PassTotals {
        let mut arena = ArenaTracker::new();
        let mut dmg: Vec<ChatCombatEvent> = Vec::new();
        let mut totals = PassTotals {
            staged_pushes: 0,
            staged_qty: 0,
            alt_staged: 0,
            xp_events: 0,
            xp_sum: 0,
            levelups: 0,
        };
        for line in lines {
            let mut st = DispatchState {
                arena_tracker: &mut arena,
                recent_damage: &mut dmg,
                current_area: None,
                count_currency,
            };
            let outcome = dispatch_chat_message(
                DispatchCtx {
                    db: pool,
                    game_data: gd,
                    identity: line.identity.as_ref(),
                    msg: &line.msg,
                    local_ts: &line.local_ts,
                    watch_rules: &[],
                    auto_watch_rules: &[],
                    // Session-scale convergence pins legacy collapse:
                    // distinct ids would turn its same-second resends into
                    // genuine duplicates. `None` keeps both passes stable.
                    source_line_id: None,
                    is_scan,
                    is_live: !is_scan,
                },
                None,
                &mut st,
            );
            if line.identity.as_ref().is_some_and(|id| id.character == HERO.0) {
                totals.staged_pushes += outcome.staged_gains.len();
                totals.staged_qty += outcome.staged_gains.iter().map(|(_, q)| *q).sum::<u32>();
            } else {
                totals.alt_staged += outcome.staged_gains.len();
            }
            match &outcome.chat_status_event {
                Some(ChatStatusEvent::XpGained { amount, .. }) => {
                    totals.xp_events += 1;
                    totals.xp_sum += *amount;
                }
                Some(ChatStatusEvent::LevelUp { .. }) => totals.levelups += 1,
                _ => {}
            }
        }
        totals
    }

    fn ledger_rows(pool: &DbPool, character: &str, server: &str) -> i64 {
        let conn = pool.get_write().unwrap();
        conn.query_row(
            "SELECT COUNT(*) FROM item_transactions
             WHERE character_name = ?1 AND server_name = ?2 AND source = 'chat_status'",
            rusqlite::params![character, server],
            |r| r.get(0),
        )
        .unwrap()
    }

    fn ledger_qty(pool: &DbPool) -> HashMap<String, i64> {
        let conn = pool.get_write().unwrap();
        let mut stmt = conn
            .prepare(
                "SELECT item_name, SUM(quantity) FROM item_transactions
                 WHERE character_name = 'GuardHero' AND server_name = 'Dreva'
                   AND source = 'chat_status' GROUP BY item_name",
            )
            .unwrap();
        stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))
            .unwrap()
            .map(|r| r.unwrap())
            .collect()
    }

    fn currency_delta(pool: &DbPool, character: &str, server: &str) -> Option<i64> {
        let conn = pool.get_write().unwrap();
        conn.query_row(
            "SELECT delta_since FROM currency_estimate
             WHERE character_name = ?1 AND server_name = ?2 AND currency_name = 'GOLD'",
            rusqlite::params![character, server],
            |r| r.get(0),
        )
        .ok()
    }

    fn skill_level_xp(pool: &DbPool, skill: &str) -> (i64, i64) {
        let conn = pool.get_write().unwrap();
        conn.query_row(
            "SELECT level, xp FROM game_state_skills
             WHERE character_name = 'GuardHero' AND server_name = 'Dreva' AND skill_name = ?1",
            rusqlite::params![skill],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap()
    }

    #[test]
    fn session_scale_chat_replay_converges() {
        let (pool, path) = test_pool();
        let gd = game_data();
        seed_skill(&gd, "Sword", 7);
        seed_skill(&gd, "Endurance", 51);
        // Wallet anchor for the hero only: every session-A councils line is
        // newer, so each accrues; the anchor-less alt session stays a no-op.
        pool.get_write()
            .unwrap()
            .execute(
                "INSERT INTO currency_estimate
                    (character_name, server_name, currency_name, anchor_amount, anchor_at, delta_since)
                 VALUES ('GuardHero','Dreva','GOLD',1000,'2026-11-22 07:00:00',0)",
                [],
            )
            .unwrap();

        let lines = parse_fixture();

        // Pass 1: live-equivalent drain over the whole session.
        let first = dispatch_pass(&pool, &gd, &lines, false, true);

        assert_eq!(ledger_rows(&pool, HERO.0, HERO.1), 11, "hero ledger rows");
        let qty = ledger_qty(&pool);
        let expected: HashMap<String, i64> = [
            ("Tiny Brain Bug Meat", 1),
            ("Mandrake Root", 6),
            ("Silver Ring", 2),
            ("Toxic Sludge", 2),
            ("Serpentine", 3),
            ("Peridot", 4),
            ("Copper Ore", 6),
            ("Oak Wood", 8),
            ("Spider Silk", 1),
            ("Button Mushroom", 5),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_string(), v))
        .collect();
        assert_eq!(qty, expected, "hero per-item tallies");
        assert_eq!(
            qty.values().sum::<i64>(),
            38,
            "hero total gained quantity"
        );
        // Both twin orders collapse to one stamped row.
        for twin in ["Toxic Sludge", "Serpentine"] {
            let conn = pool.get_write().unwrap();
            let (n, from_name, from_entity): (i64, Option<String>, Option<i64>) = conn
                .query_row(
                    "SELECT COUNT(*), MAX(from_name), MAX(from_entity_id) FROM item_transactions
                     WHERE character_name = 'GuardHero' AND server_name = 'Dreva'
                       AND item_name = ?1 AND source = 'chat_status'",
                    rusqlite::params![twin],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
                )
                .unwrap();
            assert_eq!(n, 1, "{twin} twin collapses to one row");
            assert_eq!(from_name.as_deref(), Some("Olugax The Ever-Pudding"));
            assert_eq!(from_entity, Some(257400));
        }
        assert_eq!(first.staged_pushes, 11, "exactly-once staging");
        assert_eq!(first.staged_qty, 38, "staged quantity matches ledger");
        assert_eq!(first.alt_staged, 1, "alt session stages its own gain");
        assert_eq!(currency_delta(&pool, HERO.0, HERO.1), Some(16), "councils net");
        assert_eq!(skill_level_xp(&pool, "Sword"), (5, 192), "Sword level-up");
        assert_eq!(skill_level_xp(&pool, "Endurance"), (3, 600), "Endurance awards");
        // English XP lines emit live events but persist nothing.
        assert_eq!((first.xp_events, first.xp_sum), (2, 102));
        assert_eq!(first.levelups, 1);
        // Cross-session bleed: the alt session wrote exactly its own gain,
        // and its anchor-less councils line accrued nowhere.
        assert_eq!(ledger_rows(&pool, ALT.0, ALT.1), 1, "alt ledger rows");
        assert_eq!(currency_delta(&pool, ALT.0, ALT.1), None, "alt has no wallet row");

        // Pass 2: full rescan over the same lines converges — nothing new.
        let second = dispatch_pass(&pool, &gd, &lines, true, false);
        assert_eq!(second.staged_pushes, 0, "rescan stages nothing");
        assert_eq!(second.alt_staged, 0, "rescan stages nothing for alt");
        assert_eq!(ledger_qty(&pool), expected, "tallies stable");
        assert_eq!(currency_delta(&pool, HERO.0, HERO.1), Some(16), "wallet stable");
        assert_eq!(skill_level_xp(&pool, "Sword"), (5, 192), "Sword stable");
        assert_eq!(skill_level_xp(&pool, "Endurance"), (3, 600), "Endurance stable");
        assert_eq!(ledger_rows(&pool, ALT.0, ALT.1), 1, "alt ledger stable");

        drop(pool);
        for suffix in ["", "-wal", "-shm", "-journal"] {
            let mut victim = path.as_os_str().to_owned();
            victim.push(suffix);
            let _ = std::fs::remove_file(victim);
        }
    }
}
