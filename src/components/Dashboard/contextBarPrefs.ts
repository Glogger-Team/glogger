/**
 * Shared constants for the Status (context-bar) widget preferences.
 * Used by both ContextBar.vue (display) and ContextBarConfig.vue (config popover).
 */

/** Ordered list of known currency keys (lowercase) for display sorting. */
export const CURRENCY_DISPLAY_ORDER = [
  'gold',                // Councils
  'vidaria_renown',
  'statehelm_renown',
  'glamour_credits',
  'liveeventcredits',
  'combat_wisdom',
  'guildcredits',
  'redwingtokens',
  'druidcredits',
  'wardenpoints',
  'faeenergy',
  'blood_oaths',
  'norala_tokens',
  'statehelm_demerits',
]

/** Display name overrides for currency keys from the game data. */
export const CURRENCY_DISPLAY_NAMES: Record<string, string> = {
  gold: 'Councils',
  liveeventcredits: 'Live Event Credits',
  glamour_credits: 'Glamour Credits',
  combat_wisdom: 'Combat Wisdom',
  vidaria_renown: 'Vidaria Renown',
  statehelm_renown: 'Statehelm Renown',
  guildcredits: 'Guild Credits',
  redwingtokens: 'Red Wing Tokens',
  druidcredits: 'Druid Credits',
  wardenpoints: 'Warden Points',
  faeenergy: 'Fae Energy',
  blood_oaths: 'Blood Oaths',
  norala_tokens: 'Norala Tokens',
  statehelm_demerits: 'Statehelm Demerits',
}

/** Human-readable label for a raw currency key (export spelling, any case). */
export function formatCurrencyDisplayName(name: string): string {
  const lower = name.toLowerCase()
  if (CURRENCY_DISPLAY_NAMES[lower]) return CURRENCY_DISPLAY_NAMES[lower]
  return name
    .split('_')
    .map(w => w.charAt(0).toUpperCase() + w.slice(1).toLowerCase())
    .join(' ')
}

/** Known currency pref keys and their default visibility. */
export const CURRENCY_DEFAULTS: Record<string, boolean> = {
  currency_gold: true,
  currency_vidaria_renown: true,
  currency_statehelm_renown: true,
  currency_glamour_credits: true,
  currency_liveeventcredits: true,
  currency_combat_wisdom: false,
  currency_guildcredits: true,
  currency_redwingtokens: true,
  currency_druidcredits: true,
  currency_wardenpoints: true,
  currency_faeenergy: true,
  currency_blood_oaths: true,
  currency_norala_tokens: true,
  currency_statehelm_demerits: true,
}

export interface ContextBarPrefs extends Record<string, unknown> {
  showCharacter: boolean
  showServer: boolean
  showZone: boolean
  showGameTime: boolean
  showServerTime: boolean
  showLocalTime: boolean
  showMoon: boolean
  showWeather: boolean
  showCombat: boolean
  use24h: boolean
  // Per-currency toggles are dynamic: `currency_<lowercase_name>: boolean`
  [key: string]: unknown
}

export const CONTEXT_BAR_DEFAULTS: ContextBarPrefs = {
  showCharacter: true,
  showServer: true,
  showZone: true,
  showGameTime: true,
  showServerTime: true,
  showLocalTime: false,
  showMoon: true,
  showWeather: true,
  showCombat: true,
  use24h: true,
  ...CURRENCY_DEFAULTS,
}
