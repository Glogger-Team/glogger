<template>
  <div class="flex flex-col gap-3 h-full min-h-0">
    <!-- Bar header -->
    <div class="flex items-center justify-between px-1">
      <div class="flex items-center gap-2">
        <h3 class="dialog-title">{{ barLabel }} Abilities</h3>
        <span class="text-xs text-text-muted">
          {{ barAbilities.length }}/{{ maxSlots }} slots
        </span>
      </div>
    </div>

    <div class="flex-1 flex gap-3 min-h-0">
      <!-- Assigned abilities -->
      <div class="w-72 shrink-0 flex flex-col gap-1.5 min-h-0 overflow-y-auto">
        <h4 class="panel-label">Ability Bar</h4>
        <div v-if="barAbilities.length === 0" class="text-xs text-text-dim py-2">
          No abilities assigned yet. Browse and add from the right.
        </div>
        <div
          v-for="ability in barAbilities"
          :key="ability.id"
          class="flex items-center gap-2 px-2 py-1.5 rounded text-sm bg-surface-elevated border border-border-default group">
          <span class="text-[10px] text-text-dim w-4 shrink-0 text-center">{{ ability.slot_position + 1 }}</span>
          <AbilityInline
            v-if="resolvedAbilities[ability.ability_id]"
            :ability="resolvedAbilities[ability.ability_id]!" />
          <span v-else class="text-text-secondary text-xs">{{ ability.ability_name ?? `#${ability.ability_id}` }}</span>
          <span class="flex-1" />
          <button
            class="text-red-400/60 hover:text-red-400 text-xs opacity-0 group-hover:opacity-100 transition-opacity cursor-pointer shrink-0"
            title="Remove ability"
            @click="store.removeAbility(ability)">
            x
          </button>
        </div>
      </div>

      <!-- Available abilities browser -->
      <div class="flex-1 flex flex-col gap-2 min-h-0">
        <div class="flex items-center gap-2">
          <h4 class="panel-label">Available Abilities</h4>
          <input
            v-model="abilityFilter"
            type="text"
            placeholder="Filter abilities..."
            class="bg-surface-elevated border border-border-default rounded px-2 py-0.5 text-xs text-text-primary flex-1 max-w-60" />
        </div>

        <div v-if="loading" class="text-xs text-text-muted py-4 text-center">
          Loading abilities...
        </div>

        <div v-else class="flex-1 overflow-y-auto flex flex-col gap-1">
          <!-- Skill ability families -->
          <template v-if="skillFamilyEntries.length > 0">
            <div class="sticky top-0 bg-surface-base py-1 z-10">
              <h5 class="text-[10px] font-semibold uppercase tracking-wider text-blue-400">
                {{ skillName }} ({{ filteredSkillFamilies.length }})
              </h5>
            </div>
            <AbilityFamilyOption
              v-for="entry in filteredSkillFamilies"
              :key="entry.family.base_internal_name"
              :family="entry.family"
              :tiers="entry.tiers"
              :assigned-ids="assignedIdSet"
              :mod-boost-count="getModBoostCount(entry.family.base_name)"
              @add="handleAdd" />
          </template>

          <!-- Sidebar abilities from other skills -->
          <template v-if="store.activeBar === 'sidebar' && sidebarFamilyEntries.length > 0">
            <div class="sticky top-0 bg-surface-base py-1 z-10">
              <h5 class="micro-label">
                Sidebar Skills ({{ filteredSidebarFamilies.length }})
              </h5>
            </div>
            <AbilityFamilyOption
              v-for="entry in filteredSidebarFamilies"
              :key="entry.family.base_internal_name"
              :family="entry.family"
              :tiers="entry.tiers"
              :assigned-ids="assignedIdSet"
              :mod-boost-count="getModBoostCount(entry.family.base_name)"
              @add="handleAdd" />
          </template>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, watch, onMounted } from 'vue'
import { useBuildPlannerStore } from '../../../stores/buildPlannerStore'
import { useGameDataStore } from '../../../stores/gameDataStore'
import { useSettingsStore } from '../../../stores/settingsStore'
import type { AbilityInfo, AbilityFamily } from '../../../types/gameData'
import { expandBuildSkill } from '../../../types/buildPlanner'
import AbilityInline from '../../Shared/Ability/AbilityInline.vue'
import AbilityFamilyOption from './AbilityFamilyOption.vue'

interface FamilyEntry {
  family: AbilityFamily
  tiers: AbilityInfo[]
}

const store = useBuildPlannerStore()
const gameData = useGameDataStore()
const settingsStore = useSettingsStore()

const skillFamilyEntries = ref<FamilyEntry[]>([])
const sidebarFamilyEntries = ref<FamilyEntry[]>([])
const resolvedAbilities = ref<Record<number, AbilityInfo>>({})
const loading = ref(false)
const abilityFilter = ref('')

const SIDEBAR_SKILLS = ['FirstAid', 'ArmorPatching', 'SurvivalInstincts']

const barLabel = computed(() => {
  if (store.activeBar === 'primary') return store.activePreset?.skill_primary ?? 'Primary'
  if (store.activeBar === 'secondary') return store.activePreset?.skill_secondary ?? 'Secondary'
  return 'Sidebar'
})

const skillName = computed(() => {
  if (store.activeBar === 'primary') return store.activePreset?.skill_primary ?? 'Primary'
  if (store.activeBar === 'secondary') return store.activePreset?.skill_secondary ?? 'Secondary'
  return 'Sidebar'
})

const maxSlots = computed(() => store.activeBar === 'sidebar' ? 10 : 6)

const barAbilities = computed(() => {
  if (!store.activeBar) return []
  return store.getBarAbilities(store.activeBar)
})

const assignedIdSet = computed(() => {
  return new Set(barAbilities.value.map(a => a.ability_id))
})

function filterFamilyEntries(entries: FamilyEntry[], q: string): FamilyEntry[] {
  if (!q) return entries
  const lower = q.toLowerCase()
  return entries.filter(e =>
    e.family.base_name.toLowerCase().includes(lower) ||
    e.tiers.some(t =>
      t.name.toLowerCase().includes(lower) ||
      (t.description?.toLowerCase().includes(lower) ?? false)
    )
  )
}

const filteredSkillFamilies = computed(() =>
  filterFamilyEntries(skillFamilyEntries.value, abilityFilter.value)
)

const filteredSidebarFamilies = computed(() =>
  filterFamilyEntries(sidebarFamilyEntries.value, abilityFilter.value)
)

/** Count how many assigned mods reference an ability by base name */
function getModBoostCount(baseName: string): number {
  const lower = baseName.toLowerCase()
  if (!lower) return 0
  return store.presetMods.filter(m =>
    m.power_name.toLowerCase().includes(lower)
  ).length
}

async function handleAdd(ability: AbilityInfo, _familyBaseName: string) {
  if (store.activeBar) {
    await store.setAbilityAtSlot(store.activeBar, 0, ability.id, ability.name)
  }
}

function filterObtainable(tiers: AbilityInfo[]): AbilityInfo[] {
  return tiers.filter(a => {
    // Always hide internal abilities in the build planner
    if ((a.raw_json as Record<string, unknown>)?.InternalAbility) return false
    if (settingsStore.settings.showUnobtainableItems) return true
    return !a.keywords.includes('Lint_NotLearnable') &&
      !a.keywords.includes('Lint_NotObtainable')
  })
}

async function resolveFamilies(skill: string, filterSidebar: boolean): Promise<FamilyEntry[]> {
  const families = await gameData.getAbilityFamiliesForSkill(skill)
  const entries: FamilyEntry[] = []

  for (const family of families) {
    // Resolve all tiers in parallel
    const tierPromises = family.tier_ids.map(id => gameData.resolveAbility(id))
    let tiers = (await Promise.all(tierPromises)).filter((t): t is AbilityInfo => t !== null)

    // Filter unobtainable
    tiers = filterObtainable(tiers)

    // For sidebar, filter out abilities that can't be on sidebar
    if (filterSidebar) {
      tiers = tiers.filter(t =>
        (t.raw_json as Record<string, unknown>)?.CanBeOnSidebar !== false
      )
    }

    if (tiers.length > 0) {
      entries.push({ family, tiers })
    }
  }

  return entries
}

async function loadAbilities() {
  loading.value = true
  try {
    // Load families for the bar's skill
    const skill = store.activeBar === 'primary'
      ? store.activePreset?.skill_primary
      : store.activeBar === 'secondary'
        ? store.activePreset?.skill_secondary
        : null

    if (skill) {
      // Fold child skills (e.g. Fairy Magic under Mentalism) into the bar's skill.
      const results: FamilyEntry[] = []
      for (const s of expandBuildSkill(skill)) {
        try {
          results.push(...await resolveFamilies(s, false))
        } catch { /* skill might not exist */ }
      }
      skillFamilyEntries.value = results
    } else {
      skillFamilyEntries.value = []
    }

    // For sidebar bar, also load sidebar-eligible ability families
    if (store.activeBar === 'sidebar') {
      const sidebarResults: FamilyEntry[] = []
      for (const sideSkill of SIDEBAR_SKILLS) {
        try {
          const entries = await resolveFamilies(sideSkill, true)
          sidebarResults.push(...entries)
        } catch {
          // Skill might not exist
        }
      }
      sidebarFamilyEntries.value = sidebarResults
    } else {
      sidebarFamilyEntries.value = []
    }

    // Build resolution map for assigned abilities (for the left panel)
    await resolveAssignedAbilities()
  } finally {
    loading.value = false
  }
}

async function resolveAssignedAbilities() {
  const allTiers = [
    ...skillFamilyEntries.value.flatMap(e => e.tiers),
    ...sidebarFamilyEntries.value.flatMap(e => e.tiers),
  ]
  const map: Record<number, AbilityInfo> = {}
  for (const a of allTiers) {
    map[a.id] = a
  }
  resolvedAbilities.value = map
}

watch(() => store.activeBar, () => {
  abilityFilter.value = ''
  if (store.activeBar) loadAbilities()
})

watch(() => [store.activePreset?.skill_primary, store.activePreset?.skill_secondary], () => {
  if (store.activeBar) loadAbilities()
})

onMounted(() => {
  if (store.activeBar) loadAbilities()
})
</script>
