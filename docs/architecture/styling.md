# Styling Guide

How glogger handles visual design, from tooling to conventions.

## Stack

| Tool | Version | Role |
|------|---------|------|
| **Tailwind CSS** | v4 | Utility-first styling |
| **@tailwindcss/vite** | — | Vite plugin (no PostCSS config needed) |

Tailwind is loaded via the Vite plugin in [`vite.config.ts`](../../vite.config.ts):

```ts
import tailwindcss from "@tailwindcss/vite";
export default defineConfig({ plugins: [vue(), tailwindcss()] });
```

## CSS Architecture

All styles live in [`src/assets/css/`](../../src/assets/css/). The entry point is [`main.css`](../../src/assets/css/main.css), imported once in [`main.ts`](../../src/main.ts).

```text
src/assets/css/
├── main.css        # Entry point — imports everything below
├── theme.css       # Design tokens (@theme block)
├── base.css        # Global resets, body defaults, keyframes
├── components.css  # Reusable component classes (.btn, .card, etc.)
└── utilities.css   # Custom utility classes (currently empty)
```

### File Responsibilities

**theme.css** — All color tokens live in the `@theme {}` block. Tailwind v4 reads these and generates utility classes automatically (e.g., `--color-accent-gold` becomes `text-accent-gold`, `bg-accent-gold`, `border-accent-gold`).

**base.css** — `@layer base {}` for global element defaults. Body background/color/font, default button appearance, and keyframe definitions.

**components.css** — `@layer components {}` for reusable class patterns built with `@apply`. These are for patterns that repeat across many components. If a pattern only appears once, use inline utilities instead.

**utilities.css** — `@layer utilities {}` for custom one-off utilities that Tailwind doesn't provide.

## Design Tokens

The full token set is defined in [`theme.css`](../../src/assets/css/theme.css). Key groups:

### Surfaces

| Token | Hex | Usage |
|-------|-----|-------|
| `surface-dark` | `#111111` | Page/body background |
| `surface-base` | `#1a1a1a` | Section backgrounds |
| `surface-card` | `#222222` | Card/panel backgrounds |
| `surface-elevated` | `#2a2a2a` | Hover states, raised elements |

### Borders

| Token | Hex | Usage |
|-------|-----|-------|
| `border-default` | `#333333` | Default border |
| `border-light` | `#444444` | Subtle emphasis |
| `border-hover` | `#555555` | Hover/focus states |

### Text

| Token | Hex | Usage |
|-------|-----|-------|
| `text-primary` | `#cccccc` | Main content |
| `text-secondary` | `#888888` | Supporting text |
| `text-muted` | `#666666` | De-emphasized text |
| `text-dim` | `#555555` | Barely visible labels |
| `text-system` | `#999999` | System messages |

### Accents

| Token | Hex | Usage |
|-------|-----|-------|
| `accent-gold` | `#e0c060` | Primary accent, active states |
| `accent-blue` | `#4a90e2` | Links, informational |
| `accent-green` | `#5cb85c` | Success, positive |
| `accent-red` | `#d9534f` | Error, danger |
| `accent-warning` | `#f0ad4e` | Warnings |

### Entity Colors

Each game entity type has a color for consistent visual identification. Used by the [shared components](../reference/shared-components.md).

| Token | Hex | Entity |
|-------|-----|--------|
| `entity-item` | `#7ec8e3` | Items |
| `entity-quest` | `#e0c060` | Quests |
| `entity-skill` | `#5cb85c` | Skills |
| `entity-npc` | `#e0965c` | NPCs |
| `entity-ability` | `#b07ce0` | Abilities |
| `entity-area` | `#6a9fb5` | Areas |
| `entity-enemy` | `#d9534f` | Enemies |

### Channel Colors

Chat channels each have a dedicated token (e.g., `channel-global`, `channel-trade`, `channel-combat`). See [`theme.css`](../../src/assets/css/theme.css) for the full list.

## Component Classes

Defined in [`components.css`](../../src/assets/css/components.css). Use these for patterns that repeat across many files.

### Buttons

```html
<button class="btn btn-primary">Save</button>
<button class="btn btn-secondary">Cancel</button>
<button class="btn btn-warning">Warn</button>
<button class="btn btn-danger">Delete</button>
```

`.btn` provides the base shape (padding, rounded, font, transition). Variants add color.

### Layout

```html
<div class="card">...</div>          <!-- bg-surface-card, border, rounded -->
<div class="settings-section">...</div>  <!-- padded section with heading support -->
<div class="status-panel">...</div>  <!-- monospace data panel -->
```

### Inputs

```html
<input class="input" />  <!-- styled text input with focus ring -->
```

### Tabs

```html
<button class="tab" :class="{ 'tab-active': isActive }">Tab</button>
```

### Feedback Boxes

```html
<div class="error-box">Something failed</div>
<div class="success-box">Operation complete</div>
<div class="info-box">Informational note</div>
```

### Status Panels

```html
<div class="status-panel">
  <div class="status-row">
    <span class="status-label">Key</span>
    <span class="status-value">Value</span>
  </div>
</div>
```

### Headings

```html
<h1 class="h1-brand">Brand</h1>                        <!-- text-2xl, app brand mark -->
<h2 class="screen-title">Screen Name</h2>           <!-- text-xl, screen/tab titles -->
<h2 class="display-title">Channel Name</h2>          <!-- text-lg, gold display titles -->
<h3 class="section-heading">Section</h3>             <!-- text-sm, major section dividers -->
<h3 class="dialog-title">Dialog</h3>                 <!-- text-sm, dialog/overlay titles -->
<h4 class="panel-label">Panel</h4>                   <!-- text-xs, sub-section labels -->
<h5 class="micro-label">Dense Label</h5>             <!-- text-[10px], build planner micro headings -->
```

`.section-heading`, `.panel-label`, and `.micro-label` are uppercase, semibold, with wide tracking. `.screen-title` is medium weight, normal case. `.dialog-title` is semibold, normal case, primary text. `.display-title` and `.h1-brand` are gold (semibold and bold respectively). Add margin/padding alongside the class as needed (e.g., `class="section-heading mb-2"`).

## Conventions

### No `<style>` blocks

Components must not contain `<style>` or `<style scoped>` blocks. All styling is done through:

1. Tailwind utility classes in the template
2. Global component classes from `components.css`
3. `:style` bindings for dynamic computed values only

### Tailwind v4 Syntax

Use the **canonical** (postfix) `!important` syntax:

```html
<!-- Correct -->
<div class="bg-surface-elevated! text-accent-gold!">

<!-- Wrong (v3 syntax) -->
<div class="!bg-surface-elevated !text-accent-gold">
```

Use Tailwind's native units where possible instead of arbitrary values:

```html
<!-- Prefer -->
<div class="max-w-225 min-h-125 min-w-40">

<!-- Avoid -->
<div class="max-w-[900px] min-h-[500px] min-w-[160px]">
```

### When to Use Arbitrary Values

Arbitrary hex colors in utility classes (e.g., `bg-[#1a1a2e]`, `text-[#7ec8e3]`) are acceptable for one-off or context-specific colors that don't warrant a theme token. If you find yourself using the same arbitrary color in 3+ components, promote it to a token in `theme.css`.

### When to Use `:style` Bindings

Only for values that are truly dynamic at runtime — computed from JavaScript, driven by data, or calculated from props. Examples:

- Progress bar widths: `:style="{ width: percent + '%' }"`
- Data-driven border colors: `:style="{ borderLeftColor: kindColor[entry.kind] }"`

Static visual styling should always be a Tailwind class, never an inline `style=""` attribute.

### When to Create a Component Class

Add to `components.css` when a pattern:

- Appears in **3+ components** with the same class combination
- Represents a **semantic concept** (a "card", a "button variant", a "status panel")

Don't create component classes for:

- One-off layouts specific to a single view
- Simple combinations like `flex gap-4`

### File Structure

Vue files follow this order — template first, then script. No style block.

```vue
<template>
  <!-- markup with Tailwind utilities -->
</template>

<script setup lang="ts">
// composition API
</script>
```

### Color Hierarchy

For consistent visual weight across the app:

| Purpose | Approach |
|---------|----------|
| Active/selected nav items | `text-accent-gold` / `border-accent-gold` |
| Section headings | `text-[#7ec8e3]` (info blue) |
| Positive values (gains, revenue) | `text-[#7ec87e]` or `text-[#8ec88e]` |
| Negative values (costs, losses) | `text-[#c87e7e]` |
| Gold/currency amounts | `text-[#d4af37]` |
| Neutral data | `text-text-primary` |
| Labels / captions | `text-text-muted` or `text-text-dim` |

### Pseudo-elements in Tailwind

Use Tailwind's pseudo-element variants for decorative elements:

```html
<!-- Active tab indicator -->
<button class="after:content-[''] after:absolute after:-bottom-0.5 after:h-0.5 after:bg-[#7ec8e3]">

<!-- List bullet -->
<div class="before:content-['•'] before:absolute before:left-0">
```

## Adding New Tokens

1. Add the variable to the `@theme {}` block in [`theme.css`](../../src/assets/css/theme.css)
2. Tailwind v4 auto-generates utilities from `@theme` — no config file needed
3. Use the token as `text-{name}`, `bg-{name}`, `border-{name}`, etc.

```css
/* In theme.css */
@theme {
  --color-my-new-color: #abc123;
}
```

```html
<!-- Immediately available -->
<div class="text-my-new-color bg-my-new-color/50">
```

## Adding Component Classes

1. Add to the `@layer components {}` block in [`components.css`](../../src/assets/css/components.css)
2. Use `@apply` with Tailwind utilities
3. Keep classes small and composable — a base + variants pattern (like `.btn` + `.btn-primary`)

```css
@layer components {
  .my-panel {
    @apply bg-surface-base border border-border-default rounded-lg p-4;
  }
}
```
