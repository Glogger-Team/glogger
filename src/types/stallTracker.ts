// TypeScript shapes that mirror the Rust serde structs in
// src-tauri/src/db/stall_tracker_commands.rs and src-tauri/src/stall_aggregations.rs.
//
// Byte-identical shapes are re-exported from the generated Specta bindings
// (`stall.specta.ts`). The hand definitions that remain differ from the
// bindings: every filter/params type is all-optional here but required-nullable
// in the bindings, `sortDir`/`granularity`/`period` are narrow literal unions
// here but plain `string` in the bindings, and `InventoryItem.avg_per_day` /
// `InventoryResult.avg_daily_revenue` are `number` here but `number | null` in
// the bindings. `Granularity` / `TrendsPeriod` have no generated counterpart.
//
// Naming convention:
// - Response types use snake_case because the Rust structs have no
//   `rename_all` attribute (StallEvent, StallStats, RevenueResult, ...).
// - Request/param types use camelCase because the Rust structs are
//   `#[serde(rename_all = "camelCase")]` (StallEventsFilters, StallEventsParams,
//   StallRevenueParams, StallInventoryParams).

export type {
  StallEvent,
  StallEventsPage,
  StallStats,
  StallFilterOptions,
  RevenuePeriod,
  RevenueCell,
  RevenueResult,
  PriceTier,
  StallBaseline,
  TimeseriesLine,
  SalesTimeseriesResult,
  ImportResult,
  ExportResult,
  RecountLine,
  RecountPreview,
} from './stall.specta'
import type { PriceTier } from './stall.specta'

/** Filter shape sent to backend. camelCase to match Rust serde rename. */
export interface StallEventsFilters {
  owner?: string | null;
  action?: string | null;
  player?: string | null;
  item?: string | null;
  dateFrom?: string | null;
  dateTo?: string | null;
  includeIgnored?: boolean | null;
}

/** Params for `get_stall_events`. Filters are flattened into the top level. */
export interface StallEventsParams extends StallEventsFilters {
  sortBy?: string | null;
  sortDir?: 'asc' | 'desc' | null;
  limit?: number | null;
  offset?: number | null;
  forceAction?: string | null;
}

// ── Revenue (aggregations) ──────────────────────────────────────────────

export type Granularity = 'daily' | 'weekly' | 'monthly';

export interface StallRevenueParams {
  owner?: string | null;
  granularity?: Granularity | null;
  dateFrom?: string | null;
  dateTo?: string | null;
  player?: string | null;
  item?: string | null;
}

// ── Inventory (aggregations) ────────────────────────────────────────────

export interface InventoryItem {
  item: string;
  quantity: number;
  price_tiers: PriceTier[];
  estimated_value: number;
  period_sold: number;
  period_revenue: number;
  avg_per_day: number;
  last_sold_at: string | null;
  last_activity_at: string | null;
  /** Most recent priced event seen for this item. Feeds the `~` fallback
   * price when no priced tier survived (added-but-never-priced stock). */
  last_known_price: number | null;
  /** Units priced at `last_known_price` because no priced tier survived
   * (added-but-never-priced stock). 0 when every unit has a real price. */
  fallback_priced_qty: number;
  /** True when the earliest event for this item is not a baseline
   * (`added`/`visible`/`configured`) — the log starts mid-history. */
  history_truncated: boolean;
  // Winning baseline recorded_at ("YYYY-MM-DD HH:MM:SS"), null when uncounted.
  baseline_at: string | null;
  // Surviving log events replayed on top of the baseline. 0 when none / no baseline.
  events_since_baseline: number;
   /** Earliest `event_at` seen for this item (`YYYY-MM-DD HH:MM:SS`). */
   first_event_at: string | null;
 }

export interface InventoryResult {
  items: InventoryItem[];
  /** Distinct dates with activity, **newest-first**. */
  active_dates: string[];
  estimated_value: number;
  total_sold: number;
  avg_daily_revenue: number;
}

export interface StallInventoryParams {
  owner?: string | null;
  periodDays?: number | null;
  /** History window in days; null/<=0 replays the full history. */
  windowDays?: number | null;
}

// ── Opening-stock baseline (manual ground truth) ─────────────────────────

/** Input for `save_stall_baseline`. `quantity: 0` stores a zero row that
 * clears the item (supersedes pre-clear history). */
export interface StallBaselineInput {
  owner?: string | null;
  item?: string | null;
  quantity?: number | null;
  priceUnit?: number | null;
  note?: string | null;
}

// ── Sales time-series (Trends tab) ──────────────────────────────────────

export type TrendsPeriod = '1d' | '3d' | '7d' | '1m' | 'all';

export interface StallTimeseriesParams {
  owner?: string | null;
  period?: TrendsPeriod | null;
  /** Set for single-item mode. Mutually exclusive with `topN`. */
  item?: string | null;
  /** 5 or 10 for top-grossing mode. Both `item`/`topN` unset → Total line. */
  topN?: number | null;
}
