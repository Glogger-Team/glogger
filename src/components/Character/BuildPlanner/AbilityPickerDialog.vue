<template>
  <Teleport to="body">
    <Transition name="modal">
      <div v-if="show" class="fixed inset-0 z-50 flex items-center justify-center">
        <!-- Backdrop -->
        <div class="absolute inset-0 bg-black/60" @click="close" />

        <!-- Dialog -->
        <div class="relative bg-surface-base border border-border-default rounded-lg shadow-xl w-250 max-w-[95vw] max-h-[85vh] flex flex-col">
          <!-- Header -->
          <div class="flex items-center justify-between px-4 pt-4 pb-2 shrink-0">
            <div class="flex items-center gap-3">
              <h3 class="text-sm font-semibold text-text-primary">
                {{ barDisplayLabel }} — Slot {{ targetSlot + 1 }}
              </h3>
              <span v-if="currentSlotAbilityName" class="text-xs text-text-muted">
                ({{ currentSlotAbilityName }})
              </span>
            </div>
            <div class="flex items-center gap-2">
              <button
                v-if="currentSlotAbilityName"
                class="text-[10px] text-red-400/70 hover:text-red-400 cursor-pointer px-1.5 py-0.5 rounded border border-red-700/30 hover:border-red-700/50 transition-colors"
                title="Clear this slot"
                @click="clearSlot">
                Clear Slot
              </button>
              <button
                v-if="barAbilities.length > 0"
                class="text-[10px] text-red-400/70 hover:text-red-400 cursor-pointer px-1.5 py-0.5 rounded border border-red-700/30 hover:border-red-700/50 transition-colors"
                title="Clear all abilities from this bar"
                @click="clearAll">
                Clear All
              </button>
              <button
                class="text-text-muted hover:text-text-primary text-lg cursor-pointer leading-none"
                @click="close">
                &times;
              </button>
            </div>
          </div>

          <!-- Filter + toggles row -->
          <div class="px-4 pb-2 shrink-0 flex flex-col gap-1.5">
            <input
              v-model="searchFilter"
              type="text"
              placeholder="Search abilities..."
              class="bg-surface-elevated border border-border-default rounded px-2 py-1 text-xs text-text-primary w-full focus:border-accent-gold/50 focus:outline-none" />
            <div class="flex items-center gap-4 text-[10px]">
              <label class="flex items-center gap-1 text-text-secondary cursor-pointer">
                <input v-model="hideUnlearned" type="checkbox" class="accent-accent-gold cursor-pointer" />
                Hide unlearned abilities
              </label>
              <label class="flex items-center gap-1 text-text-secondary cursor-pointer">
                <input v-model="limitBySkillLevel" type="checkbox" class="accent-accent-gold cursor-pointer" />
                Limit tier by skill level
              </label>
              <span v-if="characterSkillLevel != null" class="text-text-dim ml-auto">
                {{ currentSkill }}: Lv {{ characterSkillLevel }}
              </span>
            </div>
          </div>

          <!-- Ability list -->
          <div v-if="loading" class="flex-1 flex items-center justify-center text-xs text-text-muted py-8">
            Loading abilities...
          </div>

          <div v-else-if="!currentSkill && bar !== 'sidebar'" class="flex-1 flex items-center justify-center text-xs text-text-muted py-8">
            Select a skill above to see available abilities.
          </div>

          <div v-else class="flex-1 overflow-y-auto px-4 pb-4 min-h-0">
            <!-- Main skill families -->
            <template v-if="filteredSkillFamilies.length > 0">
              <div v-if="bar === 'sidebar'" class="sticky top-0 bg-surface-base py-1 z-10">
                <h5 class="micro-label">
                  {{ currentSkill ?? 'Sidebar' }} ({{ filteredSkillFamilies.length }})
                </h5>
              </div>
              <div class="grid grid-cols-2 gap-1">
                <AbilityFamilyOption
                  v-for="entry in filteredSkillFamilies"
                  :key="entry.family.base_internal_name"
                  :family="entry.family"
                  :tiers="entry.visibleTiers"
                  :assigned-ids="assignedIdSet"
                  :assigned-families="assignedFamilyNames"
                  :mod-boost-count="getModBoostCount(entry.family.base_name)"
                  :default-tier-index="entry.defaultTierIndex"
                  @add="handleAdd" />
              </div>
            </template>

            <div v-if="filteredSkillFamilies.length === 0 && filteredSidebarFamilies.length === 0" class="text-xs text-text-dim py-4 text-center">
              No abilities found.
            </div>

            <!-- Sidebar-only skills -->
            <template v-if="bar === 'sidebar' && filteredSidebarFamilies.length > 0">
              <div class="sticky top-0 bg-surface-base py-1 z-10 mt-2">
                <h5 class="micro-label">
                  Sidebar Skills ({{ filteredSidebarFamilies.length }})
                </h5>
              </div>
              <div class="grid grid-cols-2 gap-1">
                <AbilityFamilyOption
                  v-for="entry in filteredSidebarFamilies"
                  :key="entry.family.base_internal_name"
                  :family="entry.family"
                  :tiers="entry.visibleTiers"
                  :assigned-ids="assignedIdSet"
                  :assigned-families="assignedFamilyNames"
                  :mod-boost-count="getModBoostCount(entry.family.base_name)"
                  :default-tier-index="entry.defaultTierIndex"
                  @add="handleAdd" />
              </div>
            </template>
          </div>
        </div>
      </div>
    </Transition>
  </Teleport>
</template>

<script setup lang="ts">
import { ref, computed, watch } from 'vue'
import { useBuildPlannerStore } from '../../../stores/buildPlannerStore'
import { useGameDataStore } from '../../../stores/gameDataStore'
import { useCharacterStore } from '../../../stores/characterStore'
import { useBuildCrossRef } from '../../../composables/useBuildCrossRef'
import { useSettingsStore } from '../../../stores/settingsStore'
import type { AbilityInfo, AbilityFamily } from '../../../types/gameData'
import { expandBuildSkill } from '../../../types/buildPlanner'
import AbilityFamilyOption from './AbilityFamilyOption.vue'
interface FamilyEntry {
  family: AbilityFamily
  tiers: AbilityInfo[]
  visibleTiers: AbilityInfo[]
  defaultTierIndex: number
}

const props = defineProps<{
  show: boolean
  bar: 'primary' | 'secondary' | 'sidebar'
  /** The slot position being filled or replaced */
  targetSlot: number
}>()

const emit = defineEmits<{
  'update:show': [value: boolean]
  added: [ability: AbilityInfo]
}>()

const store = useBuildPlannerStore()
const gameData = useGameDataStore()
const characterStore = useCharacterStore()
const settingsStore = useSettingsStore()
const { familyHasKnownAbility } = useBuildCrossRef()

const searchFilter = ref('')
const hideUnlearned = ref(false)
const limitBySkillLevel = ref(false)
const loading = ref(false)

const rawSkillFamilies = ref<FamilyEntry[]>([])
const rawSidebarFamilies = ref<FamilyEntry[]>([])

const SIDEBAR_SKILLS = ['FirstAid', 'ArmorPatching', 'SurvivalInstincts']

// ── Skill handling ──────────────────────────────────────────────────────────

const currentSkill = computed(() => {
  if (props.bar === 'primary') return store.activePreset?.skill_primary ?? null
  if (props.bar === 'secondary') return store.activePreset?.skill_secondary ?? null
  return null
})

// ── Character skill level ────────────────────────────────────────────────────

const characterSkillLevel = computed(() => {
  const skill = currentSkill.value
  if (!skill) return null
  return characterStore.skills.find(s => s.skill_name === skill)?.level ?? null
})

function getCharacterLevelForSkill(skill: string | null): number | null {
  if (!skill) return null
  return characterStore.skills.find(s => s.skill_name === skill)?.level ?? null
}

// ── Bar state ────────────────────────────────────────────────────────────────

const barDisplayLabel = computed(() => {
  if (props.bar === 'primary') return store.activePreset?.skill_primary ?? 'Primary'
  if (props.bar === 'secondary') return store.activePreset?.skill_secondary ?? 'Secondary'
  return 'Sidebar'
})

const barAbilities = computed(() => store.getBarAbilities(props.bar))

/** The ability currently in the target slot (if replacing) */
const currentSlotAbility = computed(() =>
  barAbilities.value.find(a => a.slot_position === props.targetSlot) ?? null
)

const currentSlotAbilityName = computed(() =>
  currentSlotAbility.value?.ability_name ?? null
)

/** Assigned IDs on this bar, excluding the ability in the slot we're replacing */
const assignedIdSet = computed(() => {
  const ids = new Set<number>()
  for (const a of barAbilities.value) {
    if (currentSlotAbility.value && a.slot_position === props.targetSlot) continue
    ids.add(a.ability_id)
  }
  return ids
})

/** Family base names assigned on this bar, excluding the slot we're replacing */
const assignedFamilyNames = computed(() => {
  const names = new Set<string>()
  for (const a of barAbilities.value) {
    if (currentSlotAbility.value && a.slot_position === props.targetSlot) continue
    names.add((a.ability_name ?? '').replace(/\s+\d+$/, ''))
  }
  return names
})

// ── Filtering ────────────────────────────────────────────────────────────────

function filterObtainable(tiers: AbilityInfo[]): AbilityInfo[] {
  return tiers.filter(a => {
    if ((a.raw_json as Record<string, unknown>)?.InternalAbility) return false
    if (settingsStore.settings.showUnobtainableItems) return true
    return !a.keywords.includes('Lint_NotLearnable') &&
      !a.keywords.includes('Lint_NotObtainable')
  })
}

function applyTierConstraints(entry: { tiers: AbilityInfo[], family: AbilityFamily }): { visibleTiers: AbilityInfo[], defaultTierIndex: number } {
  let visible = [...entry.tiers]
  const skillLevel = getCharacterLevelForSkill(entry.family.skill)

  // Known-ability truth: a non-empty known set for this skill means a
  // post-v74 import — hide families absent from the set. Empty set for this
  // skill = pre-v74 snapshot (or none selected): fall back to the level
  // proxy so the checkbox never hides everything on missing data.
  const skillKnownKeys = new Set(
    characterStore.snapshotAbilities
      .filter(a => a.skill_name === entry.family.skill)
      .map(a => a.ability_key),
  )
  if (hideUnlearned.value && skillKnownKeys.size > 0) {
    if (!familyHasKnownAbility(entry.tiers, entry.family.base_internal_name, skillKnownKeys)) {
      visible = []
    }
  } else if (hideUnlearned.value && skillLevel != null) {
    visible = visible.filter(t => (t.level ?? 0) <= skillLevel)
  }

  if (visible.length === 0) {
    return { visibleTiers: visible, defaultTierIndex: 0 }
  }

  let defaultIdx = visible.length - 1
  if (limitBySkillLevel.value && skillLevel != null) {
    for (let i = visible.length - 1; i >= 0; i--) {
      if ((visible[i].level ?? 0) <= skillLevel) {
        defaultIdx = i
        break
      }
      if (i === 0) defaultIdx = 0
    }
  }

  // Skip already-assigned tiers
  for (let i = defaultIdx; i >= 0; i--) {
    if (!assignedIdSet.value.has(visible[i].id)) {
      defaultIdx = i
      break
    }
  }

  return { visibleTiers: visible, defaultTierIndex: defaultIdx }
}

function filterFamilyEntries(entries: FamilyEntry[], q: string): FamilyEntry[] {
  // Hide families that are already on the bar (unless it's the one we're replacing)
  let filtered = entries.filter(e => {
    if (e.visibleTiers.length === 0) return false
    if (!assignedFamilyNames.value.has(e.family.base_name)) return true
    // Family is assigned elsewhere — hide it
    return false
  })

  if (!q) return filtered
  const lower = q.toLowerCase()
  return filtered.filter(e =>
    e.family.base_name.toLowerCase().includes(lower) ||
    e.tiers.some(t =>
      t.name.toLowerCase().includes(lower) ||
      (t.description?.toLowerCase().includes(lower) ?? false)
    )
  )
}

const processedSkillFamilies = computed(() =>
  rawSkillFamilies.value.map(entry => ({
    ...entry,
    ...applyTierConstraints(entry),
  }))
)

const processedSidebarFamilies = computed(() =>
  rawSidebarFamilies.value.map(entry => ({
    ...entry,
    ...applyTierConstraints(entry),
  }))
)

const filteredSkillFamilies = computed(() =>
  filterFamilyEntries(processedSkillFamilies.value, searchFilter.value)
)

const filteredSidebarFamilies = computed(() =>
  filterFamilyEntries(processedSidebarFamilies.value, searchFilter.value)
)

// ── Mod boost count ──────────────────────────────────────────────────────────

function getModBoostCount(baseName: string): number {
  const lower = baseName.toLowerCase()
  if (!lower) return 0
  return store.presetMods.filter(m =>
    m.power_name.toLowerCase().includes(lower)
  ).length
}

// ── Loading ──────────────────────────────────────────────────────────────────

async function resolveFamilies(skill: string, filterSidebar: boolean): Promise<FamilyEntry[]> {
  const families = await gameData.getAbilityFamiliesForSkill(skill)
  const entries: FamilyEntry[] = []

  for (const family of families) {
    const tierPromises = family.tier_ids.map(id => gameData.resolveAbility(id))
    let tiers = (await Promise.all(tierPromises)).filter((t): t is AbilityInfo => t !== null)
    tiers = filterObtainable(tiers)

    if (filterSidebar) {
      tiers = tiers.filter(t =>
        (t.raw_json as Record<string, unknown>)?.CanBeOnSidebar !== false
      )
    }

    if (tiers.length > 0) {
      entries.push({ family, tiers, visibleTiers: tiers, defaultTierIndex: tiers.length - 1 })
    }
  }

  return entries
}

async function loadAbilities() {
  loading.value = true
  try {
    const skill = currentSkill.value

    if (skill) {
      // Fold child skills (e.g. Fairy Magic under Mentalism) into the bar's skill.
      const results: FamilyEntry[] = []
      for (const s of expandBuildSkill(skill)) {
        try {
          results.push(...await resolveFamilies(s, false))
        } catch { /* skill might not exist */ }
      }
      rawSkillFamilies.value = results
    } else if (props.bar === 'sidebar') {
      const skills: string[] = [
        ...expandBuildSkill(store.activePreset?.skill_primary),
        ...expandBuildSkill(store.activePreset?.skill_secondary),
      ]

      const results: FamilyEntry[] = []
      for (const s of skills) {
        try {
          const entries = await resolveFamilies(s, true)
          results.push(...entries)
        } catch { /* skill might not exist */ }
      }
      rawSkillFamilies.value = results
    } else {
      rawSkillFamilies.value = []
    }

    if (props.bar === 'sidebar') {
      const sidebarResults: FamilyEntry[] = []
      for (const sideSkill of SIDEBAR_SKILLS) {
        try {
          const entries = await resolveFamilies(sideSkill, true)
          sidebarResults.push(...entries)
        } catch { /* skill might not exist */ }
      }
      rawSidebarFamilies.value = sidebarResults
    } else {
      rawSidebarFamilies.value = []
    }
  } finally {
    loading.value = false
  }
}

// ── Actions ──────────────────────────────────────────────────────────────────

async function handleAdd(ability: AbilityInfo, _familyBaseName: string) {
  await store.setAbilityAtSlot(props.bar, props.targetSlot, ability.id, ability.name)
  emit('added', ability)
  close()
}

async function clearSlot() {
  await store.clearAbilitySlot(props.bar, props.targetSlot)
  close()
}

async function clearAll() {
  await store.clearBar(props.bar)
  close()
}

function close() {
  emit('update:show', false)
}

// ── Watchers ─────────────────────────────────────────────────────────────────

watch(() => props.show, (open) => {
  if (open) {
    searchFilter.value = ''
    loadAbilities()
  }
}, { immediate: true })

watch(() => [store.activePreset?.skill_primary, store.activePreset?.skill_secondary], () => {
  if (props.show) loadAbilities()
})
</script>

<style scoped>
.modal-enter-active,
.modal-leave-active {
  transition: opacity 0.15s ease;
}
.modal-enter-from,
.modal-leave-to {
  opacity: 0;
}
</style>
