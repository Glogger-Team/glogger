<template>
  <div class="flex flex-col gap-4 h-full overflow-y-auto">
    <div v-if="loading" class="space-y-3">
      <div class="flex gap-6">
        <SkeletonLoader v-for="i in 4" :key="i" variant="rect" width="w-20" height="h-12" />
      </div>
      <SkeletonLoader variant="text" :lines="4" />
    </div>
    <div v-else-if="error" class="text-[#c87e7e] text-sm">{{ error }}</div>
    <EmptyState v-else-if="sessions.length === 0" variant="panel" primary="No saved farming sessions" secondary="Complete a farming session to see history here." />

    <template v-else>
      <!-- Aggregate stats -->
      <div class="flex gap-6 flex-wrap text-center">
        <div>
          <div class="text-[0.65rem] text-text-muted uppercase tracking-wide">Sessions</div>
          <div class="text-lg font-bold text-text-primary">{{ sessions.length }}</div>
        </div>
        <div>
          <div class="text-[0.65rem] text-text-muted uppercase tracking-wide">Total Time</div>
          <div class="text-lg font-bold text-text-primary">{{ formatDuration(totalElapsed) }}</div>
        </div>
        <div>
          <div class="text-[0.65rem] text-text-muted uppercase tracking-wide">Total XP</div>
          <div class="text-lg font-bold text-[#7ec87e]">{{ totalXp.toLocaleString() }}</div>
        </div>
        <div v-if="totalGold > 0">
          <div class="text-[0.65rem] text-text-muted uppercase tracking-wide">Total Vendor Gold</div>
          <div class="text-lg font-bold text-[#d4af37]">{{ totalGold.toLocaleString() }}g</div>
        </div>
      </div>

      <!-- Session list -->
      <div class="flex flex-col gap-2">
        <div
          v-for="session in sessions"
          :key="session.id"
          class="bg-surface-card border border-border-light rounded-lg overflow-hidden">
          <!-- Summary row -->
          <div
            class="flex items-center justify-between px-4 py-3 cursor-pointer hover:bg-border-subtle transition-colors"
            @click="toggleExpanded(session.id)">
            <div class="flex items-center gap-3">
              <span class="text-xs text-text-dim">{{ formatDate(session.created_at) }}</span>
              <span class="text-sm font-bold text-entity-item">{{ session.name }}</span>
              <span class="text-xs text-text-muted">{{ formatDuration(session.elapsed_seconds) }}</span>
            </div>
            <div class="flex items-center gap-4 text-xs">
              <EntityTooltipWrapper
                v-if="sessionTotalXp(session) > 0"
                :delay="500"
                :interactive="true"
                border-class="border-entity-item/40"
                @click.stop>
                <span class="text-[#7ec87e] cursor-help">
                  +{{ sessionTotalXp(session).toLocaleString() }} XP
                </span>
                <template #tooltip>
                  <XpBreakdownChart :skills="xpSkillsFor(session)" :total-xp="sessionTotalXp(session)" />
                </template>
              </EntityTooltipWrapper>
              <span v-if="sessionTotalXp(session) > 0" class="text-text-dim">
                {{ xpPerHour(session).toLocaleString() }}/hr
              </span>
              <span v-if="session.items.length > 0" class="text-text-secondary">
                {{ session.items.length }} item{{ session.items.length !== 1 ? 's' : '' }}
              </span>
              <span v-if="sessionTotalKills(session) > 0" class="text-[#e87e7e]">
                {{ sessionTotalKills(session) }} kill{{ sessionTotalKills(session) !== 1 ? 's' : '' }}
              </span>
              <span v-if="session.vendor_gold > 0" class="text-[#d4af37]">
                {{ session.vendor_gold.toLocaleString() }}g
              </span>
              <span class="text-text-dim">{{ expanded.has(session.id) ? '\u25B2' : '\u25BC' }}</span>
            </div>
          </div>

          <!-- Expanded detail -->
          <div v-if="expanded.has(session.id)" class="border-t border-border-default px-4 py-3">
            <!-- Editable name + notes -->
            <div class="flex items-start gap-3 mb-3">
              <div class="flex flex-col gap-1 flex-1">
                <input
                  :value="session.name"
                  @change="updateSession(session, 'name', ($event.target as HTMLInputElement).value)"
                  class="text-sm font-bold text-entity-item bg-transparent border-none outline-none w-full hover:bg-border-subtle focus:bg-border-subtle rounded px-1 -mx-1"
                />
                <textarea
                  :value="session.notes"
                  @change="updateSession(session, 'notes', ($event.target as HTMLTextAreaElement).value)"
                  placeholder="Add notes..."
                  rows="2"
                  class="w-full px-2 py-1 text-xs bg-[#12122a] border border-border-default rounded text-text-secondary placeholder-text-dim outline-none resize-y focus:border-entity-item"
                />
              </div>
            </div>

            <!-- Items | Favor | Kills — boxed + independently scrollable, matching the Active Session layout -->
            <div class="grid grid-cols-[repeat(auto-fit,minmax(220px,1fr))] gap-3 mb-3">
              <!-- Items -->
              <div v-if="session.items.length > 0" class="bg-surface-dark border border-border-default rounded-lg p-3 max-h-56 overflow-y-auto">
                <div class="text-[0.6rem] uppercase tracking-widest text-text-dim mb-1.5 font-bold">Items</div>
                <div class="flex flex-col gap-1">
                  <!-- Whole row is the drop-rate hover target (parity with the
                       Active Session item hover). The name stays clickable to
                       drill into the item's entity detail. A plain resolved name
                       is used rather than ItemInline so it doesn't spawn its own
                       competing tooltip. -->
                  <EntityTooltipWrapper
                    v-for="item in session.items"
                    :key="item.item_name"
                    :delay="500"
                    :interactive="true"
                    border-class="border-entity-item/40"
                    class="w-full!">
                    <div class="flex items-center justify-between px-2 py-1 rounded text-xs bg-black/20 border border-border-default hover:border-entity-item/40 cursor-help w-full">
                      <span
                        class="text-entity-item font-medium truncate cursor-pointer hover:underline"
                        @click="navigateToEntity({ type: 'item', id: displayName(item.item_name) })">
                        {{ displayName(item.item_name) }}
                      </span>
                      <div class="flex items-center gap-2 shrink-0 ml-2">
                        <span
                          :class="[
                            'font-mono font-bold',
                            item.net_quantity > 0 ? 'text-[#7ec87e]' : 'text-[#c87e7e]'
                          ]">
                          {{ item.net_quantity > 0 ? '+' : '' }}{{ item.net_quantity }}
                        </span>
                        <span class="text-text-dim text-[0.6rem]">{{ itemPerHour(item.net_quantity, session.elapsed_seconds) }}/hr</span>
                      </div>
                    </div>
                    <template #tooltip>
                      <div class="flex flex-col gap-2 w-72 max-w-[20rem]">
                        <div class="flex items-center justify-between border-b border-border-default pb-1.5">
                          <span class="text-entity-item font-medium truncate">{{ displayName(item.item_name) }}</span>
                          <span class="text-[0.6rem] text-text-muted uppercase tracking-wide shrink-0">
                            Looted <span class="text-value-positive font-bold">{{ item.net_quantity }}</span> this session
                          </span>
                        </div>
                        <ExtractDetailTable :item-name="item.item_name" />
                        <ItemDropBreakdownTable :item-name="item.item_name" scope="combined" />
                        <p class="text-[10px] text-text-dim leading-tight pt-0.5">
                          Lifetime drop rate = kills that dropped this item ÷ all kills of that enemy.
                        </p>
                      </div>
                    </template>
                  </EntityTooltipWrapper>
                </div>
              </div>

              <!-- Favors -->
              <div v-if="session.favors.length > 0" class="bg-surface-dark border border-border-default rounded-lg p-3 max-h-56 overflow-y-auto">
                <div class="text-[0.6rem] uppercase tracking-widest text-text-dim mb-1.5 font-bold">Favor</div>
                <div class="flex flex-col gap-1">
                  <div
                    v-for="fav in session.favors"
                    :key="fav.npc_name"
                    class="flex items-center justify-between px-2 py-1 rounded text-xs bg-black/20 border border-border-default">
                    <NpcInline :reference="fav.npc_name" />
                    <span
                      :class="[
                        'font-mono font-bold',
                        fav.delta > 0 ? 'text-[#c8b47e]' : 'text-[#c87e7e]'
                      ]">
                      {{ fav.delta > 0 ? '+' : '' }}{{ fav.delta.toFixed(1) }}
                    </span>
                  </div>
                </div>
              </div>

              <!-- Kills -->
              <div v-if="session.kills && session.kills.length > 0" class="bg-surface-dark border border-border-default rounded-lg p-3 max-h-56 overflow-y-auto">
                <div class="text-[0.6rem] uppercase tracking-widest text-[#e87e7e] mb-1.5 font-bold">Kills</div>
                <div class="flex flex-col gap-1">
                  <div
                    v-for="kill in session.kills"
                    :key="kill.enemy_name"
                    class="flex items-center justify-between px-2 py-1 rounded text-xs bg-black/20 border border-border-default">
                    <EnemyInline :reference="kill.enemy_name" />
                    <div class="flex items-center gap-2">
                      <span class="text-[#e87e7e] font-mono font-bold">x{{ kill.kill_count }}</span>
                      <span class="text-text-dim text-[0.6rem]">{{ killsPerHour(kill.kill_count, session.elapsed_seconds) }}/hr</span>
                    </div>
                  </div>
                </div>
              </div>
            </div>

            <!-- Delete button -->
            <div class="flex justify-end pt-2 border-t border-border-default">
              <button
                @click.stop="deleteSession(session.id)"
                class="px-3 py-1 text-xs bg-[#3a2a2a]! border border-[#5a3a3a]! rounded text-[#c87e7e]! cursor-pointer transition-all font-medium hover:bg-[#4a3a3a] hover:border-[#6a4a4a]">
                Delete Session
              </button>
            </div>
          </div>
        </div>
      </div>
    </template>
  </div>
</template>

<script setup lang="ts">
import { ref, reactive, onMounted, computed } from "vue";
import { invoke } from "@tauri-apps/api/core";
import type { HistoricalFarmingSession } from "../../types/farming";
import EmptyState from "../Shared/EmptyState.vue";
import SkeletonLoader from "../Shared/SkeletonLoader.vue";
import { formatDateTimeShort, formatDuration } from "../../composables/useTimestamp";
import NpcInline from "../Shared/NPC/NpcInline.vue";
import EnemyInline from "../Shared/Enemy/EnemyInline.vue";
import EntityTooltipWrapper from "../Shared/EntityTooltipWrapper.vue";
import XpBreakdownChart from "./XpBreakdownChart.vue";
import ItemDropBreakdownTable from "./ItemDropBreakdownTable.vue";
import ExtractDetailTable from "./ExtractDetailTable.vue";
import { useGameDataStore } from "../../stores/gameDataStore";
import { useEntityNavigation } from "../../composables/useEntityNavigation";

const sessions = ref<HistoricalFarmingSession[]>([]);
const loading = ref(false);
const error = ref<string | null>(null);
const expanded = ref<Set<number>>(new Set());

const gameData = useGameDataStore();
const { navigateToEntity } = useEntityNavigation();

// Lazily resolve internal item names ("SpiderLeg") to display names
// ("Spider Leg"). The row is the drop-breakdown hover target, so we render a
// plain resolved name rather than ItemInline (which would spawn its own
// competing tooltip) — matching the Active Session card.
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

async function loadSessions() {
  loading.value = true;
  error.value = null;
  try {
    sessions.value = await invoke<HistoricalFarmingSession[]>("get_farming_sessions", { limit: 50 });
  } catch (e: any) {
    error.value = String(e);
  } finally {
    loading.value = false;
  }
}

function toggleExpanded(id: number) {
  if (expanded.value.has(id)) {
    expanded.value.delete(id);
  } else {
    expanded.value.add(id);
  }
  expanded.value = new Set(expanded.value);
}

async function updateSession(
  session: HistoricalFarmingSession,
  field: 'name' | 'notes',
  value: string,
) {
  const updated = { ...session, [field]: value };
  try {
    await invoke("update_farming_session", {
      sessionId: session.id,
      name: updated.name,
      notes: updated.notes,
    });
    // Update local state
    const idx = sessions.value.findIndex((s) => s.id === session.id);
    if (idx >= 0) {
      sessions.value[idx] = { ...sessions.value[idx], [field]: value };
    }
  } catch (e) {
    console.error("[farming] Failed to update session:", e);
  }
}

async function deleteSession(id: number) {
  try {
    await invoke("delete_farming_session", { sessionId: id });
    sessions.value = sessions.value.filter((s) => s.id !== id);
  } catch (e) {
    console.error("[farming] Failed to delete session:", e);
  }
}

function sessionTotalXp(session: HistoricalFarmingSession): number {
  return session.skills.reduce((sum, s) => sum + s.xp_gained, 0);
}

function xpPerHour(session: HistoricalFarmingSession): number {
  const hours = Math.max(1, session.elapsed_seconds) / 3600;
  return Math.round(sessionTotalXp(session) / hours);
}

function xpSkillsFor(session: HistoricalFarmingSession) {
  const hours = Math.max(1, session.elapsed_seconds) / 3600;
  return session.skills.map((s) => ({
    name: s.skill_name,
    gained: s.xp_gained,
    perHour: Math.round(s.xp_gained / hours),
    levelsGained: s.levels_gained,
  }));
}

function itemPerHour(netQuantity: number, elapsedSeconds: number): number {
  const hours = Math.max(1, elapsedSeconds) / 3600;
  return Math.round(Math.abs(netQuantity) / hours);
}

function sessionTotalKills(session: HistoricalFarmingSession): number {
  return (session.kills ?? []).reduce((sum, k) => sum + k.kill_count, 0);
}

function killsPerHour(killCount: number, elapsedSeconds: number): number {
  const hours = Math.max(1, elapsedSeconds) / 3600;
  return Math.round(killCount / hours);
}

const totalElapsed = computed(() =>
  sessions.value.reduce((sum, s) => sum + s.elapsed_seconds, 0)
);

const totalXp = computed(() =>
  sessions.value.reduce((sum, s) => sum + sessionTotalXp(s), 0)
);

const totalGold = computed(() =>
  sessions.value.reduce((sum, s) => sum + s.vendor_gold, 0)
);

function formatDate(isoStr: string): string {
  return formatDateTimeShort(isoStr)
}

onMounted(loadSessions);
</script>
