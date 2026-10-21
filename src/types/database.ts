// Database statistics and admin types

export interface TableSize {
  name: string;
  size_bytes: number;
}

export interface DatabaseStats {
  total_size_bytes: number;
  cdn_size_bytes: number;
  player_data_size_bytes: number;
  free_bytes: number;
  market_prices_count: number;
  sales_history_count: number;
  survey_sessions_count: number;
  event_log_count: number;
  item_transactions_count: number;
  chat_messages_count: number;
  largest_tables: TableSize[];
}

export interface PurgeOptions {
  older_than_days?: number;
  purge_all: boolean;
}

export interface PurgeResult {
  market_prices_deleted: number;
  sales_deleted: number;
  survey_sessions_deleted: number;
  events_deleted: number;
  item_transactions_deleted: number;
  chat_messages_deleted: number;
  bytes_reclaimed: number;
}

export interface CompactResult {
  bytes_before: number;
  bytes_after: number;
  bytes_reclaimed: number;
}

// Market price types
export interface MarketPriceInput {
  item_id: number;
  price: number;
  quantity: number;
  vendor_type: 'bazaar' | 'player_vendor' | 'work_order';
  vendor_name?: string;
  notes?: string;
}

export interface MarketPriceRecord {
  id: number;
  item_id: number;
  price: number;
  quantity: number;
  vendor_type: string;
  vendor_name?: string;
  observed_at: string;
  notes?: string;
}

// Sales history types
export interface SaleInput {
  item_id: number;
  quantity: number;
  sale_price: number;
  sale_method: 'vendor' | 'bazaar' | 'trade' | 'consignment';
  buyer_name?: string;
  notes?: string;
}

export interface SaleRecord {
  id: number;
  item_id: number;
  quantity: number;
  sale_price: number;
  sale_method: string;
  buyer_name?: string;
  sold_at: string;
  notes?: string;
}

// Event log types
export interface LogEventInput {
  event_type: string;
  event_data: any;
}

export interface EventLogRecord {
  id: number;
  event_type: string;
  event_data: any;
  created_at: string;
}

// Survey session stats types
export interface SaveSessionStatsInput {
  start_time: string;
  end_time: string | null;
  maps_started: number;
  surveys_located: number;
  surveys_completed: number;
  surveying_xp_gained: number;
  mining_xp_gained: number;
  geology_xp_gained: number;
  total_revenue: number;
  total_cost: number;
  total_profit: number;
  profit_per_hour: number;
  elapsed_seconds: number;
  is_manual: boolean;
}

export interface HistoricalSession {
  id: number;
  start_time: string;
  end_time: string | null;
  maps_started: number;
  surveys_completed: number;
  total_completions: number;
  total_revenue: number;
  total_cost: number;
  total_profit: number;
  profit_per_hour: number;
  elapsed_seconds: number;
  speed_bonus_count: number;
  survey_types_used: string | null;
  maps_used_summary: string | null;
  name: string;
  notes: string;
  surveying_xp_gained: number;
  mining_xp_gained: number;
  geology_xp_gained: number;
}

// Survey event types
export interface LogSurveyEventInput {
  timestamp: string;
  session_id: number | null;
  event_type: 'session_start' | 'completed';
  map_type: string | null;
  survey_type: string | null;
  speed_bonus_earned: boolean;
}

export interface SurveyEventRecord {
  id: number;
  timestamp: string;
  session_id: number | null;
  event_type: string;
  map_type: string | null;
  survey_type: string | null;
  speed_bonus_earned: boolean;
  created_at: string;
}

// Survey loot item types
export interface LogSurveyLootItemInput {
  event_id: number;
  item_id: number | null;
  item_name: string;
  quantity: number;
  is_speed_bonus: boolean;
  is_primary: boolean;
}

export interface SurveyLootItemRecord {
  id: number;
  event_id: number;
  item_id: number | null;
  item_name: string;
  quantity: number;
  is_speed_bonus: boolean;
  is_primary: boolean;
  obtained_at: string;
}

// Survey analytics types
export interface SpeedBonusStats {
  total_surveys: number;
  speed_bonus_count: number;
  speed_bonus_rate: number;
  total_bonus_items: number;
  unique_bonus_items: number;
}

export interface LootBreakdownEntry {
  item_name: string;
  item_id: number | null;
  total_quantity: number;
  primary_quantity: number;
  bonus_quantity: number;
  times_received: number;
}

export interface SurveyTypeMetrics {
  survey_type: string;
  total_completed: number;
  speed_bonus_count: number;
  speed_bonus_rate: number;
  total_items: number;
  total_bonus_items: number;
  avg_items_per_survey: number;
}

// Zone-level survey analytics types (from get_zone_analytics command)
export interface SpeedBonusItemStats {
  item_name: string;
  total_quantity: number;
  times_seen: number;
  total_procs: number;
  min_per_proc: number;
  max_per_proc: number;
  avg_per_proc: number;
}

export interface CategorySpeedBonusStats {
  category: string;
  total_surveys: number;
  speed_bonus_count: number;
  speed_bonus_rate: number;
  avg_bonus_value: number;
  item_stats: SpeedBonusItemStats[];
}

export interface SurveyItemStats {
  item_name: string;
  total_quantity: number;
  times_seen: number;
  min_per_completion: number;
  max_per_completion: number;
  avg_per_completion: number;
}

export interface SurveyTypeAnalytics {
  survey_type: string;
  category: string;
  crafting_cost: number;
  total_completed: number;
  item_stats: SurveyItemStats[];
}

export interface ZoneAnalytics {
  zone: string;
  speed_bonus_stats: CategorySpeedBonusStats[];
  survey_type_stats: SurveyTypeAnalytics[];
}

export interface SurveyImportInfo {
  id: number;
  label: string;
  source_player: string | null;
  session_count: number;
  event_count: number;
  imported_at: string;
}

// Chat message types
export interface ChatItemLink {
  raw_text: string;
  item_name: string;
  item_id: number | null;
}

export interface ChatMessage {
  id: number;
  timestamp: string;
  channel: string | null;
  sender: string | null;
  message: string;
  is_system: boolean;
  from_player?: boolean | null;
  item_links?: ChatItemLink[];
}

export interface ChatFilter extends Record<string, unknown> {
  channel?: string | null;
  sender?: string | null;
  searchText?: string | null;
  startTime?: string | null;
  endTime?: string | null;
  hasItemLinks?: boolean;
  itemName?: string;
  tellPartner?: string;
  limit?: number;
  offset?: number;
  sortOrder?: 'asc' | 'desc';
}

export interface ScanResult {
  files_processed: number;
  messages_imported: number;
}

// Watch rule types

export type ConditionMatch = 'All' | 'Any';

export interface WatchRule {
  id: number;
  name: string;
  enabled: boolean;
  channels: string[] | null;
  match_mode: ConditionMatch;
  conditions: WatchCondition[];
  notify: WatchNotifyConfig;
}

export interface WatchCondition {
  type: 'ContainsText' | 'ContainsItemLink' | 'FromSender';
  value: string;
}

export interface WatchNotifyConfig {
  sound: boolean;
  toast: boolean;
  highlight: boolean;
}

export interface WatchRuleTriggered {
  rule_id: number;
  rule_name: string;
  notify: WatchNotifyConfig;
  channel: string | null;
  sender: string | null;
  message: string;
  timestamp: string;
}

export interface ChannelStat {
  channel: string;
  count: number;
}

// Chat day navigation
export interface ChatDay {
  day: string;
  count: number;
}

// Character import types

export interface CharacterSnapshotSummary {
  id: number
  character_name: string
  server_name: string
  snapshot_timestamp: string
  race: string
  import_date: string
  skill_count: number
}

export interface SnapshotSkillLevel {
  skill_name: string
  level: number
  bonus_levels: number
  xp_toward_next: number
  xp_needed_for_next: number
}

export interface SnapshotNpcFavor {
  npc_key: string
  favor_level: string
  favor: number | null
  councils_spent_at_vendor: number | null
  vendor_reset_time: string | null
  player_notes: string | null
}

export interface SnapshotRecipeCompletion {
  recipe_key: string
  completions: number
}

export interface SnapshotStat {
  stat_key: string
  value: number
}

export interface SnapshotCurrency {
  currency_key: string
  amount: number
}

export interface SnapshotActiveQuest {
  quest_key: string
  category: 'active' | 'work_order' | 'completed_work_order' | 'completed'
}

export interface ImportResult {
  character_name: string
  server_name: string
  snapshot_timestamp: string
  skills_imported: number
  npcs_imported: number
  recipes_imported: number
  stats_imported: number
  currencies_imported: number
  quests_imported: number
  was_duplicate: boolean
}

export interface SkillDiff {
  skill_name: string
  old_level: number
  new_level: number
  level_change: number
  old_xp: number
  new_xp: number
}

export interface CharacterInfo {
  character_name: string
  server_name: string
  latest_snapshot: string
  snapshot_count: number
}

// Inventory import types

export interface InventorySnapshotSummary {
  id: number
  character_name: string
  server_name: string
  snapshot_timestamp: string
  import_date: string
  item_count: number
}

export interface SnapshotItem {
  id: number
  type_id: number
  storage_vault: string
  is_in_inventory: boolean
  stack_size: number
  value: number | null
  item_name: string
  rarity: string | null
  slot: string | null
  level: number | null
  is_crafted: boolean
  crafter: string | null
  durability: number | null
  craft_points: number | null
  uses_remaining: number | null
  transmute_count: number | null
  attuned_to: string | null
  tsys_powers: string | null
  tsys_imbue_power: string | null
  tsys_imbue_power_tier: number | null
  pet_husbandry_state: string | null
}

export interface InventoryImportResult {
  character_name: string
  server_name: string
  snapshot_timestamp: string
  items_imported: number
  was_duplicate: boolean
}

export interface InventorySummary {
  total_items: number
  total_stacks: number
  total_value: number
  unique_items: number
  items_by_vault: Record<string, number>
  items_by_rarity: Record<string, number>
}

/** One item from a character's latest inventory snapshot, tagged with its owner.
 *  Returned by `get_all_character_items` for the cross-character storage search. */
export interface CrossCharacterItem {
  character_name: string
  server_name: string
  item_name: string
  storage_vault: string
  is_in_inventory: boolean
  stack_size: number
  type_id: number
  rarity: string | null
  value: number | null
}
