mod arena_parser;
mod books_watcher;
mod cdn;
mod cdn_commands;
mod cdn_diff;
mod chat_commands;
mod chat_parser;
mod chat_combat_parser;
mod chat_resuscitate_parser;
mod chat_status_parser;
mod commands;
mod coordinator;
mod debug_capture;
mod db;
mod game_data;
mod game_state;
mod log_watchers;
mod parsers;
mod player_event_parser;
mod replay;
mod report_stats;
mod settings;
mod setup_commands;
mod shop_log_parser;
mod single_instance;
mod stall_aggregations;
mod stall_ocr;
mod stall_year_resolver;
mod survey;
mod external_fetch;
mod gst_manager;
mod trip_router;
mod watch_rules;
mod unified_search;
mod zone_graph;

use chrono::Local;
use std::sync::{Arc, Mutex};
use std::time::Instant;
use tauri::{Emitter, Manager};
use tokio::sync::RwLock;

use cdn_commands::{
    check_cdn_version,
    force_refresh_cdn,
    get_abilities_for_skill,
    get_ability_families_for_skill,
    get_ability_family,
    get_ability_sources,
    get_skills_with_ability_counts,
    search_ability_families,
    get_all_item_keywords,
    get_all_lorebooks,
    get_all_npcs,
    get_all_player_titles,
    get_all_quests,
    get_all_skills,
    get_cache_status,
    get_combat_skills,
    get_cp_recipes_for_slot,
    get_effect,
    get_equip_slots,
    get_icon_path,
    get_item_sources,
    get_items_by_keyword,
    get_lorebook_categories,
    get_recipe_ingredient_keywords,
    get_npcs_in_area,
    get_monsters_in_area,
    get_all_areas,
    get_all_monsters,
    get_quest_sources,
    get_recipe_sources,
    get_recipes_for_item,
    get_recipes_for_skill,
    get_recipes_producing_items,
    get_brewing_recipes,
    get_brewing_ingredients,
    get_recipes_using_item,
    get_vendor_purchasable_item_ids,
    get_npc_vendor_items,
    get_vendor_item_counts,
    get_vendors_for_item,
    get_storage_vault_metadata,
    get_storage_vault_zones,
    get_storage_vaults_in_area,
    get_all_tsys,
    get_tsys_power_info,
    get_tsys_power_info_batch,
    compute_ability_build_stats,
    get_tsys_powers_for_slot,
    get_tsys_profiles,
    get_tsys_for_ability,
    get_abilities_for_tsys,
    get_tsys_ability_map,
    search_tsys,
    get_xp_table_for_skill,
    init_game_data,
    resolve_ability,
    resolve_area,
    resolve_effect_descs,
    // Unified entity resolvers
    resolve_item,
    resolve_items_batch,
    resolve_npc,
    resolve_quest,
    resolve_recipe,
    resolve_skill,
    search_effects,
    search_lorebooks,
    // Query/filter commands (not replaced by resolvers)
    search_items,
    search_npcs,
    search_player_titles,
    search_quests,
    search_recipes,
    // Cross-reference commands
    get_npcs_wanting_item,
    get_npcs_training_skill,
    get_quests_for_npc,
    get_quests_for_skill,
    get_quests_by_moon_phase,
    get_current_moon_phase,
    get_recipes_for_keyword,
    find_recipe_items_in_inventory,
    cdn_diff_summary,
    cdn_diff_file,
    // Enemy (AI) queries
    get_all_enemies,
    search_enemies,
    get_enemy,
    // Gardening chain
    get_gardening_product_chain,
    GameDataState,
};
use db::brewing_commands::{
    add_brewing_discovery_manual,
    get_brewing_discoveries,
    import_brewing_discoveries_csv,
    delete_brewing_discovery,
    scan_all_snapshots_for_brewing,
    scan_snapshot_for_brewing_discoveries,
};
use chat_commands::{
    delete_all_chat_messages, count_chat_messages, get_chat_channel_stats, get_chat_channels,
    get_chat_days, get_chat_messages, get_chat_messages_around, get_chat_messages_around_time,
    get_chat_stats, get_tell_conversations, get_watch_rule_messages, purge_chat_messages,
    scan_chat_log_file, scan_chat_logs, tail_chat_log,
    backfill_ability_history,
};
use commands::parse_log;
use debug_capture::replay_capture_file;
use coordinator::{
    debug_capture_discard, debug_capture_save, debug_capture_start, debug_capture_status,
    debug_capture_stop,
    get_coordinator_status, poll_watchers, start_background_polling, stop_background_polling,
    start_chat_tailing, start_player_tailing,
    stop_chat_tailing, stop_player_tailing, scan_player_log_for_poems,
    spawn_polling_thread, DataIngestCoordinator, PollingHandle,
};
use db::admin_commands::{
    compact_database, force_rebuild_cdn_tables, get_database_stats, purge_player_data,
};
use db::aggregate_commands::{get_aggregate_inventory, get_aggregate_skills, get_aggregate_vendor, get_aggregate_wealth};
use db::build_planner_commands::{
    clear_build_preset_slot_item, clone_build_preset, create_build_preset, delete_build_preset,
    export_build_preset, get_build_preset_abilities, get_build_preset_cp_recipes,
    get_build_preset_mods, get_build_preset_slot_items, get_build_presets,
    get_equipped_gear_candidates, import_build_preset, set_build_preset_abilities,
    set_build_preset_cp_recipes,
    set_build_preset_mods, set_build_preset_slot_item, update_build_preset,
    update_build_preset_slot_props,
};
use db::character_commands::{
    compare_snapshots, get_character_snapshots, get_characters, get_snapshot_active_quests,
    get_snapshot_currencies, get_snapshot_npc_favor, get_snapshot_recipes, get_snapshot_skills,
    get_snapshot_stats, import_character_report,
};
use db::death_commands::{get_character_deaths, get_death_damage_sources};
use db::resuscitate_commands::get_character_resuscitations;
use db::crafting_commands::{
    add_project_entry, batch_update_entry_expansions, check_material_availability,
    create_crafting_project, delete_crafting_project, duplicate_crafting_project,
    export_crafting_project, get_crafting_project, get_crafting_projects,
    get_work_orders_from_snapshot, import_crafting_project, remove_project_entry,
    reorder_project_entries, update_crafting_project, update_project_entry,
};
use db::farming_commands::{
    delete_farming_session, get_farming_sessions, save_farming_session, update_farming_session,
};
use db::kill_tracking_commands::{
    delete_imported_source, export_kill_loot_database, get_corpse_extract_details,
    get_enemy_harvest_stats, get_enemy_kill_stats, get_item_drop_sources, import_kill_loot_database,
    list_imported_sources, search_database_enemies, search_database_harvested, search_database_items,
};
use db::game_state_commands::{
    get_currency_estimate,
    get_game_state_active_skills, get_game_state_attributes, get_game_state_currencies,
    get_game_state_effects, get_game_state_equipment, get_game_state_favor,
    get_game_state_inventory, get_game_state_recipes, get_game_state_skills,
    get_character_report_stats, get_game_state_books, get_game_state_storage,
    get_game_state_vendor, get_game_state_world, get_gift_log, get_milking_timers,
    add_manual_gift, remove_last_gift, set_manual_vendor_gold, get_tracked_skills, set_tracked_skills,
    get_computed_stats, get_item_ownership,
    get_attribute_extremes,
    get_player_milking_leaderboard,
    get_pooping_stats,
    get_teleportation_binds,
    set_mushroom_circles,
    get_garden_almanac,
    get_garden_almanac_history,
    get_harvest_almanac,
};
use db::gourmand_commands::{
    export_text_file, get_all_foods, get_gourmand_eaten_foods,
    import_gourmand_report, import_latest_gourmand_report, toggle_food_eaten_status,
};
use db::inventory_commands::{
    get_all_character_items, get_inventory_snapshots, get_inventory_summary, get_snapshot_items,
    import_inventory_report,
};
use db::timer_commands::{delete_user_timer, get_user_timers, save_user_timer};
use db::market_commands::{
    bulk_delete_market_values, bulk_update_market_values, delete_market_value,
    export_market_values, get_market_value, get_market_values, import_market_values,
    set_market_value,
};
use db::player_commands::{
    add_market_price, add_sale, get_market_prices_for_item, get_recent_events, get_recent_sales,
    log_event,
};
use replay::{ingest_player_log, replay_dual_logs};
use trip_router::plan_trip;
use external_fetch::{fetch_github_releases, fetch_pg_news};
use gst_manager::{gst_check_status, gst_download, gst_launch};
use unified_search::unified_search;
use settings::{
    get_default_game_data_path_command, get_default_player_log_path_command, get_server_list,
    get_settings_file_path, load_settings, save_settings, SettingsManager,
};
use setup_commands::{
    complete_setup, delete_character, get_user_characters, import_all_inventory_for_server,
    import_latest_inventory_for_character, import_latest_report_for_character,
    import_reports_for_server, save_user_character, scan_reports_for_characters,
    set_active_character, validate_game_data_path,
};

/// Timestamped log line for startup diagnostics.
macro_rules! startup_log {
    ($($arg:tt)*) => {
        eprintln!("[{}] {}", Local::now().format("%H:%M:%S%.3f"), format!($($arg)*));
    };
}

/// Frontend can push a message into the stderr startup log stream.
#[tauri::command]
fn log_startup(message: String) {
    startup_log!("{}", message);
}

/// For experimental builds: seed the data directory from the release install.
///
/// On first run or when the version changes, this copies `glogger.db` and
/// `settings.json` from the release data directory so testers start with real
/// data. The release install is never modified.
///
/// Operates on `<app_data_dir>/glogger.db` unconditionally — even when the
/// user set a custom `settings.db_path` — because it runs before
/// `SettingsManager` init and is what copies `settings.json` in the first
/// place. Must stay ordered before settings init in `run()`.
fn seed_experimental_data(app_data_dir: &std::path::Path, current_version: &Option<String>) {
    use std::fs;

    // Locate the release data directory (sibling of experimental dir)
    let release_data_dir = if let Some(parent) = app_data_dir.parent() {
        parent.join("glogger.Release")
    } else {
        startup_log!("Experimental seed: cannot determine parent directory, skipping");
        return;
    };

    if !release_data_dir.exists() {
        startup_log!("Experimental seed: no release data dir found at {:?}, starting fresh", release_data_dir);
        return;
    }

    // Check if we need to seed: either no DB exists, or version changed
    let db_path = app_data_dir.join("glogger.db");
    let settings_path = app_data_dir.join("settings.json");
    let needs_seed = if !db_path.exists() {
        startup_log!("Experimental seed: no database found, will copy from release");
        true
    } else {
        // Read the stored version from our settings to see if it's stale
        let stored_version = if settings_path.exists() {
            fs::read_to_string(&settings_path)
                .ok()
                .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
                .and_then(|v| v.get("last_app_version")?.as_str().map(String::from))
        } else {
            None
        };

        if stored_version.as_ref() != current_version.as_ref() {
            startup_log!(
                "Experimental seed: version changed ({} -> {}), will re-copy from release",
                stored_version.as_deref().unwrap_or("unknown"),
                current_version.as_deref().unwrap_or("unknown"),
            );
            true
        } else {
            startup_log!("Experimental seed: version unchanged, keeping existing data");
            false
        }
    };

    if !needs_seed {
        return;
    }

    // Ensure the experimental data directory exists
    if let Err(e) = fs::create_dir_all(app_data_dir) {
        startup_log!("Experimental seed: failed to create data dir: {}", e);
        return;
    }

    // Remove old experimental data so we start clean
    if db_path.exists() {
        if let Err(e) = fs::remove_file(&db_path) {
            startup_log!("Experimental seed: failed to remove old db: {}", e);
            return;
        }
    }
    // Also drop the SQLite journal sidecars. A stale `-wal` beside a freshly
    // copied main file can make the next open recover a journal that does not
    // belong to the file. They may legitimately not exist.
    for suffix in ["-wal", "-shm"] {
        let mut sidecar = db_path.clone().into_os_string();
        sidecar.push(suffix);
        let _ = fs::remove_file(std::path::PathBuf::from(sidecar));
    }

    // Copy database from release
    let release_db = release_data_dir.join("glogger.db");
    if release_db.exists() {
        match fs::copy(&release_db, &db_path) {
            Ok(bytes) => {
                startup_log!("Experimental seed: copied database ({} bytes)", bytes);
            }
            Err(e) => {
                startup_log!("Experimental seed: failed to copy database: {}", e);
                return;
            }
        }
    } else {
        startup_log!("Experimental seed: no release database found, starting fresh");
    }

    // Copy settings from release (preserves game data paths, preferences, etc.)
    let release_settings = release_data_dir.join("settings.json");
    if release_settings.exists() {
        match fs::copy(&release_settings, &settings_path) {
            Ok(_) => {
                startup_log!("Experimental seed: copied settings.json");
            }
            Err(e) => {
                startup_log!("Experimental seed: failed to copy settings: {}", e);
            }
        }
    }
}

/// Work around a WebKitGTK DMABUF-renderer crash on Linux.
///
/// WebKitGTK's DMABUF renderer fails to allocate GBM buffers (`KMS:
/// DRM_IOCTL_MODE_CREATE_DUMB failed: Permission denied` / `Failed to create
/// GBM buffer`) on a range of GPU/driver stacks and aborts the whole process
/// with SIGABRT before a window ever appears — killing the native AppImage and
/// `.deb` builds. We first saw this on the proprietary NVIDIA driver, then on a
/// pure Mesa stack (AMD/Intel) with a bleeding-edge Mesa (26.1 on Nobara/Fedora
/// 43), so the crash is **not** NVIDIA-specific.
///
/// Disabling the DMABUF renderer makes the webview fall back to a working
/// buffer-sharing path. The compositing perf cost is negligible for this
/// data-tracker UI, so we apply it unconditionally on Linux rather than trying
/// to guess which driver/version combinations are affected. We never override a
/// value the user set explicitly, so anyone who wants DMABUF back can export
/// `WEBKIT_DISABLE_DMABUF_RENDERER=0`.
#[cfg(target_os = "linux")]
fn apply_webkit_dmabuf_workaround() {
    if std::env::var_os("WEBKIT_DISABLE_DMABUF_RENDERER").is_some() {
        return;
    }
    std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    #[cfg(target_os = "linux")]
    apply_webkit_dmabuf_workaround();

    let game_data_state: GameDataState = Arc::new(RwLock::new(game_data::GameData::empty()));

    tauri::Builder::default()
        .plugin(tauri_plugin_window_state::Builder::new().build())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .manage(game_data_state.clone())
        .setup(move |app| {
            let app_handle = app.handle().clone();
            let state = game_data_state.clone();

            let current_version = app.config().version.clone();
            startup_log!("glogger v{} starting up", current_version.as_deref().unwrap_or("unknown"));

            // Step 0: refuse to start a second instance against the same app
            // data (two processes on one DB file produce real `database is
            // locked`). Must run before any DB/settings work.
            if !single_instance::acquire(app.handle()) {
                use tauri_plugin_dialog::DialogExt;
                app.dialog()
                    .message("glogger is already running. Close the other window first.")
                    .title("glogger is already running")
                    .blocking_show();
                app.handle().exit(0);
                return Ok(());
            }

            // Step 1: Get app data directory
            let app_data_dir = app
                .path()
                .app_data_dir()
                .expect("Cannot resolve app data dir");

            // Step 1b: Experimental build — seed data from release on new version
            // Experimental builds get a fresh copy of the release database each time
            // the version changes, so testers always validate against real data without
            // risking their release install.
            let app_identifier = app.config().identifier.clone();
            if app_identifier.contains("Experimental") {
                startup_log!("Experimental build detected ({})", app_identifier);
                seed_experimental_data(&app_data_dir, &current_version);
            }

            // Step 2: Initialize settings FIRST (before database)
            let settings_manager = Arc::new(
                SettingsManager::new(app_data_dir.clone())
                    .expect("Failed to initialize settings")
            );
            startup_log!("Settings loaded");

            // Step 2b: Re-run platform path auto-detection on startup if enabled.
            // Must happen before the coordinator starts the log watchers so the
            // refreshed paths take effect this launch.
            {
                let s = settings_manager.get();
                if s.auto_detect_paths_on_startup {
                    let game_path = settings::get_default_game_data_path();
                    // Player.log default: explicit on macOS, empty on Windows
                    // (derived from game_data_path/Player.log at read time).
                    let log_path = settings::get_default_player_log_path();
                    if !game_path.is_empty()
                        && (game_path != s.game_data_path || log_path != s.log_file_path)
                    {
                        let mut updated = s.clone();
                        updated.game_data_path = game_path;
                        updated.log_file_path = log_path;
                        match settings_manager.update(updated) {
                            Ok(()) => {
                                startup_log!("Auto-detected paths on startup (enabled in settings)");
                            }
                            Err(e) => {
                                startup_log!("Auto-detect paths on startup failed to save: {e}");
                            }
                        }
                    }
                }
            }

            // Step 3: Track app version (database is preserved across upgrades via migrations)
            let db_path = settings_manager.get_db_path(&app_data_dir);
            {
                let stored_version = settings_manager.get().last_app_version;
                if stored_version.as_ref() != current_version.as_ref() {
                    startup_log!(
                        "App version changed ({} -> {}), migrations will handle schema updates",
                        stored_version.as_deref().unwrap_or("first run"),
                        current_version.as_deref().unwrap_or("unknown"),
                    );
                    // Invalidate cached indices so CDN data is re-indexed with new code.
                    // This ensures bug fixes to index-building logic (e.g. ability family
                    // construction) take effect without waiting for a CDN version bump.
                    let indices_path = app_data_dir.join("data").join("indices.json");
                    if indices_path.exists() {
                        startup_log!("Removing cached indices.json to force re-index");
                        let _ = std::fs::remove_file(&indices_path);
                    }
                }
                let mut updated = settings_manager.get();
                updated.last_app_version = current_version.clone();
                settings_manager.update(updated).expect("Failed to save version to settings");
            }

            // Step 4: Initialize database (migrations run automatically)
            // Pass timezone offset for one-time migration to fix historical timestamps
            let tz_offset = {
                let s = settings_manager.get();
                s.manual_timezone_override.or(s.timezone_offset_seconds)
            };
            let db_pool = db::init_pool(db_path, tz_offset).expect("Failed to initialize database");
            startup_log!("Database initialized");

            // Step 5: Initialize DataIngestCoordinator
            let coordinator = Arc::new(Mutex::new(
                DataIngestCoordinator::new(
                    db_pool.clone(),
                    settings_manager.clone(),
                    app_handle.clone(),
                    state.clone(),
                ).expect("Failed to initialize DataIngestCoordinator")
            ));
            startup_log!("Coordinator initialized");

            // Step 5b: Spawn background polling thread (starts paused,
            // frontend calls start_background_polling after catch-up)
            let polling_handle = Arc::new(Mutex::new(
                spawn_polling_thread(coordinator.clone(), std::time::Duration::from_millis(500))
            ));
            startup_log!("Background polling thread spawned (paused)");

            // Step 5c: Spawn the Player-prev.log backfill watcher — captures
            // sessions played without glogger running by ingesting Player-prev.log
            // into the kill/loot database when the game rotates it.
            replay::spawn_player_prev_watcher(settings_manager.clone(), db_pool.clone());

            // Step 5c+1: Books-dir backfill watcher — ingests the game's
            // <game_data>/Books/*.txt book exports (written both live and while
            // glogger was closed), mirroring the live ProcessBook ingest via
            // coordinator's shared ingestion helpers. Per-file mtime tracking,
            // 30s cadence.
            books_watcher::spawn_books_watcher(
                settings_manager.clone(),
                db_pool.clone(),
                app_handle.clone(),
            );

            // Step 5d: One-shot backfills from the historical ChatLogs — Combat
            // Wisdom awards, casino roulette outcomes, arena fights, and spoken
            // words of power. All idempotent (unique indexes / delete-by-word).
            //
            // These run SEQUENTIALLY in one spawned task, in cheap-first order.
            // Each uses per-file BEGIN IMMEDIATE transactions; running them
            // concurrently starves the losers — four backfills doing
            // back-to-back per-file transactions leave ~microsecond gaps, and
            // SQLite's busy handler can't land in one, so the other BEGINs hit
            // their 5s busy_timeout (observed in startup logs). Serializing
            // them removes the contention entirely; the scans also share file
            // cache. Catch-up/chat polling still interleaves fine (its bursts
            // are short and its writers wait, not starve).
            {
                let sm = settings_manager.clone();
                let dbp = db_pool.clone();
                let ah = app_handle.clone();
                tauri::async_runtime::spawn(async move {
                    match db::combat_wisdom_commands::backfill_from_chat_logs(&sm, &dbp) {
                        Ok(n) => {
                            startup_log!("Combat Wisdom backfill: {} new award(s)", n);
                        }
                        Err(e) => eprintln!("Combat Wisdom backfill failed: {e}"),
                    }
                    match db::roulette_commands::backfill_from_chat_logs(&sm, &dbp) {
                        Ok(n) => {
                            startup_log!("Roulette backfill: {} new spin(s)", n);
                        }
                        Err(e) => eprintln!("Roulette backfill failed: {e}"),
                    }
                    match db::arena_commands::backfill_from_chat_logs(&sm, &dbp) {
                        Ok(n) => {
                            startup_log!("Arena backfill: {} new match(es)", n);
                        }
                        Err(e) => eprintln!("Arena backfill failed: {e}"),
                    }
                    // Personal betting history from Player.log + Player-prev.log.
                    match db::arena_commands::backfill_bets_from_player_logs(&sm, &dbp) {
                        Ok(n) => {
                            startup_log!("Arena bet backfill: {} new bet(s)", n);
                        }
                        Err(e) => eprintln!("Arena bet backfill failed: {e}"),
                    }
                    match db::words_of_power_commands::backfill_used_words_from_chat_logs(&sm, &dbp)
                    {
                        Ok(0) => {}
                        Ok(n) => {
                            startup_log!("Words of Power backfill: {} used word(s) removed", n);
                            ah.emit("game-state-updated", vec!["words_of_power"]).ok();
                        }
                        Err(e) => eprintln!("Words of Power usage backfill failed: {e}"),
                    }
                });
            }

            // Step 5e: Auto-purge old user data on startup, if enabled in
            // settings. Bounds unbounded growth of the big append-only tables
            // (item_transactions, chat_messages + its FTS index). Non-fatal —
            // a failure here must never block startup.
            {
                let s = settings_manager.get();
                if s.auto_purge_enabled {
                    match db_pool.get_write() {
                        Ok(conn) => {
                            match db::admin_commands::check_auto_purge(&conn, Some(s.auto_purge_days)) {
                                Ok(r) if r.bytes_reclaimed > 0 => {
                                    startup_log!(
                                        "Auto-purge reclaimed {} bytes (data older than {}d)",
                                        r.bytes_reclaimed,
                                        s.auto_purge_days
                                    );
                                }
                                Ok(_) => {}
                                Err(e) => {
                                    startup_log!("Auto-purge failed: {e}");
                                }
                            }
                        }
                        Err(e) => {
                            startup_log!("Auto-purge skipped (no DB connection): {e}");
                        }
                    }
                }
            }

            // Step 6: Register managed state
            app.manage(settings_manager.clone());
            app.manage(db_pool.clone());
            app.manage(coordinator.clone());
            app.manage(polling_handle.clone());
            app.manage(db::stall_tracker_commands::StallOpsLock::default());

            startup_log!("Splash screen displayed (frontend rendering)");

            // Kick off CDN init in the background — non-blocking.
            // The frontend should listen for a "game-data-ready" event
            // or poll get_cache_status() to know when data is available.
            startup_log!("Starting game data load (background)");
            tauri::async_runtime::spawn(async move {
                let t0 = Instant::now();
                match init_game_data(&app_handle, &state).await {
                    Ok(()) => {
                        let data = state.read().await;
                        startup_log!(
                            "Game data ready: v{} — {} items, {} skills, {} recipes, {} npcs, {} effects, {} areas (took {:.1}s)",
                            data.version,
                            data.items.len(),
                            data.skills.len(),
                            data.recipes.len(),
                            data.npcs.len(),
                            data.effects.len(),
                            data.areas.len(),
                            t0.elapsed().as_secs_f64(),
                        );

                        // Persist CDN data to database only if version changed
                        if let Some(pool) = app_handle.try_state::<db::DbPool>() {
                            if let Ok(mut conn) = pool.get_write() {
                                let db_version = db::queries::cdn_data::get_cdn_version(&*conn).ok().flatten();
                                if db_version == Some(data.version) {
                                    startup_log!("CDN v{} already persisted to database, skipping", data.version);
                                } else {
                                    cdn_commands::emit_startup_detail(&app_handle, 0, "Saving to database...");
                                    if let Err(e) = db::cdn_persistence::persist_cdn_data(&mut conn, &data) {
                                        startup_log!("Failed to persist CDN data to database: {e}");
                                    } else {
                                        startup_log!("CDN data persisted to database");
                                    }
                                }
                            }
                        }

                        // Notify the frontend that data is loaded
                        app_handle.emit("game-data-ready", data.version).ok();
                    }
                    Err(e) => {
                        startup_log!("Game data init FAILED: {e}");
                        app_handle.emit("game-data-error", e).ok();
                    }
                }
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            // Startup diagnostics
            log_startup,
            // Log parsing (manual upload)
            parse_log,
            replay_dual_logs,
            ingest_player_log,
            // Combat Wisdom tracking
            db::combat_wisdom_commands::get_combat_wisdom_monsters,
            db::combat_wisdom_commands::backfill_combat_wisdom_from_chat_logs,
            db::roulette_commands::get_roulette_stats,
            db::roulette_commands::backfill_roulette_from_chat_logs,
            db::arena_commands::get_arena_stats,
            db::arena_commands::backfill_arena_from_chat_logs,
            db::arena_commands::backfill_arena_bets_from_player_logs,
            // CDN management
            get_cache_status,
            check_cdn_version,
            force_refresh_cdn,
            cdn_diff_summary,
            cdn_diff_file,
            // Unified entity resolvers
            resolve_item,
            resolve_items_batch,
            resolve_skill,
            resolve_recipe,
            resolve_quest,
            resolve_npc,
            resolve_ability,
            resolve_area,
            // Unified search
            unified_search,
            // Item queries
            search_items,
            get_items_by_keyword,
            get_recipe_ingredient_keywords,
            get_all_item_keywords,
            get_equip_slots,
            // Skill queries
            get_all_skills,
            // Ability queries
            get_abilities_for_skill,
            get_ability_families_for_skill,
            get_ability_family,
            search_ability_families,
            get_skills_with_ability_counts,
            // Recipe queries
            get_recipes_for_item,
            get_recipes_using_item,
            search_recipes,
            get_recipes_for_skill,
            get_recipes_producing_items,
            // Brewing queries
            get_brewing_recipes,
            get_brewing_ingredients,
            get_brewing_discoveries,
            add_brewing_discovery_manual,
            scan_snapshot_for_brewing_discoveries,
            scan_all_snapshots_for_brewing,
            import_brewing_discoveries_csv,
            delete_brewing_discovery,
            // Quest queries
            get_all_quests,
            search_quests,
            // NPC queries
            get_all_npcs,
            search_npcs,
            get_npcs_in_area,
            get_monsters_in_area,
            get_all_areas,
            get_all_monsters,
            // Enemy (AI) queries
            get_all_enemies,
            search_enemies,
            get_enemy,
            // Gardening chain
            get_gardening_product_chain,
            // Effect queries
            search_effects,
            get_effect,
            resolve_effect_descs,
            get_tsys_power_info,
            get_tsys_power_info_batch,
            compute_ability_build_stats,
            // TSys browser queries
            get_all_tsys,
            search_tsys,
            get_tsys_profiles,
            get_tsys_for_ability,
            get_abilities_for_tsys,
            get_tsys_ability_map,
            // Player Title queries
            get_all_player_titles,
            search_player_titles,
            // Lorebook queries
            get_all_lorebooks,
            get_lorebook_categories,
            search_lorebooks,
            // Storage vault queries
            get_storage_vault_zones,
            get_storage_vault_metadata,
            get_storage_vaults_in_area,
            // Icons
            get_icon_path,
            // Source queries
            get_ability_sources,
            get_item_sources,
            get_recipe_sources,
            get_quest_sources,
            get_vendor_purchasable_item_ids,
            get_npc_vendor_items,
            get_vendor_item_counts,
            get_vendors_for_item,
            // Cross-reference queries
            get_npcs_wanting_item,
            get_npcs_training_skill,
            get_quests_for_npc,
            get_quests_for_skill,
            get_quests_by_moon_phase,
            get_current_moon_phase,
            get_recipes_for_keyword,
            find_recipe_items_in_inventory,
            // Player data - Market prices
            add_market_price,
            get_market_prices_for_item,
            // Player data - Sales
            add_sale,
            get_recent_sales,
            // Player data - Event log
            log_event,
            get_recent_events,
            // Database admin
            get_database_stats,
            compact_database,
            force_rebuild_cdn_tables,
            purge_player_data,
            // Settings
            load_settings,
            save_settings,
            get_settings_file_path,
            get_default_player_log_path_command,
            get_default_game_data_path_command,
            get_server_list,
            // Setup / Onboarding
            validate_game_data_path,
            scan_reports_for_characters,
            save_user_character,
            get_user_characters,
            set_active_character,
            delete_character,
            complete_setup,
            import_latest_report_for_character,
            import_latest_inventory_for_character,
            import_reports_for_server,
            import_all_inventory_for_server,
            // Chat
            scan_chat_logs,
            scan_chat_log_file,
            backfill_ability_history,
            get_chat_messages,
            count_chat_messages,
            get_chat_messages_around,
            get_chat_messages_around_time,
            get_chat_days,
            get_chat_channels,
            get_chat_channel_stats,
            get_chat_stats,
            tail_chat_log,
            get_tell_conversations,
            purge_chat_messages,
            delete_all_chat_messages,
            get_watch_rule_messages,
            // Coordinator
            start_player_tailing,
            stop_player_tailing,
            scan_player_log_for_poems,
            start_chat_tailing,
            stop_chat_tailing,
            get_coordinator_status,
            poll_watchers,
            start_background_polling,
            stop_background_polling,
            // Debug capture
            debug_capture_start,
            debug_capture_stop,
            debug_capture_save,
            debug_capture_discard,
            debug_capture_status,
            replay_capture_file,
            // Survey tracker (Phase 5)
            survey::commands::survey_tracker_status,
            survey::commands::survey_tracker_start_session,
            survey::commands::survey_tracker_end_session,
            survey::commands::survey_tracker_recent_sessions,
            survey::commands::survey_tracker_session_detail,
            survey::commands::survey_tracker_historical_sessions,
            survey::commands::survey_tracker_update_session_notes,
            survey::commands::survey_tracker_update_session_name,
            survey::commands::survey_tracker_update_session_times,
            survey::commands::survey_tracker_delete_session,
            survey::commands::survey_tracker_analytics,
            survey::commands::survey_tracker_item_cost_analysis,
            // Character import
            import_character_report,
            get_characters,
            get_character_snapshots,
            get_snapshot_skills,
            get_snapshot_npc_favor,
            get_snapshot_recipes,
            get_snapshot_stats,
            get_snapshot_currencies,
            get_snapshot_active_quests,
            compare_snapshots,
            // Character deaths
            get_character_deaths,
            get_death_damage_sources,
            // Resuscitations
            get_character_resuscitations,
            // Inventory import
            import_inventory_report,
            get_inventory_snapshots,
            get_snapshot_items,
            get_inventory_summary,
            get_all_character_items,
            // Gourmand tracker
            get_all_foods,
            import_gourmand_report,
            get_gourmand_eaten_foods,
            toggle_food_eaten_status,
            export_text_file,
            import_latest_gourmand_report,
            // Farming calculator
            save_farming_session,
            get_farming_sessions,
            update_farming_session,
            delete_farming_session,
            // Kill tracking / drop-rate database
            get_enemy_kill_stats,
            get_item_drop_sources,
            search_database_enemies,
            search_database_items,
            search_database_harvested,
            export_kill_loot_database,
            import_kill_loot_database,
            list_imported_sources,
            delete_imported_source,
            get_corpse_extract_details,
            get_enemy_harvest_stats,
            // Crafting helper
            create_crafting_project,
            get_crafting_projects,
            get_crafting_project,
            update_crafting_project,
            delete_crafting_project,
            add_project_entry,
            update_project_entry,
            batch_update_entry_expansions,
            remove_project_entry,
            reorder_project_entries,
            duplicate_crafting_project,
            export_crafting_project,
            import_crafting_project,
            check_material_availability,
            get_work_orders_from_snapshot,
            // CDN - XP tables
            get_xp_table_for_skill,
            // CDN - Build planner queries
            get_combat_skills,
            get_tsys_powers_for_slot,
            get_cp_recipes_for_slot,
            // Build planner persistence
            create_build_preset,
            clone_build_preset,
            get_build_presets,
            update_build_preset,
            delete_build_preset,
            get_build_preset_mods,
            set_build_preset_mods,
            set_build_preset_slot_item,
            clear_build_preset_slot_item,
            get_build_preset_slot_items,
            get_equipped_gear_candidates,
            update_build_preset_slot_props,
            set_build_preset_abilities,
            get_build_preset_abilities,
            get_build_preset_cp_recipes,
            set_build_preset_cp_recipes,
            export_build_preset,
            import_build_preset,
            // Game state queries
            get_game_state_skills,
            get_game_state_attributes,
            get_game_state_active_skills,
            get_game_state_world,
            get_game_state_inventory,
            get_game_state_recipes,
            get_game_state_equipment,
            get_game_state_favor,
            get_gift_log,
            add_manual_gift,
            remove_last_gift,
            get_game_state_currencies,
            get_currency_estimate,
            get_game_state_effects,
            get_game_state_storage,
            get_game_state_vendor,
            set_manual_vendor_gold,
            get_game_state_books,
            get_milking_timers,
            get_character_report_stats,
            get_computed_stats,
            get_item_ownership,
            get_attribute_extremes,
            get_player_milking_leaderboard,
            get_pooping_stats,
            get_tracked_skills,
            set_tracked_skills,
            get_market_values,
            get_market_value,
            set_market_value,
            delete_market_value,
            export_market_values,
            import_market_values,
            bulk_update_market_values,
            bulk_delete_market_values,
            get_aggregate_inventory,
            get_aggregate_wealth,
            get_aggregate_skills,
            get_aggregate_vendor,
            // Stall Tracker
            db::stall_tracker_commands::get_stall_events,
            db::stall_tracker_commands::get_stall_stats,
            db::stall_tracker_commands::get_stall_revenue,
            db::stall_tracker_commands::get_stall_inventory,
            db::stall_tracker_commands::get_stall_sales_timeseries,
            db::stall_tracker_commands::get_stall_filter_options,
            db::stall_tracker_commands::toggle_stall_event_ignored,
            db::stall_tracker_commands::clear_stall_events,
            db::stall_tracker_commands::import_shop_log_file,
            db::stall_tracker_commands::export_shop_log_files,
            db::stall_tracker_commands::seed_stall_events_dev,
            // Stall Price Capture
            db::stall_commands::get_stall_price_observations,
            db::stall_commands::record_stall_prices,
            db::stall_commands::update_stall_price_observation,
            db::stall_commands::delete_stall_price_observation,
            // Stall OCR (Phase B)
            stall_ocr::scan_stall_window,
            stall_ocr::ocr_check_status,
            stall_ocr::ocr_download,
            stall_ocr::ocr_launch_setup,
            // Hoplology
            db::hoplology_commands::get_hoplology_studies,
            db::hoplology_commands::get_hoplology_stats,
            // Words of Power
            db::words_of_power_commands::get_words_of_power,
            db::words_of_power_commands::add_word_of_power,
            db::words_of_power_commands::delete_word_of_power,
            db::words_of_power_commands::import_words_of_power_csv,
            db::words_of_power_commands::backfill_words_of_power_usage,
            // User timers
            get_user_timers,
            save_user_timer,
            delete_user_timer,
            // Teleportation binds
            get_teleportation_binds,
            set_mushroom_circles,
            // Player messages (pigeons & stall notes)
            db::message_commands::get_player_messages,
            // Recorded poems (Chat Logs → Poems tab)
            db::poem_commands::get_poems,
            // Garden almanac
            get_garden_almanac,
            get_garden_almanac_history,
            // Harvest almanac (Statehelm's Emotion-Harvesting Almanac)
            get_harvest_almanac,
            // Trip routing
            plan_trip,
            // External content
            fetch_github_releases,
            fetch_pg_news,
            // GorgonSurveyTracker management
            gst_check_status,
            gst_download,
            gst_launch,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app_handle, event| {
            if let tauri::RunEvent::Exit = event {
                // Stop the background polling thread first so it doesn't
                // fight with the coordinator lock during shutdown.
                if let Some(ph) = app_handle.try_state::<Arc<Mutex<PollingHandle>>>() {
                    ph.lock().unwrap().shutdown();
                    startup_log!("[shutdown] Polling thread stopped.");
                }

                // Save watcher positions before the process dies
                if let Some(coordinator) = app_handle.try_state::<Arc<Mutex<DataIngestCoordinator>>>() {
                    let mut coord = coordinator.lock().unwrap();
                    if let Err(e) = coord.stop_player_log_tailing() {
                        startup_log!("[shutdown] Failed to save player log position: {e}");
                    }
                    if let Err(e) = coord.stop_chat_log_tailing() {
                        startup_log!("[shutdown] Failed to save chat log position: {e}");
                    }
                    startup_log!("[shutdown] Watcher positions saved.");
                }
            }
        });
}
