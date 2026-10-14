Focus: migration and deploy safety (what a running older binary, and a rollback, will see)

Read first: `internal-docs/revert-safe-migrations.md`, and the neighbouring migrations of
any migration the PR adds.

Check:
- A migration the currently running older binary cannot survive mid-deploy: dropping or
  renaming a column or table it still reads, a NOT NULL column without a default, a type
  changed in place.
- A migration that cannot be rolled back past, or that would wedge `oxy-migrate` on a
  revert.
- A backfill that locks a large table or runs unbounded in one transaction.
- `CREATE INDEX` without `CONCURRENTLY` on a large table, where the existing migrations
  use it.
- Raw SQL elsewhere in the code still naming a column or table this PR drops or renames.
  Grep for it: raw SQL survives a schema change that the compiler cannot see.
- The entity in `crates/entity` and the migration out of step.
