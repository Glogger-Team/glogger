<template>
  <div class="flex flex-col gap-2 text-sm h-full min-h-0">
    <!-- Self-milked counter (cow form self-milking) -->
    <div v-if="selfMilkedCount > 0" class="flex items-center justify-between px-2 shrink-0">
      <span class="text-xs text-text-muted">Self-milked</span>
      <span class="text-xs font-mono text-accent-gold">{{ selfMilkedCount }}</span>
    </div>
    <div v-else-if="scanningHistory" class="px-2 shrink-0 text-xs text-text-dim italic">
      Scanning chat history...
    </div>
    <div
      v-else-if="historyError"
      class="px-2 shrink-0 text-xs text-accent-red italic truncate"
      :title="historyError">
      History import failed — check chat logs path
    </div>

    <!-- Tab bar -->
    <div class="flex gap-1 shrink-0">
      <button
        v-for="tab in tabs"
        :key="tab.key"
        class="px-2 py-1 text-xs rounded transition-colors cursor-pointer"
        :class="activeTab === tab.key
          ? 'bg-accent-gold/20 text-accent-gold border border-accent-gold/40'
          : 'text-text-muted hover:text-text-primary hover:bg-surface-elevated'"
        @click="activeTab = tab.key">
        {{ tab.label }}
      </button>
    </div>

    <!-- Players Milked tab -->
    <div v-if="activeTab === 'milked'" class="flex-1 overflow-y-auto min-h-0">
      <div v-if="milkedLeaderboard.length === 0" class="text-xs text-text-dim italic">
        No players milked yet. Milk a player-cow to start tracking.
      </div>
      <div v-else class="flex flex-col gap-0.5">
        <div
          v-for="(entry, i) in milkedLeaderboard"
          :key="entry.player_name"
          class="flex items-center justify-between gap-2 py-0.5 px-2">
          <div class="flex items-center gap-2 min-w-0">
            <span class="text-xs text-text-dim font-mono w-4 text-right shrink-0">{{ i + 1 }}</span>
            <span class="text-text-primary text-xs truncate">{{ entry.player_name }}</span>
          </div>
          <span class="text-xs font-mono text-accent-gold shrink-0">{{ entry.count }}</span>
        </div>
      </div>
    </div>

    <!-- Milked By tab -->
    <div v-else-if="activeTab === 'milked_by'" class="flex-1 overflow-y-auto min-h-0">
      <div v-if="milkedByLeaderboard.length === 0" class="text-xs text-text-dim italic">
        No one has milked you yet.
      </div>
      <div v-else class="flex flex-col gap-0.5">
        <div
          v-for="(entry, i) in milkedByLeaderboard"
          :key="entry.player_name"
          class="flex items-center justify-between gap-2 py-0.5 px-2">
          <div class="flex items-center gap-2 min-w-0">
            <span class="text-xs text-text-dim font-mono w-4 text-right shrink-0">{{ i + 1 }}</span>
            <span class="text-text-primary text-xs truncate">{{ entry.player_name }}</span>
          </div>
          <span class="text-xs font-mono text-accent-gold shrink-0">{{ entry.count }}</span>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, onMounted, onUnmounted } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { useSettingsStore } from '../../../stores/settingsStore'
import { scanning as scanningHistory, lastError as historyError, ensureAbilityHistory } from '../../../composables/useAbilityHistoryBackfill'

interface PlayerMilkingEntry {
  player_name: string
  count: number
}

const tabs = [
  { key: 'milked', label: 'Players Milked' },
  { key: 'milked_by', label: 'Milked By' },
] as const

type TabKey = typeof tabs[number]['key']

const settings = useSettingsStore()
const activeTab = ref<TabKey>('milked')
const milkedLeaderboard = ref<PlayerMilkingEntry[]>([])
const milkedByLeaderboard = ref<PlayerMilkingEntry[]>([])
const selfMilkedCount = ref(0)

let unlisten: UnlistenFn | null = null


async function loadLeaderboards() {
  const char = settings.settings.activeCharacterName
  const server = settings.settings.activeServerName
  if (!char || !server) return

  try {
    const [milked, milkedBy, selfMilked] = await Promise.all([
      invoke<PlayerMilkingEntry[]>('get_player_milking_leaderboard', {
        characterName: char,
        serverName: server,
        direction: 'milked',
      }),
      invoke<PlayerMilkingEntry[]>('get_player_milking_leaderboard', {
        characterName: char,
        serverName: server,
        direction: 'milked_by',
      }),
      invoke<PlayerMilkingEntry[]>('get_player_milking_leaderboard', {
        characterName: char,
        serverName: server,
        direction: 'self_milked',
      }),
    ])
    milkedLeaderboard.value = milked
    milkedByLeaderboard.value = milkedBy
    selfMilkedCount.value = selfMilked.reduce((sum, e) => sum + e.count, 0)

    // Self-milks only ever appear as excluded-Combat chat lines, so the archived
    // Chat-*.log history has to be imported before the counter can show
    // anything. Shared with the Character → Stats card: one scan per session,
    // never both. Not gated on a zero count — a milk that lands before the first
    // scan must not block the rest of the archive.
    if (char) {
      await ensureAbilityHistory(char, server)
      // The backfill records milked/milked_by rows silently (no
      // game-state-updated emit), so re-read all three lists, not just
      // the counter — otherwise the tabs stay empty until the next live event.
      const refreshed = await Promise.all([
        invoke<PlayerMilkingEntry[]>('get_player_milking_leaderboard', {
          characterName: char,
          serverName: server,
          direction: 'milked',
        }),
        invoke<PlayerMilkingEntry[]>('get_player_milking_leaderboard', {
          characterName: char,
          serverName: server,
          direction: 'milked_by',
        }),
        invoke<PlayerMilkingEntry[]>('get_player_milking_leaderboard', {
          characterName: char,
          serverName: server,
          direction: 'self_milked',
        }),
      ])
      milkedLeaderboard.value = refreshed[0]
      milkedByLeaderboard.value = refreshed[1]
      selfMilkedCount.value = refreshed[2].reduce((sum, e) => sum + e.count, 0)
    }
  } catch (e) {
    console.error('Failed to load milking leaderboards:', e)
  }
}

onMounted(async () => {
  await loadLeaderboards()

  unlisten = await listen<string[]>('game-state-updated', (event) => {
    if (event.payload.includes('milking')) {
      loadLeaderboards()
    }
  })
})

onUnmounted(() => {
  if (unlisten) unlisten()
})
</script>
