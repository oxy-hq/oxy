#!/usr/bin/env -S uv run
# /// script
# requires-python = ">=3.11"
# dependencies = [
#   "anthropic>=0.40.0",
# ]
# ///
"""Local dry-run tool for previewing changelog drafts against past releases.

In CI, changelog generation is handled by anthropics/claude-code-action@v1 in
.github/workflows/content-changelog.yaml, triggered when a release PR is merged.
That workflow gives Claude full MCP access (DeepWiki, gh, file tools).

This script is for LOCAL use only — preview what a changelog would look like
for any past release(s) before merging or publishing.

Flags:
  --dry-run   (implicit when run locally) — generate MDX, print to stdout.
              No git or GitHub operations are performed.

Positional args: one or more semver tags to generate the changelog for.
  If omitted, falls back to RELEASE_VERSION env var.

Examples:
  just release-changelog-preview 0.5.34
  just release-changelog-preview 0.5.33 0.5.34 0.5.35

How it works:
  1. Generates git-cliff release notes for each version (auto-runs git cliff)
  2. Fetches PR descriptions for every #NNN reference via `gh`
  3. Calls Claude (local claude CLI or ANTHROPIC_API_KEY) with product-context.md,
     oxy-content/CLAUDE.md, and DeepWiki hint (when using local claude CLI)
  4. Prints the generated MDX to stdout
"""

import json
import os
import re
import subprocess
import sys
from datetime import date
from pathlib import Path

DRY_RUN = "--dry-run" in sys.argv

# Versions: positional CLI args take precedence (for local multi-version dry-runs),
# otherwise fall back to RELEASE_VERSION env var (used by CI).
_cli_versions = [a for a in sys.argv[1:] if not a.startswith("--")]
VERSIONS: list[str] = (
    _cli_versions if _cli_versions else [os.environ["RELEASE_VERSION"]]
)
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
        cmd,
        check=True,
        capture_output=True,
        text=True,
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


# ── Writing guide + example ───────────────────────────────────────────────────


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


def get_example_changelog() -> str:
    """Return 1 recent published MDX from oxy-content as a concrete format example."""
    files = sorted(CHANGELOG_DIR.glob("*.mdx"), reverse=True)
    for f in files[:5]:
        content = f.read_text().strip()
        if len(content) > 300:  # skip near-empty stubs
            return content
    return ""


# ── Claude ────────────────────────────────────────────────────────────────────


def get_product_context() -> str:
    """Load product-context.md from the repo root (best-effort)."""
    for candidate in [
        Path("product-context.md"),
        Path(__file__).parent.parent.parent / "product-context.md",
    ]:
        if candidate.exists():
            return candidate.read_text()
    return ""


def build_prompt(
    release_notes: str,
    versions: list[str],
    existing_content: str = "",
    include_deepwiki_hint: bool = False,
) -> str:
    writing_guide = get_writing_guide()
    product_context = get_product_context()
    today = date.today().isoformat()
    versions_str = ", ".join(versions)
    versions_comment = f"<!-- versions: {versions_str} -->"

    product_block = (
        f"\n<product_context>\n{product_context}\n</product_context>\n"
        if product_context
        else ""
    )

    example = get_example_changelog()
    example_block = (
        f"\nHere is a recent published changelog entry from this repo — match its style exactly:\n"
        f"<example_changelog>\n{example}\n</example_changelog>\n"
        if example
        else ""
    )

    deepwiki_block = (
        "\nIf you need deeper context on how a specific feature works, call the DeepWiki MCP tool:\n"
        '  mcp__deepwiki__ask_question(repo="oxy-hq/oxy", question="...")\n'
        "Use it for any feature whose PR description leaves the user-facing impact unclear.\n"
        if include_deepwiki_hint
        else ""
    )

    if existing_content:
        return f"""You are writing a user-facing changelog for Oxy (an AI-powered data analytics platform).

Follow this writing guide exactly:
<writing_guide>
{writing_guide}
</writing_guide>
{product_block}{example_block}{deepwiki_block}
## Your task

An existing draft changelog entry already covers earlier releases this cycle.
Merge in release(s) {versions_str} WITHOUT rewriting existing content:
- Append new features as new `####` subsections under `### New Features`.
- Add new fixes/improvements under `### Platform Improvements`.
- Update frontmatter title/description/slug/sidebarTitle only if the new features are significant.
- Update the `<!-- versions: ... -->` tracking comment to include {versions_str}; create it if absent.

## How to read the release notes

The release notes contain two parts:
1. **Git commit subjects** (from git-cliff) — terse, engineer-facing. Use these only to identify which PRs exist.
2. **Pull Request Descriptions** (under "## Pull Request Descriptions") — the primary source. Each PR body explains
   the feature's user-facing purpose and impact. Base your feature descriptions on the PR body, not the commit subject.

Include: new features, UX improvements, meaningful bug fixes visible to users, performance improvements users notice.
Skip: internal refactors, CI/build/tooling changes, dependency bumps, test-only changes, chores with no user impact.

<release_notes>
{release_notes}
</release_notes>

<existing_draft>
{existing_content}
</existing_draft>

Return ONLY the complete updated MDX — no explanation, no code fences."""

    return f"""You are writing a user-facing changelog for Oxy (an AI-powered data analytics platform).

Follow this writing guide exactly:
<writing_guide>
{writing_guide}
</writing_guide>
{product_block}{example_block}{deepwiki_block}
## Your task

Convert the release notes below (covering {versions_str}) into a single user-friendly MDX changelog entry.
If multiple releases are provided, synthesize them into a cohesive entry — do not write a separate section per version.

## How to read the release notes

The release notes contain two parts:
1. **Git commit subjects** (from git-cliff) — terse, engineer-facing. Use these only to identify which PRs exist.
2. **Pull Request Descriptions** (under "## Pull Request Descriptions") — the primary source. Each PR body explains
   the feature's user-facing purpose and impact. Base your feature descriptions on the PR body, not the commit subject.

If a PR body is missing or too vague, fall back to inferring user impact from the commit subject and feature name.

## What to include / skip

Include: new features, UX improvements, meaningful bug fixes visible to users, performance improvements users notice.
Skip: internal refactors, CI/build/tooling changes, dependency bumps, test-only changes, chores with no user impact.
When in doubt, ask: "Would a data analyst care about this?" If no, skip it.

<release_notes>
{release_notes}
</release_notes>

## Output rules
- Use date {today} in the frontmatter `date` field.
- Author is always "Luong Vo (luong@oxy.tech)".
- After the closing `---` of the frontmatter, add exactly: `{versions_comment}`
- Do NOT add image markdown — omit screenshots entirely (human reviewer adds them).
- Return ONLY the MDX content — no explanation, no code fences."""


def call_claude(
    release_notes: str, versions: list[str], existing_content: str = ""
) -> str:
    if os.environ.get("ANTHROPIC_API_KEY"):
        import anthropic

        prompt = build_prompt(
            release_notes, versions, existing_content, include_deepwiki_hint=False
        )
        client = anthropic.Anthropic(timeout=120.0)  # 2-minute hard cap
        message = client.messages.create(
            model="claude-sonnet-4-6",
            max_tokens=8192,
            system=[
                {
                    "type": "text",
                    "text": (
                        "You are a technical writer producing user-facing release changelogs for Oxy, "
                        "an AI-powered data analytics platform. Write clearly for data analysts and "
                        "business users, not engineers. Output raw MDX only — no explanation, no markdown fences."
                    ),
                    "cache_control": {"type": "ephemeral"},
                }
            ],
            messages=[
                {"role": "user", "content": prompt},
                # Prefill forces Claude to begin with the MDX frontmatter delimiter
                # immediately — no risk of preamble text before the `---`.
                {"role": "assistant", "content": "---"},
            ],
        )
        # The prefilled `---` is consumed by the model; prepend it to the response.
        return ("---\n" + message.content[0].text).strip()
    # No API key — use local Claude Code CLI which has MCP (including DeepWiki) available
    print(
        "[dry-run] ANTHROPIC_API_KEY not set, using local claude CLI...",
        file=sys.stderr,
    )
    prompt = build_prompt(
        release_notes, versions, existing_content, include_deepwiki_hint=True
    )
    result = subprocess.run(
        ["claude", "-p", prompt],
        capture_output=True,
        text=True,
        check=True,
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
        gh(
            "label",
            "create",
            "--repo",
            CONTENT_REPO,
            PENDING_LABEL,
            "--color",
            "0075ca",
            "--description",
            "Pending changelog draft for next publish",
        )
    except subprocess.CalledProcessError:
        pass  # Label already exists


def find_open_pr():
    """Return (pr_number, branch) or (None, None)."""
    try:
        result = gh(
            "pr",
            "list",
            "--repo",
            CONTENT_REPO,
            "--label",
            PENDING_LABEL,
            "--state",
            "open",
            "--json",
            "number,headRefName",
            "--jq",
            ".[0]",
        )
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


# ── Release notes + PR context ────────────────────────────────────────────────


def get_cliff_notes(version: str) -> str:
    """Return git-cliff release notes for a version, generating if needed.

    In CI the workflow pre-generates /tmp/release-notes.md for the single
    release version; we reuse that file to avoid running git-cliff twice.
    """
    ci_path = Path("/tmp/release-notes.md")
    versioned_path = Path(f"/tmp/release-notes-{version}.md")
    # Prefer the CI-generated file for the primary version
    if ci_path.exists() and not versioned_path.exists():
        return ci_path.read_text()
    if not versioned_path.exists():
        print(
            f"Generating release notes for {version} via git-cliff...", file=sys.stderr
        )
        subprocess.run(
            ["git", "cliff", "--tag", version, "--latest", "-o", str(versioned_path)],
            check=True,
        )
    return versioned_path.read_text()


MAX_PRS = 20           # bound gh round-trips and prompt size
PR_BODY_LIMIT = 1500  # chars per PR after cleaning

# Section headers that are pure template noise
_NOISE_SECTION = re.compile(
    r"^#{1,4}\s*(testing|test plan|how to test|screenshots?|checklist|"
    r"reviewer notes?|related issues?|linked issues?|breaking changes?|"
    r"deployment notes?|rollback plan)\s*$",
    re.IGNORECASE | re.MULTILINE,
)


def clean_pr_body(body: str) -> str:
    """Strip GitHub PR template boilerplate, keep the meaningful description."""
    # Remove from each noise header to the next header or end of string
    while True:
        m = _NOISE_SECTION.search(body)
        if not m:
            break
        next_header = re.search(r"\n#{1,4}\s", body[m.end() :])
        end = m.end() + next_header.start() if next_header else len(body)
        body = body[: m.start()] + body[end:]
    # Remove checkbox lines (- [ ] or - [x])
    body = re.sub(r"^\s*- \[[ xX]\].*$", "", body, flags=re.MULTILINE)
    # Collapse excessive blank lines
    body = re.sub(r"\n{3,}", "\n\n", body)
    return body.strip()


SPARSE_BODY_THRESHOLD = 150  # chars; below this we try to enrich from issues/commits


def enrich_pr_body(pr_number: str, pr_body: str) -> str:
    """Augment a sparse PR body with linked issue descriptions and commit messages.

    Enrichment chain (each layer is appended only if the body is still thin):
      1. Closing issues referenced in the PR (GitHub `closingIssuesReferences`)
      2. Issue numbers mentioned inline (Closes #N / Fixes #N) in the body
      3. Commit messages from the PR (gives the "why" when the description is missing)
    """
    enriched = pr_body

    if len(enriched) < SPARSE_BODY_THRESHOLD:
        # Layer 1 + 2: linked issues
        issue_bodies: list[str] = []
        try:
            raw = run([
                "gh", "pr", "view", pr_number,
                "--repo", "oxy-hq/oxy-internal",
                "--json", "closingIssuesReferences",
            ])
            refs = json.loads(raw).get("closingIssuesReferences", [])
            issue_numbers = [str(r["number"]) for r in refs if r.get("number")]
        except (subprocess.CalledProcessError, json.JSONDecodeError):
            issue_numbers = []

        # Also pick up Closes/Fixes #NNN from the body text
        inline = re.findall(
            r"(?:closes?|fixes?|resolves?)\s+#(\d+)", pr_body, re.IGNORECASE
        )
        for n in inline:
            if n not in issue_numbers:
                issue_numbers.append(n)

        for issue_num in issue_numbers[:3]:  # cap at 3 issues
            try:
                raw = run([
                    "gh", "issue", "view", issue_num,
                    "--repo", "oxy-hq/oxy-internal",
                    "--json", "title,body",
                ])
                issue = json.loads(raw)
                issue_body = clean_pr_body(issue.get("body") or "")
                if issue_body:
                    issue_bodies.append(
                        f"[Issue #{issue_num}: {issue.get('title', '')}]\n{issue_body[:800]}"
                    )
            except (subprocess.CalledProcessError, json.JSONDecodeError):
                continue

        if issue_bodies:
            enriched = enriched + "\n\n" + "\n\n".join(issue_bodies)

    if len(enriched) < SPARSE_BODY_THRESHOLD:
        # Layer 3: commit messages from the PR
        try:
            raw = run([
                "gh", "pr", "view", pr_number,
                "--repo", "oxy-hq/oxy-internal",
                "--json", "commits",
            ])
            commits = json.loads(raw).get("commits", [])
            messages = [
                c["messageHeadline"]
                for c in commits
                if c.get("messageHeadline") and not c["messageHeadline"].startswith("Merge")
            ]
            if messages:
                enriched = enriched + "\n\nCommits:\n" + "\n".join(f"- {m}" for m in messages[:10])
        except (subprocess.CalledProcessError, json.JSONDecodeError):
            pass

    return enriched.strip()


def fetch_pr_context(cliff_notes: str) -> str:
    """Fetch PR titles + bodies for every (#NNN) reference in cliff_notes.

    Returns a formatted block of PR descriptions to supplement the commit-level
    notes that git-cliff produces. Skips release PRs and PRs with empty bodies.
    Silently ignores fetch errors (gh not authed, PR not found, etc.).
    Capped at MAX_PRS to bound prompt size and CI runtime.
    """
    pr_numbers = re.findall(r"\(#(\d+)\)", cliff_notes)
    if not pr_numbers:
        return ""

    unique = list(dict.fromkeys(pr_numbers))  # deduplicate, preserve order
    if len(unique) > MAX_PRS:
        print(
            f"Capping PR fetch at {MAX_PRS} (found {len(unique)}); oldest omitted.",
            file=sys.stderr,
        )
        unique = unique[:MAX_PRS]

    print(
        f"Fetching PR context for: {', '.join('#' + n for n in unique)}",
        file=sys.stderr,
    )

    sections: list[str] = []
    for num in unique:
        try:
            raw = run(
                [
                    "gh",
                    "pr",
                    "view",
                    num,
                    "--repo",
                    "oxy-hq/oxy-internal",
                    "--json",
                    "number,title,body,labels",
                ]
            )
            pr = json.loads(raw)
        except (subprocess.CalledProcessError, json.JSONDecodeError):
            continue

        title: str = pr.get("title", "")
        body: str = clean_pr_body(pr.get("body") or "")

        # Skip release PRs
        if title.startswith("chore: release"):
            continue

        # Enrich sparse bodies with linked issues and commit messages
        if len(body) < SPARSE_BODY_THRESHOLD:
            body = enrich_pr_body(num, body)

        if not body:
            continue

        # Truncate long bodies to keep the prompt manageable
        if len(body) > PR_BODY_LIMIT:
            body = body[:PR_BODY_LIMIT] + "\n…(truncated)"

        sections.append(f"### PR #{num}: {title}\n\n{body}")

    if not sections:
        return ""
    return "## Pull Request Descriptions\n\n" + "\n\n---\n\n".join(sections)


def get_release_context(version: str) -> str:
    """Combine git-cliff notes and PR descriptions for a single version."""
    cliff = get_cliff_notes(version)
    pr_context = fetch_pr_context(cliff)
    if pr_context:
        return f"{cliff}\n\n{pr_context}"
    return cliff


def gather_release_notes(versions: list[str]) -> str:
    """Assemble full context (cliff notes + PR bodies) for all versions."""
    if len(versions) == 1:
        return get_release_context(versions[0])
    parts = []
    for v in versions:
        parts.append(f"## Release {v}\n\n{get_release_context(v)}")
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
        print(
            f"Found open PR #{pr_number} on branch {pr_branch!r} — appending release {RELEASE_VERSION}"
        )
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

        current_body = gh(
            "pr",
            "view",
            str(pr_number),
            "--repo",
            CONTENT_REPO,
            "--json",
            "body",
            "--jq",
            ".body",
        )
        gh(
            "pr",
            "edit",
            str(pr_number),
            "--repo",
            CONTENT_REPO,
            "--body",
            current_body + f"\n- {RELEASE_VERSION}",
        )

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
        gh(
            "pr",
            "create",
            "--repo",
            CONTENT_REPO,
            "--title",
            f"chore: pending changelog ({today})",
            "--body",
            pr_body,
            "--label",
            PENDING_LABEL,
            "--head",
            PENDING_BRANCH,
            "--base",
            "main",
        )

        print(f"Created pending changelog PR for {today}")


if __name__ == "__main__":
    main()
