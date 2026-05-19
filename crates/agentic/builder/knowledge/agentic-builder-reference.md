---
source:
  - oxy-hq/skills/skills/oxy-agentic-builder/SKILL.md
  - oxy-hq/skills/skills/oxy-agentic-builder/QUICK-REFERENCE.md
reconciled-at: f9ebd8af267cfea5b52fa96994763898ab8a0e34
note: |
  Authored condensation. Not auto-synced — scripts/sync-skills.sh only copies
  the verbatim YAML templates. Re-condense by hand when source material
  changes materially; keep under ~200 lines so the LLM context stays focused.
  scripts/check-skills-drift.sh flags upstream changes.
---

# Oxy `.agentic.yml` Builder Reference

Agentic agents live in `*.agentic.yml` files. Each one is a multi-step FSM
that turns a natural-language question into a grounded query, runs it, and
explains the result. Reach for this format — not classic `.agent.yml` —
when you want a chat-style agent backed by the semantic layer.

## When to use which agent file

| Need                                    | File             |
| --------------------------------------- | ---------------- |
| Chat over the semantic layer            | `.agentic.yml`   |
| One-shot tool-using agent (no FSM)      | `.agent.yml`     |
| Deterministic multi-step pipeline       | `.workflow.yml`  |

The filename stem becomes the agent_id (`analytics.agentic.yml` →
`analytics`). Place the file at the project root or under any subdirectory.

## File shape

```yaml
instructions: |                  # global system prompt, every state inherits
  You are a revenue analytics assistant for Acme Corp.
  Always report currency in USD and dates in ISO 8601.

databases:                        # names from config.yml `databases:`
  - warehouse

context:                          # globs relative to this file
  - ./semantics/**/*.view.yml
  - ./semantics/**/*.topic.yml
  - ./example_sql/*.sql
  - ./docs/glossary.md
  - ./procedures/**/*.procedure.yml

llm:
  ref: claude-sonnet-4-6          # resolves vendor/key/model from config.yml
  max_tokens: 8192
  extended_thinking:              # UI-toggled preset, inherits the rest
    model: claude-opus-4-6
    thinking: adaptive

thinking: adaptive                # global default; per-state can override

states:
  clarifying:   { ... }
  specifying:   { ... }
  solving:      { ... }
  executing:    { ... }
  interpreting: { ... }

validation:
  rules:
    specified: []
    solvable:  []
    solved:    []

semantic_engine:                  # OPTIONAL — Cube/Looker delegation
  vendor: cube
  base_url: https://cube.example.com
  api_token: "${CUBE_API_TOKEN}"
```

## Pipeline states (fixed — DO NOT invent new state names)

| State          | Purpose                                | Notes                                   |
| -------------- | -------------------------------------- | --------------------------------------- |
| `clarifying`   | Triage the question, resolve metrics   | Cheap/fast model is usually right       |
| `specifying`   | Ground intent into a query spec        | Resolves joins                          |
| `solving`      | Generate SQL                           | **Skipped** when semantic layer compiled the spec |
| `executing`    | Run the query, run validators          | —                                       |
| `interpreting` | Natural-language answer + chart        | Thinking usually disabled               |
| `diagnosing`   | Back-edge entered when a validator fails | Internal — not directly configurable    |

**Skipping & retries.** When `clarifying` produces an intent the semantic
layer can compile directly, `solving` is dynamically skipped — never write
`solving` `instructions:` assuming the LLM authors SQL, because it may
never see them. When `diagnosing` re-enters an upstream state, the retry
generates fresh from the spec; it does not anchor on the previous attempt.

Each `states.<name>` block accepts `instructions` (appended to — not
replacing — the global prompt for that state), `thinking`, `max_retries`,
and `model`. State keys must match a fixed name above; otherwise the
override is silently ignored.

## `llm:` fields

| Field               | Default            | Notes                                                      |
| ------------------- | ------------------ | ---------------------------------------------------------- |
| `ref`               | —                  | Model entry from `config.yml`; vendor/key/base_url inherit |
| `vendor`            | `anthropic`        | `anthropic` \| `openai` \| `openai_compat`                 |
| `model`             | from `ref`         | Overrides the resolved model ID                            |
| `api_key`           | env var            | Falls back to `ANTHROPIC_API_KEY` / `OPENAI_API_KEY`       |
| `base_url`          | provider default   | Anthropic proxy / OpenAI Responses base / compat root      |
| `max_tokens`        | 4096               | Per-call output cap                                        |
| `thinking`          | none               | Beats top-level `thinking:`                                |
| `extended_thinking` | none               | UI-toggled `{ model, thinking }` preset                    |

## `thinking:` forms (top-level OR per-state)

```yaml
thinking: adaptive                # shorthand
thinking: disabled                # bare or quoted both work; never use `false`
thinking: effort:low              # OpenAI o-series shorthand
thinking: { budget_tokens: 10000 }
thinking: { effort: medium }      # low | medium | high
```

## Context globs by extension

| Pattern             | Where it goes                                       |
| ------------------- | --------------------------------------------------- |
| `*.view.yml`        | Semantic catalog                                    |
| `*.topic.yml`       | Semantic catalog                                    |
| `*.sql`             | Example queries injected into the Solving prompt    |
| `*.md`              | Domain docs injected into Clarifying / Interpreting |
| `*.procedure.yml`   | Indexed for the `search_procedures` tool            |

Globs are resolved relative to the directory containing the `.agentic.yml`.

## Validation rules

| Stage       | Available rule names                                                                                              |
| ----------- | ----------------------------------------------------------------------------------------------------------------- |
| `specified` | `metric_resolves`, `join_key_exists`, `filter_unambiguous`                                                        |
| `solvable`  | `sql_syntax`, `tables_exist_in_catalog`, `spec_tables_present`, `column_refs_valid`, `timeseries_order_by_check`  |
| `solved`    | `non_empty`, `shape_match`, `no_nan_inf`, `outlier_detection`, `timeseries_date_check`, `truncation_warning`, `null_ratio_check`, `duplicate_row_check` |

Tunable params:

| Rule                  | Params                                                                          |
| --------------------- | ------------------------------------------------------------------------------- |
| `sql_syntax`          | `dialect`: generic \| ansi \| postgresql \| mysql \| bigquery \| duckdb \| snowflake |
| `outlier_detection`   | `threshold_sigma` (default `5.0`), `min_rows` (default `4`)                     |
| `null_ratio_check`    | `threshold` (default `0.5`)                                                     |
| `duplicate_row_check` | `max_duplicate_ratio` (default `0.1`)                                           |

Omit the `validation:` section entirely to run every built-in rule with
defaults. Listing rules disables the unlisted ones.

## `semantic_engine:` (optional Cube / Looker delegation)

| Field                           | Cube       | Looker     |
| ------------------------------- | ---------- | ---------- |
| `vendor`                        | `cube`     | `looker`   |
| `base_url`                      | required   | required   |
| `api_token`                     | required   | —          |
| `client_id` / `client_secret`   | —          | required   |

`${VAR}` env-var interpolation is supported. A missing env var fails fast
at startup. Omit the section entirely to use Oxy's bundled semantic layer
(the default and most common case).

## Authoring patterns

1. **State the persona once, in the global `instructions:` block.** Per-state
   `instructions:` are appended on top — keep them tactical (dialect quirks,
   tie-breaking rules, output conventions) not a full re-statement of who
   the agent is.
2. **Enumerate the topics the agent can see.** A short list of `topic` /
   `view` names with a one-line description of each dramatically cuts
   hallucinated metric/dimension references.
3. **Use a cheap model for `clarifying` and a powerful one for `solving`.**
   The Solving stage benefits from extended thinking; Clarifying does not.
4. **Use `thinking: disabled`, never `thinking: false`.** Both bare
   `disabled` (parsed as a YAML identifier) and quoted `"disabled"` are
   accepted at runtime; a bare boolean `false` is rejected.
5. **Don't add `# yaml-language-server:` schema comments.** Oxy publishes
   no canonical schema URL for `.agentic.yml`, so the directive points at
   nothing actionable and creates noise in IDEs that flag unknown
   directives. Each top-level key (`llm`, `databases`, `context`,
   `states`, …) must appear at most once per file — duplicate keys cause
   the backend to reject the file.

## Common errors → fix

| Error                                      | Likely fix                                                |
| ------------------------------------------ | --------------------------------------------------------- |
| `no databases configured`                  | Add a connector name to `databases:`                      |
| `unsupported connector type: '…'`          | Match the name to a `config.yml` `databases:` entry       |
| `ambiguous table: '…'`                     | Drop one DB, or qualify the view's `data_source`          |
| `environment variable '${X}' is not set`   | Export the env var before launch                          |
| `unsupported semantic engine vendor: '…'`  | Use `cube` or `looker`, or omit `semantic_engine`         |
| `validation config error: unknown rule …`  | Use a rule name from the validation tables above          |
| `YAML parse error: duplicate key`          | Each top-level key must appear exactly once               |
| State override silently ignored            | The state key must be one of the five fixed names         |
| `thinking: false` rejected                 | Use `thinking: disabled` (bare or quoted), never `false`  |

## Validation workflow

```bash
oxy validate --file path/to/your.agentic.yml   # structural parse only
oxy validate                                   # all configs in the project
oxy build                                      # compile semantic layer
```

After authoring or editing an `.agentic.yml`, run `oxy validate --file …`
on the file before doing anything else; the structural parse catches the
majority of mistakes (missing `databases`, malformed `thinking`, unknown
fields).
