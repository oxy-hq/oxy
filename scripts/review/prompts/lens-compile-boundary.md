Focus: the compile boundary (workspace files become Postgres rows; requests do not read the filesystem)

Read first: `.claude/skills/oxy-compile-boundary/SKILL.md` and `internal-docs/compile-boundary.md`.

Check:
- A new per-request read of the workspace filesystem (`fs::read_to_string(workspace_path…)`,
  `ConfigManager::resolve_*`, a glob of the workspace dir) on a path the fleet serves,
  instead of a `*_definitions` row keyed by `revision_id`.
- A new `.foo.yml` artifact type that is not compiled into Postgres.
- "Not compiled yet" answered as empty or 404 instead of retryable.
- Absence read as empty: a missing workspace or revision must not look like one with no
  content.
