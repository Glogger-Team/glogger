<template>
  <div class="flex flex-col gap-4 px-4 py-3">
    <EmptyState v-if="!activeProject" variant="panel" primary="No project selected" secondary="Select or create a project to see recipes." />

    <div v-else class="flex flex-col gap-4">
      <!-- Project header -->
      <div class="flex items-center justify-between">
        <div class="flex items-center gap-3">
          <h3 class="text-accent-gold text-base font-semibold m-0">{{ activeProject.name }}</h3>
          <button
            class="text-text-muted text-xs cursor-pointer bg-transparent border-none hover:text-text-primary"
            @click="editProjectName">
            edit
          </button>
        </div>
        <div class="flex items-center gap-2">
          <span v-if="exportMessage" class="text-xs" :class="exportFailed ? 'text-red-400' : 'text-accent-gold'">{{ exportMessage }}</span>
          <button
            class="text-text-muted text-xs cursor-pointer bg-transparent border border-border-light rounded px-2 py-0.5 hover:text-text-primary hover:border-border-default"
            title="Copy a shareable project code to the clipboard"
            @click="exportProject">
            Export
          </button>
          <button
            class="text-text-muted text-xs cursor-pointer bg-transparent border border-border-light rounded px-2 py-0.5 hover:text-text-primary hover:border-border-default"
            @click="$emit('duplicate')">
            Duplicate
          </button>
          <button
            class="text-accent-red/70 text-xs cursor-pointer bg-transparent border border-accent-red/20 rounded px-2 py-0.5 hover:text-accent-red hover:border-accent-red/40"
            @click="$emit('delete')">
            Delete
          </button>
        </div>
      </div>

      <!-- Group assignment -->
      <div class="flex items-center gap-2 text-xs">
        <span class="text-text-dim shrink-0">Group:</span>
        <div class="relative flex-1">
          <input
            v-model="groupInput"
            class="input w-full text-xs"
            placeholder="None"
            @input="onGroupInput"
            @blur="commitGroup"
            @keyup.enter="commitGroup"
            list="group-suggestions" />
          <datalist id="group-suggestions">
            <option v-for="name in existingGroupNames" :key="name" :value="name" />
          </datalist>
        </div>
        <button
          v-if="groupInput"
          class="text-text-muted text-[0.65rem] cursor-pointer bg-transparent border-none hover:text-accent-red"
          title="Remove from group"
          @click="clearGroup">
          &#10005;
        </button>
      </div>

      <!-- Pricing mode toggle -->
      <button
        class="text-xs cursor-pointer border rounded px-2 py-1 transition-colors self-start"
        :class="pricingMode
          ? 'text-accent-gold border-accent-gold/40 bg-accent-gold/10 hover:bg-transparent hover:text-text-muted'
          : 'text-text-muted bg-transparent border-border-light hover:text-accent-gold hover:border-accent-gold/30'"
        @click="$emit('toggle-pricing')">
        {{ pricingMode ? 'Pricing On' : 'Enable Pricing' }}
      </button>

      <!-- Add recipe search -->
      <div class="flex gap-2">
        <input
          v-model="addRecipeQuery"
          class="input flex-1 text-xs"
          placeholder="Search recipes to add..."
          @input="debouncedRecipeSearch" />
        <input
          v-model.number="addRecipeQty"
          type="number"
          min="1"
          class="input w-16 text-xs text-center"
          placeholder="Qty" />
      </div>

      <!-- Recipe search results -->
      <ul
        v-if="addRecipeResults.length > 0"
        class="list-none m-0 p-0 border border-surface-elevated rounded max-h-40 overflow-y-auto -mt-2">
        <li
          v-for="recipe in addRecipeResults"
          :key="recipe.id"
          class="flex items-baseline gap-2 px-3 py-1.5 cursor-pointer border-b border-surface-dark text-xs hover:bg-surface-row-hover"
          @click="addRecipeToProject(recipe)">
          <span class="text-text-muted text-[0.72rem] min-w-12 shrink-0">
            [{{ recipe.skill ?? '?' }} {{ recipe.skill_level_req ?? 0 }}]
          </span>
          <span class="text-text-primary/75">{{ recipe.name }}</span>
        </li>
      </ul>

      <!-- Empty state -->
      <div v-if="activeProject.entries.length === 0" class="text-text-dim text-xs italic py-4">
        No recipes added yet. Search above to add recipes to this project.
      </div>

      <!-- Recipe entries -->
      <AccordionSection v-if="activeProject.entries.length > 0" :default-open="activeProject.entries.length <= 5">
        <template #title>Recipe List</template>
        <template #badge>
          <div class="flex items-center gap-2">
            <span class="text-text-muted text-[0.65rem]">{{ activeProject.entries.length }} recipe{{ activeProject.entries.length !== 1 ? 's' : '' }}</span>
            <button
              class="text-text-muted text-[0.65rem] cursor-pointer bg-transparent border-none hover:text-text-primary"
              @click.stop="expandAll">
              Expand All
            </button>
            <span class="text-text-muted/40 text-[0.65rem]">|</span>
            <button
              class="text-text-muted text-[0.65rem] cursor-pointer bg-transparent border-none hover:text-text-primary"
              @click.stop="collapseAll">
              Collapse All
            </button>
          </div>
        </template>
        <div class="flex flex-col gap-1">
          <ProjectEntryCard
            v-for="entry in activeProject.entries"
            :key="entry.id"
            ref="entryCards"
            :entry="entry"
            :intermediate-expansions="intermediateExpansions"
            :stock-target="stockTargets.get(entry.id)"
            @update-qty="(entryId, qty) => $emit('update-qty', entryId, qty)"
            @remove="(entryId) => $emit('remove', entryId)"
            @toggle-intermediate="(entryId, itemId) => $emit('toggle-intermediate', entryId, itemId)"
            @update-target-stock="(entryId, ts) => $emit('update-target-stock', entryId, ts)" />
        </div>
      </AccordionSection>

      <!-- Fee Configuration (pricing mode only) -->
      <div v-if="pricingMode" class="flex flex-col gap-2 border-t border-surface-elevated pt-3">
        <div class="flex items-center justify-between">
          <h4 class="text-text-secondary text-xs font-semibold uppercase tracking-wide m-0">Crafting Fee</h4>
          <div class="flex gap-1">
            <button
              class="text-text-muted text-[0.65rem] cursor-pointer bg-transparent border-none hover:text-accent-gold"
              title="Save current fee config as default for new projects"
              @click="$emit('save-defaults')">
              Save as Default
            </button>
            <span class="text-text-muted/40 text-[0.65rem]">|</span>
            <button
              class="text-text-muted text-[0.65rem] cursor-pointer bg-transparent border-none hover:text-text-primary"
              title="Reset fee config to defaults"
              @click="$emit('reset-defaults')">
              Reset
            </button>
          </div>
        </div>

        <!-- Per-craft fee -->
        <div class="flex items-center gap-2">
          <label class="text-text-muted text-[0.65rem] w-24 shrink-0">Per craft</label>
          <input
            :value="feeConfig.per_craft_fee"
            type="number"
            min="0"
            class="input flex-1 text-xs"
            @change="onFeeFieldChange('per_craft_fee', $event)" />
          <span class="text-text-dim text-[0.65rem]">gold each</span>
        </div>

        <!-- Material percentage -->
        <div class="flex items-center gap-2">
          <label class="text-text-muted text-[0.65rem] w-24 shrink-0">Material %</label>
          <input
            :value="feeConfig.material_pct"
            type="number"
            min="0"
            max="100"
            class="input w-16 text-xs"
            @change="onFeeFieldChange('material_pct', $event)" />
          <span class="text-text-dim text-[0.65rem]">% of</span>
          <select
            :value="feeConfig.material_pct_basis"
            class="px-1.5 py-0.5 bg-surface-base border border-border-default rounded text-[0.65rem] text-text-muted cursor-pointer"
            @change="onBasisChange($event)">
            <option value="total">total materials</option>
            <option value="yours">your materials</option>
            <option value="theirs">their materials</option>
          </select>
        </div>

        <!-- Flat fee -->
        <div class="flex items-center gap-2">
          <label class="text-text-muted text-[0.65rem] w-24 shrink-0">Flat fee</label>
          <input
            :value="feeConfig.flat_fee"
            type="number"
            min="0"
            class="input flex-1 text-xs"
            @change="onFeeFieldChange('flat_fee', $event)" />
          <span class="text-text-dim text-[0.65rem]">gold total</span>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, watch } from "vue";
import { useGameDataStore } from "../../stores/gameDataStore";
import { useCraftingStore } from "../../stores/craftingStore";
import type { CraftingProject, FeeConfig, MaterialPctBasis } from "../../types/crafting";
import type { RecipeInfo } from "../../types/gameData/recipes";
import EmptyState from "../Shared/EmptyState.vue";
import AccordionSection from "../Shared/AccordionSection.vue";
import ProjectEntryCard from "./ProjectEntryCard.vue";

const props = defineProps<{
  activeProject: CraftingProject | null
  intermediateExpansions: Map<string, boolean>
  stockTargets: Map<number, { effectiveQty: number; currentStock: number }>
  pricingMode: boolean
  feeConfig: FeeConfig
}>();

const emit = defineEmits<{
  'duplicate': []
  'delete': []
  'update-qty': [entryId: number, qty: number]
  'remove': [entryId: number]
  'toggle-intermediate': [entryId: number, itemId: number | null]
  'update-target-stock': [entryId: number, targetStock: number | null]
  'toggle-pricing': []
  'update-fee': [feeConfig: FeeConfig]
  'save-defaults': []
  'reset-defaults': []
}>();

const gameData = useGameDataStore();
const store = useCraftingStore();

const entryCards = ref<InstanceType<typeof ProjectEntryCard>[]>([]);

const addRecipeQuery = ref("");
const addRecipeQty = ref(1);
const addRecipeResults = ref<RecipeInfo[]>([]);
let searchTimeout: ReturnType<typeof setTimeout> | null = null;

const groupInput = ref("");

watch(() => props.activeProject, (p) => {
  groupInput.value = p?.group_name ?? "";
}, { immediate: true });

const existingGroupNames = computed(() => {
  const names = new Set<string>();
  for (const p of store.projects) {
    if (p.group_name) names.add(p.group_name);
  }
  return Array.from(names).sort();
});

function onGroupInput() {}

function commitGroup() {
  if (!props.activeProject) return;
  const newGroup = groupInput.value.trim() || null;
  if (newGroup !== (props.activeProject.group_name ?? null)) {
    store.updateProject(props.activeProject.id, props.activeProject.name, props.activeProject.notes, newGroup);
  }
}

function clearGroup() {
  groupInput.value = "";
  commitGroup();
}

function editProjectName() {
  if (!props.activeProject) return;
  const name = prompt("Project name:", props.activeProject.name);
  if (name && name.trim()) {
    store.updateProject(props.activeProject.id, name.trim(), props.activeProject.notes, props.activeProject.group_name);
  }
}

const exportMessage = ref("");
const exportFailed = ref(false);
let exportMessageTimeout: ReturnType<typeof setTimeout> | null = null;

async function exportProject() {
  if (!props.activeProject) return;
  try {
    const code = await store.exportProject(props.activeProject.id);
    await navigator.clipboard.writeText(code);
    exportFailed.value = false;
    exportMessage.value = "Copied to clipboard!";
  } catch (e) {
    exportFailed.value = true;
    exportMessage.value = `Export failed: ${e}`;
  }
  if (exportMessageTimeout) clearTimeout(exportMessageTimeout);
  exportMessageTimeout = setTimeout(() => { exportMessage.value = ""; }, 3000);
}

function debouncedRecipeSearch() {
  if (searchTimeout) clearTimeout(searchTimeout);
  searchTimeout = setTimeout(async () => {
    if (!addRecipeQuery.value.trim()) {
      addRecipeResults.value = [];
      return;
    }
    addRecipeResults.value = await gameData.searchRecipes(addRecipeQuery.value, 20);
  }, 250);
}

async function addRecipeToProject(recipe: RecipeInfo) {
  if (!props.activeProject) return;
  await store.addEntry(
    props.activeProject.id,
    recipe.id,
    recipe.name,
    Math.max(1, addRecipeQty.value),
  );
  addRecipeQuery.value = "";
  addRecipeResults.value = [];
  addRecipeQty.value = 1;
}

function expandAll() {
  for (const card of entryCards.value) {
    card.expanded = true;
  }
}

function collapseAll() {
  for (const card of entryCards.value) {
    card.expanded = false;
  }
}

function onFeeFieldChange(field: keyof FeeConfig, event: Event) {
  const target = event.target as HTMLInputElement;
  const value = Math.max(0, parseFloat(target.value) || 0);
  emit('update-fee', { ...props.feeConfig, [field]: value });
}

function onBasisChange(event: Event) {
  const target = event.target as HTMLSelectElement;
  emit('update-fee', {
    ...props.feeConfig,
    material_pct_basis: target.value as MaterialPctBasis,
  });
}
</script>
