---
name: glogger-smoke
description: Live-smoke the glogger desktop app over WebView2 CDP — launch with remote debugging, attach, drive navigateToView, assert render → sort → drill → silence on the real Dev profile.
---

# Glogger Live Smoke

Owns the verbatim in-window visual-QA recipe. For release build/install routine see `skill://glogger-release`; for TOS hard-line patterns see `scripts/check-glogger-constraints.ps1`; for grid exemption semantics see `scripts/check-grid-standards.mjs` (cited, never restated here).

## Gate rows (measure once in parent, paste into briefs, never re-derive)

1. `npm run build` (`vue-tsc --noEmit && vite build`)
2. `cd src-tauri && cargo test --lib` (with exact count)
3. `npm run check:grids`
4. `pwsh scripts/check-glogger-constraints.ps1`

## Recipe

1. **Launch:** `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9222` on `npm run tauri dev`.
2. **Attach:** `browser.open({ app: { cdp_url: 'http://127.0.0.1:9222' } })`.
3. **Dataset:** real `glogger.Dev` DB + CDN v486, characters TwinkleofToes/Dreva; real logs under `AppData\LocalLow\Elder Game\Project Gorgon\` (`Player.log`, `Player-prev.log`). Never a bare preview.
4. **Drive:** via injected `navigateToView({ view, subTab })` off `Symbol(view-navigation)` in the nav button's Vue instance. Do not click labels: keep-alive panes keep hidden screens mounted and labels repeat.
5. **DOM discipline:** `tab.run` has no `document` — DOM work goes in `tab.evaluate`/`page.evaluate` (`tab.waitFor` does see DOM); prefer MutationObserver over CDP round trips.
6. **Assert shape render → sort → drill → silence:** real row counts, header-click sort flips order and back, `rowClick` drills, `tab.errors()` empty after the whole tour.
7. **Throwaway harness `_dtprobe`:** Vite page mounting the real component + app `main.css`, driven over CDP, then deleted. Throwaway profile: copy dev DB + synthetic log under `%TEMP%`, point `db_path`/`log_file_path` there, backup `%APPDATA%\glogger.Dev\settings.json`, restore byte-identical (md5 + mtime check), delete dir.
8. **Precondition trap:** external browser at `localhost:1420` cannot reach Tauri IPC (`invoke` → `transformCallback` undefined) — visual QA must be in-window.
