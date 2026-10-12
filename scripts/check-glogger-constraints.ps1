#!/usr/bin/env pwsh
# glogger hard-lines enforcement — docs/research/glogger-game-interaction-limits.md
# Full-compliance mode: glogger commits to follow the Project: Gorgon TOS and
# Code of Conduct completely, with no defensible carve-out reasoning. Fails
# (exit 1) when any prohibited artifact or API appears in the repo.
#
# Scope: src-tauri/*.rs (runtime code), docs/, scripts/, and the repo tree.
# Runs in two places, so a skipped local hook can never let a violation ship:
#   - .githooks/pre-push (local, PowerShell 5.1; GLOGGER_SKIP_CONSTRAINTS=1
#     skips it for emergencies, loudly)
#   - .github/workflows/release.yml -> validate job (pwsh, before any build)
# If a legitimate false positive appears, reword the code/comment rather than
# weakening this list. Changes to prohibited patterns require a TOS-doc
# change-control note in the same commit.

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$src = Join-Path $root 'src-tauri'

$violations = @()
function Add-Violation([string]$kind, [string]$path, [int]$line, [string]$text) {
    $script:violations += "${kind} at ${path}:${line}: ${text}"
}

# Path filters must work on Windows and on Linux (CI runs this on
# ubuntu-latest): a backslash-only pattern would not exclude `…/target/…`
# there, and would scan build output / node_modules.
$excludedDirPattern = '[/\\](\.git|node_modules|target|dist)[/\\]'

# ── 1. Game-process access / injection / memory APIs (ACTk + TOS §5) ────────
$forbiddenPatterns = @(
    'OpenProcess\s*\(',
    'WriteProcessMemory',
    'ReadProcessMemory',
    'VirtualAllocEx',
    'VirtualProtectEx',
    'CreateRemoteThread',
    'NtCreateThreadEx',
    'RtlCreateUserThread',
    'DebugActiveProcess',
    'DebugActiveProcessStop',
    'SetWindowsHookEx\s*\(',
    'LoadLibrary.*WindowsPlayer',
    'DllInject',
    # Input synthesis (we never act inside the game)
    'SendInput\s*\(',
    'keybd_event\s*\(',
    'mouse_event\s*\(',
    'BlockInput\s*\(',
    # Clock manipulation
    'SetSystemTime\s*\(',
    'SetLocalTime\s*\(',
    'NtSetSystemTime',
    'ZwSetSystemTime',
    # Acting inside the game window via messages
    'SendMessage.*WindowsPlayer',
    'PostMessage.*WindowsPlayer'
)
foreach ($pat in $forbiddenPatterns) {
    $hits = Get-ChildItem -Path $src -Recurse -Filter '*.rs' |
        Where-Object { $_.FullName -notmatch $excludedDirPattern } |
        Select-String -Pattern $pat
    foreach ($hit in $hits) {
        Add-Violation "FORBIDDEN-API '$pat'" $hit.Path $hit.LineNumber $hit.Line.Trim()
    }
}

# ── 2. Traffic-capture tooling (TOS §5 bullet 2, §8) — prohibited by name ───
#     glogger ships no capture component and never touches the game port.
#     The literal names below are the known tool identifiers; they are data,
#     not documentation — this file is excluded from its own scan.
$packetPatterns = @(
    'WinDivert',
    'Npcap',
    'pcapng',
    '\bpcap\b',
    'pktmon',
    'PacketMon',
    'CapturePacket',
    'PacketCapture',
    'tcp\.port|tcpdump|tshark|windump'
)
foreach ($pat in $packetPatterns) {
    $hits = Get-ChildItem -Path $src -Recurse -Filter '*.rs' |
        Where-Object { $_.FullName -notmatch $excludedDirPattern } |
        Select-String -Pattern $pat
    foreach ($hit in $hits) {
        Add-Violation "PACKET-TOOLING '$pat'" $hit.Path $hit.LineNumber $hit.Line.Trim()
    }
}
# Any socket touching the game's port — even reads. Excludes target/ build
# output (generated crate code contains unrelated constants).
$gameSock = Get-ChildItem -Path $src -Recurse -Filter '*.rs' |
    Where-Object { $_.FullName -notmatch $excludedDirPattern } |
    Select-String -Pattern '9002'
foreach ($hit in $gameSock) {
    Add-Violation 'GAME-PORT-SOCKET' $hit.Path $hit.LineNumber $hit.Line.Trim()
}

# ── 3. Reverse engineering / client-derived artifacts (TOS §5 bullet 1) ─────
#     No game binaries, metadata, dump outputs, or decompiled artifacts in the
#     repo — even in docs. NOTE: this script names the forbidden patterns, so
#     it (and the limits doc, which quotes the TOS) are excluded from their own
#     scan via the allowlist below.
$allowlist = @($PSCommandPath, [IO.Path]::Combine($root, 'docs', 'research', 'glogger-game-interaction-limits.md'))

function Get-RepoFiles {
    Get-ChildItem -Path $root -Recurse -File -ErrorAction SilentlyContinue |
        Where-Object {
            $_.FullName -notmatch $excludedDirPattern -and
            $allowlist -notcontains $_.FullName
        }
}

$rePatterns = @(
    'GameAssembly',
    'global-metadata',
    'il2cpp_data',
    'Metadata\.dat',
    'dump\.cs',
    'DummyDll',
    'Cpp2IL',
    'Il2CppDumper',
    'il2cpp-dumper',
    'il2cppdumper'
)
foreach ($pat in $rePatterns) {
    $hits = Get-RepoFiles |
        Select-String -Pattern $pat -ErrorAction SilentlyContinue
    foreach ($hit in $hits) {
        $rel = $hit.Path.Replace($root, '')
        Add-Violation "RE-ARTIFACT '$pat'" $rel $hit.LineNumber $hit.Line.Trim()
    }
}

# ── 4. Wire-protocol knowledge (§5 "packet interception" derivative) ────────
#     No wire framing, command ids, or protocol-decoder logic anywhere in the
#     repo. The shipped parsers read Player.log text only.
$wirePatterns = @(
    'LEB128',
    'ReadLeb128',
    'WriteLeb128',
    'GorgonClient',
    'GorgonProtocolUtils',
    'ServerCommand\b',
    'ClientCommand\b',
    'ReadBytesWithDebugPadding'
)
foreach ($pat in $wirePatterns) {
    $hits = Get-RepoFiles |
        Where-Object { $_.Name -ne 'Cargo.lock' } |
        Select-String -Pattern $pat -ErrorAction SilentlyContinue
    foreach ($hit in $hits) {
        $rel = $hit.Path.Replace($root, '')
        Add-Violation "WIRE-KNOWLEDGE '$pat'" $rel $hit.LineNumber $hit.Line.Trim()
    }
}

if ($violations.Count -gt 0) {
    Write-Host "HARD-LINE VIOLATIONS (see docs/research/glogger-game-interaction-limits.md):" -ForegroundColor Red
    $violations | ForEach-Object { Write-Host "  $_" -ForegroundColor Red }
    exit 1
}

Write-Host "OK: repo is fully compliant with the Project: Gorgon TOS/CoC hard lines." -ForegroundColor Green
exit 0
