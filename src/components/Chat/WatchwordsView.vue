<template>
  <div class="flex h-full">
    <!-- Left Sidebar: Watch Rules list + CRUD -->
    <div class="w-72 bg-surface-dark border-r border-border-default flex flex-col">
      <div class="p-4 border-b border-border-default flex justify-between items-center">
        <h3 class="display-title m-0">Watch Rules</h3>
        <button
          class="px-3 py-1.5 bg-surface-elevated border border-border-light rounded text-text-secondary text-sm cursor-pointer hover:bg-surface-base hover:text-text-primary hover:border-border-hover transition-all"
          @click="openNewRule"
        >+ Add</button>
      </div>
      <div class="flex-1 overflow-y-auto p-2">
        <div v-if="rules.length === 0" class="px-4 py-8 text-text-muted text-center text-sm">
          No watch rules yet. Click "+ Add" to create one.
        </div>
        <div
          v-for="rule in rules"
          :key="rule.id"
          class="mb-1 rounded transition-all cursor-pointer"
          :class="selectedRuleId === rule.id
            ? 'bg-surface-elevated'
            : 'hover:bg-surface-base'"
          @click="selectRule(rule)"
        >
          <div class="flex items-center justify-between px-3 py-2.5">
            <div class="flex items-center gap-2 min-w-0 flex-1">
              <span
                class="w-2 h-2 rounded-full flex-shrink-0"
                :class="rule.enabled ? 'bg-green-500' : 'bg-gray-600'"
              ></span>
              <span
                class="font-medium truncate"
                :class="selectedRuleId === rule.id ? 'text-accent-gold' : 'text-text-secondary'"
              >{{ rule.name }}</span>
            </div>
            <div class="flex gap-0.5 flex-shrink-0">
              <button
                class="bg-transparent border border-transparent text-text-muted cursor-pointer text-sm px-1.5 py-0.5 rounded hover:bg-surface-dark hover:border-border-light hover:text-text-primary transition-all"
                title="Edit"
                @click.stop="editRule(rule)"
              >&#9998;</button>
              <button
                class="bg-transparent border border-transparent text-text-muted cursor-pointer text-sm px-1.5 py-0.5 rounded hover:bg-red-900/30 hover:border-red-800 hover:text-red-400 transition-all"
                title="Delete"
                @click.stop="confirmDeleteRule(rule.id)"
              >&#10005;</button>
            </div>
          </div>
          <div class="flex flex-wrap gap-1 px-3 pb-2">
            <span
              v-for="(cond, i) in rule.conditions"
              :key="i"
              class="text-[0.7rem] px-1.5 py-0.5 rounded bg-surface-dark border border-border-default"
              :class="conditionColor(cond.type)"
            >{{ conditionShortLabel(cond) }}</span>
          </div>
        </div>
      </div>

      <!-- Delete confirmation -->
      <div v-if="deleteConfirmId !== null" class="p-3 border-t border-red-900/50 bg-red-900/10">
        <p class="m-0 mb-2 text-sm text-red-400">Delete this rule?</p>
        <div class="flex gap-2">
          <button
            class="px-3 py-1.5 bg-red-900/30 border border-red-800 rounded text-red-400 text-sm cursor-pointer hover:bg-red-900/50 transition-all"
            @click="deleteRule(deleteConfirmId!)"
          >Delete</button>
          <button
            class="px-3 py-1.5 bg-surface-elevated border border-border-light rounded text-text-secondary text-sm cursor-pointer hover:bg-surface-base transition-all"
            @click="deleteConfirmId = null"
          >Cancel</button>
        </div>
      </div>

      <!-- Auto-watch skillbooks (TODO #17): two modes, both write -->
      <!-- source:"auto_skillbook" rules via settingsStore -->
      <div class="p-3 border-t border-border-default">
        <p class="m-0 mb-1 text-sm font-semibold text-text-secondary">Auto-watch skillbooks</p>
        <p class="m-0 mb-2 text-xs text-text-muted">Watch chat item links for books you could learn but don't own or know.</p>
        <button
          class="w-full px-3 py-1.5 bg-surface-elevated border border-border-light rounded text-text-secondary text-sm cursor-pointer hover:bg-surface-base hover:text-text-primary transition-all disabled:opacity-50"
          :disabled="autoBusy"
          @click="watchTrainedSkills"
        >{{ autoBusy ? 'Working…' : 'Watch books for trained skills' }}</button>
        <button
          class="w-full mt-2 px-3 py-1.5 bg-transparent border border-border-light rounded text-text-muted text-sm cursor-pointer hover:bg-surface-base hover:text-text-primary transition-all"
          @click="toggleFuturePicker"
        >{{ showFuturePicker ? 'Hide future-skill picker' : 'Pick future skills…' }}</button>
        <div v-if="showFuturePicker" class="mt-2">
          <input
            v-model="skillFilter"
            placeholder="Filter skills…"
            class="w-full px-2 py-1 mb-2 bg-surface-base border border-border-light rounded text-text-primary text-sm"
          />
          <div class="max-h-40 overflow-y-auto border border-border-default rounded p-1">
            <label
              v-for="name in filteredSkillNames"
              :key="name"
              class="flex items-center gap-2 px-1.5 py-1 text-sm text-text-secondary cursor-pointer hover:bg-surface-base rounded"
            >
              <input type="checkbox" :value="name" v-model="selectedFutureSkills" />
              <span class="truncate">{{ name }}</span>
            </label>
          </div>
          <button
            class="w-full mt-2 px-3 py-1.5 bg-surface-elevated border border-border-light rounded text-text-secondary text-sm cursor-pointer hover:bg-surface-base hover:text-text-primary transition-all disabled:opacity-50"
            :disabled="autoBusy || selectedFutureSkills.length === 0"
            @click="watchFutureSkills"
          >{{ autoBusy ? 'Working…' : `Watch selected (${selectedFutureSkills.length})` }}</button>
        </div>
        <p v-if="autoStatus" class="m-0 mt-2 text-xs text-green-400">{{ autoStatus }}</p>
        <p v-if="autoError" class="m-0 mt-2 text-xs text-red-400">{{ autoError }}</p>
      </div>
    </div>

    <!-- Right Content: Messages or Rule Editor -->
    <div class="flex-1 flex flex-col overflow-hidden">
      <!-- Rule Editor mode -->
      <div v-if="editing" class="flex-1 overflow-y-auto p-6">
        <h2 class="screen-title mt-0 mb-4">
          {{ editingId === null ? 'New Watch Rule' : 'Edit Watch Rule' }}
        </h2>

        <div class="mb-4">
          <label class="block text-text-secondary text-sm mb-1.5">Rule Name</label>
          <input
            v-model="form.name"
            type="text"
            class="w-full max-w-md px-3 py-2 bg-surface-elevated border border-border-light rounded text-text-primary focus:outline-none focus:border-accent-gold"
            placeholder="e.g., Flamestrike deals"
          />
        </div>

        <div class="mb-4">
          <label class="block text-text-secondary text-sm mb-1.5">Channels</label>
          <label class="flex items-center gap-2 cursor-pointer text-text-primary mb-2">
            <input type="checkbox" :checked="form.allChannels" @change="form.allChannels = !form.allChannels" class="w-4 h-4" />
            <span>All channels</span>
          </label>
          <div v-if="!form.allChannels" class="flex flex-wrap gap-x-4 gap-y-2 p-3 bg-surface-dark border border-border-default rounded">
            <label
              v-for="ch in knownChannels"
              :key="ch"
              class="flex items-center gap-1.5 cursor-pointer text-text-secondary text-sm min-w-24"
            >
              <input type="checkbox" :checked="form.channels.includes(ch)" @change="toggleChannel(ch)" class="w-4 h-4" />
              <span>{{ ch }}</span>
            </label>
          </div>
        </div>

        <div class="mb-4">
          <label class="block text-text-secondary text-sm mb-1.5">Match Mode</label>
          <div class="flex gap-1 bg-surface-dark border border-border-default rounded p-0.5 w-fit">
            <button
              class="px-3 py-1.5 rounded text-sm cursor-pointer transition-all border"
              :class="form.matchMode === 'Any'
                ? 'bg-surface-elevated border-accent-gold/50 text-accent-gold'
                : 'bg-transparent border-transparent text-text-muted hover:text-text-secondary'"
              @click="form.matchMode = 'Any'"
            >Any match (OR)</button>
            <button
              class="px-3 py-1.5 rounded text-sm cursor-pointer transition-all border"
              :class="form.matchMode === 'All'
                ? 'bg-surface-elevated border-accent-gold/50 text-accent-gold'
                : 'bg-transparent border-transparent text-text-muted hover:text-text-secondary'"
              @click="form.matchMode = 'All'"
            >All match (AND)</button>
          </div>
        </div>

        <div class="mb-4">
          <label class="block text-text-secondary text-sm mb-1.5">Conditions <span class="text-text-muted">({{ form.matchMode === 'Any' ? 'any one must match' : 'all must match' }})</span></label>
          <div
            v-for="(cond, i) in form.conditions"
            :key="i"
            class="flex gap-2 items-center mb-2"
          >
            <select
              v-model="cond.type"
              class="px-3 py-2 bg-surface-elevated border border-border-light rounded text-text-primary text-sm min-w-40 focus:outline-none focus:border-accent-gold"
            >
              <option value="ContainsText">Contains Text</option>
              <option value="ContainsItemLink">Contains Item Link</option>
              <option value="FromSender">From Sender</option>
            </select>
            <input
              v-model="cond.value"
              type="text"
              class="flex-1 max-w-xs px-3 py-2 bg-surface-elevated border border-border-light rounded text-text-primary focus:outline-none focus:border-accent-gold"
              :placeholder="conditionPlaceholder(cond.type)"
            />
            <button
              class="bg-transparent border border-transparent text-text-muted cursor-pointer px-2 py-1 rounded hover:bg-red-900/30 hover:border-red-800 hover:text-red-400 transition-all"
              @click="form.conditions.splice(i, 1)"
            >&#10005;</button>
          </div>
          <button
            class="px-3 py-1.5 bg-surface-elevated border border-border-light rounded text-text-secondary text-sm cursor-pointer hover:bg-surface-base hover:text-text-primary transition-all"
            @click="addCondition"
          >+ Add Condition</button>
        </div>

        <div class="mb-6">
          <label class="block text-text-secondary text-sm mb-1.5">Notification Options</label>
          <div class="flex flex-col gap-2">
            <label class="flex items-center gap-2 cursor-pointer text-text-primary">
              <input type="checkbox" v-model="form.highlight" class="w-4 h-4" />
              <span>Highlight in chat</span>
            </label>
            <label class="flex items-center gap-2 cursor-pointer text-text-primary">
              <input type="checkbox" v-model="form.toast" class="w-4 h-4" />
              <span>Toast notification</span>
            </label>
            <label class="flex items-center gap-2 cursor-pointer text-text-primary">
              <input type="checkbox" v-model="form.sound" class="w-4 h-4" />
              <span>Play sound</span>
            </label>
          </div>
        </div>

        <div class="flex gap-2">
          <button
            class="px-4 py-2 bg-surface-elevated border border-accent-gold/50 rounded text-accent-gold cursor-pointer font-medium hover:bg-accent-gold/10 transition-all disabled:opacity-40 disabled:cursor-default"
            :disabled="!isFormValid"
            @click="saveRule"
          >{{ editingId === null ? 'Create Rule' : 'Save Changes' }}</button>
          <button
            class="px-4 py-2 bg-surface-elevated border border-border-light rounded text-text-secondary cursor-pointer hover:bg-surface-base hover:text-text-primary transition-all"
            @click="cancelEdit"
          >Cancel</button>
        </div>
      </div>

      <!-- Message view mode -->
      <template v-else>
        <div class="px-6 py-4 border-b border-border-default flex justify-between items-center bg-surface-base">
          <h2 class="screen-title m-0">
            {{ selectedRule ? selectedRule.name : 'Select a Watch Rule' }}
          </h2>
          <button
            v-if="selectedRule"
            @click="refreshMessages"
            :disabled="loading"
            class="w-9 h-9 p-0 bg-surface-elevated border border-border-light rounded text-text-primary text-xl cursor-pointer transition-all flex items-center justify-center hover:bg-border-default hover:border-border-hover disabled:opacity-50 disabled:cursor-not-allowed"
            title="Refresh"
          >⟳</button>
        </div>
        <ChatMessageList
          :messages="messages"
          :loading="loading"
          :has-more="hasMore"
          show-channel
          @load-more="loadMore"
        />
      </template>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, watch } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { useSettingsStore } from '../../stores/settingsStore'
import { pendingWatchwordRuleId } from '../../composables/useViewNavigation'
import type { WatchRule, WatchCondition, ConditionMatch, ChatMessage } from '../../types/database'
import ChatMessageList from './ChatMessageList.vue'
import { useChatRequestGuard } from '../../composables/useChatRequestGuard'
import { useAutoSkillbookWatch } from '../../composables/useAutoSkillbookWatch'

const settingsStore = useSettingsStore()
const reqGuard = useChatRequestGuard()

const knownChannels = ['Global', 'Trade', 'Guild', 'Party', 'Nearby', 'Tell', 'Help']

// ── Message display state ───────────────────────────────────────

const selectedRuleId = ref<number | null>(null)
const messages = ref<ChatMessage[]>([])
const loading = ref(false)
const hasMore = ref(true)
const offset = ref(0)
const LIMIT = 100

const rules = computed(() => settingsStore.settings.watchRules)
const selectedRule = computed(() =>
  rules.value.find((r) => r.id === selectedRuleId.value) ?? null
)

async function selectRule(rule: WatchRule) {
  if (editing.value) return
  selectedRuleId.value = rule.id
  messages.value = []
  offset.value = 0
  hasMore.value = true
  await loadMessages()
}

async function loadMessages() {
  if (!selectedRuleId.value) return

  loading.value = true
  const generation = reqGuard.begin()
  try {
    const newMessages = await invoke<ChatMessage[]>('get_watch_rule_messages', {
      ruleId: selectedRuleId.value,
      limit: LIMIT,
      offset: offset.value,
    })

    if (!reqGuard.isCurrent(generation)) return

    if (offset.value === 0) {
      messages.value = newMessages
    } else {
      messages.value = [...messages.value, ...newMessages]
    }

    hasMore.value = newMessages.length === LIMIT
    offset.value += newMessages.length
  } catch (e) {
    console.error('Failed to load watch rule messages:', e)
  } finally {
    if (reqGuard.isCurrent(generation)) loading.value = false
  }
}

function loadMore() {
  if (loading.value) return
  loadMessages()
}

function refreshMessages() {
  offset.value = 0
  hasMore.value = true
  loadMessages()
}

// ── Rule CRUD state ─────────────────────────────────────────────

const editing = ref(false)
const editingId = ref<number | null>(null)
const deleteConfirmId = ref<number | null>(null)

const form = ref({
  name: '',
  allChannels: true,
  channels: [] as string[],
  matchMode: 'Any' as ConditionMatch,
  conditions: [] as { type: WatchCondition['type']; value: string }[],
  highlight: true,
  toast: true,
  sound: false,
})

const isFormValid = computed(() =>
  form.value.name.trim().length > 0 &&
  form.value.conditions.length > 0 &&
  form.value.conditions.every((c) => c.value.trim().length > 0)
)

function openNewRule() {
  editingId.value = null
  form.value = {
    name: '',
    allChannels: true,
    channels: [],
    matchMode: 'Any',
    conditions: [{ type: 'ContainsText', value: '' }],
    highlight: true,
    toast: true,
    sound: false,
  }
  editing.value = true
}

function editRule(rule: WatchRule) {
  editingId.value = rule.id
  form.value = {
    name: rule.name,
    allChannels: rule.channels === null,
    channels: rule.channels ? [...rule.channels] : [],
    matchMode: rule.match_mode ?? 'Any',
    conditions: rule.conditions.map((c) => ({ type: c.type, value: c.value })),
    highlight: rule.notify.highlight,
    toast: rule.notify.toast,
    sound: rule.notify.sound,
  }
  editing.value = true
}

function cancelEdit() {
  editing.value = false
  editingId.value = null
}

function addCondition() {
  form.value.conditions.push({ type: 'ContainsText', value: '' })
}

function toggleChannel(ch: string) {
  const idx = form.value.channels.indexOf(ch)
  if (idx >= 0) {
    form.value.channels.splice(idx, 1)
  } else {
    form.value.channels.push(ch)
  }
}

function saveRule() {
  const existingSource =
    editingId.value !== null
      ? (rules.value.find((r) => r.id === editingId.value)?.source ?? null)
      : null
  const rule: WatchRule = {
    id: editingId.value ?? Date.now(),
    name: form.value.name.trim(),
    enabled: true,
    channels: form.value.allChannels ? null : [...form.value.channels],
    match_mode: form.value.matchMode,
    conditions: form.value.conditions
      .filter((c) => c.value.trim().length > 0)
      .map((c) => ({ type: c.type, value: c.value.trim() })),
    notify: {
      highlight: form.value.highlight,
      toast: form.value.toast,
      sound: form.value.sound,
    },
    source: existingSource,
  }

  let updated: WatchRule[]
  if (editingId.value !== null) {
    updated = rules.value.map((r) => (r.id === editingId.value ? rule : r))
  } else {
    updated = [...rules.value, rule]
  }

  settingsStore.updateSettings({ watchRules: updated })
  editing.value = false
  editingId.value = null

  // Select and load the saved rule
  selectedRuleId.value = rule.id
  offset.value = 0
  hasMore.value = true
  loadMessages()
}

function confirmDeleteRule(id: number) {
  deleteConfirmId.value = id
}

function deleteRule(id: number) {
  const updated = rules.value.filter((r) => r.id !== id)
  settingsStore.updateSettings({ watchRules: updated })
  deleteConfirmId.value = null

  if (selectedRuleId.value === id) {
    selectedRuleId.value = null
    messages.value = []
  }
}

// ── Auto-watch skillbooks (TODO #17) ────────────────────────────────────
// Both modes compose the landed pieces via useAutoSkillbookWatch and write
// through the existing settings watchRules path; manual rules are untouched.

const autoWatch = useAutoSkillbookWatch()
const autoBusy = ref(false)
const autoStatus = ref('')
const autoError = ref('')
const showFuturePicker = ref(false)
const allSkillNames = ref<string[]>([])
const skillFilter = ref('')
const selectedFutureSkills = ref<string[]>([...autoWatch.prefs.value.futureSkills])

const filteredSkillNames = computed(() => {
  const q = skillFilter.value.trim().toLowerCase()
  if (!q) return allSkillNames.value
  return allSkillNames.value.filter((n) => n.toLowerCase().includes(q))
})

watch(selectedFutureSkills, (skills) => {
  autoWatch.setFutureSkills([...skills])
})

async function toggleFuturePicker() {
  showFuturePicker.value = !showFuturePicker.value
  if (showFuturePicker.value && allSkillNames.value.length === 0) {
    autoBusy.value = true
    autoError.value = ''
    try {
      allSkillNames.value = await autoWatch.loadAllSkillNames()
    } catch (e) {
      autoError.value = String(e)
    } finally {
      autoBusy.value = false
    }
  }
}

async function watchTrainedSkills() {
  if (autoBusy.value) return
  autoBusy.value = true
  autoStatus.value = ''
  autoError.value = ''
  try {
    const skills = await autoWatch.loadTrainedSkills()
    if (skills.length === 0) {
      autoStatus.value = 'No trained skills found for the active character.'
      return
    }
    const created = await autoWatch.suggestFor(skills)
    // C1 remove step: suggest-only — ephemeral session set lands in C2.
    autoStatus.value = created.length === 0
      ? 'Nothing new — all trained-skill books are known, owned, or already watched.'
      : `Found ${created.length} learnable book${created.length === 1 ? '' : 's'} (session watch lands next).`
  } catch (e) {
    autoError.value = e instanceof Error ? e.message : String(e)
  } finally {
    autoBusy.value = false
  }
}

async function watchFutureSkills() {
  if (autoBusy.value) return
  autoBusy.value = true
  autoStatus.value = ''
  autoError.value = ''
  try {
    const created = await autoWatch.suggestFor(selectedFutureSkills.value)
    // C1 remove step: suggest-only — ephemeral session set lands in C2.
    autoStatus.value = created.length === 0
      ? 'Nothing new — the selected skills\u2019 books are known, owned, or already watched.'
      : `Found ${created.length} learnable book${created.length === 1 ? '' : 's'} (session watch lands next).`
  } catch (e) {
    autoError.value = e instanceof Error ? e.message : String(e)
  } finally {
    autoBusy.value = false
  }
}

// ── Display helpers ─────────────────────────────────────────────

function conditionShortLabel(cond: WatchCondition): string {
  switch (cond.type) {
    case 'ContainsText': return `"${cond.value}"`
    case 'ContainsItemLink': return `Item: ${cond.value}`
    case 'FromSender': return `From: ${cond.value}`
  }
}

function conditionColor(type: WatchCondition['type']): string {
  switch (type) {
    case 'ContainsText': return 'text-blue-300/70'
    case 'ContainsItemLink': return 'text-green-300/70'
    case 'FromSender': return 'text-amber-300/70'
  }
}

function conditionPlaceholder(type: WatchCondition['type']): string {
  switch (type) {
    case 'ContainsText': return 'e.g., flamestrike'
    case 'ContainsItemLink': return 'e.g., Strange Dirt'
    case 'FromSender': return 'e.g., TraderJoe'
  }
}

// ── External navigation (from dashboard widget) ────────────────
watch(pendingWatchwordRuleId, (ruleId) => {
  if (ruleId == null) return
  const rule = rules.value.find(r => r.id === ruleId)
  if (rule) {
    selectRule(rule)
  }
  pendingWatchwordRuleId.value = null
}, { immediate: true })
</script>
