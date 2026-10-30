# Widget: Status

**ID:** `context-bar` | **Default size:** Small | **Component:** `ContextBar.vue`

Compact at-a-glance card combining time, environment, and economy info in an organized two-column layout.

## Layout

**Identity section** — character name, server name, and current zone stacked vertically (each independently toggleable). Zone displays the `ShortFriendlyName` from `areas.json` CDN data (falls back to `FriendlyName`, then raw area key). Resolved via `gameDataStore.resolveArea()`.

**Time + Moon section** — two columns side by side:
- **Left column: Times** — Game time, Server time, and Local time stacked vertically (each independently toggleable)
- **Right column: Moon phase** — large phase emoji, phase label, days until next phase, and days until full moon (hidden when already full moon)

**Middle section** — horizontal row:
- **Weather** — current zone weather, live from `AreaConfig Setting weather:` log lines (the game stopped emitting `ProcessSetWeather`)

**Bottom section** — currencies listed vertically with label left and gold-accented amount right (floored to integers). Each currency has its own visibility toggle. "Gold" from game data displays as "Councils" (the in-game name). Currencies are displayed in a fixed order: Councils, Vidaria Renown, Statehelm Renown, Glamour Credits, Live Event Credits, Combat Wisdom, Guild Credits, Red Wing Tokens, Druid Credits, Warden Points, Fae Energy, Blood Oaths, Norala Tokens, Statehelm Demerits.

Sections are separated by horizontal dividers that only render when both adjacent sections are visible.

## Data sources

- **Character name** — from `settingsStore.settings.activeCharacterName`
- **Server name** — from `settingsStore.settings.activeServerName`
- **Current zone** — area key from `gameStateStore.world.area.area_name`, resolved to friendly name via `gameDataStore.resolveArea()` (prefers `short_friendly_name`, falls back to `friendly_name`, then raw key)
- **Game time** — 12 game days per real day (1 game hour = 5 real minutes), anchored to Eastern midnight, from `gameStateStore.gameTime`
- **Server time** — US Eastern (where servers are hosted), from `gameStateStore.serverTime`
- **Local time** — optional, off by default (useful when player isn't in Eastern)
- **Moon phase** — current phase emoji, label, and days until next phase / full moon (via `useMoonPhase` composable)
- **Weather** — current zone weather from `gameStateStore.world.weather`
- **Currencies** — non-zero balances from `gameStateStore.currencies`

Clock refs tick every second in the store; the widget's own local clock re-renders on minute boundaries. Times display without seconds in either 24h or 12h format. The moon section shows a loading placeholder until `get_current_moon_phase` resolves.

## Configurable sections

All toggles are in the gear icon config popover. Preferences persist via `useViewPrefs('widget.context-bar')`.

| Option | Key | Default |
|---|---|---|
| Character Name | `showCharacter` | on |
| Server Name | `showServer` | on |
| Current Zone | `showZone` | on |
| Game Time | `showGameTime` | on |
| Server Time | `showServerTime` | on |
| Local Time | `showLocalTime` | off |
| 24-hour format | `use24h` | on |
| Moon Phase | `showMoon` | on |
| Weather | `showWeather` | on |
| Councils | `currency_gold` | on |
| Vidaria Renown | `currency_vidaria_renown` | on |
| Statehelm Renown | `currency_statehelm_renown` | on |
| Glamour Credits | `currency_glamour_credits` | on |
| Live Event Credits | `currency_liveeventcredits` | on |
| Combat Wisdom | `currency_combat_wisdom` | off |
| Guild Credits | `currency_guildcredits` | on |
| Red Wing Tokens | `currency_redwingtokens` | on |
| Druid Credits | `currency_druidcredits` | on |
| Warden Points | `currency_wardenpoints` | on |
| Fae Energy | `currency_faeenergy` | on |
| Blood Oaths | `currency_blood_oaths` | on |
| Norala Tokens | `currency_norala_tokens` | on |
| Statehelm Demerits | `currency_statehelm_demerits` | on |
Currency display names and sort order are defined in `contextBarPrefs.ts`. Unknown currencies from the character data appear at the end and default to visible.

**Config component:** `ContextBarConfig.vue` — renders section toggles and per-currency toggles in the DashboardCard config popover.

**Shared prefs:** Both `ContextBar.vue` and `ContextBarConfig.vue` share the same reactive pref instance via `useViewPrefs`, so config changes are reflected immediately without a reload.
