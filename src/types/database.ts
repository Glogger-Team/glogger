// Database statistics and admin types
//
// Byte-identical shapes are re-exported from the generated Specta bindings
// (`admin` / `character` / `inventory` / `chat` / `settings` / `chatDb`
// `*.specta.ts`). The hand definitions that remain differ from the bindings
// (`PurgeOptions` optional vs required-null; `MarketPrice*`/`Sale*` prices are
// `number` here but `number | null` in the bindings and vendor/method fields
// are narrow unions here but `string` there; `EventLogRecord` carries an extra
// `event_data` payload the bindings skip; `WatchRule.match_mode`/`source` are
// required here but optional in the bindings; `ChatMessage.from_player` is
// optional here but required in `ChatMessageRow`; `SnapshotStat.value` and
// `InventorySummary.total_value` are `number` here but `number | null` in the
// bindings; `SnapshotActiveQuest.category` is a narrow union here but `string`
// in the bindings; `SnapshotItem` lacks the bindings' newer nullable columns)
// or have no generated counterpart (all survey types, `ChatFilter`).

export type {
  TableSize,
  DatabaseStats,
  PurgeResult,
  CompactResult,
} from './admin.specta'
export type {
  CharacterSnapshotSummary,
  SnapshotSkillLevel,
  SnapshotSkillAbility,
  SnapshotNpcFavor,
  SnapshotRecipeCompletion,
  SnapshotCurrency,
  ImportResult,
  SkillDiff,
  CharacterInfo,
} from './character.specta'
export type {
  InventorySnapshotSummary,
  InventoryImportResult,
  CrossCharacterItem,
} from './inventory.specta'
export type {
  ScanResult,
  ChannelStat,
  WatchRuleTriggered,
} from './chat.specta'
export type { ConditionMatch, WatchCondition } from './settings.specta'
export type { WatchNotifyConfig } from './settings.specta'
export type {
  ChatItemLinkRow as ChatItemLink,
  ChatDayRow as ChatDay,
} from './chatDb.specta'
import type { ChatItemLinkRow as ChatItemLink } from './chatDb.specta'
import type {
  ConditionMatch,
  WatchCondition,
  WatchNotifyConfig,
} from './settings.specta'

export interface PurgeOptions {
  older_than_days?: number;
  purge_all: boolean;
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
  jsonScope?: 'only' | 'exclude';
}

// Watch rule types

export interface WatchRule {
  id: number;
  name: string;
  enabled: boolean;
  channels: string[] | null;
  match_mode: ConditionMatch;
  conditions: WatchCondition[];
  notify: WatchNotifyConfig;
  source: string | null;
}

// Character import types

export interface SnapshotStat {
  stat_key: string
  value: number
}

export interface SnapshotActiveQuest {
  quest_key: string
  category: 'active' | 'work_order' | 'completed_work_order' | 'completed'
}

// Inventory import types

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

export interface InventorySummary {
  total_items: number
  total_stacks: number
  total_value: number
  unique_items: number
  items_by_vault: Record<string, number>
  items_by_rarity: Record<string, number>
}
