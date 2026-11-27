<template>
  <div class="flex flex-col gap-4">
    <TimeRangeSelector @change="onRangeChange" />

    <div v-if="loading" class="text-xs text-text-dim italic">Reading the window…</div>
    <div v-else-if="error" class="text-[#c87e7e] text-sm">{{ error }}</div>
    <template v-else>
      <!-- Summary -->
      <div class="flex gap-6 flex-wrap text-center">
        <div>
          <div class="text-[0.65rem] text-text-muted uppercase tracking-wide">Kills</div>
          <div class="text-lg font-bold text-text-primary">{{ totalKills.toLocaleString() }}</div>
        </div>
        <div>
          <div class="text-[0.65rem] text-text-muted uppercase tracking-wide">Mobs</div>
          <div class="text-lg font-bold text-text-primary">{{ kills.length }}</div>
        </div>
        <div>
          <div class="text-[0.65rem] text-text-muted uppercase tracking-wide">Loot gained</div>
          <div class="text-lg font-bold text-[#7ec87e]">{{ totalLoot.toLocaleString() }}</div>
        </div>
        <div>
          <div class="text-[0.65rem] text-text-muted uppercase tracking-wide">XP</div>
          <div class="text-lg font-bold text-[#7ec87e]">{{ totalXp.toLocaleString() }}</div>
        </div>
      </div>

      <EmptyState v-if="totalKills === 0 && totalLoot === 0 && totalXp === 0" variant="panel"
        primary="Nothing in this window"
        secondary="No kills, loot, or XP were recorded between these timestamps." />

      <template v-else>
        <div v-if="kills.length > 0" class="flex flex-col gap-1">
          <h3 class="section-heading">Kills by enemy</h3>
          <DataTable :columns="killColumns" :rows="(kills as unknown as Record<string, unknown>[])" :row-key="(row) => String(row.enemy_name)" compact size="sm">
            <template #cell-enemy_name="{ row }">
              <EnemyInline :reference="String(row.enemy_name)" />
            </template>
          </DataTable>
        </div>

        <div v-if="loot.length > 0" class="flex flex-col gap-1">
          <h3 class="section-heading">Loot gained</h3>
          <DataTable :columns="lootColumns" :rows="(loot as unknown as Record<string, unknown>[])" :row-key="(row) => String(row.item_name)" compact size="sm">
            <template #cell-item_name="{ row }">
              <ItemInline :reference="String(row.item_name)" />
            </template>
          </DataTable>
        </div>

        <div v-if="xp.length > 0" class="flex flex-col gap-1">
          <h3 class="section-heading">XP by skill</h3>
          <DataTable :columns="xpColumns" :rows="(xp as unknown as Record<string, unknown>[])" :row-key="(row) => String(row.skill_name)" compact size="sm">
            <template #cell-skill_name="{ row }">
              <SkillInline :reference="String(row.skill_name)" />
            </template>
          </DataTable>
        </div>
      </template>
    </template>
  </div>
</template>

<script setup lang="ts">
import { ref, computed } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import TimeRangeSelector, { type TimeRange } from './TimeRangeSelector.vue'
import DataTable, { type ColumnDef } from './DataTable.vue'
import EmptyState from './EmptyState.vue'
import EnemyInline from './Enemy/EnemyInline.vue'
import SkillInline from './Skill/SkillInline.vue'
import ItemInline from './Item/ItemInline.vue'
import { useSettingsStore } from '../../stores/settingsStore'
import type { FarmingItemRecord, FarmingKillRecord, FarmingSkillRecord } from '../../types/farming.specta'

const killColumns: ColumnDef[] = [
  { key: 'enemy_name', label: 'Enemy' },
  { key: 'kill_count', label: 'Kills', align: 'right' },
]
const lootColumns: ColumnDef[] = [
  { key: 'item_name', label: 'Item' },
  { key: 'net_quantity', label: 'Gained', align: 'right' },
]
const xpColumns: ColumnDef[] = [
  { key: 'skill_name', label: 'Skill' },
  { key: 'xp_gained', label: 'Gained', align: 'right' },
]

const kills = ref<FarmingKillRecord[]>([])
const loot = ref<FarmingItemRecord[]>([])
const xp = ref<FarmingSkillRecord[]>([])
const loading = ref(false)
const error = ref<string | null>(null)
const loadedRange = ref<TimeRange | null>(null)

const totalKills = computed(() => kills.value.reduce((s, k) => s + k.kill_count, 0))
const totalLoot = computed(() => loot.value.reduce((s, l) => s + l.net_quantity, 0))
const totalXp = computed(() => xp.value.reduce((s, x) => s + x.xp_gained, 0))

async function onRangeChange(range: TimeRange) {
  loadedRange.value = range
  if (!range.start || !range.end) return
  loading.value = true
  error.value = null
  try {
    const settings = useSettingsStore()
    const args = {
      startTs: range.start,
      endTs: range.end,
      characterName: settings.settings.activeCharacterName ?? null,
      serverName: settings.settings.activeServerName ?? null,
    }
    const [k, l, x] = await Promise.all([
      invoke<FarmingKillRecord[]>('get_timeboxed_kills', args),
      invoke<FarmingItemRecord[]>('get_timeboxed_loot', args),
      invoke<FarmingSkillRecord[]>('get_timeboxed_xp', args),
    ])
    // Rapid preset switches: drop a stale response that no longer matches.
    if (loadedRange.value !== range) return
    kills.value = k
    loot.value = l
    xp.value = x
  } catch (e: any) {
    error.value = String(e)
  } finally {
    if (loadedRange.value === range) loading.value = false
  }
}
</script>
