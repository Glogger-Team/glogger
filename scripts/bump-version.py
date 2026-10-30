"""Bump the version across all project files.

Port of bump-version.sh (stdlib only, works in cmd/PowerShell/git-bash alike).
Writes are exact-string replacements, never json.dump, so file formatting
never churns.

Usage:
    python scripts/bump-version.py <new-version>
    python scripts/bump-version.py patch|minor|major

Examples:
    python scripts/bump-version.py 0.2.0
    python scripts/bump-version.py patch    # 0.1.2 -> 0.1.3
    python scripts/bump-version.py minor    # 0.1.2 -> 0.2.0
    python scripts/bump-version.py major    # 0.1.2 -> 1.0.0
"""

import argparse
import re
import shutil
import subprocess
import sys
from pathlib import Path

VERSION_RE = re.compile(r'"version"\s*:\s*"(\d+\.\d+\.\d+)"')
EXPLICIT_RE = re.compile(r"^\d+\.\d+\.\d+$")


def replace_once(path, old, new):
    """Replace the first occurrence of old with new; exit 1 if absent."""
    # Read/write as bytes: text-mode newline translation would turn CRLF
    # files into LF (read) or double CRs (write with newline="\r\n").
    text = path.read_bytes().decode("utf-8")
    if old not in text:
        print(f"Error: expected string not found in {path}: {old}")
        return False
    path.write_bytes(text.replace(old, new, 1).encode("utf-8"))
    return True


def replace_regex(path, pattern, replacement):
    """Replace the first regex match; exit 1 if absent."""
    text = path.read_bytes().decode("utf-8")
    new_text, count = re.subn(pattern, replacement, text, count=1)
    if count == 0:
        print(f"Error: expected pattern not found in {path}: {pattern}")
        return False
    path.write_bytes(new_text.encode("utf-8"))
    return True


def main():
    parser = argparse.ArgumentParser(
        description="Bump the version across all project files."
    )
    parser.add_argument("input", help="<new-version> | patch | minor | major")
    args = parser.parse_args()

    root = Path(__file__).resolve().parent.parent
    tauri_conf = root / "src-tauri" / "tauri.conf.json"
    tauri_release_conf = root / "src-tauri" / "tauri.release.conf.json"
    tauri_experimental_conf = root / "src-tauri" / "tauri.experimental.conf.json"
    package_json = root / "package.json"
    cargo_toml = root / "src-tauri" / "Cargo.toml"

    match = VERSION_RE.search(tauri_conf.read_text(encoding="utf-8"))
    if not match:
        print(f"Error: could not read current version from {tauri_conf}")
        return 1
    current = match.group(1)
    print(f"Current version: {current}")

    cur_major, cur_minor, cur_patch = (int(x) for x in current.split("."))
    kind = args.input
    if kind == "patch":
        new_version = f"{cur_major}.{cur_minor}.{cur_patch + 1}"
    elif kind == "minor":
        new_version = f"{cur_major}.{cur_minor + 1}.0"
    elif kind == "major":
        new_version = f"{cur_major + 1}.0.0"
    elif EXPLICIT_RE.match(kind):
        new_version = kind
    elif re.match(r"^[0-9]", kind):
        print("Error: Version must be in semver format (e.g., 1.2.3)")
        return 1
    else:
        print(
            f"Error: Unknown argument '{kind}'. Use patch, minor, major, "
            "or a semver string."
        )
        return 1

    print(f"Bumping to:      {new_version}")

    # 1. tauri.conf.json - version field
    if not replace_once(
        tauri_conf,
        f'"version": "{current}"',
        f'"version": "{new_version}"',
    ):
        return 1

    # 2. tauri.conf.json - dev window title
    if not replace_once(
        tauri_conf,
        f'"title": "glogger v{current} DEV"',
        f'"title": "glogger v{new_version} DEV"',
    ):
        return 1

    # 3. tauri.release.conf.json - release window title (version-agnostic)
    if not replace_regex(
        tauri_release_conf,
        r'"title": "glogger v\d+\.\d+\.\d+(\.\d+)?"',
        f'"title": "glogger v{new_version}"',
    ):
        return 1

    # 3b. tauri.experimental.conf.json - experimental window title
    if not replace_once(
        tauri_experimental_conf,
        f'"title": "glogger v{current} EXPERIMENTAL"',
        f'"title": "glogger v{new_version} EXPERIMENTAL"',
    ):
        return 1

    # 4. package.json
    if not replace_regex(
        package_json,
        r'"version": "\d+\.\d+\.\d+"',
        f'"version": "{new_version}"',
    ):
        return 1

    # 5. Cargo.toml - only the package version (first occurrence, like the
    # awk in the .sh), not dependency versions.
    if not replace_once(
        cargo_toml,
        f'version = "{current}"',
        f'version = "{new_version}"',
    ):
        return 1

    # 6. Cargo.lock via cargo, failure ignored (the `|| true` equivalent).
    if shutil.which("cargo"):
        subprocess.run(
            ["cargo", "update", "-p", "glogger", "--precise", new_version],
            cwd=root / "src-tauri",
            check=False,
        )

    print()
    print(f"Version bumped to {new_version} in:")
    print("  - src-tauri/tauri.conf.json")
    print("  - src-tauri/tauri.release.conf.json")
    print("  - src-tauri/tauri.experimental.conf.json")
    print("  - package.json")
    print("  - src-tauri/Cargo.toml")
    print()
    print(
        f"Synced to {new_version} (CI release builds stop here - no commit; "
        "local use: commit as usual.)"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
