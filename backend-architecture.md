# Backend Architecture Rules

## Dependency Direction (HARD — never violate)

The agentic subsystem has strict layering. Imports must flow downward only:

```
Transport (agentic-http)
    ↓ imports only
Facade (agentic-pipeline)
    ↓ imports only
Domain (agentic-analytics, agentic-app-builder)
    ↓ imports only
Infrastructure (agentic-connector, agentic-llm, agentic-runtime)
    ↓ imports only
Core (agentic-core)
```

Concrete rules:

- **Platform → Agentic: NEVER.** Platform crates (`oxy`, `oxy-agent`, `oxy-workflow`, `oxy-shared`) must never import any `agentic-*` crate.
- **Agentic-core → anything agentic: NEVER.** Core is the pure FSM framework — zero agentic dependencies.
- **Domain ↔ Domain: NEVER.** `agentic-analytics` and `agentic-app-builder` must never import each other.
- **agentic-http → domain crates: NEVER.** HTTP handlers enter agentic only through `agentic-pipeline`. Never import `agentic-analytics`, `agentic-app-builder`, `agentic-connector`, or `agentic-llm` directly.
- **Acceptable upward deps:** `agentic-pipeline` and `agentic-http` may import `oxy` (for WorkspaceManager, ProjectManager, auth). This is the only allowed upward dependency.

**Litmus test:** Before adding an import, ask: "Would this crate still compile if I deleted every other domain crate?" If no, the dependency is in the wrong place.

## Function & File Size (HARD — always follow)

- **Max function length: ~60 lines.** If a function exceeds this, split it. Extract logical steps into well-named private functions.
- **Max file length: ~400 lines.** If a file exceeds this, split by responsibility. A solver file should not also contain types, helpers, and tests.
- **No god functions.** If a function takes more than 4-5 parameters, consider grouping into a config/context struct. If it has more than 3 levels of nesting, extract inner blocks.

## Thin Transport Layer (HARD — always follow)

HTTP handlers must be thin:

1. Parse/validate the request
2. Call into `agentic-pipeline` or `agentic-runtime`
3. Serialize the response

Do not put business logic, state transitions, or domain decisions in HTTP handlers. If a handler grows beyond ~30 lines, logic is leaking into the transport layer.

## Feature Slices

- Build features as **vertical slices**: types → domain logic → pipeline registration → handler → tests. Not horizontal layers across multiple features.
- If a feature needs both analytics and builder behavior, coordinate in `agentic-pipeline`, not by cross-importing domains.
- Keep domain logic in domain crates (solvers, state handlers). Pipeline is for composition only.

## Crate Management (HARD — always follow)

**When to add a new crate:**

- The code serves a clearly distinct domain or infrastructure concern that doesn't fit any existing crate.
- Two existing crates would need to depend on each other without it (break a cycle).
- Always add new crates to the workspace `Cargo.toml` members list.

**When NOT to add a new crate:**

- The code is only used by one existing crate — put it in that crate as a module.
- The code is a small utility — add it to the appropriate existing crate (`oxy-shared` for cross-cutting utils, domain crate for domain-specific utils).
- You're creating a crate just to "keep things organized" — use modules within the existing crate instead.

**When modifying an existing crate:**

- Prefer adding a new module over making an existing file larger.
- If adding a public API, check whether it belongs in this crate's responsibility or is leaking from another layer.

## Testing Guidelines

- **Write tests alongside implementation**, not as an afterthought. For bug fixes, write a failing test first when practical.
- For agentic pipeline features, use `run_agentic_eval()` for integration/end-to-end tests.
- **Unit test domain logic** (solvers, state transitions) in isolation — no HTTP, no database.
- **Prefer real structs over mocks.** Only mock external boundaries (LLM calls, database connections).
- **Test error paths**, not just happy paths — especially for state machine transitions.
- Keep test files co-located in the same crate. Use `#[cfg(test)] mod tests` for unit tests, `tests/` directory for integration tests.

## State & Event Flow

- Pipeline state flows through the FSM (`ProblemState`). Do not use side channels (statics, thread-locals, global mutable state).
- **Events are the contract between layers.** When adding new behavior, prefer new event variants over ad-hoc callbacks or return values.
- Database access belongs in the infrastructure layer (`agentic-connector`, `agentic-db`), not in solvers or core.

## Error Handling

- Use `thiserror` / `OxyError` patterns — look at existing patterns before creating new error enums.
- Do not swallow errors silently — propagate with `?` or log explicitly with `tracing::warn!`.
- Use `#[instrument]` on async entry points (handlers, solver methods) for tracing spans.
