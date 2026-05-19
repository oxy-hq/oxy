# Builder knowledge module

This directory holds reference documents that are **compile-time embedded**
into the `agentic-builder` crate via `include_str!` (see
[`src/prompts.rs`](../src/prompts.rs)). Every document here is either a
verbatim copy from the [`oxy-hq/skills`](https://github.com/oxy-hq/skills)
repository or an authored condensation of material from that repository.
The skills repo is the **source of truth**; everything in this directory
exists to vendor that knowledge into the builder binary.

## Contents

| File                            | Provenance                                                                        | Shape    |
| ------------------------------- | --------------------------------------------------------------------------------- | -------- |
| `semantic-layer-reference.md`   | Condensed from `skills/oxy-semantic-layer/SKILL.md` + `QUICK-REFERENCE.md`        | authored |
| `view-template.yml`             | `skills/oxy-semantic-layer/view-template.yml`                                     | verbatim |
| `topic-template.yml`            | `skills/oxy-semantic-layer/topic-template.yml`                                    | verbatim |
| `app-builder-reference.md`      | Authored from `skills/oxy-app-builder/SKILL.md` + `QUICK-REFERENCE.md`            | authored |
| `agent-builder-reference.md`    | Authored from `skills/oxy-workflow-builder/SKILL.md` (agent section)              | authored |
| `agentic-builder-reference.md`  | Authored from `skills/oxy-agentic-builder/SKILL.md` + `QUICK-REFERENCE.md`        | authored |
| `agentic-template.yml`          | `skills/oxy-agentic-builder/agentic-template.yml`                                 | verbatim |

Last synced: skills@f9ebd8af267cfea5b52fa96994763898ab8a0e34

## Provenance frontmatter on authored cards

Each `*-reference.md` carries machine-readable provenance in YAML
frontmatter at the top of the file:

```yaml
---
source:
  - oxy-hq/skills/skills/<skill>/SKILL.md
  - oxy-hq/skills/skills/<skill>/QUICK-REFERENCE.md
reconciled-at: <skills-repo-commit-sha>
note: |
  Free-form note for human maintainers.
---
```

The frontmatter is **stripped** by `strip_vendoring_header()` in
`src/prompts.rs` before the card is fenced into the system prompt, so
the LLM never sees it. Treat the fields as the contract:

- `source:` — list of upstream paths in `oxy-hq/skills` that the card
  condenses. Used by the drift detector below.
- `reconciled-at:` — full SHA on `oxy-hq/skills@main` that the card was
  last reconciled against. Bump this whenever you re-condense the card.

## Do not edit by hand

These files feed the builder agent's system prompts. Hand-editing them
creates drift between the canonical skills documentation and the prompts
the builder ships with. Instead:

1. Edit the source in `../skills` (the `oxy-hq/skills` repository).
2. Push your change there.
3. Run `./scripts/sync-skills.sh` from the `oxygen-internal` repo root. The
   script copies the verbatim YAML templates back into this directory
   and stamps the `Last synced:` line above with the skills-repo commit.
   Pin to a specific upstream SHA with `SKILLS_PIN=<sha>`.
4. For authored documents (`*-reference.md`) the script does **not**
   overwrite them. When the drift check below flags one as `BEHIND`,
   re-condense it by hand from the upstream sources, then bump the
   `reconciled-at:` field in its frontmatter to a recent SHA on
   `skills@main`. Keep each card under ~200 lines so the LLM context
   stays focused.

## Drift detection

`scripts/check-skills-drift.sh` reports whether any card has fallen
behind upstream. It reads each card's frontmatter, asks the GitHub
compare API whether any commit on `oxy-hq/skills@main` since the card's
`reconciled-at` SHA modified one of the listed `source:` paths, and
prints a status table. Exit code is `0` when every card is current,
`1` when any card is behind.

```bash
# Run locally (uses anonymous GitHub API, 60 req/hr limit):
bash scripts/check-skills-drift.sh

# Or with a token to lift the rate limit:
GH_TOKEN=$(gh auth token) bash scripts/check-skills-drift.sh
```

The check also runs in CI via `.github/workflows/skills-reconcile.yaml`:
- on PRs that touch this directory or the sync scripts (informational
  `check` job, doesn't block the PR), and
- on `main` via the `reconcile` job, which goes a step further than
  reporting and opens a PR with re-condensed cards for human review.

The `reconcile` job is webhook-driven: a companion workflow in
`oxy-hq/skills` (`.github/workflows/notify-oxygen-internal.yaml`) fires
a `skills-updated` repository_dispatch on every push to `skills@main`
that touches a tracked file, and oxygen-internal runs the reconcile
flow within seconds. A weekly Monday cron stays as a safety net for
missed dispatches (token expiry, a tracked path filter we forgot to
extend upstream, etc.).

The reconcile loop is therefore: **upstream push →** dispatch → drift
detected → re-condense the affected card from the listed sources →
**bump `reconciled-at:`** to the new SHA → **re-run**
`scripts/sync-skills.sh` so the templates and `Last synced:` line move
together → open PR for human review.

## Why compile-time embedding

- **Reproducibility** — every builder binary ships with a pinned copy of
  the knowledge it was built against. No runtime filesystem lookup, no
  cross-repository coupling at deploy time.
- **Offline robustness** — the builder still works in sandboxes that
  cannot reach the skills repo or the network.
- **Reviewability** — changes to the knowledge module show up in the same
  PR as the code change that depends on them.

## When to grow this module

Add a new file here when a builder phase starts duplicating non-trivial
domain knowledge inside its prompt string. Good signals:

- You're about to inline a YAML template into a `format!` macro.
- You're about to re-document a rule that already lives in `../skills`.
- Two different phases are each restating the same constraint in their
  own words.
