# GitHub Copilot Code Review Instructions (oxygen-internal)

## Review Philosophy

- Only comment when you have **HIGH CONFIDENCE** (>80%) that an issue exists.
- Be concise: one sentence per comment when possible.
- Focus on actionable feedback, not observations.
- For prose/docs text, comment only when wording is genuinely confusing or likely to cause user/developer error.

## Priority Areas (Review These First)

### Security & Safety

- Command injection risks (shell commands, user-controlled input)
- Path traversal vulnerabilities
- Credential exposure or hardcoded secrets
- Missing validation/sanitization on external input
- Sensitive error leakage in API/CLI output
- Unsafe Rust usage without a clear safety justification

### Correctness

- Logic bugs causing incorrect behavior or panics
- Async race conditions / ordering bugs
- Blocking work inside async contexts without justification
- Resource leaks (files, DB connections, tasks)
- Boundary/off-by-one mistakes
- Incorrect error propagation (e.g., avoid inappropriate `unwrap()`/`expect()` in production paths)
- Over-defensive or redundant checks that obscure true control flow
- Comments that only restate obvious code behavior

### Architecture & Patterns

- Violations of established crate/module patterns in this repository
- Inconsistent error handling with existing conventions
  - Prefer existing project patterns (`thiserror` / `OxyError` and contextual errors) over introducing new error styles arbitrarily
- Library crates should use `tracing` for logs, not `println!`
- CLI user-facing text should follow `StyledText` conventions from `oxy::theme`
- Sea-ORM + migration flows: ensure entity/migration consistency where relevant

### Scope Discipline

- Prefer comments on behavior, safety, and maintainability risks over stylistic rewrites.
- Avoid broad refactor asks unless necessary to fix a real defect.
- Choose the single most critical issue per comment.

## Project-Specific Context

- Monorepo: Rust workspace + frontend in `web-app` (Vite + React + TypeScript + pnpm)
- Important crates include:
  - `crates/app` (CLI + HTTP server)
  - `crates/core` (published as `oxy`)
  - `crates/agent`, `auth`, `entity`, `migration`, `semantic`, `shared`, `workflow`, `thread`, `project`, `globals`, `omni`, `a2a`
- Async runtime: Tokio
- ORM: Sea-ORM (PostgreSQL)
- HTTP framework: Axum
- MCP/A2A and provider integrations deserve extra protocol-level scrutiny

## Frontend Review Good Practices (`web-app`)

- Prioritize correctness bugs in React state/effects (stale closures, missing effect dependencies that cause wrong behavior, and update loops).
- Flag async UI races (out-of-order responses, missing cancellation/cleanup on unmount, duplicate submits without guards).
- Ensure robust UX states for networked screens (loading, error, and empty states where users would otherwise get stuck).
- Treat API data as untrusted: validate assumptions before rendering/accessing nested fields to avoid runtime crashes.
- Flag XSS risks (`dangerouslySetInnerHTML`, unsanitized HTML/markdown, or unsafe URL handling).
- Flag auth/session risks (token leakage in logs/query params, insecure persistence of sensitive data).
- Flag accessibility defects that break interaction (non-button clickable divs without keyboard support, missing labels for form controls, missing alt text on meaningful images).
- Flag real performance footguns only when impact is clear (expensive recalculation or rerenders caused by unstable props/deps in hot paths).
- Prefer review comments that include a concrete fix pattern (e.g., abort controller, dependency correction, guard clause, memoization boundary).

For frontend text/content changes, only comment on clarity when wording is likely to confuse users or produce incorrect actions.

## CI Pipeline Context (Do Not Duplicate CI Noise)

Reviews happen before/alongside CI. Do not flag issues CI reliably catches unless there is a repo-specific reason CI would miss it.

### What CI already checks (`.github/workflows/ci.yaml`)

- Rust lint/check path:
  - `cargo clippy --profile ci --workspace --fix`
- Rust tests:
  - `cargo nextest run --cargo-profile ci --workspace --no-fail-fast`
- Build and schema checks in CI flow:
  - `cargo build --profile ci`
  - `./target/ci/oxy gen-config-schema --check`
- Web checks/build when relevant changesets are present:
  - `pnpm install --frozen-lockfile --prefer-offline`
  - `pnpm lint-staged ...`
  - `pnpm build`
- Additional smoke/E2E workflows also exist (conditional)

### Practical implication

- Skip style/formatting nits and generic lint-level comments.
- Skip “missing dependency install” comments when CI clearly installs dependencies.
- Avoid speculative failures that CI setup already addresses.

## Skip These (Low Value)

Do not comment on:

- Formatting/style-only issues
- Clippy/rustfmt-only feedback without functional impact
- Generic test-failure speculation CI will surface
- Minor naming preference debates unless confusing
- Requests to add comments for self-documenting code
- Logging additions unless needed for security/error observability
- Pedantic wording tweaks that do not change meaning

## Response Format

When you identify an issue:

1. **State the problem** (1 sentence)
2. **Why it matters** (1 sentence, only if not obvious)
3. **Suggested fix** (specific action or minimal code suggestion)

Example:

This can panic when the collection is empty; use `.first()`/`.get(0)` or guard on length before indexing.

## When to Stay Silent

If uncertain whether something is a real issue, do not comment. False positives reduce trust and slow reviewers down.
