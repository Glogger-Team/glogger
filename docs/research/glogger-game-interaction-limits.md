# glogger × Project Gorgon — hard lines (full TOS/CoC compliance)

Rules for every feature that touches the game, its data, or its traffic.

**Commitment:** glogger follows the Project: Gorgon Terms of Service (v3.0,
effective 2026-04-22) and Code of Conduct (v3.0, binding, incorporated into
the TOS) **completely and without exception**. We do not use legal carve-outs,
"defensible space" reasoning, or personal-use arguments to justify exceptions.
Where the TOS prohibits a category by name, glogger simply does not do it —
regardless of what a court might allow.

Rationale: a ban costs the user their account; glogger is user-distributed
software, so a violation by one user can tar the whole tool. These lines
exist so no future session has to re-derive them — and so code review can
check against them mechanically.

Sources of truth (re-read when this doc feels stale):
- TOS v3.0: https://projectgorgon.com/terms.html — §5 (License Grant
  prohibitions), §8 (Mods, Tools and Anti-Cheat), §4.6 (Unauthorized
  Transactions), §19 (Termination).
- Code of Conduct v3.0 (binding, part of the TOS):
  https://projectgorgon.com/conduct.html — §4.C (Cheating, Exploitation, and
  Security Violations), §4.G (Commercial Misuse).
- Third-party CDN data terms: https://cdn.projectgorgon.com/v486/data/
  (governs everything glogger ingests from the game-data CDN).
- Privacy Policy v3.1: https://projectgorgon.com/privacy.html

**The one sentence:** glogger reads only what the user's own game client
already wrote to the user's own disk, plus Elder Game's own public CDN for
third-party tools. Everything else about the game is off-limits.

## 1. What the TOS names, and what we therefore never do

| TOS/CoC clause | Verbatim prohibition | glogger rule — absolute |
|---|---|---|
| §5 bullet 1 | "Reverse engineer, decompile, disassemble, modify, adapt, or create derivative works from the Game client or Services" | **Never decompile, disassemble, or dump the client.** No client-binary analysis of any kind, no dump outputs anywhere, ever — even "offline", even "personal use", even in scratch dirs outside the repo. Nothing derived from the client enters glogger. |
| §5 bullet 2 | "Use bots, macros, scripts, automation, data mining tools, packet interception tools, memory editing tools, or any unauthorized software or hardware in connection with the Services" | **No packet interception in any form.** glogger ships no capture component, never bundles or recommends capture drivers, never starts or reads traffic captures, never decodes wire protocols, never documents wire framing/command ids. No wire-reading features of any kind — glogger must not depend on, encourage, or integrate with interception. |
| Conduct §4.C bullet 3 | "Reverse engineering, tampering with, intercepting, or modifying game data, communications, or technical protections" | Same as both rows above; also **never touch** the client's own telemetry, update checks, or technical protections. |
| §5 bullet 3 | "Operate, assist, develop, advertise, or use private servers, emulators, cheats, exploits, or unauthorized modifications" | No private-server/emulator compatibility, no exploit documentation or tooling, ever. |
| §5 bullet 4 | "Exploit the Game or Services for commercial purposes without Elder Game's prior written consent" | glogger stays free. Never monetize game-derived data: no premium tiers on game-derived features, no ads against game data, no selling "market intelligence". |
| §4.6 + Conduct §4.G | "Unauthorized … selling, exchanging … accounts, virtual currency, items …"; "real money trading" | Never facilitate RMT: no seller-contact flows, no trade matchmaking, no offer/bid aggregation routing to human traders. |
| §5 bullet 6 | "Use multiple accounts simultaneously to gain an unfair gameplay advantage" | Multi-client **observation** is fine (glogger already watches multiple logs); multi-**play** coordination never: no cross-account actions, no shared queues, no input relay. |
| §5 bullet 5 | "Circumvent, disable, or interfere with technical protection measures, access controls, or security features" | Never patch/bypass/avoid ACTk, never write game files, never block the client's own telemetry or updates, never probe game servers outside the running client. |
| TOS §8 | "unauthorized overlays that affect gameplay … any tool intended to alter, automate, manipulate, intercept, or gain unfair advantage" | glogger draws only in its own windows. Never an in-game HUD, never an overlay over the game window, never input automation. Features stay informational: only data the user's own client already rendered or logged. |
| TOS §11 + Conduct §4.B | privacy: "sharing another person's private or personally identifying information"; "attempting to obtain private information through … technical abuse, or unauthorized access" | Observations store only what the user's own client displayed publicly (owner name, prices). Never de-anonymize cross-character identities, never export other players' activity beyond what the user's own session rendered, never aggregate other players' data across users. |
| Conduct §4.E | "facilitate fraud, scams, deceptive practices" | Observations are timestamped "as seen at" — never presented as authoritative/verified market truth. |
| TOS §9 | live service, anything can change anytime | Shipped features tolerate schema changes overnight; never assume old-version CDN files exist (only 3–4 versions persist). |
| CDN terms | attribution + restriction rights | Attribution present: `src/components/Help/AboutTab.vue` ("Some portions copyright 2026 Elder Game, LLC.") — keep current. Elder Game may restrict usage by individual/purpose; comply immediately if asked. |

## 2. What ACTk additionally detects (belt-and-suspenders; the TOS rows above already forbid these)

| ACTk detects | Hard line | Check |
|---|---|---|
| Process injection | **NEVER** open a handle to `WindowsPlayer.exe`, never inject a DLL/thread | Grep: `OpenProcess`, `WriteProcessMemory`, `VirtualAllocEx`, `CreateRemoteThread`, `ReadProcessMemory`, `DebugActiveProcess`, `NtCreateThreadEx`, `SetWindowsHookEx`, `LoadLibrary.*WindowsPlayer`, `DllInject` — zero hits in `src-tauri/` |
| Memory tampering | **NEVER** read or write game memory | Same grep list |
| Speed/time manipulation | **NEVER** hook/skew/fake the game's clocks; never change system time while the game runs | Grep: `SetSystemTime`, `SetLocalTime`, `NtSetSystemTime`, `timeBeginPeriod` — zero hits |
| Module integrity | **NEVER** patch/modify/replace files in the game install directory | Grep: game-install paths on write sides — zero hits |
| Input automation | **NEVER** synthesize input into the game (`SendInput`/`keybd_event`/`PostMessage` to the game) | Same grep list |

No release merges without a green `scripts/check-glogger-constraints.ps1`
(§6).

## 3. What we DO allow

- **Reading files the user's own client wrote to the user's own disk:**
  `Player.log`, `Chat.log`, our own SQLite DB, settings files.
- **Reading pixels of the user's own screen** (GDI `BitBlt` of the game
  window) for OCR — passive, no game-process handles, no input, data the user
  already sees. (Screen-scraping is not a TOS-named category; it reads the
  framebuffer of the user's own desktop session.)
- **HTTP GETs to Elder Game's public CDN for third-party tools**
  (`cdn.projectgorgon.com`) — explicitly offered for this purpose, with
  attribution. No other Elder Game endpoints. No authentication, no session
  tokens, no game-server connections of any kind outside the running client.
- **HTTP to glogger's own services** for glogger's own data.

## 4. History note — an earlier draft (removed)

An earlier draft of this work explored approaches that §1 prohibits. On
reading the actual TOS text, that work was **removed completely** — the
scratch directory, the research doc, the tooling, and incidental
client-internal references in test fixtures. Phases A (guided capture panel
from logs) and B (screen OCR) remain the shipped approach for stall pricing
— they need nothing the TOS prohibits. Do not reintroduce any approach from
the prohibited list; this section exists so future sessions don't rediscover
the same idea and redo the same mistake.

## 5. What else is *definitely* safe (feature ideas within the lines)

These need nothing the TOS prohibits; candidates, not commitments:

1. **Screen-region OCR beyond stalls** — any UI the user can already see
   (bank, storage vaults, character sheet, bazaar). Same `BitBlt`+Tesseract
   path as Phase B.
2. **Window-title / foreground-app tracking** — passive "what was I doing at
   8pm" session timelines.
3. **Own-audio event tagging** — VAD/keyword spotting on the user's own
   mic or game audio output. Own-device audio.
4. **Clipboard watch (opt-in)** — ingest copy-pasted item links the game
   emits. User-consented, passive.
5. **GDI window layout snapshots** — inventory slot positions for smarter
   OCR region discovery.
6. **Log-side signals beyond stalls** — everything in Player.log we don't
   parse yet (the game's own `Process*` stream is the client talking to
   itself in a text file it wrote; reading it is what glogger already does).
7. **Multi-client observation** — watch several own-client logs at once.
8. **Game-folder telemetry (own files)** — watch the game's own logs/
   screenshots folders for new files. Read-only, user-consented.
9. **Pure-DB analytics** — wealth curves, XP rates, loot tables, market
   history from glogger's own data.

Explicitly off this list (rejected, do not propose): packet-level anything,
protocol analysis of the game's network traffic, binary/memory analysis of
the client, any automation of gameplay.

## 6. Enforcement: `scripts/check-glogger-constraints.ps1`

Runs in two places, so skipping one cannot ship a violation:

- **Every push** from a clone with the hooks installed —
  `core.hooksPath=.githooks` (`.githooks/pre-push`; `scripts/setup-hooks.sh`
  sets it). Opt-in per clone, so check it after a fresh clone.
- **Every release**: the `validate` job of `.github/workflows/release.yml`
  runs the scanner with `pwsh` before any platform builds, so a tag push that
  violates a hard line fails the release.

In full-compliance mode the scanner checks:

- §2's forbidden APIs (`src-tauri/*.rs`),
- **capture-tooling identifiers** in `src-tauri/*.rs` (driver and packet-tool
  names — see the script for the list), plus any game-port socket reference,
- **client-derived artifacts** across the whole repo (client binaries,
  metadata files, dump outputs, analysis-tool names) — including docs and
  test fixtures,
- **wire-protocol knowledge** across the whole repo (framing encodings,
  client/network class names, protocol reader identifiers).

Verified: green on the compliant repo, red on probe violations. Skippable
only with `GLOGGER_SKIP_CONSTRAINTS=1` (prints a warning — never silent).
`Cargo.lock` is excluded from the wire-pattern scan (an unrelated
font-decoding crate shares a name fragment); the checker script excludes
itself (it must name the patterns it greps for). Raw log fixtures in
`test_data/` are the game's own output; incidental client-internals lines
were stripped from them (the parser never reads those lines — verified by
the full test suite, 582 passing).

## 7. Change-control for this doc

There is no carve-out reasoning in this doc to relax. Any proposal to touch
a prohibited category must first argue **to Elder Game** (written consent per
§5 bullet 4's "prior written consent" model) — not to this repo. Absent
that, the answer is no. If the TOS itself changes, update this doc from the
new text, never from memory.
