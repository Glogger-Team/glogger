<template>
  <div>
    <div class="flex items-center justify-between mb-4">
      <h3 class="panel-label m-0">Game State Inspector</h3>
      <button class="btn btn-secondary text-xs" @click="gameState.loadAll()">
        Reload All
      </button>
    </div>

    <p class="text-text-muted text-xs mb-4">
      Live view of all game state domains stored in the database for
      <span class="text-accent-gold">{{ characterName || '(none)' }}</span> on
      <span class="text-accent-gold">{{ serverName || '(none)' }}</span>.
    </p>

    <!-- Session -->
    <AccordionSection :default-open="false">
      <template #title>Session</template>
      <template #badge>
        <span class="text-text-muted text-xs font-mono">
          {{ gameState.activeSkills ? 'active' : 'no data' }}
        </span>
      </template>
      <div class="space-y-1 text-xs font-mono">
        <div v-if="gameState.activeSkills">
          <span class="text-text-muted">Active Skills:</span>
          {{ gameState.activeSkills.skill1_name }} / {{ gameState.activeSkills.skill2_name }}
          <span class="text-text-muted ml-2">({{ gameState.activeSkills.last_confirmed_at }})</span>
        </div>
        <div v-if="gameState.world.weather">
          <span class="text-text-muted">Weather:</span>
          {{ gameState.world.weather.weather_name }}
          <span class="text-text-muted ml-2">(active: {{ gameState.world.weather.is_active }})</span>
        </div>
        <div v-if="gameState.world.combat">
          <span class="text-text-muted">Combat:</span>
          {{ gameState.world.combat.in_combat ? 'In combat' : 'Out of combat' }}
        </div>
        <div v-if="gameState.world.mount">
          <span class="text-text-muted">Mount:</span>
          {{ gameState.world.mount.is_mounted ? 'Mounted' : 'Not mounted' }}
        </div>
        <div v-if="!gameState.activeSkills && !gameState.world.weather && !gameState.world.combat && !gameState.world.mount"
          class="text-text-muted italic">No session data</div>
      </div>
    </AccordionSection>

    <!-- Skills -->
    <DomainTable
      title="Skills"
      :count="gameState.skills.length"
      :columns="['skill_name', 'level', 'base_level', 'bonus_levels', 'xp', 'tnl', 'max_level', 'source', 'last_confirmed_at']"
      :rows="gameState.skills"
    />

    <!-- Attributes -->
    <DomainTable
      title="Attributes"
      :count="gameState.attributes.length"
      :columns="['attribute_name', 'value', 'last_confirmed_at']"
      :rows="gameState.attributes"
    />

    <!-- Inventory -->
    <DomainTable
      title="Inventory"
      :count="gameState.inventory.length"
      :columns="['instance_id', 'item_name', 'item_type_id', 'stack_size', 'slot_index', 'source', 'last_confirmed_at']"
      :rows="gameState.inventory"
    />

    <!-- Equipment -->
    <DomainTable
      title="Equipment"
      :count="gameState.equipment.length"
      :columns="['slot', 'appearance_key', 'last_confirmed_at']"
      :rows="gameState.equipment"
    />

    <!-- Recipes -->
    <DomainTable
      title="Recipes"
      :count="gameState.recipes.length"
      :columns="['recipe_id', 'completion_count', 'source', 'last_confirmed_at']"
      :rows="gameState.recipes"
    />

    <!-- Favor -->
    <DomainTable
      title="Favor"
      :count="gameState.favor.length"
      :columns="['npc_key', 'npc_name', 'session_delta', 'favor_tier', 'source', 'last_confirmed_at']"
      :rows="gameState.favor"
    />

    <!-- Currencies -->
    <DomainTable
      title="Currencies"
      :count="gameState.currencies.length"
      :columns="['currency_name', 'amount', 'source', 'last_confirmed_at']"
      :rows="gameState.currencies"
    />

    <!-- Effects -->
    <DomainTable
      title="Effects"
      :count="gameState.effects.length"
      :columns="['effect_instance_id', 'effect_name', 'source_entity_id', 'last_confirmed_at']"
      :rows="gameState.effects"
    />

    <!-- Storage -->
    <DomainTable
      title="Storage"
      :count="gameState.storage.length"
      :columns="['vault_key', 'instance_id', 'item_name', 'item_type_id', 'stack_size', 'slot_index', 'source', 'last_confirmed_at']"
      :rows="gameState.storage"
    />

    <!-- Live Inventory (in-memory) -->
    <AccordionSection :default-open="false">
      <template #title>Live Inventory (in-memory)</template>
      <template #badge>
        <span class="text-text-muted text-xs font-mono">{{ gameState.liveItemCount }} items</span>
      </template>
      <div v-if="gameState.liveItemCount === 0" class="text-text-muted text-xs italic">No live inventory data</div>
      <div v-else class="overflow-x-auto">
        <table class="w-full text-xs font-mono border-collapse">
          <thead>
            <tr class="text-text-muted text-left">
              <th class="px-2 py-1 border-b border-border-default">instance_id</th>
              <th class="px-2 py-1 border-b border-border-default">item_name</th>
              <th class="px-2 py-1 border-b border-border-default">stack_size</th>
              <th class="px-2 py-1 border-b border-border-default">slot_index</th>
              <th class="px-2 py-1 border-b border-border-default">is_new</th>
            </tr>
          </thead>
          <tbody>
            <tr v-for="item in gameState.liveItems" :key="item.instance_id" class="hover:bg-surface-elevated/30">
              <td class="px-2 py-0.5 text-text-muted">{{ item.instance_id }}</td>
              <td class="px-2 py-0.5 text-text-primary">{{ item.item_name }}</td>
              <td class="px-2 py-0.5">{{ item.stack_size }}</td>
              <td class="px-2 py-0.5">{{ item.slot_index }}</td>
              <td class="px-2 py-0.5">{{ item.is_new }}</td>
            </tr>
          </tbody>
        </table>
      </div>
    </AccordionSection>

    <!-- Live Event Log -->
    <AccordionSection :default-open="false">
      <template #title>Live Event Log</template>
      <template #badge>
        <span class="text-text-muted text-xs font-mono">{{ gameState.liveEventLog.length }} events</span>
      </template>
      <div v-if="gameState.liveEventLog.length === 0" class="text-text-muted text-xs italic">No events yet</div>
      <div v-else class="overflow-x-auto max-h-60 overflow-y-auto">
        <table class="w-full text-xs font-mono border-collapse">
          <thead class="sticky top-0 bg-surface-base">
            <tr class="text-text-muted text-left">
              <th class="px-2 py-1 border-b border-border-default">time</th>
              <th class="px-2 py-1 border-b border-border-default">kind</th>
              <th class="px-2 py-1 border-b border-border-default">item</th>
              <th class="px-2 py-1 border-b border-border-default">detail</th>
            </tr>
          </thead>
          <tbody>
            <tr v-for="(evt, i) in gameState.liveEventLog" :key="i" class="hover:bg-surface-elevated/30">
              <td class="px-2 py-0.5 text-text-muted">{{ formatTs(evt.timestamp) }}</td>
              <td class="px-2 py-0.5" :class="eventKindColor(evt.kind)">{{ evt.kind }}</td>
              <td class="px-2 py-0.5 text-text-primary">{{ evt.item_name }}</td>
              <td class="px-2 py-0.5 text-text-muted">{{ evt.detail }}</td>
            </tr>
          </tbody>
        </table>
      </div>
    </AccordionSection>

    <!-- Session Skills (in-memory) -->
    <AccordionSection :default-open="false">
      <template #title>Session Skills (in-memory)</template>
      <template #badge>
        <span class="text-text-muted text-xs font-mono">{{ gameState.sessionSkillList.length }} skills</span>
      </template>
      <div v-if="gameState.sessionSkillList.length === 0" class="text-text-muted text-xs italic">No session skill data</div>
      <div v-else class="overflow-x-auto">
        <table class="w-full text-xs font-mono border-collapse">
          <thead>
            <tr class="text-text-muted text-left">
              <th class="px-2 py-1 border-b border-border-default">skill</th>
              <th class="px-2 py-1 border-b border-border-default">level</th>
              <th class="px-2 py-1 border-b border-border-default">xp_gained</th>
              <th class="px-2 py-1 border-b border-border-default">levels_gained</th>
              <th class="px-2 py-1 border-b border-border-default">xp/hr</th>
            </tr>
          </thead>
          <tbody>
            <tr v-for="s in gameState.sessionSkillList" :key="s.skillType" class="hover:bg-surface-elevated/30">
              <td class="px-2 py-0.5 text-text-primary">{{ s.skillType }}</td>
              <td class="px-2 py-0.5">{{ s.currentLevel }}</td>
              <td class="px-2 py-0.5">{{ s.xpGained.toLocaleString() }}</td>
              <td class="px-2 py-0.5">{{ s.levelsGained }}</td>
              <td class="px-2 py-0.5">{{ gameState.xpPerHour(s).toLocaleString() }}</td>
            </tr>
          </tbody>
        </table>
      </div>
    </AccordionSection>
  </div>
</template>

<script setup lang="ts">
import { computed } from "vue";
import { useGameStateStore } from "../../stores/gameStateStore";
import { useSettingsStore } from "../../stores/settingsStore";
import { formatAnyTimestamp as formatTs } from "../../composables/useTimestamp";
import AccordionSection from "../Shared/AccordionSection.vue";
import DomainTable from "./DomainTable.vue";
import type { InventoryEventKind } from "../../stores/gameStateStore";

const gameState = useGameStateStore();
const settingsStore = useSettingsStore();

const characterName = computed(() => settingsStore.settings.activeCharacterName);
const serverName = computed(() => settingsStore.settings.activeServerName);

function eventKindColor(kind: InventoryEventKind): string {
  switch (kind) {
    case 'added': return 'text-value-positive'
    case 'removed': return 'text-value-negative'
    case 'stack_changed': return 'text-yellow-400'
    default: return 'text-text-secondary'
  }
}
</script>
