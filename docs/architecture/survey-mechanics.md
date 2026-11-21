# Survey Mechanics Reference

How the game's three survey-map kinds behave when used. This is a behavioral catalog of what we observe in Player.log and chat — naming below is **glogger-internal only** (not used by the game itself), chosen to disambiguate the three patterns.

## Why this matters

Tracking, attribution, and UX vary by kind:

- Speed bonus only applies to `Basic`. Treating motherlode/multihit gains as missing a "speed bonus" produces false negatives.
- Multihit nodes survive multiple swings; they should not be marked as completed on the first ore drop.
- Motherlode locations require triangulation across multiple uses; the parser should treat repeated "scan" uses on the same map differently from "found, ready to mine" uses.
- Loot attribution windows differ — a basic survey hands out everything in one tick, a multihit drips loot across several swings, a motherlode batches per node.

## Kinds

### Basic Survey

**Use behavior:**

- Out of target area → 2 distance numbers (N/S, E/W), target area highlighted on the map.
- In target area → all rewards drop immediately. **Speed bonus rewards** are appended if collected within X seconds of the prior survey use.

**Loot pattern in logs:**

- One `[Status] X added to inventory` per item (primary + each speed-bonus item as separate lines).
- One `ProcessScreenText(ImportantInfo, "X collected! Also found Y …")` per use.
- Survey map is consumed (`ProcessDeleteItem`) on successful collection.

**Maps:**

| Area | Maps |
|---|---|
| Serbule Hills | Rubywall Crystal, Orange Mineral, White Mineral, Simple Mining, Basic Mining |
| Serbule | Rubywall Crystal, Blue Mineral, Green Mineral, White Mineral |
| Eltibule | Orange Mineral, Blue Mineral, Green Mineral, Good Mining, Expert Mining, Masterwork Mining, Amazing Mining |
| Kur Mountains | Blue Mineral, Green Mineral, Orange Mineral, White Mineral |

### Motherlode Survey

**Use behavior:**

- Out of target area → 1 distance number (pure linear distance), target area **not** highlighted on the map. Location must be **triangulated** by using the map from multiple positions.
- In target area → spawns a minable **motherlode node**. Node disappears after a single mining hit.
- **No speed bonus** mechanic. Loot pattern is typically primary reward + bonus reward but neither is guaranteed.

**Loot pattern in logs:**

- Mining hit produces 1–2 `[Status] X added to inventory` lines; the corresponding `ProcessAddItem`/`ProcessUpdateItemCode` Player.log lines are no longer parsed (item parser retired 2026-11).
- The survey map itself is consumed on the use that spawns the node, not on the mining hit.
- Node removal is implicit (no log line specific to "node despawned"); the next interaction with that entity_id will fail or produce a new entity entirely.

**Maps:**

| Area | Maps |
|---|---|
| Kur Mountains | Simple Metal Motherlode, Basic Metal Motherlode, Good Metal Motherlode |
| Ilmari Desert | Expert Metal Motherlode, Master's Metal Motherlode, Amazing Metal Motherlode, Astounding Metal Motherlode |
| Gazluk | Astounding Metal Motherlode, Superb Metal Motherlode |

### Multihit Survey

**Use behavior:**

- Out of target area → 2 distance numbers (N/S, E/W), target area highlighted on the map (same as Basic).
- In target area → spawns a minable node. Node persists across multiple mining hits — observed range is **>2 and <20** swings before depletion. Hit count varies per node.
- **No speed bonus** mechanic.

**Loot pattern in logs:**

- Each mining hit produces its own batch of `[Status] X added to inventory` lines (the corresponding `ProcessAddItem`/`ProcessUpdateItemCode` Player.log lines are no longer parsed — item parser retired 2026-11, same as Motherlode). In Player.log-era logs each hit was its own `ProcessDoDelayLoop` cycle under a fresh `Mining` activity context.
- The survey map is consumed when the node spawns, not on individual hits.
- The node depleting is implicit (same as motherlode).

**Maps:**

| Area | Maps |
|---|---|
| Povus | Astounding Mining Survey, Superb Mining Survey, Marvelous Mining Survey, Blue Node, Green Node |
| Vidaria | Orange Node, White Node |

## CDN data — what it does and doesn't tell us

Survey maps in `items.json` carry useful structural data, but **kind is not fully derivable from CDN fields alone**. Specifically:

- **`Keywords` reliably identifies Motherlode**: the `"MotherlodeMap"` keyword is present on every motherlode map and absent everywhere else. This is the one trustworthy CDN signal for kind.
- **Area is reliably embedded in the internal name** as the second token (`SouthSerbule`, `Eltibule`, `KurMountains`, `Ilmari`, `Gazluk`, `Povus`, `Vidaria`, `Serbule`). The display name carries it too.
- **`Description`, `UseDelay`, `UseVerb`, `Keywords` other than `MotherlodeMap` do *not* distinguish Basic from Multihit.** Vidaria multihit (`GeologySurveyVidaria4`) and Serbule basic (`GeologySurveySerbule1`) have identical fields except for area and display name. Eltibule mining surveys (Basic) and Povus mining surveys (Multihit) share the same description text verbatim.

Internal-name prefix (`GeologySurvey` vs `MiningSurvey`) is **not** a kind discriminator — both Basic and Multihit maps appear under both prefixes depending on area.

### Kind classifier

A `survey_kind(item)` helper belongs in `game_data` next to `resolve_item`. The classifier needs CDN inspection plus a **hardcoded area table** for Basic/Multihit since CDN data alone can't separate them:

```text
1. Keywords.contains("MotherlodeMap")     → Motherlode
2. Area ∈ MULTIHIT_AREAS (hardcoded)      → Multihit
3. Otherwise (and InternalName starts with
   "GeologySurvey" or "MiningSurvey")     → Basic
4. Item is not a survey map               → None
```

`MULTIHIT_AREAS` is hardcoded as `{"Povus", "Vidaria"}` for now — this matches every multihit map known at time of writing. Any new area added by the game with multihit nodes will be misclassified as Basic until the constant is updated.

> **TODO**: investigate whether a dynamic kind signal exists in the CDN (other JSON files, a not-yet-parsed `items.json` field, recipes/skills that reference these maps, etc.) so we can stop hardcoding the area list. Until then, the constant lives in code with a comment pointing back to this doc.

For attribution that needs the area itself (e.g., disambiguating "Astounding Metal Motherlode Map" which exists in both Ilmari and Gazluk), use the player's tracked **live area** from game state rather than parsing the area token out of the internal name. Internal-name area is only a fallback when live area isn't available (replay, backfill).

## Behavioral disambiguation (fallback)

If the CDN-based classifier above ever returns an unknown answer (new map added in a patch, unrecognized area, etc.), fall back to observing the map's lifetime in the log:

| Observation | Inferred kind |
|---|---|
| Map consumed (`DeleteItem`) within seconds of a `Using <X>` delay loop, with `[Status]` items immediately following | `Basic` |
| Map consumed without immediate loot, followed by one or more `Mining...` delay loops on the same/nearby entity, single mining cycle yielding loot | `Motherlode` |
| Map consumed without immediate loot, followed by **multiple** `Mining...` delay loops on the same node before any fresh survey use | `Multihit` |
| `ProcessScreenText` contains "Also found … (speed bonus!)" | `Basic` (uniquely identifying — speed bonus only fires here) |

## Attribution windows by kind

When chaining mining hits back to the originating survey map (Phase 5), each kind closes its attribution window differently:

- **Basic** — closes on the same line that produces the loot. The survey map is consumed (`ProcessDeleteItem`) and `[Status]` lines fire immediately. Window = single tick.
- **Motherlode** — closes after the single mining hit on the spawned node completes (one `Mining...` delay loop ending, items dropped). Window = one mining cycle.
- **Multihit** — closes when **either** of the following occurs:
  - The player starts a mining interaction with a **different entity_id**. The game emits no explicit "node depleted" signal, but the next `Mining...` delay loop on a different entity is concrete proof the player has moved on.
  - **30 minutes** have elapsed since the last mining tick on the node. This safety timeout covers server disconnects, the player logging out and back in, area changes, idling, or any other case where we'd otherwise leave the window open forever.

Notably, area changes / teleports / logouts do **not** themselves close a multihit window — only the two conditions above do. The 30-minute timeout makes the long-travel and disconnect cases work without special handling.

This means a multihit window can legitimately stay open for a long time. That's expected and correct: a player walking between nodes might still be on the same node when they return, so we keep accumulating until we see proof of the contrary.

## Implications for downstream features

- **Survey session tracker** should attribute all `Mining`-context gains during the same survey-use → mining-hit chain to the originating survey map.
- **Per-kind statistics** become possible: avg yield per Basic survey vs avg yield per Motherlode node vs avg cumulative yield per Multihit node.
- **Speed bonus tracking** should only apply to Basic. Motherlode/Multihit "speed bonus rate" is meaningless.
- **Map-cost analysis** can divide map cost by mining hits for Multihit (per-swing yield) or by single use for Basic/Motherlode.

## Open questions

- Some maps share names across areas (e.g., Astounding Metal Motherlode appears in both Ilmari and Gazluk) — display name alone is ambiguous; use live tracked area for attribution.
- Hit-count distribution for Multihit nodes — currently observed as "2–20" per the user's recollection. Worth measuring once we're aggregating per-node mining cycles to confirm range and find any per-map patterns.
- Motherlode "primary + bonus" — is it always exactly 1 or 2 items per node, or can a single node yield more? Worth a histogram once data is flowing.
- The Basic/Multihit area table is hand-maintained and could rot if the game adds new areas. Behavioral fallback exists but a CDN signal we haven't found yet would be much better — worth a closer look at adjacent CDN files (`storagevaults.json`, `areas.json`, anything that references map item IDs) before Phase 5 lands.
