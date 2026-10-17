<template>
  <div class="space-y-4">
    <div class="flex items-center justify-between">
      <h2 class="text-accent-gold text-lg m-0">
        Server Overview — {{ serverName }}
      </h2>
      <button class="btn btn-secondary text-xs" @click="aggregateStore.loadAll()">
        Refresh
      </button>
    </div>

    <div v-if="aggregateStore.loading" class="space-y-4">
      <div class="grid grid-cols-3 gap-4">
        <SkeletonLoader v-for="i in 3" :key="i" variant="rect" height="h-16" />
      </div>
      <SkeletonLoader variant="rect" height="h-32" />
    </div>

    <template v-else-if="aggregateStore.wealth">
      <!-- Wealth Summary -->
      <div class="grid grid-cols-3 gap-4">
        <div class="card p-4">
          <div class="text-text-muted text-xs uppercase tracking-wide mb-1">Total Currencies</div>
          <div class="text-accent-gold text-xl font-bold">{{ Math.round(aggregateStore.wealth.total_currency).toLocaleString() }}g</div>
        </div>
        <div class="card p-4">
          <div class="text-text-muted text-xs uppercase tracking-wide mb-1">Inventory Market Value</div>
          <div class="text-accent-green text-xl font-bold">{{ aggregateStore.wealth.total_market_value.toLocaleString() }}g</div>
        </div>
        <div class="card p-4">
          <div class="text-text-muted text-xs uppercase tracking-wide mb-1">Grand Total</div>
          <div class="text-text-primary text-xl font-bold">{{ aggregateStore.wealth.grand_total.toLocaleString() }}g</div>
        </div>
      </div>

      <!-- Per-Character Wealth Breakdown -->
      <div v-if="aggregateStore.wealth.per_character.length > 1" class="card p-4">
        <h3 class="text-text-secondary text-sm font-bold uppercase tracking-wide mb-3 mt-0">Wealth by Character</h3>
        <DataTable
          :columns="wealthColumns"
          :rows="(aggregateStore.wealth.per_character as unknown as Record<string, unknown>[])"
          compact
          :hoverable="false">
          <template #cell-character_name="{ row }">
            <span class="text-text-primary">{{ row.character_name }}</span>
          </template>
          <template #cell-currency_total="{ row }">
            <span class="text-accent-gold">{{ Math.round(row.currency_total as number).toLocaleString() }}g</span>
          </template>
          <template #cell-market_value_total="{ row }">
            <span class="text-accent-green">{{ (row.market_value_total as number).toLocaleString() }}g</span>
          </template>
          <template #cell-total="{ row }">
            <span class="text-text-primary font-bold">{{ (Math.round(row.currency_total as number) + (row.market_value_total as number)).toLocaleString() }}g</span>
          </template>
        </DataTable>
      </div>

      <!-- Aggregate Inventory -->
      <div class="card p-4">
        <div class="flex items-center justify-between mb-3">
          <h3 class="text-text-secondary text-sm font-bold uppercase tracking-wide m-0">Combined Inventory</h3>
          <input
            v-model="invSearch"
            placeholder="Search items..."
            class="input text-sm w-48" />
        </div>

        <EmptyState
          v-if="filteredInventory.length === 0"
          variant="compact"
          :primary="invSearch ? 'No matching items' : 'No inventory data across characters'" />

        <DataTable
          v-else
          :columns="inventoryColumns"
          :rows="(inventoryRows as unknown as Record<string, unknown>[])"
          compact
          :hoverable="false">
          <template #cell-item_name="{ row }">
            <span class="text-text-primary">{{ row.item_name }}</span>
          </template>
          <template #cell-total_stack_size="{ row }">
            <span class="text-accent-gold">{{ (row.total_stack_size as number).toLocaleString() }}</span>
          </template>
          <template #cell-character_count="{ row }">
            <span class="text-text-muted">{{ row.character_count }}</span>
          </template>
          <template #cell-breakdown="{ row }">
            <span class="text-text-muted">{{ row.breakdown_text }}</span>
          </template>
        </DataTable>

        <div v-if="filteredInventory.length > 0" class="text-text-muted text-xs mt-2">
          {{ filteredInventory.length }} of {{ aggregateStore.inventory.length }} items shown
        </div>
      </div>

      <!-- Aggregate Skills (collapsed by default) -->
      <div class="card p-4">
        <button
          class="flex items-center gap-2 w-full text-left bg-transparent border-none cursor-pointer p-0"
          @click="showSkills = !showSkills">
          <span class="text-text-secondary text-sm font-bold uppercase tracking-wide">Skills Across Characters</span>
          <span class="text-text-muted text-xs">{{ showSkills ? '\u25BC' : '\u25B6' }}</span>
        </button>

        <div v-if="showSkills" class="mt-3">
          <input
            v-model="skillSearch"
            placeholder="Search skills..."
            class="input text-sm w-48 mb-3" />

          <table class="w-full text-sm">
            <thead>
              <tr class="text-text-muted text-xs text-left border-b border-border-default">
                <th class="py-1.5 font-normal">Skill</th>
                <th v-for="name in skillCharacterNames" :key="name" class="py-1.5 font-normal text-center">
                  {{ name }}
                </th>
              </tr>
            </thead>
            <tbody>
              <tr
                v-for="skill in filteredSkills"
                :key="skill.skill_name"
                class="border-b border-border-default/30">
                <td class="py-1 text-text-primary">{{ skill.skill_name }}</td>
                <td
                  v-for="name in skillCharacterNames"
                  :key="name"
                  class="py-1 text-center"
                  :class="getSkillLevel(skill, name) > 0 ? 'text-accent-gold' : 'text-text-dim'">
                  {{ getSkillLevel(skill, name) || '-' }}
                </td>
              </tr>
            </tbody>
          </table>
        </div>
      </div>
    </template>

    <EmptyState v-else variant="panel" primary="No aggregate data available" secondary="Select a server and ensure characters have data." />
  </div>
</template>

<script setup lang="ts">
import { ref, computed, onMounted } from 'vue'
import { useAggregateStore, type AggregateSkillSummary } from '../../stores/aggregateStore'
import EmptyState from '../Shared/EmptyState.vue'
import SkeletonLoader from '../Shared/SkeletonLoader.vue'
import DataTable, { type ColumnDef } from '../Shared/DataTable.vue'

const aggregateStore = useAggregateStore()

const wealthColumns: ColumnDef[] = [
  { key: 'character_name', label: 'Character' },
  { key: 'currency_total', label: 'Currencies', align: 'right', numeric: true },
  { key: 'market_value_total', label: 'Inventory Value', align: 'right', numeric: true },
  { key: 'total', label: 'Total', align: 'right', numeric: true },
]

const inventoryColumns: ColumnDef[] = [
  { key: 'item_name', label: 'Item' },
  { key: 'total_stack_size', label: 'Total', align: 'right', numeric: true },
  { key: 'character_count', label: 'Characters', align: 'right' },
  { key: 'breakdown', label: 'Breakdown' },
]

const invSearch = ref('')
const skillSearch = ref('')
const showSkills = ref(false)

const serverName = computed(() => aggregateStore.serverName ?? 'Unknown')

const filteredInventory = computed(() => {
  if (!invSearch.value.trim()) return aggregateStore.inventory
  const q = invSearch.value.toLowerCase()
  return aggregateStore.inventory.filter(i => i.item_name.toLowerCase().includes(q))
})

const inventoryRows = computed(() =>
  filteredInventory.value.map(i => ({
    ...i,
    breakdown_text: i.characters.map(c => `${c.character_name}: ${c.stack_size}`).join(', '),
  })),
)

const skillCharacterNames = computed(() => {
  const names = new Set<string>()
  for (const skill of aggregateStore.skills) {
    for (const entry of skill.characters) {
      names.add(entry.character_name)
    }
  }
  return [...names].sort()
})

const filteredSkills = computed(() => {
  if (!skillSearch.value.trim()) return aggregateStore.skills
  const q = skillSearch.value.toLowerCase()
  return aggregateStore.skills.filter(s => s.skill_name.toLowerCase().includes(q))
})

function getSkillLevel(skill: AggregateSkillSummary, characterName: string): number {
  return skill.characters.find(c => c.character_name === characterName)?.level ?? 0
}

onMounted(() => {
  aggregateStore.loadAll()
})
</script>
