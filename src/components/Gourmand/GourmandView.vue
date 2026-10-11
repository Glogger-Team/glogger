<template>
  <PaneLayout
    screen-key="char-gourmand"
    :left-pane="{ title: 'Progress', defaultWidth: 280, minWidth: 220, maxWidth: 400 }"
    :right-pane="hasSelection ? { title: 'Food Buff', defaultWidth: 300, minWidth: 220, maxWidth: 450 } : undefined"
  >
    <!-- Left Pane: Progress + Favorites -->
    <template #left>
      <div class="p-3 space-y-3 overflow-y-auto h-full">
        <!-- Progress Summary -->
        <div class="space-y-1.5">
          <h3 class="text-accent-gold font-bold uppercase tracking-wide text-xs mb-1.5">Progress</h3>
          <GourmandProgressBar label="Overall" :eaten="store.totalEaten" :total="store.progressFoods.length" />
          <GourmandProgressBar label="Meals" :eaten="store.mealsEaten" :total="store.progressMeals.length" />
          <GourmandProgressBar label="Snacks" :eaten="store.snacksEaten" :total="store.progressSnacks.length" />
          <GourmandProgressBar label="Instant-Snacks" :eaten="store.instantSnacksEaten" :total="store.progressInstantSnacks.length" />
          <label
            class="flex items-center gap-1.5 text-text-secondary text-xs cursor-pointer pt-1"
            :title="`Count progress against only the foods you can still realistically get — excludes ${excludedCount} event and unobtainable foods.`"
          >
            <input type="checkbox" v-model="store.attainableOnly" class="accent-accent-gold" />
            Attainable only
          </label>
        </div>

        <!-- Favorites -->
        <div v-if="store.reportLoaded && hasFavorites">
          <h3 class="text-accent-gold font-bold uppercase tracking-wide text-xs mb-1.5">Favorites</h3>
          <div class="space-y-2">
            <div v-if="store.favoriteMeals.length > 0">
              <div class="text-text-muted text-xs uppercase tracking-wide mb-0.5">Meals</div>
              <div v-for="fav in store.favoriteMeals" :key="fav.name" class="flex justify-between text-xs py-0.5">
                <span class="text-text-primary truncate mr-2">{{ fav.name }}</span>
                <span class="text-accent-gold shrink-0">&times;{{ fav.count }}</span>
              </div>
            </div>
            <div v-if="store.favoriteSnacks.length > 0">
              <div class="text-text-muted text-xs uppercase tracking-wide mb-0.5">Snacks</div>
              <div v-for="fav in store.favoriteSnacks" :key="fav.name" class="flex justify-between text-xs py-0.5">
                <span class="text-text-primary truncate mr-2">{{ fav.name }}</span>
                <span class="text-accent-gold shrink-0">&times;{{ fav.count }}</span>
              </div>
            </div>
            <div v-if="store.favoriteInstantSnacks.length > 0">
              <div class="text-text-muted text-xs uppercase tracking-wide mb-0.5">Instant</div>
              <div v-for="fav in store.favoriteInstantSnacks" :key="fav.name" class="flex justify-between text-xs py-0.5">
                <span class="text-text-primary truncate mr-2">{{ fav.name }}</span>
                <span class="text-accent-gold shrink-0">&times;{{ fav.count }}</span>
              </div>
            </div>
          </div>
        </div>
      </div>
    </template>

    <!-- Right Pane: Food Buff Comparison -->
    <template #right>
      <div class="p-3 overflow-y-auto h-full">
        <FoodComparisonPanel
          :selected-meal="store.selectedMeal"
          :selected-snack="store.selectedSnack"
          @clear="store.clearSelection()"
        />
      </div>
    </template>

    <!-- Center: Header + Controls + Food Lists -->
    <div class="flex flex-col gap-3 h-full min-h-0">
      <!-- Header Bar -->
      <div class="flex items-center justify-between bg-surface-card border border-border-default rounded px-4 py-2 shrink-0">
        <div class="flex items-center gap-4">
          <h2 class="text-accent-gold font-bold text-lg">Gourmand Tracker</h2>
          <button
            class="px-3 py-1.5 text-sm bg-surface-elevated border border-border-default rounded text-text-secondary hover:text-text-primary hover:border-border-hover transition-all"
            @click="store.importReport()"
            :disabled="store.loading"
          >
            {{ store.reportLoaded ? 'Re-import Report' : 'Import Gourmand Report' }}
          </button>
          <span v-if="store.reportLoaded" class="text-accent-green text-xs">
            {{ store.totalEaten }} foods eaten
          </span>
          <span v-if="unmatchedCount > 0" class="text-accent-warning text-xs">
            ({{ unmatchedCount }} unmatched)
          </span>
        </div>

        <div class="flex items-center gap-3">
          <!-- Gourmand Level -->
          <div class="flex items-center gap-2 text-sm">
            <span class="text-text-muted">Gourmand:</span>
            <template v-if="store.gourmandLevel !== null && !editingLevel">
              <span class="text-accent-gold font-bold">Lv{{ store.gourmandLevel }}</span>
              <span class="text-text-dim text-xs">({{ store.gourmandLevelSource }})</span>
              <button
                class="text-text-dim hover:text-text-secondary text-xs"
                @click="startEditLevel"
                title="Override level"
              >
                edit
              </button>
            </template>
            <template v-else>
              <input
                ref="levelInput"
                type="number"
                min="0"
                max="125"
                class="w-16 px-2 py-1 bg-surface-dark border border-border-default rounded text-text-primary text-sm text-center"
                :value="store.manualGourmandLevel ?? store.gourmandLevel ?? ''"
                placeholder="Lv"
                @keydown.enter="applyLevel"
                @blur="applyLevel"
              />
              <button
                v-if="store.manualGourmandLevel !== null"
                class="text-text-dim hover:text-accent-red text-xs"
                @click="store.setManualLevel(null); editingLevel = false"
                title="Clear override"
              >
                clear
              </button>
            </template>
          </div>
        </div>
      </div>

      <!-- Error message -->
      <div v-if="store.error" class="bg-accent-red/10 border border-accent-red/30 rounded p-3 text-accent-red text-sm shrink-0">
        {{ store.error }}
      </div>

      <!-- Loading state -->
      <div v-if="store.loading && allFoods.length === 0" class="text-text-muted text-center py-8">
        Loading food data...
      </div>

      <!-- No food data yet -->
      <EmptyState
        v-if="!store.loading && allFoods.length === 0"
        variant="panel"
        primary="No food data available"
        secondary="Make sure game data has been loaded." />

      <!-- Main content (only when foods loaded) -->
      <template v-if="allFoods.length > 0">
        <!-- No report hint -->
        <div v-if="!store.reportLoaded" class="bg-surface-card border border-border-default rounded p-4 text-text-secondary text-sm shrink-0">
          Import a gourmand report to see which foods you've eaten. In-game, use the Gourmand skill's
          "Request Skill Report" ability, then find the file in your Books folder.
        </div>

        <!-- Controls Bar -->
        <div class="flex items-center gap-4 text-sm shrink-0">
          <label class="flex items-center gap-1.5 text-text-secondary cursor-pointer">
            <input type="checkbox" v-model="store.hideEaten" class="accent-accent-gold" />
            Hide eaten
          </label>

          <label class="flex items-center gap-1.5 text-text-secondary cursor-pointer">
            <input type="checkbox" v-model="store.hideUnusable" class="accent-accent-gold" />
            Hide unusable
          </label>

          <div class="flex items-center gap-1.5">
            <span class="text-text-muted">Source:</span>
            <select
              v-model="store.sourceFilter"
              class="bg-surface-dark border border-border-default rounded px-2 py-1 text-text-primary text-sm"
              title="Filter by where the food comes from"
            >
              <option v-for="opt in sourceFilterOptions" :key="opt.value" :value="opt.value">
                {{ opt.label }}{{ opt.count !== null ? ` (${opt.count})` : '' }}
              </option>
            </select>
          </div>

          <div class="flex items-center gap-1.5">
            <span class="text-text-muted">Sort:</span>
            <select
              v-model="store.sortMode"
              class="bg-surface-dark border border-border-default rounded px-2 py-1 text-text-primary text-sm"
            >
              <option value="level">Gourmand Level</option>
              <option value="food-level">Food Level</option>
              <option value="alpha">Alphabetical</option>
              <option value="source">Source</option>
            </select>
            <button
              class="px-1.5 py-1 text-xs bg-surface-dark border border-border-default rounded text-text-secondary hover:text-text-primary hover:border-border-hover transition-all"
              @click="store.sortAsc = !store.sortAsc"
              :title="store.sortAsc ? 'Ascending' : 'Descending'"
            >
              {{ store.sortAsc ? '&#9650;' : '&#9660;' }}
            </button>
          </div>

          <div class="flex items-center gap-1.5">
            <span class="text-text-muted">View:</span>
            <div class="flex border border-border-default rounded overflow-hidden">
              <button
                class="px-2 py-1 text-xs transition-all"
                :class="store.viewMode === 'card' ? 'bg-surface-elevated text-accent-gold' : 'bg-surface-dark text-text-secondary hover:text-text-primary'"
                @click="store.viewMode = 'card'"
              >
                Cards
              </button>
              <button
                class="px-2 py-1 text-xs transition-all"
                :class="store.viewMode === 'list' ? 'bg-surface-elevated text-accent-gold' : 'bg-surface-dark text-text-secondary hover:text-text-primary'"
                @click="store.viewMode = 'list'"
              >
                List
              </button>
            </div>
          </div>

          <button
            v-if="store.reportLoaded"
            class="ml-auto px-3 py-1 text-xs bg-surface-elevated border border-border-default rounded text-text-secondary hover:text-text-primary hover:border-border-hover transition-all"
            @click="store.exportUneaten()"
          >
            Export Uneaten
          </button>
        </div>

        <!-- Food Lists: scrollable area -->
        <div class="flex-1 min-h-0 overflow-y-auto">
          <div :class="store.viewMode === 'list' ? 'grid grid-cols-3 gap-4' : ''">
            <FoodCategorySection
              title="Meals"
              :foods="store.meals"
              :eaten-foods="store.eatenFoods"
              :manually-marked-foods="store.manuallyMarkedFoods"
              :hide-eaten="store.hideEaten"
              :hide-unusable="store.hideUnusable"
              :source-filter="store.sourceFilter"
              :sort-mode="store.sortMode"
              :sort-asc="store.sortAsc"
              :view-mode="store.viewMode"
              :selectable="true"
              :gourmand-level="store.gourmandLevel"
              :selected-food="store.selectedMeal"
              @select="handleMealSelect"
              @toggle="handleToggle"
            />

            <FoodCategorySection
              title="Snacks"
              :foods="store.snacks"
              :eaten-foods="store.eatenFoods"
              :manually-marked-foods="store.manuallyMarkedFoods"
              :hide-eaten="store.hideEaten"
              :hide-unusable="store.hideUnusable"
              :source-filter="store.sourceFilter"
              :sort-mode="store.sortMode"
              :sort-asc="store.sortAsc"
              :view-mode="store.viewMode"
              :selectable="true"
              :gourmand-level="store.gourmandLevel"
              :selected-food="store.selectedSnack"
              @select="handleSnackSelect"
              @toggle="handleToggle"
            />

            <FoodCategorySection
              title="Instant-Snacks"
              :foods="store.instantSnacks"
              :eaten-foods="store.eatenFoods"
              :manually-marked-foods="store.manuallyMarkedFoods"
              :hide-eaten="store.hideEaten"
              :hide-unusable="store.hideUnusable"
              :source-filter="store.sourceFilter"
              :sort-mode="store.sortMode"
              :sort-asc="store.sortAsc"
              :view-mode="store.viewMode"
              :selectable="false"
              :gourmand-level="store.gourmandLevel"
              :selected-food="null"
              @toggle="handleToggle"
            />

            <!-- Foods the CDN filed under a category with no section: rendered, not
                 dropped, so a spelling drift or new game category gets noticed. -->
            <FoodCategorySection
              v-if="store.unknownCategoryFoods.length > 0"
              title="Unknown Categories"
              :hint="unknownCategoryHint"
              :foods="store.unknownCategoryFoods"
              :eaten-foods="store.eatenFoods"
              :manually-marked-foods="store.manuallyMarkedFoods"
              :hide-eaten="store.hideEaten"
              :hide-unusable="store.hideUnusable"
              :source-filter="store.sourceFilter"
              :sort-mode="store.sortMode"
              :sort-asc="store.sortAsc"
              :view-mode="store.viewMode"
              :selectable="false"
              :gourmand-level="store.gourmandLevel"
              :selected-food="null"
              @toggle="handleToggle"
            />
          </div>
        </div>
      </template>
    </div>
  </PaneLayout>
</template>

<script setup lang="ts">
import { ref, onMounted, computed, nextTick } from 'vue'
import { useGourmandStore } from '../../stores/gourmandStore'
import type { FoodSourceFilter } from '../../stores/gourmandStore'
import { matchesSourceFilter } from '../../stores/gourmandStore'
import type { FoodItem } from '../../types/gourmand'
import { FOOD_SOURCE_ORDER, foodSourceKindLabel, isAttainable } from '../../types/gourmand'
import PaneLayout from '../Shared/PaneLayout.vue'
import EmptyState from '../Shared/EmptyState.vue'
import GourmandProgressBar from './GourmandProgressBar.vue'
import FoodComparisonPanel from './FoodComparisonPanel.vue'
import FoodCategorySection from './FoodCategorySection.vue'

const store = useGourmandStore()
const editingLevel = ref(false)
const levelInput = ref<HTMLInputElement | null>(null)

const allFoods = computed(() => store.allFoods)

const hasFavorites = computed(() =>
  store.favoriteMeals.length > 0 ||
  store.favoriteSnacks.length > 0 ||
  store.favoriteInstantSnacks.length > 0
)

const hasSelection = computed(() => store.selectedMeal || store.selectedSnack)

const excludedCount = computed(() => allFoods.value.filter(f => !isAttainable(f)).length)

// Every kind that at least one food actually carries, so the dropdown never
// offers an option that filters to nothing. Counts come along for the ride —
// they make "how much of this is event food?" answerable without clicking.
const sourceFilterOptions = computed<{ value: FoodSourceFilter; label: string; count: number | null }[]>(() => {
  const foods = allFoods.value
  const options: { value: FoodSourceFilter; label: string; count: number | null }[] = [
    { value: 'all', label: 'All', count: foods.length },
  ]

  const notCrafted = foods.filter(f => matchesSourceFilter(f, 'not-crafted')).length
  if (notCrafted > 0 && notCrafted < foods.length) {
    options.push({ value: 'not-crafted', label: 'Not crafted', count: notCrafted })
  }

  for (const kind of FOOD_SOURCE_ORDER) {
    const count = foods.filter(f => f.source_kinds.includes(kind)).length
    if (count > 0) {
      options.push({ value: kind, label: foodSourceKindLabel(kind), count })
    }
  }

  return options
})

// The Unknown Categories bucket exists so a CDN spelling drift or a brand-new
// game category stays visible instead of silently losing its section. Name the
// offending tokens and where ingestion has to learn about them.
const unknownCategories = computed(() =>
  [...new Set(store.unknownCategoryFoods.map(f => f.food_category))].sort(),
)

const unknownCategoryHint = computed(
  () =>
    `Unrecognised CDN category ${unknownCategories.value.map(c => `"${c}"`).join(', ')} — teach ` +
    'canonical_food_category() about it (cdn_persistence.rs) or these foods stay unsectioned.',
)

const unmatchedCount = computed(() => {
  if (!store.reportLoaded || allFoods.value.length === 0) return 0
  const foodNames = new Set(allFoods.value.map(f => f.name))
  let count = 0
  for (const name of store.eatenFoods.keys()) {
    if (!foodNames.has(name)) count++
  }
  return count
})

onMounted(async () => {
  await store.loadAllFoods()
  // Try auto-import from Books folder first, then load from DB
  await store.tryAutoImport()
  await store.loadEatenFoods()
})

function handleMealSelect(food: FoodItem) {
  if (store.selectedMeal?.item_id === food.item_id) {
    store.selectMeal(null)
  } else {
    store.selectMeal(food)
  }
}

function handleSnackSelect(food: FoodItem) {
  if (store.selectedSnack?.item_id === food.item_id) {
    store.selectSnack(null)
  } else {
    store.selectSnack(food)
  }
}

function handleToggle(food: FoodItem) {
  store.toggleFoodEaten(food.name)
}

function startEditLevel() {
  editingLevel.value = true
  nextTick(() => levelInput.value?.focus())
}

function applyLevel(e: Event) {
  const input = e.target as HTMLInputElement
  const val = parseInt(input.value, 10)
  if (!isNaN(val) && val >= 0) {
    store.setManualLevel(val)
  }
  editingLevel.value = false
}
</script>
