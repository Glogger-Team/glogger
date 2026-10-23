# Glogger

**[Download Latest Release](https://github.com/cprivitere/glogger-twinkleoftoes/releases)**

A desktop companion app for **[Project: Gorgon](https://projectgorgon.com)**. glogger reads your game's log files in real time and gives you dashboards, analytics, and tools for tracking your gameplay.

## About

- Community fork managed by oddio of arisetsu. 
- Huge shoutout to Super for helping figure out how to operate this stuff. Zenith/Danielout - for creating the base application. To Chompynibblers for the inspiration, sharing your database knowledge, and help finding the necessary tags in the log files to properly implement it. To Derava, your friendship keeps me coming back to the game. May this app continue to serve the entire community on the shoulders of these giants.

## What is glogger?

https://www.youtube.com/watch?v=wXSaIyMzuuA

## Original Owners Info

- Thanks Reyetta, Wogan, Kaeus, Fidge, TwinkleofToes, DisasterGaymer, Cakedydidooda, Lucyfeir and Deradon! Feedback and early testing was super helpful in getting Glogger to where it is now. :D
- Some things only will work with VIP! This is just a limitation of how and where I can access data. The DataBrowser works no matter what though, and it is a very handy. Nothing in here is intentionally broken because of non-vip. I just can only work with what I have.
- It operates on the Player.log, ChatLogs, and the CDN data provided by Citan for app developers. (Thanks Citan!) No shady poking in to running processes or anything like that.
- There is no server for this app to talk to; everything runs locally to try and keep it as speedy and responsive as possible. Has to download some Citan-provided data on boot and with new patches, and pokes github to check for app updates.

I apologize for bad UI/UX/programming - I'm a game designer by trade, not a software engineer. I'm more at home with excel and scripts than app development, so I'm learning as I go.

Maintained by [cprivitere (TwinkleofToes)](https://github.com/cprivitere). Built on [crisp-oddio/glogger-oddio](https://github.com/crisp-oddio/glogger-oddio) by oddio — its features, fixes and release pipeline carry over here — originally [danielout/glogger](https://github.com/danielout/glogger) by Daniel Auchenpaugh.

## Why beta?

- We still don't have 100% capture rate on items going in/out of inventory sometimes. It is a stubborn bug, and I'm trying to track it down. It is pretty dang good, but not 100%
- There is a lot of UI/UX inconsistencies that I just haven't rooted out yet.
- Lots of little leftover minor bugs to fix here and there.
- A few key features I want to implement before it feels whole.
- but the biggest issue is _no documentation_ so far! It very much is in a "play with it and figure it out" state, and a lot of the screens _really_ need some guidance to tell you how to use them.

## Initial Setup

Instructions available in short (<2 minutes) video form here. Featuring Cheb! https://www.youtube.com/watch?v=9BhzbM92rv4

1. Go to your VIP settings, export your character and storage JSONs. (If you've got alts, it is helpful to do them as well!)
2. Go to GUI, scroll to the bottom, and turn on the chat logging! This will give you a chat history browser and allow you to set watchwords! Also turning on combat logging doesn't do a ton, but does allow you to see death reports on what killed you. (I have no plans to try to make DPS meters or anything.)
3. Start Glogger! The setup wizard should walk you through the very fast setup with hopefully minimal issues. 

## Features

### Dashboard
Customizable widget grid with drag-to-reorder. Widgets include live skill tracking, item gain/loss feeds, currency changes, zone info with NPC favor, death tracker, watchword alerts, Words of Power capture (with wiki-sourced category/level grouping and CSV export), trip planner, Statehelm gifting progress (favor-aware, so maxed NPCs fall off and the next-best backfills), Vendor Council tracking (Available + Earned tabs — current-period and lifetime councils earned per NPC), Combat Wisdom tracker (session totals plus per-monster reuse-cooldown countdowns learned from your own kill history), XP Rate / Prodigy Tracker, critical resource tracking, arena tracking (Earthblood arena kill stats), mushroom farming and milking timers, garden/harvest almanacs, roulette, stat tracking, and personal notes.

### Character
- **Skills** — Live XP tracking with session gains, progression charts, and CDN-enriched skill details (including snapshot-known abilities imported from character reports)
- **Stats** — Character report import and snapshot management
- **NPCs** — Favor progression, services, and gift preferences
- **Quests** — Personalized quest reference with requirement eligibility checking
- **Gourmand** — Food tracking for the Gourmand skill
- **Statehelm** — Weekly gift tracker with NPC services and filters
- **Build Planner** — Combat build planning with mod, ability, and combat point management
- **Account** — Aggregate view across all characters on a server

### Inventory
- **Live Inventory** — Real-time tracking of items as they enter and leave your inventory
- **Snapshots** — Point-in-time inventory browsing from `/outputitems` exports
- **Storage Vaults** — Area-grouped vault browser with capacity tracking

### Crafting
- **Projects** — Multi-recipe material breakdowns with pickup lists, shopping lists, and live crafting detection
- **Price Helper** — Pricing mode with customer-provides and fee configuration
- **XP Leveling Optimizer** — Find the best recipes to level a crafting skill
- **Crafting History** — Log of everything you've crafted
- **Work Orders** — Work order tracking
- **Cook's Helper** — Cooking-specific crafting assistant
- **Brewery** — Brewing discovery journal with per-player effect mapping and ingredient matrix
- **Dynamic Items** — Configure wildcard ingredient slot preferences

### Economics
- **Market Prices** — Player-maintained price database
- **Farming** — Session-based profitability tracking, with a per-item drop-rate hover (active session and history) and skinning/butchering harvest detail (skill level, equipment bonus, anatomy level at the time of harvest)
- **Drop-Rate Database** — Community-sharable kill/loot dataset under the Farming tab. Tracks every lootable corpse search (not just chat kill messages), is zone-aware (the same monster drops differently in different areas) and combat-loadout-aware (drop tables filtered by your equipped combat-skill pair, with an "unattributed" baseline for legacy/imported data). CSV import/export (auto-detects aggregated or raw per-kill event sheets), permanent merged imports, and a searchable Monsters/Items/Harvested browser
- **Surveying** — Full survey session tracker with analytics, session history, and loot attribution (chat-log-authoritative, so it works even when verbose Player.log item logging doesn't reach you)
- **Stall Tracker** — Shop log parsing with sales, revenue, and inventory analytics, plus a **Trends** tab charting sales over time

### Chat Logs
Full-text search across all chat channels. Browse by channel, tells, party, nearby, guild, or system. Watchword alerts let you set up keyword-based notifications for trade chat and more. Item references in chat are linked to the data browser.

### Data Browser
Browse the full Project: Gorgon CDN dataset — items, skills, abilities, recipes, quests, NPCs, enemies, areas, effects, lorebooks, titles, achievements, and treasure effects. Hover tooltips, click-to-navigate, search, and favorites. Enemy detail pages show Kill Stats and Harvest Stats (what a monster skins/butchers into) pulled from your own logged data.

### Unified Search
Scryfall-inspired structured query syntax across 11 searchable categories. Quick search overlay with `Ctrl+F` (or `Ctrl+K`).

### Trip Planner
Multi-zone route planner with teleport-aware pathfinding. Respects bind locations and known teleport abilities.

## Screenshots

| | |
|---|---|
| ![Dashboard](docs/screenshots/readme/dashboard.webp) | ![Build Planner](docs/screenshots/readme/build-planner.webp) |
| Dashboard — widget grid | Character → Build Planner |
| ![Vaults](docs/screenshots/readme/vaults.webp) | ![Crafting Projects](docs/screenshots/readme/crafting-projects.webp) |
| Inventory → Vaults | Crafting → Projects |
| ![Farming Database](docs/screenshots/readme/farming-database.webp) | |
| Economics → Farming → Database | |

## Download

Grab the latest release for your platform:

**[Download Latest Release](https://github.com/cprivitere/glogger-twinkleoftoes/releases)**

| Platform | Installer |
|----------|-----------|
| Windows  | `.exe` (NSIS installer) |
| macOS    | `.dmg` (Apple silicon; **unsigned** — right-click → Open on first launch, or clear the quarantine flag: `xattr -d com.apple.quarantine /Applications/glogger.app`) |
| Linux    | `.deb`, `.AppImage`, `.flatpak` |

Release builds include automatic update checking — glogger will notify you when a new version is available.

## Building from Source

### Prerequisites

- [Node.js](https://nodejs.org) (24, npm only)
- [Rust + rustup](https://rustup.rs)
- Platform-specific build tools:
  - **Windows:** Visual Studio C++ build tools
  - **macOS:** Xcode command line tools
  - **Linux:** `webkit2gtk`, `libappindicator3`, `librsvg2`, `patchelf`

### Setup

```bash
git clone https://github.com/cprivitere/glogger-twinkleoftoes.git
cd glogger-twinkleoftoes
npm install
npm run tauri dev
```

### Build a release binary

```bash
npm run tauri:build
```

### Run tests

```bash
cd src-tauri && cargo test --lib
```

## Tech Stack

- **[Tauri 2](https://tauri.app)** — Rust backend with OS webview
- **[Vue 3](https://vuejs.org)** + **[Pinia](https://pinia.vuejs.org)** — Frontend UI and state management
- **[Tailwind CSS 4](https://tailwindcss.com)** — Styling
- **SQLite** (via rusqlite) — Local data persistence
- **tokio** — Async log file watching and network requests

## How It Works

glogger tails Project: Gorgon's `Player.log` and `Chat.log` files in real time. A Rust event parser processes raw log lines into structured events, which flow through feature-specific coordinators into a SQLite database. The Vue frontend renders live dashboards from that data.

```
Player.log → PlayerEventParser → Feature Coordinators → SQLite → Vue Frontend
```

Game reference data (items, skills, recipes, etc.) is downloaded from the public Project: Gorgon CDN and cached locally.

## Contributing

Contributions are welcome! See [CONTRIBUTING.md](CONTRIBUTING.md) for setup instructions, conventions, and how to submit a PR.

## License

This project is licensed under the [GNU General Public License v3.0](LICENSE).
