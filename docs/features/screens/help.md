# Help

The Help overlay is the central reference hub, accessible via the `?` button in the top-right of the menu bar. It renders as a centered modal popup (not a full-screen view) with a backdrop, matching the DataBrowserOverlay pattern.

## Layout

Modal overlay (85vw wide, 80vh tall) with a two-column interior: vertical tab navigation on the left and scrollable content area on the right. Closes on Escape or backdrop click.

## Components

- **`HelpOverlay.vue`** (`src/components/Help/HelpOverlay.vue`) — Modal wrapper with Teleport, backdrop, transitions, and tab navigation.
- Tab components live in `src/components/Help/`.

## Tabs

### About (default)
**`AboutTab.vue`** (`src/components/Settings/AboutTab.vue`) — Shared component, also used historically from Settings. Shows app name/version, description, special thanks, tech stack, and Buy Me a Coffee link.

### Help
**`HelpSetupTab.vue`** — Setup instructions for new users: game data path, chat logging, character/inventory JSON imports. Also lists keyboard shortcuts and useful in-game commands.

Helper components:
- **`SetupStep.vue`** — Numbered step card with title and slotted description.
- **`ShortcutRow.vue`** — Keyboard shortcut display row.

### Glogger Changelog
**`ChangelogTab.vue`** — Fetches release notes from the GitHub releases (`Glogger-Team/glogger`) via the `fetch_github_releases` Tauri command. Renders markdown release bodies with basic formatting (headers, bold, code, lists). Loads on mount with retry on failure.

### Known Issues
**`KnownIssuesTab.vue`** — Known bugs, limitations, and tips. Extracted from the original `HelpView`. Uses `IssueCard.vue` for individual entries with severity indicators (bug/limitation/cosmetic).

### PG News
**`PgNewsTab.vue`** — Fetches Project: Gorgon update notes from `client.projectgorgon.com/news.txt` via the `fetch_pg_news` Tauri command. Parses Unity rich text tags (`<size>`, `<color>`, `<b>`, `<i>`) into styled HTML. Splits entries on the title pattern and renders each as a collapsible card.

## Backend Commands

Two Tauri commands in `src-tauri/src/external_fetch.rs` support the network tabs:

- **`fetch_github_releases`** — Returns up to 20 releases from the `glogger` GitHub repo via the GitHub API.
- **`fetch_pg_news`** — Returns the raw text content from the PG news URL.

Both use `reqwest` with a custom user agent, matching the pattern in `update_check.rs`.
