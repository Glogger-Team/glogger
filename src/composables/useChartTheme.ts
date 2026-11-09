// Shared chart palette + theme text reader for vue-data-ui charts.
//
// Ten muted, matte mid-tone hues — one per line. Top-10 mode draws all ten;
// Top-5 uses the first five; single-item and Total use the first.
// Index-based so each line on the chart is a distinct color.
//
// These sit a notch deeper/more saturated than true pastels on purpose: very
// light pale colors wash out toward white on the dark background and — more so
// — under Windows HDR SDR-tone-mapping. Mid-tone (≈55-65% lightness, moderate
// chroma) keeps them matte/non-vivid while holding a readable hue.
export const chartPalette = [
  '#5f9ad4', // blue
  '#d18585', // dusty rose
  '#82c07b', // sage green
  '#a98fd6', // muted violet
  '#d9bd6e', // ochre / gold
  '#6dc0b6', // teal
  '#dca06e', // clay / amber
  '#c98cbf', // orchid
  '#a4b86e', // olive
  '#8395cc', // indigo / periwinkle
]

// Chart label colors resolve theme.css text tokens at runtime: vue-data-ui
// config takes plain color strings, so read the custom properties with
// getComputedStyle (fallbacks match the token values in theme.css).
export function themeText(token: string, fallback: string): string {
  const value = getComputedStyle(document.documentElement).getPropertyValue(token).trim()
  return value || fallback
}

// Tooltip chrome resolves surface/border tokens at runtime (same
// getComputedStyle pattern as themeText). Token choice: #27272a (zinc-800)
// is closest to --color-surface-elevated (#2a2a2a), #3f3f46 (zinc-700) to
// --color-border-light (#444444). Fallbacks keep the current hexes for
// themes that do not define these tokens.
export function themeTooltipBg(): string {
  return themeText('--color-surface-elevated', '#27272a')
}

export function themeTooltipBorder(): string {
  return themeText('--color-border-light', '#3f3f46')
}
