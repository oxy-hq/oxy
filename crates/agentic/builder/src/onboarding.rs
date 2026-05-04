//! Onboarding prompt builder — constructs the builder agent prompt from
//! structured onboarding context (tables, warehouse type, model config).
//!
//! Six focused prompts are generated, one per build phase:
//! - `SemanticLayer` — inspect schemas, update config.yml, create .view.yml files (legacy, all-in-one)
//! - `Config`        — update config.yml only (model entry + database defaults)
//! - `SemanticView`  — inspect one table, create matching .view.yml + .topic.yml files
//! - `Agent`         — create analytics.agentic.yml (agentic analytics agent)
//! - `App`           — create apps/overview.app.yml (semantic_query-powered starter dashboard)
//! - `App2`          — create apps/detail.app.yml (cross-topic deep-dive dashboard,
//!                     only triggered when the workspace has ≥ 2 topics)
//!
//! This keeps the prompt templates server-side so the frontend only sends
//! structured selections, not raw LLM instructions.

use serde::Deserialize;

/// Fallback model name used when the frontend doesn't supply a `model_config`.
const DEFAULT_MODEL: &str = "claude-sonnet-4-6";

/// Which build phase this run covers.
#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum OnboardingBuildStep {
    /// Inspect tables, update config.yml, create .view.yml files (legacy all-in-one).
    ///
    /// **Superseded** by the `Config` + `SemanticView` decomposition. Kept only as
    /// a backward-compat fallback for older frontend builds and as the `Default`
    /// target for deserializing requests with no explicit `step`. New callers must
    /// not construct this variant directly.
    #[default]
    SemanticLayer,
    /// Update config.yml only (model entry + database defaults).
    Config,
    /// Inspect one table and create a single .view.yml file.
    /// Uses the first entry in `tables` as the target table.
    SemanticView,
    /// Create the default agentic analytics agent (`analytics.agentic.yml`).
    ///
    /// This replaces the legacy `.agent.yml` classic-agent template — users
    /// onboarding now end up with the multi-step FSM analytics pipeline
    /// (`agentic-analytics`) rather than a single-turn tool-calling agent.
    Agent,
    /// Create the starter `.app.yml` dashboard (`apps/overview.app.yml`).
    ///
    /// Onboarding always generates this — a credible starter artifact
    /// (trend chart + top performers table + bottom performers table, plus
    /// an optional fourth high-signal block) that showcases the user's data
    /// on the first topic alphabetically.
    App,
    /// Create a second `.app.yml` dashboard (`apps/detail.app.yml`) pivoted on
    /// a *different* topic than the overview.
    ///
    /// The frontend only triggers this phase when the workspace has ≥ 2
    /// topics (i.e. the user selected ≥ 2 tables). The prompt is explicitly
    /// cross-topic — it focuses on a different business concept than the
    /// overview so the two dashboards feel complementary, not redundant.
    #[serde(rename = "app2")]
    App2,
}

/// A column definition pre-fetched during schema discovery.
#[derive(Debug, Clone, Deserialize)]
pub struct TableColumnDef {
    pub name: String,
    #[serde(alias = "type")]
    pub column_type: String,
}

/// Structured context sent from the onboarding frontend.
#[derive(Debug, Clone, Deserialize)]
pub struct OnboardingContext {
    /// Selected tables in "schema.table" format.
    pub tables: Vec<String>,
    /// Warehouse type (e.g., "clickhouse", "postgres", "bigquery").
    pub warehouse_type: String,
    /// Model configuration to write to config.yml.
    #[serde(default)]
    pub model_config: Option<OnboardingModelConfig>,
    /// Which build phase this run is for.
    #[serde(default)]
    pub step: OnboardingBuildStep,
    /// Pre-fetched column definitions for the target table (avoids DESCRIBE round-trip).
    #[serde(default)]
    pub table_schema: Option<Vec<TableColumnDef>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct OnboardingModelConfig {
    pub name: String,
    pub vendor: String,
    pub model_ref: String,
    pub key_var: String,
}

impl OnboardingContext {
    /// Build the focused prompt for the requested build phase.
    pub fn build_prompt(&self) -> String {
        match self.step {
            OnboardingBuildStep::SemanticLayer => self.build_semantic_layer_prompt(),
            OnboardingBuildStep::Config => self.build_config_prompt(),
            OnboardingBuildStep::SemanticView => self.build_semantic_view_prompt(),
            OnboardingBuildStep::Agent => self.build_agent_prompt(),
            OnboardingBuildStep::App => self.build_app_prompt(),
            OnboardingBuildStep::App2 => self.build_app2_prompt(),
        }
    }

    // ── helpers ──────────────────────────────────────────────────────────────

    fn table_list(&self) -> String {
        self.tables
            .iter()
            .map(|t| format!("- {t}"))
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn model_name(&self) -> &str {
        self.model_config
            .as_ref()
            .map(|m| m.name.as_str())
            .unwrap_or(DEFAULT_MODEL)
    }

    // ── Phase 1a: Config only ────────────────────────────────────────────────

    fn build_config_prompt(&self) -> String {
        let db_name = &self.warehouse_type;

        let model_instructions = if let Some(mc) = &self.model_config {
            format!(
                r#"
- A model entry (if not already present):
```yaml
models:
  - name: {name}
    vendor: {vendor}
    model_ref: {model_ref}
    key_var: {key_var}
```
- Builder agent config: `builder_agent: {{ model: {name} }}`
"#,
                name = mc.name,
                vendor = mc.vendor,
                model_ref = mc.model_ref,
                key_var = mc.key_var,
            )
        } else {
            String::new()
        };

        format!(
            r#"I just connected a {db_name} warehouse.

Your task: **update config.yml** with the required configuration.

Use the propose_change tool for the file you modify.

---

## Instructions

Read the existing config.yml first. Then propose changes to ensure it has:
{model_instructions}
- A `defaults.database` pointing to `{db_name}` if not set

Do NOT create any other files. Only update config.yml.

Call `propose_change` **exactly once** for config.yml. Do not call it a second time — no revisions, no re-drafts. If the file already has content, use `from_line: 1, to_line: <current line count>` in your change block to replace it fully; never use `from_line: 1, to_line: 1` with multi-line content on a non-empty file (that will duplicate existing lines).

After proposing the change, STOP — do NOT write a summary or explanation."#,
        )
    }

    // ── Phase 1b: Single semantic view ───────────────────────────────────────

    fn build_semantic_view_prompt(&self) -> String {
        let db_name = &self.warehouse_type;
        // Use the first table in the list as the target
        let table = self
            .tables
            .first()
            .map(|t| t.as_str())
            .unwrap_or("unknown_table");

        let view_name = table.rsplit('.').next().unwrap_or(table);

        // If pre-fetched schema is available, inline it and skip the DESCRIBE step.
        let (schema_section, view_step) = match &self.table_schema {
            Some(cols) if !cols.is_empty() => {
                let col_list = cols
                    .iter()
                    .map(|c| format!("  - `{}` ({})", c.name, c.column_type))
                    .collect::<Vec<_>>()
                    .join("\n");
                (
                    format!(
                        r#"## Table schema (pre-fetched)

Table: `{table}`
Columns:
{col_list}

Use these columns directly — do NOT run any SQL queries."#
                    ),
                    1, // Create the view file at step 1
                )
            }
            _ => (
                format!(
                    r#"## Step 1: Inspect the table schema

Use execute_sql with `DESCRIBE TABLE {table}` (or the {db_name}-equivalent).
Understand column names, types, and cardinality."#
                ),
                2, // Create the view file at step 2
            ),
        };

        let topic_step = view_step + 1;

        format!(
            r#"I need a semantic layer entry for a single table in my {db_name} warehouse.

Your task: **create two files** — a `.view.yml` and a matching `.topic.yml` — for table `{table}`.

Use the propose_change tool for each file you create.

---

{schema_section}

## Step {view_step}: Create the view file

Create `semantics/{view_name}.view.yml`:

```yaml
name: {view_name}
description: "<one-line business description of what this table contains>"
datasource: {db_name}
table: "{table}"

entities:
  - name: {view_name}
    type: primary
    description: "<what one row represents>"
    key: <primary_key_dimension_name>   # MUST match the `name` of a dimension defined below

dimensions:
  - name: <snake_case_name>
    type: string          # one of: string | number | date | datetime | boolean (lowercase)
    description: "<what this dimension represents>"
    expr: <column_name>   # REQUIRED — the SQL column or expression

measures:
  - name: <snake_case_name>
    type: count           # one of: count | sum | average | min | max | count_distinct | median | custom
    description: "<what this measure calculates>"
    expr: <column_name>   # REQUIRED for every type except `count` (omit `expr` when type is count)
```

### View rules (violations break `oxy build`)

- `entities` is REQUIRED. Exactly one entity with `type: primary`.
- The primary entity's `key` MUST reference the `name` of a dimension in this view — not a raw column name.
- Every dimension MUST have an `expr` field (usually just the column name).
- Dimension `type` must be lowercase: `string`, `number`, `date`, `datetime`, `boolean`.
- Use `expr:` on measures (not `sql:`). Omit `expr` for `type: count`.
- Do NOT add a `# yaml-language-server:` schema comment.

Pick a primary-key dimension (id, uuid, or the most specific unique column). Include 3–8 dimensions and 2–4 measures that make analytical sense.

## Step {topic_step}: Create the topic file

Create `semantics/{view_name}.topic.yml`:

```yaml
name: {view_name}
description: "<one-line description of the business domain this topic covers>"
base_view: {view_name}
views:
  - {view_name}
```

Topics are what the analytics agent and dashboards query against — every view needs a matching topic.

Call `propose_change` **exactly once** per file (once for the view, once for the topic). For each file use a single change block with `from_line: 1, to_line: 1` and the full file contents. Do not call `propose_change` again for the same file — no revisions, no re-drafts.

After proposing both files, STOP — do NOT write a summary, explanation, or any follow-up text."#,
        )
    }

    // ── Phase 1 (legacy): Semantic Layer ─────────────────────────────────────

    fn build_semantic_layer_prompt(&self) -> String {
        let table_list = self.table_list();
        let db_name = &self.warehouse_type;

        let model_instructions = if let Some(mc) = &self.model_config {
            format!(
                r#"
- A model entry (if not already present):
```yaml
models:
  - name: {name}
    vendor: {vendor}
    model_ref: {model_ref}
    key_var: {key_var}
```
- Builder agent config: `builder_agent: {{ model: {name} }}`
"#,
                name = mc.name,
                vendor = mc.vendor,
                model_ref = mc.model_ref,
                key_var = mc.key_var,
            )
        } else {
            String::new()
        };

        format!(
            r#"I just connected a {db_name} warehouse and selected the following tables for my semantic layer:

{table_list}

Your task for this step: **inspect the tables and create the semantic layer**.

Use the propose_change tool for each file you create or modify.

---

## Step 1: Inspect table schemas

Use execute_sql with `DESCRIBE TABLE <table>` (or the {db_name}-equivalent) for each selected table.
Understand column names, types, and cardinality before creating any files.

## Step 2: Update config.yml

Read the existing config.yml first. Then propose changes to ensure it has:
{model_instructions}
- A `defaults.database` pointing to `{db_name}` if not set

## Step 3: Create semantic layer views

For each table, create a `.view.yml` file under `semantics/` and a matching `.topic.yml` so the analytics agent can query it. Example structure:

```yaml
name: <view_name>
description: "<description of what this table contains>"
datasource: {db_name}
table: "<fully_qualified_table_name>"

entities:
  - name: <view_name>
    type: primary
    description: "<what one row represents>"
    key: <primary_key_dimension_name>   # MUST match the `name` of a dimension defined below

dimensions:
  - name: <snake_case_name>
    type: string          # one of: string | number | date | datetime | boolean (lowercase)
    description: "<what this dimension represents>"
    expr: <column_name>   # REQUIRED — the SQL column or expression

measures:
  - name: <snake_case_name>
    type: count           # one of: count | sum | average | min | max | count_distinct | median | custom
    description: "<what this measure calculates>"
    expr: <column_name>   # REQUIRED for every type except `count` (omit `expr` when type is count)
```

Also create `semantics/<view_name>.topic.yml` for each view:

```yaml
name: <view_name>
description: "<one-line description of the business domain this topic covers>"
base_view: <view_name>
views:
  - <view_name>
```

### View rules (violations break `oxy build`)

- `entities` is REQUIRED. Exactly one entity with `type: primary`.
- The primary entity's `key` MUST reference the `name` of a dimension in this view — not a raw column name.
- Every dimension MUST have an `expr` field (usually just the column name).
- Dimension `type` must be lowercase: `string`, `number`, `date`, `datetime`, `boolean`.
- Use `expr:` on measures (not `sql:`). Omit `expr` for `type: count`.
- Do NOT add a `# yaml-language-server:` schema comment.

Choose dimensions and measures that make analytical sense for the table's data.

After proposing each file, STOP — do NOT write a summary or explanation."#,
        )
    }

    // ── Phase 2: Analytics Agent ──────────────────────────────────────────────

    fn build_agent_prompt(&self) -> String {
        let db_name = &self.warehouse_type;
        let model_name = self.model_name();

        format!(
            r#"The semantic layer for the {db_name} warehouse has just been created — views (`semantics/*.view.yml`) and matching topics (`semantics/*.topic.yml`).

Your task for this step: **create the default agentic analytics agent**.

Use the propose_change tool ONCE to create `analytics.agentic.yml`.

---

## Step 1: Read the semantic layer

Read at least one `.topic.yml` and one `.view.yml` from `semantics/` so you can sanity-check that the files exist and parse. The `context:` glob in the template below wires the whole `semantics/` tree into the pipeline automatically — you do NOT need to enumerate topic names into the agent file.

## Step 2: Create analytics.agentic.yml

Call `propose_change` exactly once, targeting `analytics.agentic.yml` at the project root. The `content` argument must be the file body below **verbatim** — do not duplicate it, do not wrap it in another document, do not append a second copy.

This is the agentic analytics agent users will interact with to ask questions about their data. It runs a multi-step FSM pipeline (clarify → specify → generate SQL → execute → interpret) rather than a single LLM tool loop.

```yaml
# yaml-language-server: $schema=https://raw.githubusercontent.com/oxy-hq/oxygen/refs/heads/main/json-schemas/agentic.json
llm:
  ref: {model_name}

databases:
  - {db_name}

context:
  - ./semantics/**/*
  - ./apps/**/*.app.yml
  - ./example_sql/**/*.sql

states:
  specifying:
    max_retries: 10
  interpreting:
    thinking: disabled
```

Rules:
- The file must contain the `# yaml-language-server: $schema=...` directive exactly once on the first line, followed by exactly one YAML document with `llm:`, `databases:`, `context:`, and `states:` appearing exactly once each. Duplicate top-level keys will cause the backend to reject the file with a 400 error.
- `llm.ref` references the model entry in `config.yml` by name — the agent inherits vendor, API key, and base URL from that entry.
- `databases` lists the connector name to use; the HTTP layer resolves it against `config.yml` at runtime.
- `context` globs are resolved relative to the file's directory and wire `.view.yml` / `.topic.yml` / `.app.yml` / `.sql` into the pipeline context.
- Do NOT create an `.agent.yml` file — the legacy classic-agent format is no longer used for onboarding.

After the single `propose_change` call, STOP — do NOT call `propose_change` again for this file, and do NOT write a summary or explanation."#,
        )
    }

    // ── Phase 3: Starter Dashboard App ─────────────────────────────────────────

    fn build_app_prompt(&self) -> String {
        let db_name = &self.warehouse_type;

        format!(
            r#"The semantic layer for {db_name} has been created. Each `.view.yml` has a matching `.topic.yml`.

Your task: **create `apps/overview.app.yml`** — a starter dashboard powered by the semantic layer.

This is the first artifact the user sees after onboarding, so every block must earn its place. The goal is **insight density**, not block count: a short, high-signal dashboard beats a long, generic one.

## Steps

1. List the `.topic.yml` files in `semantics/` sorted alphabetically. Read the **first** one and its matching `.view.yml`. Record the topic name, view name, and the full list of available dimensions and measures.
2. Pick the following fields — use only real names you just read, never invent:
   - **Primary metric (ONE measure)** — the most business-interesting value in the view. Prefer `sum` / `average` over a raw `count`. Use this same measure across every task below so all four blocks tell one coherent story.
   - **Date or datetime dimension** for the trend chart. If none exists, skip the trend chart entirely — do not try to invent a time axis.
   - **Entity dimension** for the ranking tables — the highest-cardinality *business identifier* (store name, product, customer, sku, city, region, title). This is what "top performers" / "bottom performers" rank.
3. Decide whether a fourth block is worth it using the "Fourth-block decision" rules below. It is *fine* — often better — to ship three strong blocks than four mixed ones.
4. Call `propose_change` **exactly once** to create `apps/overview.app.yml`. Use a single change block with `from_line: 1, to_line: 1` and the full file contents. Do not call `propose_change` again — no revisions, no re-drafts, no second calls.

## Required blocks (in order)

Every dashboard ships with these three, assuming the data supports them:

| # | Task name           | Block       | Purpose                                              |
|---|---------------------|-------------|------------------------------------------------------|
| 1 | `trend_over_time`   | line_chart  | How is the primary metric changing?                  |
| 2 | `top_performers`    | table       | Top 10 entities by the primary metric (descending).  |
| 3 | `bottom_performers` | table       | Bottom 10 entities by the primary metric (ascending). |

Top + bottom tables together give leaders *and* laggards — that pairing is almost always more interesting than a chart/table duplicate of the same cut.

## Fourth-block decision (optional)

Include a fourth block ONLY if it adds information the first three do not. Prefer, in order:

1. **`ranked_entities` bar chart** of the top ~15 entities by the primary metric. Adds visual distribution/shape (steep drop vs long tail) that tables can't convey.
2. **Grouped comparison bar chart** on a *genuinely meaningful* non-binary categorical dimension (product_family, region, segment, channel, status with ≥ 3 non-trivial values). Skip unless this cut reveals something the top/bottom tables miss.
3. **Trend-breakdown line chart** with `series: <view_name>.<entity_dimension>` showing the top entities' trajectories over time — only if there are ≤ 8 stable top entities.

**Omit the fourth block entirely** when none of the above clearly adds insight. Three strong blocks > four diluted blocks.

### Do NOT use these dimensions for the ranked / grouped / fourth block

- Binary or boolean flags: `holiday_flag`, `is_active`, `has_*`, `*_flag`, any dimension with only 2 distinct values (0/1, true/false, yes/no).
- Dimensions with fewer than 3 distinct meaningful values (splitting "null" vs "not null" doesn't count as two values).
- Raw surrogate keys or IDs that are not human-readable (pick a name / title / label instead when available).
- The same dimension used in both a chart and a table — no redundant chart + table pairs on the same cut.

## Template

Fill every `<placeholder>` with a real field name or a concrete human title (e.g. `"Weekly sales trend"`, not `"Chart 1"`). Include the commented blocks inline as guides — they are for your reasoning only; emit the final YAML without the `# OPTIONAL` comments.

The `title:` field is the dashboard's human-readable name shown in listings — infer a short, business-friendly label from what the data actually represents (e.g. "Sales Overview", "Customer Orders", "Product Performance"). Do NOT just title-case the table name — `raw_orders` should become "Orders Overview", not "Raw_orders Dashboard". Always include the word "Overview" since this is the overview dashboard.

```yaml
title: "<Business-friendly name> Overview"
description: "Overview of <topic in plain English> — trend, top performers, and weak spots."

tasks:
  - name: trend_over_time
    type: semantic_query
    topic: <topic_name>
    dimensions:
      - <view_name>.<date_dimension>
    measures:
      - <view_name>.<primary_measure>
    orders:
      - field: <view_name>.<date_dimension>
        direction: asc

  - name: top_performers
    type: semantic_query
    topic: <topic_name>
    dimensions:
      - <view_name>.<entity_dimension>
    measures:
      - <view_name>.<primary_measure>
    orders:
      - field: <view_name>.<primary_measure>
        direction: desc
    limit: 10

  - name: bottom_performers
    type: semantic_query
    topic: <topic_name>
    dimensions:
      - <view_name>.<entity_dimension>
    measures:
      - <view_name>.<primary_measure>
    orders:
      - field: <view_name>.<primary_measure>
        direction: asc
    limit: 10

  # OPTIONAL fourth task — include ONLY if it clears the Fourth-block decision bar above.
  # Example (ranked-entities variant):
  # - name: ranked_entities
  #   type: semantic_query
  #   topic: <topic_name>
  #   dimensions:
  #     - <view_name>.<entity_dimension>
  #   measures:
  #     - <view_name>.<primary_measure>
  #   orders:
  #     - field: <view_name>.<primary_measure>
  #       direction: desc
  #   limit: 15

display:
  - type: markdown
    content: |
      # <Topic in Title Case> Overview
      A quick read on <primary measure, in plain English>: where it's trending, who's leading, and where the weak spots are.
  - type: line_chart
    title: "<primary measure> over time"
    data: trend_over_time
    x: <view_name>__<date_dimension>
    y: <view_name>__<primary_measure>
    # Include `y_format: currency` ONLY when the primary measure is monetary
    # (see "Number formatting" below). Omit the line otherwise.
  - type: row
    children:
      - type: table
        title: "Top 10 <entities> by <primary measure>"
        data: top_performers
        # Include `formats:` ONLY when the primary measure is monetary.
      - type: table
        title: "Bottom 10 <entities> by <primary measure>"
        data: bottom_performers
  # OPTIONAL fourth display block — emit ONLY when the fourth task is included.
  # Example (ranked bar chart):
  # - type: bar_chart
  #   title: "Top 15 <entities> by <primary measure>"
  #   data: ranked_entities
  #   x: <view_name>__<entity_dimension>
  #   y: <view_name>__<primary_measure>
```

### Rules (violations break the dashboard)

- Task `dimensions` and `measures` references use a single dot: `<view_name>.<field_name>`.
- Display chart refs (`x:`, `y:`) use DOUBLE UNDERSCORE between view and field: `<view_name>__<field_name>`. This is how the semantic engine names its output columns.
- `table` blocks do NOT take `x` / `y` — they only take `data:`, `title:`, and optionally `formats:`. The table renders every column the task returns.
- `topic`, `<view_name>`, and every dimension/measure must match the real names you read in Step 1. Do not invent names.
- If the view has no usable date/datetime dimension, omit the `trend_over_time` task AND its line_chart entirely. In that case the fourth block is not optional — add a ranked bar chart so the dashboard has at least one visual.
- If the view has no usable entity dimension, reuse a meaningful categorical dimension for top/bottom — but still respect the "no binary flags" rule.
- Reuse the same `<primary_measure>` across every task so the dashboard tells one coherent story.
- Never emit more than four tasks or six display blocks total. Shorter is better.

### Number formatting

Pick a `DisplayFormat` per measure column based on what the measure actually represents. This is a judgement call — the measure name, its `description` in the `.view.yml`, its `type` (sum / average / count / …), and the business concept of the topic all inform the right answer. Do not treat the keywords below as an exhaustive checklist; treat them as examples.

- `currency` — any monetary quantity. Common signals: mentions of money, revenue, spend, cost, price, fees, billing, payments, GMV, ARR/MRR, LTV, AOV, ARPU, ACV, gross/net, a currency symbol or code in the description, or a `sum`/`average` measure over a column that is obviously dollars / euros / etc. When a data-literate user would naturally read the value with a `$`, use `currency`.
- `percent` — rates, shares, completion ratios, margins, attach/churn/conversion rates. Only when the underlying value is already scaled to 0–100 (a 0–1 ratio would render as `0.25%` with our current implementation, which is wrong — prefer plain `number` in that case, or omit the format).
- `number` — high-magnitude counts or integers that benefit from thousands separators (page views, sessions, users, orders, transactions, units sold, clicks). Use this for any `count` / `count_distinct` measure where values typically reach five digits or more, so `1234567` reads as `1,234,567`.
- Omit the format — small integer counts, already-formatted strings, or anything where formatting adds no clarity.

For charts (`line_chart` / `bar_chart`) set `y_format: <format>`; for the pie chart `value_format`; for tables use a `formats:` map keyed by the output column name, one entry per measure column on display. When two interpretations are plausible, pick the one a finance-literate user would expect — `total_weekly_sales` reads as currency, `session_count` reads as number, `conversion_rate_pct` reads as percent.

After creating the file, output **only** a "Sample Questions" section with 5 numbered questions users could ask the analytics agent about this data. Nothing else."#,
        )
    }

    // ── Phase 3b: Cross-topic Deep-dive Dashboard ──────────────────────────────

    fn build_app2_prompt(&self) -> String {
        let db_name = &self.warehouse_type;

        format!(
            r#"The semantic layer for {db_name} has been created, and `apps/overview.app.yml` already exists for the first topic alphabetically.

Your task: **create a second dashboard on a *different* topic** — a focused deep-dive that complements (does not duplicate) the overview.

This phase only runs when the workspace has multiple topics, so you can assume at least two `.topic.yml` files exist. Use that to your advantage: the whole point of this dashboard is to cover a business concept the overview does not.

## Steps

1. List the `.topic.yml` files in `semantics/` sorted alphabetically. Pick the **second** one (index 1) and read it plus its matching `.view.yml`. Record the topic name, view name, and available dimensions/measures.
2. Derive a snake_case slug from that topic name (strip `.topic.yml`, keep it as-is if already snake_case). This slug is both the YAML file name AND the dashboard's identity. Example: topic `customers` → write to `apps/customers.app.yml`; topic `order_items` → write to `apps/order_items.app.yml`.
3. From that view, pick:
   - **Primary metric (ONE measure)** — prefer a sum/average over a raw count when available. Used across both tasks so the dashboard stays coherent.
   - **Entity dimension** — the highest-cardinality *business identifier* (name, title, id with a human label). This is what "top performers" ranks.
4. Call `propose_change` **exactly once** to create `apps/<topic_slug>.app.yml`. Use a single change block with `from_line: 1, to_line: 1` and the full file contents. Do not call `propose_change` again — no revisions, no re-drafts, no second calls.

### Dimensions NOT to use as the entity dimension

- Binary or boolean flags: `*_flag`, `is_*`, `has_*`, any dimension with only 2 distinct values (0/1, true/false, yes/no). These almost never make a compelling ranking.
- Dimensions with fewer than 3 distinct meaningful values.
- Raw surrogate keys that are not human-readable — pick a name / title / label instead when available.

If the view has no usable entity-style dimension, use the best available non-binary categorical dimension as a fallback. Never rank on a binary flag.

## Template

Fill in every `<placeholder>` with real field names and topic-specific titles — do NOT leave placeholder words like "Topic" or "Category" in the output. The dashboard must read as a dedicated view of *this* specific business concept.

The `title:` field is the dashboard's human-readable name shown in listings. Infer a short, business-friendly label from what the data actually represents — NEVER just title-case the raw table or topic slug. Examples: a `raw_orders` topic becomes "Orders Deep Dive", a `customers` topic becomes "Customer Insights", a `product_inventory` topic becomes "Inventory Analysis". Read the view's columns, measures, and description to pick a title a business user would recognize at a glance. Keep it short (2–4 words), title-cased, and distinct from the overview dashboard's title.

The two blocks are intentionally different in shape: the bar chart shows visual distribution of the top ~15 entities; the table shows exact numbers for the bottom 10. Together they reveal the leaders *and* the weak spots without duplicating.

```yaml
title: "<Business-friendly name>"
description: "Deep dive into <topic in plain English> — leaders and weak spots."

tasks:
  - name: ranked_entities
    type: semantic_query
    topic: <topic_name>
    dimensions:
      - <view_name>.<entity_dimension>
    measures:
      - <view_name>.<primary_measure>
    orders:
      - field: <view_name>.<primary_measure>
        direction: desc
    limit: 15

  - name: bottom_performers
    type: semantic_query
    topic: <topic_name>
    dimensions:
      - <view_name>.<entity_dimension>
    measures:
      - <view_name>.<primary_measure>
    orders:
      - field: <view_name>.<primary_measure>
        direction: asc
    limit: 10

display:
  - type: markdown
    content: |
      # <Topic in Title Case>
      A closer look at <primary measure, in plain English>: who's leading and where the weak spots are.
  - type: bar_chart
    title: "Top 15 <entities> by <primary measure>"
    data: ranked_entities
    x: <view_name>__<entity_dimension>
    y: <view_name>__<primary_measure>
    # Include `y_format: currency` ONLY when the primary measure is monetary.
  - type: table
    title: "Bottom 10 <entities> by <primary measure>"
    data: bottom_performers
    # Include `formats:` ONLY when the primary measure is monetary.
```

### Rules (violations break the dashboard)

- Filename must be `apps/<topic_slug>.app.yml`, matching the topic you picked in Step 1. Do NOT name the file `apps/detail.app.yml` or reuse the overview's filename.
- The topic you pick must NOT be the same topic the overview dashboard uses. If there is only one topic, you must not have been invoked — stop and do nothing.
- Task `dimensions` and `measures` references use a single dot: `<view_name>.<field_name>`.
- Display chart refs (`x:`, `y:`) use DOUBLE UNDERSCORE: `<view_name>__<field_name>`.
- `table` blocks only take `data:`, `title:`, and optionally `formats:` — no `x` / `y`.
- Reuse the same `<primary_measure>` across both tasks so the dashboard is coherent.
- Keep the output compact: 2 tasks, 1 markdown + 1 chart + 1 table. This is a focused deep-dive, not a second overview.

### Number formatting

Pick a `DisplayFormat` for the primary measure based on what it actually represents. Use the measure's name, description, and type as reasoning inputs — not as a regex match.

- `currency` — monetary quantities (revenue, sales, spend, cost, price, fees, billing, GMV, ARR/MRR, LTV, AOV, ARPU, payments, margins-as-dollars, etc.). When a finance-literate user would naturally read the value with a `$`, use currency.
- `percent` — rates, shares, or ratios already scaled to 0–100 (conversion rate, churn rate, margin percentage). Do not use `percent` for a 0–1 ratio — prefer `number` or omit.
- `number` — high-magnitude counts or integers that benefit from thousands separators (`count` / `count_distinct` measures where values reach five digits or more).
- Omit — small integer counts or measures where formatting adds no clarity.

Set `y_format: <format>` on the bar chart as a sibling of `title:` / `data:`, and add a `formats:` map to the table listing the measure column:
```yaml
formats:
  <view_name>__<primary_measure>: <format>
```

After creating the file, STOP — do NOT write a summary or explanation."#,
        )
    }
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx_with(step: OnboardingBuildStep) -> OnboardingContext {
        OnboardingContext {
            tables: vec!["public.orders".to_string(), "public.customers".to_string()],
            warehouse_type: "postgres".to_string(),
            model_config: Some(OnboardingModelConfig {
                name: "sonnet-4-6".to_string(),
                vendor: "anthropic".to_string(),
                model_ref: "claude-sonnet-4-6".to_string(),
                key_var: "ANTHROPIC_API_KEY".to_string(),
            }),
            step,
            table_schema: None,
        }
    }

    /// Extract the first ```yaml fenced block from a prompt. Panics if none exists.
    fn extract_yaml_block(prompt: &str) -> &str {
        let start = prompt
            .find("```yaml\n")
            .expect("prompt has no ```yaml fence");
        let body_start = start + "```yaml\n".len();
        let end_offset = prompt[body_start..]
            .find("\n```")
            .expect("yaml fence is not closed");
        &prompt[body_start..body_start + end_offset]
    }

    // ── Agent phase: new .agentic.yml template ──────────────────────────────

    #[test]
    fn agent_prompt_creates_agentic_yml_at_project_root() {
        let prompt = ctx_with(OnboardingBuildStep::Agent).build_prompt();
        assert!(
            prompt.contains("analytics.agentic.yml"),
            "expected prompt to reference analytics.agentic.yml, got: {prompt}"
        );
        // Must not instruct the builder to create a classic .agent.yml.
        assert!(
            !prompt.contains("agents/default.agent.yml"),
            "prompt should no longer reference the legacy agents/default.agent.yml path"
        );
    }

    #[test]
    fn agent_prompt_mentions_warehouse_and_model() {
        let prompt = ctx_with(OnboardingBuildStep::Agent).build_prompt();
        assert!(
            prompt.contains("postgres"),
            "prompt should reference the configured warehouse name"
        );
        assert!(
            prompt.contains("sonnet-4-6"),
            "prompt should reference the configured model name (llm.ref)"
        );
    }

    #[test]
    fn agent_prompt_falls_back_to_default_model() {
        let mut ctx = ctx_with(OnboardingBuildStep::Agent);
        ctx.model_config = None;
        let prompt = ctx.build_prompt();
        assert!(
            prompt.contains(DEFAULT_MODEL),
            "prompt should fall back to DEFAULT_MODEL when no model_config is supplied"
        );
    }

    #[test]
    fn agent_prompt_yaml_block_parses() {
        let prompt = ctx_with(OnboardingBuildStep::Agent).build_prompt();
        let yaml = extract_yaml_block(&prompt);
        let value: serde_yaml::Value = serde_yaml::from_str(yaml)
            .unwrap_or_else(|e| panic!("embedded YAML is not valid YAML: {e}\n---\n{yaml}"));

        // Top-level shape matches agentic-analytics AgentConfig.
        let map = value.as_mapping().expect("top-level YAML must be a map");
        let llm = map
            .get(serde_yaml::Value::String("llm".into()))
            .expect("missing llm: section");
        let llm_map = llm.as_mapping().expect("llm: must be a map");
        let llm_ref = llm_map
            .get(serde_yaml::Value::String("ref".into()))
            .and_then(|v| v.as_str())
            .expect("missing llm.ref field");
        assert_eq!(llm_ref, "sonnet-4-6");

        let databases = map
            .get(serde_yaml::Value::String("databases".into()))
            .and_then(|v| v.as_sequence())
            .expect("missing databases: sequence");
        let db_names: Vec<&str> = databases.iter().filter_map(|v| v.as_str()).collect();
        assert_eq!(db_names, vec!["postgres"]);

        let context = map
            .get(serde_yaml::Value::String("context".into()))
            .and_then(|v| v.as_sequence())
            .expect("missing context: sequence");
        let globs: Vec<&str> = context.iter().filter_map(|v| v.as_str()).collect();
        assert!(
            globs.iter().any(|g| g.contains("semantics")),
            "context must include a semantics/** glob, got: {globs:?}"
        );

        // states overrides are present and parse.
        let states = map
            .get(serde_yaml::Value::String("states".into()))
            .and_then(|v| v.as_mapping())
            .expect("missing states: map");
        assert!(
            states.contains_key(serde_yaml::Value::String("specifying".into())),
            "expected a `specifying` state override in states:"
        );
    }

    #[test]
    fn agent_prompt_tells_builder_to_use_propose_change() {
        let prompt = ctx_with(OnboardingBuildStep::Agent).build_prompt();
        assert!(
            prompt.contains("propose_change"),
            "prompt should instruct the builder to use the propose_change tool"
        );
    }

    #[test]
    fn agent_prompt_embeds_yaml_language_server_schema_directive() {
        // The embedded template must carry the IDE schema directive so the
        // rendered file is validated against `json-schemas/agentic.json`.
        let prompt = ctx_with(OnboardingBuildStep::Agent).build_prompt();
        let yaml = extract_yaml_block(&prompt);
        assert!(
            yaml.lines().next().is_some_and(|first| first
                .starts_with("# yaml-language-server: $schema=")
                && first.contains("agentic.json")),
            "first line of embedded YAML must be the yaml-language-server directive pointing at agentic.json, got:\n{yaml}"
        );
    }

    #[test]
    fn agent_prompt_forbids_duplicate_writes() {
        // Regression guard against the LLM emitting the file content twice
        // (duplicate top-level keys → invalid YAML → 400 from the backend).
        let prompt = ctx_with(OnboardingBuildStep::Agent).build_prompt();
        assert!(
            prompt.contains("exactly once") || prompt.contains("verbatim"),
            "prompt should explicitly forbid duplicate writes; got:\n{prompt}"
        );
    }

    #[test]
    fn build_step_dispatches_agent_variant_to_agent_prompt() {
        // Sanity check: the Agent enum variant actually dispatches through
        // build_agent_prompt (and not a stale variant from an earlier rename).
        let prompt = ctx_with(OnboardingBuildStep::Agent).build_prompt();
        assert!(prompt.contains("agentic analytics agent"));
    }

    #[test]
    fn other_phases_do_not_reference_legacy_agent_path() {
        // Regression guard: app / app2 / config / semantic_view prompts should
        // no longer instruct the builder to produce a classic .agent.yml file.
        for step in [
            OnboardingBuildStep::Config,
            OnboardingBuildStep::SemanticView,
            OnboardingBuildStep::App,
            OnboardingBuildStep::App2,
        ] {
            let prompt = ctx_with(step.clone()).build_prompt();
            assert!(
                !prompt.contains("agents/default.agent.yml"),
                "phase {step:?} still mentions legacy agents/default.agent.yml"
            );
        }
    }

    // ── SemanticView phase: view + topic schema guards ──────────────────────

    #[test]
    fn semantic_view_prompt_includes_entities_block() {
        // Views without an entities block fail `oxy build` with
        // "View must have at least one entity".
        let prompt = ctx_with(OnboardingBuildStep::SemanticView).build_prompt();
        assert!(
            prompt.contains("entities:"),
            "SemanticView prompt must require an entities: block; got:\n{prompt}"
        );
        assert!(
            prompt.contains("type: primary"),
            "SemanticView prompt must instruct the agent to declare a primary entity"
        );
    }

    #[test]
    fn semantic_view_prompt_uses_expr_not_sql() {
        // Dimensions and measures use `expr:` in the current schema; `sql:`
        // is the old key and causes "missing field `expr`" at build time.
        let prompt = ctx_with(OnboardingBuildStep::SemanticView).build_prompt();
        assert!(
            prompt.contains("expr:"),
            "SemanticView prompt must use `expr:` for dimension/measure expressions"
        );
        // The view rules section explicitly forbids `sql:` on measures, so
        // the word appears — but only in the negative guidance. Ensure there
        // is no positive example using `sql:` as a dimension/measure key.
        assert!(
            !prompt.contains("sql: <column_or_expression>"),
            "SemanticView prompt must not show `sql:` as a dimension/measure value"
        );
    }

    #[test]
    fn semantic_view_prompt_creates_topic_file() {
        // Every view must ship with a matching .topic.yml — the analytics
        // agent's semantic_query tool and all app tasks query against topics.
        let prompt = ctx_with(OnboardingBuildStep::SemanticView).build_prompt();
        assert!(
            prompt.contains(".topic.yml"),
            "SemanticView prompt must instruct the agent to create a .topic.yml file"
        );
        assert!(
            prompt.contains("base_view:"),
            "SemanticView topic template must include a base_view: field"
        );
    }

    // ── App phase: semantic_query + display reference guards ───────────────

    #[test]
    fn app_prompts_use_semantic_query_task_type() {
        for step in [OnboardingBuildStep::App, OnboardingBuildStep::App2] {
            let prompt = ctx_with(step.clone()).build_prompt();
            assert!(
                prompt.contains("type: semantic_query"),
                "phase {step:?} must use `type: semantic_query` tasks, not raw SQL"
            );
            assert!(
                !prompt.contains("type: execute_sql"),
                "phase {step:?} should no longer emit `type: execute_sql` app tasks"
            );
        }
    }

    #[test]
    fn app_prompts_use_double_underscore_in_display_refs() {
        // airlayer joins view + field with `__` in its output column names,
        // so chart `x:` / `y:` must use that convention. Using a dot there
        // silently produces empty charts.
        for step in [OnboardingBuildStep::App, OnboardingBuildStep::App2] {
            let prompt = ctx_with(step.clone()).build_prompt();
            assert!(
                prompt.contains("__"),
                "phase {step:?} must document the double-underscore display ref convention"
            );
        }
    }

    #[test]
    fn app_prompt_produces_overview_file() {
        // The starter dashboard's filename is fixed at apps/overview.app.yml
        // so the frontend can key its completion fallback off a stable path.
        let prompt = ctx_with(OnboardingBuildStep::App).build_prompt();
        assert!(
            prompt.contains("apps/overview.app.yml"),
            "app phase must target apps/overview.app.yml"
        );
    }

    #[test]
    fn app_prompt_includes_high_signal_table_blocks() {
        // The starter dashboard relies on tables for the "wow" moment —
        // top + bottom performers. A regression to charts-only or a retreat
        // to the old `breakdown` table would bring back the sparse feel we
        // moved away from.
        let prompt = ctx_with(OnboardingBuildStep::App).build_prompt();
        assert!(
            prompt.matches("type: table").count() >= 2,
            "app phase must include at least two `type: table` display blocks; got:\n{prompt}"
        );
        assert!(
            prompt.contains("top_performers"),
            "app phase must include a top-N ranking task named `top_performers`"
        );
        assert!(
            prompt.contains("bottom_performers"),
            "app phase must pair top_performers with a `bottom_performers` ascending-ranked table"
        );
        assert!(
            prompt.contains("direction: asc"),
            "app phase must include an ascending order (for the bottom-performers ranking)"
        );
    }

    #[test]
    fn app_prompt_bans_binary_flag_dimensions() {
        // Regression guard for the "holiday_flag bar chart" bug: the prompt
        // must explicitly tell the LLM not to rank on binary/boolean
        // dimensions, which produced low-signal output in the early starter
        // dashboards.
        let prompt = ctx_with(OnboardingBuildStep::App).build_prompt();
        assert!(
            prompt.contains("binary") && (prompt.contains("flag") || prompt.contains("boolean")),
            "app phase must forbid binary/boolean flag dimensions for the main splits"
        );
        assert!(
            prompt.contains("2 distinct values")
                || prompt.contains("only 2 distinct")
                || prompt.contains("fewer than 3 distinct"),
            "app phase must forbid ≤2-value dimensions from being used as a ranking dimension"
        );
    }

    #[test]
    fn app_prompt_retires_low_signal_defaults() {
        // The old prompt always emitted a `breakdown` table and a
        // `comparison_by_group` bar chart on the same categorical dimension
        // — a redundant chart/table pair that collapsed into noise when the
        // dimension was low-cardinality (see the `holiday_flag` screenshot).
        // Those tasks are gone; guard against them silently coming back.
        let prompt = ctx_with(OnboardingBuildStep::App).build_prompt();
        assert!(
            !prompt.contains("- name: breakdown"),
            "app phase must not define a `breakdown` task; top+bottom tables replaced it"
        );
        assert!(
            !prompt.contains("- name: comparison_by_group"),
            "app phase must not define a `comparison_by_group` task; the fourth block is now conditional"
        );
    }

    #[test]
    fn app_prompt_makes_fourth_block_conditional() {
        // The fourth block is opt-in: the prompt must give the LLM explicit
        // permission to ship only three strong blocks rather than pad with a
        // weak fourth one.
        let prompt = ctx_with(OnboardingBuildStep::App).build_prompt();
        assert!(
            prompt.contains("Fourth-block decision") || prompt.contains("fourth block"),
            "app phase must describe how to decide on the fourth block"
        );
        assert!(
            prompt.to_lowercase().contains("omit") || prompt.contains("Three strong blocks"),
            "app phase must explicitly allow omitting the fourth block"
        );
    }

    #[test]
    fn app2_prompt_is_cross_topic_and_topic_named() {
        // App2 is the deep-dive dashboard. It must pivot on a DIFFERENT topic
        // than the overview (second topic alphabetically) and its filename
        // must be derived from that topic — never the generic
        // `apps/detail.app.yml`.
        let prompt = ctx_with(OnboardingBuildStep::App2).build_prompt();
        assert!(
            prompt.contains("second") || prompt.contains("Second"),
            "app2 prompt must instruct the builder to pick the second topic alphabetically"
        );
        assert!(
            prompt.contains("apps/<topic_slug>.app.yml")
                || prompt.contains("apps/<topic_name>.app.yml"),
            "app2 prompt must name the file after the topic, not `detail`"
        );
        assert!(
            !prompt.contains("apps/detail.app.yml")
                || prompt.contains("Do NOT name the file `apps/detail.app.yml`")
                || prompt.contains("not name the file")
                || prompt.contains("not name the file `apps/detail.app.yml`"),
            "app2 prompt must avoid writing to apps/detail.app.yml (or explicitly forbid it)"
        );
        assert!(
            prompt.contains("different topic") || prompt.contains("NOT be the same topic"),
            "app2 prompt must explicitly require a different topic than the overview"
        );
    }

    #[test]
    fn app_prompts_require_title_field() {
        // Both onboarding dashboards must emit a human-friendly `title:` field
        // so the completion screen can show a business-friendly label instead
        // of the raw filename (e.g. "Orders Overview" rather than
        // "Raw_orders Dashboard"). Skipping this field means listings fall
        // back to the filename, which is exactly the regression we're
        // guarding against.
        for step in [OnboardingBuildStep::App, OnboardingBuildStep::App2] {
            let prompt = ctx_with(step.clone()).build_prompt();
            let yaml = extract_yaml_block(&prompt);
            assert!(
                yaml.contains("title:"),
                "phase {step:?} YAML template must include a `title:` field so the LLM emits one"
            );
            assert!(
                prompt.to_lowercase().contains("business-friendly"),
                "phase {step:?} prompt must instruct the LLM to infer a business-friendly title from the data"
            );
        }
    }

    #[test]
    fn app2_prompt_is_smaller_than_overview() {
        // The overview is the big "wow" artifact — 4 tasks, markdown + 2 rows
        // with 4 display children. App2 should be a focused deep-dive with
        // fewer blocks so the two dashboards feel distinct, not redundant.
        let overview = ctx_with(OnboardingBuildStep::App).build_prompt();
        let deep_dive = ctx_with(OnboardingBuildStep::App2).build_prompt();
        assert!(
            overview.matches("- name:").count() > deep_dive.matches("- name:").count(),
            "overview must define more tasks than the deep-dive (overview={}, deep-dive={})",
            overview.matches("- name:").count(),
            deep_dive.matches("- name:").count()
        );
    }

    // ── Legacy SemanticLayer phase: keep it aligned with new schema ─────────

    #[test]
    fn legacy_semantic_layer_prompt_matches_current_schema() {
        // The legacy all-in-one phase is still the Default variant and can be
        // hit by older frontends or requests that omit the `step` field.
        // It must produce views that validate against the same rules as the
        // per-phase SemanticView path.
        let prompt = ctx_with(OnboardingBuildStep::SemanticLayer).build_prompt();
        assert!(
            prompt.contains("entities:") && prompt.contains("type: primary"),
            "legacy SemanticLayer prompt must require entities: + primary entity"
        );
        assert!(
            prompt.contains("expr:"),
            "legacy SemanticLayer prompt must use `expr:` for dimensions/measures"
        );
        assert!(
            !prompt.contains("sql: <column_or_expression>"),
            "legacy SemanticLayer prompt must not show `sql:` as a measure value"
        );
        assert!(
            prompt.contains(".topic.yml"),
            "legacy SemanticLayer prompt must instruct topic creation alongside views"
        );
    }
}
