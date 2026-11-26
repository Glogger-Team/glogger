<template>
  <div class="flex flex-col gap-3 h-full min-h-0 px-1">
    <div class="flex items-center gap-2">
      <h3 class="dialog-title shrink-0">Search All Mods</h3>
      <input
        v-model="query"
        type="text"
        placeholder="Search every mod by name, skill, or slot…"
        class="bg-surface-elevated border border-border-default rounded px-2 py-1 text-xs text-text-primary flex-1 min-w-0" />
    </div>

    <div
      v-if="applyMessage"
      class="text-xs px-2 py-1 rounded"
      :class="applyOk ? 'text-accent-gold bg-accent-gold/10' : 'text-red-400 bg-red-900/10'">
      {{ applyMessage }}
    </div>

    <div v-if="!query.trim()" class="text-xs text-text-muted py-4 text-center leading-relaxed">
      Type to search the full mod catalog (e.g., "Thunderstrike", "Sword", "Necklace") —
      no equipment slot needs to be selected.<br />
      Click a slot under a mod to apply it to that slot of your build.
    </div>

    <div v-else-if="loading" class="text-xs text-text-muted py-4 text-center">
      Searching…
    </div>

    <div v-else-if="groupedResults.length === 0" class="text-xs text-text-dim py-4 text-center">
      No mods match "{{ query }}"
    </div>

    <div v-else class="flex-1 overflow-y-auto space-y-3">
      <div v-for="group in groupedResults" :key="group.skill">
        <h4 class="panel-label mb-1">
          {{ group.skill }} ({{ group.mods.length }})
        </h4>
        <div class="space-y-1">
          <div
            v-for="mod in group.mods"
            :key="mod.key"
            class="px-2 py-1.5 rounded text-sm bg-surface-elevated border border-border-default">
            <div class="font-medium text-text-primary">{{ mod.name }}</div>
            <div v-if="mod.slots.length" class="flex flex-wrap gap-1 mt-1">
              <button
                v-for="s in mod.slots"
                :key="s.id"
                class="text-[10px] px-1.5 py-0.5 rounded bg-surface-base text-text-dim border border-border-default/60 hover:bg-accent-gold/20 hover:text-accent-gold hover:border-accent-gold/40 cursor-pointer transition-colors"
                :title="`Add ${mod.name} to ${s.label}`"
                @click="applyMod(mod, s)">
                + {{ s.label }}
              </button>
            </div>
            <div v-if="effects[mod.key]?.length" class="mt-1 space-y-0.5">
              <div
                v-for="(eff, i) in effects[mod.key]"
                :key="i"
                class="text-[11px] text-text-secondary leading-snug">{{ eff }}</div>
            </div>
          </div>
        </div>
      </div>

      <div class="text-[10px] text-text-dim text-center pt-2">
        {{ totalMatches }} mod{{ totalMatches !== 1 ? 's' : '' }} found
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, watch } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { useBuildPlannerStore } from '../../../stores/buildPlannerStore'
import { EQUIPMENT_SLOTS } from '../../../types/buildPlanner'

/** Subset of the backend TsysBrowserEntry we render here. */
interface TsysEntry {
  key: string
  internal_name: string | null
  skill: string | null
  slots: string[]
  prefix: string | null
  suffix: string | null
  is_unavailable: boolean | null
  tiers: Record<string, unknown>
}

interface SlotTarget {
  id: string
  label: string
}

interface DisplayMod {
  key: string
  name: string
  internalName: string | null
  slots: SlotTarget[]
}

interface ModGroup {
  skill: string
  mods: DisplayMod[]
}

const store = useBuildPlannerStore()

const query = ref('')
const loading = ref(false)
const results = ref<TsysEntry[]>([])
/** Resolved (max-tier) effect text per mod, keyed by mod key. */
const effects = ref<Record<string, string[]>>({})

// Map CDN equipment-slot names to the build planner's slot ids.
const CDN_TO_BUILD_SLOT: Record<string, string> = {
  Head: 'Head', Chest: 'Chest', Legs: 'Legs', Hands: 'Hands', Feet: 'Feet',
  MainHand: 'MainHand', OffHand: 'OffHand', OffHandShield: 'OffHand',
  Ring: 'Ring', Necklace: 'Necklace', Waist: 'Belt',
}
const SLOT_LABELS: Record<string, string> = Object.fromEntries(
  EQUIPMENT_SLOTS.map(s => [s.id, s.label]),
)

// Resolve a mod's CDN slots into clickable build-slot targets (deduped, only
// slots the build planner actually has).
function slotTargets(slots: string[]): SlotTarget[] {
  const seen = new Set<string>()
  const out: SlotTarget[] = []
  for (const s of slots) {
    const id = CDN_TO_BUILD_SLOT[s]
    if (!id || seen.has(id) || !SLOT_LABELS[id]) continue
    seen.add(id)
    out.push({ id, label: SLOT_LABELS[id] })
  }
  return out
}

function displayName(e: TsysEntry): string {
  return e.prefix ?? e.suffix ?? e.internal_name ?? e.key
}

function skillGroup(e: TsysEntry): string {
  return e.skill && e.skill !== 'AnySkill' ? e.skill : 'Generic'
}

// Tier keys look like "id_N"; the highest N is the max-level version of the mod,
// which we resolve effects for as the representative "what this mod does".
function maxTier(tiers: Record<string, unknown>): number | null {
  let max: number | null = null
  for (const k of Object.keys(tiers ?? {})) {
    const m = /id_(\d+)/.exec(k)
    if (m) {
      const n = parseInt(m[1], 10)
      if (max === null || n > max) max = n
    }
  }
  return max
}

let debounce: ReturnType<typeof setTimeout> | null = null
watch(query, () => {
  if (debounce) clearTimeout(debounce)
  if (!query.value.trim()) {
    results.value = []
    effects.value = {}
    return
  }
  debounce = setTimeout(runSearch, 250)
})

async function runSearch() {
  const q = query.value.trim()
  if (!q) return
  loading.value = true
  try {
    const rows = await invoke<TsysEntry[]>('search_tsys', { query: q, limit: 300 })
    const filtered = rows.filter(r => r.is_unavailable !== true)
    results.value = filtered
    await resolveEffects(filtered)
  } catch (e) {
    console.error('[global-mod-search] search failed:', e)
    results.value = []
    effects.value = {}
  } finally {
    loading.value = false
  }
}

// Batch-resolve the highest-tier effects for every result in one backend call.
async function resolveEffects(rows: TsysEntry[]) {
  const pairs: [string, number][] = []
  const keyByPair = new Map<string, string>()
  for (const e of rows) {
    const tier = maxTier(e.tiers)
    if (!e.internal_name || tier === null) continue
    pairs.push([e.internal_name, tier])
    keyByPair.set(`${e.internal_name}:${tier}`, e.key)
  }
  if (pairs.length === 0) {
    effects.value = {}
    return
  }
  try {
    const map = await invoke<Record<string, { tier_effects: string[] }>>(
      'get_tsys_power_info_batch',
      { powers: pairs },
    )
    const out: Record<string, string[]> = {}
    for (const [pairKey, info] of Object.entries(map)) {
      const modKey = keyByPair.get(pairKey)
      if (modKey) out[modKey] = info.tier_effects ?? []
    }
    effects.value = out
  } catch (e) {
    console.error('[global-mod-search] effect resolve failed:', e)
    effects.value = {}
  }
}

// ── Apply to slot ────────────────────────────────────────────────────────────
const applyMessage = ref('')
const applyOk = ref(true)
let applyMsgTimer: ReturnType<typeof setTimeout> | null = null

function reasonText(reason?: string): string {
  switch (reason) {
    case 'duplicate': return 'already on that slot'
    case 'full': return 'slot is full'
    case 'no-room': return 'no room for that skill/type at this rarity'
    case 'not-eligible': return "mod doesn't fit that slot"
    case 'no-preset': return 'no build selected'
    default: return reason ?? 'unknown error'
  }
}

async function applyMod(mod: DisplayMod, slot: SlotTarget) {
  if (!mod.internalName) return
  const res = await store.addCatalogModToSlot(slot.id, mod.internalName)
  applyOk.value = res.ok
  applyMessage.value = res.ok
    ? `Added ${mod.name} to ${slot.label}`
    : `Couldn't add to ${slot.label}: ${reasonText(res.reason)}`
  if (applyMsgTimer) clearTimeout(applyMsgTimer)
  applyMsgTimer = setTimeout(() => { applyMessage.value = '' }, 4000)
}

const groupedResults = computed((): ModGroup[] => {
  const groups = new Map<string, DisplayMod[]>()
  for (const e of results.value) {
    const g = skillGroup(e)
    if (!groups.has(g)) groups.set(g, [])
    groups.get(g)!.push({
      key: e.key,
      name: displayName(e),
      internalName: e.internal_name,
      slots: slotTargets(e.slots),
    })
  }
  // Sort mods within each group by name.
  for (const mods of groups.values()) {
    mods.sort((a, b) => a.name.localeCompare(b.name))
  }
  // Skill groups alphabetical, with "Generic" last.
  return [...groups.entries()]
    .map(([skill, mods]) => ({ skill, mods }))
    .sort((a, b) => {
      if (a.skill === 'Generic') return 1
      if (b.skill === 'Generic') return -1
      return a.skill.localeCompare(b.skill)
    })
})

const totalMatches = computed(() =>
  groupedResults.value.reduce((sum, g) => sum + g.mods.length, 0)
)
</script>
