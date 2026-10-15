import { defineStore, acceptHMRUpdate } from "pinia";
import { ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import type { WatchRule } from "../types/database";

export interface AppSettings {
  logFilePath: string;
  autoWatchOnStartup: boolean;
  gameDataPath: string;
  autoPurgeEnabled: boolean;
  autoPurgeDays: number;
  autoTailChat: boolean;
  autoTailPlayerLog: boolean;
  dbPath: string | null;
  excludedChatChannels: string[];
  devModeEnabled: boolean;
  autoCheckGameData: boolean;
  autoUpdateGameData: boolean;
  watchRules: WatchRule[];
  setupCompleted: boolean;
  activeCharacterName: string | null;
  activeServerName: string | null;
  autoLoadLastCharacter: boolean;
  autoWatchReports: boolean;
  reportWatchIntervalSeconds: number;
  excludeMaxEnchantedRecipes: boolean;
  marketPriceMode: string;
  itemValuationMode: string;
  showRawJsonInDataBrowser: boolean;
  showUnobtainableItems: boolean;
  timestampDisplayMode: 'local' | 'server' | 'utc';
  timezoneOffsetSeconds: number | null;
  manualTimezoneOverride: number | null;
  autoStartSurveySessions: boolean;
  use24HourTime: boolean;
  uiFontFamily: string;
  uiFontSize: number;
  dashboardWidgetOpacity: number;
  uiScale: number;
  autoDetectPathsOnStartup: boolean;
  autoIngestPlayerPrev: boolean;
  playerLogArchiveRetentionDays: number;
  playerLogArchiveMaxFiles: number;
  playerLogArchiveMaxMb: number;
  farmingAutosaveMinutes: number;
  autoStartFarmingSessions: boolean;
  farmingSessionCapMinutes: number;
  viewPreferences: Record<string, Record<string, unknown>>;
}

// Backend settings format (snake_case)
interface BackendSettings {
  log_file_path: string;
  auto_watch_on_startup: boolean;
  game_data_path: string;
  auto_purge_enabled: boolean;
  auto_purge_days: number;
  auto_tail_chat: boolean;
  auto_tail_player_log: boolean;
  db_path: string | null;
  excluded_chat_channels: string[];
  dev_mode_enabled: boolean;
  auto_check_game_data: boolean;
  auto_update_game_data: boolean;
  watch_rules: WatchRule[];
  setup_completed: boolean;
  active_character_name: string | null;
  active_server_name: string | null;
  auto_load_last_character: boolean;
  auto_watch_reports: boolean;
  report_watch_interval_seconds: number;
  exclude_max_enchanted_recipes: boolean;
  market_price_mode: string;
  item_valuation_mode: string;
  show_raw_json_in_data_browser: boolean;
  show_unobtainable_items: boolean;
  timestamp_display_mode: string;
  timezone_offset_seconds: number | null;
  manual_timezone_override: number | null;
  auto_start_survey_sessions: boolean;
  use_24_hour_time: boolean;
  ui_font_family: string;
  ui_font_size: number;
  dashboard_widget_opacity: number;
  ui_scale: number;
  auto_detect_paths_on_startup: boolean;
  auto_ingest_player_prev: boolean;
  player_log_archive_retention_days: number;
  player_log_archive_max_files: number;
  player_log_archive_max_mb: number;
  farming_autosave_minutes: number;
  auto_start_farming_sessions: boolean;
  farming_session_cap_minutes: number;
  view_preferences: Record<string, Record<string, unknown>>;
}

// Convert frontend format to backend format
function toBackendSettings(settings: AppSettings): BackendSettings {
  return {
    log_file_path: settings.logFilePath,
    auto_watch_on_startup: settings.autoWatchOnStartup,
    game_data_path: settings.gameDataPath,
    auto_purge_enabled: settings.autoPurgeEnabled,
    auto_purge_days: settings.autoPurgeDays,
    auto_tail_chat: settings.autoTailChat,
    auto_tail_player_log: settings.autoTailPlayerLog,
    db_path: settings.dbPath,
    excluded_chat_channels: settings.excludedChatChannels,
    dev_mode_enabled: settings.devModeEnabled,
    auto_check_game_data: settings.autoCheckGameData,
    auto_update_game_data: settings.autoUpdateGameData,
    watch_rules: settings.watchRules,
    setup_completed: settings.setupCompleted,
    active_character_name: settings.activeCharacterName,
    active_server_name: settings.activeServerName,
    auto_load_last_character: settings.autoLoadLastCharacter,
    auto_watch_reports: settings.autoWatchReports,
    report_watch_interval_seconds: settings.reportWatchIntervalSeconds,
    exclude_max_enchanted_recipes: settings.excludeMaxEnchantedRecipes,
    market_price_mode: settings.marketPriceMode,
    item_valuation_mode: settings.itemValuationMode,
    show_raw_json_in_data_browser: settings.showRawJsonInDataBrowser,
    show_unobtainable_items: settings.showUnobtainableItems,
    timestamp_display_mode: settings.timestampDisplayMode,
    timezone_offset_seconds: settings.timezoneOffsetSeconds,
    manual_timezone_override: settings.manualTimezoneOverride,
    auto_start_survey_sessions: settings.autoStartSurveySessions,
    use_24_hour_time: settings.use24HourTime,
    ui_font_family: settings.uiFontFamily,
    ui_font_size: settings.uiFontSize,
    dashboard_widget_opacity: settings.dashboardWidgetOpacity,
    ui_scale: settings.uiScale,
    auto_detect_paths_on_startup: settings.autoDetectPathsOnStartup,
    auto_ingest_player_prev: settings.autoIngestPlayerPrev,
    player_log_archive_retention_days: settings.playerLogArchiveRetentionDays,
    player_log_archive_max_files: settings.playerLogArchiveMaxFiles,
    player_log_archive_max_mb: settings.playerLogArchiveMaxMb,
    farming_autosave_minutes: settings.farmingAutosaveMinutes,
    auto_start_farming_sessions: settings.autoStartFarmingSessions,
    farming_session_cap_minutes: settings.farmingSessionCapMinutes,
    view_preferences: settings.viewPreferences,
  };
}

const DEFAULT_EXCLUDED_CHANNELS = [
  "System", "Error", "Emotes", "Action Emotes", "NPC Chatter", "Status", "Combat"
];

// Default UI font. "monospace" preserves the app's original appearance.
export const DEFAULT_FONT_FAMILY = "monospace";

// Selectable UI font options. The `value` is used directly as a CSS
// font-family stack; each falls back to monospace/sans-serif as appropriate.
export const FONT_FAMILY_OPTIONS: { value: string; label: string }[] = [
  { value: "monospace", label: "Monospace (default)" },
  { value: "Consolas, monospace", label: "Consolas" },
  { value: "'Cascadia Code', 'Cascadia Mono', monospace", label: "Cascadia Code" },
  { value: "'Courier New', Courier, monospace", label: "Courier New" },
  { value: "'Lucida Console', monospace", label: "Lucida Console" },
  { value: "Expressway, 'Segoe UI', sans-serif", label: "Expressway" },
  { value: "'Segoe UI', system-ui, sans-serif", label: "Segoe UI" },
  { value: "system-ui, sans-serif", label: "System Default (sans-serif)" },
  { value: "Arial, Helvetica, sans-serif", label: "Arial" },
  { value: "Georgia, 'Times New Roman', serif", label: "Georgia (serif)" },
  { value: "Verdana, Geneva, sans-serif", label: "Verdana" },
];

// Apply the chosen font family to the document by setting the CSS variable
// consumed in base.css (--app-font-family).
export function applyFontFamily(fontFamily: string) {
  const value = fontFamily && fontFamily.trim().length > 0 ? fontFamily : DEFAULT_FONT_FAMILY;
  document.documentElement.style.setProperty("--app-font-family", value);
}

// Default root font size in pixels (matches base.css / Tailwind's 16px base).
// This drives all rem-based sizing, so it scales the whole UI uniformly.
export const DEFAULT_FONT_SIZE = 16;
export const MIN_FONT_SIZE = 12;
export const MAX_FONT_SIZE = 24;

// Apply the chosen root font size to the document by setting the CSS variable
// consumed in base.css (--app-font-size on the html element).
export function applyFontSize(fontSize: number) {
  const clamped = Math.min(MAX_FONT_SIZE, Math.max(MIN_FONT_SIZE, fontSize || DEFAULT_FONT_SIZE));
  document.documentElement.style.setProperty("--app-font-size", `${clamped}px`);
}

// Dashboard widget background opacity, as a percentage (0–100). 100 = fully
// opaque (default), lower values let the page background show through.
export const DEFAULT_WIDGET_OPACITY = 100;
export const MIN_WIDGET_OPACITY = 0;
export const MAX_WIDGET_OPACITY = 100;

// Apply the dashboard widget background opacity by setting the CSS variable
// consumed by DashboardCard (--dashboard-widget-bg-opacity, a 0..1 fraction).
export function applyDashboardWidgetOpacity(percent: number) {
  const clamped = Math.min(MAX_WIDGET_OPACITY, Math.max(MIN_WIDGET_OPACITY, percent ?? DEFAULT_WIDGET_OPACITY));
  document.documentElement.style.setProperty("--dashboard-widget-bg-opacity", `${clamped / 100}`);
}

// Whole-app UI scale as a percentage. 100 = native. Applied via CSS `zoom` on the
// document root, which scales the entire interface uniformly (text, spacing,
// icons, fixed elements) — useful on high-DPI / 4K displays where everything
// renders too large.
export const DEFAULT_UI_SCALE = 100;
export const MIN_UI_SCALE = 50;
export const MAX_UI_SCALE = 200;

export function applyUiScale(percent: number) {
  const clamped = Math.min(MAX_UI_SCALE, Math.max(MIN_UI_SCALE, percent || DEFAULT_UI_SCALE));
  // `zoom` takes a unitless multiplier (1 = 100%).
  (document.documentElement.style as CSSStyleDeclaration & { zoom: string }).zoom = `${clamped / 100}`;
}

// Convert backend format to frontend format
function fromBackendSettings(settings: BackendSettings): AppSettings {
  return {
    logFilePath: settings.log_file_path,
    autoWatchOnStartup: settings.auto_watch_on_startup,
    gameDataPath: settings.game_data_path,
    autoPurgeEnabled: settings.auto_purge_enabled,
    autoPurgeDays: settings.auto_purge_days,
    autoTailChat: settings.auto_tail_chat ?? false,
    autoTailPlayerLog: settings.auto_tail_player_log ?? false,
    dbPath: settings.db_path ?? null,
    excludedChatChannels: settings.excluded_chat_channels ?? DEFAULT_EXCLUDED_CHANNELS,
    devModeEnabled: settings.dev_mode_enabled ?? false,
    autoCheckGameData: settings.auto_check_game_data ?? true,
    autoUpdateGameData: settings.auto_update_game_data ?? true,
    watchRules: settings.watch_rules ?? [],
    setupCompleted: settings.setup_completed ?? false,
    activeCharacterName: settings.active_character_name ?? null,
    activeServerName: settings.active_server_name ?? null,
    autoLoadLastCharacter: settings.auto_load_last_character ?? true,
    autoWatchReports: settings.auto_watch_reports ?? true,
    reportWatchIntervalSeconds: settings.report_watch_interval_seconds ?? 60,
    excludeMaxEnchantedRecipes: settings.exclude_max_enchanted_recipes ?? true,
    marketPriceMode: settings.market_price_mode ?? 'universal',
    itemValuationMode: settings.item_valuation_mode ?? 'highest_market_vendor',
    showRawJsonInDataBrowser: settings.show_raw_json_in_data_browser ?? false,
    showUnobtainableItems: settings.show_unobtainable_items ?? false,
    timestampDisplayMode: (settings.timestamp_display_mode ?? 'local') as 'local' | 'server' | 'utc',
    timezoneOffsetSeconds: settings.timezone_offset_seconds ?? null,
    manualTimezoneOverride: settings.manual_timezone_override ?? null,
    autoStartSurveySessions: settings.auto_start_survey_sessions ?? true,
    use24HourTime: settings.use_24_hour_time ?? true,
    uiFontFamily: settings.ui_font_family ?? DEFAULT_FONT_FAMILY,
    uiFontSize: settings.ui_font_size ?? DEFAULT_FONT_SIZE,
    dashboardWidgetOpacity: settings.dashboard_widget_opacity ?? DEFAULT_WIDGET_OPACITY,
    uiScale: settings.ui_scale ?? DEFAULT_UI_SCALE,
    autoDetectPathsOnStartup: settings.auto_detect_paths_on_startup ?? false,
    autoIngestPlayerPrev: settings.auto_ingest_player_prev ?? true,
    playerLogArchiveRetentionDays: settings.player_log_archive_retention_days ?? 14,
    playerLogArchiveMaxFiles: settings.player_log_archive_max_files ?? 20,
    playerLogArchiveMaxMb: settings.player_log_archive_max_mb ?? 512,
    farmingAutosaveMinutes: settings.farming_autosave_minutes ?? 5,
    autoStartFarmingSessions: settings.auto_start_farming_sessions ?? false,
    farmingSessionCapMinutes: settings.farming_session_cap_minutes ?? 120,
    viewPreferences: settings.view_preferences ?? {},
  };
}

// Default settings
function getDefaultSettings(): AppSettings {
  return {
    logFilePath: "",
    autoWatchOnStartup: false,
    gameDataPath: "",
    autoPurgeEnabled: false,
    autoPurgeDays: 90,
    autoTailChat: false,
    autoTailPlayerLog: false,
    dbPath: null,
    excludedChatChannels: DEFAULT_EXCLUDED_CHANNELS,
    devModeEnabled: false,
    autoCheckGameData: true,
    autoUpdateGameData: true,
    watchRules: [],
    setupCompleted: false,
    activeCharacterName: null,
    activeServerName: null,
    autoLoadLastCharacter: true,
    autoWatchReports: true,
    reportWatchIntervalSeconds: 60,
    excludeMaxEnchantedRecipes: true,
    marketPriceMode: 'universal',
    itemValuationMode: 'highest_market_vendor',
    showRawJsonInDataBrowser: false,
    showUnobtainableItems: false,
    timestampDisplayMode: 'local' as const,
    timezoneOffsetSeconds: null,
    manualTimezoneOverride: null,
    autoStartSurveySessions: true,
    use24HourTime: true,
    uiFontFamily: DEFAULT_FONT_FAMILY,
    uiFontSize: DEFAULT_FONT_SIZE,
    dashboardWidgetOpacity: DEFAULT_WIDGET_OPACITY,
    uiScale: DEFAULT_UI_SCALE,
    autoDetectPathsOnStartup: false,
    autoIngestPlayerPrev: true,
    playerLogArchiveRetentionDays: 14,
    playerLogArchiveMaxFiles: 20,
    playerLogArchiveMaxMb: 512,
    farmingAutosaveMinutes: 5,
    autoStartFarmingSessions: false,
    farmingSessionCapMinutes: 120,
    viewPreferences: {},
  };
}

async function loadSettings(): Promise<AppSettings> {
  try {
    const backendSettings = await invoke<BackendSettings>("load_settings");
    return fromBackendSettings(backendSettings);
  } catch (e) {
    console.error("Failed to load settings:", e);
    return getDefaultSettings();
  }
}

async function saveSettings(settings: AppSettings): Promise<void> {
  try {
    const backendSettings = toBackendSettings(settings);
    await invoke("save_settings", { settings: backendSettings });
  } catch (e) {
    console.error("Failed to save settings:", e);
  }
}

export const useSettingsStore = defineStore("settings", () => {
  const settings = ref<AppSettings>(getDefaultSettings());
  const settingsFilePath = ref<string>("");
  const isLoaded = ref(false);

  // Initialize settings on store creation
  async function initialize() {
    if (isLoaded.value) return;

    settings.value = await loadSettings();
    applyFontFamily(settings.value.uiFontFamily);
    applyFontSize(settings.value.uiFontSize);
    applyDashboardWidgetOpacity(settings.value.dashboardWidgetOpacity);
    applyUiScale(settings.value.uiScale);
    isLoaded.value = true;

    // Get settings file path for user reference
    try {
      settingsFilePath.value = await invoke<string>("get_settings_file_path");
    } catch (e) {
      console.error("Failed to get settings file path:", e);
    }
  }

  async function updateLogFilePath(path: string) {
    settings.value.logFilePath = path;
    await saveSettings(settings.value);
  }

  async function updateAutoWatchOnStartup(enabled: boolean) {
    settings.value.autoWatchOnStartup = enabled;
    await saveSettings(settings.value);
  }

  async function updateGameDataPath(path: string) {
    const oldPath = settings.value.gameDataPath;
    settings.value.gameDataPath = path;
    // Update log path only if it was derived from the old game data path
    // (Windows: same directory). On macOS the paths are independent.
    if (oldPath && settings.value.logFilePath.startsWith(oldPath)) {
      settings.value.logFilePath = path + settings.value.logFilePath.slice(oldPath.length);
    }
    await saveSettings(settings.value);
  }

  async function updateSettings(newSettings: Partial<AppSettings>) {
    settings.value = { ...settings.value, ...newSettings };
    if (newSettings.uiFontFamily !== undefined) {
      applyFontFamily(settings.value.uiFontFamily);
    }
    if (newSettings.uiFontSize !== undefined) {
      applyFontSize(settings.value.uiFontSize);
    }
    if (newSettings.dashboardWidgetOpacity !== undefined) {
      applyDashboardWidgetOpacity(settings.value.dashboardWidgetOpacity);
    }
    if (newSettings.uiScale !== undefined) {
      applyUiScale(settings.value.uiScale);
    }
    await saveSettings(settings.value);
  }

  function getPlayerLogPath(): string {
    return settings.value.gameDataPath + "/Player.log";
  }

  function getChatLogsPath(): string {
    return settings.value.gameDataPath + "/ChatLogs";
  }

  function getReportsPath(): string {
    return settings.value.gameDataPath + "/Reports";
  }

  async function updateAutoPurgeEnabled(enabled: boolean) {
    settings.value.autoPurgeEnabled = enabled;
    await saveSettings(settings.value);
  }

  async function updateAutoPurgeDays(days: number) {
    settings.value.autoPurgeDays = days;
    await saveSettings(settings.value);
  }

  async function updateAutoTailChat(enabled: boolean) {
    settings.value.autoTailChat = enabled;
    await saveSettings(settings.value);
  }

  return {
    settings,
    settingsFilePath,
    isLoaded,
    initialize,
    updateLogFilePath,
    updateAutoWatchOnStartup,
    updateGameDataPath,
    updateSettings,
    getPlayerLogPath,
    getChatLogsPath,
    getReportsPath,
    updateAutoPurgeEnabled,
    updateAutoPurgeDays,
    updateAutoTailChat,
  };
});

if (import.meta.hot) {
  import.meta.hot.accept(acceptHMRUpdate(useSettingsStore, import.meta.hot))
}
