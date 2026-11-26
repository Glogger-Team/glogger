/**
 * Regression pins for the Statehelm gift planner core.
 * Run: `node --test src/composables/statehelmPlannerLogic.test.ts`
 * (no runner dependency — Node ≥24 strips types natively).
 *
 * @types/node is deliberately not a dependency (the frontend never uses
 * node APIs beyond this runner), so the two node imports below are
 * @ts-ignored: runtime resolution is proven by the passing run.
 */
// @ts-ignore: no @types/node on purpose (see header)
import { describe, it } from 'node:test'
// @ts-ignore: no @types/node on purpose (see header)
import assert from 'node:assert/strict'
import { computed, nextTick, ref, watch } from 'vue'
import { assignGifts, buildPickupStops } from './statehelmPlannerLogic.ts'

function match(vault: string, item: string, npc: string, desire = 'Love', pref = 10) {
  return { vault_key: vault, item_name: item, npc_key: npc, desire, pref }
}

describe('P2-2: same-area vaults each surface a pickup stop', () => {
  it('emits one stop per vault group even when areas coincide', () => {
    const stops = buildPickupStops([
      { vaultArea: 'AreaStatehelm', vaultLabel: 'Gossamyr', items: ['Garlic'] },
      { vaultArea: 'AreaStatehelm', vaultLabel: 'Captain Gregory', items: ['Red Rose'] },
    ])
    assert.equal(stops.length, 2)
    assert.ok(stops[0].details.includes('Gossamyr'))
    assert.ok(stops[1].details.includes('Captain Gregory'))
  })

  it('still skips vaults with no routable area', () => {
    const stops = buildPickupStops([
      { vaultArea: null, vaultLabel: 'Unknown Vault', items: ['Garlic'] },
      { vaultArea: 'AreaSerbule', vaultLabel: 'Serbule Vault', items: ['Apple'] },
    ])
    assert.equal(stops.length, 1)
    assert.equal(stops[0].zone, 'AreaSerbule')
  })
})

describe('P2-1: recounts invalidate the plan', () => {
  it('a counts change that caps an NPC drops it from the assignments', () => {
    const matches = [match('V1', 'Garlic', 'NPC_A'), match('V1', 'Red Rose', 'NPC_B')]
    const excluded = new Set<string>()
    // Both NPCs have gifts remaining — both claimed.
    assert.deepEqual(
      assignGifts(matches, { NPC_A: 1, NPC_B: 1 }, excluded).map((m) => m.npc_key),
      ['NPC_A', 'NPC_B'],
    )
    // NPC_A hits its cap (gift given / manual +) — tables drop it.
    assert.deepEqual(
      assignGifts(matches, { NPC_A: 0, NPC_B: 1 }, excluded).map((m) => m.npc_key),
      ['NPC_B'],
    )
  })

  it('a watcher on the assignments nulls a rendered route when they change', async () => {
    // Mirrors the composable wiring: `watch(assignments, () => route = null)`.
    const remaining = ref<Record<string, number>>({ NPC_A: 1 })
    const matches = ref([match('V1', 'Garlic', 'NPC_A')])
    const assignments = computed(() => assignGifts(matches.value, remaining.value, new Set()))
    const route = ref<string | null>('rendered-route')
    watch(assignments, () => {
      route.value = null
    })
    assert.equal(assignments.value.length, 1)
    remaining.value = { NPC_A: 0 }
    await nextTick()
    assert.equal(assignments.value.length, 0)
    assert.equal(route.value, null)
  })
})
