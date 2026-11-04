#!/usr/bin/env python3
"""glogger hard-lines enforcement — docs/research/glogger-game-interaction-limits.md.

Python port of check-glogger-constraints.ps1 (stdlib only). See the .ps1 for
the full header comment; behavior (patterns, scopes, output) is identical.
"""

import argparse
import re
import sys
from pathlib import Path

EXCLUDED_DIRS = {".git", "node_modules", "target", "dist"}


class GateReadError(Exception):
    """A selected file could not be read — fail closed, never skip silent."""


FORBIDDEN_PATTERNS = [
    r"OpenProcess\s*\(",
    r"WriteProcessMemory",
    r"ReadProcessMemory",
    r"VirtualAllocEx",
    r"VirtualProtectEx",
    r"CreateRemoteThread",
    r"NtCreateThreadEx",
    r"RtlCreateUserThread",
    r"DebugActiveProcess",
    r"DebugActiveProcessStop",
    r"SetWindowsHookEx\s*\(",
    r"LoadLibrary.*WindowsPlayer",
    r"DllInject",
    # Input synthesis (we never act inside the game)
    r"SendInput\s*\(",
    r"keybd_event\s*\(",
    r"mouse_event\s*\(",
    r"BlockInput\s*\(",
    # Clock manipulation
    r"SetSystemTime\s*\(",
    r"SetLocalTime\s*\(",
    r"NtSetSystemTime",
    r"ZwSetSystemTime",
    # Acting inside the game window via messages
    r"SendMessage.*WindowsPlayer",
    r"PostMessage.*WindowsPlayer",
]

PACKET_PATTERNS = [
    r"WinDivert",
    r"Npcap",
    r"pcapng",
    r"\bpcap\b",
    r"pktmon",
    r"PacketMon",
    r"CapturePacket",
    r"PacketCapture",
    r"tcp\.port|tcpdump|tshark|windump",
]

GAME_PORT_PATTERN = r"9002"

RE_PATTERNS = [
    r"GameAssembly",
    r"global-metadata",
    r"il2cpp_data",
    r"Metadata\.dat",
    r"dump\.cs",
    r"DummyDll",
    r"Cpp2IL",
    r"Il2CppDumper",
    r"il2cpp-dumper",
    r"il2cppdumper",
]

WIRE_PATTERNS = [
    r"LEB128",
    r"ReadLeb128",
    r"WriteLeb128",
    r"GorgonClient",
    r"GorgonProtocolUtils",
    r"ServerCommand\b",
    r"ClientCommand\b",
    r"ReadBytesWithDebugPadding",
]

HERE = Path(__file__).resolve()
DEFAULT_ROOT = HERE.parent.parent
LIMITS_DOC = Path("docs", "research", "glogger-game-interaction-limits.md")


def _excluded(path: Path, root: Path) -> bool:
    try:
        rel = path.relative_to(root)
    except ValueError:
        return True
    return any(part in EXCLUDED_DIRS for part in rel.parts)


def _read_lines(path: Path):
    try:
        with open(path, "r", encoding="utf-8", errors="ignore") as fh:
            return fh.read().splitlines()
    except OSError as exc:
        raise GateReadError(str(path), exc)


def _src_rs_files(root: Path):
    src = root / "src-tauri"
    if not src.is_dir():
        return []
    files = [p for p in src.rglob("*.rs") if p.is_file()]
    files = [p for p in files if not _excluded(p, root)]
    return sorted(files)


def _is_self_bytecode(path: Path) -> bool:
    # Running this script compiles its pattern literals to bytecode; like the
    # script itself, that cache names every forbidden pattern, so it shares
    # the self-scan exclusion (same rationale as $allowlist in the .ps1).
    return (
        path.parent.name == "__pycache__"
        and path.stem.startswith("check-glogger-constraints")
        and path.parent.parent == HERE.parent
    )


def _repo_files(root: Path, allowlist: set):
    try:
        files = [p for p in root.rglob("*") if p.is_file()]
    except OSError:
        return []
    out = []
    for p in files:
        s = str(p)
        if _excluded(p, root):
            continue
        if s in allowlist:
            continue
        if _is_self_bytecode(p):
            continue
        out.append(p)
    return sorted(out)


def check(root: Path):
    violations = []
    root_str = str(root)

    def add(kind, path, line, text):
        violations.append(f"{kind} at {path}:{line}: {text}")

    src_files = _src_rs_files(root)
    src_lines = {p: _read_lines(p) for p in src_files}

    for pat in FORBIDDEN_PATTERNS:
        rx = re.compile(pat, re.IGNORECASE)
        for path in src_files:
            for i, line in enumerate(src_lines[path], 1):
                if rx.search(line):
                    add(f"FORBIDDEN-API '{pat}'", str(path), i, line.strip())

    for pat in PACKET_PATTERNS:
        rx = re.compile(pat, re.IGNORECASE)
        for path in src_files:
            for i, line in enumerate(src_lines[path], 1):
                if rx.search(line):
                    add(f"PACKET-TOOLING '{pat}'", str(path), i, line.strip())

    rx_port = re.compile(GAME_PORT_PATTERN, re.IGNORECASE)
    for path in src_files:
        for i, line in enumerate(src_lines[path], 1):
            if rx_port.search(line):
                add("GAME-PORT-SOCKET", str(path), i, line.strip())

    allowlist = {
        str(HERE),
        str(root / "scripts" / "check-glogger-constraints.ps1"),
        str(root / "scripts" / "check-glogger-constraints.py"),
        str(root / LIMITS_DOC),
    }
    repo_files = _repo_files(root, allowlist)
    repo_lines = {p: _read_lines(p) for p in repo_files}

    for pat in RE_PATTERNS:
        rx = re.compile(pat, re.IGNORECASE)
        for path in repo_files:
            for i, line in enumerate(repo_lines[path], 1):
                if rx.search(line):
                    rel = str(path).replace(root_str, "")
                    add(f"RE-ARTIFACT '{pat}'", rel, i, line.strip())

    for pat in WIRE_PATTERNS:
        rx = re.compile(pat, re.IGNORECASE)
        for path in repo_files:
            if path.name == "Cargo.lock":
                continue
            for i, line in enumerate(repo_lines[path], 1):
                if rx.search(line):
                    rel = str(path).replace(root_str, "")
                    add(f"WIRE-KNOWLEDGE '{pat}'", rel, i, line.strip())

    return violations


def main(argv=None):
    parser = argparse.ArgumentParser(description="Check TOS/CoC hard lines.")
    parser.add_argument("--root", default=str(DEFAULT_ROOT))
    args = parser.parse_args(argv)
    root = Path(args.root).resolve()
    try:
        violations = check(root)
    except GateReadError as exc:
        print(f"constraints: failed to read {exc} — failing closed", file=sys.stderr)
        return 2
    if violations:
        print(
            "HARD-LINE VIOLATIONS (see docs/research/glogger-game-interaction-limits.md):"
        )
        for v in violations:
            print(f"  {v}")
        return 1
    print("OK: repo is fully compliant with the Project: Gorgon TOS/CoC hard lines.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
