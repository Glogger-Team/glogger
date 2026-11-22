// TypeScript types for the Farming Calculator feature
// (historic loot/drop-rate database + corpse-extract details; live
// sessions are frozen — the Combat view covers live kills/deaths/XP).

// Per-corpse butchering detail for an item, from the `get_corpse_extract_details`
// backend command (lifetime, persisted) — used by the History tab hover.
export interface ExtractDetail {
  corpse_name: string | null
  skill: string
  times: number
  total_quantity: number
  skill_level: number | null
  equipment_bonus: number | null
  anatomy_family: string | null
  anatomy_level: number | null
}

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

export interface EnemySearchResult {
  enemy_name: string
  zone: string | null   // internal area key (null = unknown zone)
  total_kills: number
  distinct_loot_items: number
}

export interface ItemSearchResult {
  item_name: string
  total_quantity: number
  distinct_enemies: number
}

export interface HarvestSearchResult {
  item_name: string
  total_quantity: number
  distinct_corpses: number
  total_extracts: number
}

export interface IngestResult {
  kills_added: number
  loot_added: number
  already_ingested: boolean
}

export interface ItemDropSource {
  enemy_name: string
  zone: string | null   // internal area key (null = unknown zone)
  total_kills: number
  times_dropped: number
  total_quantity: number
  drop_rate: number
}

export interface ImportSummary {
  source_label: string
  enemies_imported: number
  loot_rows_imported: number
}

export interface ImportedSource {
  source_label: string
  display_name: string
  imported_at: string
  enemy_count: number
}

// One row of the per-item drop breakdown shown in the hover popover.
export interface ItemDropBreakdownRow {
  enemyName: string
  // This session
  sessionQuantity: number
  sessionDrops: number
  sessionKills: number
  // All-time (from DB) — null until stats load
  allTimeKills: number | null
  allTimeDropRate: number | null      // 0..1, times_dropped / total_kills
  lootTableSharePct: number | null    // item's share of this enemy's full loot table, by quantity
  // Extract-mode harvest conditions (skinning/butchering only)
  skill?: string             // "Skinning" | "Butchering"
  skillLevel?: number
  equipmentBonus?: number
  anatomyFamily?: string
  anatomyLevel?: number
}

// === Historical (loaded from DB) ===

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
