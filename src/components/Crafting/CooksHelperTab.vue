<template>
  <div class="flex flex-col gap-4 h-[calc(100vh-200px)]">
    <!-- Import bar -->
    <div class="flex items-center justify-between bg-surface-card border border-border-default rounded px-4 py-2">
      <div class="flex items-center gap-4">
        <h3 class="display-title">Cook's Helper</h3>
        <button
          class="px-3 py-1.5 text-sm bg-surface-elevated border border-border-default rounded text-text-secondary hover:text-text-primary hover:border-border-hover transition-all"
          :disabled="store.loading"
          @click="store.importFile()">
          {{ store.isImported ? 'Re-import' : 'Import Skill Report' }}
        </button>
      </div>

      <div v-if="store.isImported" class="flex items-center gap-4 text-xs">
        <span class="text-text-muted">
          {{ store.stats.eaten }} eaten
        </span>
        <span class="text-text-secondary">
          {{ store.stats.uneaten }} uneaten
        </span>
        <span class="text-accent-gold font-semibold">
          {{ store.stats.craftable }} you can cook
        </span>
      </div>
    </div>

    <!-- Error -->
    <div v-if="store.error" class="bg-accent-red/10 border border-accent-red/30 rounded p-3 text-accent-red text-sm">
      {{ store.error }}
    </div>

    <!-- Not imported state -->
    <div v-if="!store.isImported && !store.loading" class="flex flex-col items-center gap-4 py-8">
      <EmptyState
        variant="panel"
        primary="No skill report imported"
        secondary="Import a player's gourmand skill report, or start fresh to see all cookable foods." />
      <button
        class="btn-secondary text-xs py-1.5 px-4"
        @click="store.startFresh()">
        Start Fresh (all foods uneaten)
      </button>
    </div>

    <!-- Loading -->
    <div v-if="store.loading" class="flex-1 px-4 py-4">
      <DataTableSkeleton :columns="4" :rows="8" />
    </div>

    <!-- Main content -->
    <template v-if="store.isImported && !store.loading">
      <!-- Filters -->
      <div class="flex items-center gap-3 flex-wrap">
        <!-- Skill filter pills -->
        <button
          v-for="option in skillOptions"
          :key="option.value"
          :class="[
            'text-[0.65rem] px-2 py-0.5 rounded border cursor-pointer transition-colors',
            store.filterSkill === option.value
              ? 'bg-accent-gold/20 border-accent-gold/40 text-accent-gold'
              : 'bg-transparent border-border-light text-text-muted hover:text-text-primary',
          ]"
          @click="store.filterSkill = option.value">
          {{ option.label }} ({{ option.count }})
        </button>

        <div class="w-px h-4 bg-border-light" />

        <!-- Availability filter -->
        <select
          v-model="store.filterAvailability"
          class="text-xs bg-surface-elevated border border-border-default rounded px-2 py-1 text-text-secondary">
          <option value="all">All</option>
          <option value="can-craft">Can Craft</option>
          <option value="missing-materials">Missing Materials</option>
        </select>

        <!-- Sort -->
        <select
          v-model="store.sortMode"
          class="text-xs bg-surface-elevated border border-border-default rounded px-2 py-1 text-text-secondary">
          <option value="name">Sort: Name</option>
          <option value="skill-level">Sort: Skill Level</option>
          <option value="food-level">Sort: Food Level</option>
        </select>

        <!-- Search -->
        <input
          v-model="store.searchQuery"
          type="text"
          placeholder="Search..."
          class="text-xs bg-surface-elevated border border-border-default rounded px-2 py-1 text-text-primary w-40" />

        <div class="w-px h-4 bg-border-light" />

        <!-- Check materials -->
        <button
          class="text-xs bg-surface-elevated border border-border-default rounded px-2.5 py-1 text-text-secondary hover:text-text-primary hover:border-border-hover transition-all"
          :disabled="store.checkingMaterials"
          @click="store.checkAllMaterials()">
          {{ store.checkingMaterials ? 'Checking...' : 'Check Materials' }}
        </button>
      </div>

      <!-- Select all + action bar -->
      <div v-if="store.filteredRecipes.length > 0" class="flex items-center justify-between">
        <label class="flex items-center gap-2 text-text-dim text-xs cursor-pointer">
          <input
            type="checkbox"
            :checked="allSelected"
            :indeterminate="someSelected && !allSelected"
            class="accent-accent-gold"
            @change="toggleSelectAll" />
          Select all ({{ store.selectionCount }}/{{ store.filteredRecipes.length }})
        </label>

        <div v-if="store.selectionCount > 0" class="flex items-center gap-2">
          <!-- Add to existing project -->
          <select
            v-if="craftingStore.projects.length > 0"
            v-model="targetProjectId"
            class="text-xs bg-surface-elevated border border-border-default rounded px-2 py-1 text-text-secondary">
            <option :value="null" disabled>Add to project...</option>
            <option
              v-for="project in craftingStore.projects"
              :key="project.id"
              :value="project.id">
              {{ project.name }}
            </option>
          </select>
          <button
            v-if="targetProjectId !== null"
            class="text-accent-gold text-xs cursor-pointer bg-transparent border border-accent-gold/30 rounded px-2.5 py-1 hover:bg-accent-gold/10 transition-colors"
            @click="addToExisting">
            Add ({{ store.selectionCount }})
          </button>

          <div class="w-px h-4 bg-border-light" />

          <!-- Create new project -->
          <button
            class="text-accent-gold text-xs cursor-pointer bg-transparent border border-accent-gold/30 rounded px-2.5 py-1 hover:bg-accent-gold/10 transition-colors"
            @click="createNew">
            New Project ({{ store.selectionCount }})
          </button>
        </div>
      </div>

      <!-- Recipe list -->
      <div class="flex-1 overflow-y-auto">
        <div v-if="store.filteredRecipes.length === 0" class="flex items-center justify-center h-full text-border-default italic text-xs">
          {{ store.helpfulRecipes.length === 0 ? 'No cookable uneaten foods found.' : 'No recipes match current filters.' }}
        </div>

        <div v-else class="flex flex-col gap-1.5">
          <CooksHelperRecipeRow
            v-for="entry in store.filteredRecipes"
            :key="entry.recipe.id"
            :entry="entry"
            :is-selected="store.selectedRecipeIds.has(entry.recipe.id)"
            :material-needs="store.materialNeedsMap.get(entry.recipe.id)"
            @toggle="store.toggleSelection" />
        </div>
      </div>
    </template>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, onMounted } from 'vue'
import { useCooksHelperStore } from '../../stores/cooksHelperStore'
import { useCraftingStore } from '../../stores/craftingStore'
import EmptyState from '../Shared/EmptyState.vue'
import DataTableSkeleton from '../Shared/DataTableSkeleton.vue'
import CooksHelperRecipeRow from './CooksHelperRecipeRow.vue'

const store = useCooksHelperStore()
const craftingStore = useCraftingStore()

const targetProjectId = ref<number | null>(null)

// Load project list for the dropdown
onMounted(() => {
  void store.tryAutoImport()
  craftingStore.loadProjects()
})

// Skill filter options with counts — derived dynamically from loaded recipes
const skillOptions = computed(() => {
  const recipes = store.helpfulRecipes
  const options: { value: string; label: string; count: number }[] = [
    { value: 'all', label: 'All', count: recipes.length },
  ]
  for (const skill of store.availableSkills) {
    const count = recipes.filter(h => h.recipe.skill === skill).length
    if (count > 0) options.push({ value: skill, label: skill, count })
  }
  return options
})

// Selection helpers
const allSelected = computed(() =>
  store.filteredRecipes.length > 0 &&
  store.filteredRecipes.every(h => store.selectedRecipeIds.has(h.recipe.id))
)
const someSelected = computed(() =>
  store.filteredRecipes.some(h => store.selectedRecipeIds.has(h.recipe.id))
)

function toggleSelectAll() {
  if (allSelected.value) {
    store.deselectAll()
  } else {
    store.selectAll()
  }
}

async function addToExisting() {
  if (targetProjectId.value === null) return
  await store.addToProject(targetProjectId.value)
  targetProjectId.value = null
}

async function createNew() {
  await store.createProjectFromSelection()
}
</script>
