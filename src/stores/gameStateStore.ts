import { defineStore } from 'pinia'
import { ref, computed } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { useSettingsStore } from './settingsStore'
import { useCharacterStore } from './characterStore'
import {
  skillTotalLevel,
  type GameStateSkill,
  type GameStateAttribute,
  type GameStateActiveSkills,
  type GameStateWorld,
  type GameStateInventoryItem,
  type GameStateRecipe,
  type GameStateEquipmentSlot,
  type GameStateFavor,
  type GameStateCurrency,
  type GameStateEffect,
  type GameStateStorageItem,
  type GameStateVendor,
  type StorageVaultDetail,
} from '../types/gameState'
import type { PlayerEvent } from '../types/playerEvents'

// ── Live Inventory Tracking ──────────────────────────────────────────────

export interface LiveInventoryItem {
  instance_id: number
  item_name: string
  item_type_id: number | null
  stack_size: number
  slot_index: number
  added_at: string
  is_new: boolean
}

export type InventoryEventKind = 'added' | 'removed' | 'stack_changed'

export interface InventoryEventLog {
  timestamp: string
  kind: InventoryEventKind
  item_name: string
  detail: string
}

const INVENTORY_EVENT_LOG_MAX = 50

// ── Chat Status Event Types ──────────────────────────────────────────────
// Matches Rust ChatStatusEvent enum (#[serde(tag = "kind")])

export type ChatStatusEvent =
  | { kind: 'ItemGained'; timestamp: string; item_name: string; quantity: number }
  | { kind: 'XpGained'; timestamp: string; skill: string; amount: number }
  | { kind: 'ProdigyXpGained'; timestamp: string; skill: string; amount: number }
  | { kind: 'LevelUp'; timestamp: string; skill: string; level: number; xp: number }
  | { kind: 'CoinsLooted'; timestamp: string; amount: number }
  | { kind: 'CouncilsChanged'; timestamp: string; amount: number }
  | { kind: 'TreasureDistance'; timestamp: string; meters: number }
  | { kind: 'AnatomyResult'; timestamp: string; success: boolean }
  | { kind: 'Summoned'; timestamp: string; item_name: string; quantity: number }
  | {
      kind: 'CombatWisdomEarned'
      timestamp: string
      amount: number
      source_name: string | null
      verb: string
      zone: string | null
    }
  | { kind: 'RouletteResult'; timestamp: string; number: number }
  // Backend-only kinds (WordOfPowerUsed, ItemStudied, ReportSaved) and
  // MilkedBy carry no session-feed payload — the widgets they feed refresh
  // from persisted DB rows via game-state-updated domains, so the handler
  // switch intentionally has no case for them.
  | { kind: 'MilkedBy'; timestamp: string; milker: string }

// ── Currency Estimate ─────────────────────────────────────────────────────
// Matches Rust CurrencyEstimate (db/game_state_commands.rs). Running estimate
// of the council wallet (GOLD): last export's balance + live chat deltas.

export interface CurrencyEstimate {
  currency_name: string
  anchor_amount: number
  anchor_at: string | null
  delta_since: number
  estimated: number
  has_anchor: boolean
}

// ── Combat Wisdom Types ──────────────────────────────────────────────────
// Matches Rust CombatWisdomMonster (db/combat_wisdom_commands.rs).

// ── Roulette Types ───────────────────────────────────────────────────────
// Matches Rust RouletteStats (db/roulette_commands.rs).

export interface RouletteNumberCount {
  number: number
  count: number
}

export interface RouletteStats {
  total: number
  counts: RouletteNumberCount[]
  last_spun_at: string | null
  last_number: number | null
  recent: number[]
}

// ── Arena Types ──────────────────────────────────────────────────────────
// Matches Rust ArenaStats (db/arena_commands.rs).

export interface ArenaFighter {
  name: string
  wins: number
  losses: number
  win_pct: number
}

export interface ArenaHeadToHead {
  fighter: string
  opponent: string
  wins: number
  losses: number
}

export interface ArenaRecentMatch {
  fought_at: string
  fighter_a: string
  fighter_b: string
  winner: string
}

export interface ArenaBettingSummary {
  total: number
  won: number
  win_pct: number
  total_wagered: number
  total_won: number
  net_profit: number
}

export interface ArenaStats {
  total_matches: number
  fighters: ArenaFighter[]
  head_to_head: ArenaHeadToHead[]
  recent: ArenaRecentMatch[]
  betting: ArenaBettingSummary
}

export interface CombatWisdomMonster {
  name: string
  verb: string
  last_earned_ms: number
  count: number
  total_amount: number
  min_gap_secs: number | null
}

// ── Activity Feed Types ──────────────────────────────────────────────────

export interface ActivityEntry {
  timestamp: string
  label: string
  amount: number
  detail?: string
}

const ACTIVITY_LOG_MAX = 30

// ── Session Skill Tracking ────────────────────────────────────────────────

/** Per-skill session tracking — XP deltas accumulated since first seen */
export interface SkillSessionData {
  skillType: string
  currentLevel: number
  tnl: number
  xpGained: number
  levelsGained: number
  firstTimestamp: string
  lastTimestamp: string
  firstXp: number
}

function timestampToSeconds(ts: string): number {
  const [h, m, s] = ts.split(':').map(Number)
  return h * 3600 + m * 60 + s
}

// ── Store ─────────────────────────────────────────────────────────────────

export const useGameStateStore = defineStore('gameState', () => {
  const settingsStore = useSettingsStore()

  // ── Persisted State (from DB) ─────────────────────────────────────────
  const skills = ref<GameStateSkill[]>([])
  const attributes = ref<GameStateAttribute[]>([])
  const activeSkills = ref<GameStateActiveSkills | null>(null)
  const world = ref<GameStateWorld>({ weather: null, combat: null, mount: null, area: null })
  const inventory = ref<GameStateInventoryItem[]>([])
  const recipes = ref<GameStateRecipe[]>([])
  const equipment = ref<GameStateEquipmentSlot[]>([])
  const favor = ref<GameStateFavor[]>([])
  const currencies = ref<GameStateCurrency[]>([])
  const effects = ref<GameStateEffect[]>([])
  const storage = ref<GameStateStorageItem[]>([])
  const vendor = ref<GameStateVendor[]>([])

  // CDN vault metadata (loaded once)
  const storageVaults = ref<StorageVaultDetail[]>([])

  const loading = ref(false)
  const initialized = ref(false)

  // Set to true once the startup sequence is complete.
  // During startup catch-up, character-login events are handled by the
  // startup sequence itself — the listener skips heavy work until ready.
  const startupComplete = ref(false)

  // ── Session Skill State (in-memory, not persisted) ────────────────────
  const sessionSkills = ref<Record<string, SkillSessionData>>({})

  // ── Session XP-rate tracking (driven by [Status] chat events) ─────────
  // Separate from sessionSkills (which tracks per-skill XP from skill-update
  // events). This aggregates combat vs prodigy XP for the XP/hr widget, using
  // wall-clock time so the rate stays live during play.
  const xpRateSession = ref<{
    combatXp: number
    prodigyXp: number
    firstSeenMs: number | null
    lastSeenMs: number | null
  }>({ combatXp: 0, prodigyXp: 0, firstSeenMs: null, lastSeenMs: null })

  // ── Session Combat Wisdom tracking (driven by [Status] chat events) ────
  // Per-session list of Combat Wisdom awards earned (which monster, how much,
  // when). Resets on character login, like xpRateSession. Per-monster reuse
  // cooldowns are sourced separately from the persisted DB (fetchCombatWisdom).
  const combatWisdomSession = ref<{
    total: number
    earns: Array<{
      name: string | null
      amount: number
      verb: string
      zone: string | null
      atMs: number
    }>
  }>({ total: 0, earns: [] })

  /** Persisted per-monster Combat Wisdom cooldown data (from the DB). */
  const combatWisdomMonsters = ref<CombatWisdomMonster[]>([])

  /** Persisted casino roulette outcome history (from the DB). Drives the
   *  Roulette widget's outcome-percentage pie chart. */
  const rouletteStats = ref<RouletteStats>({
    total: 0,
    counts: [],
    last_spun_at: null,
    last_number: null,
    recent: [],
  })

  /** Persisted casino arena fight history (from the DB). Drives the Arena
   *  widget's fighter rankings + head-to-head matchup matrix. */
  const arenaStats = ref<ArenaStats>({
    total_matches: 0,
    fighters: [],
    head_to_head: [],
    recent: [],
    betting: {
      total: 0,
      won: 0,
      win_pct: 0,
      total_wagered: 0,
      total_won: 0,
      net_profit: 0,
    },
  })

  // ── Council-wallet estimate (anchored on last export + live deltas) ───
  const currencyEstimate = ref<CurrencyEstimate | null>(null)

  /** Display names of combat skills (from CDN). Used to filter the
   *  non-prodigy combat XP line. Loaded lazily on first loadAll. */
  const combatSkillNames = ref<Set<string>>(new Set())

  /** XP required to gain one Prodigy level (per the Prodigy Potential wiki). */
  const PRODIGY_XP_PER_LEVEL = 250_000_000

  // ── Tracked Skills (persisted per character) ────────────────────────────
  const trackedSkillNames = ref<string[]>([])

  // ── Live Inventory State (in-memory, not persisted) ─────────────────
  const liveItemMap = ref<Map<number, LiveInventoryItem>>(new Map())
  const liveEventLog = ref<InventoryEventLog[]>([])

  // ── Clocks (computed from wall clock, session-only) ────────────────
  // Server time = US Eastern. Game time = 12 game days per real day
  // (1 game hour = 5 real minutes), anchored to Eastern midnight.
  const serverTime = ref('--:--')
  const gameTime = ref('--:--')

  let _clockInterval: ReturnType<typeof setInterval> | null = null

  function _tickClocks() {
    const now = new Date()

    // Server time (Eastern)
    serverTime.value = now.toLocaleTimeString('en-US', {
      hour: '2-digit', minute: '2-digit', hour12: false,
      timeZone: 'America/New_York',
    })

    // Game time: 12× real speed anchored to Eastern midnight
    const eastern = new Date(now.toLocaleString('en-US', { timeZone: 'America/New_York' }))
    const realMinutes = eastern.getHours() * 60 + eastern.getMinutes() + eastern.getSeconds() / 60
    const gameMinutesInDay = (realMinutes * 12) % 1440
    const gh = Math.floor(gameMinutesInDay / 60)
    const gm = Math.floor(gameMinutesInDay % 60)
    gameTime.value = `${String(gh).padStart(2, '0')}:${String(gm).padStart(2, '0')}`
  }

  function startClocks() {
    _tickClocks()
    if (!_clockInterval) {
      _clockInterval = setInterval(_tickClocks, 1000)
    }
  }

  // Start immediately when the store is created
  startClocks()

  // ── Activity Feeds (in-memory, session-only) ────────────────────────
  const itemsIncoming = ref<ActivityEntry[]>([])
  const itemsOutgoing = ref<ActivityEntry[]>([])
  const councilChanges = ref<ActivityEntry[]>([])
  const currencyChanges = ref<ActivityEntry[]>([])
  const favorChanges = ref<ActivityEntry[]>([])

  function pushActivity(feed: typeof itemsIncoming, entry: ActivityEntry) {
    feed.value.unshift(entry)
    if (feed.value.length > ACTIVITY_LOG_MAX) feed.value.length = ACTIVITY_LOG_MAX
  }

  // ── Computed: Persisted State ─────────────────────────────────────────

  /** Skills indexed by name for O(1) lookup */
  const skillsByName = computed(() => {
    const map: Record<string, GameStateSkill> = {}
    for (const s of skills.value) map[s.skill_name] = s
    return map
  })

  /** Items carried in inventory (merges DB inventory + live inventory, excludes storage) */
  const inventoryItemCounts = computed(() => {
    const counts: Record<string, number> = {}
    for (const item of inventory.value) {
      counts[item.item_name] = (counts[item.item_name] ?? 0) + item.stack_size
    }
    for (const item of liveItemMap.value.values()) {
      counts[item.item_name] = (counts[item.item_name] ?? 0) + item.stack_size
    }
    return counts
  })

  /** Items held on the player (inventory + saddlebag when we track it) */
  const heldItemCounts = computed(() => {
    // Currently the same as inventoryItemCounts — when saddlebag tracking
    // is added, merge it in here.
    return inventoryItemCounts.value
  })

  /** Total owned count per item name (inventory + storage) */
  const ownedItemCounts = computed(() => {
    const counts: Record<string, number> = { ...inventoryItemCounts.value }
    for (const item of storage.value) {
      counts[item.item_name] = (counts[item.item_name] ?? 0) + item.stack_size
    }
    return counts
  })

  /** Attributes indexed by name for O(1) lookup */
  const attributesByName = computed(() => {
    const map: Record<string, number> = {}
    for (const a of attributes.value) map[a.attribute_name] = a.value
    return map
  })

  /** Recipes indexed by ID for O(1) lookup */
  const recipesById = computed(() => {
    const map: Record<number, GameStateRecipe> = {}
    for (const r of recipes.value) map[r.recipe_id] = r
    return map
  })

  /** Recipe completions indexed by string key (e.g. "Recipe_12345") for compatibility with CDN recipe data */
  const recipeCompletions = computed(() => {
    const map: Record<string, number> = {}
    for (const r of recipes.value) {
      map[`Recipe_${r.recipe_id}`] = r.completion_count
    }
    return map
  })

  /** Set of known recipe keys — known AND crafted at least once (completion_count > 0) */
  const knownRecipeKeys = computed(() => {
    const set = new Set<string>()
    for (const r of recipes.value) {
      if (r.completion_count > 0) set.add(`Recipe_${r.recipe_id}`)
    }
    return set
  })

  /** Set of learned recipe keys — a game_state_recipes row exists iff the cook knows it (completion_count stays 0 until the first craft) */
  const learnedRecipeKeys = computed(() => {
    const set = new Set<string>()
    for (const r of recipes.value) set.add(`Recipe_${r.recipe_id}`)
    return set
  })

  /** NPC favor indexed by npc_key for O(1) lookup */
  const favorByNpc = computed(() => {
    const map: Record<string, GameStateFavor> = {}
    for (const f of favor.value) map[f.npc_key] = f
    return map
  })

  /** Currencies indexed by currency_name for O(1) lookup */
  const currenciesByName = computed(() => {
    const map: Record<string, GameStateCurrency> = {}
    for (const c of currencies.value) map[c.currency_name] = c
    return map
  })

  /** Active effects indexed by instance ID for O(1) lookup */
  const effectsById = computed(() => {
    const map: Record<number, GameStateEffect> = {}
    for (const e of effects.value) map[e.effect_instance_id] = e
    return map
  })

  /** Named effects only (those with display names resolved) */
  const namedEffects = computed(() => effects.value.filter(e => e.effect_name !== null))

  /** Vendor state indexed by npc_key for O(1) lookup */
  const vendorByNpc = computed(() => {
    const map: Record<string, GameStateVendor> = {}
    for (const v of vendor.value) map[v.npc_key] = v
    return map
  })

  /** Storage items grouped by vault_key */
  const storageByVault = computed(() => {
    const map: Record<string, GameStateStorageItem[]> = {}
    for (const item of storage.value) {
      if (!map[item.vault_key]) map[item.vault_key] = []
      map[item.vault_key].push(item)
    }
    return map
  })

  /** Vault metadata indexed by key for O(1) lookup */
  const storageVaultsByKey = computed(() => {
    const map: Record<string, StorageVaultDetail> = {}
    for (const v of storageVaults.value) map[v.key] = v
    return map
  })

  // Favor tier ordering (highest to lowest)
  const FAVOR_TIER_ORDER = [
    'SoulMates', 'LikeFamily', 'BestFriends', 'CloseFriends',
    'Friends', 'Comfortable', 'Neutral', 'Despised'
  ]

  /** Get the maximum possible slots for a vault (highest tier or fixed count) */
  function getVaultMaxPossibleSlots(vault: StorageVaultDetail): number | null {
    // Fixed slot count (skip 0 — NPC vaults have num_slots=0 in CDN data)
    if (vault.num_slots != null && vault.num_slots > 0) return vault.num_slots
    if (vault.slot_attribute) {
      // For attribute-based vaults, we don't know the theoretical max easily
      return vault.num_slots_script_atomic_max ?? null
    }
    if (vault.levels) {
      // Highest slot count from any favor tier
      let max = 0
      for (const slots of Object.values(vault.levels)) {
        if (slots > max) max = slots
      }
      return max > 0 ? max : null
    }
    if (vault.num_slots_script_atomic_max != null) return vault.num_slots_script_atomic_max
    return null
  }

  /** Get the player's currently unlocked slots for a vault based on favor/attributes */
  function getVaultUnlockedSlots(vault: StorageVaultDetail): number | null {
    // Fixed slot count (always fully unlocked)
    if (vault.num_slots != null && vault.num_slots > 0) return vault.num_slots
    if (vault.slot_attribute) {
      return attributesByName.value[vault.slot_attribute] ?? null
    }
    if (vault.levels) {
      // Favor is stored by vault key (e.g., "NPC_Joe") from character reports
      const npcFavor = favorByNpc.value[vault.key]
      if (!npcFavor?.favor_tier) return null
      // Find the highest tier the player qualifies for
      const playerTierIndex = FAVOR_TIER_ORDER.indexOf(npcFavor.favor_tier)
      if (playerTierIndex === -1) return null
      let unlocked = 0
      for (const [tier, slots] of Object.entries(vault.levels)) {
        const tierIndex = FAVOR_TIER_ORDER.indexOf(tier)
        if (tierIndex !== -1 && tierIndex >= playerTierIndex && slots > 0) {
          unlocked = Math.max(unlocked, slots)
        }
      }
      return unlocked > 0 ? unlocked : null
    }
    if (vault.num_slots_script_atomic_max != null) return vault.num_slots_script_atomic_max
    return null
  }

  /** Get the player's favor tier for a vault's NPC (null if non-NPC vault or no favor data) */
  function getVaultFavorTier(vault: StorageVaultDetail): string | null {
    // Favor is stored by vault key (e.g., "NPC_Joe") from character reports
    return favorByNpc.value[vault.key]?.favor_tier ?? null
  }

  // ── Computed: Session Skills ──────────────────────────────────────────

  /** List of session skill entries (for iteration in components) */
  const sessionSkillList = computed(() => Object.values(sessionSkills.value))

  // ── Computed: Live Inventory ────────────────────────────────────────

  /** Live inventory items sorted by slot index */
  const liveItems = computed<LiveInventoryItem[]>(() => {
    return [...liveItemMap.value.values()].sort((a, b) => a.slot_index - b.slot_index)
  })

  const liveItemCount = computed(() => liveItemMap.value.size)

  const liveTotalStacks = computed(() => {
    let total = 0
    for (const item of liveItemMap.value.values()) {
      total += item.stack_size
    }
    return total
  })

  const isLivePopulated = computed(() => liveItemMap.value.size > 0)

  // ── Helpers ───────────────────────────────────────────────────────────

  function getCharacterName(): string | null {
    return settingsStore.settings.activeCharacterName
  }

  function getServerName(): string | null {
    return settingsStore.settings.activeServerName
  }

  // ── Session Skill Methods ─────────────────────────────────────────────

  /** Handle a skill-update event from the backend (same payload as old skillStore) */
  function handleSkillUpdate(payload: {
    skill_type: string
    level: number
    bonus: number
    xp: number
    tnl: number
    timestamp: string
  }) {
    // During startup catch-up, skill updates come from historical log replay
    // and may include data from other characters. Skip session accumulation
    // until the app is fully started and we're in live tailing mode.
    if (!startupComplete.value) return

    const key = payload.skill_type

    if (!sessionSkills.value[key]) {
      sessionSkills.value[key] = {
        skillType: payload.skill_type,
        currentLevel: payload.level,
        tnl: payload.tnl,
        xpGained: 0,
        levelsGained: 0,
        firstTimestamp: payload.timestamp,
        lastTimestamp: payload.timestamp,
        firstXp: payload.xp,
      }
    } else {
      const s = sessionSkills.value[key]
      const prevLevel = s.currentLevel

      s.currentLevel = payload.level
      s.tnl = payload.tnl
      s.lastTimestamp = payload.timestamp

      if (payload.level > prevLevel) {
        // Level-up: add remaining XP in old level + current XP in new level
        s.xpGained += s.tnl - s.firstXp + payload.xp
        s.levelsGained += payload.level - prevLevel
        s.firstXp = payload.xp
      } else if (payload.xp >= s.firstXp) {
        s.xpGained += payload.xp - s.firstXp
        s.firstXp = payload.xp
      }
    }
  }

  /** XP per hour for a given session skill */
  function xpPerHour(skill: SkillSessionData): number {
    const startSec = timestampToSeconds(skill.firstTimestamp)
    const endSec = timestampToSeconds(skill.lastTimestamp)
    const elapsedHours = (endSec - startSec) / 3600
    if (elapsedHours <= 0) return 0
    return Math.round(skill.xpGained / elapsedHours)
  }

  /** Estimated time to next level for a given session skill */
  function timeToNextLevel(skill: SkillSessionData): string {
    const rate = xpPerHour(skill)
    if (rate <= 0) return '—'
    const hoursLeft = skill.tnl / rate
    const totalMinutes = Math.round(hoursLeft * 60)
    if (totalMinutes < 1) return '< 1 min'
    if (totalMinutes < 60) return `~${totalMinutes} min`
    const h = Math.floor(totalMinutes / 60)
    const m = totalMinutes % 60
    return m > 0 ? `~${h}h ${m}m` : `~${h}h`
  }

  /** Reset session skill tracking (e.g., on manual log parse or session restart) */
  function resetSessionSkills() {
    sessionSkills.value = {}
    resetXpRateSession()
    combatWisdomSession.value = { total: 0, earns: [] }
  }

  /** Reset the combat/prodigy XP-rate accumulator. */
  function resetXpRateSession() {
    xpRateSession.value = {
      combatXp: 0,
      prodigyXp: 0,
      firstSeenMs: null,
      lastSeenMs: null,
    }
  }

  /** Fetch persisted per-monster Combat Wisdom cooldown data. */
  async function fetchCombatWisdomMonsters() {
    try {
      combatWisdomMonsters.value = await invoke<CombatWisdomMonster[]>(
        'get_combat_wisdom_monsters',
      )
    } catch (e) {
      console.error('[gameStateStore] Failed to load combat wisdom monsters:', e)
    }
  }

  /** Fetch persisted casino roulette outcome history. */
  async function fetchRouletteStats() {
    try {
      rouletteStats.value = await invoke<RouletteStats>('get_roulette_stats')
    } catch (e) {
      console.error('[gameStateStore] Failed to load roulette stats:', e)
    }
  }

  /** Fetch persisted casino arena fight history. */
  async function fetchArenaStats() {
    try {
      arenaStats.value = await invoke<ArenaStats>('get_arena_stats')
    } catch (e) {
      console.error('[gameStateStore] Failed to load arena stats:', e)
    }
  }

  /** Reconcile the council-wallet estimate from the DB (anchor + delta_since).
   *  Call on load/login and after a fresh export re-anchors it. */
  async function fetchCurrencyEstimate() {
    const characterName = getCharacterName()
    const serverName = getServerName()
    if (!characterName || !serverName) return
    try {
      currencyEstimate.value = await invoke<CurrencyEstimate | null>(
        'get_currency_estimate', { characterName, serverName },
      )
    } catch (e) {
      console.error('[gameStateStore] Failed to load currency estimate:', e)
    }
  }

  /** Optimistically apply a live wallet delta so the council figure ticks
   *  without a round-trip. The backend applies the same delta to delta_since;
   *  fetchCurrencyEstimate() reconciles on the next load. No-op until anchored
   *  (mirrors the backend guard) and only while live (post-catch-up). */
  function applyWalletDelta(amount: number) {
    const e = currencyEstimate.value
    if (!e || !e.has_anchor || amount === 0) return
    e.delta_since += amount
    e.estimated += amount
  }

  /** Lazily load the set of combat-skill display names from the CDN. */
  async function loadCombatSkillNames() {
    if (combatSkillNames.value.size > 0) return
    try {
      const skills = await invoke<{ name: string }[]>('get_combat_skills')
      combatSkillNames.value = new Set(skills.map((s) => s.name))
    } catch (e) {
      console.error('[gameStateStore] Failed to load combat skills:', e)
    }
  }

  /** Accrue a live XP gain into the combat or prodigy bucket. */
  function accrueXpRate(kind: 'combat' | 'prodigy', amount: number) {
    const s = xpRateSession.value
    const now = Date.now()
    if (s.firstSeenMs === null) s.firstSeenMs = now
    s.lastSeenMs = now
    if (kind === 'combat') s.combatXp += amount
    else s.prodigyXp += amount
  }

  /** XP/hr over the active window, against a caller-supplied "now" (ms) so the
   *  widget can tick it live. Returns 0 until at least a few seconds elapse. */
  function xpRateOf(kind: 'combat' | 'prodigy', nowMs: number): number {
    const s = xpRateSession.value
    if (s.firstSeenMs === null) return 0
    const elapsedHours = (nowMs - s.firstSeenMs) / 3_600_000
    if (elapsedHours <= 0) return 0
    const total = kind === 'combat' ? s.combatXp : s.prodigyXp
    return Math.round(total / elapsedHours)
  }

  /** Format an hours-to-go value as a compact ETA string. */
  function formatEta(hours: number): string {
    if (!isFinite(hours) || hours <= 0) return '—'
    const totalMinutes = Math.round(hours * 60)
    if (totalMinutes < 1) return '< 1 min'
    if (totalMinutes < 60) return `~${totalMinutes} min`
    const h = Math.floor(totalMinutes / 60)
    const m = totalMinutes % 60
    if (h < 24) return m > 0 ? `~${h}h ${m}m` : `~${h}h`
    const d = Math.floor(h / 24)
    const rh = h % 24
    return rh > 0 ? `~${d}d ${rh}h` : `~${d}d`
  }

  /** ETA to the next prodigy level (one full 250M) at the given prodigy rate. */
  function prodigyEta(prodigyPerHour: number): string {
    if (prodigyPerHour <= 0) return '—'
    return formatEta(PRODIGY_XP_PER_LEVEL / prodigyPerHour)
  }

  // ── Tracked Skills Methods ──────────────────────────────────────────

  async function loadTrackedSkills() {
    const characterName = getCharacterName()
    const serverName = getServerName()
    if (!characterName || !serverName) return

    try {
      const rows = await invoke<{ skill_name: string; sort_order: number }[]>(
        'get_tracked_skills', { characterName, serverName }
      )
      trackedSkillNames.value = rows.map(r => r.skill_name)
    } catch (e) {
      console.error('[gameStateStore] Failed to load tracked skills:', e)
    }
  }

  async function saveTrackedSkills() {
    const characterName = getCharacterName()
    const serverName = getServerName()
    if (!characterName || !serverName) return

    try {
      const skills = trackedSkillNames.value.map((name, i) => ({
        skill_name: name,
        sort_order: i,
      }))
      await invoke('set_tracked_skills', { characterName, serverName, skills })
    } catch (e) {
      console.error('[gameStateStore] Failed to save tracked skills:', e)
    }
  }

  async function trackSkill(skillName: string) {
    if (trackedSkillNames.value.includes(skillName)) return
    trackedSkillNames.value.push(skillName)
    await saveTrackedSkills()
  }

  async function untrackSkill(skillName: string) {
    trackedSkillNames.value = trackedSkillNames.value.filter(n => n !== skillName)
    await saveTrackedSkills()
  }

  function isSkillTracked(skillName: string): boolean {
    return trackedSkillNames.value.includes(skillName)
  }

  // ── Live Inventory Methods ──────────────────────────────────────────

  function pushInventoryEvent(kind: InventoryEventKind, item_name: string, detail: string, timestamp: string) {
    liveEventLog.value.unshift({ timestamp, kind, item_name, detail })
    if (liveEventLog.value.length > INVENTORY_EVENT_LOG_MAX) {
      liveEventLog.value.length = INVENTORY_EVENT_LOG_MAX
    }
  }

  function clearLiveInventory() {
    liveItemMap.value = new Map()
    liveEventLog.value = []
    clearActivityFeeds()
  }

  /** Handle a player-event for inventory tracking */
  function handleInventoryEvent(event: PlayerEvent) {
    // Skip live inventory accumulation during startup catch-up — historical
    // replay may include events from other characters or stale sessions.
    if (!startupComplete.value) return

    switch (event.kind) {
      case 'ItemAdded': {
        const entry: LiveInventoryItem = {
          instance_id: event.instance_id,
          item_name: event.item_name,
          item_type_id: null,
          stack_size: event.initial_quantity ?? 1,
          slot_index: event.slot_index,
          added_at: event.timestamp,
          is_new: event.is_new,
        }
        const newMap = new Map(liveItemMap.value)
        newMap.set(event.instance_id, entry)
        liveItemMap.value = newMap

        if (event.is_new) {
          pushInventoryEvent('added', event.item_name, `Slot ${event.slot_index}`, event.timestamp)
        }
        break
      }

      case 'ItemStackChanged': {
        const newMap = new Map(liveItemMap.value)
        const existing = newMap.get(event.instance_id)

        if (existing) {
          const updated = { ...existing, stack_size: event.new_stack_size }
          if (event.item_type_id && !existing.item_type_id) {
            updated.item_type_id = event.item_type_id
          }
          newMap.set(event.instance_id, updated)

          if (existing.is_new && event.delta !== 0) {
            const sign = event.delta > 0 ? '+' : ''
            pushInventoryEvent('stack_changed', existing.item_name, `${sign}${event.delta} (now ${event.new_stack_size})`, event.timestamp)
          }
        } else {
          const name = event.item_name ?? 'Unknown Item'
          newMap.set(event.instance_id, {
            instance_id: event.instance_id,
            item_name: name,
            item_type_id: event.item_type_id,
            stack_size: event.new_stack_size,
            slot_index: -1,
            added_at: event.timestamp,
            is_new: false,
          })
        }

        liveItemMap.value = newMap
        break
      }

      case 'ItemDeleted': {
        const newMap = new Map(liveItemMap.value)
        const removed = newMap.get(event.instance_id)
        newMap.delete(event.instance_id)
        liveItemMap.value = newMap

        if (removed) {
          const contextLabel = event.context === 'StorageTransfer' ? 'stored'
            : event.context === 'VendorSale' ? 'sold'
            : event.context === 'Consumed' ? 'consumed'
            : 'removed'
          pushInventoryEvent('removed', removed.item_name, contextLabel, event.timestamp)
        }
        break
      }
    }
  }

  // ── Activity Feed: Player Events ─────────────────────────────────────

  /** Route player events into the appropriate activity feed */
  function handlePlayerActivityEvent(event: PlayerEvent) {
    // Skip session feed accumulation during startup catch-up — historical
    // replay may include events from other characters or stale sessions.
    if (!startupComplete.value) return

    switch (event.kind) {
      // Items incoming is handled exclusively by chat status events
      // (ItemGained/Summoned) to avoid double-counting — Player.log fires
      // both ItemAdded and ItemStackChanged for the same pickup.

      case 'ItemStackChanged':
        // Only track negative deltas (outgoing) from Player.log.
        // Positive deltas are covered by chat status ItemGained events.
        if (event.delta < 0) {
          pushActivity(itemsOutgoing, {
            timestamp: event.timestamp,
            label: event.item_name ?? `item#${event.item_type_id}`,
            amount: Math.abs(event.delta),
            detail: 'stack reduced',
          })
        }
        break

      case 'ItemDeleted': {
        // Look up real stack size from live inventory (still present because
        // activity handler now runs before inventory handler)
        const tracked = liveItemMap.value.get(event.instance_id)
        const name = tracked?.item_name ?? event.item_name ?? 'Unknown Item'
        const amount = tracked?.stack_size ?? 1
        const contextLabel = event.context === 'StorageTransfer' ? 'stored'
          : event.context === 'VendorSale' ? 'sold'
          : event.context === 'Consumed' ? 'consumed'
          : 'removed'
        pushActivity(itemsOutgoing, {
          timestamp: event.timestamp,
          label: name,
          amount,
          detail: contextLabel,
        })
        break
      }

      case 'FavorChanged':
        pushActivity(favorChanges, {
          timestamp: event.timestamp,
          label: event.npc_name,
          amount: event.delta,
          detail: event.is_gift ? 'gift' : undefined,
        })
        break

      case 'VendorSold':
        pushActivity(councilChanges, {
          timestamp: event.timestamp,
          label: `Sold ${event.item_name}`,
          amount: event.price,
          detail: 'vendor',
        })
        break
    }
  }

  // ── Activity Feed: Chat Status Events ──────────────────────────────

  function handleChatStatusEvent(event: ChatStatusEvent) {
    // Skip session feed accumulation during startup catch-up — historical
    // replay may include events from other characters or stale sessions.
    if (!startupComplete.value) return

    switch (event.kind) {
      case 'ItemGained':
        pushActivity(itemsIncoming, {
          timestamp: event.timestamp,
          label: event.item_name,
          amount: event.quantity,
        })
        break

      case 'Summoned':
        pushActivity(itemsIncoming, {
          timestamp: event.timestamp,
          label: event.item_name,
          amount: event.quantity,
          detail: 'summoned',
        })
        break

      case 'CouncilsChanged':
        pushActivity(councilChanges, {
          timestamp: event.timestamp,
          label: event.amount > 0 ? 'Received' : 'Spent',
          amount: event.amount,
        })
        applyWalletDelta(event.amount)
        break

      case 'CoinsLooted':
        pushActivity(councilChanges, {
          timestamp: event.timestamp,
          label: 'Looted from corpse',
          amount: event.amount,
        })
        applyWalletDelta(event.amount)
        break

      case 'XpGained':
        // Only combat-skill XP feeds the non-prodigy combat rate line.
        if (combatSkillNames.value.has(event.skill)) {
          accrueXpRate('combat', event.amount)
        }
        break

      case 'ProdigyXpGained':
        accrueXpRate('prodigy', event.amount)
        break

      case 'CombatWisdomEarned': {
        const atMs = Date.parse(event.timestamp.replace(' ', 'T') + 'Z')
        combatWisdomSession.value.total += event.amount
        combatWisdomSession.value.earns.unshift({
          name: event.source_name,
          amount: event.amount,
          verb: event.verb,
          zone: event.zone,
          atMs: Number.isNaN(atMs) ? Date.now() : atMs,
        })
        // Refresh persisted per-monster cooldowns so the widget's countdown
        // updates after a new monster award (skip prodigy-level awards).
        if (event.source_name) void fetchCombatWisdomMonsters()
        break
      }

      case 'RouletteResult':
        // A new casino spin landed — refresh the persisted outcome stats so
        // the Roulette widget's pie chart and last-number update live.
        void fetchRouletteStats()
        break
    }
  }

  function clearActivityFeeds() {
    itemsIncoming.value = []
    itemsOutgoing.value = []
    councilChanges.value = []
    currencyChanges.value = []
    favorChanges.value = []
  }

  // ── DB Actions ────────────────────────────────────────────────────────

  /** Load all game state domains from the database for the active character+server */
  async function loadAll() {
    const characterName = getCharacterName()
    const serverName = getServerName()
    if (!characterName || !serverName) return

    loading.value = true
    try {
      const [sk, attr, active, w, inv, rec, eq, fav, cur, eff, stor, vend] = await Promise.all([
        invoke<GameStateSkill[]>('get_game_state_skills', { characterName, serverName }),
        invoke<GameStateAttribute[]>('get_game_state_attributes', { characterName, serverName }),
        invoke<GameStateActiveSkills | null>('get_game_state_active_skills', { characterName, serverName }),
        invoke<GameStateWorld>('get_game_state_world', { characterName, serverName }),
        invoke<GameStateInventoryItem[]>('get_game_state_inventory', { characterName, serverName }),
        invoke<GameStateRecipe[]>('get_game_state_recipes', { characterName, serverName }),
        invoke<GameStateEquipmentSlot[]>('get_game_state_equipment', { characterName, serverName }),
        invoke<GameStateFavor[]>('get_game_state_favor', { characterName, serverName }),
        invoke<GameStateCurrency[]>('get_game_state_currencies', { characterName, serverName }),
        invoke<GameStateEffect[]>('get_game_state_effects', { characterName, serverName }),
        invoke<GameStateStorageItem[]>('get_game_state_storage', { characterName, serverName }),
        invoke<GameStateVendor[]>('get_game_state_vendor', { characterName, serverName }),
      ])
      skills.value = sk
      attributes.value = attr
      activeSkills.value = active
      world.value = w
      inventory.value = inv
      recipes.value = rec
      equipment.value = eq
      favor.value = fav
      currencies.value = cur
      effects.value = eff
      storage.value = stor
      vendor.value = vend
      initialized.value = true
      // Load tracked skills separately (non-blocking)
      loadTrackedSkills()
      // Load combat-skill names for the XP-rate widget (non-blocking)
      loadCombatSkillNames()
      // Load the council-wallet estimate (anchor + live deltas) (non-blocking)
      fetchCurrencyEstimate()
    } catch (e) {
      console.error('[gameStateStore] Failed to load game state:', e)
    } finally {
      loading.value = false
    }
  }

  /** Refresh a single domain after a game-state-updated notification */
  async function refreshDomain(domain: string) {
    const characterName = getCharacterName()
    const serverName = getServerName()
    if (!characterName || !serverName) return

    try {
      switch (domain) {
        case 'skills':
          skills.value = await invoke('get_game_state_skills', { characterName, serverName })
          break
        case 'attributes':
          attributes.value = await invoke('get_game_state_attributes', { characterName, serverName })
          break
        case 'active_skills':
          activeSkills.value = await invoke('get_game_state_active_skills', { characterName, serverName })
          break
        case 'weather':
        case 'combat':
        case 'mount':
        case 'area':
          world.value = await invoke('get_game_state_world', { characterName, serverName })
          break
        case 'inventory':
          inventory.value = await invoke('get_game_state_inventory', { characterName, serverName })
          break
        case 'recipes':
          recipes.value = await invoke('get_game_state_recipes', { characterName, serverName })
          break
        case 'equipment':
          equipment.value = await invoke('get_game_state_equipment', { characterName, serverName })
          break
        case 'favor':
          favor.value = await invoke('get_game_state_favor', { characterName, serverName })
          break
        case 'currencies':
          currencies.value = await invoke('get_game_state_currencies', { characterName, serverName })
          break
        case 'effects':
          effects.value = await invoke('get_game_state_effects', { characterName, serverName })
          break
        case 'storage':
          storage.value = await invoke('get_game_state_storage', { characterName, serverName })
          break
        case 'vendor':
          vendor.value = await invoke('get_game_state_vendor', { characterName, serverName })
          break
        case 'currency_estimate':
          await fetchCurrencyEstimate()
          break
      }
    } catch (e) {
      console.error(`[gameStateStore] Failed to refresh ${domain}:`, e)
    }
  }

  /** Load CDN vault metadata (call once on init) */
  async function loadStorageVaults() {
    try {
      storageVaults.value = await invoke<StorageVaultDetail[]>('get_storage_vault_metadata')
    } catch (e) {
      console.error('[gameStateStore] Failed to load storage vault metadata:', e)
    }
  }

  // ── Event Listeners ───────────────────────────────────────────────────

  listen<string[]>('game-state-updated', (event) => {
    const unique = [...new Set(event.payload)]
    for (const domain of unique) {
      // Arena history is global (not character-scoped), so it bypasses
      // refreshDomain's active-character guard.
      if (domain === 'arena') {
        void fetchArenaStats()
        continue
      }
      refreshDomain(domain)
    }
  })

  listen<PlayerEvent[]>('player-events-batch', (event) => {
    for (const pe of event.payload) {
      handlePlayerActivityEvent(pe) // activity feed reads liveItemMap before inventory handler modifies it
      handleInventoryEvent(pe)
    }
  })

  listen<ChatStatusEvent>('chat-status-event', (event) => {
    handleChatStatusEvent(event.payload)
  })

  listen<string>('server-detected', (event) => {
    settingsStore.settings.activeServerName = event.payload
  })

  listen<string>('character-login', async (event) => {
    // During startup catch-up, the startup sequence handles character
    // loading explicitly after the catch-up poll completes.  We still
    // update the reactive setting so the UI reflects the latest name,
    // but skip the heavy reload work until the app is fully ready.
    settingsStore.settings.activeCharacterName = event.payload

    if (!startupComplete.value) return

    resetSessionSkills()
    clearLiveInventory()
    await loadAll()

    // Reload character reports/snapshots for the newly active character
    const characterStore = useCharacterStore()
    characterStore.initForActiveCharacter()
  })

  // ── Public API ────────────────────────────────────────────────────────

  return {
    // Persisted state
    skills,
    attributes,
    activeSkills,
    world,
    inventory,
    recipes,
    equipment,
    favor,
    currencies,
    effects,
    storage,
    vendor,
    storageVaults,
    loading,
    initialized,

    // Persisted computed
    skillsByName,
    skillTotalLevel,
    inventoryItemCounts,
    heldItemCounts,
    ownedItemCounts,
    attributesByName,
    recipesById,
    recipeCompletions,
    knownRecipeKeys,
    learnedRecipeKeys,
    favorByNpc,
    currenciesByName,
    effectsById,
    namedEffects,
    vendorByNpc,
    storageByVault,
    storageVaultsByKey,
    getVaultMaxPossibleSlots,
    getVaultUnlockedSlots,
    getVaultFavorTier,
    FAVOR_TIER_ORDER,

    // Session skill tracking
    sessionSkills,
    sessionSkillList,
    handleSkillUpdate,
    xpPerHour,
    timeToNextLevel,
    resetSessionSkills,

    // Session XP-rate tracking (combat vs prodigy)
    xpRateSession,
    PRODIGY_XP_PER_LEVEL,
    xpRateOf,
    prodigyEta,
    formatEta,
    resetXpRateSession,

    // Combat Wisdom tracking
    combatWisdomSession,
    combatWisdomMonsters,
    fetchCombatWisdomMonsters,

    // Casino roulette outcome history
    rouletteStats,
    fetchRouletteStats,
    arenaStats,
    fetchArenaStats,

    // Council-wallet estimate
    currencyEstimate,
    fetchCurrencyEstimate,

    // Tracked skills
    trackedSkillNames,
    loadTrackedSkills,
    trackSkill,
    untrackSkill,
    isSkillTracked,

    // Live inventory
    liveItems,
    liveItemCount,
    liveTotalStacks,
    isLivePopulated,
    liveEventLog,
    handleInventoryEvent,
    clearLiveInventory,

    // Clocks
    serverTime,
    gameTime,

    // Activity feeds
    itemsIncoming,
    itemsOutgoing,
    councilChanges,
    currencyChanges,
    favorChanges,

    // DB actions
    loadAll,
    refreshDomain,
    loadStorageVaults,

    // Startup lifecycle
    startupComplete,
  }
})
