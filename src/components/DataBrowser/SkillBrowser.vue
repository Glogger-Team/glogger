<template>
  <PaneLayout screen-key="db-skills" :left-pane="{ title: 'Skills', defaultWidth: 360, minWidth: 280, maxWidth: 500 }">
    <template #left>
      <!-- Status banner if data not ready -->
      <div v-if="store.status !== 'ready'" class="p-4 text-sm">
        <span v-if="store.status === 'loading'" class="text-accent-gold"
          >⟳ Loading game data…</span
        >
        <span v-else-if="store.status === 'error'" class="text-accent-red"
          >✕ {{ store.errorMessage }}</span
        >
      </div>

      <template v-else>
      <div class="flex flex-col gap-2 h-full overflow-hidden">
        <div class="flex items-center gap-2 relative">
          <input
            v-model="query"
            class="input flex-1"
            placeholder="Search skills…"
            autofocus />
          <span v-if="loading" class="text-accent-gold text-sm animate-spin">⟳</span>
          <span v-else-if="filteredSkills.length" class="text-text-dim text-xs min-w-6 text-right">{{
            filteredSkills.length
          }}</span>
        </div>

        <div v-if="!allSkills.length && !loading" class="text-text-dim text-xs italic py-1">
          No skills loaded
        </div>

        <div v-else-if="filteredSkills.length === 0 && query" class="text-text-dim text-xs italic py-1">
          No skills found for "{{ query }}"
        </div>

        <ul ref="listRef" v-else class="list-none m-0 p-0 overflow-y-auto flex-1 border border-surface-elevated">
          <li
            v-for="(skill, idx) in filteredSkills"
            :key="skill.id"
            class="flex items-baseline gap-2 px-2 py-1 cursor-pointer border-b border-surface-dark text-xs hover:bg-surface-row-hover"
            :class="{
              'bg-surface-card border-l-2 border-l-accent-gold': selected?.id === skill.id,
              'bg-surface-elevated': selectedIndex === idx && selected?.id !== skill.id
            }"
            @click="selectSkill(skill)">
            <span class="text-text-dim text-[0.72rem] min-w-12 shrink-0">#{{ skill.id }}</span>
            <span class="text-text-primary/75 flex-1 overflow-hidden text-ellipsis whitespace-nowrap">{{ skill.name }}</span>
          </li>
        </ul>
      </div>
      </template>
    </template>

    <!-- Right panel: skill detail -->
    <div
      class="h-full overflow-y-auto border-l border-surface-elevated p-4 flex flex-col gap-4"
      :class="{ 'items-center justify-center': !selected }">
        <div v-if="!selected" class="text-border-default italic">
          Select a skill to inspect
        </div>

        <template v-else>
          <div class="flex gap-3 items-start">
            <!-- Icon -->
            <div class="shrink-0">
              <img
                v-if="iconSrc"
                :src="iconSrc"
                class="w-12 h-12 [image-rendering:pixelated] border border-border-default"
                alt="skill icon" />
              <div v-else-if="iconLoading" class="w-12 h-12 bg-surface-base border border-surface-elevated flex items-center justify-center text-[0.65rem] text-accent-gold animate-spin">
                ⟳
              </div>
              <div v-else-if="selected.icon_id" class="w-12 h-12 bg-surface-base border border-surface-elevated flex items-center justify-center text-[0.65rem] text-text-dim">
                {{ selected.icon_id }}
              </div>
              <div v-else class="w-12 h-12 bg-surface-base border border-surface-elevated flex items-center justify-center text-[0.65rem] text-border-default">—</div>
            </div>

            <div class="flex-1 min-w-0">
              <div class="text-accent-gold text-base font-bold mb-1">{{ selected.name }}</div>
              <div class="text-xs text-text-dim mb-1">
                ID: <span class="text-text-secondary font-mono">{{ selected.id }}</span>
                <template v-if="selected.icon_id">
                  · Icon:
                  <span class="text-text-secondary font-mono">{{ selected.icon_id }}</span></template
                >
                <template v-if="selected.xp_table">
                  · XP Table:
                  <span class="text-text-secondary font-mono">{{ selected.xp_table }}</span></template
                >
              </div>
              <div v-if="selected.description" class="text-xs text-text-secondary italic">
                {{ selected.description }}
              </div>
            </div>

            <button
              class="bg-transparent border-none cursor-pointer px-1 py-0 text-sm shrink-0 transition-colors"
              :class="isFav ? 'text-accent-gold' : 'text-text-dim hover:text-accent-gold'"
              :title="isFav ? 'Remove from favorites' : 'Add to favorites'"
              @click="dataBrowserStore.toggleFavorite({ type: 'skill', reference: selected.name, label: selected.name })"
            >&#x2605;</button>
            <button class="bg-transparent border-none text-text-dim cursor-pointer px-1 py-0 text-sm shrink-0 hover:text-accent-red" @click="clearSelection">✕</button>
          </div>

          <!-- Skill Details -->
          <div class="flex flex-col gap-1.5">
            <div class="text-[0.65rem] uppercase tracking-widest text-text-dim border-b border-surface-card pb-0.5">Details</div>
            <div class="grid grid-cols-[repeat(auto-fit,minmax(160px,1fr))] gap-1.5">
              <div class="text-xs flex gap-2">
                <span class="text-text-muted min-w-24">Type:</span>
                <span class="text-text-secondary">{{ selected.combat ? 'Combat' : 'Non-Combat' }}</span>
              </div>
              <div v-if="selected.max_bonus_levels" class="text-xs flex gap-2">
                <span class="text-text-muted min-w-24">Max Bonus Lvl:</span>
                <span class="text-text-secondary">{{ selected.max_bonus_levels }}</span>
              </div>
              <div v-if="selected.guest_level_cap" class="text-xs flex gap-2">
                <span class="text-text-muted min-w-24">Guest Cap:</span>
                <span class="text-text-secondary">{{ selected.guest_level_cap }}</span>
              </div>
              <div v-if="selected.advancement_table" class="text-xs flex gap-2">
                <span class="text-text-muted min-w-24">Advancement:</span>
                <span class="text-text-secondary font-mono">{{ selected.advancement_table }}</span>
              </div>
              <div v-if="selected.parents?.length" class="text-xs flex gap-2">
                <span class="text-text-muted min-w-24">Parents:</span>
                <span class="text-text-secondary">{{ selected.parents.join(', ') }}</span>
              </div>
              <div v-if="selected.hide_when_zero" class="text-xs flex gap-2">
                <span class="text-text-muted min-w-24">Hidden at 0:</span>
                <span class="text-text-secondary">Yes</span>
              </div>
            </div>
          </div>

          <!-- Advancement Hints -->
          <div v-if="selected.advancement_hints && Object.keys(selected.advancement_hints).length" class="flex flex-col gap-1.5">
            <div class="text-[0.65rem] uppercase tracking-widest text-text-dim border-b border-surface-card pb-0.5">Advancement Hints</div>
            <div class="flex flex-col gap-1">
              <div
                v-for="(hint, level) in selected.advancement_hints"
                :key="level"
                class="text-xs flex gap-2 px-2 py-0.5 bg-surface-inset">
                <span class="text-text-muted min-w-14 shrink-0">Lv {{ level }}:</span>
                <span class="text-text-secondary">{{ hint }}</span>
              </div>
            </div>
          </div>

          <!-- Related Abilities -->
          <div v-if="relatedAbilities.length" class="flex flex-col gap-1.5">
            <div class="text-[0.65rem] uppercase tracking-widest text-text-dim border-b border-surface-card pb-0.5">Related Abilities ({{ relatedAbilities.length }})</div>
            <ul class="m-0 p-0 list-none max-h-75 overflow-y-auto border border-surface-dark">
              <li
                v-for="ability in relatedAbilities"
                :key="ability.id"
                class="text-xs text-text-secondary px-2 py-0.5 flex gap-2 border-b border-surface-inset hover:bg-surface-base">
                <span class="text-text-muted text-[0.72rem] min-w-14 shrink-0">[Lv {{ ability.level || 0 }}]</span>
                <span class="text-[#7ec8e3] flex-1">{{ ability.name }}</span>
              </li>
            </ul>
          </div>

          <!-- Related Recipes -->
          <div v-if="relatedRecipes.length" class="flex flex-col gap-1.5">
            <div class="text-[0.65rem] uppercase tracking-widest text-text-dim border-b border-surface-card pb-0.5">Recipes ({{ relatedRecipes.length }})</div>
            <ul class="m-0 p-0 list-none max-h-75 overflow-y-auto border border-surface-dark">
              <li
                v-for="recipe in relatedRecipes"
                :key="recipe.id"
                class="text-xs px-2 py-0.5 flex gap-2 items-center border-b border-surface-inset hover:bg-surface-base">
                <span class="text-text-muted text-[0.72rem] min-w-14 shrink-0">[Lv {{ recipe.skill_level_req || 0 }}]</span>
                <RecipeInline :reference="recipe.name" />
              </li>
            </ul>
          </div>

          <!-- NPCs Training This Skill -->
          <div v-if="npcsTraining.length" class="flex flex-col gap-1.5">
            <div class="text-[0.65rem] uppercase tracking-widest text-text-dim border-b border-surface-card pb-0.5">Trained By ({{ npcsTraining.length }})</div>
            <div class="flex flex-wrap gap-1.5">
              <NpcInline v-for="npc in npcsTraining" :key="npc.key" :reference="npc.key" />
            </div>
          </div>

          <!-- Work Order Quests -->
          <div v-if="workOrderQuests.length" class="flex flex-col gap-1.5">
            <div class="text-[0.65rem] uppercase tracking-widest text-text-dim border-b border-surface-card pb-0.5">Work Order Quests ({{ workOrderQuests.length }})</div>
            <ul class="m-0 p-0 list-none max-h-50 overflow-y-auto border border-surface-dark">
              <li
                v-for="quest in workOrderQuests"
                :key="quest.internal_name"
                class="text-xs px-2 py-0.5 flex gap-2 items-center border-b border-surface-inset hover:bg-surface-base">
                <QuestInline :reference="quest.internal_name" />
              </li>
            </ul>
          </div>

          <!-- Keywords -->
          <div v-if="selected.keywords.length" class="flex flex-col gap-1.5">
            <div class="text-[0.65rem] uppercase tracking-widest text-text-dim border-b border-surface-card pb-0.5">Keywords</div>
            <div class="flex flex-wrap gap-1">
              <span
                v-for="kw in selected.keywords"
                :key="kw"
                class="text-[0.72rem] px-1.5 py-0.5 bg-surface-card border border-border-subtle text-[#7ec8e3]"
                :class="{ 'bg-[#1e1a10]! border-[#3a3010]! text-[#887040]!': kw.startsWith('Lint_') }"
                >{{ kw }}</span
              >
            </div>
          </div>

          <!-- Raw JSON -->
          <div v-if="settingsStore.settings.showRawJsonInDataBrowser" class="flex flex-col gap-1.5">
            <div class="text-[0.65rem] uppercase tracking-widest text-text-dim border-b border-surface-card pb-0.5">Raw JSON</div>
            <pre class="bg-surface-dark border border-surface-card p-3 text-[0.72rem] text-text-muted overflow-x-auto whitespace-pre m-0 leading-relaxed">{{ JSON.stringify(selected, null, 2) }}</pre>
          </div>
        </template>
    </div>
  </PaneLayout>
</template>

<script setup lang="ts">
import PaneLayout from "../Shared/PaneLayout.vue";
import { ref, computed, onMounted, watch } from "vue";
import { convertFileSrc } from "@tauri-apps/api/core";
import { useKeyboard } from "../../composables/useKeyboard";
import { useDataBrowserSearch } from "../../composables/useDataBrowserSearch";
import { combineFields } from "../../utils/SearchParser";
import { useGameDataStore } from "../../stores/gameDataStore";
import { useSettingsStore } from "../../stores/settingsStore";
import { useDataBrowserStore } from "../../stores/dataBrowserStore";
import type { EntityNavigationTarget } from "../../composables/useEntityNavigation";
import type { SkillInfo, AbilityInfo, RecipeInfo, NpcInfo, QuestInfo } from "../../types/gameData";
import RecipeInline from "../Shared/Recipe/RecipeInline.vue";
import NpcInline from "../Shared/NPC/NpcInline.vue";
import QuestInline from "../Shared/Quest/QuestInline.vue";

const props = defineProps<{
  navTarget?: EntityNavigationTarget | null;
}>();

const store = useGameDataStore();
const settingsStore = useSettingsStore();
const dataBrowserStore = useDataBrowserStore();

const isFav = computed(() =>
  selected.value ? dataBrowserStore.isFavorite("skill", selected.value.name) : false
);

const allSkills = ref<SkillInfo[]>([]);
const selected = ref<SkillInfo | null>(null);
const selectedIndex = ref(0);
const listRef = ref<HTMLElement | null>(null);
const relatedAbilities = ref<AbilityInfo[]>([]);
const relatedRecipes = ref<RecipeInfo[]>([]);
const npcsTraining = ref<NpcInfo[]>([]);
const workOrderQuests = ref<QuestInfo[]>([]);
const iconSrc = ref<string | null>(null);
const iconLoading = ref(false);
const loading = ref(false);

const { query, filtered: filteredSkills } = useDataBrowserSearch(allSkills, {
  searchText: (skill) => combineFields(skill.name, skill.description),
});

onMounted(async () => {
  if (store.status === "ready") {
    await loadAllSkills();
  }
});

watch(() => store.status, async (newStatus) => {
  if (newStatus === "ready") {
    await loadAllSkills();
  }
});

async function loadAllSkills() {
  loading.value = true;
  try {
    const skills = await store.getAllSkills();
    allSkills.value = skills.sort((a, b) => a.name.localeCompare(b.name));
  } finally {
    loading.value = false;
  }
}

watch(query, () => {
  selectedIndex.value = 0;
});

useKeyboard({
  listNavigation: {
    items: filteredSkills,
    selectedIndex,
    onConfirm: (idx) => {
      const skill = filteredSkills.value[idx];
      if (skill) selectSkill(skill);
    },
    scrollContainerRef: listRef,
  },
});

async function selectSkill(skill: SkillInfo) {
  selected.value = skill;
  iconSrc.value = null;
  dataBrowserStore.addToHistory({ type: "skill", reference: skill.name, label: skill.name });
  relatedAbilities.value = [];
  relatedRecipes.value = [];
  npcsTraining.value = [];
  workOrderQuests.value = [];

  // Load icon if present
  if (skill.icon_id) {
    iconLoading.value = true;
    try {
      const path = await store.getIconPath(skill.icon_id);
      iconSrc.value = convertFileSrc(path);
    } catch (e) {
      console.warn("Icon fetch failed:", e);
    } finally {
      iconLoading.value = false;
    }
  }

  // Load related abilities, recipes, NPCs, and quests in parallel
  Promise.all([
    store.getAbilitiesForSkill(skill.name),
    store.getRecipesForSkill(skill.name),
    store.getNpcsTrainingSkill(skill.name),
    store.getQuestsForSkill(skill.name),
  ]).then(([abilities, recipes, npcs, quests]) => {
    relatedAbilities.value = abilities.sort((a, b) => (a.level || 0) - (b.level || 0));
    relatedRecipes.value = recipes.sort((a, b) => (a.skill_level_req || 0) - (b.skill_level_req || 0));
    npcsTraining.value = npcs;
    workOrderQuests.value = quests;
  }).catch(e => { console.warn("Failed to load related data:", e); });
}

function clearSelection() {
  selected.value = null;
  iconSrc.value = null;
  relatedAbilities.value = [];
  relatedRecipes.value = [];
  npcsTraining.value = [];
  workOrderQuests.value = [];
}

// Navigate to a specific skill when navTarget changes
watch(() => props.navTarget, async (target) => {
  if (!target || target.type !== 'skill') return;
  const name = String(target.id);
  if (selected.value?.name === name) return;

  const skill = await store.resolveSkill(name);
  if (skill) {
    query.value = skill.name;
    selectSkill(skill);
  }
}, { immediate: true });
</script>
