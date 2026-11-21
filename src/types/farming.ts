// TypeScript types for the Farming Calculator feature

// === Active Session State ===
// Frozen: the 10/21 game client stopped loot/XP tracking, so no new live
// sessions accumulate. The Combat view covers live kills/deaths/XP. These
// aliases stay so historic readers keep compiling; the wide session shape is
// intentionally not rebuilt.

export interface FarmingSkillEntry {
  baseline: number
  baselineTnl: number
  gained: number
  level: number
  tnl: number
  levelsGained: number
}

export interface FarmingFavorEntry {
  delta: number
}

// Per-item loot tally from a single enemy type, this session.
export interface FarmingKillLoot {
  quantity: number  // total amount of the item looted from this enemy
  drops: number     // number of distinct corpse pickups that yielded the item
}

export interface FarmingKillEntry {
  count: number
  loot: Record<string, FarmingKillLoot>  // item_name -> loot tally from this enemy type
}

// Per-item extract tally from skinning/butchering a corpse, this session.
export interface FarmingExtractLoot {
  quantity: number  // total amount extracted from this enemy type
  drops: number     // number of distinct extract events
  skill: string     // "Butchering" | "Skinning"
  // Harvest conditions (latest observed this session), parsed from Player.log.
  skillLevel?: number       // Butchering/Skinning level at harvest time
  equipmentBonus?: number   // "+N skill bonus from equipment"
  anatomyFamily?: string    // e.g. "Canines" — the monster's anatomy family
  anatomyLevel?: number     // your anatomy level for that family
}

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

// === Loot drop breakdown (session tab item popover) ===

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

// Frozen alias: the wide live-session object is gone (see note at top), but
// historic code paths still name it. Kept minimal on purpose.
export type FarmingSession = Record<string, unknown>;

// === Activity Log ===
// Frozen: the live session log is gone; Combat view keeps its own log type.

export type FarmingLogKind =
  | 'session-start'
  | 'item-gained'
  | 'item-lost'
  | 'xp-gain'
  | 'level-up'
  | 'favor-change'
  | 'vendor-sale'
  | 'enemy-killed'
  | 'session-end'

export interface FarmingLogEntry {
  kind: FarmingLogKind
  timestamp: string
  label: string
  detail?: string
}

// === Persistence (save to DB) ===
// Frozen: no new rows are written (Combat view is read-only over old rows),
// but the invoke payloads in DatabaseTab/import paths still name the shape.

export interface SaveFarmingSessionInput {
  /** When set, update this existing session row instead of inserting a new one
   *  (used by the periodic auto-save of an in-progress session). */
  session_id?: number | null
  name: string
  notes: string
  start_time: string
  end_time: string | null
  elapsed_seconds: number
  total_paused_seconds: number
  vendor_gold: number
  skills: Array<{ skill_id: number; skill_name: string; xp_gained: number; levels_gained: number }>
  items: Array<{ item_name: string; net_quantity: number }>
  favors: Array<{ npc_key: string; npc_name: string; delta: number }>
  kills: Array<{ enemy_name: string; kill_count: number }>
  character_name?: string | null
  server_name?: string | null
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
