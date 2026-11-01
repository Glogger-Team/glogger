<template>
  <Teleport to="body">
    <Transition name="search-overlay">
      <div v-if="show" class="fixed inset-0 z-50 flex items-start justify-center pt-[15vh]" @mousedown.self="close">
        <!-- Backdrop -->
        <div class="absolute inset-0 bg-black/40" @click="close" />

        <!-- Search panel -->
        <div class="relative w-150 max-w-[90vw] max-h-[60vh] flex flex-col bg-surface-base border border-border-default rounded-lg shadow-2xl overflow-hidden">
          <!-- Search input -->
          <div class="flex items-center gap-2 px-4 py-3 border-b border-border-default">
            <span class="text-text-muted text-sm">&#x1F50D;</span>
            <input
              ref="inputRef"
              v-model="query"
              type="text"
              placeholder="Search everything... (try type:item, skill:Sword, level:30-50)"
              class="flex-1 bg-transparent border-none text-sm text-text-primary placeholder-text-muted focus:outline-none"
              @keydown="handleKeydown"
            />
            <kbd v-if="!query" class="text-[10px] text-text-muted bg-surface-elevated border border-border-default rounded px-1.5 py-0.5">ESC</kbd>
          </div>

          <!-- Results -->
          <div v-if="categories.length > 0" ref="resultsRef" class="flex-1 overflow-y-auto py-1">
            <template v-for="category in categories" :key="category.name">
              <div class="px-3 py-1">
                <div class="text-[10px] font-semibold text-text-muted uppercase tracking-wider">{{ category.name }}</div>
              </div>
              <button
                v-for="(result, rIdx) in category.results"
                :key="`${category.name}-${rIdx}`"
                class="w-full flex items-center gap-3 px-4 py-1.5 text-left cursor-pointer border-none transition-colors"
                :class="flatIndex(category.name, rIdx) === selectedIndex
                  ? 'bg-accent-gold/15 text-text-primary'
                  : 'bg-transparent text-text-secondary hover:bg-surface-elevated hover:text-text-primary'"
                @click="selectResult(result)"
                @mouseenter="selectedIndex = flatIndex(category.name, rIdx)"
              >
                <span class="flex-1 text-sm truncate" @click.stop="selectResult(result)">
                  <ItemInline v-if="category.name === 'Game Items' || category.name === 'Your Items'" :reference="result.label" :show-icon="false" />
                  <RecipeInline v-else-if="category.name === 'Game Recipes'" :reference="result.label" :show-icon="false" />
                  <NpcInline v-else-if="category.name === 'NPCs'" :reference="String(result.navigation.entityId ?? result.label)" />
                  <QuestInline v-else-if="category.name === 'Quests'" :reference="String(result.navigation.entityId ?? result.label)" />
                  <SkillInline v-else-if="category.name === 'Skills' || category.name === 'Your Skills'" :reference="result.label" :show-icon="false" />
                  <AreaInline v-else-if="category.name === 'Areas'" :reference="String(result.navigation.entityId ?? result.label)" />
                  <EnemyInline v-else-if="category.name === 'Enemies'" :reference="String(result.navigation.entityId ?? result.label)" />
                  <span v-else>{{ result.label }}</span>
                </span>
                <span class="text-xs text-text-muted truncate max-w-50">{{ result.detail }}</span>
              </button>
            </template>
          </div>

          <!-- Empty state -->
          <div v-else-if="query.length >= 2 && !loading" class="px-4 py-8 text-center text-sm text-text-muted">
            No results for "{{ query }}"
          </div>

          <!-- Loading indicator -->
          <div v-else-if="loading" class="px-4 py-8 text-center text-sm text-text-muted">
            Searching...
          </div>

          <!-- Syntax help when empty -->
          <div v-else class="px-4 py-4 text-xs text-text-muted">
            <div class="text-center mb-3">Type to search across all game data, your inventory, and more</div>
            <div class="grid grid-cols-2 gap-x-6 gap-y-1 max-w-sm mx-auto">
              <span class="text-text-secondary font-mono">type:item</span>
              <span>restrict to entity type</span>
              <span class="text-text-secondary font-mono">skill:Sword</span>
              <span>filter by skill</span>
              <span class="text-text-secondary font-mono">area:Serbule</span>
              <span>filter by zone</span>
              <span class="text-text-secondary font-mono">level:30-50</span>
              <span>level range</span>
              <span class="text-text-secondary font-mono">keyword:Food</span>
              <span>item keyword</span>
              <span class="text-text-secondary font-mono">"exact phrase"</span>
              <span>exact match</span>
              <span class="text-text-secondary font-mono">-keyword:X</span>
              <span>exclude</span>
            </div>
          </div>

          <!-- Footer -->
          <div v-if="categories.length > 0" class="px-3 py-1.5 border-t border-border-default flex items-center gap-3 text-[10px] text-text-muted">
            <span><kbd class="bg-surface-elevated border border-border-default rounded px-1 py-0.5">↑↓</kbd> navigate</span>
            <span><kbd class="bg-surface-elevated border border-border-default rounded px-1 py-0.5">↵</kbd> open</span>
            <span><kbd class="bg-surface-elevated border border-border-default rounded px-1 py-0.5">esc</kbd> close</span>
          </div>
        </div>
      </div>
    </Transition>
  </Teleport>
</template>

<script setup lang="ts">
import { ref, watch, nextTick, computed, onMounted, onBeforeUnmount } from "vue"
import ItemInline from "../Shared/Item/ItemInline.vue"
import RecipeInline from "../Shared/Recipe/RecipeInline.vue"
import NpcInline from "../Shared/NPC/NpcInline.vue"
import QuestInline from "../Shared/Quest/QuestInline.vue"
import SkillInline from "../Shared/Skill/SkillInline.vue"
import AreaInline from "../Shared/Area/AreaInline.vue"
import EnemyInline from "../Shared/Enemy/EnemyInline.vue"
import { useUnifiedSearch, type SearchResult } from "../../composables/useUnifiedSearch"

const props = defineProps<{
  show: boolean
}>()

const emit = defineEmits<{
  "update:show": [value: boolean]
  navigate: [result: SearchResult]
}>()

const inputRef = ref<HTMLInputElement>()
const resultsRef = ref<HTMLElement>()
const query = ref("")
const selectedIndex = ref(0)
let previouslyFocused: HTMLElement | null = null

const { categories, loading } = useUnifiedSearch(query)

// Build a flat index map so arrow keys work across categories
const allResults = computed(() => {
  const flat: { category: string; result: SearchResult }[] = []
  for (const cat of categories.value) {
    for (const r of cat.results) {
      flat.push({ category: cat.name, result: r })
    }
  }
  return flat
})

function flatIndex(categoryName: string, resultIndex: number): number {
  let idx = 0
  for (const cat of categories.value) {
    if (cat.name === categoryName) return idx + resultIndex
    idx += cat.results.length
  }
  return 0
}

// Reset state when opening
watch(() => props.show, async (open) => {
  if (open) {
    previouslyFocused = document.activeElement instanceof HTMLElement ? document.activeElement : null
    query.value = ""
    selectedIndex.value = 0
    await nextTick()
    inputRef.value?.focus()
  } else {
    previouslyFocused?.focus()
    previouslyFocused = null
  }
})

// Reset selection when results change
watch(categories, () => {
  selectedIndex.value = 0
})

function close() {
  emit("update:show", false)
}

// Window-level Esc: the footer promises `esc closes`, but the input-only
// handler orphans Esc as soon as focus lands on a result button.
// Capture phase + stopPropagation: QuickSearch sits above picker dialogs
// (global Ctrl+F layer), so its Esc must not reach their window listeners —
// one Esc closes only the topmost overlay.
function onWindowKeydown(event: KeyboardEvent) {
  if (event.key === "Escape" && props.show) {
    close()
    event.preventDefault()
    event.stopPropagation()
  }
}
onMounted(() => window.addEventListener("keydown", onWindowKeydown, true))
onBeforeUnmount(() => window.removeEventListener("keydown", onWindowKeydown, true))

function handleKeydown(event: KeyboardEvent) {
  const total = allResults.value.length

  if (event.key === "Escape") {
    close()
    event.preventDefault()
  } else if (event.key === "ArrowDown") {
    selectedIndex.value = Math.min(total - 1, selectedIndex.value + 1)
    scrollSelectedIntoView()
    event.preventDefault()
  } else if (event.key === "ArrowUp") {
    selectedIndex.value = Math.max(0, selectedIndex.value - 1)
    scrollSelectedIntoView()
    event.preventDefault()
  } else if (event.key === "Enter" && total > 0) {
    selectResult(allResults.value[selectedIndex.value].result)
    event.preventDefault()
  }
}

function scrollSelectedIntoView() {
  nextTick(() => {
    if (!resultsRef.value) return
    // Category headers + result buttons are all children; find the right button
    const buttons = resultsRef.value.querySelectorAll("button")
    buttons[selectedIndex.value]?.scrollIntoView({ block: "nearest" })
  })
}

function selectResult(result: SearchResult) {
  emit("navigate", result)
  close()
}
</script>

<style scoped>
.search-overlay-enter-active,
.search-overlay-leave-active {
  transition: opacity 0.15s ease;
}
.search-overlay-enter-from,
.search-overlay-leave-to {
  opacity: 0;
}
</style>
