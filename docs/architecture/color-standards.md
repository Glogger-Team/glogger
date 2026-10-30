# Color Standards

Audit of color usage across the frontend codebase, with proposed standards for consistency.

## Current State

### Overall Numbers

- **286** total `.vue` files
- **91** files use standard Tailwind palette colors (e.g., `text-green-400`, `bg-red-900/30`)
- **40** files use arbitrary hex colors in Tailwind classes (e.g., `text-[#7ec87e]`, `bg-[#151515]`)
- **~65** distinct arbitrary hex values appear across the codebase
- Theme token usage dominates overall (thousands of uses), but Tailwind defaults and arbitrary hex values are widespread

### Theme Token Adoption (Strong)

The theme system defined in [`theme.css`](../../src/assets/css/theme.css) is well-used. Top usage counts:

| Token | Approximate Uses |
|-------|-----------------|
| `text-text-muted` | 741 |
| `text-text-dim` | 720 |
| `text-text-secondary` | 576 |
| `text-text-primary` | 565 |
| `border-border-default` | 474 |
| `text-accent-gold` | 400 |
| `bg-surface-elevated` | 322 |
| `bg-surface-base` | 158 |

Surface, border, text, accent, and entity tokens are all used consistently for their intended purposes.

### Three Competing Color Systems

The codebase currently uses three different approaches for the same semantic purposes:

**1. Theme tokens** (intended system) — well-adopted for surfaces, borders, primary text, accents, and entity type colors.

**2. Standard Tailwind palette colors** — used heavily for status/semantic colors. Examples:
- `text-green-400` (65 uses) — positive values, success states, "have enough" indicators
- `text-red-400` (66 uses) — negative values, errors, danger actions
- `text-yellow-400` (20 uses) — warnings, gold amounts, partial states
- `text-blue-400` (19 uses) — informational, links, "too high" indicators
- `text-purple-400` (17 uses) — augments, build planner mod power
- `bg-green-500` (20 uses) — status dots (online/active)
- `bg-red-500` (13 uses) — status dots (offline/inactive)

**3. Arbitrary hex colors** — repeated patterns that have become informal conventions:

| Hex | Uses | Semantic Meaning |
|-----|------|-----------------|
| `#7ec87e` | 7+ | Positive values (XP gain, item gain) |
| `#c87e7e` | 10+ | Negative values (item loss, errors) |
| `#c8b47e` | 6+ | Neutral-warm (level ups, favor gains, paused state) |
| `#d4af37` | 5+ | Gold/currency amounts |
| `#8ec88e` | 3+ | Brighter positive (hover states for gains) |
| `#aedaae` | 2 | Even brighter positive hover |
| `#e08060` | 4 | Requirements, prerequisites |
| `#c0a0e0` | 4 | Favor rewards, account-wide badges |
| `#60e090` | 6 | Quest rewards |
| `#6a9fb5` | 3 | Area references (duplicates `entity-area` token) |
| `#887040` | 6 | Lint keywords (dim brownish) |
| `#8888bb` | 2 | Quest keywords |
| `#6a8a6a` | 3 | Source icons |
| `#9a9` | 3 | Skill XP, recipe rewards |
| `#151515` | 24 | Sub-surface background (darker than `surface-dark`) |
| `#1a1a2e` | 36 | Card background (same as `surface-card` token, but hardcoded) |
| `#1e1e1e` | 19 | Hover background for list rows |
| `#2a2a3e` | 6+ | Tooltip section borders, hover backgrounds |
| `#2a2a4e` | 12 | Tag/badge borders |
| `#12122a` | 2 | Deep input backgrounds |

### Notable Inconsistencies

**Chat channel colors are defined twice, differently:**
- [`theme.css`](../../src/assets/css/theme.css) defines `--color-channel-global: #ffd700`, `--color-channel-trade: #66cc66`, etc.
- [`ChatMessage.vue`](../../src/components/Chat/ChatMessage.vue) uses standard Tailwind colors instead (`text-yellow-400`, `text-green-400`, `text-purple-400`, etc.)
- These are visually similar but not identical — `#ffd700` (gold) vs `text-yellow-400` (#facc15), `#66cc66` vs `text-green-400` (#4ade80)

**Positive/negative value colors differ by screen:**
- Farming components use `#7ec87e` / `#c87e7e` (muted green/red)
- Crafting, Build Planner, Character use `text-green-400` / `text-red-400` (Tailwind defaults)
- The theme defines `accent-green: #5cb85c` and `accent-red: #d9534f` — neither matches what's actually used for gain/loss

**Surface-card token is hardcoded in 36 places:**
- `bg-[#1a1a2e]` appears 36 times — this is exactly `--color-surface-card` from the theme
- Components should use `bg-surface-card` instead

**Sub-surface and row-hover backgrounds have no tokens:**
- `bg-[#151515]` (24 uses) — a background darker than `surface-base` used inside cards/panels
- `hover:bg-[#1e1e1e]` (19 uses) — list row hover state, slightly brighter than `surface-base`
- `bg-[#2a2a3e]` — tooltip/section border color with no token

**Rarity colors are inconsistent in format but consistent in values:**
- Three components define the same rarity color map (`InventoryTable`, `InventoryItemPanel`, `InventoryLargeGrid`) using Tailwind defaults (green-400, blue-400, purple-400, orange-400, yellow-400)
- These have no theme tokens

**Chart colors are fully outside the theme:**
- [`LootDonutChart.vue`](../../src/components/Surveying/LootDonutChart.vue) and [`SkillsTab.vue`](../../src/components/Crafting/SkillsTab.vue) use hardcoded hex palettes for chart.js — `#7ec8e3`, `#6366f1`, `#f59e0b`, etc.
- Chart axis/grid/label colors use zinc palette hex values (`#52525b`, `#a1a1aa`, `#d4d4d8`, `#27272a`)

**Status indicator dots use theme tokens where they carry semantics:**
- `bg-status-active` / `bg-status-inactive` for tailing/offline dots (MenuBar)
- `bg-yellow-500` for warning-level capacity bars (VaultRow, VaultAreaCard) — no `status-warning` token use yet

## Existing System

The theme in [`theme.css`](../../src/assets/css/theme.css) covers:

| Category | Tokens | Coverage |
|----------|--------|----------|
| Surfaces | 4 tokens | Good — widely used, but `surface-card` bypassed by 36 hardcoded instances |
| Borders | 3 tokens | Good — but missing tooltip-border and tag-border variants |
| Text | 5 tokens | Good — universally adopted |
| Accents | 5 tokens | Partial — `accent-gold` is heavily used; `accent-green`/`accent-red` exist but are not used for gain/loss semantics |
| Vitals | 4 tokens | Complete — health/armor/power/metabolism |
| Channels | 9 tokens | Defined but ignored in `ChatMessage.vue` |
| Entity types | 8 tokens | Good — consistently used via shared inline components |
| Sender | 1 token | Present |

**Not covered by the theme:**
- Positive/negative value semantics (gain/loss, surplus/shortfall)
- Status indicators (online/offline dots, recording states)
- Item rarity tiers
- List row hover backgrounds
- Sub-surface / inset backgrounds
- Tag/badge backgrounds and borders
- Chart palette
- Quest-specific colors (rewards, requirements, keywords)
- Favor desire colors (love/like/dislike/hate)
- Build planner augment color (purple)

Component classes in [`components.css`](../../src/assets/css/components.css) define `.btn-warning` and `.btn-danger` using arbitrary hex values (`#3a2a1a`, `#664422`, etc.) rather than deriving from theme tokens. Similarly `.error-box`, `.success-box`, and `.info-box` use arbitrary hex backgrounds.

## Proposed Standards

### Tier 1: High-Impact Token Additions

These address the most frequent arbitrary color usage and would eliminate the most inconsistency.

**Value semantics (gain/loss):**
| Proposed Token | Hex | Replaces |
|---------------|-----|----------|
| `value-positive` | `#4ade80` (green-400) | `text-green-400`, `text-[#7ec87e]`, `text-[#8ec88e]` |
| `value-negative` | `#f87171` (red-400) | `text-red-400`, `text-[#c87e7e]` |
| `value-neutral-warm` | `#c8b47e` | `text-[#c8b47e]` (level-ups, favor) |
| `value-gold` | `#d4af37` | `text-[#d4af37]` (gold/currency) |

**Missing surface variants:**
| Proposed Token | Hex | Replaces |
|---------------|-----|----------|
| `surface-inset` | `#151515` | `bg-[#151515]` (24 uses) |
| `surface-row-hover` | `#1e1e1e` | `hover:bg-[#1e1e1e]` (19 uses) |
| `border-subtle` | `#2a2a3e` | `border-[#2a2a3e]` in tooltips, `border-[#2a2a4e]` in tags |

**Status indicators:**
| Proposed Token | Hex | Replaces |
|---------------|-----|----------|
| `status-active` | `#22c55e` (green-500) | `bg-green-500` for online dots |
| `status-inactive` | `#ef4444` (red-500) | `bg-red-500` for offline dots |
| `status-warning` | `#eab308` (yellow-500) | `bg-yellow-500` for capacity bars |

### Tier 2: Domain-Specific Tokens

**Item rarity colors** (used in 3+ components with identical maps):
| Proposed Token | Hex | Rarity |
|---------------|-----|--------|
| `rarity-common` | (inherit text-primary) | Common |
| `rarity-uncommon` | `#4ade80` | Uncommon |
| `rarity-rare` | `#60a5fa` | Rare |
| `rarity-exceptional` | `#c084fc` | Exceptional |
| `rarity-epic` | `#fb923c` | Epic |
| `rarity-legendary` | `#facc15` | Legendary |

**Favor desire colors** (used in ItemSearch, NpcCard, NpcPreferencesSection):
| Proposed Token | Hex | Desire |
|---------------|-----|--------|
| `desire-love` | `#ff69b4` | Love |
| `desire-like` | `#7ec8e3` | Like |
| `desire-dislike` | (use `accent-red`) | Dislike |
| `desire-hate` | `#aa4444` | Hate |

**Build planner augment** (used in 10+ components):
| Proposed Token | Hex | Purpose |
|---------------|-----|---------|
| `mod-augment` | `#c084fc` (purple-400) | Augment indicators |

### Tier 3: Cleanup / Alignment

**Chat channel colors:** [`ChatMessage.vue`](../../src/components/Chat/ChatMessage.vue) should use the existing `channel-*` tokens from the theme instead of Tailwind defaults.

**Surface-card hardcoding:** Replace all 36 instances of `bg-[#1a1a2e]` with `bg-surface-card`.

**Component class cleanup:** `.btn-warning`, `.btn-danger`, `.error-box`, `.success-box`, `.info-box` in [`components.css`](../../src/assets/css/components.css) should derive their background/border colors from theme tokens rather than arbitrary hex values.

**Accent-green / accent-red alignment:** Decide whether `accent-green` and `accent-red` should match the gain/loss colors or remain distinct. Currently they exist in the theme but are rarely used for value semantics — the Tailwind defaults (`green-400`, `red-400`) or the farming hex values (`#7ec87e`, `#c87e7e`) are used instead. Either update the accent tokens to match the chosen gain/loss values, or keep them separate and add dedicated `value-positive` / `value-negative` tokens.

**Chart colors:** Consider defining a `chart-*` palette in the theme so chart components can reference tokens. At minimum, chart axis/label colors should use theme text tokens instead of hardcoded zinc values.

## Migration Status

### Completed

1. **New tokens added to `theme.css`**: value semantics (`value-positive`, `value-negative`, `value-neutral-warm`, `value-gold`), surface variants (`surface-inset`, `surface-row-hover`, `border-subtle`), status indicators (`status-active`, `status-inactive`, `status-warning`), rarity colors (`rarity-uncommon` through `rarity-legendary`), domain-specific (`desire-love`, `desire-like`, `desire-hate`, `mod-augment`).

2. **Hardcoded `#1a1a2e` replaced with `bg-surface-card`** across 23 src/ files.

3. **`bg-[#151515]` replaced with `bg-surface-inset`** across 12 src/ files.

4. **`hover:bg-[#1e1e1e]` replaced with `hover:bg-surface-row-hover`** across 17 src/ files.

5. **`border-[#2a2a3e]` and `border-[#2a2a4e]` replaced with `border-border-subtle`** across 13 src/ files.

6. **Farming hex colors standardized**: `text-[#7ec87e]`/`text-[#8ec88e]` to `text-value-positive`, `text-[#c87e7e]` to `text-value-negative`, `text-[#c8b47e]` to `text-value-neutral-warm`, `text-[#d4af37]` to `text-value-gold`, and bg variants.

7. **Chat channel colors** in `ChatMessage.vue` updated to use `text-channel-*` theme tokens instead of Tailwind defaults. Sender color updated to `text-sender`.

8. **Component classes** in `components.css` updated: `.btn-warning`, `.btn-danger`, `.error-box`, `.success-box`, `.info-box` now derive colors from theme tokens.

### Remaining Work

- **Rarity color maps**: No duplicated rarity maps were found in the current codebase (the inventory components mentioned in the audit don't exist yet). The `rarity-*` tokens are defined and ready for use when those components are built.
- **`text-purple-400` for augments**: No instances found in current src/ codebase. The `mod-augment` token is defined and ready.
- **`bg-green-500`/`bg-red-500`/`bg-yellow-500` status dots**: No instances found in current src/ codebase. The `status-*` tokens are defined and ready.
- **`text-green-400`/`text-red-400` for gain/loss**: No instances found in current src/ codebase outside of ChatMessage.vue (which was handled separately as channel colors).
- **Chart palette migration**: Chart colors are passed as JS config objects to Vue Data UI, not as Tailwind classes. Migrating these would require using `getComputedStyle` to read CSS custom properties at runtime.
- **Reference directory**: Files under `reference/` still contain hardcoded hex values. These are reference/archived components and were not migrated.
