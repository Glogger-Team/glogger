<template>
  <div class="flex flex-col gap-2 overflow-hidden h-full">
    <!-- Filters -->
    <div class="flex items-center gap-2 flex-wrap">
      <input
        v-model="filter"
        type="text"
        placeholder="Filter skills..."
        class="px-3 py-1.5 bg-surface-base border border-border-default rounded text-sm text-text-primary placeholder-text-muted focus:outline-none focus:border-accent-gold/50 w-40" />

      <select
        v-model="groupBy"
        class="px-2 py-1.5 bg-surface-base border border-border-default rounded text-xs text-text-primary cursor-pointer">
        <option value="type">By Type</option>
        <option value="level">By Level</option>
        <option value="none">No Grouping</option>
      </select>

      <select
        v-model="sortBy"
        class="px-2 py-1.5 bg-surface-base border border-border-default rounded text-xs text-text-primary cursor-pointer">
        <option value="level">Level</option>
        <option value="name">Name</option>
        <option value="session">Session XP</option>
      </select>

      <label class="flex items-center gap-1 text-[0.65rem] text-text-muted cursor-pointer">
        <input type="checkbox" v-model="hideMaxed" class="cursor-pointer" />
        Hide maxed
      </label>

      <label class="flex items-center gap-1 text-[0.65rem] text-text-muted cursor-pointer">
        <input type="checkbox" v-model="hideZero" class="cursor-pointer" />
        Hide zero
      </label>

      <span class="text-xs text-text-dim ml-auto">{{ totalVisible }} skills</span>
    </div>

    <!-- Grouped skill list -->
    <div class="overflow-y-auto flex-1 border border-surface-elevated">
      <template v-for="group in groupedSkills" :key="group.label">
        <div
          v-if="group.label"
          class="sticky top-0 z-10 bg-surface-base px-2 py-1 text-[0.65rem] uppercase tracking-widest text-text-dim border-b border-surface-elevated cursor-pointer flex items-center gap-1"
          @click="toggleGroup(group.label)">
          <span class="text-text-secondary text-xs">{{ collapsedGroups.has(group.label) ? '▶' : '▼' }}</span>
          {{ group.label }} ({{ group.skills.length }})
        </div>

        <template v-if="!group.label || !collapsedGroups.has(group.label)">
          <div
            v-for="row in group.skills"
            :key="row.skill_name"
            class="flex items-center gap-2 px-2 py-1 cursor-pointer border-b border-surface-dark text-xs hover:bg-surface-row-hover"
            :class="{
              'bg-surface-card border-l-2 border-l-accent-gold': selectedSkill === row.skill_name,
            }"
            @click="$emit('select', row.skill_name)">
            <span class="flex-1 text-text-primary/75 truncate">{{ row.skill_name }}</span>

            <!-- Session activity dot -->
            <span
              v-if="row.session"
              class="w-1.5 h-1.5 rounded-full bg-accent-gold shrink-0"
              title="Active this session"></span>

            <!-- Active skill badge -->
            <span
              v-if="row.isActive"
              class="text-[0.6rem] text-accent-gold/70 shrink-0"
              title="Equipped combat skill">&#x2694;</span>

            <!-- Level: always show effective (base+bonus) -->
            <span class="text-right min-w-20 shrink-0">
              <SkillLevelDisplay :skill="row">
                <span class="text-accent-gold font-bold">{{ skillTotalLevel(row) }}</span>
              </SkillLevelDisplay>
            </span>

            <!-- Compact progress bar -->
            <div class="w-12 h-1 bg-border-default rounded-sm overflow-hidden shrink-0">
              <div class="h-full bg-accent-gold/60 rounded-sm" :style="{ width: xpPercent(row) + '%' }"></div>
            </div>
          </div>
        </template>
      </template>

      <div v-if="totalVisible === 0" class="text-text-dim text-xs italic p-4 text-center">
        {{ store.skills.length === 0 ? 'No skill data loaded.' : 'No skills match your filters.' }}
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed } from 'vue'
import { useGameStateStore } from '../../stores/gameStateStore'
import { skillTotalLevel, type GameStateSkill } from '../../types/gameState'
import type { SkillInfo } from '../../types/gameData'
import type { SkillSessionData } from '../../stores/gameStateStore'
import SkillLevelDisplay from '../Shared/SkillLevelDisplay.vue'
interface SkillRow {
  skill_name: string
  level: number
  base_level: number
  bonus_levels: number
  xp: number
  tnl: number
  max_level: number
  cdnData: SkillInfo | null
  session: SkillSessionData | null
  isActive: boolean
}

interface SkillGroup {
  label: string
  skills: SkillRow[]
}

const props = defineProps<{
  selectedSkill: string | null
  cdnSkills: Record<string, SkillInfo>
}>()

defineEmits<{
  select: [skillName: string]
}>()

const store = useGameStateStore()

const filter = ref('')
const groupBy = ref<'type' | 'level' | 'none'>('type')
const sortBy = ref<'level' | 'name' | 'session'>('level')
const hideMaxed = ref(false)
const hideZero = ref(true)
const collapsedGroups = ref(new Set<string>())

function toggleGroup(label: string) {
  const next = new Set(collapsedGroups.value)
  if (next.has(label)) next.delete(label)
  else next.add(label)
  collapsedGroups.value = next
}

const activeSkillNames = computed(() => {
  const a = store.activeSkills
  if (!a) return new Set<string>()
  return new Set([a.skill1_name, a.skill2_name])
})

// Build enriched rows from persisted skills
const allRows = computed<SkillRow[]>(() => {
  const rows: SkillRow[] = store.skills.map((s: GameStateSkill) => ({
    skill_name: s.skill_name,
    level: s.level,
    base_level: s.base_level,
    bonus_levels: s.bonus_levels,
    xp: s.xp,
    tnl: s.tnl,
    max_level: s.max_level,
    cdnData: props.cdnSkills[s.skill_name] ?? null,
    session: store.sessionSkills[s.skill_name] ?? null,
    isActive: activeSkillNames.value.has(s.skill_name),
  }))

  // Add session-only skills (XP gained before login dump)
  for (const [name, session] of Object.entries(store.sessionSkills)) {
    if (!store.skillsByName[name]) {
      rows.push({
        skill_name: name,
        level: session.currentLevel,
        base_level: session.currentLevel,
        bonus_levels: 0,
        // Same as SkillsScreen's detail fallback: chat lines can't supply
        // into-level XP, so xp stays 0 until the persisted row exists.
        xp: 0,
        tnl: session.tnl,
        max_level: 0,
        cdnData: props.cdnSkills[name] ?? null,
        session,
        isActive: activeSkillNames.value.has(name),
      })
    }
  }

  return rows
})

// Apply filters
const filteredRows = computed(() => {
  let rows = allRows.value

  const f = filter.value.toLowerCase()
  if (f) {
    rows = rows.filter(r => r.skill_name.toLowerCase().includes(f))
  }
  if (hideMaxed.value) {
    rows = rows.filter(r => r.tnl > 0)
  }
  if (hideZero.value) {
    rows = rows.filter(r => r.level > 0)
  }

  return rows
})

// Sort
const sortedRows = computed(() => {
  const rows = [...filteredRows.value]
  switch (sortBy.value) {
    case 'level':
      rows.sort((a, b) => skillTotalLevel(b) - skillTotalLevel(a) || a.skill_name.localeCompare(b.skill_name))
      break
    case 'name':
      rows.sort((a, b) => a.skill_name.localeCompare(b.skill_name))
      break
    case 'session':
      rows.sort((a, b) => (b.session?.xpGained ?? 0) - (a.session?.xpGained ?? 0) || skillTotalLevel(b) - skillTotalLevel(a))
      break
  }
  return rows
})

// Group
const groupedSkills = computed<SkillGroup[]>(() => {
  const rows = sortedRows.value

  if (groupBy.value === 'none') {
    return [{ label: '', skills: rows }]
  }

  if (groupBy.value === 'level') {
    const ranges = [
      { label: 'Level 81+', min: 81, max: Infinity },
      { label: 'Level 61-80', min: 61, max: 80 },
      { label: 'Level 41-60', min: 41, max: 60 },
      { label: 'Level 21-40', min: 21, max: 40 },
      { label: 'Level 1-20', min: 1, max: 20 },
    ]
    return ranges
      .map(r => ({
        label: r.label,
        skills: rows.filter(s => skillTotalLevel(s) >= r.min && skillTotalLevel(s) <= r.max),
      }))
      .filter(g => g.skills.length > 0)
  }

  // Group by type (combat / non-combat / other)
  const combat: SkillRow[] = []
  const nonCombat: SkillRow[] = []
  const other: SkillRow[] = []

  for (const row of rows) {
    if (row.cdnData?.combat === true) combat.push(row)
    else if (row.cdnData?.combat === false) nonCombat.push(row)
    else other.push(row)
  }

  const groups: SkillGroup[] = []
  if (combat.length) groups.push({ label: 'Combat Skills', skills: combat })
  if (nonCombat.length) groups.push({ label: 'Non-Combat Skills', skills: nonCombat })
  if (other.length) groups.push({ label: 'Other', skills: other })
  return groups
})

const totalVisible = computed(() => filteredRows.value.length)

function xpPercent(row: SkillRow): number {
  if (row.tnl <= 0) return 100
  const total = row.xp + row.tnl
  if (total <= 0) return 0
  return Math.min(100, Math.round((row.xp / total) * 100))
}
</script>
