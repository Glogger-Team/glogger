<template>
  <div
    class="h-full overflow-y-auto p-4"
    :class="{ 'flex items-center justify-center': !skill }">
    <div v-if="!skill" class="text-border-default italic">
      Select a skill to inspect
    </div>

    <!--
      Grid layout (matching mockup):
        Row 1: [Header/XP]  [Description]  [Rewards ↕]
        Row 2: [Bonus Lvls]  [Adv. Hints]  [Rewards ↕]
        Row 3: [Related Abilities ——————]   [Rewards ↕]
      Rewards spans all rows on the right.
    -->
    <div v-else class="skill-detail-grid">
      <!-- ═══ Row 1, Col 1: Header + XP ═══ -->
      <div class="flex flex-col gap-3" style="grid-area: header;">
        <!-- Skill name / level / badges -->
        <div>
          <div class="text-accent-gold text-base font-bold mb-0.5">{{ skill.skill_name }}</div>
          <div class="text-xs text-text-secondary mb-1">
            Level <SkillLevelDisplay :skill="skill"><span class="text-white font-bold">{{ skillTotalLevel(skill) }}</span></SkillLevelDisplay>
          </div>
          <div class="flex items-center gap-2">
            <span v-if="cdnData?.combat === true" class="text-[0.65rem] px-1.5 py-0.5 bg-red-900/30 border border-red-700/40 text-red-300 rounded">Combat</span>
            <span v-else-if="cdnData?.combat === false" class="text-[0.65rem] px-1.5 py-0.5 bg-blue-900/30 border border-blue-700/40 text-blue-300 rounded">Non-Combat</span>
            <span v-if="isActive" class="text-[0.65rem] px-1.5 py-0.5 bg-accent-gold/20 border border-accent-gold/40 text-accent-gold rounded">Active</span>
            <span v-if="isProdigyEnabled" class="text-[0.65rem] px-1.5 py-0.5 bg-accent-gold/20 border border-accent-gold/40 text-accent-gold rounded" title="Eligible for Prodigy levels">Prodigy</span>
          </div>
        </div>

        <!-- XP Progress with Track button -->
        <div class="flex flex-col gap-1">
          <div class="flex items-center gap-2">
            <div class="text-[0.65rem] uppercase tracking-widest text-text-dim flex-1">XP Progress</div>
            <button
              class="px-2 py-0.5 text-[0.65rem] rounded cursor-pointer transition-all border shrink-0"
              :class="isTracked
                ? 'bg-accent-gold/20 border-accent-gold/40 text-accent-gold hover:bg-accent-gold/30'
                : 'bg-surface-base border-border-default text-text-muted hover:text-accent-gold hover:border-accent-gold/40'"
              @click="toggleTrack">
              {{ isTracked ? 'Untrack' : 'Track' }}
            </button>
          </div>
          <div class="h-2 bg-border-default rounded-sm overflow-hidden">
            <div class="h-full bg-accent-gold rounded-sm transition-all duration-300" :style="{ width: xpPercent + '%' }"></div>
          </div>
          <div class="text-xs text-text-secondary">
            <template v-if="skill.tnl <= 0">MAX</template>
            <template v-else>{{ skill.xp.toLocaleString() }} / {{ (skill.xp + skill.tnl).toLocaleString() }} XP <span class="text-text-dim">({{ skill.tnl.toLocaleString() }} to next level)</span></template>
          </div>
        </div>

        <!-- Session Stats -->
        <div v-if="session" class="flex flex-col gap-1">
          <div class="text-[0.65rem] uppercase tracking-widest text-text-dim border-b border-surface-card pb-0.5">Session</div>
          <div class="grid grid-cols-2 gap-1">
            <div class="text-xs">
              <span class="text-text-muted">XP Gained:</span>
              <span class="text-text-primary font-bold ml-1">{{ session.xpGained.toLocaleString() }}</span>
            </div>
            <div class="text-xs">
              <span class="text-text-muted">XP/Hour:</span>
              <span class="text-text-primary font-bold ml-1">{{ xphr }}</span>
            </div>
            <div class="text-xs">
              <span class="text-text-muted">Levels Gained:</span>
              <span class="text-text-primary font-bold ml-1">{{ session.levelsGained }}</span>
            </div>
            <div class="text-xs">
              <span class="text-text-muted">Next Level:</span>
              <span class="text-text-primary font-bold ml-1">{{ ttl }}</span>
            </div>
          </div>
        </div>
      </div>

      <!-- ═══ Row 1, Col 2: Description ═══ -->
      <div v-if="cdnData?.description" class="flex flex-col gap-1" style="grid-area: desc;">
        <div class="text-[0.65rem] uppercase tracking-widest text-text-dim border-b border-surface-card pb-0.5">Description</div>
        <div class="text-xs text-text-secondary italic">{{ cdnData.description }}</div>
      </div>

      <!-- ═══ Row 2, Col 1: Bonus Level Sources ═══ -->
      <div v-if="bonusSources.length" class="flex flex-col gap-1.5" style="grid-area: bonus;">
        <div class="text-[0.65rem] uppercase tracking-widest text-text-dim border-b border-surface-card pb-0.5">
          Bonus Levels ({{ skill.bonus_levels }}/{{ cdnData?.max_bonus_levels ?? '?' }})
        </div>
        <div class="flex flex-col gap-0.5">
          <div
            v-for="src in bonusSources"
            :key="src.skillName + '-' + src.level"
            class="text-xs flex items-center gap-2 px-2 py-0.5 bg-surface-inset">
            <span class="w-4 shrink-0 text-center">
              <span v-if="src.achieved" class="text-green-500">&#10003;</span>
              <span v-else class="text-text-dim">&#9744;</span>
            </span>
            <SkillInline :reference="src.skillName" />
            <span class="text-text-muted">Lv {{ src.level }}</span>
          </div>
        </div>
      </div>

      <!-- ═══ Row 2, Col 2: Advancement Hints ═══ -->
      <div v-if="allHints.length" class="flex flex-col gap-1.5" style="grid-area: hints;">
        <div class="text-[0.65rem] uppercase tracking-widest text-text-dim border-b border-surface-card pb-0.5">Advancement Hints</div>
        <div class="flex flex-col gap-0.5">
          <div
            v-for="hint in allHints"
            :key="hint.level"
            class="text-xs flex gap-2 px-2 py-0.5"
            :class="hint.attained ? 'bg-[#101510] opacity-70' : 'bg-surface-inset'">
            <span class="w-4 shrink-0 text-center">
              <span v-if="hint.attained" class="text-green-500">&#10003;</span>
            </span>
            <span class="text-text-muted min-w-10 shrink-0">Lv {{ hint.level }}:</span>
            <span class="text-text-secondary">{{ hint.text }}</span>
          </div>
        </div>
      </div>

      <!-- ═══ Row 3, Col 1-2: Related Abilities + Keywords ═══ -->
      <div class="flex flex-col gap-4" style="grid-area: abilities;">
        <div v-if="knownAbilityKeys.length" class="flex flex-col gap-1.5">
          <div class="text-[0.65rem] uppercase tracking-widest text-text-dim border-b border-surface-card pb-0.5">
            Known Abilities — Report ({{ knownAbilityKeys.length }})
          </div>
          <div class="flex flex-wrap gap-1">
            <span
              v-for="key in knownAbilityKeys"
              :key="key"
              class="text-[0.72rem] px-1.5 py-0.5 bg-surface-card border border-border-subtle text-entity-ability"
              :title="`Known in report: ${key}`">{{ key }}</span>
          </div>
        </div>
        <div v-if="abilities.length" class="flex flex-col gap-1.5">
          <div class="text-[0.65rem] uppercase tracking-widest text-text-dim border-b border-surface-card pb-0.5">
            Related Abilities ({{ abilities.length }})
          </div>
          <div class="flex flex-col gap-0.5">
            <div
              v-for="ability in abilities"
              :key="ability.id"
              class="text-xs flex items-center gap-2 px-2 py-0.5 border-b border-[#151515] hover:bg-surface-base">
              <span class="text-text-muted text-[0.72rem] min-w-10 shrink-0">[Lv {{ ability.level || 0 }}]</span>
              <AbilityInline :ability="ability" class="flex-1" />
              <span v-if="abilitySourceLabels[ability.id]" class="text-text-dim text-[0.6rem] shrink-0">
                {{ abilitySourceLabels[ability.id] }}
              </span>
            </div>
          </div>
        </div>

        <div v-if="cdnData?.keywords?.length" class="flex flex-col gap-1.5">
          <div class="text-[0.65rem] uppercase tracking-widest text-text-dim border-b border-surface-card pb-0.5">Keywords</div>
          <div class="flex flex-wrap gap-1">
            <span
              v-for="kw in cdnData.keywords"
              :key="kw"
              class="text-[0.72rem] px-1.5 py-0.5 bg-surface-card border border-border-subtle text-entity-item">{{ kw }}</span>
          </div>
        </div>
      </div>

      <!-- ═══ Row 1-3, Col 3: Rewards (spans full height) ═══ -->
      <div v-if="allRewards.length" class="flex flex-col gap-1.5" style="grid-area: rewards;">
        <div class="text-[0.65rem] uppercase tracking-widest text-text-dim border-b border-surface-card pb-0.5">Rewards</div>
        <div class="flex flex-col gap-0.5">
          <div
            v-for="reward in allRewards"
            :key="reward.level"
            class="text-xs flex items-center gap-2 px-2 py-0.5"
            :class="reward.attained ? 'bg-[#101510] opacity-70' : 'bg-surface-inset'">
            <span class="w-4 shrink-0 text-center">
              <span v-if="reward.attained" class="text-green-500">&#10003;</span>
            </span>
            <span class="text-text-muted min-w-10 shrink-0">Lv {{ reward.level }}:</span>
            <!-- Ability reward -->
            <template v-if="reward.ability">
              <AbilityInline :ability="reward.ability" />
            </template>
            <template v-else-if="reward.abilityKey">
              <span class="text-entity-ability">{{ reward.abilityKey }}</span>
            </template>
            <!-- Bonus to skill reward -->
            <template v-if="reward.bonusSkill">
              <span class="text-text-secondary">Bonus to</span>
              <SkillInline :reference="reward.bonusSkill" />
            </template>
            <!-- Recipe reward -->
            <template v-if="reward.recipeKey">
              <RecipeInline :reference="reward.recipeKey" />
            </template>
            <!-- Fallback -->
            <span v-if="!reward.ability && !reward.abilityKey && !reward.bonusSkill && !reward.recipeKey" class="text-text-secondary">
              {{ reward.fallback }}
            </span>
          </div>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, watch } from 'vue'
import { useGameStateStore } from '../../stores/gameStateStore'
import { useGameDataStore } from '../../stores/gameDataStore'
import { useCharacterStore } from '../../stores/characterStore'
import { skillTotalLevel, type GameStateSkill } from '../../types/gameState'
import type { SkillInfo, AbilityInfo, EntitySources } from '../../types/gameData'
import SkillLevelDisplay from '../Shared/SkillLevelDisplay.vue'
import AbilityInline from '../Shared/Ability/AbilityInline.vue'
import SkillInline from '../Shared/Skill/SkillInline.vue'
import RecipeInline from '../Shared/Recipe/RecipeInline.vue'

interface ParsedReward {
  level: number
  ability: AbilityInfo | null
  abilityKey: string | null
  bonusSkill: string | null
  recipeKey: string | null
  fallback: string
  attained: boolean
}

interface BonusSource {
  skillName: string
  level: number
  achieved: boolean
}

const props = defineProps<{
  skill: GameStateSkill | null
  cdnSkills?: Record<string, SkillInfo>
}>()
const store = useGameStateStore()
const gameData = useGameDataStore()
const characterStore = useCharacterStore()

/** Bare ability keys stored on the selected snapshot for this skill (import-time truth). */
const knownAbilityKeys = computed(() => {
  if (!props.skill) return [] as string[]
  const name = props.skill.skill_name
  return characterStore.snapshotAbilities
    .filter(a => a.skill_name === name)
    .map(a => a.ability_key)
    .sort((a, b) => a.localeCompare(b))
})

const cdnData = ref<SkillInfo | null>(null)
const abilities = ref<AbilityInfo[]>([])
const abilitySourceLabels = ref<Record<number, string>>({})

const session = computed(() =>
  props.skill ? store.sessionSkills[props.skill.skill_name] ?? null : null
)

const isActive = computed(() => {
  if (!props.skill || !store.activeSkills) return false
  return store.activeSkills.skill1_name === props.skill.skill_name
    || store.activeSkills.skill2_name === props.skill.skill_name
})

const isTracked = computed(() =>
  props.skill ? store.isSkillTracked(props.skill.skill_name) : false
)

const isProdigyEnabled = computed(() =>
  !!((cdnData.value?.raw_json as unknown as Record<string, unknown> | undefined)?.ProdigyEnabledInteractionFlag)
)

async function toggleTrack() {
  if (!props.skill) return
  if (isTracked.value) {
    await store.untrackSkill(props.skill.skill_name)
  } else {
    await store.trackSkill(props.skill.skill_name)
  }
}

const xphr = computed(() => {
  if (!session.value) return '0'
  return store.xpPerHour(session.value).toLocaleString()
})

const ttl = computed(() => {
  if (!session.value) return '—'
  return store.timeToNextLevel(session.value)
})

const xpPercent = computed(() => {
  if (!props.skill || props.skill.tnl <= 0) return 100
  const total = props.skill.xp + props.skill.tnl
  if (total <= 0) return 0
  return Math.min(100, Math.round((props.skill.xp / total) * 100))
})

// Parse all advancement hints from CDN data, marking attained levels.
// Favor-gated XP-cap hints require the BASE level: bonus levels don't unlock
// them. A hint at exactly base level counts only with XP banked into the next
// level (xp > 0); at xp == 0 the boundary hasn't been crossed yet.
// Chat-sourced rows are exempt from the boundary clause: their xp is the
// killing-blow XP of the line that reached the level, not XP banked into it,
// so it must not tick the boundary until the login dump supplies real xp.
const allHints = computed(() => {
  if (!cdnData.value?.advancement_hints || !props.skill) return []
  const hints = cdnData.value.advancement_hints as Record<string, string>
  const base = props.skill!.base_level
  const xpIntoNext = props.skill!.xp
  const chatSourced = (props.skill!.source as string) === 'chat'
  return Object.entries(hints)
    .map(([level, text]) => ({ level: Number(level), text, attained: base > Number(level) || (base === Number(level) && !chatSourced && xpIntoNext > 0) }))
    .sort((a, b) => a.level - b.level)
})

// Parse all rewards from CDN data, marking attained vs upcoming
const allRewards = computed<ParsedReward[]>(() => {
  if (!cdnData.value?.rewards || !props.skill) return []
  const rewards = cdnData.value.rewards as Record<string, Record<string, unknown>>

  // Build a lookup from internal ability key to AbilityInfo
  const abilityByInternalName: Record<string, AbilityInfo> = {}
  for (const ab of abilities.value) {
    const internalName = ab.raw_json?.InternalName as string | undefined
    if (internalName) abilityByInternalName[internalName] = ab
    abilityByInternalName[ab.name] = ab
  }

  const currentLevel = props.skill!.level

  return Object.entries(rewards)
    .map(([level, data]): ParsedReward => {
      const abilityKey = data.Ability as string | undefined
      const bonusSkill = data.BonusToSkill as string | undefined
      const recipeKey = data.Recipe as string | undefined

      let resolvedAbility: AbilityInfo | null = null
      if (abilityKey) {
        resolvedAbility = abilityByInternalName[abilityKey] ?? null
      }

      return {
        level: Number(level),
        ability: resolvedAbility,
        abilityKey: resolvedAbility ? null : (abilityKey ?? null),
        bonusSkill: bonusSkill ?? null,
        recipeKey: recipeKey ?? null,
        fallback: !abilityKey && !bonusSkill && !recipeKey ? JSON.stringify(data) : '',
        attained: Number(level) <= currentLevel,
      }
    })
    .sort((a, b) => a.level - b.level)
})

// Compute which skills grant bonus levels to this skill (reverse lookup)
const bonusSources = computed<BonusSource[]>(() => {
  if (!props.skill || !props.cdnSkills) return []
  // BonusToSkill values in CDN rewards use internal names (e.g. "FireMagic"),
  // so resolve the target skill's internal name for comparison
  const targetInternalName = cdnData.value?.internal_name ?? null
  const targetDisplayName = props.skill.skill_name
  const sources: BonusSource[] = []

  for (const [skillName, skillInfo] of Object.entries(props.cdnSkills)) {
    if (!skillInfo.rewards) continue
    const rewards = skillInfo.rewards as Record<string, Record<string, unknown>>
    for (const [level, data] of Object.entries(rewards)) {
      if (data.BonusToSkill === targetInternalName || data.BonusToSkill === targetDisplayName) {
        // Check if the player has reached this level in the granting skill
        const playerSkill = store.skillsByName[skillName]
        sources.push({
          skillName,
          level: Number(level),
          achieved: playerSkill ? playerSkill.level >= Number(level) : false,
        })
      }
    }
  }

  return sources.sort((a, b) => a.skillName.localeCompare(b.skillName) || a.level - b.level)
})

/** Summarize the primary source type for an ability as a short label */
function summarizeSource(sources: EntitySources): string {
  // Check CDN sources first
  for (const s of sources.cdn_sources) {
    switch (s.source_type) {
      case 'Skill': return 'Skill level-up'
      case 'Training': return s.npc ? `Trainer: ${s.npc}` : 'Trainer'
      case 'Quest': return 'Quest reward'
      case 'HangOut': return 'Hang out'
      case 'Effect': return 'Effect'
      case 'NpcGift': return 'NPC gift'
      case 'Monster': return 'Monster drop'
    }
  }
  // Check items
  if (sources.bestowed_by_items.length > 0) {
    return 'Skill book'
  }
  // Check quests
  if (sources.rewarded_by_quests.length > 0) {
    return 'Quest reward'
  }
  return ''
}

// Load CDN data + abilities + sources when skill changes
watch(() => props.skill?.skill_name, async (name) => {
  cdnData.value = null
  abilities.value = []
  abilitySourceLabels.value = {}
  if (!name) return

  try {
    const [info, abs] = await Promise.all([
      gameData.resolveSkill(name),
      gameData.getAbilitiesForSkill(name),
    ])
    cdnData.value = info
    abilities.value = abs.sort((a, b) => (a.level || 0) - (b.level || 0))

    // Load sources for each ability in the background
    const labels: Record<number, string> = {}
    const sourcePromises = abs.map(async (ab) => {
      try {
        const sources = await gameData.getAbilitySources(ab.id)
        const label = summarizeSource(sources)
        if (label) labels[ab.id] = label
      } catch {
        // silently skip failed source lookups
      }
    })
    await Promise.all(sourcePromises)
    abilitySourceLabels.value = labels
  } catch (e) {
    console.warn('Failed to load skill detail:', e)
  }
}, { immediate: true })
</script>

<style scoped>
.skill-detail-grid {
  display: grid;
  grid-template-columns: 1fr 1fr 1fr;
  grid-template-rows: auto auto auto;
  grid-template-areas:
    "header    desc      rewards"
    "bonus     hints     rewards"
    "abilities abilities rewards";
  gap: 1rem;
  align-items: start;
}
</style>
