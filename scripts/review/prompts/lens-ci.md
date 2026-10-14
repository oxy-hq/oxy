Focus: CI workflows, actions and repo scripts (their security, and whether a check still means what it says)

Check:
- Script injection: `${{ github.event.* }}` (titles, bodies, branch names, comments)
  interpolated into a `run:` block instead of passed through `env:`.
- `pull_request_target` or `workflow_run` that checks out or runs PR code with secrets or
  a write token.
- A third-party action not pinned to a full commit SHA (this repo pins every action), or
  a permission broader than the job needs.
- A secret echoed, or written to a file or an artifact.
- A check that now passes when it did nothing: a step that exits 0 on an empty result,
  `continue-on-error` hiding a failure, a path filter or `if:` that skips the job for the
  change it should cover, a grep that silently matches nothing.
- Paid automation (LLM calls, large runners) without a hard cost or time cap.
- A `justfile` or script change that breaks a documented developer command.
