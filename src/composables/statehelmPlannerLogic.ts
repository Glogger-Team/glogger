/**
 * Pure, dependency-free core of the Statehelm gift planner.
 *
 * No Vue, Pinia, or Tauri imports — runnable under plain `node --test`
 * (see `statehelmPlannerLogic.test.ts`). The composable
 * (`useStatehelmPlanner.ts`) wires these to stores and IPC.
 */

/** Minimal shape of a backend gift match needed for assignment. */
export interface MatchInput {
  vault_key: string
  item_name: string
  npc_key: string
  desire: string
  pref: number
}

/** Love (1) outranks Like (0); anything else is unassignable (-1). */
export function desireRank(desire: string): number {
  if (desire === 'Love') return 1
  if (desire === 'Like') return 0
  return -1
}

/**
 * Greedy assignment over backend-ordered matches (Love first, higher pref
 * first): each (vault, item) stack goes to the highest-benefit NPC that
 * still has weekly gifts remaining and is not excluded. Never exceeds the
 * given remaining counts. Pure: never mutates its inputs.
 */
export function assignGifts<M extends MatchInput>(
  matches: readonly M[],
  remaining: Readonly<Record<string, number>>,
  excluded: ReadonlySet<string>,
): M[] {
  const left = { ...remaining }
  const claimedItems = new Set<string>()
  const out: M[] = []
  for (const m of matches) {
    if (excluded.has(m.npc_key)) continue
    if (desireRank(m.desire) < 0) continue
    if ((left[m.npc_key] ?? 0) <= 0) continue
    const itemKey = JSON.stringify([m.vault_key, m.item_name])
    if (claimedItems.has(itemKey)) continue
    claimedItems.add(itemKey)
    left[m.npc_key] = (left[m.npc_key] ?? 0) - 1
    out.push(m)
  }
  return out
}

/** One vault group's contribution to route planning. */
export interface PickupGroupInput {
  vaultArea: string | null
  vaultLabel: string
  items: string[]
}

/** A pickup stop for `plan_trip`. */
export interface PickupStop {
  zone: string
  purpose: 'pickup'
  details: string
}

/**
 * One pickup stop per vault group with a routable area. Same-area vaults
 * each get their own stop — `trip_router` groups same-zone stops itself, so
 * merging here would drop vaults from the route details.
 */
export function buildPickupStops(groups: readonly PickupGroupInput[]): PickupStop[] {
  const stops: PickupStop[] = []
  for (const g of groups) {
    if (!g.vaultArea) continue
    stops.push({
      zone: g.vaultArea,
      purpose: 'pickup',
      details: `Pick up ${g.items.join(', ')} from ${g.vaultLabel}`,
    })
  }
  return stops
}
