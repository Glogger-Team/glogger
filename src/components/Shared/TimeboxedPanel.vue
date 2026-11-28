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
import { ref, computed, watch, onMounted, onBeforeUnmount } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import TimeRangeSelector, { type TimeRange } from './TimeRangeSelector.vue'
import DataTable, { type ColumnDef } from './DataTable.vue'
import EmptyState from './EmptyState.vue'
import EnemyInline from './Enemy/EnemyInline.vue'
import SkillInline from './Skill/SkillInline.vue'
import ItemInline from './Item/ItemInline.vue'
import { useSettingsStore } from '../../stores/settingsStore'
import { isReplayChatStatusEvent, type ChatStatusEvent, type ReplayChatStatusEnvelope } from '../../stores/gameStateStore'
import type { FarmingItemRecord, FarmingKillRecord, FarmingSkillRecord } from '../../types/farming.specta'

// Inactive (v-show-hidden) instances skip listener-triggered reloads and
// reload once on flip back to active — same pattern as CouncilWealthView.
const props = defineProps<{ active?: boolean }>()
const isActive = computed(() => props.active !== false)

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

function identityOf() {
  const settings = useSettingsStore()
  const character = settings.settings.activeCharacterName
  const server = settings.settings.activeServerName
  if (!character || !server) return null
  return { character, server }
}

// Monotonic reload sequence (CouncilWealthView precedent): a range/identity
// switch mid-fetch must not let the older response plant or clear `loading`
// while the newer fetch is still in flight.
let reloadSeq = 0
let inactiveDirty = false

async function reload() {
  const range = loadedRange.value
  const seq = ++reloadSeq
  if (!range || !range.start || !range.end) return
  const id = identityOf()
  loading.value = true
  error.value = null
  try {
    const args = {
      startTs: range.start,
      endTs: range.end,
      characterName: id?.character ?? null,
      serverName: id?.server ?? null,
    }
    const [k, l, x] = await Promise.all([
      invoke<FarmingKillRecord[]>('get_timeboxed_kills', args),
      invoke<FarmingItemRecord[]>('get_timeboxed_loot', args),
      invoke<FarmingSkillRecord[]>('get_timeboxed_xp', args),
    ])
    if (seq !== reloadSeq) return
    // Rapid preset/character switches: drop a stale response that no longer
    // matches (same guard shape as the arena tips fetch).
    if (loadedRange.value !== range) return
    const now = identityOf()
    if ((now?.character ?? null) !== (id?.character ?? null) || (now?.server ?? null) !== (id?.server ?? null)) return
    kills.value = k
    loot.value = l
    xp.value = x
  } catch (e: any) {
    if (seq !== reloadSeq) return
    error.value = String(e)
  } finally {
    if (seq === reloadSeq) loading.value = false
  }
}

function requestReload() {
  if (!isActive.value) {
    inactiveDirty = true
    return
  }
  void reload()
}

function onRangeChange(range: TimeRange) {
  loadedRange.value = range
  if (!range.start || !range.end) return
  void reload()
}

watch(
  () => {
    const id = identityOf()
    return id ? `${id.character}@${id.server}` : ''
  },
  () => {
    // Clear synchronously: the previous identity's rows must not render
    // under the new identity while the fresh fetch is in flight.
    kills.value = []
    loot.value = []
    xp.value = []
    requestReload()
  },
)

watch(isActive, (active) => {
  if (active && inactiveDirty) {
    inactiveDirty = false
    void reload()
  }
})

let unlistenKill: UnlistenFn | null = null
let unlistenStatus: UnlistenFn | null = null

onMounted(async () => {
  // Live refresh: keep-alive removed the remount-refetch, so new kills, loot
  // gains, and XP awards must trigger a reload explicitly. Replay preview
  // re-emits on the live channel tagged `is_replay` — never reload from it.
  unlistenKill = await listen('enemy-killed', () => {
    requestReload()
  })
  unlistenStatus = await listen<ChatStatusEvent | ReplayChatStatusEnvelope>('chat-status-event', (e) => {
    if (isReplayChatStatusEvent(e.payload)) return
    const kind = (e.payload as { kind: string }).kind
    if (kind === 'ItemGained' || kind === 'XpGained' || kind === 'ProdigyXpGained' || kind === 'LevelUp') {
      requestReload()
    }
  })
})

onBeforeUnmount(() => {
  unlistenKill?.()
  unlistenStatus?.()
})
</script>
