---
source:
  - oxy-hq/skills/skills/oxy-workflow-builder/SKILL.md
reconciled-at: 7d98cd247517749da07819263ce87577a3583ff1
note: |
  Authored condensation of the agent-section of oxy-workflow-builder. Not
  auto-synced — scripts/sync-skills.sh only copies the verbatim YAML
  templates. Re-condense by hand when source material changes materially;
  keep under ~200 lines so the LLM context stays focused.
  scripts/check-skills-drift.sh flags upstream changes.
---

# Oxy Agent Builder Reference

Agents live in `*.agent.yml` files. Each one is a single LLM call with a
system prompt, a model, and a set of tools. This reference focuses on the
classic `.agent.yml` format — not the multi-step `.agentic.yml` FSM agents.

## The Oxy hierarchy (critical)

When deciding what tool an agent needs, walk this ladder in order. Stop at
the first rung that fits.

1. **`semantic_query`** — use this when the project has a semantic layer
   (any `*.view.yml` and `*.topic.yml` under `semantics/`) that covers
   the agent's domain. One `semantic_query` tool per topic. The
   semantic engine handles joins, aggregations, and dialect dispatch, so
   the agent never writes SQL.
2. **`execute_sql`** — fall back to raw SQL only when no semantic topic
   covers the data. Agents that reach for `execute_sql` when a semantic
   topic exists re-derive join logic on every call and drift from the
   curated layer.
3. **Agents as a tool themselves** — reserve general-purpose AI reasoning
   for exploratory, non-deterministic tasks. Deterministic answers should
   come from semantic queries or SQL.

## File shape (semantic-query agent, preferred)

```yaml
name: sales_analyst
description: "Answers questions about sales using the semantic layer"

model: openai                         # name from config.yml `models:`

system_instructions: |
  You are a data analyst specialising in sales performance.
  Use the semantic layer tools to answer questions; the layer handles
  joins and aggregations, so you do not write SQL.

  Available topics:
  - sales_mrr: revenue and MRR over time, by segment, by region.
  - customer_health: churn risk, tenure, usage intensity.

  When asked for trends, break out by month unless a finer granularity
  is requested. Always report revenue rounded to whole dollars.

tools:
  - type: semantic_query
    name: query_sales_mrr             # the LLM picks tools by this name
    description: "Query MRR and revenue data"
    topic: sales_mrr                  # must match a *.topic.yml `name`
    dry_run_limit: 100                # cap rows during testing

  - type: semantic_query
    name: query_customer_health
    description: "Query churn risk and customer health data"
    topic: customer_health
```

## File shape (text-to-SQL agent, fallback)

Use only when no semantic layer covers the domain.

```yaml
name: raw_sql_analyst
description: "Analyzes raw tables directly via SQL"

model: openai

system_instructions: |
  You are a data analyst. Query the warehouse directly; no semantic layer
  is available for this domain. Prefer CTEs for readability.

tools:
  - type: execute_sql
    database: clickhouse              # name from config.yml `databases:`

  - type: retrieval                   # feeds the agent example queries
    src:
      - example_sql/*.sql
      - workflows/*.workflow.yml
    key_var: OPENAI_API_KEY
```

## System-instruction patterns

1. **State the role in one sentence** — "You are a data analyst specialising
   in X." Keep it short; the rest of the prompt is concrete guidance.
2. **Enumerate the topics (or tables) the agent can see.** Listing each
   topic with a one-line description of what's inside dramatically cuts
   hallucinated tool calls. For semantic-query agents list every `topic:`
   value the agent is configured with.
3. **Document dialect quirks.** If a date column is stored as a string, or
   if a dialect needs `toDate(parseDateTimeBestEffort(...))`, write the
   exact cast the agent should emit. The agent will not discover this on
   its own.
4. **Spell out output conventions.** Examples: "Report revenue rounded to
   whole dollars", "Prefix dates with year (YYYY-MM)", "When filtering by
   status default to `active` unless asked otherwise".
5. **End with a guardrail if needed** — e.g. "If a question falls outside
   sales, answer: 'That's outside my domain.'" Keeps scope clean.

## Tool-selection rationale

| You need…                                   | Tool                 |
| ------------------------------------------- | -------------------- |
| A metric or dimension already in a topic    | `semantic_query`     |
| A one-off calculation over raw tables       | `execute_sql`        |
| Free-text content retrieval                 | `retrieval`          |
| Calling another agent                       | `agent`              |
| Running saved workflows                     | `workflow`           |
| Ad-hoc Python (plotting, stats)             | `python`             |

Start with the smallest tool set that solves the job. An agent with 10
tools is harder to steer than one with 2 — the LLM will pick the wrong tool
under pressure.

## Context pre-loading

When the agent needs a consistent baseline (e.g. a summary table loaded on
every turn), use a `context:` block rather than relying on the agent to
fetch it:

```yaml
context:
  - type: sql
    query: |
      SELECT *
      FROM analytics.weekly_summary
      WHERE week >= CURRENT_DATE - INTERVAL '30 days'
```

The result is injected into the prompt. Keep it small — anything you
pre-load is paid for on every turn.

## Validation

- `oxy validate --file=<agent.yml>` checks YAML shape and tool wiring.
- Smoke test with a real question:
  `oxy run my_agent.agent.yml "your question here"`.
- If the agent misbehaves, inspect its tool calls first — wrong tool choice
  is a system-instruction problem, not a model problem.
