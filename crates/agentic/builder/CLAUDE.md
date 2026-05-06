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
| `search_files` | Glob pattern search (sorted by mtime, newest first) |
| `read_file` | Read file content (offset/limit params, raw output) |
| `search_text` | Regex search across project files (glob/output_mode params) |
| `write_file` | Create or fully overwrite a file — **suspends for user confirmation** |
| `edit_file` | Exact-string replacement (old_string/new_string) — **suspends for user confirmation** |
| `delete_file` | Delete a file — **suspends for user confirmation** |
| `validate_project` | Validate against Oxy YAML schemas |
| `lookup_schema` | Retrieve JSON schema for any Oxy object type |
| `run_tests` | Execute `.test.yml` evaluation files |
| `execute_sql` | Run SQL against configured databases |
| `semantic_query` | Compile and run semantic layer queries |
| `ask_user` | Generic clarification prompt |

## Suspension / HITL

`write_file`, `edit_file`, and `delete_file` each trigger a suspension (HITL). The user sees the proposed file change and can accept or reject. The facade emits a synthetic `ToolResult` event on resume so SSE replay shows the decision.

If the LLM batches multiple write ops in a single turn, all are applied on the first resume: `edit_file` ops have their `new_content` pre-computed at suspension time (stored in `stage_data["precomputed_edits"]`) to avoid TOCTOU races.

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
