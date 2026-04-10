# scripts/release

Release automation scripts. Run with `uv run` — dependencies install automatically.

| Script | What it does |
|--------|-------------|
| `bump-version.py` | Reads git commits since last tag, determines next semver, updates `Cargo.toml` |
| `update-content-changelog.py` | Calls Claude to generate a user-facing MDX changelog draft in `oxy-hq/oxy-content` |

## Local usage

```bash
# Preview next version + unreleased changelog (read-only)
just release-preview

# Generate a changelog draft for one or more past releases and print to stdout
just release-changelog-preview 0.5.34
just release-changelog-preview 0.5.33 0.5.34 0.5.35

# Trigger the CI release PR workflow manually
just release-trigger
```

## CI

Both scripts run automatically via `.github/workflows/prepare-release.yaml`:

- **Every push to `main`** — `bump-version.py` creates/updates a `chore: release X.Y.Z` PR
- **Release PR merged** — `update-content-changelog.py` creates/updates a pending PR in `oxy-content`

Required secrets: `ANTHROPIC_API_KEY`, GitHub App with access to `oxy-content`.
