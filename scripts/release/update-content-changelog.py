#!/usr/bin/env -S uv run
# /// script
# requires-python = ">=3.11"
# dependencies = [
#   "anthropic>=0.40.0",
# ]
# ///
"""Creates or updates a pending changelog PR in oxy-hq/oxy-content.

When a release is made in oxy-internal this script:
  1. Checks oxy-content for an open PR labelled "pending-release".
  2. If found  → appends the new release to the existing draft MDX file.
  3. If absent → generates a fresh MDX file and opens a new PR.

Multiple releases can accumulate in the same pending PR until the human
reviewer curates, adds screenshots, and merges.

Environment variables required:
  ANTHROPIC_API_KEY   — Anthropic API key
  RELEASE_VERSION     — semver string, e.g. "0.5.35"
  GH_TOKEN            — GitHub token with write access to oxy-content
                        (not required in --dry-run mode)

Inputs:
  /tmp/release-notes.md   — release notes from git-cliff --latest
                            (auto-generated via git-cliff if absent)
  ./oxy-content/          — local checkout of oxy-hq/oxy-content
                            (not required in --dry-run mode)

Flags:
  --dry-run   Generate and print the MDX to stdout; skip all git/GitHub ops.
              Useful for previewing output locally against any past tag.

Example (local dry-run against a past release):
  RELEASE_VERSION=0.5.34 uv run scripts/release/update-content-changelog.py --dry-run
"""

import json
import os
import subprocess
import sys
from datetime import date
from pathlib import Path

DRY_RUN = "--dry-run" in sys.argv

# Versions: positional CLI args take precedence (for local multi-version dry-runs),
# otherwise fall back to RELEASE_VERSION env var (used by CI).
_cli_versions = [a for a in sys.argv[1:] if not a.startswith("--")]
VERSIONS: list[str] = _cli_versions if _cli_versions else [os.environ["RELEASE_VERSION"]]
RELEASE_VERSION = VERSIONS[-1]  # latest version; used for single-version CI ops

CONTENT_DIR = Path("oxy-content")
CHANGELOG_DIR = CONTENT_DIR / "changelog"
DOCS_JSON = CONTENT_DIR / "docs.json"
PENDING_BRANCH = "pending-changelog"
PENDING_LABEL = "pending-release"
CONTENT_REPO = "oxy-hq/oxy-content"


# ── Utilities ─────────────────────────────────────────────────────────────────

def run(cmd, cwd=None):
    result = subprocess.run(
        cmd, check=True, capture_output=True, text=True,
        cwd=str(cwd) if cwd else None,
    )
    return result.stdout.strip()


def gh(*args, check=True):
    try:
        return run(["gh"] + list(args))
    except subprocess.CalledProcessError:
        if check:
            raise
        return ""


def git(*args):
    return run(["git"] + list(args), cwd=CONTENT_DIR)


# ── Writing guide ─────────────────────────────────────────────────────────────

def get_writing_guide() -> str:
    """Load oxy-content's CLAUDE.md; fall back to embedded rules if not present."""
    guide_path = CONTENT_DIR / "CLAUDE.md"
    if guide_path.exists():
        return guide_path.read_text()
    # Embedded fallback for dry-run without a local oxy-content checkout
    return """
Write user-facing MDX changelogs for Oxy (AI-powered data analytics platform).

Frontmatter fields: title, description, date, author ("Luong Vo (luong@oxy.tech)"), slug, sidebarTitle.
- title: 2-4 comma-separated main features, e.g. "Builder Agent, Workspace Management, and Thinking Budget"
- slug: kebab-case of title
- sidebarTitle: 1-3 words max

Content structure:
  ### New Features
  #### Feature Name
  Brief description then bullet points: - **Bold Header** - user-facing benefit

  ---

  ### Platform Improvements
  - **Fix** - description

Rules:
- Skip internal refactors, CI/build changes, dependency bumps, test-only changes.
- No image markdown — omit screenshots (human reviewer adds them).
- Write for data analysts and business users, not engineers.
"""


# ── Claude ────────────────────────────────────────────────────────────────────

def call_claude(release_notes: str, versions: list[str], existing_content: str = "") -> str:
    writing_guide = get_writing_guide()
    today = date.today().isoformat()
    versions_str = ", ".join(versions)
    versions_comment = f"<!-- versions: {versions_str} -->"

    if existing_content:
        prompt = f"""You are writing a user-facing changelog for Oxy (an AI-powered data analytics platform).

Follow this writing guide exactly:
<writing_guide>
{writing_guide}
</writing_guide>

An existing draft changelog entry already covers earlier releases this cycle.
Merge in release(s) {versions_str} WITHOUT rewriting existing content:
- Append new features as new `####` subsections under `### New Features`.
- Add new fixes/improvements under `### Platform Improvements`.
- Update frontmatter title/description/slug/sidebarTitle only if the new features are significant.
- Update the `<!-- versions: ... -->` tracking comment to include {versions_str}; create it if absent.

<release_notes>
{release_notes}
</release_notes>

<existing_draft>
{existing_content}
</existing_draft>

Skip: internal refactors, CI/build changes, dependency bumps, test-only changes.
Return ONLY the complete updated MDX — no explanation, no code fences."""
    else:
        prompt = f"""You are writing a user-facing changelog for Oxy (an AI-powered data analytics platform).

Follow this writing guide exactly:
<writing_guide>
{writing_guide}
</writing_guide>

Convert the release notes below (covering {versions_str}) into a single user-friendly MDX changelog entry.
If multiple releases are provided, synthesize them into a cohesive entry — do not write a separate section per version.

<release_notes>
{release_notes}
</release_notes>

Additional rules:
- Use date {today} in the frontmatter `date` field.
- Author is always "Luong Vo (luong@oxy.tech)".
- After the closing `---` of the frontmatter, add exactly: `{versions_comment}`
- Skip: internal refactors, CI/build changes, dependency bumps, test-only changes.
- Do NOT add image markdown — omit screenshots entirely (human reviewer adds them).
- Return ONLY the MDX content — no explanation, no code fences."""

    if os.environ.get("ANTHROPIC_API_KEY"):
        import anthropic
        client = anthropic.Anthropic()
        message = client.messages.create(
            model="claude-sonnet-4-6",
            max_tokens=4096,
            messages=[{"role": "user", "content": prompt}],
        )
        return message.content[0].text.strip()
    else:
        # No API key — fall back to local Claude Code CLI (claude -p)
        print("[dry-run] ANTHROPIC_API_KEY not set, using local claude CLI...", file=sys.stderr)
        result = subprocess.run(
            ["claude", "-p", prompt],
            capture_output=True, text=True, check=True,
        )
        return result.stdout.strip()


# ── docs.json ─────────────────────────────────────────────────────────────────

def register_in_docs_json(slug: str):
    """Prepend slug to the Recent Updates pages array in docs.json."""
    content = json.loads(DOCS_JSON.read_text())
    for tab in content.get("navigation", {}).get("tabs", []):
        if tab.get("tab") == "Changelog":
            for group in tab.get("groups", []):
                if group.get("group") == "Recent Updates":
                    pages = group["pages"]
                    if slug not in pages:
                        pages.insert(0, slug)
                    break
    DOCS_JSON.write_text(json.dumps(content, indent=2) + "\n")


# ── PR helpers ────────────────────────────────────────────────────────────────

def ensure_label():
    try:
        gh("label", "create", "--repo", CONTENT_REPO,
           PENDING_LABEL, "--color", "0075ca",
           "--description", "Pending changelog draft for next publish")
    except subprocess.CalledProcessError:
        pass  # Label already exists


def find_open_pr():
    """Return (pr_number, branch) or (None, None)."""
    try:
        result = gh("pr", "list", "--repo", CONTENT_REPO,
                    "--label", PENDING_LABEL, "--state", "open",
                    "--json", "number,headRefName", "--jq", ".[0]")
        if result and result not in ("null", ""):
            data = json.loads(result)
            return data.get("number"), data.get("headRefName")
    except (subprocess.CalledProcessError, json.JSONDecodeError):
        pass
    return None, None


def find_pending_mdx():
    """Return the most recent .mdx in changelog/ on the current branch, or None."""
    files = sorted(CHANGELOG_DIR.glob("*.mdx"), reverse=True)
    return files[0] if files else None


# ── Release notes ─────────────────────────────────────────────────────────────

def get_release_notes(version: str) -> str:
    """Return release notes for a version, generating via git-cliff if needed."""
    path = Path(f"/tmp/release-notes-{version}.md")
    if not path.exists():
        print(f"Generating release notes for {version} via git-cliff...", file=sys.stderr)
        subprocess.run(
            ["git", "cliff", "--tag", version, "--latest", "-o", str(path)],
            check=True,
        )
    return path.read_text()


def gather_release_notes(versions: list[str]) -> str:
    """Concatenate release notes for all versions into a single string."""
    if len(versions) == 1:
        return get_release_notes(versions[0])
    parts = []
    for v in versions:
        notes = get_release_notes(v)
        parts.append(f"## Release {v}\n\n{notes}")
    return "\n\n---\n\n".join(parts)


# ── Main ──────────────────────────────────────────────────────────────────────

def main():
    release_notes = gather_release_notes(VERSIONS)

    if DRY_RUN:
        label = ", ".join(VERSIONS)
        print(f"[dry-run] Generating changelog for {label} ...\n", file=sys.stderr)
        print(call_claude(release_notes, VERSIONS))
        return

    today = date.today().isoformat()
    ensure_label()
    pr_number, pr_branch = find_open_pr()

    if pr_number:
        # ── Append to existing pending PR ────────────────────────────────────
        print(f"Found open PR #{pr_number} on branch {pr_branch!r} — appending release {RELEASE_VERSION}")
        git("fetch", "origin", pr_branch)
        git("checkout", pr_branch)

        existing_file = find_pending_mdx()
        existing_content = existing_file.read_text() if existing_file else ""
        changelog_file = existing_file or (CHANGELOG_DIR / f"{today}.mdx")

        new_content = call_claude(release_notes, VERSIONS, existing_content)
        changelog_file.write_text(new_content)

        files_to_stage = [str(changelog_file.relative_to(CONTENT_DIR))]
        if not existing_file:
            register_in_docs_json(f"changelog/{today}")
            files_to_stage.append("docs.json")

        git("add", *files_to_stage)
        git("commit", "-m", f"chore: add release {RELEASE_VERSION} to changelog draft")
        git("push", "origin", pr_branch)

        current_body = gh("pr", "view", str(pr_number), "--repo", CONTENT_REPO,
                          "--json", "body", "--jq", ".body")
        gh("pr", "edit", str(pr_number), "--repo", CONTENT_REPO,
           "--body", current_body + f"\n- {RELEASE_VERSION}")

        print(f"Updated PR #{pr_number} with release {RELEASE_VERSION}")

    else:
        # ── Create a fresh pending PR ─────────────────────────────────────────
        print(f"No open pending PR — creating new one for release {RELEASE_VERSION}")

        try:
            git("push", "origin", "--delete", PENDING_BRANCH)
        except subprocess.CalledProcessError:
            pass  # Branch didn't exist — that's fine

        git("checkout", "main")
        git("pull", "origin", "main")
        git("checkout", "-b", PENDING_BRANCH)

        changelog_file = CHANGELOG_DIR / f"{today}.mdx"
        new_content = call_claude(release_notes, VERSIONS)
        changelog_file.write_text(new_content)
        register_in_docs_json(f"changelog/{today}")

        git("add", str(changelog_file.relative_to(CONTENT_DIR)), "docs.json")
        git("commit", "-m", f"chore: changelog draft for release {RELEASE_VERSION}")
        git("push", "-u", "origin", PENDING_BRANCH)

        versions_list = "".join(f"- {v}\n" for v in VERSIONS)
        pr_body = (
            f"Changelog draft auto-generated from Oxy release notes.\n\n"
            f"**Included releases:**\n"
            f"{versions_list}\n"
            f"> Review and curate before merging.\n"
            f"> Add screenshots to `changelog/images/{today}/` where helpful.\n"
            f"> Claude may have missed details or over-included internal changes."
        )
        gh("pr", "create",
           "--repo", CONTENT_REPO,
           "--title", f"chore: pending changelog ({today})",
           "--body", pr_body,
           "--label", PENDING_LABEL,
           "--head", PENDING_BRANCH,
           "--base", "main")

        print(f"Created pending changelog PR for {today}")


if __name__ == "__main__":
    main()
