#!/usr/bin/env -S uv run
# /// script
# requires-python = ">=3.11"
# dependencies = []
# ///
"""Determines next semver version from conventional commits and updates Cargo.toml + Cargo.lock.

Usage:
  python3 scripts/release/bump-version.py           # bumps version in Cargo.toml and Cargo.lock, prints new version
  python3 scripts/release/bump-version.py --dry-run # prints new version only, no file changes

Version bump rules (conventional commits):
  Pre-1.0 (major == 0):
    All commits -> patch only (minor bumps are manual/intentional)
  Post-1.0 (major >= 1):
    feat!: / BREAKING CHANGE -> major
    feat:                     -> minor
    fix: / perf: / etc.       -> patch
"""
import subprocess
import re
import sys
import tomllib
from pathlib import Path

DRY_RUN = "--dry-run" in sys.argv


def run(cmd):
    return subprocess.run(cmd, capture_output=True, text=True, check=True).stdout.strip()


def workspace_member_names() -> list[str]:
    """Return package names of workspace members that inherit version from the workspace.

    Reads `[workspace] members = [...]` from the root Cargo.toml, then inspects each member's
    Cargo.toml. A member is included only if it uses `version.workspace = true` — crates with
    hardcoded versions track their own release cadence and must not be touched.
    """
    with open("Cargo.toml", "rb") as f:
        root = tomllib.load(f)
    members = root.get("workspace", {}).get("members", [])
    names: list[str] = []
    for m in members:
        path = Path(m) / "Cargo.toml"
        if not path.exists():
            continue
        with open(path, "rb") as f:
            crate = tomllib.load(f)
        package = crate.get("package", {})
        version = package.get("version")
        if isinstance(version, dict) and version.get("workspace") is True:
            names.append(package["name"])
    return names


try:
    latest_tag = run(["git", "describe", "--tags", "--abbrev=0"])
except subprocess.CalledProcessError:
    latest_tag = "0.0.0"

try:
    commits = run(
        ["git", "log", f"{latest_tag}..HEAD", "--pretty=format:%s"]
    ).splitlines()
except subprocess.CalledProcessError:
    commits = []

major, minor, patch = map(int, latest_tag.lstrip("v").split("."))

if major == 0:
    # Pre-1.0: always bump patch only
    patch += 1
else:
    # Post-1.0: full conventional commit rules
    bump = "patch"
    for c in commits:
        if "BREAKING CHANGE" in c or re.match(r"^feat(\(.+\))?!:", c):
            bump = "major"
            break
        if re.match(r"^feat(\(.+\))?:", c) and bump != "major":
            bump = "minor"

    if bump == "major":
        major, minor, patch = major + 1, 0, 0
    elif bump == "minor":
        minor, patch = minor + 1, 0
    else:
        patch += 1

new_version = f"{major}.{minor}.{patch}"

if not DRY_RUN:
    with open("Cargo.toml") as f:
        content = f.read()
    content = content.replace(f'version = "{latest_tag}"', f'version = "{new_version}"')
    with open("Cargo.toml", "w") as f:
        f.write(content)

    # Update workspace crate versions in Cargo.lock by enumerating members rather than
    # matching `latest_tag`. A feature branch that lands after a release can carry stale
    # versions in Cargo.lock (e.g. airhouse/oxy-platform sat at 0.5.50 in #2266 because the
    # branch predated the 0.5.51 bump); a tag-anchored regex would silently skip those.
    with open("Cargo.lock") as f:
        lockfile = f.read()
    for name in workspace_member_names():
        lockfile = re.sub(
            rf'(name = "{re.escape(name)}"\nversion = ")[^"]+"',
            rf'\g<1>{new_version}"',
            lockfile,
        )
    with open("Cargo.lock", "w") as f:
        f.write(lockfile)

print(new_version, end="")
