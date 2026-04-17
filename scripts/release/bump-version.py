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

DRY_RUN = "--dry-run" in sys.argv


def run(cmd):
    return subprocess.run(cmd, capture_output=True, text=True, check=True).stdout.strip()


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

    # Update workspace crate versions in Cargo.lock.
    # Entries look like: name = "oxy"\nversion = "0.5.35"
    # We replace the old version only on lines immediately following a workspace crate name.
    with open("Cargo.lock") as f:
        lockfile = f.read()
    lockfile = re.sub(
        rf'(name = "[^"]+"\nversion = "){re.escape(latest_tag)}"',
        rf'\g<1>{new_version}"',
        lockfile,
    )
    with open("Cargo.lock", "w") as f:
        f.write(lockfile)

print(new_version, end="")
