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

1. **Launch:** `export WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9222` **in the same command** as `npm run tauri dev` (the env var MUST be set before the WebView2 renderer spawns — a relaunch without it leaves no CDP endpoint, and `export` on a prior call does not carry over). Same-shell form: `export WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9222; npm run tauri dev`.
2. **Attach:** `browser.open({ name: '<label>', app: { cdp_url: 'http://127.0.0.1:9222' } })`. Verify `http://127.0.0.1:9222` is LISTENING (netstat) before attaching — if not, stale `msedgewebview2.exe` renderers from a force-killed instance are holding the old session: `taskkill /F /IM msedgewebview2.exe`, then restart the app (not just Vite) so a fresh renderer picks up the flag.
3. **Dataset:** real `glogger.Dev` DB + CDN v486, characters TwinkleofToes/Dreva; real logs under `AppData\LocalLow\Elder Game\Project Gorgon\` (`Player.log`, `Player-prev.log`). Never a bare preview.
4. **Drive:** via injected `navigateToView({ view, subTab })` off `Symbol(view-navigation)` in the nav button's Vue instance. Do not click labels: keep-alive panes keep hidden screens mounted and labels repeat.
5. **DOM discipline:** `tab.run` has no `document` — DOM work goes in `tab.evaluate`/`page.evaluate` (`tab.waitFor` does see DOM); prefer MutationObserver over CDP round trips. In `tab.evaluate`, avoid arrow-function IIFEs and regex literals in the evaluated string — the CDP string pipeline intermittently rejects them (`Invalid regular expression`, `Invalid or unexpected token`); use `function(){}`, string `indexOf`, and pre-built regexes.
6. **Assert shape render → sort → drill → silence:** real row counts, header-click sort flips order and back, `rowClick` drills, `tab.errors()` empty after the whole tour.
7. **Throwaway harness `_dtprobe`:** Vite page mounting the real component + app `main.css`, driven over CDP, then deleted. Throwaway profile: copy dev DB + synthetic log under `%TEMP%`, point `db_path`/`log_file_path` there, backup `%APPDATA%\glogger.Dev\settings.json`, restore byte-identical (md5 + mtime check), delete dir.
8. **Precondition trap:** external browser at `localhost:1420` cannot reach Tauri IPC (`invoke` → `transformCallback` undefined) — visual QA must be in-window.

## CDP blind spots (do not fight them — route around)

- **Native file/folder dialogs** (`@tauri-apps/plugin-dialog` `open`) are OS windows owned by the WebView2 host process: CDP keyboard/mouse goes to the page, not the dialog. Ask the user to pick the path in the dialog, then observe the result in-window. Never leave a hijacked second tab on `dev-panel.html` — both CDP tabs share the one WebView2 page, so navigating one steals the user's main window; restore it to `/` when done.
- **Tauri IPC is invisible from CDP eval:** `window.__TAURI_INTERNALS__` is `undefined` in the CDP JS context (bridge is injected per-isolated-world), so backend commands cannot be invoked through the page. Drive the real UI instead; for command-level verification without a dialog, use the dev-panel entry (`/dev-panel.html` in a second CDP tab, restored after) whose bundle context holds working `invoke`/`listen` imports via `/node_modules/.vite/deps/@tauri-apps_api_{core,event}.js`.
- **Stale-renderer reuse:** force-killed `glogger.exe` instances leave `msedgewebview2.exe` renderers alive; a relaunched app may attach to the OLD renderer (no CDP flag, old page). Symptom: port 9222 not listening despite the flag. Fix: kill the renderers, restart the app process.

## Dev-DB wipe (clean-room retest)

When the user asks to wipe/retry, or a retest needs a known-empty baseline — NEVER on an assumption, always on request or with explicit approval:
1. `taskkill /F /IM glogger.exe` (DB is file-backed + WAL; no clean shutdown needed) and confirm the process is gone.
2. Move (never delete) `glogger.db` + `-wal` + `-shm` from `%APPDATA%\glogger.Dev\` to `db-backup-<timestamp>\` beside them. Keep `settings.json` and `data/` (CDN cache) so no re-download is needed.
3. Relaunch per the Recipe above (flag + CDP), verify `Database initialized` + migrations clean in the terminal log.
