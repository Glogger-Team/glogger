<template>
  <div class="flex flex-col h-full min-h-0">
    <!-- Experimental warning -->
    <div class="shrink-0 mb-3 p-2 rounded bg-status-warning/10 border border-status-warning/30 flex items-center gap-2 text-xs">
      <span class="text-status-warning font-semibold">Experimental</span>
      <span class="text-text-secondary">This feature is a work in progress with known bugs. The consolidation plan may not always be optimal and the wizard may not perfectly track all item movements.</span>
    </div>

    <!-- Cross-alt controls (always available) -->
    <div class="shrink-0 mb-3 flex items-center gap-3 flex-wrap text-xs">
      <label class="flex items-center gap-1.5 cursor-pointer select-none">
        <input type="checkbox" v-model="includeAlts" />
        <span class="text-text-secondary">Include all alts' storage</span>
      </label>
      <template v-if="includeAlts">
        <span v-if="consolidation.altsLoading.value" class="text-text-muted flex items-center gap-1">
          <span class="inline-block w-3 h-3 border-2 border-text-muted/40 border-t-text-muted rounded-full animate-spin" />
          Pooling characters…
        </span>
        <template v-else>
          <span class="text-text-muted">Gather onto</span>
          <select v-model="bankModel" class="bg-surface-elevated border border-border-default rounded px-2 py-1 text-xs text-text-primary cursor-pointer">
            <option v-for="c in consolidation.availableCharacters.value" :key="c.key" :value="c.key">{{ c.character }}</option>
          </select>
          <span class="text-text-muted">·</span>
          <select v-model="category" class="bg-surface-elevated border border-border-default rounded px-2 py-1 text-xs text-text-primary cursor-pointer max-w-48">
            <option value="">All categories</option>
            <option v-for="c in consolidation.availableCategories.value" :key="c.keyword" :value="c.keyword">{{ c.keyword }} ({{ c.count }})</option>
          </select>
          <span class="text-text-dim">· {{ consolidation.charactersInPlan.value }} character{{ consolidation.charactersInPlan.value === 1 ? '' : 's' }} pooled</span>
        </template>
      </template>
    </div>

    <!-- Empty states -->
    <EmptyState v-if="includeAlts && consolidation.altsLoading.value && consolidation.charactersInPlan.value === 0" variant="panel" primary="Loading all characters…" secondary="Pooling storage from every character on this server." />
    <EmptyState v-else-if="consolidation.charactersInPlan.value === 0" variant="panel" primary="No storage data" secondary="Import an inventory report to see stored items." />
    <EmptyState v-else-if="plan.moves.length === 0" variant="panel" primary="Nothing to consolidate" secondary="All your items are already in single locations." />

    <template v-else>
      <!-- Summary bar -->
      <div class="shrink-0 flex items-center gap-4 mb-3 flex-wrap">
        <div class="flex gap-5 text-sm">
          <div>
            <span class="text-accent-gold font-bold text-lg tabular-nums">{{ plan.slotsSaved }}</span>
            <span class="text-text-muted text-xs ml-1">slots saveable</span>
          </div>
          <div>
            <span class="text-text-primary font-bold text-lg tabular-nums">{{ plan.itemsToMove }}</span>
            <span class="text-text-muted text-xs ml-1">items to move</span>
          </div>
          <div>
            <span class="text-text-primary font-bold text-lg tabular-nums">{{ plan.zonesInvolved }}</span>
            <span class="text-text-muted text-xs ml-1">zones</span>
          </div>
          <div v-if="consolidation.carryBag.value.size > 0">
            <span class="text-accent-blue font-bold text-lg tabular-nums">{{ consolidation.carryBag.value.size }}</span>
            <span class="text-text-muted text-xs ml-1">carrying</span>
          </div>
        </div>

        <div class="ml-auto flex items-center gap-2">
          <template v-if="!consolidation.wizardActive.value">
            <button class="btn btn-primary text-xs" @click="consolidation.startWizard()">Start Wizard</button>
            <button class="btn btn-secondary text-xs" :disabled="planning" @click="planRoute">
              {{ planning ? 'Planning...' : 'Plan Route' }}
            </button>
          </template>
          <template v-else>
            <div class="flex items-center gap-2 text-xs">
              <div class="w-32 h-1.5 bg-surface-elevated rounded-full overflow-hidden">
                <div class="h-full bg-accent-gold rounded-full transition-all" :style="{ width: `${progressPct}%` }" />
              </div>
              <span class="text-text-muted tabular-nums">{{ consolidation.completedCount.value }}/{{ consolidation.totalCount.value }}</span>
            </div>
            <button class="btn btn-secondary text-xs" @click="consolidation.stopWizard()">Exit Wizard</button>
          </template>
        </div>
      </div>

      <!-- Storage-full warning: items that couldn't be fully consolidated -->
      <div v-if="plan.blockedItems.length" class="shrink-0 mb-3 p-2 rounded bg-status-warning/10 border border-status-warning/30 text-xs">
        <div class="flex items-center gap-2 mb-1">
          <span class="text-status-warning font-semibold">Storage full</span>
          <span class="text-text-secondary">
            {{ plan.blockedItems.length }} item{{ plan.blockedItems.length === 1 ? '' : 's' }} couldn't be fully consolidated — target vault{{ plan.blockedItems.length === 1 ? '' : 's' }} at capacity.
          </span>
        </div>
        <ul class="list-disc pl-5 space-y-0.5 text-text-dim max-h-24 overflow-y-auto">
          <li v-for="b in plan.blockedItems" :key="b.itemName">
            <ItemInline :reference="b.itemName" class="text-text-secondary" />
            <span class="tabular-nums"> ×{{ b.leftoverQuantity }}</span>
            <template v-if="b.fullyBlocked">
              — nowhere with room (target <span class="text-text-secondary">{{ b.targetVaultName }}</span> full)
            </template>
            <template v-else>
              — {{ b.leftoverStacks }} stack{{ b.leftoverStacks === 1 ? '' : 's' }} left in {{ b.leftoverVaultNames.join(', ') }}
            </template>
          </li>
        </ul>
      </div>

      <!-- Route result banner -->
      <div v-if="route" class="shrink-0 mb-3 p-2 rounded bg-accent-gold/10 border border-accent-gold/30 flex items-center gap-3 text-xs">
        <span class="text-accent-gold font-semibold">Route:</span>
        <span class="text-text-primary">{{ route.total_hops }} hops</span>
        <button class="ml-auto text-text-muted hover:text-text-primary cursor-pointer" @click="showRouteSteps = !showRouteSteps">
          {{ showRouteSteps ? 'Hide' : 'Show steps' }}
        </button>
      </div>
      <div v-if="route && showRouteSteps" class="shrink-0 mb-3 border border-border-default rounded overflow-hidden max-h-32 overflow-y-auto">
        <div v-for="(step, i) in route.steps" :key="i"
          class="flex items-start gap-2 py-1 px-2 text-xs border-b border-border-default/30 last:border-b-0"
          :class="step.action === 'travel' ? 'bg-surface-elevated/30' : ''">
          <span class="shrink-0 w-4 text-text-muted text-[10px] text-right mt-0.5">{{ i + 1 }}</span>
          <span :class="step.action === 'travel' ? 'text-text-dim italic' : 'text-text-primary'">{{ step.details }}</span>
        </div>
      </div>
      <div v-if="routeError" class="shrink-0 mb-3 text-accent-red text-xs">{{ routeError }}</div>

      <!-- Wizard: current zone highlight -->
      <div v-if="consolidation.wizardActive.value && consolidation.currentZoneStop.value" class="shrink-0 mb-3 p-3 rounded-lg border-2 border-accent-gold/50 bg-accent-gold/5">
        <div class="flex items-center gap-2 mb-2">
          <span class="text-accent-gold text-xs font-bold uppercase tracking-wider">You are here</span>
          <span class="text-text-primary font-semibold">{{ consolidation.currentZoneStop.value.areaName }}</span>
        </div>
        <div class="grid grid-cols-2 gap-4">
          <div v-if="consolidation.currentZoneStop.value.pickups.length">
            <div class="micro-label mb-1 text-value-positive">Pick Up to Carry</div>
            <MoveChecklist
              :moves="consolidation.currentZoneStop.value.pickups"
              type="pickup"
              :is-done="consolidation.isPickupDone"
              :toggle="consolidation.togglePickup"
              show-vault-name
              :show-character="includeAlts" />
          </div>
          <div v-if="consolidation.currentZoneStop.value.dropoffs.length">
            <div class="micro-label mb-1 text-value-negative">Drop Off</div>
            <MoveChecklist
              :moves="consolidation.currentZoneStop.value.dropoffs"
              type="dropoff"
              :is-done="consolidation.isDropoffDone"
              :toggle="consolidation.toggleDropoff"
              show-target
              show-capacity
              :show-character="includeAlts" />
          </div>
        </div>
        <div v-if="consolidation.currentZoneStop.value.localMoves.length" class="mt-2 pt-2 border-t border-border-default/50">
          <div class="micro-label mb-1 text-accent-blue">Rearrange Locally</div>
          <MoveChecklist
            :moves="consolidation.currentZoneStop.value.localMoves"
            type="local"
            :is-done="consolidation.isLocalDone"
            :toggle="consolidation.toggleLocal"
            show-both-vaults
            :show-character="includeAlts" />
        </div>

        <!-- Next zone indicator -->
        <div v-if="consolidation.nextZoneStop.value" class="mt-3 flex items-center gap-3 px-2 py-2 rounded bg-surface-elevated/50 border border-border-default">
          <span class="text-text-muted text-xs">Next stop:</span>
          <span class="dialog-title">{{ consolidation.nextZoneStop.value.areaName }}</span>
          <span class="text-text-dim text-xs">
            {{ consolidation.nextZoneStop.value.pickups.length }} pickups
            <template v-if="consolidation.nextZoneStop.value.dropoffs.length">, {{ consolidation.nextZoneStop.value.dropoffs.length }} dropoffs</template>
          </span>
        </div>
        <div v-else-if="consolidation.completedCount.value === consolidation.totalCount.value" class="mt-3 px-2 py-2 rounded bg-value-positive/10 border border-value-positive/30 text-value-positive text-sm font-semibold text-center">
          All done!
        </div>
      </div>

      <!-- Wizard: no current zone stop -->
      <div v-else-if="consolidation.wizardActive.value && consolidation.nextZoneStop.value" class="shrink-0 mb-3 p-3 rounded-lg border border-border-default bg-surface-base/50">
        <div class="flex items-center gap-3">
          <span class="text-text-muted text-xs">Nothing to do here. Next stop:</span>
          <span class="text-text-primary font-semibold">{{ consolidation.nextZoneStop.value.areaName }}</span>
          <span class="text-text-dim text-xs">
            {{ consolidation.nextZoneStop.value.pickups.length }} pickups
            <template v-if="consolidation.nextZoneStop.value.dropoffs.length">, {{ consolidation.nextZoneStop.value.dropoffs.length }} dropoffs</template>
          </span>
        </div>
      </div>

      <!-- Zone-by-zone plan -->
      <div class="flex-1 min-h-0 overflow-y-auto pr-1">
        <div class="grid grid-cols-1 lg:grid-cols-2 xl:grid-cols-3 gap-3">
          <div v-for="zs in displayedZoneStops" :key="zs.areaKey" class="card p-0 overflow-hidden" :class="zs.completed ? 'opacity-40' : ''">
            <div class="flex items-center gap-2 px-3 py-2 bg-surface-base/50 border-b border-border-default">
              <div class="text-xs font-semibold text-text-primary flex-1">{{ zs.areaName }}</div>
              <span class="text-[10px] text-text-muted">{{ zs.pickups.length + zs.dropoffs.length + zs.localMoves.length }} actions</span>
            </div>
            <div class="divide-y divide-border-default/30">
              <div v-if="zs.pickups.length" class="px-3 py-1.5">
                <div class="micro-label mb-1 text-value-positive">Pick Up to Carry</div>
                <MoveChecklist :moves="zs.pickups" type="pickup" :is-done="consolidation.isPickupDone" :toggle="consolidation.togglePickup" show-vault-name :show-character="includeAlts" />
              </div>
              <div v-if="zs.dropoffs.length" class="px-3 py-1.5">
                <div class="micro-label mb-1 text-value-negative">Drop Off</div>
                <MoveChecklist :moves="zs.dropoffs" type="dropoff" :is-done="consolidation.isDropoffDone" :toggle="consolidation.toggleDropoff" show-target show-capacity :show-character="includeAlts" />
              </div>
              <div v-if="zs.localMoves.length" class="px-3 py-1.5">
                <div class="micro-label mb-1 text-accent-blue">Rearrange Locally</div>
                <MoveChecklist :moves="zs.localMoves" type="local" :is-done="consolidation.isLocalDone" :toggle="consolidation.toggleLocal" show-both-vaults :show-character="includeAlts" />
              </div>
            </div>
          </div>
        </div>
      </div>
    </template>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, onMounted, defineComponent, h } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { useGameStateStore } from '../../stores/gameStateStore'
import { useStorageConsolidation, type PlannedMove } from '../../composables/useStorageConsolidation'
import EmptyState from '../Shared/EmptyState.vue'
import ItemInline from '../Shared/Item/ItemInline.vue'

const gameState = useGameStateStore()
const consolidation = useStorageConsolidation()

// Cross-alt bindings. `includeAlts` is a writable computed from the composable;
// `bankModel` shows the effective bank (defaults to the active character).
const includeAlts = consolidation.includeAlts
const category = consolidation.category
const bankModel = computed({
  get: () => consolidation.bankKey.value || consolidation.bankOwnerKey.value,
  set: (v: string) => { consolidation.bankKey.value = v },
})

const planning = ref(false)
const routeError = ref('')
const showRouteSteps = ref(false)

interface PlannedRoute {
  steps: { zone: string; action: string; details: string }[]
  total_hops: number
}
const route = ref<PlannedRoute | null>(null)

onMounted(() => gameState.loadStorageVaults())

const plan = computed(() => consolidation.plan.value)

const progressPct = computed(() => {
  if (consolidation.totalCount.value === 0) return 0
  return Math.round((consolidation.completedCount.value / consolidation.totalCount.value) * 100)
})

const displayedZoneStops = computed(() => {
  if (!consolidation.wizardActive.value) return plan.value.zoneStops
  const currentArea = consolidation.currentZone.value
  return plan.value.zoneStops.filter(zs => zs.areaKey !== currentArea)
})

async function planRoute() {
  planning.value = true
  routeError.value = ''
  route.value = null
  try {
    const areaObj = gameState.world?.area as { area_name?: string } | null
    const startZone = areaObj?.area_name ?? 'AreaSerbule'
    const configStr = localStorage.getItem('tripPlannerWidget.config')
    const config = configStr ? JSON.parse(configStr) : {}
    const travelConfig = {
      primaryBind: config.primaryBind ?? null,
      secondaryBind: config.secondaryBind ?? null,
      mushroomCircle1: config.mushroomCircle ?? null,
      mushroomCircle2: null,
      useTpMachine: config.useTpMachine ?? false,
      casinoPortal: null,
    }
    const stops = consolidation.routeStops.value.map(s => ({ zone: s.zone, purpose: s.purpose, details: s.details }))
    if (stops.length === 0) {
      routeError.value = 'No routable stops.'
      return
    }
    route.value = await invoke<PlannedRoute>('plan_trip', { startZone, stops, travelConfig })
    showRouteSteps.value = false
  } catch (e) {
    routeError.value = String(e)
  } finally {
    planning.value = false
  }
}

// ── Inline checklist component ──────────────────────────────────────────────

const MoveChecklist = defineComponent({
  props: {
    moves: { type: Array as () => PlannedMove[], required: true },
    type: { type: String as () => 'pickup' | 'dropoff' | 'local', required: true },
    isDone: { type: Function as unknown as () => (m: PlannedMove) => boolean, required: true },
    toggle: { type: Function as unknown as () => (m: PlannedMove) => void, required: true },
    showVaultName: { type: Boolean, default: false },
    showTarget: { type: Boolean, default: false },
    showBothVaults: { type: Boolean, default: false },
    showCapacity: { type: Boolean, default: false },
    showCharacter: { type: Boolean, default: false },
  },
  setup(props) {
    // Colour the target occupancy badge as it approaches capacity.
    function capClass(move: PlannedMove): string {
      const cap = move.toVaultCapacity
      if (cap == null || cap === 0) return 'text-text-muted'
      const pct = (move.toVaultOccupied / cap) * 100
      if (pct >= 100) return 'text-accent-red'
      if (pct >= 90) return 'text-status-warning'
      return 'text-text-muted'
    }
    // A vault reference, optionally prefixed with its owning character.
    function vaultBits(character: string, name: string) {
      const bits = []
      if (props.showCharacter) bits.push(h('span', { class: 'text-accent-blue/80 shrink-0' }, `[${character}]`))
      bits.push(h('span', { class: 'truncate' }, name))
      return bits
    }
    return () => h('div', { class: 'flex flex-col gap-0.5' },
      props.moves.map(move => {
        const done = props.isDone(move)
        return h('label', {
          key: `${move.itemName}-${move.fromVaultKey}-${move.toVaultKey}`,
          class: ['flex items-center gap-1.5 text-xs py-0.5 cursor-pointer', done ? 'opacity-40 line-through' : ''],
        }, [
          h('input', { type: 'checkbox', checked: done, class: 'shrink-0', onChange: () => props.toggle(move) }),
          (props.showCharacter && move.crossCharacter)
            ? h('span', { class: 'text-accent-blue shrink-0', title: 'Cross-character move — switch to the owning character' }, '⇄')
            : null,
          h(ItemInline, { reference: move.itemName, class: 'flex-1 min-w-0 truncate' }),
          h('span', { class: 'tabular-nums text-text-secondary shrink-0' }, `x${move.quantity}`),
          props.showVaultName ? h('span', { class: 'text-text-dim text-xs shrink-0 flex items-center gap-1 max-w-64' }, vaultBits(move.fromCharacter, move.fromVaultName)) : null,
          props.showTarget ? h('span', { class: 'text-text-dim text-xs shrink-0 flex items-center gap-1 max-w-64' }, [
            h('span', { class: 'shrink-0' }, '→'),
            ...vaultBits(move.toCharacter, move.toVaultName),
            (props.showCapacity && move.toVaultCapacity != null)
              ? h('span', { class: ['tabular-nums shrink-0', capClass(move)] }, `${move.toVaultOccupied}/${move.toVaultCapacity}`)
              : null,
          ]) : null,
          props.showBothVaults ? h('span', { class: 'text-text-dim text-xs shrink-0 flex items-center gap-1 max-w-80' }, [
            ...vaultBits(move.fromCharacter, move.fromVaultName),
            h('span', { class: 'shrink-0' }, '→'),
            ...vaultBits(move.toCharacter, move.toVaultName),
          ]) : null,
        ])
      })
    )
  },
})
</script>
