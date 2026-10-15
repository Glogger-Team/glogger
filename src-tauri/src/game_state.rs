use crate::cdn_commands::GameDataState;
use crate::db::DbPool;
use crate::parsers::LogDateResolver;
/// Game State Manager — persists derived game state from PlayerEvents to SQLite.
///
/// Follows the SurveySessionTracker pattern: lightweight struct that receives
/// &DbPool per call, called synchronously from the coordinator's event loop.
/// Maintains "last known value" tables, not event logs.
use crate::player_event_parser::{DeleteContext, PlayerEvent};
use chrono::{Datelike, Duration, Local, Utc};
use rusqlite::Connection;

/// Timestamped log line for startup diagnostics.
macro_rules! startup_log {
    ($($arg:tt)*) => {
        eprintln!("[{}] {}", Local::now().format("%H:%M:%S%.3f"), format!($($arg)*));
    };
}

// to_datetime removed — use GameStateManager::to_utc(ts) instead

/// Monday 00:00 America/New_York of the current week, in the DB timestamp
/// format (`%Y-%m-%d %H:%M:%S`, UTC). The game's own gift-cap note says "The
/// week resets on Monday EDT.", so the weekly boundary is Eastern time — not
/// Monday 00:00 UTC (the two differ by 4–5 hours, which misattributes every
/// gift given around the reset). Matches the frontend's boundary
/// (`useStatehelmTracker.getCurrentWeekStart`).
pub(crate) fn current_week_start_utc() -> String {
    week_start_utc_at(Utc::now())
}

/// [`current_week_start_utc`] for an arbitrary instant (test seam).
pub(crate) fn week_start_utc_at(now: chrono::DateTime<Utc>) -> String {
    // Work in Eastern wall-clock terms so the week is the game's week: on a UTC
    // Monday before 04:00/05:00 the game is still in the previous week.
    let eastern = now - Duration::hours(eastern_utc_offset_hours(now.date_naive()) as i64);
    let days_since_monday = eastern.weekday().num_days_from_monday() as i64;
    let monday = eastern.date_naive() - Duration::days(days_since_monday);
    let start = monday
        .and_hms_opt(eastern_utc_offset_hours(monday), 0, 0)
        .unwrap();
    start.format("%Y-%m-%d %H:%M:%S").to_string()
}

/// UTC offset of America/New_York on `date`: 4 (EDT) from the second Sunday in
/// March to the first Sunday in November, else 5 (EST). The weekly reset is
/// Monday 00:00 local and DST transitions land on Sundays at 02:00, so the
/// offset is decided by the date alone — no tz database dependency needed.
fn eastern_utc_offset_hours(date: chrono::NaiveDate) -> u32 {
    /// `nth` occurrence (1-based) of Sunday in `month`.
    fn nth_sunday(year: i32, month: u32, nth: u32) -> chrono::NaiveDate {
        let first = chrono::NaiveDate::from_ymd_opt(year, month, 1).unwrap();
        let first_sunday = 1 + (7 - first.weekday().num_days_from_sunday()) % 7;
        chrono::NaiveDate::from_ymd_opt(year, month, first_sunday + 7 * (nth - 1)).unwrap()
    }

    let dst_start = nth_sunday(date.year(), 3, 2);
    let dst_end = nth_sunday(date.year(), 11, 1);
    if date >= dst_start && date < dst_end {
        4
    } else {
        5
    }
}

/// Result of processing a player event
pub struct ProcessResult {
    /// Which data domains were updated (e.g., ["skills", "attributes"])
    pub domains_updated: Vec<&'static str>,
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
    /// NPC key of the vendor currently being interacted with (set on InteractionStarted, cleared on InteractionEnded)
    current_vendor_npc: Option<String>,
    /// Dates for Player.log `HH:MM:SS` timestamps: an explicit override
    /// (replay / old-log reparse), else the session date declared by the log's
    /// own login line, else the clock. See [`LogDateResolver`].
    log_dates: LogDateResolver,
}

impl GameStateManager {
    pub fn new(game_data: GameDataState) -> Self {
        Self {
            active_character: None,
            active_server: None,
            live_mode: false,
            game_data,
            current_vendor_npc: None,
            log_dates: LogDateResolver::default(),
        }
    }

    /// Stamp Player.log times with an explicit UTC date instead of today's.
    /// Live tailing leaves this unset; replay / old-log reparse sets it so
    /// DB writes carry the correct historical date.
    pub fn set_base_date(&mut self, date: chrono::NaiveDate) {
        self.log_dates.set_base_date(date);
    }

    /// Anchor the session date from the Player.log login line's own
    /// `Time UTC=` (`PlayerEvent::LogSessionAnchored`): every `HH:MM:SS` line
    /// after it is dated by the session it came from, however much the read lags
    /// the log. Ignored while an explicit base date is set, which is already
    /// authoritative (replay / old-log reparse).
    pub fn anchor_log_session(&mut self, utc: chrono::NaiveDateTime) {
        self.log_dates.anchor_session(utc);
    }

    /// Mark that catch-up replay is complete and future logins are live.
    pub fn set_live_mode(&mut self) {
        self.live_mode = true;
    }

    /// Check if we are in live mode (catch-up complete).
    pub fn is_live(&self) -> bool {
        self.live_mode
    }

    /// Debug: get active character name
    pub fn get_active_character(&self) -> Option<&str> {
        self.active_character.as_deref()
    }

    /// Debug: get active server name
    pub fn get_active_server(&self) -> Option<&str> {
        self.active_server.as_deref()
    }

    /// Clear the active character so that events are silently dropped.
    /// Used during catch-up replay to skip events for non-selected characters.
    pub fn clear_active_character(&mut self) {
        self.active_character = None;
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

    /// Convert a Player.log HH:MM:SS timestamp to a full UTC datetime string.
    /// Player.log timestamps are already UTC — just needs a date component added.
    ///
    /// Precedence lives in [`LogDateResolver`]: explicit base date (replay /
    /// old-log reparse) → the session date from the log's own login line →
    /// the clock.
    ///
    /// Public so coordinator event handlers (which persist their own rows, e.g.
    /// milking) stamp events with the log's time instead of the ingest time.
    pub fn to_utc(&mut self, ts: &str) -> String {
        self.log_dates.resolve(ts)
    }

    /// [`Self::to_utc`] **without** advancing the session rollover baseline.
    ///
    /// For side channels that read a line before the batch it belongs to is
    /// replayed: the coordinator resolves milking `ScreenText` timestamps while
    /// iterating, then `process_events_batch` resolves the same batch in log
    /// order. Advancing twice, out of order, would roll the session date a day
    /// forward for every row after it. See [`crate::parsers::LogDateResolver::peek`].
    pub fn peek_utc(&self, ts: &str) -> String {
        self.log_dates.peek(ts)
    }

    /// Update the active character.
    /// During live mode, clears transient state so the login burst can repopulate.
    /// During replay/catch-up, skips clearing to preserve data built for each character.
    pub fn set_active_character(&mut self, name: &str, server: &str, db: &DbPool) {
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

        let now = Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
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

                // Reset favor deltas — new session starts accumulating from scratch
                tx.execute(
                    "UPDATE game_state_favor SET cumulative_delta = 0, source = CASE WHEN favor_tier IS NOT NULL THEN 'snapshot' ELSE source END WHERE character_name = ?1 AND server_name = ?2",
                    rusqlite::params![name, server],
                )?;
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

    /// Process a batch of PlayerEvents in a single SQLite transaction.
    /// Reduces DB overhead during rapid-fire events (e.g., spam-crafting).
    pub fn process_events_batch(&mut self, events: &[PlayerEvent], db: &DbPool) -> ProcessResult {
        let character = match &self.active_character {
            Some(c) => c.clone(),
            None => return ProcessResult { domains_updated: vec![] },
        };
        let server = match &self.active_server {
            Some(s) => s.clone(),
            None => return ProcessResult { domains_updated: vec![] },
        };
        let mut conn = match db.get_write() {
            Ok(c) => c,
            Err(e) => {
                eprintln!("[game_state] DB error on process_events_batch: {e}");
                return ProcessResult { domains_updated: vec![] };
            }
        };
        let game_data_arc = self.game_data.clone();
        let game_data_guard = game_data_arc.try_read().ok();

        let mut all_domains = Vec::new();
        let tx = match conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate) {
            Ok(tx) => tx,
            Err(e) => {
                eprintln!("[game_state] begin transaction failed: {e}");
                return ProcessResult { domains_updated: vec![] };
            }
        };
        for event in events {
            let mut domains = Vec::new();
            self.process_event_inner(event, &tx, &character, &server, &game_data_guard, &mut domains);
            all_domains.extend(domains);
        }
        if let Err(e) = tx.commit() {
            eprintln!("[game_state] commit failed: {e}");
        }

        all_domains.sort_unstable();
        all_domains.dedup();
        ProcessResult { domains_updated: all_domains }
    }

    /// Inner implementation shared by process_events_batch (and formerly process_event).
    fn process_event_inner(
        &mut self,
        event: &PlayerEvent,
        conn: &rusqlite::Connection,
        character: &str,
        server: &str,
        game_data_guard: &Option<tokio::sync::RwLockReadGuard<'_, crate::game_data::GameData>>,
        domains: &mut Vec<&'static str>,
    ) {

        match event {
            PlayerEvent::LogSessionAnchored { utc } => {
                // Not a persisted event: it only tells us which UTC day the
                // lines after it belong to (see `to_utc`).
                self.anchor_log_session(*utc);
            }

            PlayerEvent::SkillsLoaded { timestamp, skills } => {
                let dt = self.to_utc(timestamp);
                // Full skill dump on login — replace all skills for this character+server
                conn.execute(
                    "DELETE FROM game_state_skills WHERE character_name = ?1 AND server_name = ?2",
                    rusqlite::params![character, server],
                )
                .ok();

                let mut stmt = conn.prepare(
                    "INSERT INTO game_state_skills (character_name, server_name, skill_id, skill_name, level, base_level, bonus_levels, xp, tnl, max_level, last_confirmed_at, source)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, 'log')"
                ).ok();

                if let Some(stmt) = stmt.as_mut() {
                    for skill in skills {
                        // Resolve skill internal name → canonical ID + display name
                        let (skill_id, display_name) = match &game_data_guard {
                            Some(data) => match data.resolve_skill(&skill.skill_type) {
                                Some(info) => (info.id as i64, info.name.clone()),
                                None => (0i64, skill.skill_type.clone()),
                            },
                            None => (0i64, skill.skill_type.clone()),
                        };

                        let total_level = skill.raw as i32 + skill.bonus as i32;
                        stmt.execute(rusqlite::params![
                            character,
                            server,
                            skill_id,
                            display_name,
                            total_level, // level (total = base + bonus)
                            skill.raw,   // base_level (raw, without bonuses)
                            skill.bonus,
                            skill.xp,
                            skill.tnl,
                            skill.max,
                            dt,
                        ])
                        .ok();
                    }
                }
                domains.push("skills");
            }

            PlayerEvent::ActiveSkillsChanged {
                timestamp,
                skill1,
                skill2,
            } => {
                let dt = self.to_utc(timestamp);
                // Resolve skill internal names → IDs + display names
                let (skill1_id, skill1_name) = match &game_data_guard {
                    Some(data) => match data.resolve_skill(skill1) {
                        Some(info) => (info.id as i64, info.name.clone()),
                        None => (0i64, skill1.clone()),
                    },
                    None => (0i64, skill1.clone()),
                };
                let (skill2_id, skill2_name) = match &game_data_guard {
                    Some(data) => match data.resolve_skill(skill2) {
                        Some(info) => (info.id as i64, info.name.clone()),
                        None => (0i64, skill2.clone()),
                    },
                    None => (0i64, skill2.clone()),
                };

                conn.execute(
                    "INSERT INTO game_state_active_skills (character_name, server_name, skill1_id, skill1_name, skill2_id, skill2_name, last_confirmed_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                     ON CONFLICT(character_name, server_name) DO UPDATE SET
                        skill1_id = excluded.skill1_id,
                        skill1_name = excluded.skill1_name,
                        skill2_id = excluded.skill2_id,
                        skill2_name = excluded.skill2_name,
                        last_confirmed_at = excluded.last_confirmed_at",
                    rusqlite::params![character, server, skill1_id, skill1_name, skill2_id, skill2_name, dt],
                ).ok();
                domains.push("active_skills");
            }

            PlayerEvent::AttributesChanged {
                timestamp,
                attributes,
                ..
            } => {
                let dt = self.to_utc(timestamp);
                // Batch upsert in a savepoint for performance (works nested or standalone)
                conn.execute("SAVEPOINT attributes_batch", []).ok();
                {
                    let mut stmt = conn.prepare(
                        "INSERT INTO game_state_attributes (character_name, server_name, attribute_name, value, last_confirmed_at, min_value, max_value)
                         VALUES (?1, ?2, ?3, ?4, ?5, ?4, ?4)
                         ON CONFLICT(character_name, server_name, attribute_name) DO UPDATE SET
                            value = excluded.value,
                            last_confirmed_at = excluded.last_confirmed_at,
                            min_value = MIN(COALESCE(game_state_attributes.min_value, excluded.value), excluded.value),
                            max_value = MAX(COALESCE(game_state_attributes.max_value, excluded.value), excluded.value)"
                    ).ok();

                    if let Some(stmt) = stmt.as_mut() {
                        for attr in attributes {
                            stmt.execute(rusqlite::params![
                                character, server, attr.name, attr.value, dt,
                            ])
                            .ok();
                        }
                    }
                }
                conn.execute("RELEASE attributes_batch", []).ok();
                domains.push("attributes");
            }

            PlayerEvent::WeatherChanged {
                timestamp,
                weather_name,
                is_active,
            } => {
                let dt = self.to_utc(timestamp);
                conn.execute(
                    "INSERT INTO game_state_weather (id, weather_name, is_active, last_confirmed_at)
                     VALUES (1, ?1, ?2, ?3)
                     ON CONFLICT(id) DO UPDATE SET
                        weather_name = excluded.weather_name,
                        is_active = excluded.is_active,
                        last_confirmed_at = excluded.last_confirmed_at",
                    rusqlite::params![weather_name, *is_active as i32, dt],
                ).ok();
                domains.push("weather");
            }

            PlayerEvent::CombatStateChanged {
                timestamp,
                in_combat,
            } => {
                let dt = self.to_utc(timestamp);
                conn.execute(
                    "INSERT INTO game_state_combat (character_name, server_name, in_combat, last_confirmed_at)
                     VALUES (?1, ?2, ?3, ?4)
                     ON CONFLICT(character_name, server_name) DO UPDATE SET
                        in_combat = excluded.in_combat,
                        last_confirmed_at = excluded.last_confirmed_at",
                    rusqlite::params![character, server, *in_combat as i32, dt],
                ).ok();
                domains.push("combat");
            }

            PlayerEvent::MountStateChanged {
                timestamp,
                is_mounting,
                ..
            } => {
                let dt = self.to_utc(timestamp);
                conn.execute(
                    "INSERT INTO game_state_mount (character_name, server_name, is_mounted, last_confirmed_at)
                     VALUES (?1, ?2, ?3, ?4)
                     ON CONFLICT(character_name, server_name) DO UPDATE SET
                        is_mounted = excluded.is_mounted,
                        last_confirmed_at = excluded.last_confirmed_at",
                    rusqlite::params![character, server, *is_mounting as i32, dt],
                ).ok();
                domains.push("mount");
            }

            PlayerEvent::ItemAdded {
                timestamp,
                item_name,
                instance_id,
                slot_index,
                is_new,
                initial_quantity,
                provenance,
            } => {
                let dt = self.to_utc(timestamp);
                // Resolve item_type_id and display name from CDN game data
                let resolved = game_data_guard
                    .as_ref()
                    .and_then(|gd| gd.resolve_item(item_name));
                let item_type_id: Option<i64> = resolved.map(|info| info.id as i64);
                let display_name = resolved.map(|info| info.name.as_str());
                conn.execute(
                    "INSERT INTO game_state_inventory (character_name, server_name, instance_id, item_name, item_type_id, stack_size, slot_index, last_confirmed_at, source)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 'log')
                     ON CONFLICT(character_name, server_name, instance_id) DO UPDATE SET
                        item_name = excluded.item_name,
                        item_type_id = COALESCE(excluded.item_type_id, game_state_inventory.item_type_id),
                        stack_size = excluded.stack_size,
                        slot_index = excluded.slot_index,
                        last_confirmed_at = excluded.last_confirmed_at",
                    rusqlite::params![character, server, *instance_id as i64, item_name, item_type_id, *initial_quantity, slot_index, dt],
                ).ok();
                // Record transaction (only for genuinely new items, not login reloads)
                if *is_new {
                    Self::record_transaction(
                        &conn,
                        character,
                        server,
                        &dt,
                        display_name.unwrap_or(item_name),
                        Some(item_name),
                        item_type_id,
                        *initial_quantity as i32,
                        "loot",
                        "player_log",
                        Some(*instance_id),
                        None,
                        Some(provenance.to_columns()),
                    );
                }
                domains.push("inventory");
            }

            PlayerEvent::ItemStackChanged {
                timestamp,
                instance_id,
                item_name,
                item_type_id,
                new_stack_size,
                delta,
                provenance,
                ..
            } => {
                let dt = self.to_utc(timestamp);
                // Update existing row, or insert if ItemAdded hasn't arrived yet
                let name = item_name.as_deref().unwrap_or("Unknown Item");
                conn.execute(
                    "INSERT INTO game_state_inventory (character_name, server_name, instance_id, item_name, item_type_id, stack_size, last_confirmed_at, source)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'log')
                     ON CONFLICT(character_name, server_name, instance_id) DO UPDATE SET
                        stack_size = excluded.stack_size,
                        item_type_id = COALESCE(excluded.item_type_id, game_state_inventory.item_type_id),
                        last_confirmed_at = excluded.last_confirmed_at",
                    rusqlite::params![character, server, *instance_id as i64, name, *item_type_id as i32, new_stack_size, dt],
                ).ok();

                // Record a gain transaction for positive deltas so the ledger
                // captures stack merges (same-item pickups on top of an existing
                // stack never fire ItemAdded). Negative deltas are handled by
                // ItemDeleted when the stack empties; partial decrements don't
                // currently surface as transactions.
                if *delta > 0 {
                    // Resolve the display name so transaction rows use the
                    // same name format as ItemAdded and ScreenText markers —
                    // this is what lets the speed-bonus patch find them.
                    let resolved = item_name
                        .as_deref()
                        .and_then(|n| game_data_guard.as_ref().and_then(|gd| gd.resolve_item(n)));
                    let display_name = resolved.map(|info| info.name.as_str()).unwrap_or(name);
                    let item_type_id_i64 = if *item_type_id != 0 { Some(*item_type_id as i64) } else { None };
                    Self::record_transaction(
                        &conn,
                        character,
                        server,
                        &dt,
                        display_name,
                        item_name.as_deref(),
                        item_type_id_i64,
                        *delta,
                        "loot",
                        "player_log",
                        Some(*instance_id),
                        None,
                        Some(provenance.to_columns()),
                    );
                }
                domains.push("inventory");
            }

            PlayerEvent::ItemDeleted {
                timestamp,
                instance_id,
                item_name,
                context,
            } => {
                // Look up stack_size before deleting so we can record accurate quantity
                let (del_item_name, del_stack_size): (String, i32) = conn
                    .query_row(
                        "SELECT item_name, stack_size FROM game_state_inventory
                         WHERE character_name = ?1 AND server_name = ?2 AND instance_id = ?3",
                        rusqlite::params![character, server, *instance_id as i64],
                        |row| Ok((row.get(0)?, row.get(1)?)),
                    )
                    .unwrap_or_else(|_| {
                        (
                            item_name.clone().unwrap_or_else(|| "Unknown".to_string()),
                            1,
                        )
                    });

                conn.execute(
                    "DELETE FROM game_state_inventory WHERE character_name = ?1 AND server_name = ?2 AND instance_id = ?3",
                    rusqlite::params![character, server, *instance_id as i64],
                ).ok();

                let tx_context = match context {
                    DeleteContext::StorageTransfer => "storage_deposit",
                    DeleteContext::VendorSale => "vendor_sell",
                    DeleteContext::Consumed => "consumed",
                    DeleteContext::Unknown => "unknown",
                };
                let dt = self.to_utc(timestamp);
                Self::record_transaction(
                    &conn,
                    character,
                    server,
                    &dt,
                    &del_item_name,
                    item_name.as_deref(),
                    None,
                    -del_stack_size,
                    tx_context,
                    "player_log",
                    Some(*instance_id),
                    None,
                    // Deletes carry DeleteContext (storage/vendor/consumed/unknown)
                    // on the event itself rather than ItemProvenance. Not a "gain",
                    // so source_kind is left NULL.
                    None,
                );
                domains.push("inventory");
            }

            PlayerEvent::RecipeUpdated {
                timestamp,
                recipe_id,
                completion_count,
            } => {
                let dt = self.to_utc(timestamp);
                conn.execute(
                    "INSERT INTO game_state_recipes (character_name, server_name, recipe_id, completion_count, last_confirmed_at, source)
                     VALUES (?1, ?2, ?3, ?4, ?5, 'log')
                     ON CONFLICT(character_name, server_name, recipe_id) DO UPDATE SET
                        completion_count = excluded.completion_count,
                        last_confirmed_at = excluded.last_confirmed_at,
                        source = excluded.source",
                    rusqlite::params![character, server, recipe_id, completion_count, dt],
                ).ok();
                domains.push("recipes");
            }

            PlayerEvent::FavorChanged {
                timestamp,
                npc_name,
                delta,
                is_gift,
                ..
            } => {
                let dt = self.to_utc(timestamp);
                // Resolve NPC display name → CDN key (e.g., "Kalaba" → "NPC_Kalaba")
                let (npc_key, display_name) = match &game_data_guard {
                    Some(data) => match data.resolve_npc(npc_name) {
                        Some(info) => (info.key.clone(), info.name.clone()),
                        None => (npc_name.clone(), npc_name.clone()),
                    },
                    None => (npc_name.clone(), npc_name.clone()),
                };

                conn.execute(
                    "INSERT INTO game_state_favor (character_name, server_name, npc_key, npc_name, cumulative_delta, last_confirmed_at, source)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'log')
                     ON CONFLICT(character_name, server_name, npc_key) DO UPDATE SET
                        cumulative_delta = game_state_favor.cumulative_delta + excluded.cumulative_delta,
                        npc_name = excluded.npc_name,
                        last_confirmed_at = excluded.last_confirmed_at,
                        source = CASE WHEN game_state_favor.source = 'snapshot' THEN 'both' ELSE 'log' END",
                    rusqlite::params![character, server, npc_key, display_name, *delta as f64, dt],
                ).ok();

                // Log individual gift events for weekly gift-limit tracking
                if *is_gift {
                    conn.execute(
                        "INSERT INTO game_state_gift_log (character_name, server_name, npc_key, npc_name, gifted_at, favor_delta)
                         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                        rusqlite::params![character, server, npc_key, display_name, dt, *delta as f64],
                    ).ok();
                }

                domains.push("favor");
            }

            PlayerEvent::GiftCountObserved {
                timestamp,
                npc_name,
                received,
                ..
            } => {
                let dt = self.to_utc(timestamp);
                // Resolve display name (e.g. "Corinth") → CDN key + canonical name.
                let (npc_key, display_name) = match &game_data_guard {
                    Some(data) => match data.resolve_npc(npc_name) {
                        Some(info) => (info.key.clone(), info.name.clone()),
                        None => (npc_name.clone(), npc_name.clone()),
                    },
                    None => (npc_name.clone(), npc_name.clone()),
                };

                // Reconcile the week's gift-log rows for this NPC to the game's
                // authoritative count. A bulk gift (a stack given in one action)
                // emits a single ProcessDeltaFavor line, so the per-favor row count
                // can undershoot; this corrects it without disturbing other NPCs.
                // A note the game printed before this week began reports the
                // *previous* week's count. Reconciling it would write rows into
                // last week — or, when this week's count is already higher,
                // trim this week's real gifts down to last week's number. The
                // backfill drops these the same way, so the two paths agree.
                let week_start = current_week_start_utc();
                if dt >= week_start
                    && crate::db::game_state_commands::reconcile_gift_count(
                        conn,
                        character,
                        server,
                        &npc_key,
                        &display_name,
                        &dt,
                        *received,
                        &week_start,
                    )
                    .unwrap_or(false)
                {
                    domains.push("favor");
                }
            }

            PlayerEvent::EquipmentChanged {
                timestamp,
                equipment,
                ..
            } => {
                let dt = self.to_utc(timestamp);
                // Full equipment state — replace all slots
                conn.execute("DELETE FROM game_state_equipment WHERE character_name = ?1 AND server_name = ?2",
                    rusqlite::params![character, server]).ok();

                let mut stmt = conn.prepare(
                    "INSERT INTO game_state_equipment (character_name, server_name, slot, appearance_key, last_confirmed_at)
                     VALUES (?1, ?2, ?3, ?4, ?5)"
                ).ok();

                if let Some(stmt) = stmt.as_mut() {
                    for slot in equipment {
                        stmt.execute(rusqlite::params![
                            character,
                            server,
                            slot.slot,
                            slot.appearance_key,
                            dt,
                        ])
                        .ok();
                    }
                }
                domains.push("equipment");
            }

            PlayerEvent::EffectsAdded {
                timestamp,
                source_entity_id,
                effect_ids,
                is_login_batch,
                ..
            } => {
                let dt = self.to_utc(timestamp);

                // Login batch = full state dump: clear existing effects first
                if *is_login_batch {
                    conn.execute(
                        "DELETE FROM game_state_effects WHERE character_name = ?1 AND server_name = ?2",
                        rusqlite::params![character, server],
                    ).ok();
                }

                let mut stmt = conn.prepare(
                    "INSERT INTO game_state_effects (character_name, server_name, effect_instance_id, source_entity_id, last_confirmed_at)
                     VALUES (?1, ?2, ?3, ?4, ?5)
                     ON CONFLICT(character_name, server_name, effect_instance_id) DO UPDATE SET
                        source_entity_id = excluded.source_entity_id,
                        last_confirmed_at = excluded.last_confirmed_at"
                ).ok();

                if let Some(stmt) = stmt.as_mut() {
                    for id in effect_ids {
                        stmt.execute(rusqlite::params![
                            character,
                            server,
                            *id as i64,
                            *source_entity_id as i64,
                            dt,
                        ])
                        .ok();
                    }
                }
                domains.push("effects");
            }

            PlayerEvent::EffectsRemoved { .. } => {
                // ProcessRemoveEffects prints opaque System.Int32[] — we can't determine
                // which effects were removed. Emit domain update so frontend can re-query.
                // The stale entries will be cleaned up on next login batch.
                domains.push("effects");
            }

            PlayerEvent::EffectNameUpdated {
                timestamp,
                effect_instance_id,
                display_name,
                ..
            } => {
                let dt = self.to_utc(timestamp);
                conn.execute(
                    "UPDATE game_state_effects SET effect_name = ?1, last_confirmed_at = ?2
                     WHERE character_name = ?3 AND server_name = ?4 AND effect_instance_id = ?5",
                    rusqlite::params![
                        display_name,
                        dt,
                        character,
                        server,
                        *effect_instance_id as i64
                    ],
                )
                .ok();
                domains.push("effects");
            }

            PlayerEvent::StorageDeposit {
                timestamp,
                vault_key,
                instance_id,
                item_name,
                slot,
                ..
            } => {
                if let Some(vk) = vault_key {
                    let dt = self.to_utc(timestamp);
                    let display_name = game_data_guard
                        .as_ref()
                        .and_then(|gd| gd.resolve_item(item_name))
                        .map(|info| info.name.clone());
                    conn.execute(
                        "INSERT INTO game_state_storage (character_name, server_name, vault_key, instance_id, item_name, stack_size, slot_index, last_confirmed_at, source)
                         VALUES (?1, ?2, ?3, ?4, ?5, 1, ?6, ?7, 'log')
                         ON CONFLICT(character_name, server_name, vault_key, instance_id) DO UPDATE SET
                            item_name = excluded.item_name,
                            slot_index = excluded.slot_index,
                            last_confirmed_at = excluded.last_confirmed_at",
                        rusqlite::params![character, server, vk, *instance_id as i64, item_name, slot, dt],
                    ).ok();
                    Self::record_transaction(
                        &conn,
                        character,
                        server,
                        &dt,
                        display_name.as_deref().unwrap_or(item_name),
                        Some(item_name),
                        None,
                        1, // stack_size=1 initially; corrected later
                        "storage_deposit",
                        "player_log",
                        Some(*instance_id),
                        Some(vk),
                        // Deposits move items between player-owned storages
                        // (inventory ↔ vault). Not a "gain" and not ascribed to
                        // any activity context.
                        None,
                    );
                    domains.push("storage");
                }
            }

            PlayerEvent::StorageWithdrawal {
                timestamp,
                vault_key,
                instance_id,
                quantity,
                provenance,
                ..
            } => {
                if let Some(vk) = vault_key {
                    // Look up item name before deleting
                    let stored_name: String = conn
                        .query_row(
                            "SELECT item_name FROM game_state_storage
                             WHERE character_name = ?1 AND server_name = ?2 AND vault_key = ?3 AND instance_id = ?4",
                            rusqlite::params![character, server, vk, *instance_id as i64],
                            |row| row.get(0),
                        )
                        .unwrap_or_else(|_| "Unknown".to_string());

                    conn.execute(
                        "DELETE FROM game_state_storage WHERE character_name = ?1 AND server_name = ?2 AND vault_key = ?3 AND instance_id = ?4",
                        rusqlite::params![character, server, vk, *instance_id as i64],
                    ).ok();

                    let dt = self.to_utc(timestamp);
                    Self::record_transaction(
                        &conn,
                        character,
                        server,
                        &dt,
                        &stored_name,
                        None,
                        None,
                        -(*quantity as i32),
                        "storage_withdraw",
                        "player_log",
                        Some(*instance_id),
                        Some(vk),
                        // Negative quantity here represents the vault side of
                        // a withdrawal; pair with the inventory gain from
                        // ProcessAddItem that carries the provenance proper.
                        // Still attach provenance so the withdrawal row can be
                        // grouped with its counterpart gain when querying.
                        Some(provenance.to_columns()),
                    );
                    domains.push("storage");
                }
            }

            PlayerEvent::InteractionStarted {
                timestamp,
                npc_name,
                ..
            } => {
                let dt = self.to_utc(timestamp);
                let (npc_key, _display_name) = match &game_data_guard {
                    Some(data) => match data.resolve_npc(npc_name) {
                        Some(info) => (info.key.clone(), info.name.clone()),
                        None => (npc_name.clone(), npc_name.clone()),
                    },
                    None => (npc_name.clone(), npc_name.clone()),
                };

                self.current_vendor_npc = Some(npc_key.clone());

                conn.execute(
                    "INSERT INTO game_state_npc_vendor (character_name, server_name, npc_key, last_interaction_at, last_confirmed_at)
                     VALUES (?1, ?2, ?3, ?4, ?4)
                     ON CONFLICT(character_name, server_name, npc_key) DO UPDATE SET
                        last_interaction_at = excluded.last_interaction_at,
                        last_confirmed_at = excluded.last_confirmed_at",
                    rusqlite::params![character, server, npc_key, dt],
                ).ok();
                domains.push("vendor");
            }

            PlayerEvent::InteractionEnded { .. } => {
                self.current_vendor_npc = None;
            }

            PlayerEvent::VendorSold {
                timestamp,
                price,
                ..
            } => {
                if let Some(npc_key) = self.current_vendor_npc.clone() {
                    let dt = self.to_utc(timestamp);
                    let councils = *price as i64;
                    conn.execute(
                        "INSERT INTO game_state_npc_vendor (character_name, server_name, npc_key, last_sell_at, last_confirmed_at, councils_earned_current, councils_earned_lifetime)
                         VALUES (?1, ?2, ?3, ?4, ?4, ?5, ?5)
                         ON CONFLICT(character_name, server_name, npc_key) DO UPDATE SET
                            last_sell_at = excluded.last_sell_at,
                            last_confirmed_at = excluded.last_confirmed_at,
                            councils_earned_current = councils_earned_current + ?5,
                            councils_earned_lifetime = councils_earned_lifetime + ?5",
                        rusqlite::params![character, server, npc_key, dt, councils],
                    ).ok();
                    domains.push("vendor");
                } else {
                    eprintln!("[game_state] VendorSold with no current interaction NPC — skipping");
                }
            }

            PlayerEvent::VendorGoldChanged {
                timestamp,
                current_gold,
                max_gold,
                ..
            } => {
                if let Some(npc_key) = self.current_vendor_npc.clone() {
                    let dt = self.to_utc(timestamp);
                    if *current_gold < *max_gold {
                        conn.execute(
                            "INSERT INTO game_state_npc_vendor (character_name, server_name, npc_key, vendor_gold_available, vendor_gold_max, vendor_gold_timer_start, last_confirmed_at)
                             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)
                             ON CONFLICT(character_name, server_name, npc_key) DO UPDATE SET
                                vendor_gold_available = excluded.vendor_gold_available,
                                vendor_gold_max = excluded.vendor_gold_max,
                                vendor_gold_timer_start = CASE
                                    WHEN game_state_npc_vendor.vendor_gold_timer_start IS NULL THEN excluded.vendor_gold_timer_start
                                    WHEN datetime(game_state_npc_vendor.vendor_gold_timer_start, '+168 hours') < excluded.last_confirmed_at THEN excluded.vendor_gold_timer_start
                                    ELSE game_state_npc_vendor.vendor_gold_timer_start
                                END,
                                councils_earned_current = CASE
                                    WHEN game_state_npc_vendor.vendor_gold_timer_start IS NOT NULL
                                         AND datetime(game_state_npc_vendor.vendor_gold_timer_start, '+168 hours') < excluded.last_confirmed_at THEN 0
                                    ELSE councils_earned_current
                                END,
                                last_confirmed_at = excluded.last_confirmed_at",
                            rusqlite::params![character, server, npc_key, *current_gold as i64, *max_gold as i64, dt],
                        ).ok();
                    } else {
                        conn.execute(
                            "INSERT INTO game_state_npc_vendor (character_name, server_name, npc_key, vendor_gold_available, vendor_gold_max, vendor_gold_timer_start, last_confirmed_at)
                             VALUES (?1, ?2, ?3, ?4, ?5, NULL, ?6)
                             ON CONFLICT(character_name, server_name, npc_key) DO UPDATE SET
                                vendor_gold_available = excluded.vendor_gold_available,
                                vendor_gold_max = excluded.vendor_gold_max,
                                vendor_gold_timer_start = NULL,
                                councils_earned_current = CASE
                                    WHEN game_state_npc_vendor.vendor_gold_timer_start IS NOT NULL THEN 0
                                    ELSE councils_earned_current
                                END,
                                last_confirmed_at = excluded.last_confirmed_at",
                            rusqlite::params![character, server, npc_key, *current_gold as i64, *max_gold as i64, dt],
                        ).ok();
                    }
                    domains.push("vendor");
                } else {
                    eprintln!("[game_state] VendorGoldChanged with no current interaction NPC — skipping");
                }
            }

            // ── New game state events ──────────────────────────────────

            PlayerEvent::MoonPhaseChanged { timestamp, phase } => {
                let dt = self.to_utc(timestamp);
                conn.execute(
                    "INSERT INTO game_state_moon (id, phase, last_confirmed_at)
                     VALUES (1, ?1, ?2)
                     ON CONFLICT(id) DO UPDATE SET
                        phase = excluded.phase,
                        last_confirmed_at = excluded.last_confirmed_at",
                    rusqlite::params![phase, dt],
                )
                .ok();
                domains.push("moon");
            }

            PlayerEvent::GuildInfoLoaded {
                timestamp,
                guild_id,
                guild_name,
                motd,
            } => {
                let dt = self.to_utc(timestamp);
                conn.execute(
                    "INSERT INTO game_state_guild (character_name, server_name, guild_id, guild_name, motd, last_confirmed_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                     ON CONFLICT(character_name, server_name) DO UPDATE SET
                        guild_id = excluded.guild_id,
                        guild_name = excluded.guild_name,
                        motd = excluded.motd,
                        last_confirmed_at = excluded.last_confirmed_at",
                    rusqlite::params![character, server, *guild_id as i64, guild_name, motd, dt],
                )
                .ok();
                domains.push("guild");
            }

            PlayerEvent::DirectedGoalsLoaded {
                timestamp,
                goal_ids,
            } => {
                let dt = self.to_utc(timestamp);
                // Full replacement on login — same pattern as SkillsLoaded
                conn.execute(
                    "DELETE FROM game_state_directed_goals WHERE character_name = ?1 AND server_name = ?2",
                    rusqlite::params![character, server],
                )
                .ok();
                if let Ok(mut stmt) = conn.prepare(
                    "INSERT INTO game_state_directed_goals (character_name, server_name, goal_id, last_confirmed_at)
                     VALUES (?1, ?2, ?3, ?4)",
                ) {
                    for goal_id in goal_ids {
                        stmt.execute(rusqlite::params![
                            character,
                            server,
                            *goal_id as i64,
                            dt
                        ])
                        .ok();
                    }
                }
                domains.push("directed_goals");
            }

            PlayerEvent::PlayerStringUpdated {
                timestamp,
                key,
                value,
            } => {
                let dt = self.to_utc(timestamp);
                conn.execute(
                    "INSERT INTO game_state_strings (character_name, server_name, key, value, last_confirmed_at)
                     VALUES (?1, ?2, ?3, ?4, ?5)
                     ON CONFLICT(character_name, server_name, key) DO UPDATE SET
                        value = excluded.value,
                        last_confirmed_at = excluded.last_confirmed_at",
                    rusqlite::params![character, server, key, value, dt],
                )
                .ok();
                domains.push("strings");
            }

            PlayerEvent::PlayerMessage {
                timestamp,
                message_type,
                direction,
                other_player,
                body,
                item_name,
            } => {
                let dt = self.to_utc(timestamp);
                conn.execute(
                    "INSERT INTO player_messages (character_name, server_name, timestamp, message_type, direction, other_player, body, item_name)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                    rusqlite::params![character, server, dt, message_type, direction, other_player, body, item_name],
                )
                .ok();
                domains.push("player_messages");
            }

            PlayerEvent::PoemRecorded {
                timestamp,
                author,
                title,
                content,
            } => {
                let dt = self.to_utc(timestamp);
                // Poems are global (not character-scoped). The character/server
                // that recorded it are kept for reference only. INSERT OR IGNORE
                // + the UNIQUE(author, title, content) index dedups the same poem
                // heard multiple times or across characters.
                conn.execute(
                    "INSERT OR IGNORE INTO poems (author, title, content, recorded_at, recorded_by_character, recorded_by_server)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                    rusqlite::params![author, title, content, dt, character, server],
                )
                .ok();
                domains.push("poems");
            }

            _ => {}
        }
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
        if quantity <= 1 {
            return vec![];
        }

        let (character, server) = match (&self.active_character, &self.active_server) {
            (Some(c), Some(s)) => (c.as_str(), s.as_str()),
            _ => return vec![],
        };

        let conn = match db.get_write() {
            Ok(c) => c,
            Err(_) => return vec![],
        };

        // Resolve display name to internal name via CDN data
        let internal_name = self
            .game_data
            .try_read()
            .ok()
            .and_then(|gd| gd.resolve_item(display_name).and_then(|info| info.internal_name.clone()));

        let mut domains = Vec::new();

        // Try to correct game_state_inventory — match by internal_name or display_name
        let inv_corrected = correct_stack_in_table(
            &conn,
            "game_state_inventory",
            "character_name = ?1 AND server_name = ?2",
            character,
            server,
            None, // no vault_key filter
            internal_name.as_deref(),
            display_name,
            quantity,
        );
        if inv_corrected {
            domains.push("inventory");
        }

        // Try to correct game_state_storage — same logic, any vault
        let storage_corrected = correct_stack_in_table(
            &conn,
            "game_state_storage",
            "character_name = ?1 AND server_name = ?2",
            character,
            server,
            None,
            internal_name.as_deref(),
            display_name,
            quantity,
        );
        if storage_corrected {
            domains.push("storage");
        }

        domains
    }

    /// Record an item transaction in the ledger.
    ///
    /// `provenance_columns` is `None` for transactions that don't have
    /// ItemProvenance attached (e.g., chat-status-sourced rows inserted
    /// directly by the coordinator without parser context). Callers with a
    /// PlayerEvent in hand should pass `Some(event_provenance.to_columns())`
    /// so downstream aggregates can filter and group by source.
    #[allow(clippy::too_many_arguments)]
    fn record_transaction(
        conn: &Connection,
        character: &str,
        server: &str,
        dt: &str,
        item_name: &str,
        internal_name: Option<&str>,
        item_type_id: Option<i64>,
        quantity: i32,
        context: &str,
        source: &str,
        instance_id: Option<u64>,
        vault_key: Option<&str>,
        provenance_columns: Option<crate::player_event_parser::ProvenanceColumns>,
    ) {
        let (source_kind, source_details, confidence) = match provenance_columns {
            Some(p) => (p.source_kind, p.source_details, p.confidence),
            None => (None, None, None),
        };
        conn.execute(
            "INSERT INTO item_transactions (timestamp, character_name, server_name, item_name, internal_name, item_type_id, quantity, context, source, instance_id, vault_key, source_kind, source_details, confidence)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
            rusqlite::params![
                dt,
                character,
                server,
                item_name,
                internal_name,
                item_type_id,
                quantity,
                context,
                source,
                instance_id.map(|id| id as i64),
                vault_key,
                source_kind,
                source_details,
                confidence,
            ],
        )
        .ok();
    }
}

/// Correct a single stack_size=1 row in an inventory/storage table.
/// Returns true if a row was updated.
fn correct_stack_in_table(
    conn: &Connection,
    table: &str,
    base_where: &str,
    character: &str,
    server: &str,
    _vault_key: Option<&str>,
    internal_name: Option<&str>,
    display_name: &str,
    quantity: u32,
) -> bool {
    // Build the item name match: prefer internal_name, fall back to display_name
    let name_to_match = internal_name.unwrap_or(display_name);

    // Find the most recent row with stack_size=1 for this item
    // (rowid ordering gives us recency since rows are inserted in order)
    let query = format!(
        "UPDATE {table} SET stack_size = ?1
         WHERE rowid = (
             SELECT rowid FROM {table}
             WHERE {base_where} AND item_name = ?3 AND stack_size = 1
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
                         WHERE {base_where} AND item_name = ?3 AND stack_size = 1
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

    fn week_start(instant: &str) -> String {
        week_start_utc_at(
            chrono::DateTime::parse_from_rfc3339(instant)
                .unwrap()
                .with_timezone(&Utc),
        )
    }

    /// The weekly gift boundary is Monday 00:00 America/New_York (the game's
    /// own note: "The week resets on Monday EDT."), i.e. 04:00 UTC in DST and
    /// 05:00 UTC in EST — never Monday 00:00 UTC.
    #[test]
    fn week_boundary_is_monday_midnight_eastern() {
        // Thu 01:00 UTC = Wed 21:00 EDT → week started Mon 10-12.
        assert_eq!(week_start("2026-10-15T01:00:00Z"), "2026-10-12 04:00:00");
        // Mon 01:00 UTC is still Sunday 21:00 EDT — the game's week has NOT
        // rolled; a Monday-UTC boundary would have jumped ahead here.
        assert_eq!(week_start("2026-10-19T01:00:00Z"), "2026-10-12 04:00:00");
        // Mon 01:00 EDT → the new week.
        assert_eq!(week_start("2026-10-19T05:00:00Z"), "2026-10-19 04:00:00");
        // EST (after the first Sunday in November): 05:00 UTC.
        assert_eq!(week_start("2026-11-02T06:00:00Z"), "2026-11-02 05:00:00");
        assert_eq!(week_start("2026-11-02T04:30:00Z"), "2026-10-26 04:00:00");
        // EDT resumes the second Sunday in March.
        assert_eq!(week_start("2026-03-09T04:30:00Z"), "2026-03-09 04:00:00");
        assert_eq!(week_start("2026-03-09T03:30:00Z"), "2026-03-02 05:00:00");
    }

    /// Two DST rules, one boundary: assert the offset itself at the edges.
    #[test]
    fn eastern_offset_switches_on_the_us_dst_dates() {
        let d = |s: &str| chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap();
        assert_eq!(eastern_utc_offset_hours(d("2026-03-07")), 5); // Sat before
        assert_eq!(eastern_utc_offset_hours(d("2026-03-08")), 4); // 2nd Sunday
        assert_eq!(eastern_utc_offset_hours(d("2026-10-31")), 4);
        assert_eq!(eastern_utc_offset_hours(d("2026-11-01")), 5); // 1st Sunday
        assert_eq!(eastern_utc_offset_hours(d("2026-11-02")), 5);
    }

    /// A gift-cap note the game printed before this week began reports the
    /// previous week's count: it must not be reconciled at all — otherwise it
    /// writes rows into last week, or trims this week's real gifts down to last
    /// week's number. (The backfill drops these; this pins the live arm to the
    /// same rule.)
    #[test]
    fn gift_note_from_a_previous_week_is_not_reconciled() {
        use std::sync::Arc;

        let db_path = std::env::temp_dir().join(format!(
            "glogger_gift_week_test_{}.db",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let pool = crate::db::init_pool(db_path.clone(), Some(0)).expect("init pool");

        let week_start = current_week_start_utc();
        let week_start_date =
            chrono::NaiveDate::parse_from_str(&week_start[..10], "%Y-%m-%d").unwrap();
        let day_of = |offset_days: i64, hhmmss: &str| {
            format!(
                "{} {hhmmss}",
                (week_start_date + Duration::days(offset_days)).format("%Y-%m-%d")
            )
        };

        // Five real gifts already recorded this week. The arm resolves NPC
        // names against game data (empty here), so it falls back to the raw
        // name — seed rows under the same key it will look for.
        {
            let conn = pool.get_write().unwrap();
            for i in 0..5 {
                conn.execute(
                    "INSERT INTO game_state_gift_log (character_name, server_name, npc_key, npc_name, gifted_at, favor_delta)
                     VALUES ('TestPlayer', 'Dreva', 'Corinth', 'Corinth', ?1, 0.0)",
                    rusqlite::params![day_of(0, &format!("1{i}:00:00"))],
                )
                .unwrap();
            }
        }

        let mut manager = GameStateManager::new(Arc::new(tauri::async_runtime::RwLock::new(
            crate::game_data::GameData::empty(),
        )));
        manager.set_active_character_name("TestPlayer");
        manager.set_active_server_name("Dreva");

        let count_this_week = || -> i64 {
            pool.get()
                .unwrap()
                .query_row(
                    "SELECT COUNT(*) FROM game_state_gift_log WHERE gifted_at >= ?1",
                    [&week_start],
                    |row| row.get(0),
                )
                .unwrap()
        };

        // Last week's final note (the count was 2 that week).
        manager.set_base_date(week_start_date - Duration::days(1));
        let stale = vec![PlayerEvent::GiftCountObserved {
            timestamp: "23:30:00".to_string(),
            npc_name: "Corinth".to_string(),
            received: 2,
            max: 5,
        }];
        let result = manager.process_events_batch(&stale, &pool);
        assert!(
            !result.domains_updated.contains(&"favor"),
            "an out-of-week note must not report a gift change"
        );
        assert_eq!(count_this_week(), 5, "this week's gifts survive untouched");
        let rows_last_week: i64 = pool
            .get()
            .unwrap()
            .query_row(
                "SELECT COUNT(*) FROM game_state_gift_log WHERE gifted_at < ?1",
                [&week_start],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(rows_last_week, 0, "nothing is written into last week either");

        // The same note *inside* this week still reconciles (5 → 2).
        manager.set_base_date(week_start_date);
        let in_week = vec![PlayerEvent::GiftCountObserved {
            timestamp: "12:00:00".to_string(),
            npc_name: "Corinth".to_string(),
            received: 2,
            max: 5,
        }];
        let result = manager.process_events_batch(&in_week, &pool);
        assert!(
            result.domains_updated.contains(&"favor"),
            "an in-week note still reconciles"
        );
        assert_eq!(count_this_week(), 2, "trimmed to the game's count");

        let _ = std::fs::remove_file(&db_path);
    }

    /// The manager dates lines from the log's own login line (via the
    /// `LogSessionAnchored` event) and rolls the date when the log crosses UTC
    /// midnight — no dependence on the ingest clock, so a catch-up lagging the
    /// log by any amount is dated by the session it came from.
    #[test]
    fn manager_dates_lines_from_the_login_anchor() {
        use std::sync::Arc;

        let mut manager = GameStateManager::new(Arc::new(tauri::async_runtime::RwLock::new(
            crate::game_data::GameData::empty(),
        )));

        // The anchor event, as the parser emits it from the real login line.
        manager.anchor_log_session(
            chrono::NaiveDateTime::parse_from_str("2026-10-14 22:43:41", "%Y-%m-%d %H:%M:%S")
                .unwrap(),
        );
        assert_eq!(manager.to_utc("22:43:41"), "2026-10-14 22:43:41");
        assert_eq!(manager.to_utc("23:31:15"), "2026-10-14 23:31:15");
        // Past UTC midnight in the log → next day, still not "now".
        assert_eq!(manager.to_utc("00:10:00"), "2026-10-15 00:10:00");

        // A side channel peeks a later line (the coordinator's milking resolve
        // runs before the event's batch is replayed); the in-order scan must
        // still date the batch's earlier lines from the session. Before the peek
        // seam this rolled the session date a day forward.
        assert_eq!(manager.peek_utc("00:40:00"), "2026-10-15 00:40:00");
        assert_eq!(manager.to_utc("00:20:00"), "2026-10-15 00:20:00");

        // A replay/reparse base date still wins over the anchor.
        manager.set_base_date(chrono::NaiveDate::from_ymd_opt(2026, 10, 1).unwrap());
        assert_eq!(manager.to_utc("23:31:15"), "2026-10-01 23:31:15");
    }
}
