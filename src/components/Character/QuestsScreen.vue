<template>
  <div class="flex flex-col gap-3 h-full overflow-hidden">
    <!-- Header: character + tab bar -->
    <div v-if="hasSnapshot" class="flex flex-col gap-2 shrink-0">
      <div class="flex items-center gap-3">
        <span class="text-sm text-text-primary font-semibold">
          {{ settingsStore.settings.activeCharacterName ?? 'Character' }}
        </span>
        <span v-if="settingsStore.settings.activeServerName" class="text-xs text-text-muted">
          {{ settingsStore.settings.activeServerName }}
        </span>
      </div>

      <!-- Tabs -->
      <div class="flex items-center gap-2 flex-wrap">
        <button
          v-for="tab in tabs"
          :key="tab.key"
          :class="[
            'text-xs px-3 py-1 rounded border cursor-pointer transition-colors',
            activeTab === tab.key
              ? 'bg-accent-gold/20 border-accent-gold/40 text-accent-gold'
              : 'bg-transparent border-border-light text-text-muted hover:text-text-primary',
          ]"
          @click="selectTab(tab.key)">
          {{ tab.label }} ({{ tab.count }})
        </button>
      </div>
    </div>

    <!-- Empty / loading states -->
    <EmptyState
      v-if="!hasSnapshot && !characterStore.loading"
      primary="No character data loaded."
      secondary="Import a character report to see your quests." />

    <EmptyState
      v-else-if="characterStore.loading || loading"
      primary="Loading quest data..."
      secondary="Resolving quest details from game data." />

    <!-- Two-panel layout -->
    <div v-else class="flex flex-col gap-2 flex-1 min-h-0">
      <!-- Hint when completed-quest data is missing (old export) -->
      <div
        v-if="(activeTab === 'completed' || activeTab === 'uncompleted') && completedKeys.length === 0"
        class="text-[0.7rem] text-text-dim italic px-2 py-1 border border-surface-elevated rounded bg-surface-base">
        No completed-quest data in this export. Completed quests are only recorded by newer game
        clients — re-export this character in-game (<span class="text-text-secondary">Export Character as JSON in Settings → VIP</span>) to populate this.
      </div>

      <!-- Uncompleted work orders backlog (its own layout) -->
      <WorkOrdersTodoPanel
        v-if="activeTab === 'work_orders_todo'"
        :orders="workOrderTodos" />

      <!-- Quest list + detail (all other tabs) -->
      <div v-else class="flex gap-3 flex-1 min-h-0">
        <div class="w-80 shrink-0 flex flex-col min-h-0">
          <QuestListPanel
            :quests="currentTabQuests"
            :skills-by-name="gameState.skillsByName"
            :favor-by-npc="gameState.favorByNpc"
            :selected-quest-key="selectedQuestKey"
            :quest-categories="currentTabCategories"
            @select="selectQuest" />
        </div>

        <QuestDetailPanel
          :quest="selectedQuest"
          :skills-by-name="gameState.skillsByName"
          :favor-by-npc="gameState.favorByNpc" />
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, watch } from 'vue'
import { useCharacterStore } from '../../stores/characterStore'
import { useGameDataStore } from '../../stores/gameDataStore'
import { useGameStateStore } from '../../stores/gameStateStore'
import { useSettingsStore } from '../../stores/settingsStore'
import type { QuestInfo } from '../../types/gameData'
import type { WorkOrderTodo } from '../../types/crafting'
import EmptyState from '../Shared/EmptyState.vue'
import QuestListPanel from './QuestListPanel.vue'
import QuestDetailPanel from './QuestDetailPanel.vue'
import WorkOrdersTodoPanel from './WorkOrdersTodoPanel.vue'

type TabKey = 'active' | 'work_orders' | 'work_orders_todo' | 'uncompleted' | 'completed'

const characterStore = useCharacterStore()
const gameData = useGameDataStore()
const gameState = useGameStateStore()
const settingsStore = useSettingsStore()

const selectedQuestKey = ref<string | null>(null)
const activeTab = ref<TabKey>('active')

const hasSnapshot = computed(() => !!characterStore.selectedSnapshot)

// Quest details (names, zones, the Uncompleted set) all come from the CDN quest
// list — show a loading state until it's cached rather than flashing raw keys.
const loading = computed(() => hasSnapshot.value && gameData.allQuestsCache.length === 0)

// ── Master lookup: internal_name → full CDN quest (avoids N per-quest invokes) ──
const questByName = computed(() => {
  const m = new Map<string, QuestInfo>()
  for (const q of gameData.allQuestsCache) m.set(q.internal_name, q)
  return m
})

function toQuestInfo(key: string): QuestInfo {
  return questByName.value.get(key) ?? { internal_name: key, raw: { InternalName: key } }
}

function isRepeatable(q: QuestInfo): boolean {
  return !!(q.raw?.ReuseTime_Minutes || q.raw?.ReuseTime_Days)
}

// Every work order has a fixed turn-in board (1 of 9). The NPC comes from the
// Scripted "Deliver to X" objective; the location is parsed from the flavor
// Description ("…to X in/on/at <Location>."). Verified to cover all 1,299 WOs.
function parseWorkOrderBoard(raw: QuestInfo['raw']): { npc: string | null; location: string | null } {
  if (!raw) return { npc: null, location: null }
  let npc: string | null = null
  const scripted = raw.Objectives?.find(
    o => o.Type === 'Scripted' && /deliver to/i.test(o.Description ?? ''),
  )
  if (scripted?.Description) npc = scripted.Description.replace(/^deliver to\s*/i, '').trim() || null

  let location: string | null = null
  const m = (raw.Description ?? '').match(/\bto (.+?) (?:in|on|at) (.+?)\.(?:\s|$)/)
  if (m) {
    npc = npc ?? m[1].trim()
    location = m[2].replace(/^the /, '').trim()
  }
  return { npc, location }
}

// ── Quest keys grouped by the category stored in the character snapshot ────────
const activeKeys = computed(() =>
  characterStore.activeQuests.filter(q => q.category === 'active').map(q => q.quest_key)
)
// A repeatable work order can appear in both the active and completed export
// arrays. Dedupe by key, preferring the active ('work_order') state so it shows
// as available rather than done.
const workOrderRows = computed(() => {
  const byKey = new Map<string, { quest_key: string; category: string }>()
  for (const q of characterStore.activeQuests) {
    if (q.category !== 'work_order' && q.category !== 'completed_work_order') continue
    const existing = byKey.get(q.quest_key)
    if (!existing || q.category === 'work_order') {
      byKey.set(q.quest_key, { quest_key: q.quest_key, category: q.category })
    }
  }
  return Array.from(byKey.values())
})
const completedKeys = computed(() =>
  characterStore.activeQuests.filter(q => q.category === 'completed').map(q => q.quest_key)
)

const activeKeySet = computed(() => new Set(activeKeys.value))
const completedKeySet = computed(() => new Set(completedKeys.value))

// Work-order completion/acceptance sets (separate export arrays from quests).
const completedWorkOrderSet = computed(
  () => new Set(workOrderRows.value.filter(r => r.category === 'completed_work_order').map(r => r.quest_key))
)
const activeWorkOrderSet = computed(
  () => new Set(workOrderRows.value.filter(r => r.category === 'work_order').map(r => r.quest_key))
)

// Uncompleted work-order backlog: every CDN work order for a crafting skill the
// player actually has, minus the ones already completed. "Unknown"-skill orders
// (Fitz-the-Boatman gathering turn-ins) are excluded — they aren't craftable.
// Recipes are resolved later (at project-creation), so this stays cheap.
const workOrderTodos = computed<WorkOrderTodo[]>(() => {
  const completed = completedWorkOrderSet.value
  const active = activeWorkOrderSet.value
  const skills = gameState.skillsByName
  const out: WorkOrderTodo[] = []
  for (const q of gameData.allQuestsCache) {
    const raw = q.raw
    const skill = raw?.WorkOrderSkill
    if (!skill || skill === 'Unknown') continue
    const playerSkill = skills[skill]
    if (!playerSkill || (playerSkill.level ?? 0) < 1) continue // skill-scoped
    if (completed.has(q.internal_name)) continue // uncompleted only
    const collect = raw?.Objectives?.find(o => o.Type === 'Collect')
    let industryXp = 0
    let goldReward = 0
    for (const rw of raw?.Rewards ?? []) {
      if (rw.T === 'SkillXp' && rw.Skill === 'Industry') industryXp = rw.Xp ?? 0
      if (rw.T === 'WorkOrderCurrency' && rw.Currency === 'Gold') goldReward = rw.Amount ?? 0
    }
    const board = parseWorkOrderBoard(raw)
    out.push({
      quest_key: q.internal_name,
      name: raw?.Name ?? q.internal_name,
      craft_skill: skill,
      item_internal_name: collect?.ItemName ?? null,
      quantity: collect?.Number ?? 0,
      industry_xp: industryXp,
      gold_reward: goldReward,
      is_active: active.has(q.internal_name),
      board_npc: board.npc,
      board_location: board.location,
    })
  }
  return out
})

// ── Per-tab quest lists ────────────────────────────────────────────────────────
const activeQuestList = computed(() => activeKeys.value.map(toQuestInfo))
const workOrderList = computed(() => workOrderRows.value.map(r => toQuestInfo(r.quest_key)))
const completedQuestList = computed(() => completedKeys.value.map(toQuestInfo))

// Uncompleted = every known non-work-order quest that isn't active, minus quests
// you've permanently completed. Repeatable quests you've done DO show here since
// they're available again.
const uncompletedList = computed<QuestInfo[]>(() => {
  const active = activeKeySet.value
  const completed = completedKeySet.value
  const out: QuestInfo[] = []
  for (const q of gameData.allQuestsCache) {
    if (q.raw?.WorkOrderSkill) continue // work orders live in their own tab
    if (active.has(q.internal_name)) continue // shown in Active tab
    const done = completed.has(q.internal_name)
    if (done && !isRepeatable(q)) continue // permanently completed → hide
    out.push(q)
  }
  return out
})

const tabs = computed(() => [
  { key: 'active' as const, label: 'Active', count: activeQuestList.value.length },
  { key: 'work_orders' as const, label: 'Work Orders', count: workOrderList.value.length },
  { key: 'work_orders_todo' as const, label: 'Uncompleted WOs', count: workOrderTodos.value.length },
  { key: 'uncompleted' as const, label: 'Uncompleted', count: uncompletedList.value.length },
  { key: 'completed' as const, label: 'Completed', count: completedQuestList.value.length },
])

const currentTabQuests = computed<QuestInfo[]>(() => {
  switch (activeTab.value) {
    case 'active': return activeQuestList.value
    case 'work_orders': return workOrderList.value
    case 'completed': return completedQuestList.value
    case 'uncompleted': return uncompletedList.value
    default: return []
  }
})

const currentTabCategories = computed(() => {
  const m = new Map<string, string>()
  switch (activeTab.value) {
    case 'active':
      for (const k of activeKeys.value) m.set(k, 'active')
      break
    case 'work_orders':
      for (const r of workOrderRows.value) m.set(r.quest_key, r.category)
      break
    case 'completed':
      for (const k of completedKeys.value) m.set(k, 'completed')
      break
    case 'uncompleted':
      for (const q of uncompletedList.value) {
        m.set(q.internal_name, isRepeatable(q) ? 'repeatable' : 'available')
      }
      break
  }
  return m
})

const selectedQuest = computed<QuestInfo | null>(() => {
  if (!selectedQuestKey.value) return null
  return (
    currentTabQuests.value.find(q => q.internal_name === selectedQuestKey.value) ??
    questByName.value.get(selectedQuestKey.value) ??
    null
  )
})

function selectQuest(key: string) {
  selectedQuestKey.value = key
}

function selectTab(key: TabKey) {
  activeTab.value = key
  // Clear the selection if it isn't present in the newly-active tab.
  if (
    selectedQuestKey.value &&
    !currentTabQuests.value.some(q => q.internal_name === selectedQuestKey.value)
  ) {
    selectedQuestKey.value = null
  }
}

// Ensure the full CDN quest list is loaded (needed for the Uncompleted tab and to
// resolve active/completed quest details without per-quest round-trips).
watch(
  () => gameData.status,
  (status) => {
    if (status === 'ready') gameData.loadAllQuests()
  },
  { immediate: true },
)
</script>
