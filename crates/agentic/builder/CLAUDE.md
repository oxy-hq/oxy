# agentic-builder

Built-in copilot for reading and modifying Oxy project files. Uses a single-state LLM tool loop (skips clarifying/specifying/executing, goes straight to solving).

## Pipeline

```
Solving (tool loop, max 30 rounds) → Interpreting → Done
```

The solver runs a single LLM tool loop in the Solving phase. Tools let the LLM read files, search code, propose changes, run SQL, validate configs, and run tests. The Interpreting phase synthesizes a user-facing summary from the tool exchanges.

## Tools

| Tool | What it does |
| ------ | ------------- |
| `search_files` | Glob pattern search |
| `read_file` | Read file content (with optional line range) |
| `search_text` | Regex search across project files |
| `propose_change` | Propose file change/deletion — **suspends for user confirmation** |
| `validate_project` | Validate against Oxy YAML schemas |
| `lookup_schema` | Retrieve JSON schema for any Oxy object type |
| `run_tests` | Execute `.test.yml` evaluation files |
| `execute_sql` | Run SQL against configured databases |
| `semantic_query` | Compile and run semantic layer queries |
| `ask_user` | Generic clarification prompt |

## Suspension / HITL

`propose_change` triggers a suspension (HITL). The user sees the proposed file content and can accept or reject. The facade emits a synthetic `ToolResult` event on resume so SSE replay shows the decision.

## Events

- `BuilderEvent::ProposedChange` — file path, description, new content
- `BuilderEvent::ToolUsed` — tool name, one-line summary

## Key Exports

- `BuilderSolver` / `build_builder_handlers()` — domain solver + state handlers
- `start_pipeline(BuilderPipelineParams)` → `PipelineHandle<BuilderEvent>`
- `event_handler()` → `DomainHandler` for EventRegistry registration
- `BuilderTestRunner` — trait for running test files (injected by `oxy-app`)
- `ConversationTurn` / `ToolExchange` — thread history types

## Rules

- Must NOT import `agentic-analytics`, `agentic-http`, or `agentic-pipeline`.
- May import `agentic-core`, `agentic-runtime`, `agentic-llm`.
- `BuilderPipelineParams` does NOT include `secrets_manager` — it's passed via `db_provider` or direct solver injection.
