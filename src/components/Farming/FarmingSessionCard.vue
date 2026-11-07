<template>
  <!-- Persistent auto-logging toggle — mirrors the third status light in the top-left menu bar.
       Visible in both the no-session and active-session states so it can be toggled any time. -->
  <label class="flex items-center gap-2 text-xs text-text-secondary cursor-pointer shrink-0 self-start">
    <input
      type="checkbox"
      class="accent-accent-gold"
      :checked="autoStartEnabled"
      @change="onToggleAutoStart(($event.target as HTMLInputElement).checked)"
    />
    Automatically start &amp; log a farming session
    <span class="text-text-dim">— turning this off ends &amp; saves the current session</span>
  </label>

  <!-- No active session -->
  <div v-if="!store.sessionActive" class="py-4 flex flex-col items-center gap-4">
    <EmptyState variant="compact" primary="No active farming session" secondary="Start one to track XP, items, favor, and more." />
    <div class="flex items-center gap-3">
      <input
        v-model="sessionName"
        type="text"
        placeholder="Session name (optional)"
        class="px-3 py-2 text-sm bg-surface-card border border-border-light rounded text-text-primary placeholder-text-dim outline-none focus:border-entity-item"
      />
      <button
        @click="store.startSession(sessionName || undefined)"
        class="px-4! py-2! text-sm! bg-[#2a3a2a]! border border-[#4a5a4a]! text-value-positive! rounded cursor-pointer transition-all font-medium hover:bg-[#3a4a3a] hover:border-[#5a7a5a] hover:text-value-positive">
        Start Session
      </button>
    </div>
  </div>

  <!-- Active session -->
  <template v-else-if="s">
    <!-- Session header bar -->
    <div class="bg-surface-card border border-border-light rounded-lg p-3">
      <div class="flex items-center justify-between">
        <div class="flex items-center gap-3">
          <span
            :class="[
              'inline-block w-2 h-2 rounded-full',
              s.endTime ? 'bg-text-dim' : s.isPaused ? 'bg-value-neutral-warm animate-pulse' : 'bg-value-positive animate-pulse'
            ]" />
          <input
            :value="s.name"
            @change="store.updateName(($event.target as HTMLInputElement).value)"
            class="text-base font-bold text-entity-item bg-transparent border-none outline-none w-64 hover:bg-[#2a2a3e] focus:bg-[#2a2a3e] rounded px-1 -mx-1"
          />
          <span v-if="s.endTime" class="text-xs text-text-dim uppercase tracking-wide">(Ended)</span>
          <span v-if="s.isPaused" class="text-xs text-value-neutral-warm font-bold uppercase tracking-wide">(Paused)</span>
        </div>

        <div class="flex items-center gap-3">
          <!-- Live timer -->
          <span class="text-lg font-mono font-bold text-text-primary">{{ store.elapsed }}</span>

          <button
            v-if="!s.endTime"
            @click="store.togglePause"
            :class="[
              'px-3 py-1.5 text-xs border rounded cursor-pointer transition-all font-medium',
              s.isPaused
                ? 'bg-[#3a4a2a]! border-[#5a7a3a]! text-value-positive!'
                : 'bg-[#2a2a3e] border-border-light text-text-secondary hover:bg-[#3a3a4e] hover:text-text-primary'
            ]">
            {{ s.isPaused ? "Resume" : "Pause" }}
          </button>
          <button
            v-if="!s.endTime"
            @click="store.endSession"
            class="px-3 py-1.5 text-xs bg-[#3a2a2a]! border border-[#5a3a3a]! rounded text-value-negative! cursor-pointer transition-all font-medium hover:bg-[#4a3a3a] hover:border-[#6a4a4a]">
            End Session
          </button>
          <button
            @click="store.reset"
            class="px-3 py-1.5 text-xs bg-[#2a2a3a]! border border-[#4a4a5a]! rounded text-text-secondary cursor-pointer transition-all font-medium hover:bg-[#3a3a4e] hover:border-border-hover hover:text-text-primary">
            Reset
          </button>
        </div>
      </div>

      <!-- Timing + notes row -->
      <div class="flex items-start gap-4 mt-2">
        <span class="text-xs text-text-muted shrink-0 pt-1">
          Started {{ formatTs(s.startTime) }}
          <span v-if="s.endTime"> · Ended {{ formatTs(s.endTime) }}</span>
        </span>
        <textarea
          :value="s.notes"
          @change="store.updateNotes(($event.target as HTMLTextAreaElement).value)"
          placeholder="Session notes..."
          rows="1"
          class="flex-1 px-2 py-1 text-xs bg-[#12122a] border border-border-default rounded text-text-secondary placeholder-text-dim outline-none resize-y focus:border-entity-item"
        />
      </div>

      <!-- Quick stats -->
      <div class="flex gap-6 mt-2 flex-wrap">
        <div class="text-center">
          <span class="text-[0.6rem] text-text-muted uppercase tracking-wide">Total XP</span>
          <EntityTooltipWrapper :delay="500" :interactive="true" border-class="border-entity-item/40" class="inline-flex!">
            <span class="text-sm font-bold text-text-primary ml-1 cursor-help">{{ store.totalXpGained.toLocaleString() }}</span>
            <template #tooltip>
              <XpBreakdownChart :skills="store.skillSummary" :total-xp="store.totalXpGained" />
            </template>
          </EntityTooltipWrapper>
        </div>
        <div class="text-center">
          <span class="text-[0.6rem] text-text-muted uppercase tracking-wide">Items +</span>
          <span class="text-sm font-bold text-value-positive ml-1">{{ store.totalItemsGained }}</span>
        </div>
        <div v-if="store.totalItemsLost > 0" class="text-center">
          <span class="text-[0.6rem] text-text-muted uppercase tracking-wide">Items -</span>
          <span class="text-sm font-bold text-value-negative ml-1">{{ store.totalItemsLost }}</span>
        </div>
        <div v-if="store.totalFavorGained !== 0" class="text-center">
          <span class="text-[0.6rem] text-text-muted uppercase tracking-wide">Favor</span>
          <span :class="['text-sm font-bold ml-1', store.totalFavorGained > 0 ? 'text-value-neutral-warm' : 'text-value-negative']">
            {{ store.totalFavorGained > 0 ? '+' : '' }}{{ store.totalFavorGained.toFixed(0) }}
          </span>
        </div>
        <div v-if="store.totalKills > 0" class="text-center">
          <span class="text-[0.6rem] text-text-muted uppercase tracking-wide">Kills</span>
          <span class="text-sm font-bold text-[#e87e7e] ml-1">{{ store.totalKills }}</span>
        </div>
        <div v-if="s.vendorGold > 0" class="text-center">
          <span class="text-[0.6rem] text-text-muted uppercase tracking-wide">Councils</span>
          <span class="text-sm font-bold text-value-gold ml-1">{{ s.vendorGold.toLocaleString() }}g</span>
        </div>
      </div>
    </div>

    <!-- 4-column layout: Skills | Looted Items | (Skinning/Butchering + Mining/Survey stacked) | Activity Log -->
    <div class="grid grid-cols-[240px_1fr_1fr_280px] gap-3 flex-1 min-h-0">
      <!-- LEFT: Skills Panel -->
      <div class="bg-surface-dark border border-border-default rounded-lg p-3 overflow-y-auto">
        <div class="text-[0.65rem] uppercase tracking-widest text-entity-item mb-2 font-bold">Skills</div>
        <EmptyState v-if="store.skillSummary.length === 0" variant="compact" primary="No skill gains yet" />
        <div class="flex flex-col gap-1">
          <div
            v-for="skill in store.skillSummary"
            :key="skill.name"
            class="relative rounded overflow-hidden bg-black/30 border border-border-default">
            <!-- XP progress fill background -->
            <div
              class="absolute inset-0 bg-[#2a4a2a] transition-[width] duration-300"
              :style="{ width: skill.xpProgress + '%' }" />
            <!-- Content -->
            <div class="relative flex items-center justify-between px-2 py-1.5 z-10">
              <div class="flex items-center gap-1.5 min-w-0">
                <SkillInline :reference="skill.name" :show-icon="true" class="text-xs" />
                <span v-if="skill.levelsGained > 0" class="text-[0.6rem] text-value-neutral-warm font-bold">
                  +{{ skill.levelsGained }}lvl
                </span>
              </div>
              <div class="flex flex-col items-end shrink-0">
                <span class="text-xs font-bold text-value-positive">+{{ skill.gained.toLocaleString() }}</span>
                <span class="text-[10px] leading-tight text-text-dim">{{ skill.perHour.toLocaleString() }}/hr</span>
              </div>
            </div>
          </div>
        </div>

        <!-- Favor section -->
        <template v-if="store.favorSummary.length > 0">
          <div class="text-[0.65rem] uppercase tracking-widest text-text-dim mt-3 mb-2 font-bold">Favor</div>
          <div class="flex flex-col gap-1">
            <div
              v-for="fav in store.favorSummary"
              :key="fav.name"
              class="flex items-center justify-between px-2 py-1.5 rounded text-xs bg-black/20 border border-border-default">
              <NpcInline :reference="fav.name" />
              <span
                :class="[
                  'font-mono font-bold',
                  fav.delta > 0 ? 'text-value-neutral-warm' : 'text-value-negative'
                ]">
                {{ fav.delta > 0 ? '+' : '' }}{{ fav.delta.toFixed(1) }}
              </span>
            </div>
          </div>
        </template>

        <!-- Kills section -->
        <template v-if="store.killSummary.length > 0">
          <div class="text-[0.65rem] uppercase tracking-widest text-[#e87e7e] mt-3 mb-2 font-bold">Kills</div>
          <div class="flex flex-col gap-1">
            <div
              v-for="kill in store.killSummary"
              :key="kill.name"
              class="rounded text-xs bg-black/20 border border-border-default">
              <div class="flex items-center justify-between px-2 py-1.5">
                <EnemyInline :reference="kill.name" />
                <div class="flex items-center gap-2 shrink-0">
                  <span class="font-mono font-bold text-[#e87e7e]">x{{ kill.count }}</span>
                  <span class="text-[10px] leading-tight text-text-dim">{{ kill.perHour }}/hr</span>
                </div>
              </div>
              <!-- Loot from this enemy type -->
              <div v-if="kill.loot.length > 0" class="px-2 pb-1.5 flex flex-wrap gap-x-3 gap-y-0.5">
                <div v-for="loot in kill.loot" :key="loot.name" class="flex items-center gap-1 text-[0.6rem] text-text-dim">
                  <ItemInline :reference="loot.name" class="text-[0.6rem]!" />
                  <span class="text-value-positive font-mono">x{{ loot.quantity }}</span>
                </div>
              </div>
            </div>
          </div>
        </template>
      </div>

      <!-- CENTER: Looted Items Panel -->
      <div class="bg-surface-dark border border-border-default rounded-lg p-3 overflow-y-auto">
        <div class="text-[0.65rem] uppercase tracking-widest text-text-dim font-bold mb-2">Looted Items</div>
        <EmptyState v-if="store.lootedItems.length === 0" variant="compact" primary="No items looted yet" secondary="Hover an item to see drop rates per enemy." />
        <div class="flex flex-col gap-1">
          <EntityTooltipWrapper
            v-for="item in store.lootedItems"
            :key="item.name"
            :delay="500"
            :interactive="true"
            border-class="border-entity-item/40"
            class="w-full!">
            <div class="flex items-center justify-between px-2 py-1.5 rounded text-xs bg-black/20 border border-border-default hover:border-entity-item/40 cursor-help w-full">
              <span class="text-entity-item font-medium truncate">{{ displayName(item.name) }}</span>
              <span class="text-text-secondary shrink-0 ml-2">
                Looted <span class="text-value-positive font-mono font-bold">{{ item.quantity }}</span>
              </span>
            </div>
            <template #tooltip>
              <ItemDropBreakdown :item-name="item.name" :total-looted="item.quantity" />
            </template>
          </EntityTooltipWrapper>
        </div>
      </div>

      <!-- CENTER-RIGHT: Skinning/Butchering (top) + Mining/Survey (bottom), stacked -->
      <div class="flex flex-col gap-3 min-h-0">
        <!-- Skinning & Butchering Panel -->
        <div class="bg-surface-dark border border-border-default rounded-lg p-3 overflow-y-auto flex-1 min-h-0">
          <div class="text-[0.65rem] uppercase tracking-widest text-[#c8a47e] font-bold mb-2">Skinning &amp; Butchering</div>
          <EmptyState v-if="store.extractedItems.length === 0" variant="compact" primary="Nothing extracted yet" secondary="Skin or butcher a corpse to track yields." />
          <div class="flex flex-col gap-1">
            <EntityTooltipWrapper
              v-for="item in store.extractedItems"
              :key="item.name"
              :delay="500"
              :interactive="true"
              border-class="border-[#c8a47e]/40"
              class="w-full!">
              <div class="flex items-center justify-between px-2 py-1.5 rounded text-xs bg-black/20 border border-border-default hover:border-[#c8a47e]/40 cursor-help w-full">
                <span class="flex items-center gap-1.5 min-w-0">
                  <span class="text-[#c8a47e] font-medium truncate">{{ displayName(item.name) }}</span>
                  <span class="text-[10px] leading-tight text-text-dim uppercase shrink-0">{{ item.skill }}</span>
                </span>
                <span class="text-text-secondary shrink-0 ml-2">
                  Extracted <span class="text-value-positive font-mono font-bold">{{ item.quantity }}</span>
                </span>
              </div>
              <template #tooltip>
                <ItemDropBreakdown :item-name="item.name" :total-looted="item.quantity" mode="extract" />
              </template>
            </EntityTooltipWrapper>
          </div>
        </div>

        <!-- Mining & Survey Panel -->
        <div class="bg-surface-dark border border-border-default rounded-lg p-3 overflow-y-auto flex-1 min-h-0">
          <div class="text-[0.65rem] uppercase tracking-widest text-[#7ea4c8] font-bold mb-2">Mining &amp; Survey</div>
          <EmptyState v-if="store.gatheredItems.length === 0" variant="compact" primary="Nothing gathered yet" secondary="Mine a node or use a survey map to track yields by source." />
          <div class="flex flex-col gap-1">
            <EntityTooltipWrapper
              v-for="item in store.gatheredItems"
              :key="item.name"
              :delay="500"
              :interactive="true"
              border-class="border-[#7ea4c8]/40"
              class="w-full!">
              <div class="flex items-center justify-between px-2 py-1.5 rounded text-xs bg-black/20 border border-border-default hover:border-[#7ea4c8]/40 cursor-help w-full">
                <span class="flex items-center gap-1.5 min-w-0">
                  <span class="text-[#7ea4c8] font-medium truncate">{{ displayName(item.name) }}</span>
                  <span class="text-[10px] leading-tight text-text-dim uppercase shrink-0">{{ item.skill }}</span>
                </span>
                <span class="text-text-secondary shrink-0 ml-2">
                  Gathered <span class="text-value-positive font-mono font-bold">{{ item.quantity }}</span>
                </span>
              </div>
              <template #tooltip>
                <ItemDropBreakdown :item-name="item.name" :total-looted="item.quantity" mode="gathered" />
              </template>
            </EntityTooltipWrapper>
          </div>
        </div>
      </div>

      <!-- RIGHT: Activity Log -->
      <FarmingLog />
    </div>
  </template>
</template>

<script setup lang="ts">
import { ref, computed, reactive } from "vue";
import { useFarmingStore } from "../../stores/farmingStore";
import { useGameDataStore } from "../../stores/gameDataStore";
import { useSettingsStore } from "../../stores/settingsStore";
import { formatAnyTimestamp as formatTs } from "../../composables/useTimestamp";
import EmptyState from "../Shared/EmptyState.vue";
import ItemInline from "../Shared/Item/ItemInline.vue";
import SkillInline from "../Shared/Skill/SkillInline.vue";
import NpcInline from "../Shared/NPC/NpcInline.vue";
import EnemyInline from "../Shared/Enemy/EnemyInline.vue";
import EntityTooltipWrapper from "../Shared/EntityTooltipWrapper.vue";
import ItemDropBreakdown from "./ItemDropBreakdown.vue";
import XpBreakdownChart from "./XpBreakdownChart.vue";
import FarmingLog from "./FarmingLog.vue";

const store = useFarmingStore();
const gameData = useGameDataStore();
const settingsStore = useSettingsStore();
const s = computed(() => store.session);
const sessionName = ref("");

// Persistent auto-logging toggle, shared with the menu-bar status light via the settings store.
const autoStartEnabled = computed(() => settingsStore.settings.autoStartFarmingSessions);

function onToggleAutoStart(enabled: boolean) {
  void store.setAutoStart(enabled);
}

// Lazily resolve internal item names (e.g. "SpiderLeg") to display names
// ("Spider Leg") for the looted-items list. The row is the hover target for
// the drop breakdown, so we render a plain resolved name rather than an
// ItemInline (which would spawn its own competing tooltip).
const resolvedNames = reactive<Record<string, string>>({});
function displayName(reference: string): string {
  if (!(reference in resolvedNames)) {
    resolvedNames[reference] = reference;
    gameData.resolveItem(reference).then((item) => {
      if (item?.name) resolvedNames[reference] = item.name;
    }).catch(() => {});
  }
  return resolvedNames[reference];
}
</script>
