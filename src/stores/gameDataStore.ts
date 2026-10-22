import { defineStore, acceptHMRUpdate } from "pinia";
import { ref, computed } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type {
  ItemInfo,
  SkillInfo,
  AbilityInfo,
  AbilityFamily,
  RecipeInfo,
  QuestInfo,
  NpcInfo,
  EffectInfo,
  EnemyInfo,
  LorebookEntry,
  LorebookCategoryInfo,
  PlayerTitleInfo,
  AchievementInfo,
  TsysBrowserEntry,
  CacheStatus,
  EntitySources,
  NpcFavorEntry,
  TsysAbilityXref,
  AbilityTsysXref,
  GardeningProductChain,
} from "../types/gameData";
import type { AbilityBuildStats, AbilityItemRef, AbilityModRef } from "../types/abilityStats";
import { extractNpcKeyFromFavorPath } from "../utils/questDisplay";

export type DataStatus = "loading" | "ready" | "error" | "empty";

export const useGameDataStore = defineStore("gameData", () => {
  // ── State ──────────────────────────────────────────────────────────────────
  const status = ref<DataStatus>("loading");
  const errorMessage = ref<string | null>(null);
  const cacheStatus = ref<CacheStatus | null>(null);

  // ── Preloaded NPC map (loaded once on game-data-ready) ────────────────────
  const npcsByKey = ref<Record<string, NpcInfo>>({})
  const npcsByDisplayName = ref<Record<string, NpcInfo>>({})

  async function loadAllNpcsMap(): Promise<void> {
    if (Object.keys(npcsByKey.value).length > 0) return
    const all = await invoke<NpcInfo[]>('get_all_npcs')
    const byKey: Record<string, NpcInfo> = {}
    const byName: Record<string, NpcInfo> = {}
    for (const npc of all) {
      byKey[npc.key] = npc
      byName[npc.name] = npc
    }
    npcsByKey.value = byKey
    npcsByDisplayName.value = byName
  }


  // ── Preloaded quest data + NPC→quest index ───────────────────────────────
  const allQuestsCache = ref<QuestInfo[]>([])

  async function loadAllQuests(): Promise<void> {
    if (allQuestsCache.value.length > 0) return
    allQuestsCache.value = await invoke<QuestInfo[]>('get_all_quests')
  }

  // ── Preloaded achievement catalog (11 rows, backend-sorted) ──────────────
  const achievementsCache = ref<AchievementInfo[]>([])

  async function loadAllAchievements(): Promise<void> {
    if (achievementsCache.value.length > 0) return
    achievementsCache.value = await invoke<AchievementInfo[]>('get_all_achievements')
  }

  const questsByNpc = computed<Map<string, QuestInfo[]>>(() => {
    const map = new Map<string, QuestInfo[]>()
    for (const quest of allQuestsCache.value) {
      const favorNpc = quest.raw?.FavorNpc
      if (!favorNpc) continue
      const npcKey = extractNpcKeyFromFavorPath(favorNpc)
      const existing = map.get(npcKey)
      if (existing) {
        existing.push(quest)
      } else {
        map.set(npcKey, [quest])
      }
    }
    return map
  })

  function getQuestsForNpc(npcKey: string): QuestInfo[] {
    return questsByNpc.value.get(npcKey) ?? []
  }

  // Icon path cache: icon_id → local filesystem path
  const iconPaths = ref<Record<number, string>>({});

  // ── Startup ────────────────────────────────────────────────────────────────

  // Listen for the Rust-emitted events fired when init_game_data() finishes
  listen<number>("game-data-ready", async (_event) => {
    status.value = "ready";
    errorMessage.value = null;
    await Promise.all([refreshCacheStatus(), loadAllNpcsMap(), loadAllQuests(), loadAllAchievements()]);
    startCdnPolling();
  });

  listen<string>("game-data-error", (event) => {
    status.value = "error";
    errorMessage.value = event.payload;
  });

  // Kick off an initial status check immediately (covers the case where
  // game-data-ready fires before we're listening)
  refreshCacheStatus().then(() => {
    if (status.value === 'ready') {
      loadAllNpcsMap()
      loadAllQuests()
      loadAllAchievements()
    }
  });

  // ── Cache management ───────────────────────────────────────────────────────

  async function refreshCacheStatus() {
    try {
      const s: CacheStatus = await invoke("get_cache_status");
      cacheStatus.value = s;
      if (s.item_count > 0) {
        status.value = "ready";
      }
    } catch (e) {
      console.error("get_cache_status failed:", e);
    }
  }

  async function forceRefreshCdn(): Promise<CacheStatus> {
    status.value = "loading";
    try {
      const s: CacheStatus = await invoke("force_refresh_cdn");
      cacheStatus.value = s;
      status.value = "ready";
      return s;
    } catch (e: any) {
      status.value = "error";
      errorMessage.value = String(e);
      throw e;
    }
  }

  // ── CDN version monitoring ───────────────────────────────────��─────────────
  // Polls for CDN version changes while the app is running.
  // When a mismatch is detected, surfaces a blocking modal via cdnUpdateAvailable.

  const CDN_POLL_INTERVAL_MS = 15 * 60 * 1000; // 15 minutes
  const CDN_DISMISS_MS = 30 * 60 * 1000; // 30 minutes

  const cdnUpdateAvailable = ref(false);
  const cdnCurrentVersion = ref(0);
  const cdnRemoteVersion = ref(0);
  const cdnDismissed = ref(false);

  let cdnPollId: ReturnType<typeof setInterval> | null = null;
  let cdnRedismissId: ReturnType<typeof setTimeout> | null = null;

  async function checkCdnVersion() {
    try {
      const result = await invoke<{
        update_available: boolean;
        current_version: number;
        remote_version: number;
      }>("check_cdn_version");

      if (result.update_available) {
        cdnUpdateAvailable.value = true;
        cdnCurrentVersion.value = result.current_version;
        cdnRemoteVersion.value = result.remote_version;
      }
    } catch {
      // Silently ignore — offline, CDN down, etc.
    }
  }

  function startCdnPolling() {
    if (cdnPollId) return;
    // First check after 2 minutes, then every 15 minutes
    setTimeout(() => checkCdnVersion(), 2 * 60 * 1000);
    cdnPollId = setInterval(() => checkCdnVersion(), CDN_POLL_INTERVAL_MS);
  }

  function stopCdnPolling() {
    if (cdnPollId) {
      clearInterval(cdnPollId);
      cdnPollId = null;
    }
  }

  function dismissCdnUpdate() {
    cdnDismissed.value = true;
    if (cdnRedismissId) clearTimeout(cdnRedismissId);
    cdnRedismissId = setTimeout(() => {
      cdnDismissed.value = false;
    }, CDN_DISMISS_MS);
  }

  // ── Unified entity resolvers ─────────────────────────────────────────────
  // Each accepts any known reference form (numeric ID, display name, internal
  // name, CDN key) and resolves to the canonical entity.

  async function resolveItem(reference: string | number): Promise<ItemInfo | null> {
    return invoke<ItemInfo | null>("resolve_item", { reference: String(reference) });
  }

  async function resolveItemsBatch(references: string[]): Promise<Record<string, ItemInfo>> {
    return invoke<Record<string, ItemInfo>>("resolve_items_batch", { references });
  }

  async function resolveSkill(reference: string | number): Promise<SkillInfo | null> {
    return invoke<SkillInfo | null>("resolve_skill", { reference: String(reference) });
  }

  async function resolveRecipe(reference: string | number): Promise<RecipeInfo | null> {
    return invoke<RecipeInfo | null>("resolve_recipe", { reference: String(reference) });
  }

  async function resolveQuest(reference: string): Promise<QuestInfo | null> {
    return invoke<QuestInfo | null>("resolve_quest", { reference });
  }

  /** Synchronous NPC resolver using preloaded maps. Tries CDN key, then display name. */
  function resolveNpcSync(reference: string): NpcInfo | null {
    return npcsByKey.value[reference] ?? npcsByDisplayName.value[reference] ?? null
  }

  /** Async NPC resolver via backend (for cases where NPC map isn't loaded yet). */
  async function resolveNpc(reference: string): Promise<NpcInfo | null> {
    return invoke<NpcInfo | null>("resolve_npc", { reference });
  }

  /** Resolve an ability by numeric ID or display name. */
  async function resolveAbility(reference: string | number): Promise<AbilityInfo | null> {
    return invoke<AbilityInfo | null>("resolve_ability", { reference: String(reference) });
  }

  interface AreaInfo {
    friendly_name: string | null
    short_friendly_name: string | null
  }

  async function resolveArea(reference: string): Promise<AreaInfo | null> {
    return invoke<AreaInfo | null>("resolve_area", { reference });
  }

  // ── Query/filter commands (not replaced by resolvers) ───────────────────

  async function searchItems(
    query: string,
    filters?: {
      equipSlot?: string
      levelMin?: number
      levelMax?: number
      armorType?: string
      effectText?: string
    },
  ): Promise<ItemInfo[]> {
    return invoke<ItemInfo[]>("search_items", {
      query,
      equipSlot: filters?.equipSlot ?? null,
      levelMin: filters?.levelMin ?? null,
      levelMax: filters?.levelMax ?? null,
      armorType: filters?.armorType ?? null,
      effectText: filters?.effectText ?? null,
    });
  }

  async function getItemsByKeyword(keyword: string): Promise<ItemInfo[]> {
    return invoke<ItemInfo[]>("get_items_by_keyword", { keyword });
  }

  async function getAllItemKeywords(): Promise<string[]> {
    return invoke<string[]>("get_all_item_keywords");
  }

  interface IngredientKeywordInfo {
    keyword: string
    description: string
  }

  async function getRecipeIngredientKeywords(): Promise<IngredientKeywordInfo[]> {
    return invoke<IngredientKeywordInfo[]>("get_recipe_ingredient_keywords");
  }

  async function getEquipSlots(): Promise<string[]> {
    return invoke<string[]>("get_equip_slots");
  }

  // ── Skill queries ──────────────────────────────────────────────────────────

  async function getAllSkills(): Promise<SkillInfo[]> {
    return invoke<SkillInfo[]>("get_all_skills");
  }

  // ── Ability queries ────────────────────────────────────────────────────────

  async function getAbilitiesForSkill(skill: string): Promise<AbilityInfo[]> {
    return invoke<AbilityInfo[]>("get_abilities_for_skill", { skill });
  }

  async function getAbilityFamiliesForSkill(skill: string, includeMonster = false): Promise<AbilityFamily[]> {
    return invoke<AbilityFamily[]>("get_ability_families_for_skill", { skill, includeMonster });
  }

  async function getAbilityFamily(abilityId: number): Promise<AbilityFamily | null> {
    return invoke<AbilityFamily | null>("get_ability_family", { abilityId });
  }

  async function searchAbilityFamilies(query: string, skill?: string, limit?: number, includeMonster = false): Promise<AbilityFamily[]> {
    return invoke<AbilityFamily[]>("search_ability_families", { query, skill: skill ?? null, limit: limit ?? null, includeMonster });
  }

  async function getSkillsWithAbilityCounts(includeMonster = false): Promise<[string, number][]> {
    return invoke<[string, number][]>("get_skills_with_ability_counts", { includeMonster });
  }

  // ── Recipe queries ─────────────────────────────────────────────────────────

  async function getRecipesForItem(itemId: number): Promise<RecipeInfo[]> {
    return invoke<RecipeInfo[]>("get_recipes_for_item", { itemId });
  }

  async function getRecipesUsingItem(itemId: number): Promise<RecipeInfo[]> {
    return invoke<RecipeInfo[]>("get_recipes_using_item", { itemId });
  }

  async function searchRecipes(query: string, limit = 50): Promise<RecipeInfo[]> {
    return invoke<RecipeInfo[]>("search_recipes", { query, limit });
  }

  async function getRecipesForSkill(skill: string): Promise<RecipeInfo[]> {
    return invoke<RecipeInfo[]>("get_recipes_for_skill", { skill });
  }

  // ── Gardening product chain ────────────────────────────────────────────────

  async function getGardeningProductChain(itemId: number): Promise<GardeningProductChain | null> {
    return invoke<GardeningProductChain | null>("get_gardening_product_chain", { itemId });
  }

  // ── Quest queries ──────────────────────────────────────────────────────────

  async function getAllQuests(): Promise<QuestInfo[]> {
    return invoke<QuestInfo[]>("get_all_quests");
  }

  async function searchQuests(query: string): Promise<QuestInfo[]> {
    return invoke<QuestInfo[]>("search_quests", { query });
  }

  // ── NPC queries ────────────────────────────────────────────────────────────

  async function getAllNpcs(): Promise<NpcInfo[]> {
    return invoke<NpcInfo[]>("get_all_npcs");
  }

  async function searchNpcs(query: string): Promise<NpcInfo[]> {
    return invoke<NpcInfo[]>("search_npcs", { query });
  }

  async function getNpcsInArea(area: string): Promise<NpcInfo[]> {
    return invoke<NpcInfo[]>("get_npcs_in_area", { area });
  }

  // ── Enemy (AI) queries ─────────────────────────────────────────────────────

  async function getAllEnemies(): Promise<EnemyInfo[]> {
    return invoke<EnemyInfo[]>("get_all_enemies");
  }

  async function searchEnemies(query: string): Promise<EnemyInfo[]> {
    return invoke<EnemyInfo[]>("search_enemies", { query });
  }

  // ── Effect queries ─────────────────────────────────────────────────────────

  async function searchEffects(query: string, limit = 50): Promise<EffectInfo[]> {
    return invoke<EffectInfo[]>("search_effects", { query, limit });
  }

  async function getEffect(id: number): Promise<EffectInfo | null> {
    return invoke<EffectInfo | null>("get_effect", { id });
  }

  // ── TSys browser queries ────────────────────────────────────────────────

  async function getAllTsys(): Promise<TsysBrowserEntry[]> {
    return invoke<TsysBrowserEntry[]>("get_all_tsys");
  }

  async function searchTsys(query: string, limit = 100): Promise<TsysBrowserEntry[]> {
    return invoke<TsysBrowserEntry[]>("search_tsys", { query, limit });
  }

  // ── Player Title queries ──────────────────────────────────────────────────

  async function getAllPlayerTitles(): Promise<PlayerTitleInfo[]> {
    return invoke<PlayerTitleInfo[]>("get_all_player_titles");
  }

  async function searchPlayerTitles(query: string): Promise<PlayerTitleInfo[]> {
    return invoke<PlayerTitleInfo[]>("search_player_titles", { query });
  }

  // ── Achievement queries ───────────────────────────────────────────────────

  async function getAllAchievements(): Promise<AchievementInfo[]> {
    if (achievementsCache.value.length > 0) return achievementsCache.value
    await loadAllAchievements()
    return achievementsCache.value
  }

  async function searchAchievements(query: string): Promise<AchievementInfo[]> {
    return invoke<AchievementInfo[]>("search_achievements", { query });
  }

  // ── Lorebook queries ───────────────────────────────────────────────────────

  async function getAllLorebooks(): Promise<LorebookEntry[]> {
    return invoke<LorebookEntry[]>("get_all_lorebooks");
  }

  async function getLorebookCategories(): Promise<LorebookCategoryInfo[]> {
    return invoke<LorebookCategoryInfo[]>("get_lorebook_categories");
  }

  async function searchLorebooks(query: string, category?: string): Promise<LorebookEntry[]> {
    return invoke<LorebookEntry[]>("search_lorebooks", { query, category: category ?? null });
  }

  // ── Source queries ─────────────────────────────────────────────────────────

  async function getAbilitySources(id: number): Promise<EntitySources> {
    return invoke<EntitySources>("get_ability_sources", { id });
  }

  async function getItemSources(id: number): Promise<EntitySources> {
    return invoke<EntitySources>("get_item_sources", { id });
  }

  async function getRecipeSources(id: number): Promise<EntitySources> {
    return invoke<EntitySources>("get_recipe_sources", { id });
  }

  async function getQuestSources(key: string): Promise<EntitySources> {
    return invoke<EntitySources>("get_quest_sources", { key });
  }

  // ── Cross-reference queries ────────────────────────────────────────────────

  async function getNpcsWantingItem(itemId: number): Promise<NpcFavorEntry[]> {
    return invoke<NpcFavorEntry[]>("get_npcs_wanting_item", { itemId });
  }

  async function getNpcsTrainingSkill(skill: string): Promise<NpcInfo[]> {
    return invoke<NpcInfo[]>("get_npcs_training_skill", { skill });
  }

  async function getQuestsForSkill(skill: string): Promise<QuestInfo[]> {
    return invoke<QuestInfo[]>("get_quests_for_skill", { skill });
  }

  async function getRecipesForKeyword(keyword: string): Promise<RecipeInfo[]> {
    return invoke<RecipeInfo[]>("get_recipes_for_keyword", { keyword });
  }

  async function getQuestsByMoonPhase(moonPhase: string): Promise<QuestInfo[]> {
    return invoke<QuestInfo[]>("get_quests_by_moon_phase", { moonPhase });
  }

  // ── Ability ↔ TSys cross-reference queries ────────────────────────────────

  async function getTsysForAbility(abilityId: number): Promise<TsysAbilityXref[]> {
    return invoke<TsysAbilityXref[]>("get_tsys_for_ability", { abilityId });
  }

  async function getAbilitiesForTsys(tsysKey: string): Promise<AbilityTsysXref[]> {
    return invoke<AbilityTsysXref[]>("get_abilities_for_tsys", { tsysKey });
  }

  /** Batch lookup: TSys keys → ability IDs each one affects. Uses precomputed index (O(1) per key). */
  async function getTsysAbilityMap(tsysKeys: string[]): Promise<Record<string, number[]>> {
    return invoke<Record<string, number[]>>("get_tsys_ability_map", { tsysKeys });
  }

  /**
   * Fold a build's assigned gear mods and equipped items into an ability's combat stats
   * (effective damage/heal/cost). `equippedSkills` gates skill-conditional mods ("while X active").
   */
  async function computeAbilityBuildStats(
    abilityId: number,
    mods: AbilityModRef[],
    items: AbilityItemRef[],
    equippedSkills: string[],
    pvp = false,
  ): Promise<AbilityBuildStats | null> {
    return invoke<AbilityBuildStats | null>("compute_ability_build_stats", {
      abilityId,
      mods,
      items,
      equippedSkills,
      pvp,
    });
  }

  // ── Storage vault queries ──────────────────────────────────────────────────

  interface StorageVaultZoneInfo {
    vault_key: string
    area_key: string | null
    area_name: string | null
    npc_friendly_name: string | null
    num_slots: number | null
  }

  const storageVaultZones = ref<StorageVaultZoneInfo[] | null>(null)

  async function getStorageVaultZones(): Promise<StorageVaultZoneInfo[]> {
    if (storageVaultZones.value) return storageVaultZones.value
    const zones = await invoke<StorageVaultZoneInfo[]>('get_storage_vault_zones')
    storageVaultZones.value = zones
    return zones
  }

  // ── Icon helpers ───────────────────────────────────────────────────────────

  /**
   * Returns a local file path for the given icon ID, fetching from CDN if needed.
   * Results are memoised in iconPaths for the lifetime of the session.
   *
   * Usage in a Vue template:
   *   <img :src="convertFileSrc(iconPath)" />
   * where convertFileSrc is imported from @tauri-apps/api/core.
   */
  async function getIconPath(iconId: number): Promise<string> {
    if (iconPaths.value[iconId]) {
      return iconPaths.value[iconId];
    }
    const path = await invoke<string>("get_icon_path", { iconId });
    iconPaths.value[iconId] = path;
    return path;
  }

  return {
    // State
    status,
    errorMessage,
    cacheStatus,
    iconPaths,
    // Unified entity resolvers
    resolveItem,
    resolveItemsBatch,
    resolveSkill,
    resolveRecipe,
    resolveQuest,
    resolveNpcSync,
    resolveNpc,
    resolveAbility,
    resolveArea,
    // Query/filter actions
    refreshCacheStatus,
    forceRefreshCdn,
    searchItems,
    getItemsByKeyword,
    getAllItemKeywords,
    getRecipeIngredientKeywords,
    getEquipSlots,
    getAllSkills,
    getAbilitiesForSkill,
    getAbilityFamiliesForSkill,
    getAbilityFamily,
    searchAbilityFamilies,
    getSkillsWithAbilityCounts,
    getRecipesForItem,
    getRecipesUsingItem,
    searchRecipes,
    getRecipesForSkill,
    getGardeningProductChain,
    getAllQuests,
    searchQuests,
    getAllNpcs,
    searchNpcs,
    getNpcsInArea,
    npcsByKey,
    npcsByDisplayName,
    loadAllNpcsMap,
    getAllEnemies,
    searchEnemies,
    searchEffects,
    getEffect,
    getAllTsys,
    searchTsys,
    getAllLorebooks,
    getLorebookCategories,
    searchLorebooks,
    getAllPlayerTitles,
    searchPlayerTitles,
    getAllAchievements,
    searchAchievements,
    loadAllAchievements,
    achievementsCache,
    getAbilitySources,
    getItemSources,
    getRecipeSources,
    getQuestSources,
    getStorageVaultZones,
    storageVaultZones,
    getIconPath,
    // CDN version monitoring
    cdnUpdateAvailable,
    cdnCurrentVersion,
    cdnRemoteVersion,
    cdnDismissed,
    checkCdnVersion,
    dismissCdnUpdate,
    stopCdnPolling,
    // Cross-reference queries
    getNpcsWantingItem,
    getNpcsTrainingSkill,
    getQuestsForNpc,
    getQuestsForSkill,
    getQuestsByMoonPhase,
    loadAllQuests,
    allQuestsCache,
    getRecipesForKeyword,
    getTsysForAbility,
    getAbilitiesForTsys,
    getTsysAbilityMap,
    computeAbilityBuildStats,
  };
});

if (import.meta.hot) {
  import.meta.hot.accept(acceptHMRUpdate(useGameDataStore, import.meta.hot))
}
