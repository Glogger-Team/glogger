use rusqlite::{Connection, Result};

/// Run all database migrations in order.
///
/// Each migration is applied exactly once and tracked in `schema_migrations`.
/// The v1 migration is the baseline schema — all tables as of the initial release.
/// New schema changes MUST be added as new numbered migrations (v2, v3, …) below.
/// Never modify migration_v1 after it has shipped — existing user databases already
/// have that schema applied and won't re-run it.
///
/// Example of adding a new migration:
/// ```text
/// if current_version < 2 {
///     migration_v2_add_foo(conn)?;
///     super::record_migration(conn, 2)?;
/// }
/// ```
pub fn run_migrations(conn: &Connection, tz_offset_seconds: Option<i32>) -> Result<()> {
    // Create migrations table if it doesn't exist
    conn.execute(
        "CREATE TABLE IF NOT EXISTS schema_migrations (
            version INTEGER PRIMARY KEY,
            applied_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
        )",
        [],
    )?;

    let current_version = super::get_schema_version(conn)?;

    // Apply every pending migration inside one transaction so a mid-run
    // failure rolls the whole run back and leaves `schema_migrations` at the
    // last committed version (a non-idempotent ALTER/CREATE mid-way would
    // otherwise leave a half-applied schema). `&tx` coerces to `&Connection`
    // at every `migration_vN(..)` / `record_migration(..)` call site.
    let tx = conn.unchecked_transaction()?;
    let conn = &tx;

    if current_version < 1 {
        migration_v1_unified_schema(conn)?;
        super::record_migration(conn, 1)?;
    }

    if current_version < 2 {
        migration_v2_skill_base_level(conn)?;
        super::record_migration(conn, 2)?;
    }

    if current_version < 3 {
        migration_v3_fix_skill_levels(conn)?;
        super::record_migration(conn, 3)?;
    }

    if current_version < 4 {
        migration_v4_fix_crafting_project_cascade(conn)?;
        super::record_migration(conn, 4)?;
    }

    if current_version < 5 {
        migration_v5_build_planner(conn)?;
        super::record_migration(conn, 5)?;
    }

    if current_version < 6 {
        migration_v6_build_preset_slot_items(conn)?;
        super::record_migration(conn, 6)?;
    }

    if current_version < 7 {
        migration_v7_build_preset_abilities(conn)?;
        super::record_migration(conn, 7)?;
    }

    if current_version < 8 {
        migration_v8_slot_item_level_rarity(conn)?;
        super::record_migration(conn, 8)?;
    }

    if current_version < 9 {
        migration_v9_slot_per_slot_skills(conn)?;
        super::record_migration(conn, 9)?;
    }

    if current_version < 10 {
        migration_v10_crafting_groups_and_stock_targets(conn)?;
        super::record_migration(conn, 10)?;
    }

    if current_version < 11 {
        migration_v11_character_deaths(conn)?;
        super::record_migration(conn, 11)?;
    }

    if current_version < 12 {
        migration_v12_game_state_area(conn)?;
        super::record_migration(conn, 12)?;
    }

    if current_version < 13 {
        migration_v13_death_damage_type(conn)?;
        super::record_migration(conn, 13)?;
    }

    if current_version < 14 {
        migration_v14_death_damage_sources(conn)?;
        super::record_migration(conn, 14)?;
    }

    if current_version < 15 {
        migration_v15_item_transactions(conn)?;
        super::record_migration(conn, 15)?;
    }

    if current_version < 16 {
        migration_v16_survey_data_imports(conn)?;
        super::record_migration(conn, 16)?;
    }

    if current_version < 17 {
        migration_v17_gift_log(conn)?;
        super::record_migration(conn, 17)?;
    }

    if current_version < 18 {
        let applied = migration_v18_fix_timestamps(conn, tz_offset_seconds)?;
        if applied {
            super::record_migration(conn, 18)?;
        }
    }

    if current_version < 19 {
        migration_v19_price_helper(conn)?;
        super::record_migration(conn, 19)?;
    }

    if current_version < 20 {
        migration_v20_project_pricing(conn)?;
        super::record_migration(conn, 20)?;
    }

    if current_version < 21 {
        migration_v21_build_preset_cp_recipes(conn)?;
        super::record_migration(conn, 21)?;
    }

    if current_version < 22 {
        migration_v22_resuscitations(conn)?;
        super::record_migration(conn, 22)?;
    }

    if current_version < 23 {
        migration_v23_stall_events(conn)?;
        super::record_migration(conn, 23)?;
    }

    if current_version < 24 {
        migration_v24_npc_vendor(conn)?;
        super::record_migration(conn, 24)?;
    }

    if current_version < 25 {
        migration_v25_item_transaction_provenance(conn)?;
        super::record_migration(conn, 25)?;
    }

    if current_version < 26 {
        migration_v26_survey_tracker_rewrite(conn)?;
        super::record_migration(conn, 26)?;
    }

    if current_version < 27 {
        migration_v27_drop_legacy_survey_tables(conn)?;
        super::record_migration(conn, 27)?;
    }

    if current_version < 28 {
        migration_v28_backfill_survey_use_areas(conn)?;
        super::record_migration(conn, 28)?;
    }

    if current_version < 29 {
        migration_v29_session_name_and_timestamps(conn)?;
        super::record_migration(conn, 29)?;
    }

    if current_version < 30 {
        migration_v30_normalize_survey_use_areas(conn)?;
        super::record_migration(conn, 30)?;
    }

    if current_version < 31 {
        migration_v31_abilities_internal_name(conn)?;
        super::record_migration(conn, 31)?;
    }

    if current_version < 32 {
        migration_v32_brewing_discoveries(conn)?;
        super::record_migration(conn, 32)?;
    }

    if current_version < 33 {
        migration_v33_new_game_state_tables(conn)?;
        super::record_migration(conn, 33)?;
    }

    if current_version < 34 {
        migration_v34_milking_and_stats(conn)?;
        super::record_migration(conn, 34)?;
    }

    if current_version < 35 {
        migration_v35_attribute_extremes(conn)?;
        super::record_migration(conn, 35)?;
    }

    if current_version < 36 {
        migration_v36_player_milking_log(conn)?;
        super::record_migration(conn, 36)?;
    }

    if current_version < 37 {
        migration_v37_words_of_power(conn)?;
        super::record_migration(conn, 37)?;
    }

    if current_version < 38 {
        migration_v38_self_milking(conn)?;
        super::record_migration(conn, 38)?;
    }

    if current_version < 39 {
        migration_v39_teleportation_binds(conn)?;
        super::record_migration(conn, 39)?;
    }

    if current_version < 40 {
        migration_v40_user_timers(conn)?;
        super::record_migration(conn, 40)?;
    }

    if current_version < 41 {
        migration_v41_garden_almanac(conn)?;
        super::record_migration(conn, 41)?;
    }

    if current_version < 42 {
        migration_v42_player_messages(conn)?;
        super::record_migration(conn, 42)?;
    }

    if current_version < 43 {
        migration_v43_hoplology_studies(conn)?;
        super::record_migration(conn, 43)?;
    }

    if current_version < 44 {
        migration_v44_hoplology_dedup(conn)?;
        super::record_migration(conn, 44)?;
    }

    if current_version < 45 {
        migration_v45_kill_tracking(conn)?;
        super::record_migration(conn, 45)?;
    }

    if current_version < 46 {
        migration_v46_gourmand_manually_marked(conn)?;
        super::record_migration(conn, 46)?;
    }

    if current_version < 47 {
        migration_v47_imported_kill_loot_data(conn)?;
        super::record_migration(conn, 47)?;
    }

    if current_version < 48 {
        migration_v48_kill_dedup_and_ingest_tracking(conn)?;
        super::record_migration(conn, 48)?;
    }

    if current_version < 49 {
        migration_v49_corpse_search_loot_model(conn)?;
        super::record_migration(conn, 49)?;
    }

    if current_version < 50 {
        migration_v50_corpse_extracts(conn)?;
        super::record_migration(conn, 50)?;
    }

    if current_version < 51 {
        migration_v51_combat_wisdom_earns(conn)?;
        super::record_migration(conn, 51)?;
    }

    if current_version < 52 {
        migration_v52_vendor_earnings(conn)?;
        super::record_migration(conn, 52)?;
    }

    if current_version < 53 {
        migration_v53_persist_imported_drop_data(conn)?;
        super::record_migration(conn, 53)?;
    }

    if current_version < 54 {
        migration_v54_zone_aware_drops(conn)?;
        super::record_migration(conn, 54)?;
    }

    if current_version < 55 {
        migration_v55_loadout_aware_drops(conn)?;
        super::record_migration(conn, 55)?;
    }

    if current_version < 56 {
        migration_v56_currency_estimate(conn)?;
        super::record_migration(conn, 56)?;
    }

    if current_version < 57 {
        migration_v57_fix_chat_fts_delete_triggers(conn)?;
        super::record_migration(conn, 57)?;
    }

    if current_version < 58 {
        migration_v58_roulette_results(conn)?;
        super::record_migration(conn, 58)?;
    }

    if current_version < 59 {
        migration_v59_project_entry_slot_pins(conn)?;
        super::record_migration(conn, 59)?;
    }

    if current_version < 60 {
        migration_v60_survey_use_id_index(conn)?;
        super::record_migration(conn, 60)?;
    }

    if current_version < 61 {
        migration_v61_poems(conn)?;
        super::record_migration(conn, 61)?;
    }

    if current_version < 62 {
        migration_v62_purge_misattributed_xogrite(conn)?;
        super::record_migration(conn, 62)?;
    }

    if current_version < 63 {
        migration_v63_milking_timers_per_cow(conn)?;
        super::record_migration(conn, 63)?;
    }

    if current_version < 64 {
        migration_v64_arena_matches(conn)?;
        super::record_migration(conn, 64)?;
    }

    if current_version < 65 {
        migration_v65_arena_bets(conn)?;
        super::record_migration(conn, 65)?;
    }

    if current_version < 66 {
        migration_v66_harvest_almanac(conn)?;
        super::record_migration(conn, 66)?;
    }

    if current_version < 67 {
        migration_v67_food_sources(conn)?;
        super::record_migration(conn, 67)?;
    }

    if current_version < 68 {
        migration_v68_stall_price_observations(conn)?;
        super::record_migration(conn, 68)?;
    }

    if current_version < 69 {
        migration_v69_chat_channel_timestamp_index(conn)?;
        super::record_migration(conn, 69)?;
    }

    if current_version < 70 {
        migration_v70_canonical_food_categories(conn)?;
        super::record_migration(conn, 70)?;
    }

    if current_version < 71 {
        migration_v71_player_pooping_log(conn)?;
        super::record_migration(conn, 71)?;
    }

    if current_version < 72 {
        migration_v72_milking_dedup(conn)?;
        super::record_migration(conn, 72)?;
    }

    if current_version < 73 {
        migration_v73_milking_cooldown_unknown(conn)?;
        super::record_migration(conn, 73)?;
    }

    if current_version < 74 {
        migration_v74_snapshot_skill_abilities(conn)?;
        super::record_migration(conn, 74)?;
    }

    if current_version < 75 {
        migration_v75_achievements(conn)?;
        super::record_migration(conn, 75)?;
    }

    if current_version < 76 {
        migration_v76_npc_favor_details(conn)?;
        super::record_migration(conn, 76)?;
    }

    tx.commit()?;
    Ok(())
}

/// Migration V73: let a cow cooldown admit that its milk time is unknown.
///
/// The game's "You've already milked X in the past hour." error only bounds the
/// milk: it happened somewhere in `(T-60m, T]`, so a timer that claims a point
/// either lies optimistically or pessimistically. With `cooldown_unknown = 1`,
/// `last_milked_at` is documented as the *earliest observed failed attempt* —
/// giving a provable upper bound on the remaining cooldown
/// (`≤ attempt + 60m - now`) that the widget renders as "≤ 45m" instead of a
/// fabricated countdown. A successful milk clears the flag and stores the true
/// time. Pre-existing rows default to 0 (they hold the old 59-minute guess,
/// which is indistinguishable from a real stamp — they age out on the next milk).
fn migration_v73_milking_cooldown_unknown(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "ALTER TABLE milking_timers ADD COLUMN cooldown_unknown INTEGER NOT NULL DEFAULT 0;",
    )?;
    Ok(())
}

/// Migration V74: persist known abilities per skill per character snapshot.
///
/// `SkillData.abilities` was parsed from `/outputcharacter` exports but dropped
/// (never stored). Every `character_snapshots.raw_json` already contains the
/// arrays, so new imports populate this table; old snapshots simply have no
/// rows (no backfill — re-import the export to populate).
fn migration_v74_snapshot_skill_abilities(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "CREATE TABLE snapshot_skill_abilities (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            snapshot_id INTEGER NOT NULL,
            skill_name TEXT NOT NULL,
            ability_key TEXT NOT NULL,
            FOREIGN KEY (snapshot_id) REFERENCES character_snapshots(id) ON DELETE CASCADE
        );
        CREATE INDEX idx_skill_abilities_snapshot ON snapshot_skill_abilities(snapshot_id);
        CREATE INDEX idx_skill_abilities_ability ON snapshot_skill_abilities(ability_key, snapshot_id);",
    )?;
    Ok(())
}

/// Migration V75: CDN achievement reference table (read-only catalog).
///
/// `achievements.json` is a small array (11 entries at v487) of `{id, name,
/// description, iconType}` keyed by string `id` (e.g. `Combat_Any_25`). No
/// per-character unlock tracking: the game is the tracker, first-round
/// achievements are retroactive on login, and no log signal is known — so a
/// flat catalog is all we persist. Standalone table (no snapshot FK): it is
/// global CDN reference data like `quests`/`areas`, not per-snapshot rows.
fn migration_v75_achievements(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "CREATE TABLE achievements (
            id TEXT PRIMARY KEY,
            name TEXT,
            description TEXT,
            icon_type TEXT
        );
        CREATE INDEX idx_achievements_name ON achievements(name);",
    )?;
    Ok(())
}

/// Migration V76: new VIP-report NPC columns from the 10/21/2026 update.
///
/// VIP exports gained raw favor points, councils spent at the vendor, vendor
/// reset time, and GUI player notes per NPC. All nullable: older exports omit
/// them, and the importer passes NULLs through (serde `default` on `Option`
/// fields). No backfill — re-import the export to populate.
fn migration_v76_npc_favor_details(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "ALTER TABLE character_npc_favor ADD COLUMN favor REAL;
         ALTER TABLE character_npc_favor ADD COLUMN councils_spent_at_vendor INTEGER;
         ALTER TABLE character_npc_favor ADD COLUMN vendor_reset_time TEXT;
         ALTER TABLE character_npc_favor ADD COLUMN player_notes TEXT;",
    )?;
    Ok(())
}

/// Migration V72: one row per milking *event*, not per ingestion.
///
/// `player_milking_log` rows are now stamped with the event's own time and both
/// the live path and the Chat.log backfill can see the same line (a catch-up
/// replay re-reads, and a backfill rescans archives). Without a key those become
/// duplicate rows and the widget's counter double-counts, so add the same
/// `INSERT OR IGNORE` identity the pooping log uses. Existing exact duplicates
/// (same character/server/direction/other/datetime) collapse to the lowest id.
fn migration_v72_milking_dedup(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "DELETE FROM player_milking_log
          WHERE id NOT IN (
                SELECT MIN(id) FROM player_milking_log
                 GROUP BY character_name, server_name, direction, other_player, milked_at
          );
         CREATE UNIQUE INDEX IF NOT EXISTS idx_player_milking_event
             ON player_milking_log(character_name, server_name, direction, other_player, milked_at);",
    )?;
    Ok(())
}

/// Migration V71: Player pooping log — one row per `[Combat] Name: Poop on
/// Target!` line (the cow-curse "Poop" ability; the Dungcrafting skill's
/// xp_table is literally `Pooping`). Powers the lifetime count on the
/// Character → Stats card.
///
/// `pooped_at` holds the chat log's **local wall-clock** timestamp, not UTC:
/// the live tail and the chat-log backfill must agree on the dedup key, and the
/// chat backfill never converts to UTC (same convention as the Combat Wisdom
/// store). The UNIQUE key makes `INSERT OR IGNORE` idempotent, so re-running
/// the backfill re-counts nothing.
fn migration_v71_player_pooping_log(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS player_pooping_log (
             id INTEGER PRIMARY KEY AUTOINCREMENT,
             character_name TEXT NOT NULL,
             server_name TEXT NOT NULL,
             target_name TEXT NOT NULL,
             pooped_at TEXT NOT NULL,
             UNIQUE(character_name, server_name, pooped_at, target_name)
         );
         CREATE INDEX IF NOT EXISTS idx_player_pooping_char
             ON player_pooping_log(character_name, server_name, pooped_at);",
    )?;
    Ok(())
}

/// Migration V69: composite `(channel, timestamp)` index for the "messages
/// around a target" query, which filters one channel by a timestamp range and
/// orders by timestamp. The single-column `idx_chat_messages_channel` forced a
/// temp-B-tree sort of the entire channel per neighbour scan; this index turns
/// each scan into an ordered range seek (near-instant even for 50k-row channels).
fn migration_v69_chat_channel_timestamp_index(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "CREATE INDEX IF NOT EXISTS idx_chat_messages_channel_timestamp
            ON chat_messages(channel, timestamp);",
    )?;
    Ok(())
}

/// Migration V70: canonicalise the food category tokens already on disk.
///
/// CDN 486 spells instant snacks `"Instant Snack"` (built from the
/// `Food_Effect_Level_InstantSnack` string) where earlier CDN data used
/// `"Instant-Snack"`. `insert_foods` canonicalises the token itself now, but the
/// `foods` table is only rebuilt on a CDN persist, so rewrite the stored rows —
/// `gourmandStore` matches the hyphenated spelling and silently drops every row
/// that carries a space (57 foods, including the whole Instant-Snacks section).
///
/// `cdn_version` is deliberately left alone: the backfill repairs the existing
/// data in place, and the next persist writes canonical tokens anyway.
fn migration_v70_canonical_food_categories(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "UPDATE foods
            SET food_category = 'Instant-Snack'
          WHERE food_category <> 'Instant-Snack'
            AND REPLACE(REPLACE(REPLACE(LOWER(food_category), '-', ''), ' ', ''), '_', '') = 'instantsnack';",
    )?;
    Ok(())
}

/// Migration V67: where each food comes from (Gourmand tracker source filter).
///
/// `source_kinds` and `craft_skills` are JSON arrays derived during CDN
/// ingestion by `game_data::food_sources` — see `insert_foods`. Both are
/// populated only by a CDN persist, and `persist_cdn_data` is skipped on
/// startup when the stored `cdn_version` already matches the loaded data, so
/// clear `cdn_version` here to force one rebuild. Existing rows keep the `[]`
/// default until that runs; if the CDN fails to load, the old rows stay in
/// place exactly as before.
fn migration_v67_food_sources(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "ALTER TABLE foods ADD COLUMN source_kinds TEXT NOT NULL DEFAULT '[]';
         ALTER TABLE foods ADD COLUMN craft_skills TEXT NOT NULL DEFAULT '[]';
         DELETE FROM cdn_version;",
    )?;
    Ok(())
}

/// Migration V66: the Emotion-Harvesting Almanac (the `AlmanacPriestHarvesting`
/// NPC in Statehelm) — which monsters are the day's harvesting foci. Mirrors
/// `garden_almanac`: the whole snapshot is replaced on each read, so no unique
/// index is needed. The almanac names a monster but no zone, so `zone_name` is
/// deliberately absent — the widget resolves zones from the kill database.
fn migration_v66_harvest_almanac(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS harvest_almanac (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            character_name TEXT NOT NULL,
            server_name TEXT NOT NULL,
            monster_name TEXT NOT NULL,
            description TEXT,
            event_start TEXT,
            event_end TEXT,
            is_current INTEGER NOT NULL DEFAULT 0,
            captured_at TEXT NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_harvest_almanac_char_server
            ON harvest_almanac(character_name, server_name);",
    )?;
    Ok(())
}

/// Migration V65: the player's own casino arena bets (drives the Arena widget's
/// "Betting Rate" + P&L). Parsed from `Player.log` NPC 5712 talk screens; each
/// resolved bet records the pick, opponent, wager, payout, and whether it won.
/// Idempotent via the unique index — re-ingested from the Player.log backfill,
/// the same bet dedups on (placed_at, pick).
fn migration_v65_arena_bets(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS arena_bets (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            placed_at TEXT NOT NULL,
            pick TEXT NOT NULL,
            opponent TEXT NOT NULL,
            wager INTEGER NOT NULL,
            payout INTEGER NOT NULL,
            won INTEGER NOT NULL
        );
        CREATE UNIQUE INDEX IF NOT EXISTS idx_arena_bets_dedup
            ON arena_bets(placed_at, pick);",
    )?;
    Ok(())
}

/// Migration V64: casino arena fight outcomes (drives the Arena dashboard
/// widget's fighter rankings + head-to-head matchup matrix). Kuzavek narrates
/// each match on the `[NPC Chatter]` channel; we record the matchup and winner
/// only (the player's wager lives in Player.log and isn't tracked here).
/// Idempotent via the unique index — the same match re-ingested from chat
/// backfill dedups on (fought_at, fighter_a, fighter_b).
fn migration_v64_arena_matches(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS arena_matches (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            fought_at TEXT NOT NULL,
            fighter_a TEXT NOT NULL,
            fighter_b TEXT NOT NULL,
            winner TEXT NOT NULL
        );
        CREATE UNIQUE INDEX IF NOT EXISTS idx_arena_dedup
            ON arena_matches(fought_at, fighter_a, fighter_b);",
    )?;
    Ok(())
}

/// Migration V61: recorded poems (Chat Logs → Poems tab).
///
/// Poems recited by other players via `Poem by X` NPC talk screens are captured
/// globally — they are *not* character-scoped, so the same poem heard by any
/// character (or heard more than once) is stored a single time. The
/// UNIQUE(author, title, content) index drives that dedup via INSERT OR IGNORE,
/// so re-scanning a Player.log never produces duplicate rows. The
/// `recorded_by_*` columns retain which character/server first captured the poem
/// for reference only.
fn migration_v61_poems(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS poems (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            author TEXT NOT NULL,
            title TEXT NOT NULL DEFAULT '',
            content TEXT NOT NULL DEFAULT '',
            recorded_at TEXT NOT NULL,
            recorded_by_character TEXT,
            recorded_by_server TEXT,
            created_at TEXT NOT NULL DEFAULT (datetime('now')),
            UNIQUE(author, title, content)
        );
        CREATE INDEX IF NOT EXISTS idx_poems_recorded_at
            ON poems(recorded_at DESC);
        CREATE INDEX IF NOT EXISTS idx_poems_author
            ON poems(author);",
    )?;
    Ok(())
}

/// Migration V62: purge Xogrite Chunks mis-filed as monster loot.
///
/// Xogrite Chunks are gathered from ground nodes (Povus caves), never dropped by
/// a corpse. A parser bug attributed any `ProcessRemoveLoot` to whatever
/// `Search Corpse of X` window was still inside its 30s lifetime, so Xogrite
/// gathered seconds after a nearby kill was filed under that monster (e.g. under
/// "Aktaari Royal Guard"). The parser is fixed to require a *fresh* corpse search
/// (see `CORPSE_LOOT_MAX_SEARCH_AGE_SECS` in `player_event_parser.rs`); this
/// removes the rows the old code already wrote.
///
/// Only the loot rows are deleted — the `enemy_kills` denominators are genuine
/// kills and stay. Match both the internal name (own tracking) and the display
/// name (imported CSV/shared data). Idempotent: a no-op once clean.
fn migration_v62_purge_misattributed_xogrite(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "DELETE FROM enemy_kill_loot
             WHERE item_name IN ('XogriteChunk', 'Xogrite Chunk');
         DELETE FROM imported_enemy_kill_loot_agg
             WHERE item_name IN ('XogriteChunk', 'Xogrite Chunk');",
    )?;
    Ok(())
}

/// Migration V60: index survey-attributed item transactions by their
/// `survey_use_id`.
///
/// The survey tracker's loot/economics/analytics queries join
/// `item_transactions` to `survey_uses` on
/// `CAST(json_extract(source_details,'$.survey_use_id') AS INTEGER)`. That
/// expression can't use any existing index, so every such query full-scanned
/// the entire ledger computing JSON per row — and the loot-summary query's
/// correlated `NOT EXISTS` sub-select made it effectively O(N²). On a real
/// 53k-transaction / 1481-use session this took ~20s per call, run while the
/// global coordinator mutex was held, which froze log ingestion and the UI.
///
/// We materialize the extracted id as a VIRTUAL generated column and index
/// it (partial — only the survey-attributed rows). The column recomputes
/// automatically whenever `source_details` changes, so it stays correct with
/// no write-path changes. Measured: the loot-summary query drops from ~20s to
/// ~25ms (identical results). The column is VIRTUAL (computed on read) so it
/// costs no table storage; the index holds the small set of non-NULL ids.
fn migration_v60_survey_use_id_index(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "ALTER TABLE item_transactions ADD COLUMN survey_use_id INTEGER
             GENERATED ALWAYS AS
             (CAST(json_extract(source_details, '$.survey_use_id') AS INTEGER))
             VIRTUAL;
         CREATE INDEX IF NOT EXISTS idx_item_tx_survey_use
             ON item_transactions(survey_use_id)
             WHERE survey_use_id IS NOT NULL;",
    )?;
    Ok(())
}

/// Migration V59: per-entry variable-slot ingredient pins for crafting projects.
/// Keyword-slot recipes (e.g. brewing) have "variable" ingredient slots that the
/// generic material resolver fills from global preferences. To let a single
/// project entry represent one *specific* combination — such as a chosen Brewery
/// discovery combo — we store the concrete item IDs picked for those slots, in
/// slot order. Empty ('[]') means "resolve slots generically", preserving all
/// existing entries' behavior.
fn migration_v59_project_entry_slot_pins(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "ALTER TABLE crafting_project_entries ADD COLUMN slot_item_ids TEXT NOT NULL DEFAULT '[]';",
    )?;
    Ok(())
}

/// Migration V58: casino roulette spin outcomes (drives the Roulette dashboard
/// widget's outcome-percentage pie chart). Only the winning number is logged
/// (`[Status] Roulette ball ended on N!`); bets/payouts are never written to
/// disk, so we store outcomes only. Idempotent via the unique index — the same
/// spin re-ingested from chat backfill dedups on (spun_at, number).
fn migration_v58_roulette_results(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS roulette_results (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            spun_at TEXT NOT NULL,
            number INTEGER NOT NULL
        );
        CREATE UNIQUE INDEX IF NOT EXISTS idx_roulette_dedup
            ON roulette_results(spun_at, number);",
    )?;
    Ok(())
}

/// Migration v57: fix the chat_messages FTS sync triggers.
///
/// `chat_messages_fts` is an **external-content** FTS5 table (`content=`). For
/// such tables a row must be removed with the special
/// `INSERT INTO ft(ft, rowid, …) VALUES('delete', …)` command, which supplies
/// the original column values so FTS5 knows which tokens to drop. The original
/// delete/update triggers (created with the table) used a plain
/// `DELETE FROM chat_messages_fts WHERE rowid = old.id`; in an AFTER DELETE
/// trigger the content row is already gone, so FTS5 can't read the old values
/// and the index/`_docsize` shadow table accumulate orphaned entries (and can
/// later report "database disk image is malformed").
///
/// This was latent because nothing deleted chat rows before — the new user-data
/// purge does, so the triggers must be correct first. We also run a one-time
/// `'rebuild'` to repair any drift already present.
fn migration_v57_fix_chat_fts_delete_triggers(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "DROP TRIGGER IF EXISTS chat_messages_fts_delete;
         DROP TRIGGER IF EXISTS chat_messages_fts_update;

         CREATE TRIGGER chat_messages_fts_delete AFTER DELETE ON chat_messages BEGIN
             INSERT INTO chat_messages_fts(chat_messages_fts, rowid, message, sender)
             VALUES('delete', old.id, old.message, old.sender);
         END;

         CREATE TRIGGER chat_messages_fts_update AFTER UPDATE ON chat_messages BEGIN
             INSERT INTO chat_messages_fts(chat_messages_fts, rowid, message, sender)
             VALUES('delete', old.id, old.message, old.sender);
             INSERT INTO chat_messages_fts(rowid, message, sender)
             VALUES (new.id, new.message, new.sender);
         END;

         -- Rebuild the index from the content table to clear any orphaned
         -- entries left by the previous (incorrect) delete idiom.
         INSERT INTO chat_messages_fts(chat_messages_fts) VALUES('rebuild');",
    )?;
    Ok(())
}

/// Migration v54: make kill/loot drop data zone-aware. Drop rates vary by zone
/// for the same monster name, so each kill records the area it happened in (the
/// internal area key, e.g. `AreaDesert2`, from `LOADING LEVEL` transitions) and
/// drop-rate stats are keyed by (enemy_name, zone). Imported aggregates carry the
/// zone too. Existing rows get NULL (= "unknown zone") since their zone wasn't
/// captured at the time; new kills and re-imports fill it in.
fn migration_v54_zone_aware_drops(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "ALTER TABLE enemy_kills ADD COLUMN zone TEXT;
         CREATE INDEX idx_enemy_kills_name_zone ON enemy_kills(enemy_name, zone);
         ALTER TABLE imported_enemy_kills_agg ADD COLUMN zone TEXT;
         ALTER TABLE imported_enemy_kill_loot_agg ADD COLUMN zone TEXT;",
    )?;
    Ok(())
}

/// Migration v55: make kill/loot drop data loadout-aware. Drop rates vary with the
/// player's equipped combat-skill pair (e.g. Sword/Shield vs Fire/Ice), so each
/// kill records the active loadout at the time — a normalized, order-independent
/// `skillA+skillB` key (from `ProcessSetActiveSkills` / `ProcessLoadAbilities`).
/// Drop-rate stats can then be segmented by (enemy_name, zone, combat_skills).
/// Imported aggregates carry it too. Existing rows and loadout-less imports get
/// NULL (= "unattributed"); new kills fill it in.
fn migration_v55_loadout_aware_drops(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "ALTER TABLE enemy_kills ADD COLUMN combat_skills TEXT;
         CREATE INDEX idx_enemy_kills_name_zone_skills
             ON enemy_kills(enemy_name, zone, combat_skills);
         ALTER TABLE imported_enemy_kills_agg ADD COLUMN combat_skills TEXT;
         ALTER TABLE imported_enemy_kill_loot_agg ADD COLUMN combat_skills TEXT;",
    )?;
    Ok(())
}

/// Migration v56: running estimate of the council wallet (internal `GOLD`),
/// per character+server. Anchored on the last character export
/// (`anchor_amount` at `anchor_at`); `delta_since` accumulates live wallet
/// gains/losses parsed from chat *after* the anchor. Estimate =
/// `anchor_amount + delta_since`. A fresh export re-anchors (resets
/// `delta_since` to 0). Income is captured well from the logs; most spends are
/// invisible, so the estimate drifts high until the next export re-anchors it.
fn migration_v56_currency_estimate(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS currency_estimate (
            character_name TEXT NOT NULL,
            server_name    TEXT NOT NULL,
            currency_name  TEXT NOT NULL DEFAULT 'GOLD',
            anchor_amount  INTEGER NOT NULL DEFAULT 0,
            anchor_at      TEXT,
            delta_since    INTEGER NOT NULL DEFAULT 0,
            updated_at     TEXT NOT NULL DEFAULT (datetime('now')),
            PRIMARY KEY (character_name, server_name, currency_name)
        );",
    )?;
    Ok(())
}

/// Migration V51: per-monster Combat Wisdom award history (drives the Combat
/// Wisdom dashboard widget — session earns + empirical per-monster cooldowns).
fn migration_v51_combat_wisdom_earns(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS combat_wisdom_earns (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            earned_at TEXT NOT NULL,
            amount INTEGER NOT NULL,
            source_name TEXT,
            verb TEXT NOT NULL,
            zone TEXT
        );
        CREATE UNIQUE INDEX IF NOT EXISTS idx_cw_dedup
            ON combat_wisdom_earns(earned_at, source_name, amount);
        CREATE INDEX IF NOT EXISTS idx_cw_source
            ON combat_wisdom_earns(source_name, earned_at);",
    )?;
    Ok(())
}

/// Migration V52: vendor earnings tracking — add columns to track councils
/// earned from each vendor in the current period and lifetime.
fn migration_v52_vendor_earnings(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "ALTER TABLE game_state_npc_vendor ADD COLUMN councils_earned_current INTEGER DEFAULT 0;
         ALTER TABLE game_state_npc_vendor ADD COLUMN councils_earned_lifetime INTEGER DEFAULT 0;",
    )?;
    Ok(())
}

/// Migration v53: make imported drop-rate data permanent.
///
/// Originally the imported aggregate tables had a
/// `FOREIGN KEY (source_label) REFERENCES imported_kill_sources ON DELETE
/// CASCADE`, so removing an entry from the "Imported Sources" list also deleted
/// its contributed kill/loot data. The new behavior is that imported data merges
/// permanently into the player's database; "Remove" only drops the bookkeeping
/// entry from the sources list, leaving the merged data intact.
///
/// SQLite can't drop a foreign key in place, so rebuild both aggregate tables
/// without the FK (keeping `source_label` as a plain column — re-importing the
/// same file still replaces just that source's rows via the label). No other
/// table references these, so the rebuild is self-contained.
fn migration_v53_persist_imported_drop_data(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "CREATE TABLE imported_enemy_kills_agg_new (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            source_label TEXT NOT NULL,
            enemy_name TEXT NOT NULL,
            total_kills INTEGER NOT NULL DEFAULT 0
        );
        INSERT INTO imported_enemy_kills_agg_new (id, source_label, enemy_name, total_kills)
            SELECT id, source_label, enemy_name, total_kills FROM imported_enemy_kills_agg;
        DROP TABLE imported_enemy_kills_agg;
        ALTER TABLE imported_enemy_kills_agg_new RENAME TO imported_enemy_kills_agg;
        CREATE INDEX idx_imported_kills_source ON imported_enemy_kills_agg(source_label);
        CREATE INDEX idx_imported_kills_enemy ON imported_enemy_kills_agg(enemy_name);

        CREATE TABLE imported_enemy_kill_loot_agg_new (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            source_label TEXT NOT NULL,
            enemy_name TEXT NOT NULL,
            item_name TEXT NOT NULL,
            total_quantity INTEGER NOT NULL DEFAULT 0,
            times_dropped INTEGER NOT NULL DEFAULT 0
        );
        INSERT INTO imported_enemy_kill_loot_agg_new
            (id, source_label, enemy_name, item_name, total_quantity, times_dropped)
            SELECT id, source_label, enemy_name, item_name, total_quantity, times_dropped
            FROM imported_enemy_kill_loot_agg;
        DROP TABLE imported_enemy_kill_loot_agg;
        ALTER TABLE imported_enemy_kill_loot_agg_new RENAME TO imported_enemy_kill_loot_agg;
        CREATE INDEX idx_imported_loot_source ON imported_enemy_kill_loot_agg(source_label);
        CREATE INDEX idx_imported_loot_enemy ON imported_enemy_kill_loot_agg(enemy_name);
        CREATE INDEX idx_imported_loot_item ON imported_enemy_kill_loot_agg(item_name);",
    )?;
    Ok(())
}

/// Migration V21: CP-consuming recipes (shamanic infusion, crafting enhancements) for build planner.
fn migration_v21_build_preset_cp_recipes(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "CREATE TABLE build_preset_cp_recipes (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            preset_id INTEGER NOT NULL REFERENCES build_presets(id) ON DELETE CASCADE,
            equip_slot TEXT NOT NULL,
            recipe_id INTEGER NOT NULL,
            recipe_name TEXT,
            cp_cost INTEGER NOT NULL,
            effect_type TEXT NOT NULL,
            effect_key TEXT NOT NULL,
            sort_order INTEGER DEFAULT 0
        );
        CREATE INDEX idx_build_preset_cp_recipes_preset ON build_preset_cp_recipes(preset_id);",
    )?;
    Ok(())
}

/// Migration V22: Resuscitation tracking — records who rezzed whom and when.
fn migration_v22_resuscitations(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "CREATE TABLE character_resuscitations (
            id INTEGER PRIMARY KEY,
            character_name TEXT NOT NULL,
            server_name TEXT NOT NULL,
            occurred_at TEXT NOT NULL,
            caster_name TEXT NOT NULL,
            target_name TEXT NOT NULL,
            success INTEGER NOT NULL DEFAULT 1,
            area TEXT
        );
        CREATE INDEX idx_resuscitations_character_server
            ON character_resuscitations(character_name, server_name);
        CREATE INDEX idx_resuscitations_occurred_at
            ON character_resuscitations(occurred_at);",
    )?;
    Ok(())
}

/// Migration V68: Other players' stall price observations — manual captures
/// and auto-detected purchases from browsing someone else's stall.
///
/// `price_unit = 0` is the "price unknown" sentinel (the game has no
/// zero-price listings; minimum is 1 council). The unique key drives
/// `INSERT OR IGNORE` idempotency: re-saving or re-detecting the same
/// (timestamp, item, stall, price) never duplicates. Purgeable via
/// `admin_commands::PURGE_TABLES` on the `observed_at` column.
fn migration_v68_stall_price_observations(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "CREATE TABLE stall_price_observations (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            character_name TEXT NOT NULL,
            server_name TEXT NOT NULL,
            item_name TEXT NOT NULL,
            internal_name TEXT,
            item_type_id INTEGER,
            quantity INTEGER NOT NULL DEFAULT 1,
            price_unit INTEGER NOT NULL,
            stall_npc_entity_id INTEGER NOT NULL,
            stall_label TEXT NOT NULL,
            owner_name TEXT,
            source TEXT NOT NULL,
            observed_at TEXT NOT NULL,
            notes TEXT,
            created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
            UNIQUE(observed_at, item_name, stall_npc_entity_id, price_unit)
        );
        CREATE INDEX idx_spo_item ON stall_price_observations(item_type_id);
        CREATE INDEX idx_spo_observed ON stall_price_observations(observed_at DESC);
        CREATE INDEX idx_spo_stall ON stall_price_observations(stall_npc_entity_id);",
    )?;
    Ok(())
}

/// Migration V23: Stall Tracker — persisted PlayerShopLog book entries.
///
/// Every row is scoped to a specific `owner` (character) so multi-alt accounts
/// can't cross-contaminate. The unique key intentionally includes `entry_index`:
/// two buyers of the same item at the same price in the same minute would
/// otherwise collapse into one row since `event_timestamp` only has minute
/// precision. See docs/features/screens/economics/economics-stall-tracker.md.
fn migration_v23_stall_events(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "CREATE TABLE stall_events (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            event_timestamp TEXT NOT NULL,
            event_at TEXT,
            log_timestamp TEXT NOT NULL,
            log_title TEXT NOT NULL,
            action TEXT NOT NULL,
            player TEXT NOT NULL,
            owner TEXT,
            item TEXT,
            quantity INTEGER NOT NULL DEFAULT 1,
            price_unit REAL,
            price_total INTEGER,
            raw_message TEXT NOT NULL,
            entry_index INTEGER NOT NULL DEFAULT 0,
            ignored INTEGER NOT NULL DEFAULT 0,
            created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
            UNIQUE(event_timestamp, raw_message, entry_index)
        );
        CREATE INDEX idx_stall_events_action          ON stall_events(action);
        CREATE INDEX idx_stall_events_created         ON stall_events(created_at DESC);
        CREATE INDEX idx_stall_events_timestamp       ON stall_events(event_timestamp);
        CREATE INDEX idx_stall_events_event_at        ON stall_events(event_at DESC);
        CREATE INDEX idx_stall_events_action_event_at ON stall_events(action, event_at DESC);
        CREATE INDEX idx_stall_events_player          ON stall_events(player);
        CREATE INDEX idx_stall_events_item            ON stall_events(item);
        CREATE INDEX idx_stall_events_owner           ON stall_events(owner);",
    )?;
    Ok(())
}

fn migration_v24_npc_vendor(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS game_state_npc_vendor (
            character_name TEXT NOT NULL,
            server_name TEXT NOT NULL,
            npc_key TEXT NOT NULL,
            vendor_gold_available INTEGER,
            vendor_gold_max INTEGER,
            vendor_gold_timer_start TEXT,
            last_interaction_at TEXT,
            last_sell_at TEXT,
            last_confirmed_at TEXT NOT NULL,
            PRIMARY KEY (character_name, server_name, npc_key)
        );
        CREATE INDEX IF NOT EXISTS idx_gs_npc_vendor_char ON game_state_npc_vendor(character_name, server_name);"
    )?;
    Ok(())
}

/// Migration V25: Item transaction provenance — attach source attribution to
/// every item gain recorded in `item_transactions`.
///
/// - `source_kind` — taxonomy string like `"mining"`, `"survey_map_use"`,
///   `"corpse_search"`, `"vendor_browsing"`, `"storage_browsing"`,
///   `"general_craft"`, `"survey_map_craft"`, `"uncertain"`, `"unknown"`,
///   `"not_applicable"`. Mirrors `ItemProvenance`/`ActivitySource` in the
///   Rust parser.
/// - `source_details` — JSON blob with source-kind-specific fields
///   (node_name, npc_name, recipe label, candidate list for uncertain, etc.).
/// - `confidence` — `"confident"`, `"probable"`, `"weak"`, or NULL when not
///   applicable.
///
/// All three columns are nullable so existing rows (pre-v25) read as NULL,
/// and so transactions inserted before provenance propagates through a
/// feature continue to work without code-churn.
fn migration_v25_item_transaction_provenance(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "ALTER TABLE item_transactions ADD COLUMN source_kind TEXT;
         ALTER TABLE item_transactions ADD COLUMN source_details TEXT;
         ALTER TABLE item_transactions ADD COLUMN confidence TEXT;
         CREATE INDEX IF NOT EXISTS idx_item_tx_source_kind
            ON item_transactions(source_kind);",
    )?;
    Ok(())
}

/// Migration V26: Survey tracker rewrite (Phase 5 of item-provenance overhaul).
///
/// Replaces the old `survey_session_stats` / `survey_events` / `survey_loot_items`
/// schema with a cleaner shape that integrates with the unified provenance
/// pipeline. Per-use loot is no longer denormalized into its own table — it's
/// queried from `item_transactions` filtered by `source_kind` and the
/// `source_details->>'survey_use_id'` JSON field added by the new aggregator.
///
/// See docs/plans/survey-tracker-rewrite.md for the full design.
///
/// **Destructive**: drops the old tables. Beta users may briefly lose
/// historical survey session data; this is acceptable per the user's framing.
fn migration_v26_survey_tracker_rewrite(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "
        -- NOTE: the old survey_session_stats / survey_events / survey_loot_items
        -- tables are intentionally NOT dropped in this migration. They stay in
        -- place alongside the new tables during the cutover so the legacy
        -- survey screen and its supporting Tauri commands keep working until
        -- the frontend rewrite lands. A follow-up migration (v27 most likely)
        -- will drop them once the new frontend is in place and validated.

        -- Per-session header. One session = one start→end span.
        CREATE TABLE survey_sessions (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            character_name TEXT NOT NULL,
            server_name TEXT NOT NULL,
            started_at TEXT NOT NULL,
            ended_at TEXT,
            -- 'manual' | 'crafting' | 'first_use'
            start_trigger TEXT NOT NULL,
            -- only set when start_trigger='crafting': how many maps were crafted
            -- in this session. Auto-end fires when consumed_count >= crafted_count
            -- AND no pending_loot uses remain.
            crafted_count INTEGER,
            consumed_count INTEGER NOT NULL DEFAULT 0,
            notes TEXT
        );
        CREATE INDEX idx_survey_sessions_char
            ON survey_sessions(character_name, server_name);
        -- Partial index for the common 'is there an active session?' query.
        CREATE INDEX idx_survey_sessions_active
            ON survey_sessions(character_name, server_name, ended_at)
            WHERE ended_at IS NULL;

        -- One row per survey-map use. Loot gains link back to this row via
        -- item_transactions.source_details->>'survey_use_id'.
        CREATE TABLE survey_uses (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            session_id INTEGER REFERENCES survey_sessions(id) ON DELETE SET NULL,
            character_name TEXT NOT NULL,
            server_name TEXT NOT NULL,
            used_at TEXT NOT NULL,
            map_internal_name TEXT NOT NULL,
            map_display_name TEXT NOT NULL,
            -- 'basic' | 'motherlode' | 'multihit'
            kind TEXT NOT NULL,
            -- live-tracked area where the use happened. Falls back to the area
            -- token parsed from map_internal_name if live area is unavailable.
            area TEXT,
            -- 'pending_loot' | 'completed' | 'aborted' | 'unknown'
            status TEXT NOT NULL DEFAULT 'pending_loot',
            -- denormalized convenience: total loot quantity attributed to this
            -- use. Updated as item_transactions are inserted; not the source
            -- of truth (item_transactions is).
            loot_qty INTEGER NOT NULL DEFAULT 0
        );
        CREATE INDEX idx_survey_uses_session ON survey_uses(session_id);
        CREATE INDEX idx_survey_uses_char_used
            ON survey_uses(character_name, server_name, used_at);
        -- Partial index for the 'find pending uses to time-out' sweep.
        CREATE INDEX idx_survey_uses_pending
            ON survey_uses(status) WHERE status = 'pending_loot';

        -- Multihit nodes the player is actively mining. Persisted (not
        -- in-memory) so the 30-minute window survives app restart — losing
        -- this state would lose real loot attribution.
        CREATE TABLE open_multihit_nodes (
            node_entity_id INTEGER NOT NULL,
            character_name TEXT NOT NULL,
            server_name TEXT NOT NULL,
            survey_use_id INTEGER NOT NULL REFERENCES survey_uses(id) ON DELETE CASCADE,
            opened_at TEXT NOT NULL,
            last_hit_at TEXT NOT NULL,
            PRIMARY KEY (character_name, server_name, node_entity_id)
        );
        CREATE INDEX idx_open_multihit_nodes_use
            ON open_multihit_nodes(survey_use_id);
        ",
    )?;
    Ok(())
}

/// Migration V27: Drop the legacy survey tables that v26 intentionally left
/// in place during cutover.
///
/// Order of operations across Phase 5:
///   v25 — added source_kind/source_details/confidence to item_transactions
///   v26 — created survey_sessions / survey_uses / open_multihit_nodes
///         (legacy tables intentionally kept alive so the legacy survey screen
///         could keep functioning)
///   v27 — drops the legacy tables now that the frontend's Historical and
///         Analytics tabs have been rewritten on the new ledger
///
/// The legacy schema served the old `SurveySessionTracker` and the History/
/// Analytics tabs that read from it. Both of those code paths are gone now.
/// Any historical session data written to the old tables is permanently lost
/// at this migration — beta users were warned (per the user's framing for
/// this overhaul).
fn migration_v27_drop_legacy_survey_tables(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "DROP TABLE IF EXISTS survey_loot_items;
         DROP TABLE IF EXISTS survey_events;
         DROP TABLE IF EXISTS survey_session_stats;
         -- Survey-sharing tables (also no longer reachable; legacy import/
         -- export was tied to the old per-session schema).
         DROP TABLE IF EXISTS survey_imports;",
    )?;
    Ok(())
}

/// Backfill `survey_uses.area` for rows where it's NULL. Joins
/// `survey_types.zone` (populated at CDN-import time from the item's
/// Description field + the areas map). This catches all historical uses
/// recorded before the aggregator started pulling zone from survey_types.
fn migration_v28_backfill_survey_use_areas(conn: &Connection) -> Result<()> {
    conn.execute(
        "UPDATE survey_uses
            SET area = (
                SELECT st.zone
                  FROM survey_types st
                 WHERE st.internal_name = survey_uses.map_internal_name
            )
          WHERE area IS NULL",
        [],
    )?;
    Ok(())
}

/// Add session name, user-adjusted start/end overrides, and craft/loot
/// timing columns. All nullable so existing rows survive unchanged.
fn migration_v29_session_name_and_timestamps(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "ALTER TABLE survey_sessions ADD COLUMN name TEXT;
         ALTER TABLE survey_sessions ADD COLUMN user_started_at TEXT;
         ALTER TABLE survey_sessions ADD COLUMN user_ended_at TEXT;
         ALTER TABLE survey_sessions ADD COLUMN first_craft_at TEXT;
         ALTER TABLE survey_sessions ADD COLUMN last_craft_at TEXT;
         ALTER TABLE survey_sessions ADD COLUMN first_loot_at TEXT;
         ALTER TABLE survey_sessions ADD COLUMN last_loot_at TEXT;",
    )?;
    Ok(())
}

/// Normalize all survey_uses.area values to proper area keys from
/// survey_types.zone. This catches rows that were written before the CDN
/// ingestion started producing area keys, and rows that were written with
/// old-style zone strings like "Eltibule" instead of "AreaEltibule".
/// Runs once; the CDN-reload backfill in persist_cdn_data handles future cases.
fn migration_v30_normalize_survey_use_areas(conn: &Connection) -> Result<()> {
    // Overwrite ALL non-null area values with the survey_types.zone lookup,
    // not just NULLs — this catches "Eltibule" → "AreaEltibule" etc.
    conn.execute(
        "UPDATE survey_uses
            SET area = (
                SELECT st.zone
                  FROM survey_types st
                 WHERE st.internal_name = survey_uses.map_internal_name
                   AND st.zone IS NOT NULL
            )
          WHERE EXISTS (
                SELECT 1 FROM survey_types st
                 WHERE st.internal_name = survey_uses.map_internal_name
                   AND st.zone IS NOT NULL
                   AND st.zone != COALESCE(survey_uses.area, '')
          )",
        [],
    )?;
    Ok(())
}

/// Migration V5: Build planner tables for saving gear/mod builds per character.
fn migration_v5_build_planner(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "CREATE TABLE build_presets (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            character_id TEXT NOT NULL,
            name TEXT NOT NULL,
            skill_primary TEXT,
            skill_secondary TEXT,
            target_level INTEGER DEFAULT 90,
            target_rarity TEXT DEFAULT 'Epic',
            notes TEXT,
            created_at TEXT NOT NULL DEFAULT (datetime('now')),
            updated_at TEXT NOT NULL DEFAULT (datetime('now'))
        );

        CREATE TABLE build_preset_mods (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            preset_id INTEGER NOT NULL REFERENCES build_presets(id) ON DELETE CASCADE,
            equip_slot TEXT NOT NULL,
            power_name TEXT NOT NULL,
            tier INTEGER,
            is_augment INTEGER DEFAULT 0,
            sort_order INTEGER DEFAULT 0
        );

        CREATE INDEX idx_build_presets_character ON build_presets(character_id);
        CREATE INDEX idx_build_preset_mods_preset ON build_preset_mods(preset_id);",
    )?;
    Ok(())
}

/// Migration V7: Ability bar planning for build presets.
/// Stores which abilities the user wants in each bar (primary, secondary, sidebar).
fn migration_v7_build_preset_abilities(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "CREATE TABLE build_preset_abilities (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            preset_id INTEGER NOT NULL REFERENCES build_presets(id) ON DELETE CASCADE,
            bar TEXT NOT NULL,
            slot_position INTEGER NOT NULL,
            ability_id INTEGER NOT NULL,
            ability_name TEXT
        );

        CREATE INDEX idx_build_preset_abilities_preset ON build_preset_abilities(preset_id);",
    )?;
    Ok(())
}

/// Migration V8: Add per-slot level, rarity, and crafting flags to slot items.
/// Items can now have individual level/rarity instead of using a single global value.
fn migration_v8_slot_item_level_rarity(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "ALTER TABLE build_preset_slot_items ADD COLUMN slot_level INTEGER NOT NULL DEFAULT 90;
         ALTER TABLE build_preset_slot_items ADD COLUMN slot_rarity TEXT NOT NULL DEFAULT 'Epic';
         ALTER TABLE build_preset_slot_items ADD COLUMN is_crafted INTEGER NOT NULL DEFAULT 0;
         ALTER TABLE build_preset_slot_items ADD COLUMN is_masterwork INTEGER NOT NULL DEFAULT 0;",
    )?;
    Ok(())
}

/// Migration V9: Add per-slot skill overrides to build preset slot items.
/// Each slot can now independently choose which two skills its mods come from.
fn migration_v9_slot_per_slot_skills(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "ALTER TABLE build_preset_slot_items ADD COLUMN slot_skill_primary TEXT;
         ALTER TABLE build_preset_slot_items ADD COLUMN slot_skill_secondary TEXT;",
    )?;
    Ok(())
}

/// Migration V10: Add project grouping and per-entry stock targets for crafting projects.
/// group_name allows organizing projects under collapsible group headers.
/// target_stock enables "restock to X" mode where craft quantity auto-calculates from inventory.
fn migration_v10_crafting_groups_and_stock_targets(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "ALTER TABLE crafting_projects ADD COLUMN group_name TEXT DEFAULT NULL;
         ALTER TABLE crafting_project_entries ADD COLUMN target_stock INTEGER DEFAULT NULL;",
    )?;
    Ok(())
}

/// Migration V11: Character deaths tracking table.
/// Records each player death with the killer, ability, damage, and area context.
fn migration_v11_character_deaths(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "CREATE TABLE character_deaths (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            character_name TEXT NOT NULL,
            server_name TEXT NOT NULL,
            died_at TEXT NOT NULL,
            killer_name TEXT NOT NULL,
            killer_entity_id TEXT,
            killing_ability TEXT NOT NULL,
            health_damage INTEGER NOT NULL,
            armor_damage INTEGER NOT NULL DEFAULT 0,
            area TEXT,
            created_at TEXT NOT NULL DEFAULT (datetime('now'))
        );
        CREATE INDEX idx_deaths_char ON character_deaths(character_name, server_name);",
    )?;
    Ok(())
}

/// Migration V12: Track current area per character in game state.
fn migration_v12_game_state_area(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "CREATE TABLE game_state_area (
            character_name TEXT NOT NULL,
            server_name TEXT NOT NULL,
            area_name TEXT NOT NULL,
            last_confirmed_at TEXT NOT NULL,
            PRIMARY KEY (character_name, server_name)
        );",
    )?;
    Ok(())
}

/// Migration V13: Add damage_type column to character_deaths for CDN ability enrichment.
fn migration_v13_death_damage_type(conn: &Connection) -> Result<()> {
    conn.execute_batch("ALTER TABLE character_deaths ADD COLUMN damage_type TEXT DEFAULT NULL;")?;
    Ok(())
}

/// Migration V14: Damage sources leading up to each death.
fn migration_v14_death_damage_sources(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "CREATE TABLE death_damage_sources (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            death_id INTEGER NOT NULL REFERENCES character_deaths(id) ON DELETE CASCADE,
            event_order INTEGER NOT NULL,
            timestamp TEXT NOT NULL,
            attacker_name TEXT NOT NULL,
            attacker_entity_id TEXT,
            ability_name TEXT NOT NULL,
            health_damage INTEGER NOT NULL DEFAULT 0,
            armor_damage INTEGER NOT NULL DEFAULT 0,
            is_crit INTEGER NOT NULL DEFAULT 0
        );
        CREATE INDEX idx_damage_sources_death ON death_damage_sources(death_id);",
    )?;
    Ok(())
}

/// Migration V15: Item transaction ledger — records every item gain/loss from both
/// Player.log and chat status for historical analysis and cross-source correlation.
fn migration_v15_item_transactions(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "CREATE TABLE item_transactions (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            timestamp TEXT NOT NULL,
            character_name TEXT NOT NULL,
            server_name TEXT NOT NULL,
            item_name TEXT NOT NULL,
            internal_name TEXT,
            item_type_id INTEGER,
            quantity INTEGER NOT NULL,
            context TEXT NOT NULL,
            source TEXT NOT NULL,
            instance_id INTEGER,
            vault_key TEXT,
            created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
        );
        CREATE INDEX idx_item_tx_char ON item_transactions(character_name, server_name);
        CREATE INDEX idx_item_tx_item ON item_transactions(item_name);
        CREATE INDEX idx_item_tx_time ON item_transactions(timestamp);",
    )?;
    Ok(())
}

/// Migration V6: Base item selection per equipment slot in build planner.
/// Stores which item the user wants in each slot of a build preset.
fn migration_v6_build_preset_slot_items(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "CREATE TABLE build_preset_slot_items (
            preset_id INTEGER NOT NULL REFERENCES build_presets(id) ON DELETE CASCADE,
            equip_slot TEXT NOT NULL,
            item_id INTEGER NOT NULL,
            item_name TEXT,
            PRIMARY KEY (preset_id, equip_slot)
        );

        CREATE INDEX idx_build_preset_slot_items_preset ON build_preset_slot_items(preset_id);",
    )?;
    Ok(())
}

/// Migration V4: Remove the ON DELETE CASCADE foreign key from crafting_project_entries.recipe_id.
/// The recipes table is CDN data that gets wiped and reloaded on each CDN update.
/// The CASCADE FK caused all project entries to be deleted whenever CDN data refreshed.
/// We keep the project_id CASCADE (user-owned data) but drop the recipe_id FK entirely,
/// since recipe_name is already denormalized for display purposes.
fn migration_v4_fix_crafting_project_cascade(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "CREATE TABLE crafting_project_entries_new (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            project_id INTEGER NOT NULL,
            recipe_id INTEGER NOT NULL,
            recipe_name TEXT NOT NULL,
            quantity INTEGER NOT NULL DEFAULT 1,
            sort_order INTEGER NOT NULL DEFAULT 0,
            expanded_ingredient_ids TEXT NOT NULL DEFAULT '[]',
            FOREIGN KEY (project_id) REFERENCES crafting_projects(id) ON DELETE CASCADE
        );
        INSERT INTO crafting_project_entries_new
            SELECT * FROM crafting_project_entries;
        DROP TABLE crafting_project_entries;
        ALTER TABLE crafting_project_entries_new RENAME TO crafting_project_entries;
        CREATE INDEX idx_cpe_project ON crafting_project_entries(project_id);",
    )?;
    Ok(())
}

/// Migration V3: Fix skill level data after the incorrect v2 migration.
/// v2 set base_level = level - bonus_levels, but level was still the raw (base) value,
/// so base_level ended up as raw - bonus (wrong). Level was never updated to be the total.
/// Fix: set base_level = level (which is still raw/base), then level = base_level + bonus_levels.
fn migration_v3_fix_skill_levels(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "UPDATE game_state_skills SET base_level = level;
         UPDATE game_state_skills SET level = base_level + bonus_levels;",
    )?;
    Ok(())
}

/// Migration V2: Add base_level column to game_state_skills.
/// `level` stores the total (base + bonus) — what the game displays.
/// `base_level` stores level without bonuses — used for XP table indexing.
fn migration_v2_skill_base_level(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "ALTER TABLE game_state_skills ADD COLUMN base_level INTEGER NOT NULL DEFAULT 0;
         -- Existing `level` column stores the base (raw=). Copy it to base_level,
         -- then update level to be the total (base + bonus).
         UPDATE game_state_skills SET base_level = level;
         UPDATE game_state_skills SET level = level + bonus_levels;",
    )?;
    Ok(())
}

/// Migration V1: Complete unified schema with all tables
fn migration_v1_unified_schema(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "
        -- ============================================================
        -- CDN DATA TABLES (game reference data)
        -- ============================================================

        -- CDN Version tracking
        CREATE TABLE cdn_version (
            id INTEGER PRIMARY KEY CHECK (id = 1),
            version INTEGER NOT NULL,
            loaded_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
        );

        -- Items
        CREATE TABLE items (
            id INTEGER PRIMARY KEY,
            name TEXT NOT NULL,
            description TEXT,
            icon_id INTEGER,
            value REAL,
            max_stack_size REAL,
            keywords TEXT,
            effect_descs TEXT,
            internal_name TEXT,
            food_desc TEXT,
            equip_slot TEXT,
            num_uses INTEGER,
            skill_reqs TEXT,
            behaviors TEXT,
            bestow_recipes TEXT,
            bestow_ability TEXT,
            bestow_quest TEXT,
            bestow_title INTEGER,
            craft_points INTEGER,
            crafting_target_level INTEGER,
            tsys_profile TEXT,
            raw_json TEXT NOT NULL
        );
        CREATE INDEX idx_items_name ON items(name COLLATE NOCASE);
        CREATE INDEX idx_items_icon ON items(icon_id);
        CREATE INDEX idx_items_equip_slot ON items(equip_slot);
        CREATE INDEX idx_items_food_desc ON items(food_desc);
        CREATE INDEX idx_items_tsys_profile ON items(tsys_profile);

        -- Skills
        CREATE TABLE skills (
            id INTEGER PRIMARY KEY,
            name TEXT NOT NULL UNIQUE,
            description TEXT,
            icon_id INTEGER,
            xp_table TEXT,
            keywords TEXT,
            combat BOOLEAN,
            max_bonus_levels INTEGER,
            parents TEXT,
            advancement_table TEXT,
            guest_level_cap INTEGER,
            hide_when_zero BOOLEAN,
            advancement_hints TEXT,
            rewards TEXT,
            reports TEXT,
            raw_json TEXT NOT NULL
        );
        CREATE INDEX idx_skills_name ON skills(name COLLATE NOCASE);

        -- Abilities
        CREATE TABLE abilities (
            id INTEGER PRIMARY KEY,
            name TEXT NOT NULL,
            internal_name TEXT,
            description TEXT,
            icon_id INTEGER,
            skill TEXT,
            level_req REAL,
            keywords TEXT,
            damage_type TEXT,
            reset_time REAL,
            target TEXT,
            prerequisite TEXT,
            upgrade_of TEXT,
            is_harmless BOOLEAN,
            animation TEXT,
            special_info TEXT,
            works_underwater BOOLEAN,
            works_while_falling BOOLEAN,
            pve TEXT,
            pvp TEXT,
            mana_cost INTEGER,
            power_cost INTEGER,
            armor_cost INTEGER,
            health_cost INTEGER,
            range REAL,
            raw_json TEXT NOT NULL
        );
        CREATE INDEX idx_abilities_name ON abilities(name COLLATE NOCASE);
        CREATE INDEX idx_abilities_skill ON abilities(skill);
        CREATE INDEX idx_abilities_damage_type ON abilities(damage_type);

        -- Recipes
        CREATE TABLE recipes (
            id INTEGER PRIMARY KEY,
            name TEXT NOT NULL,
            skill TEXT,
            skill_level_req REAL,
            icon_id INTEGER,
            num_result_items INTEGER,
            action_label TEXT,
            keywords TEXT,
            shares_name_with_item_id INTEGER,
            result_item_ids TEXT,
            ingredient_item_ids TEXT,
            result_effects TEXT,
            usage_delay REAL,
            reward_skill_xp_drop_off_level INTEGER,
            sort_skill TEXT,
            raw_json TEXT NOT NULL
        );
        CREATE INDEX idx_recipes_name ON recipes(name COLLATE NOCASE);
        CREATE INDEX idx_recipes_skill ON recipes(skill);
        CREATE INDEX idx_recipes_sort_skill ON recipes(sort_skill);

        -- Recipe Ingredients (normalized)
        CREATE TABLE recipe_ingredients (
            recipe_id INTEGER NOT NULL,
            item_id INTEGER,
            item_keys TEXT,
            description TEXT,
            stack_size INTEGER NOT NULL,
            chance_to_consume REAL,
            FOREIGN KEY (recipe_id) REFERENCES recipes(id) ON DELETE CASCADE,
            FOREIGN KEY (item_id) REFERENCES items(id) ON DELETE CASCADE
        );
        CREATE INDEX idx_recipe_ingredients_recipe ON recipe_ingredients(recipe_id);
        CREATE INDEX idx_recipe_ingredients_item ON recipe_ingredients(item_id);

        -- NPCs
        CREATE TABLE npcs (
            key TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            area_name TEXT,
            area_description TEXT,
            preferences TEXT,
            pos TEXT,
            services TEXT,
            raw_json TEXT NOT NULL
        );
        CREATE INDEX idx_npcs_name ON npcs(name COLLATE NOCASE);
        CREATE INDEX idx_npcs_area ON npcs(area_name);

        -- NPC Skills (many-to-many)
        CREATE TABLE npc_skills (
            npc_key TEXT NOT NULL,
            skill TEXT NOT NULL,
            PRIMARY KEY (npc_key, skill),
            FOREIGN KEY (npc_key) REFERENCES npcs(key) ON DELETE CASCADE
        );
        CREATE INDEX idx_npc_skills_skill ON npc_skills(skill);

        -- Quests
        CREATE TABLE quests (
            internal_name TEXT PRIMARY KEY,
            raw_data TEXT NOT NULL
        );

        -- XP Tables
        CREATE TABLE xp_tables (
            id INTEGER PRIMARY KEY,
            internal_name TEXT,
            xp_amounts TEXT,
            raw_json TEXT NOT NULL
        );
        CREATE INDEX idx_xp_tables_name ON xp_tables(internal_name);

        -- TSys Client Info (crafting system definitions)
        CREATE TABLE tsys_client_info (
            key TEXT PRIMARY KEY,
            internal_name TEXT,
            skill TEXT,
            slots TEXT,
            prefix TEXT,
            suffix TEXT,
            tiers TEXT,
            is_unavailable BOOLEAN,
            is_hidden_from_transmutation BOOLEAN,
            raw_json TEXT NOT NULL
        );
        CREATE INDEX idx_tsys_client_info_skill ON tsys_client_info(skill);

        -- Item Uses (recipe cross-reference)
        CREATE TABLE item_uses (
            key TEXT PRIMARY KEY,
            recipes_that_use_item TEXT,
            raw_json TEXT NOT NULL
        );

        -- Areas
        CREATE TABLE areas (
            key TEXT PRIMARY KEY,
            friendly_name TEXT,
            short_friendly_name TEXT,
            raw_json TEXT NOT NULL
        );
        CREATE INDEX idx_areas_friendly_name ON areas(friendly_name);

        -- ============================================================
        -- PLAYER DATA TABLES (user-generated data)
        -- ============================================================

        -- User Characters
        CREATE TABLE user_characters (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            character_name TEXT NOT NULL,
            server_name TEXT NOT NULL,
            source TEXT NOT NULL CHECK (source IN ('report', 'manual', 'login')),
            is_active BOOLEAN NOT NULL DEFAULT 0,
            latest_report_time TIMESTAMP,
            last_login_time TIMESTAMP,
            created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
            updated_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
            UNIQUE(character_name, server_name)
        );
        CREATE INDEX idx_user_characters_active ON user_characters(is_active);

        -- Vendor Prices
        CREATE TABLE vendor_prices (
            npc_key TEXT NOT NULL,
            item_id INTEGER NOT NULL,
            sell_price REAL NOT NULL,
            currency TEXT DEFAULT 'Councils',
            updated_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
            PRIMARY KEY (npc_key, item_id, currency),
            FOREIGN KEY (npc_key) REFERENCES npcs(key) ON DELETE CASCADE,
            FOREIGN KEY (item_id) REFERENCES items(id) ON DELETE CASCADE
        );
        CREATE INDEX idx_vendor_prices_item ON vendor_prices(item_id);
        CREATE INDEX idx_vendor_prices_npc ON vendor_prices(npc_key);

        -- Market Prices
        CREATE TABLE market_prices (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            item_id INTEGER NOT NULL,
            price REAL NOT NULL,
            quantity INTEGER NOT NULL DEFAULT 1,
            vendor_type TEXT CHECK (vendor_type IN ('bazaar', 'player_vendor', 'work_order')),
            vendor_name TEXT,
            observed_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
            notes TEXT,
            FOREIGN KEY (item_id) REFERENCES items(id) ON DELETE CASCADE
        );
        CREATE INDEX idx_market_prices_item ON market_prices(item_id);
        CREATE INDEX idx_market_prices_observed ON market_prices(observed_at DESC);
        CREATE INDEX idx_market_prices_vendor_type ON market_prices(vendor_type);

        -- Market Values (user-specified player-to-player prices)
        CREATE TABLE market_values (
            server_name TEXT NOT NULL,
            item_type_id INTEGER NOT NULL,
            item_name TEXT NOT NULL,
            market_value INTEGER NOT NULL,
            notes TEXT,
            updated_at TEXT NOT NULL DEFAULT (datetime('now')),
            PRIMARY KEY (server_name, item_type_id)
        );
        CREATE INDEX idx_market_values_item ON market_values(item_type_id);
        CREATE INDEX idx_market_values_name ON market_values(item_name);

        -- Sales History
        CREATE TABLE sales_history (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            item_id INTEGER NOT NULL,
            quantity INTEGER NOT NULL,
            sale_price REAL NOT NULL,
            sale_method TEXT CHECK (sale_method IN ('vendor', 'bazaar', 'trade', 'consignment')),
            buyer_name TEXT,
            sold_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
            notes TEXT,
            FOREIGN KEY (item_id) REFERENCES items(id) ON DELETE CASCADE
        );
        CREATE INDEX idx_sales_history_item ON sales_history(item_id);
        CREATE INDEX idx_sales_history_sold_at ON sales_history(sold_at DESC);

        -- Event Log
        CREATE TABLE event_log (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            event_type TEXT NOT NULL,
            event_data TEXT NOT NULL,
            created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
        );
        CREATE INDEX idx_event_log_type ON event_log(event_type);
        CREATE INDEX idx_event_log_created ON event_log(created_at DESC);

        -- ============================================================
        -- SURVEY DATA TABLES
        -- ============================================================

        -- Pre-parsed survey types (derived from items + recipes during CDN ingestion)
        CREATE TABLE survey_types (
            item_id          INTEGER PRIMARY KEY,
            internal_name    TEXT NOT NULL,
            name             TEXT NOT NULL,
            zone             TEXT,
            icon_id          INTEGER,
            survey_category  TEXT NOT NULL,
            is_motherlode    BOOLEAN NOT NULL DEFAULT 0,
            skill_req_name   TEXT,
            skill_req_level  INTEGER,
            survey_skill_req INTEGER,
            recipe_id        INTEGER,
            survey_xp        REAL,
            survey_xp_first_time REAL,
            crafting_cost    REAL
        );
        CREATE INDEX idx_survey_types_zone ON survey_types(zone);
        CREATE INDEX idx_survey_types_category ON survey_types(survey_category);
        CREATE INDEX idx_survey_types_name ON survey_types(name COLLATE NOCASE);

        -- Survey Session Stats - pre-computed aggregate summary for historical browsing
        CREATE TABLE survey_session_stats (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            name TEXT NOT NULL DEFAULT 'Survey Session',
            notes TEXT NOT NULL DEFAULT '',
            start_time TIMESTAMP NOT NULL,
            end_time TIMESTAMP,
            maps_started INTEGER NOT NULL DEFAULT 0,
            surveys_located INTEGER NOT NULL DEFAULT 0,
            surveys_completed INTEGER NOT NULL DEFAULT 0,
            surveying_xp_gained INTEGER NOT NULL DEFAULT 0,
            mining_xp_gained INTEGER NOT NULL DEFAULT 0,
            geology_xp_gained INTEGER NOT NULL DEFAULT 0,
            total_revenue INTEGER NOT NULL DEFAULT 0,
            total_cost INTEGER NOT NULL DEFAULT 0,
            total_profit INTEGER NOT NULL DEFAULT 0,
            profit_per_hour INTEGER NOT NULL DEFAULT 0,
            elapsed_seconds INTEGER NOT NULL DEFAULT 0,
            is_manual BOOLEAN DEFAULT 0,
            speed_bonus_count INTEGER NOT NULL DEFAULT 0,
            survey_types_used TEXT,
            maps_used_summary TEXT,
            created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
        );
        CREATE INDEX idx_survey_session_stats_start ON survey_session_stats(start_time DESC);
        CREATE INDEX idx_survey_session_stats_created ON survey_session_stats(created_at DESC);

        -- Survey Events - detailed logging of individual survey events
        CREATE TABLE survey_events (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            timestamp TIMESTAMP NOT NULL,
            session_id INTEGER,
            event_type TEXT NOT NULL CHECK (event_type IN ('session_start', 'completed', 'map_crafted', 'survey_used', 'motherlode_completed')),
            map_type TEXT,
            survey_type TEXT,
            speed_bonus_earned BOOLEAN DEFAULT 0,
            created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
            FOREIGN KEY (session_id) REFERENCES survey_session_stats(id) ON DELETE CASCADE
        );
        CREATE INDEX idx_survey_events_timestamp ON survey_events(timestamp DESC);
        CREATE INDEX idx_survey_events_session ON survey_events(session_id);
        CREATE INDEX idx_survey_events_type ON survey_events(event_type);

        -- Survey Loot Items - individual items obtained from surveys
        CREATE TABLE survey_loot_items (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            event_id INTEGER NOT NULL,
            item_id INTEGER,
            item_name TEXT NOT NULL,
            quantity INTEGER NOT NULL DEFAULT 1,
            is_speed_bonus BOOLEAN NOT NULL DEFAULT 0,
            is_primary BOOLEAN NOT NULL DEFAULT 0,
            obtained_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
            FOREIGN KEY (event_id) REFERENCES survey_events(id) ON DELETE CASCADE,
            FOREIGN KEY (item_id) REFERENCES items(id) ON DELETE SET NULL
        );
        CREATE INDEX idx_survey_loot_items_event ON survey_loot_items(event_id);
        CREATE INDEX idx_survey_loot_items_item ON survey_loot_items(item_id);
        CREATE INDEX idx_survey_loot_items_name ON survey_loot_items(item_name);
        CREATE INDEX idx_survey_loot_items_speed_bonus ON survey_loot_items(is_speed_bonus);
        CREATE INDEX idx_survey_loot_items_obtained ON survey_loot_items(obtained_at DESC);

        -- ============================================================
        -- CHAT DATA TABLES
        -- ============================================================

        -- Chat Messages with deduplication constraint
        CREATE TABLE chat_messages (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            timestamp TIMESTAMP NOT NULL,
            channel TEXT,
            sender TEXT,
            message TEXT NOT NULL,
            is_system BOOLEAN NOT NULL DEFAULT 0,
            log_file TEXT NOT NULL,
            from_player BOOLEAN DEFAULT NULL,
            created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
        );
        CREATE INDEX idx_chat_messages_timestamp ON chat_messages(timestamp DESC);
        CREATE INDEX idx_chat_messages_channel ON chat_messages(channel);
        CREATE INDEX idx_chat_messages_sender ON chat_messages(sender);
        CREATE INDEX idx_chat_messages_log_file ON chat_messages(log_file);
        CREATE INDEX idx_chat_messages_created ON chat_messages(created_at DESC);

        -- Unique constraint to prevent duplicate messages
        CREATE UNIQUE INDEX idx_chat_messages_unique
        ON chat_messages(timestamp, channel, sender, message);

        -- Full-text search index for message content
        CREATE VIRTUAL TABLE chat_messages_fts USING fts5(
            message,
            sender,
            content=chat_messages,
            content_rowid=id
        );

        -- Triggers to keep FTS index in sync
        CREATE TRIGGER chat_messages_fts_insert AFTER INSERT ON chat_messages BEGIN
            INSERT INTO chat_messages_fts(rowid, message, sender)
            VALUES (new.id, new.message, new.sender);
        END;

        CREATE TRIGGER chat_messages_fts_delete AFTER DELETE ON chat_messages BEGIN
            DELETE FROM chat_messages_fts WHERE rowid = old.id;
        END;

        CREATE TRIGGER chat_messages_fts_update AFTER UPDATE ON chat_messages BEGIN
            DELETE FROM chat_messages_fts WHERE rowid = old.id;
            INSERT INTO chat_messages_fts(rowid, message, sender)
            VALUES (new.id, new.message, new.sender);
        END;

        -- Chat Item Links - items referenced in chat messages
        CREATE TABLE chat_item_links (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            message_id INTEGER NOT NULL,
            raw_text TEXT NOT NULL,
            item_name TEXT NOT NULL,
            skill TEXT,
            item_id INTEGER,
            created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
            FOREIGN KEY (message_id) REFERENCES chat_messages(id) ON DELETE CASCADE,
            FOREIGN KEY (item_id) REFERENCES items(id) ON DELETE SET NULL
        );
        CREATE INDEX idx_chat_item_links_message ON chat_item_links(message_id);
        CREATE INDEX idx_chat_item_links_item ON chat_item_links(item_id);
        CREATE INDEX idx_chat_item_links_item_name ON chat_item_links(item_name);
        CREATE INDEX idx_chat_item_links_skill ON chat_item_links(skill);

        -- Chat log file tracking (legacy - will be replaced by log_file_positions)
        CREATE TABLE chat_log_files (
            file_path TEXT PRIMARY KEY,
            file_name TEXT NOT NULL,
            file_date TEXT NOT NULL,
            last_position INTEGER NOT NULL DEFAULT 0,
            last_processed TIMESTAMP,
            player_name TEXT,
            created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
        );
        CREATE INDEX idx_chat_log_files_date ON chat_log_files(file_date DESC);

        -- ============================================================
        -- LOG FILE POSITION TRACKING (unified for all log types)
        -- ============================================================

        CREATE TABLE log_file_positions (
            file_path TEXT PRIMARY KEY,
            file_type TEXT NOT NULL CHECK (file_type IN ('chat', 'player')),
            last_position INTEGER NOT NULL DEFAULT 0,
            last_modified TIMESTAMP,
            last_processed TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
            player_name TEXT,
            metadata TEXT
        );
        CREATE INDEX idx_log_positions_type ON log_file_positions(file_type);
        CREATE INDEX idx_log_positions_processed ON log_file_positions(last_processed DESC);

        -- ============================================================
        -- CHARACTER SNAPSHOT TABLES (from /outputcharacter and /outputitems)
        -- ============================================================

        -- Character snapshots (from /outputcharacter)
        CREATE TABLE character_snapshots (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            character_name TEXT NOT NULL,
            server_name TEXT NOT NULL,
            snapshot_timestamp TIMESTAMP NOT NULL,
            race TEXT,
            import_date TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
            raw_json TEXT NOT NULL,
            UNIQUE(character_name, server_name, snapshot_timestamp)
        );
        CREATE INDEX idx_snapshots_char ON character_snapshots(character_name, snapshot_timestamp DESC);
        CREATE INDEX idx_snapshots_import ON character_snapshots(import_date DESC);

        -- Skill levels per snapshot
        CREATE TABLE character_skill_levels (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            snapshot_id INTEGER NOT NULL,
            skill_name TEXT NOT NULL,
            level INTEGER NOT NULL,
            bonus_levels INTEGER NOT NULL DEFAULT 0,
            xp_toward_next INTEGER NOT NULL DEFAULT 0,
            xp_needed_for_next INTEGER NOT NULL DEFAULT 0,
            FOREIGN KEY (snapshot_id) REFERENCES character_snapshots(id) ON DELETE CASCADE
        );
        CREATE INDEX idx_skill_levels_snapshot ON character_skill_levels(snapshot_id);
        CREATE INDEX idx_skill_levels_skill ON character_skill_levels(skill_name, snapshot_id);

        -- NPC favor levels per snapshot
        CREATE TABLE character_npc_favor (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            snapshot_id INTEGER NOT NULL,
            npc_key TEXT NOT NULL,
            favor_level TEXT NOT NULL,
            FOREIGN KEY (snapshot_id) REFERENCES character_snapshots(id) ON DELETE CASCADE
        );
        CREATE INDEX idx_npc_favor_snapshot ON character_npc_favor(snapshot_id);
        CREATE INDEX idx_npc_favor_npc ON character_npc_favor(npc_key, snapshot_id);

        -- Item snapshots (from /outputitems)
        CREATE TABLE character_item_snapshots (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            character_name TEXT NOT NULL,
            server_name TEXT NOT NULL,
            snapshot_timestamp TIMESTAMP NOT NULL,
            import_date TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
            raw_json TEXT NOT NULL,
            UNIQUE(character_name, server_name, snapshot_timestamp)
        );
        CREATE INDEX idx_item_snapshots_char ON character_item_snapshots(character_name, snapshot_timestamp DESC);
        CREATE INDEX idx_item_snapshots_import ON character_item_snapshots(import_date DESC);

        -- Individual items in snapshots
        CREATE TABLE character_snapshot_items (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            item_snapshot_id INTEGER NOT NULL,
            type_id INTEGER NOT NULL,
            storage_vault TEXT NOT NULL DEFAULT '',
            is_in_inventory BOOLEAN NOT NULL DEFAULT 0,
            stack_size INTEGER NOT NULL,
            value INTEGER,
            item_name TEXT NOT NULL,
            rarity TEXT,
            slot TEXT,
            level INTEGER,
            is_crafted BOOLEAN NOT NULL DEFAULT 0,
            crafter TEXT,
            durability REAL,
            craft_points INTEGER,
            uses_remaining INTEGER,
            transmute_count INTEGER,
            attuned_to TEXT,
            tsys_powers TEXT,
            tsys_imbue_power TEXT,
            tsys_imbue_power_tier INTEGER,
            pet_husbandry_state TEXT,
            FOREIGN KEY (item_snapshot_id) REFERENCES character_item_snapshots(id) ON DELETE CASCADE
        );
        CREATE INDEX idx_snapshot_items_snapshot ON character_snapshot_items(item_snapshot_id);
        CREATE INDEX idx_snapshot_items_vault ON character_snapshot_items(storage_vault);
        CREATE INDEX idx_snapshot_items_type ON character_snapshot_items(type_id);

        -- Recipe completions per snapshot
        CREATE TABLE character_recipe_completions (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            snapshot_id INTEGER NOT NULL,
            recipe_key TEXT NOT NULL,
            completions INTEGER NOT NULL,
            FOREIGN KEY (snapshot_id) REFERENCES character_snapshots(id) ON DELETE CASCADE
        );
        CREATE INDEX idx_recipe_completions_snapshot ON character_recipe_completions(snapshot_id);
        CREATE INDEX idx_recipe_completions_key ON character_recipe_completions(recipe_key, snapshot_id);

        -- Character stats per snapshot
        CREATE TABLE character_stats (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            snapshot_id INTEGER NOT NULL,
            stat_key TEXT NOT NULL,
            value REAL NOT NULL,
            FOREIGN KEY (snapshot_id) REFERENCES character_snapshots(id) ON DELETE CASCADE
        );
        CREATE INDEX idx_character_stats_snapshot ON character_stats(snapshot_id);
        CREATE INDEX idx_character_stats_key ON character_stats(stat_key, snapshot_id);

        -- Character currencies per snapshot
        CREATE TABLE character_currencies (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            snapshot_id INTEGER NOT NULL,
            currency_key TEXT NOT NULL,
            amount INTEGER NOT NULL,
            FOREIGN KEY (snapshot_id) REFERENCES character_snapshots(id) ON DELETE CASCADE
        );
        CREATE INDEX idx_character_currencies_snapshot ON character_currencies(snapshot_id);
        CREATE INDEX idx_character_currencies_key ON character_currencies(currency_key, snapshot_id);

        -- Active quests per snapshot (from /outputcharacter ActiveQuests + ActiveWorkOrders + CompletedWorkOrders + CompletedQuests)
        -- category is one of: 'active', 'work_order', 'completed_work_order', 'completed'
        CREATE TABLE character_active_quests (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            snapshot_id INTEGER NOT NULL,
            quest_key TEXT NOT NULL,
            category TEXT NOT NULL DEFAULT 'active',
            FOREIGN KEY (snapshot_id) REFERENCES character_snapshots(id) ON DELETE CASCADE
        );
        CREATE INDEX idx_active_quests_snapshot ON character_active_quests(snapshot_id);
        CREATE INDEX idx_active_quests_key ON character_active_quests(quest_key, snapshot_id);

        -- ============================================================
        -- GOURMAND TRACKER TABLES
        -- ============================================================

        -- Pre-parsed food items (derived from items table during CDN ingestion)
        CREATE TABLE foods (
            item_id     INTEGER PRIMARY KEY,
            name        TEXT NOT NULL,
            icon_id     INTEGER,
            food_category TEXT NOT NULL,
            food_level  INTEGER NOT NULL,
            gourmand_req INTEGER,
            effect_descs TEXT NOT NULL,
            keywords    TEXT NOT NULL,
            value       REAL
        );
        CREATE INDEX idx_foods_category ON foods(food_category);
        CREATE INDEX idx_foods_level ON foods(food_level);
        CREATE INDEX idx_foods_name ON foods(name COLLATE NOCASE);

        -- Last-known gourmand report (single snapshot, overwritten on each import)
        CREATE TABLE gourmand_eaten_foods (
            food_name   TEXT PRIMARY KEY,
            times_eaten INTEGER NOT NULL,
            imported_at TEXT NOT NULL DEFAULT (datetime('now'))
        );

        -- ============================================================
        -- FARMING CALCULATOR TABLES
        -- ============================================================

        -- Farming session summary
        CREATE TABLE farming_sessions (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            name TEXT NOT NULL DEFAULT 'Farming Session',
            notes TEXT NOT NULL DEFAULT '',
            start_time TEXT NOT NULL,
            end_time TEXT,
            elapsed_seconds INTEGER NOT NULL DEFAULT 0,
            total_paused_seconds INTEGER NOT NULL DEFAULT 0,
            vendor_gold INTEGER NOT NULL DEFAULT 0,
            created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
        );
        CREATE INDEX idx_farming_sessions_created ON farming_sessions(created_at DESC);

        -- XP gains per skill per session
        CREATE TABLE farming_session_skills (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            session_id INTEGER NOT NULL,
            skill_id INTEGER NOT NULL,
            skill_name TEXT NOT NULL,
            xp_gained INTEGER NOT NULL DEFAULT 0,
            levels_gained INTEGER NOT NULL DEFAULT 0,
            FOREIGN KEY (session_id) REFERENCES farming_sessions(id) ON DELETE CASCADE
        );
        CREATE INDEX idx_farming_skills_session ON farming_session_skills(session_id);

        -- Net item changes per session
        CREATE TABLE farming_session_items (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            session_id INTEGER NOT NULL,
            item_name TEXT NOT NULL,
            net_quantity INTEGER NOT NULL DEFAULT 0,
            FOREIGN KEY (session_id) REFERENCES farming_sessions(id) ON DELETE CASCADE
        );
        CREATE INDEX idx_farming_items_session ON farming_session_items(session_id);

        -- Favor changes per session
        CREATE TABLE farming_session_favors (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            session_id INTEGER NOT NULL,
            npc_key TEXT NOT NULL,
            npc_name TEXT NOT NULL,
            delta REAL NOT NULL DEFAULT 0,
            FOREIGN KEY (session_id) REFERENCES farming_sessions(id) ON DELETE CASCADE
        );
        CREATE INDEX idx_farming_favors_session ON farming_session_favors(session_id);

        -- ============================================================
        -- CRAFTING HELPER TABLES
        -- ============================================================

        -- Saved crafting projects
        CREATE TABLE crafting_projects (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            name TEXT NOT NULL,
            notes TEXT NOT NULL DEFAULT '',
            created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
            updated_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
        );
        CREATE INDEX idx_crafting_projects_updated ON crafting_projects(updated_at DESC);

        -- Recipes within a project
        CREATE TABLE crafting_project_entries (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            project_id INTEGER NOT NULL,
            recipe_id INTEGER NOT NULL,
            recipe_name TEXT NOT NULL,
            quantity INTEGER NOT NULL DEFAULT 1,
            sort_order INTEGER NOT NULL DEFAULT 0,
            expanded_ingredient_ids TEXT NOT NULL DEFAULT '[]',
            FOREIGN KEY (project_id) REFERENCES crafting_projects(id) ON DELETE CASCADE,
            FOREIGN KEY (recipe_id) REFERENCES recipes(id) ON DELETE CASCADE
        );
        CREATE INDEX idx_cpe_project ON crafting_project_entries(project_id);

        -- ============================================================
        -- GAME STATE TABLES (last-known-value, per character+server)
        -- ============================================================

        -- Known servers
        CREATE TABLE servers (
            server_name TEXT PRIMARY KEY,
            display_name TEXT,
            created_at TEXT NOT NULL DEFAULT (datetime('now'))
        );

        -- Session singleton: tracks active character and last login
        CREATE TABLE game_state_session (
            id INTEGER PRIMARY KEY CHECK (id = 1),
            character_name TEXT NOT NULL,
            server_name TEXT NOT NULL,
            last_login_at TEXT,
            updated_at TEXT NOT NULL DEFAULT (datetime('now'))
        );

        -- Skills: one row per skill per character+server
        CREATE TABLE game_state_skills (
            character_name TEXT NOT NULL,
            server_name TEXT NOT NULL,
            skill_id INTEGER NOT NULL,
            skill_name TEXT NOT NULL,
            level INTEGER NOT NULL,
            bonus_levels INTEGER NOT NULL DEFAULT 0,
            xp INTEGER NOT NULL DEFAULT 0,
            tnl INTEGER NOT NULL DEFAULT 0,
            max_level INTEGER NOT NULL DEFAULT 0,
            last_confirmed_at TEXT NOT NULL,
            source TEXT NOT NULL DEFAULT 'log',
            PRIMARY KEY (character_name, server_name, skill_id)
        );
        CREATE INDEX idx_gs_skills_char ON game_state_skills(character_name, server_name);

        -- Active combat skills: single row per character+server
        CREATE TABLE game_state_active_skills (
            character_name TEXT NOT NULL,
            server_name TEXT NOT NULL,
            skill1_id INTEGER NOT NULL,
            skill1_name TEXT NOT NULL,
            skill2_id INTEGER NOT NULL,
            skill2_name TEXT NOT NULL,
            last_confirmed_at TEXT NOT NULL,
            PRIMARY KEY (character_name, server_name)
        );

        -- Attributes: one row per attribute per character+server
        CREATE TABLE game_state_attributes (
            character_name TEXT NOT NULL,
            server_name TEXT NOT NULL,
            attribute_name TEXT NOT NULL,
            value REAL NOT NULL,
            last_confirmed_at TEXT NOT NULL,
            PRIMARY KEY (character_name, server_name, attribute_name)
        );
        CREATE INDEX idx_gs_attributes_char ON game_state_attributes(character_name, server_name);

        -- Weather: singleton (world state, not per-character)
        CREATE TABLE game_state_weather (
            id INTEGER PRIMARY KEY CHECK (id = 1),
            weather_name TEXT NOT NULL,
            is_active INTEGER NOT NULL DEFAULT 1,
            last_confirmed_at TEXT NOT NULL
        );

        -- Combat state: single row per character+server
        CREATE TABLE game_state_combat (
            character_name TEXT NOT NULL,
            server_name TEXT NOT NULL,
            in_combat INTEGER NOT NULL DEFAULT 0,
            last_confirmed_at TEXT NOT NULL,
            PRIMARY KEY (character_name, server_name)
        );

        -- Mount state: single row per character+server
        CREATE TABLE game_state_mount (
            character_name TEXT NOT NULL,
            server_name TEXT NOT NULL,
            is_mounted INTEGER NOT NULL DEFAULT 0,
            last_confirmed_at TEXT NOT NULL,
            PRIMARY KEY (character_name, server_name)
        );

        -- Inventory: one row per item instance per character+server
        CREATE TABLE game_state_inventory (
            character_name TEXT NOT NULL,
            server_name TEXT NOT NULL,
            instance_id INTEGER NOT NULL,
            item_name TEXT NOT NULL,
            item_type_id INTEGER,
            stack_size INTEGER NOT NULL DEFAULT 1,
            slot_index INTEGER NOT NULL DEFAULT -1,
            last_confirmed_at TEXT NOT NULL,
            source TEXT NOT NULL DEFAULT 'log',
            PRIMARY KEY (character_name, server_name, instance_id)
        );
        CREATE INDEX idx_gs_inventory_char ON game_state_inventory(character_name, server_name);
        CREATE INDEX idx_gs_inventory_item ON game_state_inventory(item_name);

        -- Recipes: one row per recipe per character+server (completion count)
        CREATE TABLE game_state_recipes (
            character_name TEXT NOT NULL,
            server_name TEXT NOT NULL,
            recipe_id INTEGER NOT NULL,
            completion_count INTEGER NOT NULL DEFAULT 0,
            last_confirmed_at TEXT NOT NULL,
            source TEXT NOT NULL DEFAULT 'log',
            PRIMARY KEY (character_name, server_name, recipe_id)
        );
        CREATE INDEX idx_gs_recipes_char ON game_state_recipes(character_name, server_name);

        -- Equipment: one row per slot per character+server
        CREATE TABLE game_state_equipment (
            character_name TEXT NOT NULL,
            server_name TEXT NOT NULL,
            slot TEXT NOT NULL,
            appearance_key TEXT NOT NULL,
            last_confirmed_at TEXT NOT NULL,
            PRIMARY KEY (character_name, server_name, slot)
        );
        CREATE INDEX idx_gs_equipment_char ON game_state_equipment(character_name, server_name);

        -- NPC Favor: cumulative deltas from log + tier from snapshots
        CREATE TABLE game_state_favor (
            character_name TEXT NOT NULL,
            server_name TEXT NOT NULL,
            npc_key TEXT NOT NULL,
            npc_name TEXT NOT NULL,
            cumulative_delta REAL NOT NULL DEFAULT 0,
            favor_tier TEXT,
            last_confirmed_at TEXT NOT NULL,
            source TEXT NOT NULL DEFAULT 'log',
            PRIMARY KEY (character_name, server_name, npc_key)
        );
        CREATE INDEX idx_gs_favor_char ON game_state_favor(character_name, server_name);

        -- Currencies: snapshot-only currencies (Gold, Councils, etc.)
        CREATE TABLE game_state_currencies (
            character_name TEXT NOT NULL,
            server_name TEXT NOT NULL,
            currency_name TEXT NOT NULL,
            amount REAL NOT NULL DEFAULT 0,
            last_confirmed_at TEXT NOT NULL,
            source TEXT NOT NULL DEFAULT 'snapshot',
            PRIMARY KEY (character_name, server_name, currency_name)
        );
        CREATE INDEX idx_gs_currencies_char ON game_state_currencies(character_name, server_name);

        -- Active effects/buffs
        CREATE TABLE game_state_effects (
            character_name TEXT NOT NULL,
            server_name TEXT NOT NULL,
            effect_instance_id INTEGER NOT NULL,
            effect_name TEXT,
            source_entity_id INTEGER NOT NULL DEFAULT 0,
            last_confirmed_at TEXT NOT NULL,
            PRIMARY KEY (character_name, server_name, effect_instance_id)
        );
        CREATE INDEX idx_gs_effects_char ON game_state_effects(character_name, server_name);

        -- Storage vault contents: one row per item per vault per character
        CREATE TABLE game_state_storage (
            character_name TEXT NOT NULL,
            server_name TEXT NOT NULL,
            vault_key TEXT NOT NULL,
            instance_id INTEGER NOT NULL,
            item_name TEXT NOT NULL,
            item_type_id INTEGER,
            stack_size INTEGER NOT NULL DEFAULT 1,
            slot_index INTEGER NOT NULL DEFAULT -1,
            last_confirmed_at TEXT NOT NULL,
            source TEXT NOT NULL DEFAULT 'log',
            PRIMARY KEY (character_name, server_name, vault_key, instance_id)
        );
        CREATE INDEX idx_gs_storage_char ON game_state_storage(character_name, server_name);
        CREATE INDEX idx_gs_storage_vault ON game_state_storage(vault_key);

        -- Tracked skills: player-curated list of skills to watch closely
        CREATE TABLE tracked_skills (
            character_name TEXT NOT NULL,
            server_name TEXT NOT NULL,
            skill_name TEXT NOT NULL,
            sort_order INTEGER NOT NULL DEFAULT 0,
            PRIMARY KEY (character_name, server_name, skill_name)
        );
        "
    )?;

    Ok(())
}

/// Migration V17: Gift log — tracks individual gift events for weekly gift-limit tracking.
fn migration_v17_gift_log(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "CREATE TABLE game_state_gift_log (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            character_name TEXT NOT NULL,
            server_name TEXT NOT NULL,
            npc_key TEXT NOT NULL,
            npc_name TEXT NOT NULL,
            gifted_at TEXT NOT NULL,
            favor_delta REAL NOT NULL
        );
        CREATE INDEX idx_gs_gift_log_char ON game_state_gift_log(character_name, server_name);
        CREATE INDEX idx_gs_gift_log_npc_week ON game_state_gift_log(character_name, server_name, npc_key, gifted_at);
        "
    )?;

    Ok(())
}

/// Migration V18: Fix all timestamps that were stored with incorrect timezone handling.
///
/// Two bugs existed:
///
/// 1) Player.log timestamps are UTC but were treated as local time. The old code did:
///      stored = actual_utc - tz_offset_seconds
///    For UTC-7 (offset=-25200): stored = utc + 25200 (7 hours too late).
///    Fix: correct = stored + offset  (player_modifier)
///
/// 2) Chat.log timestamps are local time but were stored as-is (treated as UTC).
///    For UTC-7 (offset=-25200): stored = local = utc + (-offset) = utc - 25200
fn migration_v18_fix_timestamps(conn: &Connection, tz_offset_seconds: Option<i32>) -> Result<bool> {
    let offset = match tz_offset_seconds {
        Some(o) if o != 0 => o,
        _ => {
            // No offset known or offset is zero — nothing to fix; defer
            // record_migration(18) so the fix still runs once a real offset
            // is available (caller records only when this returns true).
            return Ok(false);
        }
    };

    // SQLite datetime() accepts a modifier like '+3600 seconds' or '-3600 seconds'
    let player_modifier = format!("{} seconds", offset);
    let chat_modifier = format!("{} seconds", -offset);

    // --- Player.log-derived timestamps (need +offset correction) ---

    // Tables with last_confirmed_at columns
    let last_confirmed_tables = [
        "game_state_skills",
        "game_state_active_skills",
        "game_state_attributes",
        "game_state_weather",
        "game_state_combat",
        "game_state_mount",
        "game_state_inventory",
        "game_state_equipment",
        "game_state_effects",
        "game_state_storage",
        "game_state_recipes",
        "game_state_favor",
    ];

    for table in &last_confirmed_tables {
        conn.execute(
            &format!(
                "UPDATE {} SET last_confirmed_at = datetime(last_confirmed_at, ?1)
                 WHERE last_confirmed_at IS NOT NULL",
                table
            ),
            [&player_modifier],
        )?;
    }

    // game_state_gift_log: gifted_at column
    conn.execute(
        "UPDATE game_state_gift_log SET gifted_at = datetime(gifted_at, ?1)
         WHERE gifted_at IS NOT NULL",
        [&player_modifier],
    )?;

    // item_transactions: Player.log-sourced rows
    conn.execute(
        "UPDATE item_transactions SET timestamp = datetime(timestamp, ?1)
         WHERE timestamp IS NOT NULL AND source != 'chat_status'",
        [&player_modifier],
    )?;

    // survey_session_stats: start_time and end_time
    conn.execute(
        "UPDATE survey_session_stats SET
            start_time = datetime(start_time, ?1),
            end_time = CASE WHEN end_time IS NOT NULL THEN datetime(end_time, ?1) ELSE NULL END
         WHERE start_time IS NOT NULL",
        [&player_modifier],
    )?;

    // survey_events: timestamp
    conn.execute(
        "UPDATE survey_events SET timestamp = datetime(timestamp, ?1)
         WHERE timestamp IS NOT NULL",
        [&player_modifier],
    )?;

    // --- Chat.log-derived timestamps (need -offset correction) ---

    // chat_messages: timestamp column
    conn.execute(
        "UPDATE chat_messages SET timestamp = datetime(timestamp, ?1)
         WHERE timestamp IS NOT NULL",
        [&chat_modifier],
    )?;

    // item_transactions: chat_status-sourced rows
    conn.execute(
        "UPDATE item_transactions SET timestamp = datetime(timestamp, ?1)
         WHERE timestamp IS NOT NULL AND source = 'chat_status'",
        [&chat_modifier],
    )?;

    // character_deaths: died_at (from chat combat events)
    conn.execute(
        "UPDATE character_deaths SET died_at = datetime(died_at, ?1)
         WHERE died_at IS NOT NULL",
        [&chat_modifier],
    )?;

    // death_damage_sources: timestamp (from chat combat events)
    conn.execute(
        "UPDATE death_damage_sources SET timestamp = datetime(timestamp, ?1)
         WHERE timestamp IS NOT NULL",
        [&chat_modifier],
    )?;

    eprintln!(
        "[migration_v18] Fixed timestamps: Player.log correction={}, Chat.log correction={}",
        player_modifier, chat_modifier
    );

    Ok(true)
}

/// Migration V19: Price Helper — persistent crafting price quotes.
fn migration_v19_price_helper(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "CREATE TABLE price_helper_quotes (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            name TEXT NOT NULL,
            notes TEXT NOT NULL DEFAULT '',
            fee_config TEXT NOT NULL DEFAULT '{\"per_craft_fee\":0,\"material_pct\":0,\"material_pct_basis\":\"total\",\"flat_fee\":0}',
            customer_provides TEXT NOT NULL DEFAULT '{}',
            created_at TEXT NOT NULL DEFAULT (datetime('now')),
            updated_at TEXT NOT NULL DEFAULT (datetime('now'))
        );

        CREATE TABLE price_helper_entries (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            quote_id INTEGER NOT NULL REFERENCES price_helper_quotes(id) ON DELETE CASCADE,
            recipe_id INTEGER NOT NULL,
            recipe_name TEXT NOT NULL,
            quantity INTEGER NOT NULL DEFAULT 1,
            sort_order INTEGER NOT NULL DEFAULT 0
        );

        CREATE INDEX idx_price_helper_entries_quote ON price_helper_entries(quote_id);
        "
    )?;

    Ok(())
}

/// Migration V20: Add pricing fields to crafting projects (integrated price helper).
fn migration_v20_project_pricing(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "ALTER TABLE crafting_projects ADD COLUMN fee_config TEXT NOT NULL DEFAULT '{\"per_craft_fee\":0,\"material_pct\":0,\"material_pct_basis\":\"total\",\"flat_fee\":0}';
         ALTER TABLE crafting_projects ADD COLUMN customer_provides TEXT NOT NULL DEFAULT '{}';"
    )?;

    Ok(())
}

/// Migration V16: Survey data sharing — import tracking table + import_id on sessions.
fn migration_v16_survey_data_imports(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "CREATE TABLE survey_data_imports (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            label TEXT NOT NULL,
            source_player TEXT,
            session_count INTEGER NOT NULL DEFAULT 0,
            event_count INTEGER NOT NULL DEFAULT 0,
            imported_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
        );

        ALTER TABLE survey_session_stats ADD COLUMN import_id INTEGER
            REFERENCES survey_data_imports(id) ON DELETE CASCADE;

        CREATE INDEX idx_survey_session_stats_import ON survey_session_stats(import_id);
        ",
    )?;

    Ok(())
}

/// Migration V31: Recreate abilities table with the full current schema.
/// The V1 schema was edited after some databases were created, so older databases
/// may be missing columns (internal_name, upgrade_of, etc.). Since abilities is
/// pure CDN data (repopulated on every CDN load), DROP + CREATE is safe and
/// guarantees all columns exist.
fn migration_v31_abilities_internal_name(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "DROP TABLE IF EXISTS abilities;
         CREATE TABLE abilities (
             id INTEGER PRIMARY KEY,
             name TEXT NOT NULL,
             internal_name TEXT,
             description TEXT,
             icon_id INTEGER,
             skill TEXT,
             level_req REAL,
             keywords TEXT,
             damage_type TEXT,
             reset_time REAL,
             target TEXT,
             prerequisite TEXT,
             upgrade_of TEXT,
             is_harmless BOOLEAN,
             animation TEXT,
             special_info TEXT,
             works_underwater BOOLEAN,
             works_while_falling BOOLEAN,
             pve TEXT,
             pvp TEXT,
             mana_cost INTEGER,
             power_cost INTEGER,
             armor_cost INTEGER,
             health_cost INTEGER,
             range REAL,
             raw_json TEXT NOT NULL
         );
         CREATE INDEX idx_abilities_name ON abilities(name COLLATE NOCASE);
         CREATE INDEX idx_abilities_skill ON abilities(skill);
         CREATE INDEX idx_abilities_damage_type ON abilities(damage_type);",
    )?;

    Ok(())
}

/// Migration V33: New game state tables for moon phase, guild, directed goals, and player strings.
fn migration_v33_new_game_state_tables(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS game_state_moon (
             id INTEGER PRIMARY KEY CHECK (id = 1),
             phase TEXT NOT NULL,
             last_confirmed_at TEXT NOT NULL
         );

         CREATE TABLE IF NOT EXISTS game_state_guild (
             character_name TEXT NOT NULL,
             server_name TEXT NOT NULL,
             guild_id INTEGER NOT NULL,
             guild_name TEXT NOT NULL,
             motd TEXT NOT NULL DEFAULT '',
             last_confirmed_at TEXT NOT NULL,
             PRIMARY KEY (character_name, server_name)
         );

         CREATE TABLE IF NOT EXISTS game_state_directed_goals (
             character_name TEXT NOT NULL,
             server_name TEXT NOT NULL,
             goal_id INTEGER NOT NULL,
             last_confirmed_at TEXT NOT NULL,
             PRIMARY KEY (character_name, server_name, goal_id)
         );

         CREATE TABLE IF NOT EXISTS game_state_strings (
             character_name TEXT NOT NULL,
             server_name TEXT NOT NULL,
             key TEXT NOT NULL,
             value TEXT NOT NULL,
             last_confirmed_at TEXT NOT NULL,
             PRIMARY KEY (character_name, server_name, key)
         );

         CREATE TABLE IF NOT EXISTS game_state_books (
             id INTEGER PRIMARY KEY AUTOINCREMENT,
             character_name TEXT NOT NULL,
             server_name TEXT NOT NULL,
             book_type TEXT NOT NULL,
             title TEXT NOT NULL,
             content TEXT NOT NULL,
             captured_at TEXT NOT NULL,
             UNIQUE(character_name, server_name, book_type, title)
         );",
    )?;
    Ok(())
}

/// Migration V34: Milking cooldown timers and character report stats.
fn migration_v34_milking_and_stats(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS milking_timers (
             character_name TEXT NOT NULL,
             server_name TEXT NOT NULL,
             cow_name TEXT NOT NULL,
             zone TEXT NOT NULL,
             last_milked_at TEXT NOT NULL,
             PRIMARY KEY (character_name, server_name, cow_name, zone)
         );

         CREATE TABLE IF NOT EXISTS character_report_stats (
             character_name TEXT NOT NULL,
             server_name TEXT NOT NULL,
             category TEXT NOT NULL,
             stat_name TEXT NOT NULL,
             stat_value TEXT NOT NULL,
             updated_at TEXT NOT NULL,
             PRIMARY KEY (character_name, server_name, category, stat_name)
         );",
    )?;
    Ok(())
}

/// Migration V36: Player-to-player milking log for leaderboards.
fn migration_v36_player_milking_log(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS player_milking_log (
             id INTEGER PRIMARY KEY AUTOINCREMENT,
             character_name TEXT NOT NULL,
             server_name TEXT NOT NULL,
             other_player TEXT NOT NULL,
             direction TEXT NOT NULL CHECK (direction IN ('milked', 'milked_by')),
             milked_at TEXT NOT NULL
         );
         CREATE INDEX idx_player_milking_char ON player_milking_log(character_name, server_name);
         CREATE INDEX idx_player_milking_other ON player_milking_log(character_name, server_name, other_player, direction);",
    )?;
    Ok(())
}

/// Migration V35: Add min/max tracking columns to game_state_attributes.
fn migration_v35_attribute_extremes(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "ALTER TABLE game_state_attributes ADD COLUMN min_value REAL;
         ALTER TABLE game_state_attributes ADD COLUMN max_value REAL;
         -- Seed min/max from current values for existing rows
         UPDATE game_state_attributes SET min_value = value, max_value = value
            WHERE min_value IS NULL;",
    )?;
    Ok(())
}

/// Migration V37: Words of Power — tracks discovered words for the dashboard widget.
fn migration_v37_words_of_power(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "CREATE TABLE words_of_power (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            character_name TEXT NOT NULL,
            server_name TEXT NOT NULL,
            word TEXT NOT NULL,
            power_name TEXT NOT NULL,
            description TEXT,
            discovered_at TEXT NOT NULL,
            source TEXT NOT NULL DEFAULT 'auto',
            created_at TEXT NOT NULL DEFAULT (datetime('now'))
        );
        CREATE INDEX idx_words_of_power_char_server ON words_of_power(character_name, server_name);",
    )?;
    Ok(())
}

fn migration_v32_brewing_discoveries(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS brewing_discoveries (
             id INTEGER PRIMARY KEY,
             character TEXT NOT NULL,
             recipe_id INTEGER NOT NULL,
             ingredient_ids TEXT NOT NULL,
             power TEXT NOT NULL,
             power_tier INTEGER NOT NULL,
             effect_label TEXT,
             race_restriction TEXT,
             item_name TEXT,
             first_seen_at TEXT NOT NULL,
             last_seen_at TEXT NOT NULL,
             UNIQUE(character, recipe_id, ingredient_ids)
         );
         CREATE INDEX IF NOT EXISTS idx_brewing_disc_char ON brewing_discoveries(character);
         CREATE INDEX IF NOT EXISTS idx_brewing_disc_recipe ON brewing_discoveries(recipe_id);",
    )?;

    Ok(())
}

/// Migration V38: Allow 'self_milked' direction in player_milking_log.
/// SQLite doesn't support ALTER CHECK constraints, so we recreate the table.
fn migration_v38_self_milking(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "CREATE TABLE player_milking_log_new (
             id INTEGER PRIMARY KEY AUTOINCREMENT,
             character_name TEXT NOT NULL,
             server_name TEXT NOT NULL,
             other_player TEXT NOT NULL,
             direction TEXT NOT NULL CHECK (direction IN ('milked', 'milked_by', 'self_milked')),
             milked_at TEXT NOT NULL
         );
         INSERT INTO player_milking_log_new SELECT * FROM player_milking_log;
         DROP TABLE player_milking_log;
         ALTER TABLE player_milking_log_new RENAME TO player_milking_log;
         CREATE INDEX idx_player_milking_char ON player_milking_log(character_name, server_name);
         CREATE INDEX idx_player_milking_other ON player_milking_log(character_name, server_name, other_player, direction);",
    )?;
    Ok(())
}

/// Migration V39: Teleportation bind locations parsed from SkillReport books.
/// Stores primary/secondary bind pad locations and mushroom circle attunements.
/// Migration V40: User timers — general-purpose countdown timers persisted per character.
/// Supports both manual timers (user-created) and auto-detected timers from game events.
fn migration_v40_user_timers(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "CREATE TABLE user_timers (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            character_name TEXT NOT NULL,
            server_name TEXT NOT NULL,
            label TEXT NOT NULL,
            duration_seconds INTEGER NOT NULL,
            started_at TEXT NOT NULL,
            paused_at TEXT,
            area_key TEXT,
            source TEXT NOT NULL DEFAULT 'manual'
        );
        CREATE INDEX idx_user_timers_char_server ON user_timers(character_name, server_name);",
    )?;
    Ok(())
}

fn migration_v41_garden_almanac(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS garden_almanac (
            character_name TEXT NOT NULL,
            server_name TEXT NOT NULL,
            crop_name TEXT NOT NULL,
            zone_name TEXT NOT NULL,
            event_start TEXT,
            event_end TEXT,
            is_current INTEGER NOT NULL DEFAULT 0,
            captured_at TEXT NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_garden_almanac_char_server
            ON garden_almanac(character_name, server_name);

        CREATE TABLE IF NOT EXISTS garden_almanac_history (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            character_name TEXT NOT NULL,
            server_name TEXT NOT NULL,
            crop_name TEXT NOT NULL,
            zone_name TEXT NOT NULL,
            event_start TEXT,
            event_end TEXT,
            captured_at TEXT NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_garden_almanac_history_char_server
            ON garden_almanac_history(character_name, server_name);
        CREATE INDEX IF NOT EXISTS idx_garden_almanac_history_crop
            ON garden_almanac_history(crop_name, zone_name);",
    )?;
    Ok(())
}

fn migration_v39_teleportation_binds(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS game_state_teleportation (
            character_name TEXT NOT NULL,
            server_name TEXT NOT NULL,
            primary_bind TEXT,
            secondary_bind TEXT,
            mushroom_circle_1 TEXT,
            mushroom_circle_2 TEXT,
            last_updated TEXT NOT NULL,
            PRIMARY KEY (character_name, server_name)
        );",
    )?;
    Ok(())
}

/// Migration V42: Player messages — tracks pigeon and stall-note messages.
fn migration_v42_player_messages(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS player_messages (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            character_name TEXT NOT NULL,
            server_name TEXT NOT NULL,
            timestamp TEXT NOT NULL,
            message_type TEXT NOT NULL,
            direction TEXT NOT NULL,
            other_player TEXT NOT NULL,
            body TEXT NOT NULL DEFAULT '',
            item_name TEXT,
            created_at TEXT NOT NULL DEFAULT (datetime('now'))
        );
        CREATE INDEX IF NOT EXISTS idx_player_messages_char_server
            ON player_messages(character_name, server_name, timestamp DESC);",
    )?;
    Ok(())
}

/// Migration V43: Hoplology studies tracking table.
fn migration_v43_hoplology_studies(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS hoplology_studies (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            character_name TEXT NOT NULL,
            server_name TEXT NOT NULL,
            item_name TEXT NOT NULL,
            studied_at TEXT NOT NULL,
            source TEXT NOT NULL DEFAULT 'auto',
            UNIQUE(character_name, server_name, item_name)
        );
        CREATE INDEX IF NOT EXISTS idx_hoplology_char_server
            ON hoplology_studies(character_name, server_name, studied_at DESC);",
    )?;
    Ok(())
}

/// Migration V44: Rebuild hoplology_studies to guarantee UNIQUE constraint
/// and clean up duplicate/stale rows from early development.
/// Migration v45: Kill tracking system.
/// - `enemy_kills` — every enemy kill observed from chat combat FATALITY lines
/// - `enemy_kill_loot` — items looted from a specific killed enemy (linked via entity_id matching)
/// - `farming_session_kills` — aggregated kill counts per enemy type per farming session
fn migration_v45_kill_tracking(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "CREATE TABLE enemy_kills (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            enemy_name TEXT NOT NULL,
            enemy_entity_id TEXT NOT NULL,
            killing_ability TEXT NOT NULL,
            health_damage INTEGER NOT NULL DEFAULT 0,
            armor_damage INTEGER NOT NULL DEFAULT 0,
            killed_at TEXT NOT NULL,
            character_name TEXT,
            server_name TEXT,
            created_at DATETIME DEFAULT CURRENT_TIMESTAMP
        );
        CREATE INDEX idx_enemy_kills_name ON enemy_kills(enemy_name);
        CREATE INDEX idx_enemy_kills_entity ON enemy_kills(enemy_entity_id);

        CREATE TABLE enemy_kill_loot (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            kill_id INTEGER NOT NULL,
            item_name TEXT NOT NULL,
            quantity INTEGER NOT NULL DEFAULT 1,
            FOREIGN KEY (kill_id) REFERENCES enemy_kills(id) ON DELETE CASCADE
        );
        CREATE INDEX idx_kill_loot_kill ON enemy_kill_loot(kill_id);

        CREATE TABLE farming_session_kills (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            session_id INTEGER NOT NULL,
            enemy_name TEXT NOT NULL,
            kill_count INTEGER NOT NULL DEFAULT 0,
            FOREIGN KEY (session_id) REFERENCES farming_sessions(id) ON DELETE CASCADE
        );
        CREATE INDEX idx_farming_kills_session ON farming_session_kills(session_id);",
    )?;
    Ok(())
}

fn migration_v44_hoplology_dedup(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "-- Rebuild table: keep best row per (character, server, item).
         -- Prefer source='chat' over 'report'/'auto'; among ties keep newest studied_at.
         CREATE TABLE hoplology_studies_new (
             id INTEGER PRIMARY KEY AUTOINCREMENT,
             character_name TEXT NOT NULL,
             server_name TEXT NOT NULL,
             item_name TEXT NOT NULL,
             studied_at TEXT NOT NULL,
             source TEXT NOT NULL DEFAULT 'auto',
             UNIQUE(character_name, server_name, item_name)
         );
         INSERT INTO hoplology_studies_new (character_name, server_name, item_name, studied_at, source)
             SELECT character_name, server_name, item_name, studied_at, source
             FROM (
                 SELECT character_name, server_name, item_name, studied_at, source,
                        ROW_NUMBER() OVER (
                            PARTITION BY character_name, server_name, item_name
                            ORDER BY CASE source WHEN 'chat' THEN 0 WHEN 'auto' THEN 1 ELSE 2 END,
                                     studied_at DESC
                        ) AS rn
                 FROM hoplology_studies
             )
             WHERE rn = 1;
         DROP TABLE hoplology_studies;
         ALTER TABLE hoplology_studies_new RENAME TO hoplology_studies;
         CREATE INDEX IF NOT EXISTS idx_hoplology_char_server
             ON hoplology_studies(character_name, server_name, studied_at DESC);"
    )?;
    Ok(())
}

/// Migration V46: Add `manually_marked` column to `gourmand_eaten_foods`.
///
/// This column was added to the v1 schema definition (for fresh installs) but
/// no incremental migration was created, so existing databases from before that
/// edit are missing it. All existing rows get `manually_marked = 0` (imported
/// from report, not manually toggled).
fn migration_v46_gourmand_manually_marked(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "ALTER TABLE gourmand_eaten_foods ADD COLUMN manually_marked INTEGER NOT NULL DEFAULT 0;",
    )?;
    Ok(())
}

/// Migration v47: Imported kill/loot drop-rate data, for the community
/// database sharing feature. Stored separately from the player's own
/// `enemy_kills`/`enemy_kill_loot` (ground truth) so "mine" / "imported" /
/// "combined" views can be computed without ever mutating personal data.
/// Each import is tagged with a `source_label` (the imported file's name);
/// re-importing the same label replaces just that source's rows, so repeat
/// imports of the same file never double-count.
fn migration_v47_imported_kill_loot_data(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "CREATE TABLE imported_kill_sources (
            source_label TEXT PRIMARY KEY,
            display_name TEXT NOT NULL,
            imported_at DATETIME DEFAULT CURRENT_TIMESTAMP
        );

        CREATE TABLE imported_enemy_kills_agg (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            source_label TEXT NOT NULL,
            enemy_name TEXT NOT NULL,
            total_kills INTEGER NOT NULL DEFAULT 0,
            FOREIGN KEY (source_label) REFERENCES imported_kill_sources(source_label) ON DELETE CASCADE
        );
        CREATE INDEX idx_imported_kills_source ON imported_enemy_kills_agg(source_label);
        CREATE INDEX idx_imported_kills_enemy ON imported_enemy_kills_agg(enemy_name);

        CREATE TABLE imported_enemy_kill_loot_agg (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            source_label TEXT NOT NULL,
            enemy_name TEXT NOT NULL,
            item_name TEXT NOT NULL,
            total_quantity INTEGER NOT NULL DEFAULT 0,
            times_dropped INTEGER NOT NULL DEFAULT 0,
            FOREIGN KEY (source_label) REFERENCES imported_kill_sources(source_label) ON DELETE CASCADE
        );
        CREATE INDEX idx_imported_loot_source ON imported_enemy_kill_loot_agg(source_label);
        CREATE INDEX idx_imported_loot_enemy ON imported_enemy_kill_loot_agg(enemy_name);
        CREATE INDEX idx_imported_loot_item ON imported_enemy_kill_loot_agg(item_name);"
    )?;
    Ok(())
}

/// Migration v48: Deduplication support for the kill/loot database, enabling
/// historical backfill (e.g. ingesting Player-prev.log on rotation) to coexist
/// with live tailing without double-counting.
///
/// - A UNIQUE index on the natural key of a kill lets both the live tailer and
///   the historical ingester use `INSERT OR IGNORE` — the same real kill
///   (identical character/server/entity/timestamp from the same Chat.log
///   FATALITY line) is written at most once regardless of source.
/// - `player_prev_ingests` records the content hash of each backed-up log so a
///   given rotation is never re-processed.
///
/// De-dupes any pre-existing duplicate kill rows first so the UNIQUE index can
/// be created (live tailing shouldn't produce exact dups, but be safe).
fn migration_v48_kill_dedup_and_ingest_tracking(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "DELETE FROM enemy_kills
         WHERE id NOT IN (
             SELECT MIN(id) FROM enemy_kills
             GROUP BY character_name, server_name, enemy_entity_id, killed_at
         );

         CREATE UNIQUE INDEX idx_enemy_kills_dedup
             ON enemy_kills(character_name, server_name, enemy_entity_id, killed_at);

         CREATE TABLE player_prev_ingests (
             content_hash TEXT PRIMARY KEY,
             source_path TEXT,
             ingested_at DATETIME DEFAULT CURRENT_TIMESTAMP,
             kills_added INTEGER NOT NULL DEFAULT 0,
             loot_added INTEGER NOT NULL DEFAULT 0
         );",
    )?;
    Ok(())
}

/// Migration v49: pivot the drop-rate model from chat-FATALITY kills to
/// Player.log corpse searches.
///
/// The old model attributed loot by matching a chat-FATALITY kill's entity_id
/// against the Player.log corpse entity_id; those IDs don't match, so loot was
/// almost never attributed and the DB sat near-empty. The new model records one
/// kill row per *lootable* searched corpse (Player.log `Search Corpse of X`)
/// keyed by the corpse entity_id, which matches the loot's corpse entity_id —
/// so attribution is reliable.
///
/// - Wipes the old (near-empty, mis-attributed) `enemy_kills`/`enemy_kill_loot`
///   rows and the `player_prev_ingests` log so backfills re-run under the new
///   model.
/// - Adds `enemy_kill_loot.instance_id` (the item's signed-32-bit instance id,
///   stored bit-preserved as INTEGER) and a UNIQUE index on
///   `(kill_id, instance_id)`. Each looted item instance is unique, so two
///   separate single-stacks of the same item off one corpse stay as two rows —
///   `INSERT OR IGNORE` only collapses a genuine re-read of the same instance.
fn migration_v49_corpse_search_loot_model(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "DELETE FROM enemy_kill_loot;
         DELETE FROM enemy_kills;
         DELETE FROM player_prev_ingests;

         ALTER TABLE enemy_kill_loot ADD COLUMN instance_id INTEGER;

         CREATE UNIQUE INDEX idx_kill_loot_dedup
             ON enemy_kill_loot(kill_id, instance_id);",
    )?;
    Ok(())
}

/// Migration v50: per-extract butchering/skinning detail.
///
/// Records one row per skinning/butchering yield with the conditions at harvest
/// time — the player's Butchering/Skinning level, the equipment skill bonus, and
/// the anatomy family + level for that monster type — all parsed from Player.log
/// (`ProcessUpdateSkill` + the corpse body's "(with a +N skill bonus from
/// equipment)"). Surfaced in the farming item hover tooltips. Separate from the
/// loot-drop tables since extracts produce no `ProcessRemoveLoot`.
fn migration_v50_corpse_extracts(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "CREATE TABLE corpse_extracts (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            character_name TEXT,
            server_name TEXT,
            corpse_name TEXT,
            item_name TEXT NOT NULL,
            quantity INTEGER NOT NULL DEFAULT 1,
            skill TEXT NOT NULL,
            skill_level INTEGER,
            equipment_bonus INTEGER,
            anatomy_family TEXT,
            anatomy_level INTEGER,
            extracted_at TEXT,
            created_at DATETIME DEFAULT CURRENT_TIMESTAMP
        );
        CREATE INDEX idx_corpse_extracts_item ON corpse_extracts(item_name);
        CREATE INDEX idx_corpse_extracts_corpse ON corpse_extracts(corpse_name);",
    )?;
    Ok(())
}

/// Migration V63: make a cow's milking timer identity per-cow, not per-cow-per-zone.
///
/// A cow's 1-hour milk cooldown is a property of the NPC itself — the game's
/// cooldown message ("You've already milked Homer in the past hour.") carries no
/// zone. The original schema keyed `milking_timers` on
/// `(character, server, cow_name, zone)`, so the *same* cow recorded once with a
/// known area and once while `current_area` was still unknown (zone = '') landed
/// in two rows. The widget then showed that cow twice — a live countdown under
/// its real zone plus a phantom "Ready!" (or vice-versa) under a blank zone.
///
/// Drop `zone` from the primary key so each cow occupies a single row. Existing
/// duplicates are merged: keep the most recent `last_milked_at`, and adopt the
/// most recent *non-empty* zone as the grouping label (falling back to '' only
/// when every recorded row had an unknown area). The write path (see
/// `record_cow_milk` / `record_cow_milk_backfill`) is updated to conflict on the
/// new key and to never overwrite a good zone with a blank one.
fn migration_v63_milking_timers_per_cow(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "CREATE TABLE milking_timers_new (
             character_name TEXT NOT NULL,
             server_name TEXT NOT NULL,
             cow_name TEXT NOT NULL,
             zone TEXT NOT NULL,
             last_milked_at TEXT NOT NULL,
             PRIMARY KEY (character_name, server_name, cow_name)
         );

         INSERT INTO milking_timers_new
             (character_name, server_name, cow_name, zone, last_milked_at)
         SELECT m.character_name, m.server_name, m.cow_name,
                COALESCE(
                    (SELECT z.zone FROM milking_timers z
                      WHERE z.character_name = m.character_name
                        AND z.server_name = m.server_name
                        AND z.cow_name = m.cow_name
                        AND z.zone != ''
                      ORDER BY z.last_milked_at DESC
                      LIMIT 1),
                    ''
                ) AS zone,
                MAX(m.last_milked_at) AS last_milked_at
         FROM milking_timers m
         GROUP BY m.character_name, m.server_name, m.cow_name;

         DROP TABLE milking_timers;
         ALTER TABLE milking_timers_new RENAME TO milking_timers;",
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    /// Recreate the original chat_messages + external-content FTS setup with the
    /// *old, buggy* delete trigger, so we can prove v57 repairs it.
    fn setup_legacy_chat_fts() -> Connection {
        let c = Connection::open_in_memory().unwrap();
        c.execute_batch(
            "CREATE TABLE chat_messages (
                 id INTEGER PRIMARY KEY AUTOINCREMENT,
                 message TEXT NOT NULL,
                 sender TEXT
             );
             CREATE VIRTUAL TABLE chat_messages_fts USING fts5(
                 message, sender, content=chat_messages, content_rowid=id
             );
             CREATE TRIGGER chat_messages_fts_insert AFTER INSERT ON chat_messages BEGIN
                 INSERT INTO chat_messages_fts(rowid, message, sender)
                 VALUES (new.id, new.message, new.sender);
             END;
             -- the buggy idiom v57 replaces
             CREATE TRIGGER chat_messages_fts_delete AFTER DELETE ON chat_messages BEGIN
                 DELETE FROM chat_messages_fts WHERE rowid = old.id;
             END;",
        )
        .unwrap();
        c
    }

    #[test]
    fn v57_delete_trigger_removes_fts_and_docsize_entries() {
        let c = setup_legacy_chat_fts();
        c.execute(
            "INSERT INTO chat_messages(message, sender) VALUES ('hello world', 'alice')",
            [],
        )
        .unwrap();
        c.execute(
            "INSERT INTO chat_messages(message, sender) VALUES ('goodbye world', 'bob')",
            [],
        )
        .unwrap();

        migration_v57_fix_chat_fts_delete_triggers(&c).unwrap();

        // Delete one message; the corrected trigger must purge it from the index.
        c.execute("DELETE FROM chat_messages WHERE sender = 'alice'", [])
            .unwrap();

        // No orphan left behind in the docsize shadow table.
        let docsize: i64 = c
            .query_row("SELECT COUNT(*) FROM chat_messages_fts_docsize", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(docsize, 1, "deleted row must not leave a docsize orphan");

        // The deleted message is no longer searchable; the survivor still is.
        let hits_hello: i64 = c
            .query_row(
                "SELECT COUNT(*) FROM chat_messages_fts WHERE chat_messages_fts MATCH 'hello'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(hits_hello, 0);
        let hits_goodbye: i64 = c
            .query_row(
                "SELECT COUNT(*) FROM chat_messages_fts WHERE chat_messages_fts MATCH 'goodbye'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(hits_goodbye, 1);
    }

    /// Recreate the original `milking_timers` schema (zone as a PK part) so we
    /// can prove v63 collapses the per-zone duplicate rows into one per cow.
    /// The widget reads via `get_harvest_almanac`, whose SELECT must line up with
    /// the columns v66 creates — a mismatch only shows up at runtime otherwise.
    #[test]
    fn v66_harvest_almanac_round_trips() {
        let c = Connection::open_in_memory().unwrap();
        migration_v66_harvest_almanac(&c).unwrap();

        c.execute_batch(
            "INSERT INTO harvest_almanac
                 (character_name, server_name, monster_name, description, event_start, event_end, is_current, captured_at)
             VALUES
                 ('oddio','Arisetsu','Skeleton Swordsmen','Seek out skeletal swordsmen.',NULL,'2026-07-29T03:59:26+00:00',1,'2026-07-28T05:57:26+00:00'),
                 ('oddio','Arisetsu','Rock Elementals',NULL,'2026-07-29T03:00:00+00:00',NULL,0,'2026-07-28T05:57:26+00:00'),
                 ('someone-else','Arisetsu','Wolves',NULL,NULL,NULL,1,'2026-07-28T05:57:26+00:00');",
        )
        .unwrap();

        // Mirrors get_harvest_almanac's query exactly.
        let mut stmt = c
            .prepare(
                "SELECT monster_name, description, event_start, event_end, is_current, captured_at
                 FROM harvest_almanac
                 WHERE character_name = ?1 AND server_name = ?2
                 ORDER BY is_current DESC, event_start ASC",
            )
            .unwrap();
        let rows: Vec<(String, Option<String>, bool)> = stmt
            .query_map(rusqlite::params!["oddio", "Arisetsu"], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get::<_, i32>(4)? != 0))
            })
            .unwrap()
            .map(|r| r.unwrap())
            .collect();

        // Scoped to the character, current focus first.
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].0, "Skeleton Swordsmen");
        assert!(rows[0].2);
        assert_eq!(rows[0].1.as_deref(), Some("Seek out skeletal swordsmen."));
        assert_eq!(rows[1].0, "Rock Elementals");
        assert!(!rows[1].2);
        assert_eq!(rows[1].1, None);
    }

    /// v67 has to leave existing food rows readable (so the Gourmand screen
    /// still works before the CDN rebuild lands) *and* clear `cdn_version`, or
    /// the startup persist short-circuits and the new columns stay empty.
    #[test]
    fn v67_defaults_food_sources_and_forces_a_cdn_rebuild() {
        let c = Connection::open_in_memory().unwrap();
        c.execute_batch(
            "CREATE TABLE foods (
                 item_id     INTEGER PRIMARY KEY,
                 name        TEXT NOT NULL,
                 icon_id     INTEGER,
                 food_category TEXT NOT NULL,
                 food_level  INTEGER NOT NULL,
                 gourmand_req INTEGER,
                 effect_descs TEXT NOT NULL,
                 keywords    TEXT NOT NULL,
                 value       REAL
             );
             CREATE TABLE cdn_version (
                 id INTEGER PRIMARY KEY CHECK (id = 1),
                 version INTEGER NOT NULL,
                 loaded_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
             );
             INSERT INTO foods VALUES (6090,'Basic Pumpkin Pie',5763,'Meal',50,15,'[]','[]',180);
             INSERT INTO cdn_version (id, version) VALUES (1, 412);",
        )
        .unwrap();

        migration_v67_food_sources(&c).unwrap();

        let (kinds, skills): (String, String) = c
            .query_row(
                "SELECT source_kinds, craft_skills FROM foods WHERE item_id = 6090",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(kinds, "[]");
        assert_eq!(skills, "[]");

        let versions: i64 = c
            .query_row("SELECT COUNT(*) FROM cdn_version", [], |r| r.get(0))
            .unwrap();
        assert_eq!(
            versions, 0,
            "cdn_version must be cleared to force a repersist"
        );
    }

    /// v70 repairs the token drift where CDN 486 writes `'Instant Snack'` but
    /// the Gourmand screen matches `'Instant-Snack'`. Untouched categories — the
    /// already-canonical ones and anything unknown — must survive verbatim.
    #[test]
    fn v70_canonicalises_instant_snack_categories() {
        let c = Connection::open_in_memory().unwrap();
        c.execute_batch(
            "CREATE TABLE foods (
                 item_id     INTEGER PRIMARY KEY,
                 name        TEXT NOT NULL,
                 icon_id     INTEGER,
                 food_category TEXT NOT NULL,
                 food_level  INTEGER NOT NULL,
                 gourmand_req INTEGER,
                 effect_descs TEXT NOT NULL,
                 keywords    TEXT NOT NULL,
                 value       REAL
             );
             INSERT INTO foods VALUES (1,'Honey',1,'Instant Snack',5,NULL,'[]','[]',10);
             INSERT INTO foods VALUES (2,'Large Strawberry',2,'instant snack',5,NULL,'[]','[]',10);
             INSERT INTO foods VALUES (3,'Cleaned Shrimp',3,'Instant-Snack',5,NULL,'[]','[]',10);
             INSERT INTO foods VALUES (4,'Basic Pumpkin Pie',4,'Meal',50,15,'[]','[]',180);
             INSERT INTO foods VALUES (5,'Mystery Brunch',5,'Brunch',1,NULL,'[]','[]',5);",
        )
        .unwrap();

        migration_v70_canonical_food_categories(&c).unwrap();

        let category = |id: i64| -> String {
            c.query_row(
                "SELECT food_category FROM foods WHERE item_id = ?1",
                [id],
                |r| r.get(0),
            )
            .unwrap()
        };
        assert_eq!(category(1), "Instant-Snack");
        assert_eq!(
            category(2),
            "Instant-Snack",
            "case-insensitive, separator-agnostic"
        );
        assert_eq!(category(3), "Instant-Snack");
        assert_eq!(category(4), "Meal");
        assert_eq!(category(5), "Brunch", "unknown categories stay as-is");

        // Idempotent: a second run changes nothing.
        migration_v70_canonical_food_categories(&c).unwrap();
        let changed: i64 = c
            .query_row(
                "SELECT COUNT(*) FROM foods WHERE food_category = 'Instant-Snack'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(changed, 3);
    }

    fn setup_legacy_milking_timers() -> Connection {
        let c = Connection::open_in_memory().unwrap();
        c.execute_batch(
            "CREATE TABLE milking_timers (
                 character_name TEXT NOT NULL,
                 server_name TEXT NOT NULL,
                 cow_name TEXT NOT NULL,
                 zone TEXT NOT NULL,
                 last_milked_at TEXT NOT NULL,
                 PRIMARY KEY (character_name, server_name, cow_name, zone)
             );",
        )
        .unwrap();
        c
    }

    #[test]
    fn v63_merges_per_zone_duplicate_cows() {
        let c = setup_legacy_milking_timers();
        // Homer recorded twice: once with a real zone (older), once while the
        // area was still unknown (newer). Cookie only has an unknown-zone row.
        // Bessie lives in a genuinely different zone and must survive alongside.
        c.execute_batch(
            "INSERT INTO milking_timers VALUES
                 ('Me','Alpha','Homer','Red Wing Casino','2026-07-20T10:00:00+00:00'),
                 ('Me','Alpha','Homer','','2026-07-20T10:56:00+00:00'),
                 ('Me','Alpha','Cookie','','2026-07-20T09:00:00+00:00'),
                 ('Me','Alpha','Bessie','Serbule','2026-07-20T08:00:00+00:00');",
        )
        .unwrap();

        migration_v63_milking_timers_per_cow(&c).unwrap();

        // One row per cow now.
        let total: i64 = c
            .query_row("SELECT COUNT(*) FROM milking_timers", [], |r| r.get(0))
            .unwrap();
        assert_eq!(total, 3, "each cow collapses to a single row");

        // Homer keeps the most recent milk time and the most recent *non-empty* zone.
        let (zone, last): (String, String) = c
            .query_row(
                "SELECT zone, last_milked_at FROM milking_timers WHERE cow_name = 'Homer'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(zone, "Red Wing Casino");
        assert_eq!(last, "2026-07-20T10:56:00+00:00");

        // Cookie only ever had an unknown zone — stays blank.
        let cookie_zone: String = c
            .query_row(
                "SELECT zone FROM milking_timers WHERE cow_name = 'Cookie'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(cookie_zone, "");

        // The new key is per-cow: re-milking Homer from another zone updates the
        // single row instead of inserting a second one. This mirrors
        // `coordinator::record_cow_milk` — keep the two in step.
        c.execute(
            "INSERT INTO milking_timers (character_name, server_name, cow_name, zone, last_milked_at)
             VALUES ('Me','Alpha','Homer','Serbule','2026-07-20T11:30:00+00:00')
             ON CONFLICT(character_name, server_name, cow_name) DO UPDATE SET
                last_milked_at = MAX(excluded.last_milked_at, milking_timers.last_milked_at),
                zone = CASE WHEN excluded.zone != '' THEN excluded.zone ELSE milking_timers.zone END",
            [],
        )
        .unwrap();
        let homer_rows: i64 = c
            .query_row(
                "SELECT COUNT(*) FROM milking_timers WHERE cow_name = 'Homer'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(homer_rows, 1, "re-milk upserts, never duplicates");

        // The same statement replayed with an *older* stamp (a catch-up replay of
        // an old chat log) must not push the cooldown back, and must still learn a
        // previously-unknown zone.
        c.execute(
            "INSERT INTO milking_timers (character_name, server_name, cow_name, zone, last_milked_at)
             VALUES ('Me','Alpha','Cookie','Serbule','2026-07-20T07:00:00+00:00')
             ON CONFLICT(character_name, server_name, cow_name) DO UPDATE SET
                last_milked_at = MAX(excluded.last_milked_at, milking_timers.last_milked_at),
                zone = CASE WHEN excluded.zone != '' THEN excluded.zone ELSE milking_timers.zone END",
            [],
        )
        .unwrap();
        c.execute(
            "INSERT INTO milking_timers (character_name, server_name, cow_name, zone, last_milked_at)
             VALUES ('Me','Alpha','Cookie','Eltibule','2026-07-20T06:00:00+00:00')
             ON CONFLICT(character_name, server_name, cow_name) DO UPDATE SET
                last_milked_at = MAX(excluded.last_milked_at, milking_timers.last_milked_at),
                zone = CASE WHEN excluded.zone != '' THEN excluded.zone ELSE milking_timers.zone END",
            [],
        )
        .unwrap();
        let (zone, last): (String, String) = c
            .query_row(
                "SELECT zone, last_milked_at FROM milking_timers WHERE cow_name = 'Cookie'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        // Cookie already had a newer stamp (09:00 from the merge), so both the
        // 07:00 row and the 06:00 replay must leave it alone — while the zone
        // still learns from the replayed lines.
        assert_eq!(
            last, "2026-07-20T09:00:00+00:00",
            "older stamps must not move the cooldown back"
        );
        assert_eq!(zone, "Eltibule", "zone still learns from the replayed line");
    }

    #[test]
    fn milking_dedup_migration_collapses_duplicates_and_installs_the_key() {
        let c = setup_legacy_milking_timers();
        assert!(migration_v36_player_milking_log(&c).is_ok());
        assert!(migration_v38_self_milking(&c).is_ok());

        // Pre-v72 data: exact duplicates of the same event, plus look-alikes that
        // differ in one key part and must survive.
        c.execute_batch(
            "INSERT INTO player_milking_log (character_name, server_name, other_player, direction, milked_at) VALUES
                 ('Zen','Dreva','Zen','self_milked','2026-10-13T01:14:16+00:00'),
                 ('Zen','Dreva','Zen','self_milked','2026-10-13T01:14:16+00:00'),
                 ('Zen','Dreva','Zen','self_milked','2026-10-13T01:14:17+00:00'),
                 ('Zen','Dreva','Reyetta','self_milked','2026-10-13T01:14:16+00:00'),
                 ('Alt','Dreva','Zen','self_milked','2026-10-13T01:14:16+00:00');",
        )
        .unwrap();

        migration_v72_milking_dedup(&c).unwrap();

        let total: i64 = c
            .query_row("SELECT COUNT(*) FROM player_milking_log", [], |r| r.get(0))
            .unwrap();
        assert_eq!(
            total, 4,
            "the byte-identical duplicate collapses, distinct keys stay"
        );

        // The index is what makes INSERT ... ON CONFLICT DO NOTHING dedup from now on.
        let idx: i64 = c
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'index' AND name = 'idx_player_milking_event'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(idx, 1, "unique dedup index is installed");

        // Re-running the migration (fresh installs run every block once) is a no-op.
        migration_v72_milking_dedup(&c).unwrap();
        let after: i64 = c
            .query_row("SELECT COUNT(*) FROM player_milking_log", [], |r| r.get(0))
            .unwrap();
        assert_eq!(after, 4);
    }

    #[test]
    fn v73_adds_cooldown_unknown_defaulting_to_known() {
        let c = setup_legacy_milking_timers();
        // A pre-v73 row: no flag column, and the value it holds is the old
        // 59-minute guess — indistinguishable from a real stamp, so it is treated
        // as known (it ages out at the cow's next milk).
        c.execute(
            "INSERT INTO milking_timers VALUES ('Me','Alpha','Cookie','Serbule','2026-07-20T10:00:00+00:00')",
            [],
        )
        .unwrap();

        migration_v73_milking_cooldown_unknown(&c).unwrap();

        let (flag, stamp): (i64, String) = c
            .query_row(
                "SELECT cooldown_unknown, last_milked_at FROM milking_timers WHERE cow_name = 'Cookie'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(flag, 0, "existing rows default to a known milk time");
        assert_eq!(stamp, "2026-07-20T10:00:00+00:00", "the stamp is untouched");
    }

    #[test]
    fn run_migrations_is_idempotent_and_atomic() {
        let conn = Connection::open_in_memory().unwrap();
        run_migrations(&conn, None).unwrap();
        assert_eq!(
            crate::db::get_schema_version(&conn).unwrap(),
            76,
            "fresh DB migrates to the head version"
        );
        // Second run: every `if current_version < N` block is skipped. If the
        // run had left the version table unadvanced (or partially advanced),
        // the non-idempotent ALTER/CREATE blocks would re-run and error here.
        run_migrations(&conn, None).unwrap();
        assert_eq!(crate::db::get_schema_version(&conn).unwrap(), 76);
    }
}
