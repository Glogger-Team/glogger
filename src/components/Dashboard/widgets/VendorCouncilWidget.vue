<template>
  <div class="flex flex-col gap-2 overflow-y-auto max-h-96">
    <!-- Tab toggle -->
    <div class="flex items-center gap-1.5 px-1">
      <button
        class="px-2 py-0.5 text-[10px] rounded cursor-pointer transition-colors"
        :class="activeTab === 'available'
          ? 'bg-accent-gold/20 border border-accent-gold/40 text-text-primary'
          : 'text-text-muted hover:text-text-secondary'"
        @click="activeTab = 'available'"
      >
        Available
      </button>
      <button
        class="px-2 py-0.5 text-[10px] rounded cursor-pointer transition-colors"
        :class="activeTab === 'earned'
          ? 'bg-accent-gold/20 border border-accent-gold/40 text-text-primary'
          : 'text-text-muted hover:text-text-secondary'"
        @click="activeTab = 'earned'"
      >
        Earned
      </button>
    </div>

    <!-- View mode toggle + item type filter (available tab only) -->
    <div v-if="activeTab === 'available'" class="flex items-center gap-1.5 px-1 flex-wrap">
      <button
        class="px-2 py-0.5 text-[10px] rounded cursor-pointer transition-colors"
        :class="viewMode === 'character'
          ? 'bg-accent-gold/20 border border-accent-gold/40 text-text-primary'
          : 'text-text-muted hover:text-text-secondary'"
        @click="viewMode = 'character'"
      >
        Active Character
      </button>
      <button
        class="px-2 py-0.5 text-[10px] rounded cursor-pointer transition-colors"
        :class="viewMode === 'all'
          ? 'bg-accent-gold/20 border border-accent-gold/40 text-text-primary'
          : 'text-text-muted hover:text-text-secondary'"
        @click="switchToAll"
      >
        All Characters
      </button>
      <button
        class="px-2 py-0.5 text-[10px] rounded cursor-pointer transition-colors text-text-muted hover:text-text-secondary"
        title="Expand all categories"
        @click="expandAllCategories"
      >
        Expand all
      </button>
      <button
        class="px-2 py-0.5 text-[10px] rounded cursor-pointer transition-colors text-text-muted hover:text-text-secondary"
        title="Collapse all categories"
        @click="collapseAllCategories"
      >
        Collapse
      </button>
      <select
        v-model="itemTypeFilter"
        class="ml-auto px-1.5 py-0.5 text-[10px] bg-surface-base border border-border-default rounded text-text-primary cursor-pointer focus:outline-none focus:border-accent-gold/50"
      >
        <option value="">All Categories</option>
        <optgroup label="Categories">
          <option v-for="cat in CATEGORY_ORDER" :key="cat" :value="'cat:' + cat">{{ cat }}</option>
        </optgroup>
        <optgroup label="Item Types">
          <option v-for="t in allItemTypes" :key="t" :value="t">{{ t }}</option>
        </optgroup>
      </select>
    </div>

    <!-- Loading state for aggregate (available tab) -->
    <div v-if="activeTab === 'available' && viewMode === 'all' && aggregateLoading" class="text-text-dim text-xs italic px-2">
      Loading cross-character data...
    </div>

    <!-- Empty state (available tab) -->
    <div v-else-if="activeTab === 'available' && vendorEntries.length === 0" class="text-text-dim text-xs italic px-2">
      No vendors found in CDN data.
    </div>

    <!-- Empty state (earned tab) -->
    <div v-else-if="activeTab === 'earned' && earnedVendorEntries.length === 0" class="text-text-dim text-xs italic px-2">
      No council earnings tracked.
    </div>

    <template v-else-if="activeTab === 'available'">
      <!-- Grand total -->
      <div class="flex items-center justify-between px-2 py-1.5 bg-surface-elevated rounded border border-border-default">
        <span class="text-xs text-text-muted">Total Councils Remaining</span>
        <span class="text-sm font-bold font-mono" :class="grandTotalColorClass">
          {{ grandTotalAvailable.toLocaleString() }}
          <span v-if="grandTotalMax > 0" class="text-text-dim font-normal">
            / {{ grandTotalMax.toLocaleString() }}
          </span>
        </span>
      </div>

      <!-- Favorites (only when pinned) -->
      <div v-if="favoriteEntries.length > 0" class="flex flex-col">
        <div class="flex items-center gap-1.5 px-1.5 py-1">
          <span class="text-xs font-semibold text-accent-gold">★ Favorites</span>
          <span class="text-[10px] text-text-dim">({{ favoriteEntries.length }})</span>
          <span class="ml-auto text-xs font-mono text-text-secondary">
            {{ favoriteEntries.reduce((sum, v) => sum + (v.goldAvailable ?? 0), 0).toLocaleString() }}
          </span>
        </div>
        <div class="flex flex-col gap-0.5 px-1">
          <VendorRow
            v-for="v in favoriteEntries"
            :key="'fav-' + v.npcKey"
            :entry="v"
            :view-mode="viewMode"
            :current-category="null"
            :is-favorite="true"
            :quick-edit-key="quickEditKey"
            :quick-edit-value="quickEditValue"
            @set-to-cap="setToCap"
            @start-quick-edit="startQuickEdit"
            @save-quick-edit="saveQuickEdit"
            @cancel-quick-edit="quickEditKey = null"
            @update:quick-edit-value="quickEditValue = $event"
            @open-context-menu="openContextMenu"
            @toggle-favorite="toggleFavorite(v.npcKey)"
          />
        </div>
      </div>
      <template v-if="itemTypeFilter">
        <div class="flex flex-col gap-0.5 px-1">
          <VendorRow
            v-for="v in vendorEntries"
            :key="v.npcKey"
            :entry="v"
            :view-mode="viewMode"
            :is-favorite="isFavorite(v.npcKey)"
            :quick-edit-key="quickEditKey"
            :quick-edit-value="quickEditValue"
            @set-to-cap="setToCap"
            @start-quick-edit="startQuickEdit"
            @save-quick-edit="saveQuickEdit"
            @cancel-quick-edit="quickEditKey = null"
            @update:quick-edit-value="quickEditValue = $event"
            @open-context-menu="openContextMenu"
            @toggle-favorite="toggleFavorite(v.npcKey)"
          />
        </div>
      </template>

      <!-- Category groups when unfiltered -->
      <template v-else>
        <div
          v-for="cat in categories"
          :key="cat.name"
          class="flex flex-col"
        >
          <button
            class="flex items-center justify-between px-1.5 py-1 cursor-pointer hover:bg-surface-elevated/50 rounded transition-colors"
            @click="toggleCategory(cat.name)"
          >
            <span class="flex items-center gap-1.5">
              <span
                class="text-[10px] transition-transform"
                :class="expandedCategories.has(cat.name) ? 'rotate-90' : ''"
              >&#x25B6;</span>
              <span class="text-xs font-semibold text-text-secondary">{{ cat.name }}</span>
              <span class="text-[10px] text-text-dim">({{ cat.vendors.length }})</span>
            </span>
            <span class="text-xs font-mono" :class="categoryColorClass(cat)" :title="sharedTotalTitle(cat)">
              {{ cat.totalAvailable.toLocaleString() }}
              <span v-if="cat.totalMax > 0" class="text-text-dim">
                / {{ cat.totalMax.toLocaleString() }}
              </span>
              <span v-if="hasSharedVendors(cat)" class="text-[10px] text-text-dim italic">
                ({{ cat.exclusiveAvailable.toLocaleString() }} only here)
              </span>
            </span>
          </button>

          <div
            v-if="expandedCategories.has(cat.name)"
            class="flex flex-col gap-0.5 pl-4 mt-0.5"
          >
            <VendorRow
              v-for="v in cat.vendors"
              :key="v.npcKey"
              :entry="v"
              :view-mode="viewMode"
              :current-category="cat.name"
              :is-favorite="isFavorite(v.npcKey)"
              :quick-edit-key="quickEditKey"
              :quick-edit-value="quickEditValue"
              @set-to-cap="setToCap"
              @start-quick-edit="startQuickEdit"
              @save-quick-edit="saveQuickEdit"
              @cancel-quick-edit="quickEditKey = null"
              @update:quick-edit-value="quickEditValue = $event"
              @open-context-menu="openContextMenu"
              @toggle-favorite="toggleFavorite(v.npcKey)"
            />
          </div>
        </div>
      </template>
    </template>

    <!-- Earned councils tab -->
    <template v-else-if="activeTab === 'earned'">
      <!-- Grand totals -->
      <div class="flex items-center justify-between px-2 py-1.5 bg-surface-elevated rounded border border-border-default">
        <span class="text-xs text-text-muted">Total Earned (Current)</span>
        <span class="text-sm font-bold font-mono text-accent-gold">
          {{ grandTotalEarnedCurrent.toLocaleString() }}
        </span>
      </div>
      <div class="flex items-center justify-between px-2 py-1.5 bg-surface-elevated rounded border border-border-default">
        <span class="text-xs text-text-muted">Total Earned (Lifetime)</span>
        <span class="text-sm font-bold font-mono text-value-positive">
          {{ grandTotalEarnedLifetime.toLocaleString() }}
        </span>
      </div>
      <!-- Filter + expand controls (mirrors available tab) -->
      <div class="flex items-center gap-1.5 px-1 flex-wrap">
        <button
          class="px-2 py-0.5 text-[10px] rounded cursor-pointer transition-colors text-text-muted hover:text-text-secondary"
          title="Expand all categories"
          @click="expandAllCategories"
        >
          Expand all
        </button>
        <button
          class="px-2 py-0.5 text-[10px] rounded cursor-pointer transition-colors text-text-muted hover:text-text-secondary"
          title="Collapse all categories"
          @click="collapseAllCategories"
        >
          Collapse
        </button>
        <select
          v-model="itemTypeFilter"
          class="ml-auto px-1.5 py-0.5 text-[10px] bg-surface-base border border-border-default rounded text-text-primary cursor-pointer focus:outline-none focus:border-accent-gold/50"
        >
          <option value="">All Categories</option>
          <optgroup label="Categories">
            <option v-for="cat in CATEGORY_ORDER" :key="cat" :value="'cat:' + cat">{{ cat }}</option>
          </optgroup>
          <optgroup label="Item Types">
            <option v-for="t in allItemTypes" :key="t" :value="t">{{ t }}</option>
          </optgroup>
        </select>
      </div>

      <template v-if="itemTypeFilter">
        <div class="flex flex-col gap-0.5 px-1">
          <EarnedVendorRow
            v-for="v in earnedVendorEntries"
            :key="v.npcKey"
            :entry="v"
          />
        </div>
      </template>

      <!-- Category groups when unfiltered -->
      <template v-else>
        <div
          v-for="cat in earnedCategories"
          :key="cat.name"
          class="flex flex-col"
        >
          <button
            class="flex items-center justify-between px-1.5 py-1 cursor-pointer hover:bg-surface-elevated/50 rounded transition-colors"
            @click="toggleCategory(cat.name)"
          >
            <span class="flex items-center gap-1.5">
              <span
                class="text-[10px] transition-transform"
                :class="expandedCategories.has(cat.name) ? 'rotate-90' : ''"
              >&#x25B6;</span>
              <span class="text-xs font-semibold text-text-secondary">{{ cat.name }}</span>
              <span class="text-[10px] text-text-dim">({{ cat.vendors.length }})</span>
            </span>
            <span class="text-xs font-mono text-text-secondary">
              {{ cat.totalCurrent.toLocaleString() }}
              <span class="text-text-dim">/ {{ cat.totalLifetime.toLocaleString() }}</span>
            </span>
          </button>

          <div
            v-if="expandedCategories.has(cat.name)"
            class="flex flex-col gap-0.5 pl-4 mt-0.5"
          >
            <EarnedVendorRow
              v-for="v in cat.vendors"
              :key="v.npcKey"
              :entry="v"
              :current-category="cat.name"
            />
          </div>
        </div>
      </template>
    </template>

    <!-- Right-click context menu (single instance, teleported to escape widget clipping) -->
    <Teleport to="body">
      <div
        v-if="ctxMenu.visible"
        class="fixed inset-0 z-50"
        @click="closeContextMenu"
        @contextmenu.prevent="closeContextMenu"
      />
      <div
        v-if="ctxMenu.visible && ctxMenu.entry"
        class="fixed z-50 min-w-[150px] rounded border border-border-default bg-surface-elevated shadow-lg py-0.5"
        :style="{ left: ctxMenu.x + 'px', top: ctxMenu.y + 'px' }"
      >
        <div class="px-2.5 py-1 text-[10px] text-text-dim truncate border-b border-border-default/60">
          {{ ctxMenu.entry?.npcName }}
        </div>
        <button
          class="w-full text-left px-2.5 py-1 text-xs text-text-primary hover:bg-surface-hover cursor-pointer disabled:opacity-40 disabled:cursor-not-allowed"
          :disabled="!ctxMenu.entry?.goldMax"
          @click="setToZeroFromMenu"
        >
          Set councils to 0
        </button>
        <button
          class="w-full text-left px-2.5 py-1 text-xs text-text-primary hover:bg-surface-hover cursor-pointer disabled:opacity-40 disabled:cursor-not-allowed"
          :disabled="ctxMenu.entry?.goldMax == null"
          @click="setToCapFromMenu"
        >
          Set to full cap
        </button>
        <button
          class="w-full text-left px-2.5 py-1 text-xs text-text-primary hover:bg-surface-hover cursor-pointer"
          @click="startQuickEditFromMenu"
        >
          Set custom&hellip;
        </button>
      </div>
    </Teleport>
  </div>
</template>

<script setup lang="ts">
import { computed, ref, watch, onBeforeUnmount } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { useGameStateStore } from '../../../stores/gameStateStore'
import { useGameDataStore } from '../../../stores/gameDataStore'
import { useCharacterStore } from '../../../stores/characterStore'
import { useSettingsStore } from '../../../stores/settingsStore'
import { useViewPrefs } from '../../../composables/useViewPrefs'
import { hasBuyCapacity, getStoreService, goldCapAtTier } from '../../../composables/useNpcServices'
import VendorRow from './VendorRow.vue'
import EarnedVendorRow from './EarnedVendorRow.vue'

const VENDOR_RESET_HOURS = 168

// ── Category mapping ────────────────────────────────────────────
//     appear under Armor, Weapons AND Jewelry.
//   Recipe = cooking recipes AND lore recipes → Food + Books.
//   Mushroom/MushroomParts = food AND alchemy reagent → both.
// Unmapped tokens fall through to 'Other' — audit new CDN drops via
// docs/CDN-full-examples/npcs.json CapIncreases when that happens.

const CATEGORY_MAP: Record<string, string[]> = {
  // Armor & Shields — worn armor and generic equippable gear
  Armor: ['Armor & Shields'],
  Shield: ['Armor & Shields'],
  ClothArmor: ['Armor & Shields'],
  ClothEquipment: ['Armor & Shields'],
  LeatherArmor: ['Armor & Shields'],
  MetalArmor: ['Armor & Shields'],
  Clothing: ['Armor & Shields'],
  // Generic equippable gear spans all three gear buckets
  Equipment: ['Armor & Shields', 'Weapons', 'Jewelry'],

  // Weapons
  Weapon: ['Weapons'],
  Arrow: ['Weapons'],

  // Jewelry — wearable rings/amulets only (NOT gems or ore)
  Jewelry: ['Jewelry'],
  Ring: ['Jewelry'],
  Amulet: ['Jewelry'],
  Racial: ['Jewelry'],

  // Gems & Crystals — raw stones, not finished jewelry
  Gem: ['Gems & Crystals'],
  Crystal: ['Gems & Crystals'],
  MassiveCrystal: ['Gems & Crystals'],
  Seashell: ['Gems & Crystals'],

  // Metals & Ores — smithing raw stock
  Ore: ['Metals & Ores'],
  MetalSlab: ['Metals & Ores'],

  // Skins & Trophies — animal hides and corpse-harvested parts
  Skin: ['Skins & Trophies'],
  Skinning: ['Skins & Trophies'],
  Fur: ['Skins & Trophies'],
  Hide: ['Skins & Trophies'],
  CorpseTrophy: ['Skins & Trophies'],
  Skull: ['Skins & Trophies'],
  Bone: ['Skins & Trophies'],
  BodyOrgan: ['Skins & Trophies'],

  // Potions & Alchemy — consumables, reagents, first aid
  Potion: ['Potions & Alchemy'],
  RestorativePotion: ['Potions & Alchemy'],
  AlchemyIngredient: ['Potions & Alchemy'],
  Alchemy: ['Potions & Alchemy'],
  AlchemyRelated: ['Potions & Alchemy'],
  Drug: ['Potions & Alchemy'],
  MagicOil: ['Potions & Alchemy'],
  MagicDust: ['Potions & Alchemy'],
  BottledItem: ['Potions & Alchemy'],
  FirstAidKit: ['Potions & Alchemy'],
  FirstAidBomb: ['Potions & Alchemy'],
  // Mushrooms are food AND alchemy reagents — both buckets
  Mushroom: ['Food & Cooking', 'Potions & Alchemy'],
  MushroomParts: ['Food & Cooking', 'Potions & Alchemy'],

  // Food & Cooking
  Food: ['Food & Cooking'],
  Edible: ['Food & Cooking'],
  PreparedFood: ['Food & Cooking'],
  CookingIngredient: ['Food & Cooking'],
  Cheese: ['Food & Cooking'],
  CheesemakingIngredient: ['Food & Cooking'],
  Butter: ['Food & Cooking'],
  BreadDish: ['Food & Cooking'],
  BrewingIngredient: ['Food & Cooking'],
  BrewingRelated: ['Food & Cooking'],
  Meat: ['Food & Cooking'],
  Fish: ['Food & Cooking'],
  Egg: ['Food & Cooking'],
  Dairy: ['Food & Cooking'],
  Vegetable: ['Food & Cooking'],
  Fruit: ['Food & Cooking'],
  Drink: ['Food & Cooking'],
  Hellfreeze: ['Food & Cooking'],
  // Recipes are bought by cooks and lore collectors alike
  Recipe: ['Food & Cooking', 'Books & Lore'],

  // Books & Lore
  Book: ['Books & Lore'],
  Document: ['Books & Lore'],
  Scroll: ['Books & Lore'],
  KhyrulekLore: ['Books & Lore'],
  Parchment: ['Books & Lore'],
  Textbook: ['Books & Lore'],
  TextbookSkill_FireMagic: ['Books & Lore'],
  TextbookSkill_IceMagic: ['Books & Lore'],

  // Gardening & Dye
  Dye: ['Gardening & Dye'],
  Flower: ['Gardening & Dye'],
  Plant: ['Gardening & Dye'],
  Seed: ['Gardening & Dye'],
  Seedling: ['Gardening & Dye'],
  Nature: ['Gardening & Dye'],
  GardeningRelated: ['Gardening & Dye'],

  // Tools & Crafting — work gear and craft mats
  Tool: ['Tools & Crafting'],
  SkillIngredient: ['Tools & Crafting'],
  Carpentry: ['Tools & Crafting'],
  Blacksmithing: ['Tools & Crafting'],
  Gadgeteering: ['Tools & Crafting'],
  Gizmo: ['Tools & Crafting'],
  Wood: ['Tools & Crafting'],
  Wooden: ['Tools & Crafting'],
  Furniture: ['Tools & Crafting'],
  Textiles: ['Tools & Crafting'],
  Candle: ['Tools & Crafting'],
  CandleRelated: ['Tools & Crafting'],

  // Art & Valuables — sell-for-cash curios
  Art: ['Art & Valuables'],
  Artwork: ['Art & Valuables'],
  Antique: ['Art & Valuables'],
  VendorTrash: ['Art & Valuables'],
  CouncilCertificate: ['Art & Valuables'],
  GoblinCallingCard: ['Art & Valuables'],
  SnailShell: ['Art & Valuables'],

  // Augments — socketables, their own system
  Augment: ['Augments'],
  AugmentationRelated: ['Augments'],
}

const CATEGORY_ORDER = [
  'Armor & Shields',
  'Weapons',
  'Jewelry',
  'Gems & Crystals',
  'Metals & Ores',
  'Skins & Trophies',
  'Potions & Alchemy',
  'Food & Cooking',
  'Books & Lore',
  'Gardening & Dye',
  'Tools & Crafting',
  'Art & Valuables',
  'Augments',
  'Other',
]

function getCategoryNames(itemType: string): string[] {
  return CATEGORY_MAP[itemType] ?? ['Other']
}

// ── Stores ──────────────────────────────────────────────────────

const gameState = useGameStateStore()
const gameData = useGameDataStore()
const characterStore = useCharacterStore()
const settingsStore = useSettingsStore()

// ── Tab and view mode ───────────────────────────────────────────

const activeTab = ref<'available' | 'earned'>('available')
const viewMode = ref<'character' | 'all'>('character')

// ── Aggregate data ──────────────────────────────────────────────

interface AggregateVendorEntry {
  npc_key: string
  characters: {
    character_name: string
    vendor_gold_available: number | null
    vendor_gold_max: number | null
    vendor_gold_timer_start: string | null
    live_confirmed_at: string | null
    favor_tier: string | null
    snapshot_spent: number | null
    snapshot_vendor_reset: string | null
    snapshot_timestamp: string | null
  }[]
}

const aggregateData = ref<AggregateVendorEntry[]>([])
const aggregateLoading = ref(false)

async function loadAggregate() {
  const serverName = settingsStore.settings.activeServerName
  if (!serverName) return
  aggregateLoading.value = true
  try {
    aggregateData.value = await invoke<AggregateVendorEntry[]>('get_aggregate_vendor', { serverName })
  } catch (e) {
    console.error('[VendorCouncilWidget] Failed to load aggregate vendor data:', e)
    aggregateData.value = []
  } finally {
    aggregateLoading.value = false
  }
}

function switchToAll() {
  viewMode.value = 'all'
  loadAggregate()
}

// Reload aggregate when vendor data changes while in aggregate mode
watch(() => gameState.vendorByNpc, () => {
  if (viewMode.value === 'all') loadAggregate()
})

// ── Collapsed/expanded state ────────────────────────────────────

const expandedCategories = ref(new Set<string>())

function toggleCategory(name: string) {
  if (expandedCategories.value.has(name)) {
    expandedCategories.value.delete(name)
  } else {
    expandedCategories.value.add(name)
  }
}

function expandAllCategories() {
  expandedCategories.value = new Set(CATEGORY_ORDER)
}

function collapseAllCategories() {
  expandedCategories.value = new Set()
}
// ── Favorites (widget-local) ────────────────────────────────────
// npcKeys pinned inside this widget; persisted via viewPreferences so they
// survive restarts. Section renders only when non-empty.
const { prefs: widgetPrefs, update: updateWidgetPrefs } = useViewPrefs('dashboard.vendor-councils', {
  favoriteNpcKeys: [] as string[],
})

const favoriteNpcKeys = computed<string[]>({
  get: () => widgetPrefs.value.favoriteNpcKeys ?? [],
  set: (keys) => updateWidgetPrefs({ favoriteNpcKeys: keys }),
})

function isFavorite(npcKey: string): boolean {
  return favoriteNpcKeys.value.includes(npcKey)
}

function toggleFavorite(npcKey: string) {
  const keys = favoriteNpcKeys.value
  favoriteNpcKeys.value = keys.includes(npcKey)
    ? keys.filter(k => k !== npcKey)
    : [...keys, npcKey]
}

// ── Quick edit ──────────────────────────────────────────────────

const quickEditKey = ref<string | null>(null)
const quickEditValue = ref(0)

function startQuickEdit(v: VendorEntry) {
  quickEditKey.value = v.npcKey
  quickEditValue.value = v.goldMax ?? 0
}

async function setToCap(v: VendorEntry) {
  if (v.goldMax == null) return
  await saveVendorGold(v.npcKey, v.goldMax, v.goldMax)
}

async function setToZero(v: VendorEntry) {
  if (!v.goldMax) return
  // available=0 with a known cap → backend starts a fresh 168h reset timer.
  await saveVendorGold(v.npcKey, 0, v.goldMax)
}

async function saveQuickEdit(v: VendorEntry) {
  const max = v.goldMax ?? quickEditValue.value
  await saveVendorGold(v.npcKey, quickEditValue.value, max)
  quickEditKey.value = null
}

async function saveVendorGold(npcKey: string, goldAvailable: number, goldMax: number) {
  const characterName = settingsStore.settings.activeCharacterName
  const serverName = settingsStore.settings.activeServerName
  if (!characterName || !serverName) return
  try {
    await invoke('set_manual_vendor_gold', { characterName, serverName, npcKey, goldAvailable, goldMax, resetHoursRemaining: null })
    await gameState.refreshDomain('vendor')
  } catch (e) {
    console.error('[VendorCouncilWidget] Failed to save vendor gold:', e)
  }
}

// ── Right-click context menu ────────────────────────────────────

const ctxMenu = ref<{ visible: boolean; x: number; y: number; entry: VendorEntry | null }>({
  visible: false,
  x: 0,
  y: 0,
  entry: null,
})

function openContextMenu(payload: { entry: VendorEntry; x: number; y: number }) {
  // Clamp so the menu never spills off the right/bottom edge.
  const menuW = 170
  const menuH = 116
  ctxMenu.value = {
    visible: true,
    x: Math.min(payload.x, window.innerWidth - menuW),
    y: Math.min(payload.y, window.innerHeight - menuH),
    entry: payload.entry,
  }
  window.addEventListener('keydown', onCtxKeydown)
  window.addEventListener('scroll', closeContextMenu, true)
}

function closeContextMenu() {
  if (!ctxMenu.value.visible) return
  ctxMenu.value.visible = false
  ctxMenu.value.entry = null
  window.removeEventListener('keydown', onCtxKeydown)
  window.removeEventListener('scroll', closeContextMenu, true)
}

function onCtxKeydown(e: KeyboardEvent) {
  if (e.key === 'Escape') closeContextMenu()
}

async function setToZeroFromMenu() {
  const entry = ctxMenu.value.entry
  closeContextMenu()
  if (entry) await setToZero(entry)
}

async function setToCapFromMenu() {
  const entry = ctxMenu.value.entry
  closeContextMenu()
  if (entry) await setToCap(entry)
}

function startQuickEditFromMenu() {
  const entry = ctxMenu.value.entry
  closeContextMenu()
  if (entry) startQuickEdit(entry)
}

onBeforeUnmount(closeContextMenu)

// ── Reset detection ─────────────────────────────────────────────

/**
 * Parse a stored vendor timer_start as UTC. The DB holds two formats:
 * the live tailer writes "YYYY-MM-DD HH:MM:SS" (space-separated, no zone),
 * while set_manual_vendor_gold writes ISO "YYYY-MM-DDTHH:MM:SSZ" (already
 * zoned). Normalize both to valid ISO-UTC so neither double-appends 'Z'.
 */
function parseVendorTimestamp(timerStart: string): Date {
  let s = timerStart.trim().replace(' ', 'T')
  if (!/[zZ]$/.test(s)) s += 'Z'
  return new Date(s)
}

/** Check if timer has expired (>168h since timer started), meaning gold has reset to cap. */
function hasTimerExpired(timerStart: string | null): boolean {
  if (!timerStart) return false
  const start = parseVendorTimestamp(timerStart)
  if (isNaN(start.getTime())) return false
  const resetAt = start.getTime() + VENDOR_RESET_HOURS * 60 * 60 * 1000
  return Date.now() >= resetAt
}

// ── Vendor entry building ───────────────────────────────────────

interface CharacterBreakdownEntry {
  characterName: string
  goldAvailable: number | null
  goldMax: number | null
}

interface VendorEntry {
  npcKey: string
  npcName: string
  area: string | null
  goldAvailable: number | null
  goldMax: number | null
  resetLabel: string | null
  assumedReset: boolean
  itemCategories: string[]
  rawItemTypes: string[]
  characterBreakdown: CharacterBreakdownEntry[] | null
  /** Snapshot import anchor: councils spent at import, display-only. */
  snapshotSpent: number | null
}

function resolvePlayerTier(npcKey: string): string {
  const gsFavor = gameState.favorByNpc[npcKey]
  if (gsFavor?.favor_tier) return gsFavor.favor_tier
  const snap = characterStore.npcFavor.find(f => f.npc_key === npcKey)
  if (snap?.favor_level) return snap.favor_level
  return 'Neutral'
}

function computeTimerLabel(timerStart: string | null): string | null {
  if (!timerStart) return null
  const start = parseVendorTimestamp(timerStart)
  if (isNaN(start.getTime())) return null
  const resetAt = new Date(start.getTime() + VENDOR_RESET_HOURS * 60 * 60 * 1000)
  const now = new Date()
  const remaining = resetAt.getTime() - now.getTime()
  if (remaining <= 0) return null
  const hours = Math.floor(remaining / (1000 * 60 * 60))
  const days = Math.floor(hours / 24)
  const remainingHours = hours % 24
  return days > 0 ? `${days}d ${remainingHours}h` : `${hours}h`
}

function getRawItemTypes(npcKey: string): string[] {
  const npc = gameData.npcsByKey[npcKey]
  if (!npc) return []
  const store = getStoreService(npc)
  if (!store) return []
  const allItemTypes = new Set<string>()
  for (const cap of store.capIncreases) {
    for (const t of cap.itemTypes) allItemTypes.add(t)
  }
  return [...allItemTypes]
}

function getItemCategories(npcKey: string): string[] {
  const raw = getRawItemTypes(npcKey)
  if (raw.length === 0) return ['Other']
  return [...new Set(raw.flatMap(getCategoryNames))]
}

// ── Item type filter ────────────────────────────────────────────

const itemTypeFilter = ref('')

/** All unique raw item types across all vendors with buy capacity, sorted */
const allItemTypes = computed(() => {
  const types = new Set<string>()
  for (const [key, npc] of Object.entries(gameData.npcsByKey)) {
    if (!hasBuyCapacity(npc)) continue
    for (const t of getRawItemTypes(key)) types.add(t)
  }
  return [...types].sort()
})

/** Councils spent → councils still available, given the favor cap. */
function spentToAvailable(spent: number, cap: number): number | null {
  if (!Number.isFinite(spent) || !Number.isFinite(cap)) return null
  if (spent === 0) return cap
  if (cap >= spent) return cap - spent
  // Cap may have dropped since the snapshot (favor decay or stale tier):
  // available must be ≥ 0, and a changed cap makes the delta meaningless.
  if (spent <= cap * 3) return 0
  return null
}

/**
 * Newest-wins vendor gold resolution. Live log lines (`last_confirmed_at`)
 * only advance when you talk to that vendor; a zone change or Export Character as JSON
 * refreshes the snapshot for every NPC at once — so a newer snapshot must
 * beat a stale live figure, and a newer sale must beat a stale snapshot.
 * Returns the winning { available, timerStart } or nulls when neither side
 * has data (non-VIP: no snapshot fields → live-only behaviour).
 */
function resolveVendorGold(
  liveAvailable: number | null,
  liveConfirmedAt: string | null | undefined,
  liveTimer: string | null,
  snapshotSpent: number | null | undefined,
  snapshotReset: string | null | undefined,
  snapshotTimestamp: string | null | undefined,
  cap: number | null,
): { available: number | null; timerStart: string | null } {
  // Normalize "YYYY-MM-DD HH:MM:SS" (live) vs ISO (snapshot) to comparable ms.
  const liveMs = liveConfirmedAt ? Date.parse(liveConfirmedAt.replace(' ', 'T') + (/[zZ]$/.test(liveConfirmedAt.trim()) ? '' : 'Z')) : NaN
  const snapMs = snapshotTimestamp ? Date.parse(snapshotTimestamp.replace(' ', 'T') + (/[zZ]$/.test(snapshotTimestamp.trim()) ? '' : 'Z')) : NaN
  const snapshotNewer = Number.isFinite(snapMs) && (!Number.isFinite(liveMs) || snapMs > liveMs)
  if (snapshotNewer && cap != null && snapshotSpent != null) {
    const derived = spentToAvailable(snapshotSpent, cap)
    return { available: derived, timerStart: snapshotReset ?? null }
  }
  if (liveAvailable != null) return { available: liveAvailable, timerStart: liveTimer }
  if (cap != null && snapshotSpent != null) {
    return { available: spentToAvailable(snapshotSpent, cap), timerStart: snapshotReset ?? null }
  }
  return { available: null, timerStart: null }
}

const characterEntries = computed<VendorEntry[]>(() => {
  const entries: VendorEntry[] = []

  for (const [key, npc] of Object.entries(gameData.npcsByKey)) {
    if (!hasBuyCapacity(npc)) continue

    const vendorState = gameState.vendorByNpc[key]
    const playerTier = resolvePlayerTier(key)
    const capAtTier = goldCapAtTier(npc, playerTier)
    const maxGold = vendorState?.vendor_gold_max ?? capAtTier?.maxGold ?? null

    // Newest-wins: live log vs snapshot, per-NPC. Snapshot covers every NPC
    // on each export; live only advances when you talk to that vendor.
    // Non-VIP: no snapshot fields → live-only, unchanged behaviour.
    const snapFavor = characterStore.npcFavor.find(f => f.npc_key === key)
    const resolved = resolveVendorGold(
      vendorState?.vendor_gold_available ?? null,
      vendorState?.last_confirmed_at,
      vendorState?.vendor_gold_timer_start ?? null,
      snapFavor?.councils_spent_at_vendor,
      snapFavor?.vendor_reset_time,
      snapFavor?.snapshot_timestamp,
      maxGold,
    )
    const snapTimer = vendorState ? resolved.timerStart : (snapFavor?.vendor_reset_time ?? null)
    // Determine effective gold: if timer has expired, assume reset to cap
    let goldAvailable = resolved.available
    let assumedReset = false
    let resetLabel: string | null = null

    if (goldAvailable != null) {
      if (hasTimerExpired(snapTimer)) {
        // Timer expired — vendor has reset to cap
        goldAvailable = maxGold
        assumedReset = true
        resetLabel = '(reset)'
      } else {
        resetLabel = computeTimerLabel(snapTimer)
        if (resetLabel) resetLabel = `(${resetLabel})`
      }
    }

    entries.push({
      npcKey: key,
      npcName: npc.name,
      area: npc.area_friendly_name ?? null,
      goldAvailable,
      goldMax: maxGold,
      resetLabel,
      assumedReset,
      itemCategories: getItemCategories(key),
      rawItemTypes: getRawItemTypes(key),
      characterBreakdown: null,
      snapshotSpent: snapFavor?.councils_spent_at_vendor ?? null,
    })
  }

  // Sort: tracked vendors first, then untracked
  entries.sort((a, b) => {
    if (a.goldAvailable != null && b.goldAvailable == null) return -1
    if (a.goldAvailable == null && b.goldAvailable != null) return 1
    return a.npcName.localeCompare(b.npcName)
  })

  return entries
})

// Aggregate vendor entries (all characters) — includes ALL CDN vendors with buy capacity
const aggregateEntries = computed<VendorEntry[]>(() => {
  // Index aggregate DB data by npc_key for lookup
  const aggByNpc = new Map<string, AggregateVendorEntry>()
  for (const agg of aggregateData.value) {
    aggByNpc.set(agg.npc_key, agg)
  }

  const entries: VendorEntry[] = []

  for (const [key, npc] of Object.entries(gameData.npcsByKey)) {
    if (!hasBuyCapacity(npc)) continue

    const agg = aggByNpc.get(key)
    // For untracked vendors, use active character's favor tier as best guess
    const fallbackTier = resolvePlayerTier(key)
    const fallbackCap = goldCapAtTier(npc, fallbackTier)?.maxGold ?? null

    if (agg) {
      // Has DB data — newest-wins per character, same as single-character.
      const resolvedCharacters = agg.characters.map(c => {
        // Use per-character favor tier from DB for accurate cap, fall back to active character's tier
        const charTier = c.favor_tier ?? fallbackTier
        const charCap = goldCapAtTier(npc, charTier)?.maxGold ?? null
        const goldMax = c.vendor_gold_max ?? charCap
        const resolved = resolveVendorGold(
          c.vendor_gold_available ?? null,
          c.live_confirmed_at,
          c.vendor_gold_timer_start ?? null,
          c.snapshot_spent,
          c.snapshot_vendor_reset,
          c.snapshot_timestamp,
          goldMax,
        )
        let goldAvail = resolved.available
        const timerStart = resolved.timerStart ?? null
        if (goldAvail != null && hasTimerExpired(timerStart)) {
          goldAvail = goldMax
        }
        return { ...c, vendor_gold_available: goldAvail, vendor_gold_max: goldMax }
      })

      const totalAvailable = resolvedCharacters.reduce((sum, c) => sum + (c.vendor_gold_available ?? 0), 0)
      const totalMax = resolvedCharacters.reduce((sum, c) => sum + (c.vendor_gold_max ?? 0), 0)

      // Find earliest active (non-expired) timer
      let earliestTimer: string | null = null
      for (const c of agg.characters) {
        if (c.vendor_gold_timer_start && !hasTimerExpired(c.vendor_gold_timer_start)) {
          if (!earliestTimer || c.vendor_gold_timer_start < earliestTimer) {
            earliestTimer = c.vendor_gold_timer_start
          }
        }
      }
      const timerLabel = computeTimerLabel(earliestTimer)

      entries.push({
        npcKey: key,
        npcName: npc.name,
        area: npc.area_friendly_name ?? null,
        goldAvailable: totalAvailable > 0 || resolvedCharacters.some(c => c.vendor_gold_available != null) ? totalAvailable : null,
        goldMax: totalMax > 0 ? totalMax : null,
        resetLabel: timerLabel ? `(${timerLabel})` : null,
        assumedReset: false,
        itemCategories: getItemCategories(key),
        rawItemTypes: getRawItemTypes(key),
        characterBreakdown: resolvedCharacters
          .filter(c => c.vendor_gold_available != null)
          .map(c => ({
            characterName: c.character_name,
            goldAvailable: c.vendor_gold_available,
            goldMax: c.vendor_gold_max,
          })),
        snapshotSpent: null,
      })
    } else {
      // No DB data — untracked vendor
      entries.push({
        npcKey: key,
        npcName: npc.name,
        area: npc.area_friendly_name ?? null,
        goldAvailable: null,
        goldMax: fallbackCap,
        resetLabel: null,
        assumedReset: false,
        itemCategories: getItemCategories(key),
        rawItemTypes: getRawItemTypes(key),
        characterBreakdown: null,
        snapshotSpent: characterStore.npcFavor.find(f => f.npc_key === key)?.councils_spent_at_vendor ?? null,
      })
    }
  }

  // Sort: tracked first, then untracked
  entries.sort((a, b) => {
    if (a.goldAvailable != null && b.goldAvailable == null) return -1
    if (a.goldAvailable == null && b.goldAvailable != null) return 1
    return a.npcName.localeCompare(b.npcName)
  })

  return entries
})

// Switch based on view mode, then apply item type filter
const vendorEntries = computed(() => {
  const base = viewMode.value === 'all' ? aggregateEntries.value : characterEntries.value
  const filter = itemTypeFilter.value
  if (!filter) return base

  if (filter.startsWith('cat:')) {
    // Filter by preset category — show vendors whose mapped categories include this one
    const catName = filter.slice(4)
    return base.filter(v => v.itemCategories.includes(catName))
  }

  // Filter by specific raw item type
  return base.filter(v => v.rawItemTypes.includes(filter))
})

// ── Favorites ───────────────────────────────────────────────────
// VendorEntry rows for pinned keys, in pin order; stale keys (no longer
// vendors, e.g. CDN drift) drop out silently.
const favoriteEntries = computed(() => {
  const keys = favoriteNpcKeys.value
  if (keys.length === 0) return []
  const byKey = new Map(vendorEntries.value.map(v => [v.npcKey, v]))
  return keys.map(k => byKey.get(k)).filter((v): v is VendorEntry => v != null)
})

// ── Category grouping ───────────────────────────────────────────

interface CategoryGroup {
  name: string
  vendors: VendorEntry[]
  totalAvailable: number
  totalMax: number
  /** Vendors appearing ONLY in this bucket — the spendable-here councils. */
  exclusiveAvailable: number
  exclusiveMax: number
}

const categories = computed<CategoryGroup[]>(() => {
  const catMap = new Map<string, VendorEntry[]>()

  for (const entry of vendorEntries.value) {
    for (const cat of entry.itemCategories) {
      if (!catMap.has(cat)) catMap.set(cat, [])
      catMap.get(cat)!.push(entry)
    }
  }

  const groups: CategoryGroup[] = []
  const buildGroup = (name: string, vendors: VendorEntry[]): CategoryGroup => {
    // Vendors listed ONLY here — shared vendors' gold is spendable in
    // several buckets, so the exclusive sum is the "only-here" number.
    const exclusive = vendors.filter(v => v.itemCategories.length === 1)
    return {
      name,
      vendors,
      totalAvailable: vendors.reduce((sum, v) => sum + (v.goldAvailable ?? 0), 0),
      totalMax: vendors.reduce((sum, v) => sum + (v.goldMax ?? 0), 0),
      exclusiveAvailable: exclusive.reduce((sum, v) => sum + (v.goldAvailable ?? 0), 0),
      exclusiveMax: exclusive.reduce((sum, v) => sum + (v.goldMax ?? 0), 0),
    }
  }
  for (const name of CATEGORY_ORDER) {
    const vendors = catMap.get(name)
    if (!vendors || vendors.length === 0) continue
    // Sort: tracked first (by gold desc), then untracked alphabetically
    vendors.sort((a, b) => {
      if (a.goldAvailable != null && b.goldAvailable == null) return -1
      if (a.goldAvailable == null && b.goldAvailable != null) return 1
      if (a.goldAvailable != null && b.goldAvailable != null) return b.goldAvailable - a.goldAvailable
      return a.npcName.localeCompare(b.npcName)
    })
    groups.push(buildGroup(name, vendors))
  }

  // Add any uncategorized
  const covered = new Set(CATEGORY_ORDER)
  for (const [name, vendors] of catMap) {
    if (covered.has(name)) continue
    vendors.sort((a, b) => {
      if (a.goldAvailable != null && b.goldAvailable == null) return -1
      if (a.goldAvailable == null && b.goldAvailable != null) return 1
      if (a.goldAvailable != null && b.goldAvailable != null) return b.goldAvailable - a.goldAvailable
      return a.npcName.localeCompare(b.npcName)
    })
    groups.push(buildGroup(name, vendors))
  }

  return groups
})

// ── Grand totals (deduped — each vendor counted once) ───────────

const grandTotalAvailable = computed(() =>
  vendorEntries.value.reduce((sum, v) => sum + (v.goldAvailable ?? 0), 0)
)

const grandTotalMax = computed(() =>
  vendorEntries.value.reduce((sum, v) => sum + (v.goldMax ?? 0), 0)
)

// ── Color helpers ───────────────────────────────────────────────

const grandTotalColorClass = computed(() => {
  if (grandTotalMax.value === 0) return 'text-text-secondary'
  const ratio = grandTotalAvailable.value / grandTotalMax.value
  if (ratio >= 0.7) return 'text-value-positive'
  if (ratio >= 0.3) return 'text-yellow-400'
  return 'text-value-negative'
})

function categoryColorClass(cat: CategoryGroup): string {
  if (cat.totalMax === 0) return 'text-text-secondary'
  const ratio = cat.totalAvailable / cat.totalMax
  if (ratio >= 0.7) return 'text-value-positive'
  if (ratio >= 0.3) return 'text-yellow-400'
  return 'text-value-negative'
}

/** True when some bucket gold is shared with other buckets. */
function hasSharedVendors(cat: CategoryGroup): boolean {
  return cat.exclusiveAvailable !== cat.totalAvailable || cat.exclusiveMax !== cat.totalMax
}

/** Tooltip explaining shared vs exclusive bucket totals. */
function sharedTotalTitle(cat: CategoryGroup): string {
  if (!hasSharedVendors(cat)) return ''
  return 'Includes vendors also listed elsewhere; ' +
    cat.exclusiveAvailable.toLocaleString() + ' is only spendable here'
}

// ── Earned councils data ────────────────────────────────────────

interface EarnedVendorEntry {
  npcKey: string
  npcName: string
  area: string | null
  earnedCurrent: number
  earnedLifetime: number
  itemCategories: string[]
  rawItemTypes: string[]
}

interface EarnedCategoryGroup {
  name: string
  vendors: EarnedVendorEntry[]
  totalCurrent: number
  totalLifetime: number
}

const earnedVendorEntriesBase = computed<EarnedVendorEntry[]>(() => {
  const entries: EarnedVendorEntry[] = []

  for (const [key, npc] of Object.entries(gameData.npcsByKey)) {
    if (!hasBuyCapacity(npc)) continue

    const vendorState = gameState.vendorByNpc[key]
    const snapFavor = characterStore.npcFavor.find(f => f.npc_key === key)
    const snapSpent = snapFavor?.councils_spent_at_vendor ?? 0
    // Same row-exists guard as the Available tab: live NULL means at-cap.
    const snapTimer = vendorState ? vendorState.vendor_gold_timer_start : snapFavor?.vendor_reset_time ?? null

    // Live-earned this period, else the snapshot's spent (zeroed once its
    // reset timer expires — same 168h rule as the Available tab). Snapshot
    // spend is period-only; lifetime stays live-tracked from zero.
    const earnedCurrent = vendorState?.councils_earned_current
      ?? (hasTimerExpired(snapTimer) ? 0 : snapSpent)
    const earnedLifetime = vendorState?.councils_earned_lifetime ?? 0

    if (earnedCurrent === 0 && earnedLifetime === 0) continue

    entries.push({
      npcKey: key,
      npcName: npc.name,
      area: npc.area_friendly_name ?? null,
      earnedCurrent,
      earnedLifetime,
      itemCategories: getItemCategories(key),
      rawItemTypes: getRawItemTypes(key),
    })
  }

  entries.sort((a, b) => {
    if (a.earnedLifetime !== b.earnedLifetime) return b.earnedLifetime - a.earnedLifetime
    return a.npcName.localeCompare(b.npcName)
  })

  return entries
})

/** Earned list honours the shared category/item-type filter. */
const earnedVendorEntries = computed<EarnedVendorEntry[]>(() => {
  const filter = itemTypeFilter.value
  if (!filter) return earnedVendorEntriesBase.value
  if (filter.startsWith('cat:')) {
    const catName = filter.slice(4)
    return earnedVendorEntriesBase.value.filter(v => v.itemCategories.includes(catName))
  }
  return earnedVendorEntriesBase.value.filter(v => v.rawItemTypes.includes(filter))
})

const earnedCategories = computed<EarnedCategoryGroup[]>(() => {
  const catMap = new Map<string, EarnedVendorEntry[]>()

  for (const entry of earnedVendorEntries.value) {
    for (const cat of entry.itemCategories) {
      if (!catMap.has(cat)) catMap.set(cat, [])
      catMap.get(cat)!.push(entry)
    }
  }

  const groups: EarnedCategoryGroup[] = []
  for (const name of CATEGORY_ORDER) {
    const vendors = catMap.get(name)
    if (!vendors || vendors.length === 0) continue
    vendors.sort((a, b) => {
      if (a.earnedLifetime !== b.earnedLifetime) return b.earnedLifetime - a.earnedLifetime
      return a.npcName.localeCompare(b.npcName)
    })
    groups.push({
      name,
      vendors,
      totalCurrent: vendors.reduce((sum, v) => sum + v.earnedCurrent, 0),
      totalLifetime: vendors.reduce((sum, v) => sum + v.earnedLifetime, 0),
    })
  }

  const covered = new Set(CATEGORY_ORDER)
  for (const [name, vendors] of catMap) {
    if (covered.has(name)) continue
    vendors.sort((a, b) => {
      if (a.earnedLifetime !== b.earnedLifetime) return b.earnedLifetime - a.earnedLifetime
      return a.npcName.localeCompare(b.npcName)
    })
    groups.push({
      name,
      vendors,
      totalCurrent: vendors.reduce((sum, v) => sum + v.earnedCurrent, 0),
      totalLifetime: vendors.reduce((sum, v) => sum + v.earnedLifetime, 0),
    })
  }

  return groups
})

const grandTotalEarnedCurrent = computed(() =>
  earnedVendorEntries.value.reduce((sum, v) => sum + v.earnedCurrent, 0)
)

const grandTotalEarnedLifetime = computed(() =>
  earnedVendorEntries.value.reduce((sum, v) => sum + v.earnedLifetime, 0)
)

</script>
