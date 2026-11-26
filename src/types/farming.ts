// TypeScript types for the Farming Calculator feature
// (historic loot/drop-rate database + corpse-extract details; live
// sessions are frozen — the Combat view covers live kills/deaths/XP).
//
// Byte-identical shapes are re-exported from the generated Specta bindings
// (`killTracking.specta.ts`, `farming.specta.ts`). The hand definitions that
// remain differ from the bindings: `EnemyLootStat.drop_rate`,
// `EnemyKillStats` (via its loot), and `ItemDropSource.drop_rate` are
// `number` here but `number | null` in the bindings. `DatabaseScope` has no
// generated counterpart.

export type {
  ExtractDetail,
  EnemySearchResult,
  ItemSearchResult,
  HarvestSearchResult,
  ImportSummary,
  ImportedSource,
} from './killTracking.specta'

// === Loot drop breakdown (hover popover) ===

// All-time per-enemy loot stats returned by the `get_enemy_kill_stats` backend command.
export interface EnemyLootStat {
  item_name: string
  total_quantity: number
  times_dropped: number
  drop_rate: number  // 0..1, times_dropped / total_kills (all-time)
}

export interface EnemyKillStats {
  enemy_name: string
  total_kills: number
  loot: EnemyLootStat[]
}

// === Drop-rate database (search, scope, import/export) ===

export type DatabaseScope = 'mine' | 'imported' | 'combined'

export interface ItemDropSource {
  enemy_name: string
  zone: string | null   // internal area key (null = unknown zone)
  total_kills: number
  times_dropped: number
  total_quantity: number
  drop_rate: number
}

// === Historical (loaded from DB) ===
//
// Stays hand-written: `favors[].delta` is `number` here but `number | null`
// in the bindings (`FarmingFavorRecord`), so re-exporting would widen every
// favor-delta consumer to null checks (HistoricalTab renders `fav.delta`
// directly today).
export interface HistoricalFarmingSession {
  id: number
  name: string
  notes: string
  start_time: string
  end_time: string | null
  elapsed_seconds: number
  total_paused_seconds: number
  vendor_gold: number
  created_at: string
  character_name: string | null
  server_name: string | null
  skills: Array<{ skill_id: number; skill_name: string; xp_gained: number; levels_gained: number }>
  items: Array<{ item_name: string; net_quantity: number }>
  favors: Array<{ npc_key: string; npc_name: string; delta: number }>
  kills: Array<{ enemy_name: string; kill_count: number }>
}
