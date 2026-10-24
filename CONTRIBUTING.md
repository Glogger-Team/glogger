# Contributing to glogger

Thanks for your interest in contributing to glogger! This document covers everything you need to get started.

## Prerequisites

- [Git](https://git-scm.com/downloads)
- [Node.js LTS](https://nodejs.org) (22+)
- [Rust + rustup](https://rustup.rs) + Visual Studio C++ build tools (Windows)
- [npm](https://www.npmjs.com/) (comes with Node.js)

## Getting Started

```bash
git clone https://github.com/Glogger-Team/glogger.git
cd glogger
npm install
npm run tauri dev
```

This starts the app in development mode with hot-reload for the Vue frontend.

## Project Structure

- `src/` — Vue 3 frontend (TypeScript, Tailwind CSS)
- `src-tauri/src/` — Rust backend (Tauri 2, SQLite, log parsing)
- `docs/` — Architecture docs, feature specs, and plans
- `test_data/` — Sanitized game log datasets for integration tests

See [docs/index.md](docs/index.md) for a full documentation map.

## Running Tests

**Rust unit tests** (fast, runs 387+ tests):

```bash
cd src-tauri
cargo test
```

**Frontend type check + build**:

```bash
npm run build
```

**Survey replay integration tests** (slow, requires CDN sample data):

```bash
npm run survey-test
```

## Commit Message Conventions

We use prefixed commit messages for changelog generation:

- `feat:` — New feature or capability
- `fix:` — Bug fix
- `impv:` — Improvement to an existing feature
- `release:` — Release commits (auto-generated, don't use manually)

Examples:

```text
feat: add recipe browser with ingredient search
fix: survey session not ending when leaving area
impv: faster item tooltip loading with cached lookups
```

## Pull Request Process

1. Fork the repo and create a feature branch from `main`
2. Make your changes, following the conventions in [CLAUDE.md](CLAUDE.md) if applicable
3. Run `cargo test` and `npm run build` to verify nothing is broken
4. Open a PR with a clear description of what changed and why
5. PRs are reviewed by the maintainers before merging

## Code Conventions

- **Vue**: Composition API with `<script setup lang="ts">`. Template first, then script block.
- **Rust**: Standard formatting (`cargo fmt`). Tests go in `#[cfg(test)]` modules inline.
- **Layout**: Screen components use `PaneLayout` as the root element.
- **Entity references**: Use shared inline components (`ItemInline`, `NpcInline`, etc.) for game entities.
- **State**: Backend-owned. Game state lives in Rust + SQLite, not in Vue components.
- **Log parsing**: All Player.log parsing goes through `PlayerEventParser` — never parse raw log lines in feature code.

## Architecture Overview

glogger tails Project: Gorgon's `Player.log` in real time, parsing events through a Rust pipeline:

```text
Player.log → PlayerEventParser → Feature Coordinators → SQLite → Vue Frontend
```

The [architecture docs](docs/architecture/) cover the event pipeline, data flow, database schema, and UI patterns in detail.

## Questions?

Open an issue if you're unsure about anything. We're happy to help point you in the right direction.
