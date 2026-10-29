<template>
  <div class="flex flex-col gap-4 h-[calc(100vh-200px)]">
    <!-- Header -->
    <div class="flex items-center justify-between">
      <h3 class="text-text-primary text-sm font-semibold m-0">Work Orders</h3>
      <div class="flex items-center gap-3">
        <label class="flex items-center gap-2 text-text-dim text-xs cursor-pointer">
          <input
            v-model="includeInventoryScrolls"
            type="checkbox"
            class="accent-accent-gold" />
          Include inventory scrolls
        </label>
        <button
          class="text-text-muted text-xs cursor-pointer bg-transparent border border-border-light rounded px-2.5 py-1 hover:text-text-primary"
          :disabled="loading"
          @click="loadWorkOrders">
          {{ loading ? 'Loading...' : 'Refresh' }}
        </button>
      </div>
    </div>

    <!-- Skill filter -->
    <div v-if="availableSkills.length > 0" class="flex items-center gap-2 flex-wrap">
      <button
        :class="[
          'text-[0.65rem] px-2 py-0.5 rounded border cursor-pointer transition-colors',
          !selectedSkill
            ? 'bg-accent-gold/20 border-accent-gold/40 text-accent-gold'
            : 'bg-transparent border-border-light text-text-muted hover:text-text-primary',
        ]"
        @click="selectedSkill = null">
        All ({{ workOrders.length }})
      </button>
      <button
        v-for="skill in availableSkills"
        :key="skill.name"
        :class="[
          'text-[0.65rem] px-2 py-0.5 rounded border cursor-pointer transition-colors',
          selectedSkill === skill.name
            ? 'bg-accent-gold/20 border-accent-gold/40 text-accent-gold'
            : 'bg-transparent border-border-light text-text-muted hover:text-text-primary',
        ]"
        @click="selectedSkill = skill.name">
        {{ skill.name }} ({{ skill.count }})
      </button>
    </div>

    <!-- Content -->
    <div class="flex-1 overflow-y-auto">
      <div v-if="loading" class="px-2 py-4">
        <SkeletonLoader variant="text" :lines="6" />
      </div>

      <EmptyState v-else-if="workOrders.length === 0" variant="panel" primary="No work orders found" secondary="Import a Character report first." />

      <div v-else class="flex flex-col gap-1.5">
        <!-- Select all for project creation -->
        <div v-if="filteredOrders.length > 0" class="flex items-center justify-between mb-1">
          <label class="flex items-center gap-2 text-text-dim text-xs cursor-pointer">
            <input
              type="checkbox"
              :checked="allSelected"
              :indeterminate="someSelected && !allSelected"
              class="accent-accent-gold"
              @change="toggleSelectAll" />
            Select all ({{ selectedCount }}/{{ filteredOrders.length }})
          </label>
          <button
            v-if="selectedCount > 0"
            class="text-accent-gold text-xs cursor-pointer bg-transparent border border-accent-gold/30 rounded px-2.5 py-1 hover:bg-accent-gold/10 transition-colors"
            :disabled="creatingProject"
            @click="createProjectFromSelected">
            {{ creatingProject ? 'Creating...' : `Create Project (${selectedCount})` }}
          </button>
        </div>

        <!-- Work order rows -->
        <div
          v-for="wo in filteredOrders"
          :key="wo.quest_key"
          class="flex items-center gap-3 px-3 py-2 bg-surface-base border border-surface-elevated rounded text-xs group"
          :class="{
            'border-l-2 border-l-green-500/50': wo.is_active,
            'border-l-2 border-l-blue-500/50': wo.is_in_inventory && !wo.is_active,
          }">
          <!-- Checkbox -->
          <input
            v-model="selectedKeys"
            type="checkbox"
            :value="wo.quest_key"
            class="accent-accent-gold shrink-0"
            :disabled="!wo.recipe_id" />

          <!-- Status badge -->
          <span
            v-if="wo.is_active"
            class="text-green-400 text-[0.6rem] font-semibold shrink-0 w-14">
            ACTIVE
          </span>
          <span
            v-else-if="wo.is_in_inventory"
            class="text-blue-400 text-[0.6rem] font-semibold shrink-0 w-14">
            SCROLL
          </span>
          <span
            v-else-if="wo.is_completed"
            class="text-text-muted text-[0.6rem] shrink-0 w-14">
            done
          </span>
          <span v-else class="w-14 shrink-0" />

          <!-- Item & quantity -->
          <div class="flex items-center gap-1.5 min-w-0 flex-1">
            <ItemInline v-if="wo.item_name" :reference="wo.item_name" />
            <span v-else class="text-text-dim">{{ wo.name }}</span>
            <span class="text-text-primary font-mono shrink-0">×{{ wo.quantity }}</span>
          </div>

          <!-- Craft skill -->
          <SkillInline
            v-if="wo.craft_skill"
            :reference="wo.craft_skill"
            :show-icon="true"
            class="shrink-0 text-[0.65rem]" />

          <!-- Rewards -->
          <div class="flex items-center gap-2 shrink-0 ml-auto">
            <span v-if="wo.industry_xp > 0" class="text-accent-gold text-[0.65rem]">
              {{ wo.industry_xp.toLocaleString() }} XP
            </span>
            <span v-if="wo.gold_reward > 0" class="text-yellow-400 text-[0.65rem]">
              {{ wo.gold_reward.toLocaleString() }}g
            </span>
          </div>

          <!-- Recipe link -->
          <div class="shrink-0 w-5">
            <span
              v-if="!wo.recipe_id"
              class="text-text-muted/50 text-[0.6rem]"
              title="No matching recipe found">
              ?
            </span>
          </div>
        </div>

        <!-- Industry totals -->
        <div v-if="filteredOrders.length > 0" class="flex items-center gap-4 mt-2 px-3 py-2 bg-surface-elevated rounded text-xs border border-border-light">
          <span class="text-text-dim">
            {{ filteredOrders.length }} work order{{ filteredOrders.length !== 1 ? 's' : '' }}
          </span>
          <span v-if="totalIndustryXp > 0" class="text-accent-gold">
            {{ totalIndustryXp.toLocaleString() }} Industry XP
          </span>
          <span v-if="totalGold > 0" class="text-yellow-400">
            {{ totalGold.toLocaleString() }}g
          </span>
        </div>
      </div>
    </div>

    <div v-if="error" class="text-accent-red text-xs">{{ error }}</div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, onMounted, watch } from "vue";
import { useCraftingStore } from "../../stores/craftingStore";
import type { EnrichedWorkOrder } from "../../types/crafting";
import EmptyState from "../Shared/EmptyState.vue";
import SkeletonLoader from "../Shared/SkeletonLoader.vue";
import ItemInline from "../Shared/Item/ItemInline.vue";
import SkillInline from "../Shared/Skill/SkillInline.vue";

const craftingStore = useCraftingStore();

const workOrders = ref<EnrichedWorkOrder[]>([]);
const loading = ref(false);
const error = ref("");
const includeInventoryScrolls = ref(false);
const selectedSkill = ref<string | null>(null);
const selectedKeys = ref<string[]>([]);
const creatingProject = ref(false);

const availableSkills = computed(() => {
  const counts = new Map<string, number>();
  for (const wo of workOrders.value) {
    const skill = wo.craft_skill ?? "Unknown";
    counts.set(skill, (counts.get(skill) ?? 0) + 1);
  }
  return [...counts.entries()]
    .map(([name, count]) => ({ name, count }))
    .sort((a, b) => a.name.localeCompare(b.name));
});

const filteredOrders = computed(() => {
  if (!selectedSkill.value) return workOrders.value;
  return workOrders.value.filter((wo) => (wo.craft_skill ?? "Unknown") === selectedSkill.value);
});

const selectedCount = computed(() => selectedKeys.value.length);
const allSelected = computed(() =>
  filteredOrders.value.length > 0
  && filteredOrders.value.filter((wo) => wo.recipe_id).every((wo) => selectedKeys.value.includes(wo.quest_key)),
);
const someSelected = computed(() => selectedKeys.value.length > 0);

const totalIndustryXp = computed(() =>
  filteredOrders.value.reduce((sum, wo) => sum + wo.industry_xp, 0),
);
const totalGold = computed(() =>
  filteredOrders.value.reduce((sum, wo) => sum + wo.gold_reward, 0),
);

function toggleSelectAll() {
  if (allSelected.value) {
    selectedKeys.value = [];
  } else {
    selectedKeys.value = filteredOrders.value
      .filter((wo) => wo.recipe_id)
      .map((wo) => wo.quest_key);
  }
}

async function loadWorkOrders() {
  loading.value = true;
  error.value = "";

  try {
    workOrders.value = await craftingStore.getWorkOrders(includeInventoryScrolls.value);
    selectedKeys.value = [];
    selectedSkill.value = null;
  } catch (e) {
    error.value = String(e);
    console.error("[crafting] Failed to load work orders:", e);
  } finally {
    loading.value = false;
  }
}

async function createProjectFromSelected() {
  if (selectedKeys.value.length === 0) return;
  creatingProject.value = true;
  error.value = "";

  try {
    const selectedOrders = workOrders.value.filter(
      (wo) => selectedKeys.value.includes(wo.quest_key) && wo.recipe_id,
    );

    if (selectedOrders.length === 0) return;

    const skillName = selectedSkill.value ?? "Mixed";
    const projectName = `Work Orders — ${skillName}`;
    const notes = `Auto-generated from ${selectedOrders.length} work order${selectedOrders.length !== 1 ? "s" : ""}`;

    const projectId = await craftingStore.createProject(projectName, notes);

    for (const wo of selectedOrders) {
      if (wo.recipe_id && wo.recipe_name) {
        await craftingStore.addEntry(projectId, wo.recipe_id, wo.recipe_name, wo.quantity);
      }
    }

    selectedKeys.value = [];
  } catch (e) {
    error.value = String(e);
    console.error("[crafting] Failed to create project from work orders:", e);
  } finally {
    creatingProject.value = false;
  }
}

watch(includeInventoryScrolls, () => {
  loadWorkOrders();
});

onMounted(() => {
  loadWorkOrders();
});
</script>
