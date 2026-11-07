<template>
  <div class="flex flex-col gap-3 h-full overflow-hidden">
    <!-- Empty state -->
    <EmptyState
      v-if="store.skills.length === 0 && store.sessionSkillList.length === 0"
      primary="No skill data loaded."
      secondary="Log in to a character to see your skills." />

    <!-- Three-panel layout -->
    <PaneLayout
      v-else
      screen-key="character-skills"
      :left-pane="{ title: 'Skills', defaultWidth: 320, minWidth: 240, maxWidth: 500 }"
      :right-pane="{ title: 'Tracked Skills', defaultWidth: 260, minWidth: 200, maxWidth: 400, defaultCollapsed: !hasTrackedSkills }">
      <template #left>
        <div class="flex flex-col gap-2 h-full overflow-hidden">
          <!-- Summary bar -->
          <div class="flex items-center justify-between px-1">
            <div class="flex items-center gap-2">
              <span class="text-xs text-text-primary font-semibold">
                {{ settingsStore.settings.activeCharacterName ?? 'Character' }}
              </span>
              <span v-if="settingsStore.settings.activeServerName" class="text-[10px] text-text-muted">
                {{ settingsStore.settings.activeServerName }}
              </span>
            </div>
            <div class="text-[10px] text-text-dim">
              {{ totalLevels.toLocaleString() }} levels
              <span v-if="totalBonusLevels > 0">({{ totalBonusLevels }} bonus)</span>
              <span v-if="store.prodigySessionCount > 0" class="text-accent-gold" title="Resets on login or log reparse; lifetime total is not available from logs or exports">
                · {{ store.prodigySessionCount }} Prodigy this session
              </span>
            </div>
          </div>
          <SkillListPanel
            ref="skillListPanelRef"
            :selected-skill="selectedSkill"
            :cdn-skills="cdnSkillMap"
            @select="selectSkill" />
        </div>
      </template>

      <SkillDetailPanel :skill="selectedGameStateSkill" :cdn-skills="cdnSkillMap" />

      <template #right>
        <TrackedSkillsBar
          :selected-skill="selectedSkill"
          :show-empty="true"
          @select="selectSkill" />
      </template>
    </PaneLayout>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, nextTick, onMounted, watch } from 'vue'
import { useGameStateStore } from '../../stores/gameStateStore'
import { useGameDataStore } from '../../stores/gameDataStore'
import { useSettingsStore } from '../../stores/settingsStore'
import { pendingSkillSelection } from '../../composables/useViewNavigation'
import type { SkillInfo } from '../../types/gameData'
import type { GameStateSkill } from '../../types/gameState'
import PaneLayout from '../Shared/PaneLayout.vue'
import EmptyState from '../Shared/EmptyState.vue'
import TrackedSkillsBar from './TrackedSkillsBar.vue'
import SkillListPanel from './SkillListPanel.vue'
import SkillDetailPanel from './SkillDetailPanel.vue'

const store = useGameStateStore()
const gameData = useGameDataStore()
const settingsStore = useSettingsStore()

const selectedSkill = ref<string | null>(null)
const skillListPanelRef = ref<InstanceType<typeof SkillListPanel> | null>(null)

/** Skill named by the last search drill-in; kept so late layout shifts (CDN load) can re-reveal it. Cleared on manual select. */
const drilledSkill = ref<string | null>(null)

 /** Consume a pending search drill-in: select the named skill if known, else no-op. Always clears pending. */
 function consumePendingSkillSelection() {
   const name = pendingSkillSelection.value
   if (!name) return
   pendingSkillSelection.value = null
   if (!store.skillsByName[name] && !store.sessionSkills[name]) return
   selectedSkill.value = name
  drilledSkill.value = name
   nextTick(() => {
     skillListPanelRef.value?.revealSkill(name)
   })
 }

// CDN skill data cache for the list panel
const cdnSkillMap = ref<Record<string, SkillInfo>>({})

const totalLevels = computed(() =>
  store.skills.reduce((sum, s) => sum + s.level, 0)
)

const totalBonusLevels = computed(() =>
  store.skills.reduce((sum, s) => sum + s.bonus_levels, 0)
)

const hasTrackedSkills = computed(() => store.trackedSkillNames.length > 0)

const selectedGameStateSkill = computed<GameStateSkill | null>(() => {
  if (!selectedSkill.value) return null
  // Try persisted skills first
  const gs = store.skillsByName[selectedSkill.value]
  if (gs) return gs
  // Fall back to session-only skill
  const session = store.sessionSkills[selectedSkill.value]
  if (session) {
    return {
      skill_id: 0,
      skill_name: selectedSkill.value,
      level: session.currentLevel,
      base_level: session.currentLevel,
      bonus_levels: 0,
      // xp stays 0: chat level-up lines carry the killing-blow XP for the
      // level just landed on, not XP banked into it, so firstXp is not
      // into-level progress on that path. Exact-boundary hints untick on
      // transient session-only rows until the login dump supplies real xp.
      xp: 0,
      tnl: session.tnl,
      max_level: 0,
      last_confirmed_at: session.lastTimestamp,
      source: 'log',
    }
  }
  return null
})

function selectSkill(name: string) {
  selectedSkill.value = name
  drilledSkill.value = null
}

// Load all CDN skills for grouping/icons
async function loadCdnSkills() {
  if (gameData.status !== 'ready') return
  try {
    const all = await gameData.getAllSkills()
    const map: Record<string, SkillInfo> = {}
    for (const s of all) {
      map[s.name] = s
    }
    cdnSkillMap.value = map
  } catch (e) {
    console.warn('Failed to load CDN skills:', e)
  }
}
onMounted(() => {
  loadCdnSkills()
  consumePendingSkillSelection()
})

watch(pendingSkillSelection, () => {
  consumePendingSkillSelection()
})
watch(cdnSkillMap, () => {
  // Late CDN arrival regroups/resizes rows, invalidating the drill-in scroll — re-reveal.
  if (drilledSkill.value) skillListPanelRef.value?.revealSkill(drilledSkill.value)
})

watch(() => gameData.status, (s) => {
  if (s === 'ready') loadCdnSkills()
})
</script>
