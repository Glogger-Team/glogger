import { computed, ref, watch } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { useGameDataStore } from '../stores/gameDataStore'
import { useGameStateStore } from '../stores/gameStateStore'
import { useSettingsStore } from '../stores/settingsStore'
import { tierIndex } from './useFavorTiers'
import { hasStorage } from './useNpcServices'
import type { NpcInfo } from '../types/gameData/npcs'
import type { SkillInfo } from '../types/gameData/skills'

export interface GiftLogEntry {
  npc_key: string
  npc_name: string
  gifted_at: string
  favor_delta: number
}

export interface StatehelmNpcStatus {
  npc: NpcInfo
  giftsThisWeek: number
  maxGifts: number
  giftLog: GiftLogEntry[]
  favorTier: string | null
}

/** A prioritized gift target: an NPC surfaced because it trains one of the
 *  player's top (or equipped) skills, annotated with the skill that put it on
 *  the list. */
export interface StatehelmGiftTarget extends StatehelmNpcStatus {
  /** Display name of the skill that drove this NPC onto the list */
  drivingSkill: string
  /** True when surfaced because the skill is currently equipped (combat only) */
  equipped: boolean
}

const MAX_GIFTS_PER_WEEK = 5

/** How many of the player's top combat / non-combat skills to surface */
const TOP_COMBAT_SKILLS = 4
const TOP_NONCOMBAT_SKILLS = 2

// Module-level cache of CDN skill metadata — shared across every composable
// instance (StatehelmView + the dashboard widget both call useStatehelmTracker).
const cachedSkills = ref<SkillInfo[]>([])
let skillsLoadPromise: Promise<void> | null = null

async function ensureSkillMeta() {
  if (cachedSkills.value.length > 0) return
  if (!skillsLoadPromise) {
    skillsLoadPromise = invoke<SkillInfo[]>('get_all_skills')
      .then((skills) => { cachedSkills.value = skills })
      .catch((e) => { console.error('Failed to load skill metadata:', e); skillsLoadPromise = null })
  }
  return skillsLoadPromise
}

/** UTC offset of America/New_York at `instant`, in minutes (`-240` EDT,
 *  `-300` EST), read from the platform's own tz database via `Intl` so the
 *  DST rules are never re-implemented here. */
function easternOffsetMinutes(instant: Date): number {
  const parts = new Intl.DateTimeFormat('en-US', {
    timeZone: 'America/New_York',
    year: 'numeric', month: '2-digit', day: '2-digit',
    hour: '2-digit', minute: '2-digit', second: '2-digit',
    hour12: false,
  }).formatToParts(instant)
  const wall: Record<string, string> = {}
  for (const { type, value } of parts) wall[type] = value
  // `hour12: false` can render midnight as "24".
  const asUtc = Date.UTC(
    Number(wall.year), Number(wall.month) - 1, Number(wall.day),
    Number(wall.hour) % 24, Number(wall.minute), Number(wall.second)
  )
  return Math.round((asUtc - instant.getTime()) / 60000)
}

/** Monday 00:00 America/New_York of the current week, as a real instant.
 *
 *  The game's gift-cap note says "The week resets on Monday EDT.", so the
 *  boundary is Eastern time — 04:00/05:00 UTC on Monday, not Monday 00:00 UTC
 *  (a UTC boundary misattributes every gift given around the reset). Must stay
 *  identical to Rust's `game_state::current_week_start_utc`. */
function getCurrentWeekStart(): Date {
  const now = new Date()
  const offsetNow = easternOffsetMinutes(now)
  const easternNow = new Date(now.getTime() + offsetNow * 60000)
  const utcDay = easternNow.getUTCDay() // 0=Sun, 1=Mon, ...
  const daysSinceMonday = utcDay === 0 ? 6 : utcDay - 1
  const mondayWall = Date.UTC(
    easternNow.getUTCFullYear(),
    easternNow.getUTCMonth(),
    easternNow.getUTCDate() - daysSinceMonday,
    0, 0, 0, 0
  )
  // The week can straddle a DST switch, so re-resolve the offset at the Monday
  // itself before turning the wall clock into an instant.
  const offsetMonday = easternOffsetMinutes(new Date(mondayWall - offsetNow * 60000))
  return new Date(mondayWall - offsetMonday * 60000)
}

/** A shared clock tick, refreshed once a minute.
 *
 *  `getCurrentWeekStart()` reads the wall clock and nothing reactive, so a plain
 *  `computed` around it is evaluated once and never invalidated — and the
 *  Statehelm view is keep-alive'd, so an app left open across the Monday reset
 *  kept *last* week's cutoff: `giftsThisWeek` counted the previous week's rows
 *  and `removeGift` handed `remove_last_gift` a stale boundary while the backend
 *  compared against a freshly computed one. Every boundary below derives from
 *  this tick, so they all roll at the reset.
 *
 *  Module-scope (with a `globalThis` guard) so the view and the dashboard widget
 *  share a single interval and HMR edits cannot stack more of them. */
const weekClock = ref(Date.now())
const weekClockGlobal = globalThis as typeof globalThis & { __gloggerWeekClockTick?: number }
if (!weekClockGlobal.__gloggerWeekClockTick) {
  weekClockGlobal.__gloggerWeekClockTick = window.setInterval(
    () => { weekClock.value = Date.now() },
    60_000,
  )
}

/** Monday 00:00 America/New_York of the current week, as a real instant. */
const weekStart = computed(() => {
  void weekClock.value // reactive dependency: recompute each tick
  return getCurrentWeekStart()
})

/** Monday 00:00 America/New_York of the *following* week — the actual reset
 *  instant. `weekStart + 168 h` is an hour off in the two DST-transition weeks
 *  (the week there is 167 h / 169 h), so every countdown reads this. */
const nextWeekStart = computed(() => {
  const w = weekStart.value
  const wall = Date.UTC(w.getUTCFullYear(), w.getUTCMonth(), w.getUTCDate() + 7, 0, 0, 0, 0)
  const offsetMonday = easternOffsetMinutes(new Date(wall - easternOffsetMinutes(w) * 60000))
  return new Date(wall - offsetMonday * 60000)
})

/** The week boundary in the gift log's storage format
 *  (`YYYY-MM-DD HH:MM:SS`, UTC). String comparisons must use this: an ISO cutoff
 *  reads `…T04:00:00.000Z`, whose index 10 is 'T' (0x54) against a row's ' '
 *  (0x20), so it sorts *after* every same-date row — the old
 *  `row >= cutoff` test was false for Monday-dated rows and dropped them. */
const weekStartDb = computed(() => {
  const d = weekStart.value
  const p = (n: number) => String(n).padStart(2, '0')
  return `${d.getUTCFullYear()}-${p(d.getUTCMonth() + 1)}-${p(d.getUTCDate())} ${p(d.getUTCHours())}:${p(d.getUTCMinutes())}:${p(d.getUTCSeconds())}`
})

export function useStatehelmTracker() {
  const gameData = useGameDataStore()
  const gameState = useGameStateStore()
  const settings = useSettingsStore()

  const giftLog = ref<GiftLogEntry[]>([])
  const loading = ref(false)

  const statehelmNpcs = computed<NpcInfo[]>(() => {
    const allNpcs = Object.values(gameData.npcsByKey)
    return allNpcs.filter(npc => {
      // The weekly 5-gift cap is a Statehelm-region rule — the game's own note
      // reads "Statehelm townsfolk are more sophisticated than villagers
      // elsewhere… it would be uncouth to receive too many gifts per week" —
      // and it applies in the Statehelm Sewers too (Hamilton, Smoke Tail and
      // Fuzzlebun, all `AreaStatehelmCaves`, show the cap note).
      const area = (npc.area_name ?? '').toLowerCase()
      return (area === 'areastatehelm' || area === 'areastatehelmcaves')
        && npc.preferences.length > 0
    }).sort((a, b) => a.name.localeCompare(b.name))
  })

  const giftsThisWeek = computed(() => {
    const cutoff = weekStartDb.value
    const counts: Record<string, GiftLogEntry[]> = {}
    for (const entry of giftLog.value) {
      // Normalized: rows written by older builds carry ISO stamps, and 'T' > ' '
      // would count a same-date row earlier in the day as in-week.
      if (entry.gifted_at.replace('T', ' ').slice(0, 19) >= cutoff) {
        if (!counts[entry.npc_key]) counts[entry.npc_key] = []
        counts[entry.npc_key].push(entry)
      }
    }
    return counts
  })

  const npcStatuses = computed<StatehelmNpcStatus[]>(() => {
    return statehelmNpcs.value.map(npc => {
      const gifts = giftsThisWeek.value[npc.key] ?? []
      const favorData = gameState.favorByNpc[npc.key]
      return {
        npc,
        giftsThisWeek: gifts.length,
        maxGifts: MAX_GIFTS_PER_WEEK,
        giftLog: gifts,
        favorTier: favorData?.favor_tier ?? null,
      }
    })
  })

  // ── Skill-driven gift prioritization ──────────────────────────────────
  // The dashboard widget surfaces the NPCs that train the player's top combat
  // skills (+ the 2 currently equipped) and top non-combat skills, until each
  // NPC's 5 weekly gifts are donated — at which point it falls off and the next
  // highest skill of that category takes its place.

  /** internal skill name → { id, combat } */
  const skillMetaByInternal = computed(() => {
    const m = new Map<string, { id: number; combat: boolean }>()
    for (const s of cachedSkills.value) {
      m.set(s.internal_name, { id: s.id, combat: s.combat === true })
    }
    return m
  })

  /** skill id → combat flag */
  const skillCombatById = computed(() => {
    const m = new Map<number, boolean>()
    for (const s of cachedSkills.value) m.set(s.id, s.combat === true)
    return m
  })

  /** skill id → internal name */
  const skillInternalById = computed(() => {
    const m = new Map<number, string>()
    for (const s of cachedSkills.value) m.set(s.id, s.internal_name)
    return m
  })

  /** internal skill name → display name */
  const skillDisplayByInternal = computed(() => {
    const m = new Map<string, string>()
    for (const s of cachedSkills.value) m.set(s.internal_name, s.name)
    return m
  })

  /** NPC key → true when the NPC trains at least one combat skill ("combat wins
   *  ties": a mixed NPC is treated as combat-only for categorization). */
  const npcIsCombatByKey = computed(() => {
    const meta = skillMetaByInternal.value
    const m = new Map<string, boolean>()
    for (const st of npcStatuses.value) {
      m.set(st.npc.key, st.npc.trains_skills.some((int) => meta.get(int)?.combat === true))
    }
    return m
  })

  /** internal skill name → statuses of NPCs that train it */
  const statusesBySkillInternal = computed(() => {
    const m = new Map<string, StatehelmNpcStatus[]>()
    for (const st of npcStatuses.value) {
      for (const int of st.npc.trains_skills) {
        if (!m.has(int)) m.set(int, [])
        m.get(int)!.push(st)
      }
    }
    return m
  })

  /** Player's combat skill internal names, highest base level first (level > 0). */
  const rankedCombatSkillInternals = computed(() => rankPlayerSkills(true))
  /** Player's non-combat skill internal names, highest base level first. */
  const rankedNonCombatSkillInternals = computed(() => rankPlayerSkills(false))

  function rankPlayerSkills(wantCombat: boolean): string[] {
    const combatById = skillCombatById.value
    const internalById = skillInternalById.value
    return [...gameState.skills]
      .filter((s) => s.base_level > 0 && (combatById.get(s.skill_id) === true) === wantCombat)
      .sort((a, b) => b.base_level - a.base_level)
      .map((s) => internalById.get(s.skill_id))
      .filter((x): x is string => !!x)
  }

  /** Currently equipped combat skills as internal names. */
  const equippedSkillInternals = computed(() => {
    const a = gameState.activeSkills
    if (!a) return []
    const internalById = skillInternalById.value
    return [a.skill1_id, a.skill2_id]
      .map((id) => internalById.get(id))
      .filter((x): x is string => !!x)
  })

  /** An NPC drops off the widget regardless of remaining gifts when its favor
   *  standing makes further gifting pointless:
   *   - SoulMates (the top tier): always excluded.
   *   - LikeFamily: excluded unless the NPC offers storage (storage capacity keeps
   *     scaling with favor past LikeFamily, so those stay worth gifting). */
  function isExcludedByFavor(status: StatehelmNpcStatus): boolean {
    const tier = status.favorTier
    if (tier === 'SoulMates') return true
    if (tier === 'LikeFamily' && !hasStorage(status.npc)) return true
    return false
  }

  /** Pick the representative NPC for a skill: the one whose category matches and
   *  that has the highest current favor standing, skipping NPCs that are maxed
   *  (5/5 this week) or excluded by favor standing. Returns null when none remain. */
  function representativeFor(internal: string, wantCombat: boolean): StatehelmNpcStatus | null {
    const candidates = (statusesBySkillInternal.value.get(internal) ?? []).filter((st) => {
      if ((npcIsCombatByKey.value.get(st.npc.key) ?? false) !== wantCombat) return false
      if (st.giftsThisWeek >= st.maxGifts) return false
      if (isExcludedByFavor(st)) return false
      return true
    })
    if (candidates.length === 0) return null
    candidates.sort((a, b) => {
      const fa = tierIndex(a.favorTier ?? 'Neutral')
      const fb = tierIndex(b.favorTier ?? 'Neutral')
      if (fa !== fb) return fa - fb // lower index = higher favor
      return a.npc.name.localeCompare(b.npc.name)
    })
    return candidates[0]
  }

  function displaySkill(internal: string): string {
    return skillDisplayByInternal.value.get(internal) ?? internal
  }

  /** Combat gift targets: the NPCs for the 2 equipped combat skills plus the
   *  player's top combat skills, up to `4 + equipped` slots. Equipped skills are
   *  prioritized (and keep their ✦ marker even when also top-ranked). Maxed NPCs
   *  are skipped, so when any NPC — equipped included — has all 5 gifts donated it
   *  falls off and the next-highest combat skill backfills the slot. */
  const combatGiftTargets = computed<StatehelmGiftTarget[]>(() => {
    // Dedupe equipped (both slots could, in theory, hold the same skill).
    const equipped = [...new Set(equippedSkillInternals.value)]
    const equippedSet = new Set(equipped)
    const cap = TOP_COMBAT_SKILLS + equipped.length

    // Priority order of skills: equipped first, then ranked by base level.
    // Dedupe by skill so an equipped skill that's also top-ranked is processed
    // once (keeping its equipped marker).
    const orderedSkills: string[] = []
    const skillSeen = new Set<string>()
    for (const internal of [...equipped, ...rankedCombatSkillInternals.value]) {
      if (skillSeen.has(internal)) continue
      skillSeen.add(internal)
      orderedSkills.push(internal)
    }

    const out: StatehelmGiftTarget[] = []
    const npcSeen = new Set<string>()
    for (const internal of orderedSkills) {
      if (out.length >= cap) break
      const rep = representativeFor(internal, true)
      if (rep && !npcSeen.has(rep.npc.key)) {
        npcSeen.add(rep.npc.key)
        out.push({ ...rep, drivingSkill: displaySkill(internal), equipped: equippedSet.has(internal) })
      }
    }

    return out
  })

  /** Non-combat gift targets: the top 2 non-combat-skill NPCs (maxed skipped). */
  const nonCombatGiftTargets = computed<StatehelmGiftTarget[]>(() => {
    const out: StatehelmGiftTarget[] = []
    const seen = new Set<string>()

    for (const internal of rankedNonCombatSkillInternals.value) {
      if (out.length >= TOP_NONCOMBAT_SKILLS) break
      const rep = representativeFor(internal, false)
      if (rep && !seen.has(rep.npc.key)) {
        seen.add(rep.npc.key)
        out.push({ ...rep, drivingSkill: displaySkill(internal), equipped: false })
      }
    }

    return out
  })

  const totalGiftsGiven = computed(() =>
    npcStatuses.value.reduce((sum, s) => sum + s.giftsThisWeek, 0)
  )

  const totalGiftsMax = computed(() =>
    npcStatuses.value.length * MAX_GIFTS_PER_WEEK
  )

  async function loadGiftLog() {
    const characterName = settings.settings.activeCharacterName
    const serverName = settings.settings.activeServerName
    if (!characterName || !serverName) return

    loading.value = true
    try {
      giftLog.value = await invoke<GiftLogEntry[]>('get_gift_log', {
        characterName,
        serverName,
      })
    } catch (e) {
      console.error('Failed to load gift log:', e)
    } finally {
      loading.value = false
    }
  }

  async function addGift(npcKey: string, npcName: string) {
    const characterName = settings.settings.activeCharacterName
    const serverName = settings.settings.activeServerName
    if (!characterName || !serverName) return

    await invoke('add_manual_gift', {
      characterName,
      serverName,
      npcKey,
      npcName,
    })
    await loadGiftLog()
  }

  async function removeGift(npcKey: string) {
    const characterName = settings.settings.activeCharacterName
    const serverName = settings.settings.activeServerName
    if (!characterName || !serverName) return

    await invoke('remove_last_gift', {
      characterName,
      serverName,
      npcKey,
      weekStart: weekStartDb.value,
    })
    await loadGiftLog()
  }

  // Watch for real-time gift events via the favor activity feed
  watch(
    () => gameState.favorChanges,
    (changes) => {
      if (changes.length === 0) return
      const latest = changes[0]
      if (latest.detail === 'gift') {
        loadGiftLog()
      }
    },
    { deep: true }
  )

  // Also watch the DB-backed favor data — this is always refreshed by
  // game-state-updated events regardless of which page is active, so
  // the gift log stays current even when the statehelm page isn't open.
  watch(
    () => gameState.favor,
    () => loadGiftLog(),
    { deep: true }
  )

  return {
    statehelmNpcs,
    npcStatuses,
    combatGiftTargets,
    nonCombatGiftTargets,
    totalGiftsGiven,
    totalGiftsMax,
    loading,
    loadGiftLog,
    loadSkillMeta: ensureSkillMeta,
    addGift,
    removeGift,
    nextWeekStart,
  }
}
