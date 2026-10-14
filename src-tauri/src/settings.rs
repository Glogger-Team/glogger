/// Application settings management - file-based storage
///
/// This module provides early initialization of settings before database setup.
/// Settings determine paths for database, game data, and auto-start behavior.
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppSettings {
    /// Path to Player.log file (legacy, may deprecate)
    pub log_file_path: String,

    /// Auto-watch Player.log on startup (legacy)
    pub auto_watch_on_startup: bool,

    /// Root game data directory.
    /// Windows: %AppData%\..\LocalLow\Elder Game\Project Gorgon\
    /// macOS:   ~/Library/Application Support/unity.Elder Game.Project Gorgon/
    pub game_data_path: String,

    /// Automatically purge old data
    pub auto_purge_enabled: bool,

    /// Days to keep data before purging
    pub auto_purge_days: u32,

    /// Automatically start chat log tailing on startup
    #[serde(default)]
    pub auto_tail_chat: bool,

    /// Automatically start Player.log tailing on startup
    #[serde(default)]
    pub auto_tail_player_log: bool,

    /// Custom database path (null = default in app data dir)
    #[serde(default)]
    pub db_path: Option<String>,

    /// Chat channels to exclude from logging
    #[serde(default = "default_excluded_channels")]
    pub excluded_chat_channels: Vec<String>,

    /// Enable dev mode (reveals beta features/tools)
    #[serde(default)]
    pub dev_mode_enabled: bool,

    /// Auto-check for new CDN game data versions
    #[serde(default = "default_true")]
    pub auto_check_game_data: bool,

    /// Auto-update CDN game data when new version found
    #[serde(default = "default_true")]
    pub auto_update_game_data: bool,

    /// Watch rules for chat notifications (watchwords, item watches, sender alerts)
    #[serde(default)]
    pub watch_rules: Vec<WatchRule>,

    /// Whether first-time setup has been completed
    #[serde(default)]
    pub setup_completed: bool,

    /// Currently active character name
    #[serde(default)]
    pub active_character_name: Option<String>,

    /// Currently active server name
    #[serde(default)]
    pub active_server_name: Option<String>,

    /// Auto-load last used character on startup
    #[serde(default = "default_true")]
    pub auto_load_last_character: bool,

    /// Auto-watch Reports folder for new character reports
    #[serde(default = "default_true")]
    pub auto_watch_reports: bool,

    /// Interval in seconds for checking the Reports folder
    #[serde(default = "default_report_watch_interval")]
    pub report_watch_interval_seconds: u32,

    /// Exclude "Max-Enchanted" recipes from automated recipe selection
    /// (leveling optimizer, work order matching, intermediate resolution)
    #[serde(default = "default_true")]
    pub exclude_max_enchanted_recipes: bool,

    /// Market price mode: "universal" (one price per item) or "per_server" (price per item per server)
    #[serde(default = "default_market_price_mode")]
    pub market_price_mode: String,

    /// Item valuation mode for wealth/cost calculations:
    /// - "highest_market_vendor" (default): max(market, vendor)
    /// - "highest_market_buy_used": max(market, vendor * 2)
    /// - "vendor_only": vendor value only
    /// - "buy_used_only": vendor * 2
    /// - "market_only": market value only
    #[serde(default = "default_item_valuation_mode")]
    pub item_valuation_mode: String,

    /// Whether to show raw JSON in the Data Browser detail panels
    #[serde(default)]
    pub show_raw_json_in_data_browser: bool,

    /// Show items/abilities tagged Lint_NotObtainable in searches and browsers
    #[serde(default)]
    pub show_unobtainable_items: bool,

    /// Opaque per-screen UI preferences (persisted as JSON, frontend-managed)
    #[serde(default)]
    pub view_preferences: Option<JsonValue>,

    /// Last app version that was run (used for logging version transitions)
    #[serde(default)]
    pub last_app_version: Option<String>,

    /// How timestamps are displayed in the UI: "local" (browser local time, default),
    /// "server" (game server time, using detected/manual timezone offset), or "utc".
    #[serde(default = "default_timestamp_display_mode")]
    pub timestamp_display_mode: String,

    /// Auto-detected timezone offset in seconds from UTC (e.g., -25200 for UTC-7).
    /// Populated from chat login line's "Timezone Offset" field.
    #[serde(default)]
    pub timezone_offset_seconds: Option<i32>,

    /// Manual timezone override in seconds from UTC. When set, takes precedence
    /// over the auto-detected offset. Advanced setting for edge cases.
    #[serde(default)]
    pub manual_timezone_override: Option<i32>,

    /// When true (default), survey sessions auto-start on crafting or
    /// first-use detection. When false, sessions only start manually.
    #[serde(default = "default_true")]
    pub auto_start_survey_sessions: bool,

    /// When true (default), display times in 24-hour format (e.g. "14:30").
    /// When false, use 12-hour format with AM/PM (e.g. "2:30 PM").
    #[serde(default = "default_true")]
    pub use_24_hour_time: bool,

    /// CSS font-family stack used for the app UI. Defaults to "monospace"
    /// to preserve the original appearance. Frontend-managed and applied
    /// via the --app-font-family CSS variable.
    #[serde(default = "default_ui_font_family")]
    pub ui_font_family: String,

    /// Base UI font size in pixels. Defaults to 14. Frontend-managed and
    /// applied via the --app-font-size CSS variable.
    #[serde(default = "default_ui_font_size")]
    pub ui_font_size: u32,

    /// Dashboard widget background opacity as a percentage (0–100). 100 = fully
    /// opaque (default). Frontend-managed; applied via the
    /// --dashboard-widget-bg-opacity CSS variable.
    #[serde(default = "default_dashboard_widget_opacity")]
    pub dashboard_widget_opacity: u32,

    /// Whole-app UI scale as a percentage (50–200). 100 = native size (default).
    /// Frontend-managed; applied via CSS `zoom` on the document root so it scales
    /// everything uniformly (handy on high-DPI / 4K displays).
    #[serde(default = "default_ui_scale")]
    pub ui_scale: u32,

    /// When true, re-run platform path auto-detection on every startup,
    /// overwriting the game data and Player.log paths with the OS defaults.
    /// Useful when the game install or user profile moves between launches.
    #[serde(default)]
    pub auto_detect_paths_on_startup: bool,

    /// When true (default), automatically back up Player-prev.log into the
    /// lifetime kill/loot database whenever the game rotates it (so sessions
    /// played without glogger running are still captured). Deduped against
    /// live data, so no double-counting.
    #[serde(default = "default_true")]
    pub auto_ingest_player_prev: bool,

    /// How often (in minutes) an active farming session is auto-saved to the
    /// database so its data survives a crash or power loss before the session is
    /// ended manually. 0 = disabled; otherwise one of 5, 10, or 30.
    #[serde(default = "default_farming_autosave_minutes")]
    pub farming_autosave_minutes: u32,

    /// When true, a farming session is automatically started on app launch (after the initial
    /// log catch-up) so farming is logged to the database without having to start one manually.
    /// Toggling it off ends and saves any active session. Off by default (opt-in).
    #[serde(default)]
    pub auto_start_farming_sessions: bool,

    /// Maximum active (unpaused) duration, in minutes, a single farming session
    /// may run before it's auto-ended (and saved) and rolled over to a fresh
    /// one — bounding how large any one session can grow so a long-running
    /// session can't slow down or crash the app. Default 120 (2 hours).
    #[serde(default = "default_farming_session_cap_minutes")]
    pub farming_session_cap_minutes: u32,
}

fn default_farming_autosave_minutes() -> u32 {
    5
}

fn default_farming_session_cap_minutes() -> u32 {
    120
}

fn default_dashboard_widget_opacity() -> u32 {
    100
}

fn default_ui_scale() -> u32 {
    100
}

fn default_ui_font_family() -> String {
    "monospace".to_string()
}

fn default_ui_font_size() -> u32 {
    16
}

fn default_timestamp_display_mode() -> String {
    "local".to_string()
}

fn default_market_price_mode() -> String {
    "universal".to_string()
}

fn default_item_valuation_mode() -> String {
    "highest_market_vendor".to_string()
}

/// Resolve the effective value of an item given vendor value and market value,
/// based on the item valuation mode setting.
pub fn resolve_item_value(mode: &str, vendor_value: f64, market_value: f64) -> f64 {
    match mode {
        "highest_market_vendor" => vendor_value.max(market_value),
        "highest_market_buy_used" => (vendor_value * 2.0).max(market_value),
        "vendor_only" => vendor_value,
        "buy_used_only" => vendor_value * 2.0,
        "market_only" => market_value,
        _ => vendor_value.max(market_value), // fallback to default
    }
}

/// How conditions within a rule are combined
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ConditionMatch {
    /// All conditions must match
    All,
    /// Any single condition is enough
    Any,
}

impl Default for ConditionMatch {
    fn default() -> Self {
        ConditionMatch::Any
    }
}

/// A single watch rule that fires when conditions match an incoming chat message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WatchRule {
    pub id: u64,
    pub name: String,
    pub enabled: bool,
    /// Channels to match against. `None` means all channels.
    pub channels: Option<Vec<String>>,
    /// How conditions are combined: All (AND) or Any (OR). Defaults to Any.
    #[serde(default)]
    pub match_mode: ConditionMatch,
    pub conditions: Vec<WatchCondition>,
    pub notify: WatchNotifyConfig,
}

/// A single condition within a watch rule
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "value")]
pub enum WatchCondition {
    /// Case-insensitive substring match on message body AND item link names
    ContainsText(String),
    /// Case-insensitive match on item link names only
    ContainsItemLink(String),
    /// Case-insensitive exact match on sender name
    FromSender(String),
}

/// How to notify the user when a watch rule fires
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WatchNotifyConfig {
    pub sound: bool,
    pub toast: bool,
    pub highlight: bool,
}

/// Known Project Gorgon servers
pub const PG_SERVERS: &[&str] = &["Dreva", "Arisetsu", "Laeth", "Miraverre", "Strekios"];

fn default_excluded_channels() -> Vec<String> {
    vec![
        "System".to_string(),
        "Error".to_string(),
        "Emotes".to_string(),
        "Action Emotes".to_string(),
        "NPC Chatter".to_string(),
        "Status".to_string(),
        "Combat".to_string(),
    ]
}

fn default_true() -> bool {
    true
}

fn default_report_watch_interval() -> u32 {
    10
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            log_file_path: get_default_player_log_path(),
            auto_watch_on_startup: false,
            game_data_path: get_default_game_data_path(),
            auto_purge_enabled: false,
            auto_purge_days: 90,
            auto_tail_chat: false,
            auto_tail_player_log: false,
            db_path: None,
            excluded_chat_channels: default_excluded_channels(),
            dev_mode_enabled: false,
            auto_check_game_data: true,
            auto_update_game_data: true,
            watch_rules: Vec::new(),
            setup_completed: false,
            active_character_name: None,
            active_server_name: None,
            auto_load_last_character: true,
            auto_watch_reports: true,
            report_watch_interval_seconds: 10,
            exclude_max_enchanted_recipes: true,
            market_price_mode: default_market_price_mode(),
            item_valuation_mode: default_item_valuation_mode(),
            show_raw_json_in_data_browser: false,
            show_unobtainable_items: false,
            view_preferences: None,
            last_app_version: None,
            timestamp_display_mode: default_timestamp_display_mode(),
            timezone_offset_seconds: None,
            manual_timezone_override: None,
            auto_start_survey_sessions: true,
            use_24_hour_time: true,
            ui_font_family: default_ui_font_family(),
            ui_font_size: default_ui_font_size(),
            dashboard_widget_opacity: default_dashboard_widget_opacity(),
            ui_scale: default_ui_scale(),
            auto_detect_paths_on_startup: false,
            auto_ingest_player_prev: true,
            farming_autosave_minutes: default_farming_autosave_minutes(),
            auto_start_farming_sessions: false,
            farming_session_cap_minutes: default_farming_session_cap_minutes(),
        }
    }
}

/// Settings manager with early initialization support
pub struct SettingsManager {
    settings: Arc<RwLock<AppSettings>>,
    settings_path: PathBuf,
}

impl SettingsManager {
    /// Create a new settings manager with the given app data directory
    pub fn new(app_data_dir: PathBuf) -> Result<Self, String> {
        // Ensure app data directory exists
        std::fs::create_dir_all(&app_data_dir)
            .map_err(|e| format!("Cannot create app data dir: {e}"))?;

        let settings_path = app_data_dir.join("settings.json");

        // Load settings from file or use defaults
        let settings = if settings_path.exists() {
            let contents = std::fs::read_to_string(&settings_path)
                .map_err(|e| format!("Failed to read settings file: {e}"))?;

            serde_json::from_str(&contents)
                .map_err(|e| format!("Failed to parse settings file: {e}"))?
        } else {
            AppSettings::default()
        };

        Ok(Self {
            settings: Arc::new(RwLock::new(settings)),
            settings_path,
        })
    }

    /// Get a clone of the current settings
    pub fn get(&self) -> AppSettings {
        self.settings.read().unwrap().clone()
    }

    /// Update settings and save to disk
    pub fn update(&self, settings: AppSettings) -> Result<(), String> {
        // Update in-memory settings
        *self.settings.write().unwrap() = settings.clone();

        // Save to disk
        let contents = serde_json::to_string_pretty(&settings)
            .map_err(|e| format!("Failed to serialize settings: {e}"))?;

        std::fs::write(&self.settings_path, contents)
            .map_err(|e| format!("Failed to write settings file: {e}"))?;

        Ok(())
    }

    /// Get the path to the settings file
    pub fn settings_file_path(&self) -> &PathBuf {
        &self.settings_path
    }

    /// Get the database path (either custom or default)
    pub fn get_db_path(&self, default_app_data_dir: &PathBuf) -> PathBuf {
        let settings = self.settings.read().unwrap();

        if let Some(custom_path) = &settings.db_path {
            PathBuf::from(custom_path)
        } else {
            default_app_data_dir.join("glogger.db")
        }
    }

    /// Get the game data path
    #[allow(dead_code)]
    pub fn get_game_data_path(&self) -> String {
        self.settings.read().unwrap().game_data_path.clone()
    }

    /// Get Player.log path.
    ///
    /// Uses the explicit `log_file_path` setting when non-empty (macOS).
    /// Falls back to `game_data_path / "Player.log"` for backward compat (Windows).
    pub fn get_player_log_path(&self) -> Option<PathBuf> {
        let settings = self.settings.read().unwrap();

        if !settings.log_file_path.is_empty() {
            return Some(PathBuf::from(&settings.log_file_path));
        }

        if settings.game_data_path.is_empty() {
            return None;
        }

        Some(PathBuf::from(&settings.game_data_path).join("Player.log"))
    }

    /// Get Player-prev.log path (the previous session, rotated by the game).
    /// Derived as a sibling of the active Player.log.
    pub fn get_player_prev_log_path(&self) -> Option<PathBuf> {
        let player_log = self.get_player_log_path()?;
        let parent = player_log.parent()?;
        Some(parent.join("Player-prev.log"))
    }

    /// Get ChatLogs directory path
    pub fn get_chat_logs_dir(&self) -> Option<PathBuf> {
        let settings = self.settings.read().unwrap();

        if settings.game_data_path.is_empty() {
            return None;
        }

        Some(PathBuf::from(&settings.game_data_path).join("ChatLogs"))
    }

    /// Whether automatic Player-prev.log backfill is enabled.
    pub fn get_auto_ingest_player_prev(&self) -> bool {
        self.settings.read().unwrap().auto_ingest_player_prev
    }
}

/// Get the default Player.log path, or empty string if not determinable.
/// On macOS Player.log lives under ~/Library/Logs/, separate from game data.
/// On Windows returns empty so callers fall back to game_data_path/Player.log.
pub fn get_default_player_log_path() -> String {
    if cfg!(target_os = "macos") {
        if let Ok(home) = std::env::var("HOME") {
            return PathBuf::from(home)
                .join("Library")
                .join("Logs")
                .join("Elder Game")
                .join("Project Gorgon")
                .join("Player.log")
                .to_str()
                .unwrap_or_default()
                .to_string();
        }
    }
    String::new()
}

/// Get default game data path (platform-aware).
///
/// Windows: %APPDATA%\..\LocalLow\Elder Game\Project Gorgon\
/// macOS:   ~/Library/Application Support/unity.Elder Game.Project Gorgon/
pub fn get_default_game_data_path() -> String {
    if cfg!(target_os = "windows") {
        if let Ok(local_appdata_low) = std::env::var("APPDATA") {
            // APPDATA points to Roaming, we need LocalLow
            let path = PathBuf::from(local_appdata_low)
                .parent()
                .map(|p| p.join("LocalLow").join("Elder Game").join("Project Gorgon"))
                .and_then(|p| p.to_str().map(|s| s.to_string()))
                .unwrap_or_default();

            if !path.is_empty() {
                return path;
            }
        }
    } else if cfg!(target_os = "macos") {
        if let Ok(home) = std::env::var("HOME") {
            return PathBuf::from(home)
                .join("Library")
                .join("Application Support")
                .join("unity.Elder Game.Project Gorgon")
                .to_str()
                .unwrap_or_default()
                .to_string();
        }
    }

    // Fallback for other OSes or if we can't determine the path
    String::new()
}

// ============================================================
// Tauri Commands (frontend interface)
// ============================================================

/// Load settings (called from frontend)
#[tauri::command]
pub fn load_settings(
    settings_manager: tauri::State<'_, Arc<SettingsManager>>,
) -> Result<AppSettings, String> {
    Ok(settings_manager.get())
}

/// Save settings (called from frontend)
#[tauri::command]
pub fn save_settings(
    settings_manager: tauri::State<'_, Arc<SettingsManager>>,
    settings: AppSettings,
) -> Result<(), String> {
    settings_manager.update(settings)
}

/// Get the list of known Project Gorgon servers
#[tauri::command]
pub fn get_server_list() -> Vec<String> {
    PG_SERVERS.iter().map(|s| s.to_string()).collect()
}

/// Return the platform-appropriate default Player.log path.
#[tauri::command]
pub fn get_default_player_log_path_command() -> String {
    get_default_player_log_path()
}

/// Return the platform-appropriate default game data path.
#[tauri::command]
pub fn get_default_game_data_path_command() -> String {
    get_default_game_data_path()
}

/// Get the settings file path (for user reference)
#[tauri::command]
pub fn get_settings_file_path(
    settings_manager: tauri::State<'_, Arc<SettingsManager>>,
) -> Result<String, String> {
    settings_manager
        .settings_file_path()
        .to_str()
        .map(|s| s.to_string())
        .ok_or_else(|| "Invalid path".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Existing settings.json files still carry the now-removed retention keys;
    /// `AppSettings` must not use `deny_unknown_fields`, so they keep parsing.
    #[test]
    fn legacy_settings_with_removed_keys_still_deserialize() {
        let json = r#"{
            "log_file_path": "",
            "auto_watch_on_startup": false,
            "game_data_path": "/tmp/game",
            "auto_purge_enabled": false,
            "auto_purge_days": 90,
            "chat_retention_days": 30,
            "tells_retention_days": 60,
            "guild_retention_days": 90,
            "user_data_auto_purge_days": 14
        }"#;
        let s: AppSettings = serde_json::from_str(json).unwrap();
        assert_eq!(s.game_data_path, "/tmp/game");
        assert_eq!(s.auto_purge_days, 90);
        assert!(!s.auto_purge_enabled);
    }
}
