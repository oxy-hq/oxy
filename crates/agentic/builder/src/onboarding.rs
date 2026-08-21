//! Onboarding prompt builder — constructs the builder agent prompt from
//! structured onboarding context (tables, warehouse type, model config).
//!
//! Six focused prompts are generated, one per build phase:
//! - `SemanticLayer` — inspect schemas, update config.yml, create .view.yml files (legacy, all-in-one)
//! - `Config`        — update config.yml only (model entry + database defaults)
//! - `SemanticView`  — inspect one table, create matching .view.yml + .topic.yml files
//! - `Agent`         — create analytics.agentic.yml (agentic analytics agent)
//! - `App`           — create apps/overview.app.yml (semantic_query-powered starter dashboard,
//!                     picked from a data-profile-driven scoring of every topic)
//! - `App2`          — create a complementary dashboard (cross-topic execute_sql JOIN
//!                     when an FK overlap is found, otherwise a single-topic deep-dive
//!                     on the first non-overview topic alphabetically). The output filename
//!                     is `apps/<topic1>_<topic2>.app.yml` for the cross-topic path or
//!                     `apps/<topic_slug>.app.yml` for the single-topic path. Only
//!                     triggered when the workspace has ≥ 2 topics.
//!
//! This keeps the prompt templates server-side so the frontend only sends
//! structured selections, not raw LLM instructions.

use serde::Deserialize;

use crate::prompts::KnowledgeCard;

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
    /// an optional fourth high-signal block) that showcases the user's data.
    /// The prompt profiles every topic (rows, cardinality, time coverage,
    /// stddev) and picks the one that passes the most fitness criteria,
    /// rather than blindly using the first topic alphabetically.
    App,
    /// Create a second `.app.yml` dashboard pivoted on a *different* topic
    /// than the overview. Filename is `apps/<topic1>_<topic2>.app.yml`
    /// (cross-topic FK-join path) or `apps/<topic_slug>.app.yml`
    /// (single-topic path) — never `apps/detail.app.yml`.
    ///
    /// The frontend only triggers this phase when the workspace has ≥ 2
    /// topics (i.e. the user selected ≥ 2 tables). The prompt looks for a
    /// shared entity key between the overview's view and a non-overview
    /// view; if it finds one, it generates a cross-topic story via
    /// `execute_sql` JOINs. If no FK overlap is viable, it falls back to a
    /// single-topic deep-dive on the first non-overview topic alphabetically.
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

    /// Reference cards to pre-populate into the builder solver's
    /// cached system prefix for this phase.  Each phase pulls in only
    /// the cards relevant to the artifact it produces, keeping the
    /// per-phase cache entry tight.  Interactive (non-onboarding)
    /// builder runs use no cards by default and rely on the
    /// `lookup_reference` tool.
    pub fn knowledge_cards(&self) -> Vec<KnowledgeCard> {
        use KnowledgeCard::*;
        match self.step {
            // config.yml has no opinionated card; the builder writes
            // the model entry and database default from inline guidance.
            OnboardingBuildStep::Config => vec![],
            // .view.yml + .topic.yml — both covered by the semantic-layer card.
            OnboardingBuildStep::SemanticView | OnboardingBuildStep::SemanticLayer => {
                vec![SemanticLayer]
            }
            // analytics.agentic.yml — covered by the agentic-builder card.
            OnboardingBuildStep::Agent => vec![AgenticBuilder],
            // .app.yml — needs both: app-builder for tasks/displays and
            // semantic-layer because tasks reference view fields.
            OnboardingBuildStep::App | OnboardingBuildStep::App2 => {
                vec![SemanticLayer, AppBuilder]
            }
        }
    }

    /// Tools to expose to the builder for this onboarding phase.
    /// Drops the 15+ dbt/airform tools, `search_text`, `run_tests`,
    /// and `manage_directory` from every phase, and trims warehouse
    /// tools (`execute_sql`, `semantic_query`) out of phases that
    /// don't need them.  Reduces tool-selection noise and shrinks the
    /// cached system prefix.
    pub fn tool_allowlist(&self) -> Vec<String> {
        // Common across every onboarding phase: file authoring, schema
        // reference, validation, and the HITL escape hatch.
        let common: &[&str] = &[
            "search_files",
            "read_file",
            "write_file",
            "edit_file",
            "delete_file",
            "validate_project",
            "lookup_reference",
            "lookup_schema",
            "ask_user",
        ];
        // Phases that touch the warehouse (DESCRIBE TABLE, smoke-test
        // queries, data profiling) need execute_sql + semantic_query.
        let warehouse: &[&str] = &["execute_sql", "semantic_query"];
        // App phases additionally need `run_app` to smoke-test the
        // generated dashboard end-to-end (catches runtime SQL errors that
        // schema validation misses — broken JOINs, dialect type mismatches).
        let app_smoke_test: &[&str] = &["execute_sql", "semantic_query", "run_app"];

        let extras: &[&str] = match self.step {
            OnboardingBuildStep::Config | OnboardingBuildStep::Agent => &[],
            OnboardingBuildStep::SemanticView | OnboardingBuildStep::SemanticLayer => warehouse,
            OnboardingBuildStep::App | OnboardingBuildStep::App2 => app_smoke_test,
        };

        common
            .iter()
            .chain(extras.iter())
            .map(|s| s.to_string())
            .collect()
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

Use `edit_file` for targeted updates to existing keys, or `write_file` if config.yml does not yet exist (or you must replace the entire file).

---

## Instructions

Read the existing config.yml first. Then propose changes to ensure it has:
{model_instructions}
- A `defaults.database` pointing to `{db_name}` if not set

Do NOT create any other files. Only update config.yml.

Prefer `edit_file` with a precise `old_string` / `new_string` pair for each missing or stale block — that avoids touching unrelated content. Only fall back to `write_file` if config.yml is missing or you genuinely need to replace the entire file. Whichever you choose, perform **exactly one** write call for config.yml — no revisions, no re-drafts.

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

        // Cross-table FK awareness: list every other selected table so the
        // agent can declare `type: foreign` entities pointing at views that
        // will exist in the same workspace. Without this the agent has no
        // way to know which `*_id` columns are real FKs vs opaque strings,
        // and onboarding-generated topics end up rendering raw UUIDs on
        // chart axes (no labeled join possible).
        let has_other_tables = self.tables.len() > 1;
        let other_tables_section = if has_other_tables {
            let others = self
                .tables
                .iter()
                .filter(|t| t.as_str() != table)
                .map(|t| {
                    let other_view = t.rsplit('.').next().unwrap_or(t.as_str());
                    format!("  - `{t}` (view name will be `{other_view}`)")
                })
                .collect::<Vec<_>>()
                .join("\n");
            format!(
                r#"

## Cross-table FK awareness

The user also selected these other tables in this workspace; matching `.view.yml` files will exist alongside the one you create now:
{others}

If this view has a column that is a foreign key into one of those tables (typical patterns: `<other>_id`, `<other>_guid`, or a column that obviously names another table like `restaurant_id` → restaurants), you MUST declare a `type: foreign` entity for it. The foreign entity's `name:` must match what the lookup view's primary entity will be named (use the lookup view's table name without prefixes — `restaurant`, `customer`, `order` — singular). The foreign entity's `key:` must reference the FK dimension on THIS view.

```yaml
entities:
  - name: <this_view_subject>
    type: primary
    key: <pk_dim>
  - name: <lookup_view_subject>      # e.g. `restaurant` if the lookup table is restaurants
    type: foreign
    key: <fk_dim_on_this_view>       # e.g. `restaurant_id`
```

Without this declaration the App phase has no way to surface the lookup view's name column on a chart axis and falls back to rendering raw UUIDs. The semantic-layer reference card has the full rule under "Critical rules" #5."#
            )
        } else {
            String::new()
        };

        let foreign_entity_bullet = if has_other_tables {
            "\n- Plus a `type: foreign` entity for every FK column pointing at one of the other selected tables (see \"Cross-table FK awareness\" above)."
        } else {
            ""
        };

        let topic_fk_views_comment = if has_other_tables {
            r#"
  # If you declared any `type: foreign` entities on the view above, also list
  # those lookup views here. Example: a fact view that declares a foreign
  # `restaurant` entity should include `restaurants` in this list, so the
  # analytics agent and apps can pull labels (e.g. `restaurants.location_name`)
  # via semantic_query instead of rendering raw FK UUIDs."#
        } else {
            ""
        };

        let topic_fk_motivation = if has_other_tables {
            " **Including FK-target views in `views:` is what unlocks human-readable labels on dashboards** — without it the App phase has to either ship UUIDs or fall back to raw SQL JOINs."
        } else {
            ""
        };

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
        let smoke_test_step = topic_step + 1;

        format!(
            r#"I need a semantic layer entry for a single table in my {db_name} warehouse.

Your task: **create two files** — a `.view.yml` and a matching `.topic.yml` — for table `{table}` — and smoke-test the result.

Use `write_file` for each new file (these are brand new — no existing content to preserve).

---

{schema_section}{other_tables_section}

## Step {view_step}: Create the view file

Create `semantics/{view_name}.view.yml`. Use:

- `name: {view_name}`
- `datasource: {db_name}`
- `table: "{table}"`
- One primary entity, plus 3–8 dimensions and 2–4 measures that make analytical sense for this table.{foreign_entity_bullet}

The full schema (entity rules, allowed dimension/measure types, `expr` requirements, naming conventions, **and the per-warehouse date-column recipes**) is in the `## Semantic layer reference` section of your system prompt — follow it exactly. Pick a primary-key dimension (id, uuid, or the most specific unique column).

**Date columns are the most common foot-gun on {db_name}.** Any column whose business meaning is a date or timestamp (`*_date`, `*_at`, `business_date`, `created`, `updated`, `event_time`) MUST be declared `type: date` (or `type: datetime`) and wrapped via the per-warehouse recipe in the reference card — not `type: number` or `type: string` over the raw column. Mismatch produces a silent `TYPE_MISMATCH` the first time the analytics agent filters on the dimension.

## Step {topic_step}: Create the topic file

Create `semantics/{view_name}.topic.yml`:

```yaml
name: {view_name}
description: "<one-line description of the business domain this topic covers>"
base_view: {view_name}
views:
  - {view_name}{topic_fk_views_comment}
```

Topics are what the analytics agent and dashboards query against — every view needs a matching topic.{topic_fk_motivation}

Call `write_file` **exactly once** per file (once for the view, once for the topic). Pass the full file contents in the `content` argument. Do not call `write_file` again for the same file — no revisions, no re-drafts.

## Step {smoke_test_step}: Smoke-test the view

Before declaring victory, prove the view actually works end-to-end by calling `semantic_query` against the topic you just created. The most failure-prone path is filtering on a date dimension, so target that:

1. Pick the primary date/datetime dimension on the view (if there is one) and the most business-interesting measure.
2. Call `semantic_query(topic="{view_name}", dimensions=["{view_name}.<date_dim>"], measures=["{view_name}.<primary_measure>"], limit=5)`.  No filter is needed — the goal is to prove the topic compiles and the dimension/measure expressions execute.
3. If the view has no date-like dimension, run `semantic_query(topic="{view_name}", measures=["{view_name}.<primary_measure>"], limit=1)` instead.

If the smoke test **succeeds**: stop, the phase is done.

If the smoke test **fails** (`TYPE_MISMATCH`, compile error, "column not found", etc.): diagnose the error, apply a single corrective `edit_file` to the view file (precise `old_string` / `new_string`), and re-run `semantic_query` once. If the second attempt still fails, stop and report the error — do NOT keep iterating.

After the smoke test passes (or you've stopped after one fix attempt), STOP — do NOT write a summary, explanation, or any follow-up text."#,
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

Use `write_file` for new files (the `.view.yml` / `.topic.yml` you author) and `edit_file` for targeted updates to existing files like `config.yml`.

---

## Step 1: Inspect table schemas

Use execute_sql with `DESCRIBE TABLE <table>` (or the {db_name}-equivalent) for each selected table.
Understand column names, types, and cardinality before creating any files.

## Step 2: Update config.yml

Read the existing config.yml first. Then propose changes to ensure it has:
{model_instructions}
- A `defaults.database` pointing to `{db_name}` if not set

## Step 3: Create semantic layer views

For each table, create a `.view.yml` file under `semantics/<view_name>.view.yml` and a matching `semantics/<view_name>.topic.yml` so the analytics agent can query it.

Use:

- `datasource: {db_name}`
- `table: "<fully_qualified_table_name>"` (matching one of the selected tables above)
- One primary entity, plus dimensions and measures that make analytical sense for the table's data.

The full schema (entity rules, allowed dimension/measure types, `expr` requirements, naming conventions, the matching topic shape, **and the per-warehouse date-column recipes**) is in the `## Semantic layer reference` section of your system prompt — follow it exactly. Pay particular attention to the date-column recipes: any `*_date`/`*_at` column on {db_name} that isn't already a Date type needs a wrapping cast, otherwise filters will fail at query time with `TYPE_MISMATCH`.

## Step 4: Smoke-test each view

After all view+topic pairs are written, prove each one actually works by calling `semantic_query` against the topic with its primary date dimension (if any) and primary measure: `semantic_query(topic=<topic_name>, dimensions=[<date_dim>], measures=[<primary_measure>], limit=5)`.  If a topic has no date-like dimension, just query the measure: `semantic_query(topic=<topic_name>, measures=[<primary_measure>], limit=1)`.

If any smoke test fails (`TYPE_MISMATCH`, compile error, "column not found"): diagnose, fix the view via a single `edit_file` (precise `old_string` / `new_string`), re-run the failing smoke test once. If the retry still fails, stop and report the error — do NOT keep iterating.

After all smoke tests pass (or you've stopped after one fix attempt per view), STOP — do NOT write a summary or explanation."#,
        )
    }

    // ── Phase 2: Analytics Agent ──────────────────────────────────────────────

    fn build_agent_prompt(&self) -> String {
        let db_name = &self.warehouse_type;
        let model_name = self.model_name();

        format!(
            r#"The semantic layer for the {db_name} warehouse has just been created — views (`semantics/*.view.yml`) and matching topics (`semantics/*.topic.yml`).

Your task for this step: **create the default agentic analytics agent**.

Use `write_file` ONCE to create `analytics.agentic.yml`.

---

The view + topic files were just created in the previous onboarding step, so you do NOT need to read them or verify they exist — go straight to creating the agentic file.  The `context:` glob in the template below wires the whole `semantics/` tree into the pipeline automatically; you do NOT need to enumerate topic names into the agent file.

## Create analytics.agentic.yml

Call `write_file` exactly once, targeting `analytics.agentic.yml` at the project root. The `content` argument must be the file body below **verbatim** — do not duplicate it, do not wrap it in another document, do not append a second copy.

This is the agentic analytics agent users will interact with to ask questions about their data. It runs a multi-step FSM pipeline (clarify → specify → generate SQL → execute → interpret) rather than a single LLM tool loop. See the `## Agentic agent reference` section in your system prompt for the full schema, per-state overrides, validation rules, and common errors.

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

Constraints:
- First line MUST be the `# yaml-language-server: $schema=…` directive shown above.
- Exactly one YAML document; each top-level key appears at most once.
- Do NOT create an `.agent.yml` file — the legacy classic-agent format is no longer used for onboarding.

After the single `write_file` call, STOP — do NOT call `write_file` again for this file, and do NOT write a summary or explanation."#,
        )
    }

    // ── Phase 3: Starter Dashboard App ─────────────────────────────────────────

    fn build_app_prompt(&self) -> String {
        let db_name = &self.warehouse_type;

        format!(
            r#"The semantic layer for {db_name} has been created. Each `.view.yml` has a matching `.topic.yml`.

Your task: **create `apps/overview.app.yml`** — a starter dashboard powered by the semantic layer.

This is the first artifact the user sees after onboarding, so every block must earn its place. The goal is **insight density**, not block count: a short, high-signal dashboard beats a long, generic one.

**Tool budget:** keep this efficient — read at most 4 `.topic.yml` files + their views, run at most 4 profiling SQL queries (one per topic), and call `write_file` exactly once. You have a 30-round tool loop budget; aim to finish well within it. Skip to step 3 once you've gathered enough signal — don't over-profile.

## Phase 1 — Discover and profile all topics

### Step 1: Read every topic and its view

List all `.topic.yml` files in `semantics/`. For **each** one, read it and its matching `.view.yml`. Record:
- Topic name and view name
- Table name (from the view's `table:` field)
- Every dimension: name, type, and `expr`
- Every measure: name, type, and `expr`

### Step 2: Run a profiling query for each topic

For each topic (profile **at most 4** — skip any beyond the fourth to keep runtime reasonable), build and run **one consolidated profiling query** against the underlying table. Derive every column identifier from the view's actual `expr:` fields — never guess column names.

The query must return in a **single SELECT**:
- `COUNT(*) AS rows`
- `COUNT(DISTINCT <entity_candidate_expr>) AS entity_card` — for the 1–2 most promising entity dimensions (string type, non-id)
- For each date/datetime dimension: `MIN(<expr>)`, `MAX(<expr>)`, `COUNT(DISTINCT DATE_TRUNC('month', <expr>)) AS month_count`
- For each numeric measure: `MIN(<expr>)`, `MAX(<expr>)`, `STDDEV(<expr>) AS measure_stddev`

Example (Postgres/DuckDB syntax) for a sales view (`table: public.sales`, entity dim `expr: store_name`, date dim `expr: week_date`, measure `expr: weekly_sales`):
```sql
SELECT
  COUNT(*) AS rows,
  COUNT(DISTINCT store_name) AS entity_card,
  MIN(week_date) AS min_date, MAX(week_date) AS max_date,
  COUNT(DISTINCT DATE_TRUNC('month', week_date)) AS month_count,
  MIN(weekly_sales) AS min_val, MAX(weekly_sales) AS max_val,
  STDDEV(weekly_sales) AS measure_stddev
FROM public.sales
```

**Use {db_name}-appropriate syntax** for date truncation and stddev. The full dialect matrix (BigQuery / Snowflake / Postgres / DuckDB / ClickHouse) lives in `## SQL dialect notes` in the app-builder reference already in your system context — consult it rather than guessing.

**On profiling-query failure:** follow `## Failure recovery` in the same reference — simplify and retry once, then skip the topic. Never loop on a failing query.

### Step 3: Pick the best topic for the overview

Evaluate each profiled topic. A topic is suitable for the overview when ALL of these hold:
- `rows` ≥ 100 (enough data for meaningful aggregation)
- At least one date dimension has `month_count` ≥ 3 (enough time for a trend line)
- The primary measure has `measure_stddev` > 0 (not a flat line — every row is NOT the same value)
- The best entity dimension has `entity_card` between 5 and 500 (useful top/bottom ranking)

**Rejection rules — skip a topic when:**
- The measure's STDDEV ≈ 0: every entity returns the same value. A horizontal trend chart at "1" or "0" is actively misleading — worse than no chart.
- `entity_card` < 5: top/bottom 10 tables would show the same rows, which is useless.
- The topic is a reference or dimension table (e.g., one row per job title with count = 1) — prefer a fact table with aggregatable measures.

Choose the topic that **passes the most criteria** above. If multiple topics pass all four, prefer the one with the highest `month_count` (more time coverage); break further ties alphabetically. If ALL topics fail the criteria, fall back to the first alphabetically and skip any block that cannot be made meaningful with the available data.

## Phase 2 — Confirm field selections

From the chosen topic's profiling results, confirm:
- **Primary metric (ONE measure)** — prefer `sum`/`average` over raw `count`. Must have STDDEV > 0.
- **Date dimension** — only include the trend chart if `month_count` ≥ 3. If not, omit `trend_over_time` entirely.
- **Entity dimension** — must have cardinality between 5 and 500.

Decide on the fourth block using the "Fourth-block decision" rules below.

## Phase 3 — Generate the app

Call `write_file` **exactly once** to create `apps/overview.app.yml`. Pass the full file contents in the `content` argument. Do not call `write_file` again — no revisions, no re-drafts, no second calls.

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

### Entity labeling — escape hatch when only a FK/UUID is available

Prefer `type: semantic_query` for every task. **One narrow exception:** if the only viable entity dimension is an opaque FK (column name ends in `_id` / `_guid`, samples look like UUIDs/hex strings) AND the chosen topic does NOT include a lookup view that exposes a name dimension for it, fall back to `type: execute_sql` for `top_performers` / `bottom_performers` (ONLY) with an explicit JOIN to the lookup view. Pull the lookup view's name column directly. Keep `trend_over_time` as `semantic_query` either way.

When you take this path:
- Locate the lookup view's `table:` and `datasource:`, plus its primary key column and a name-like dimension (`name`, `location_name`, `display_name`, `title`). If you already covered this view in Step 1's view scan, reuse what you recorded — no re-read needed. Re-read only when the lookup view wasn't part of the first 4 you looked at.
- Use {db_name}-appropriate JOIN syntax. Example skeleton (Postgres/DuckDB/ClickHouse):
  ```yaml
  - name: top_performers
    type: execute_sql
    database: <fact_view_datasource>
    sql_query: |
      SELECT
        r.<name_col> AS entity,
        SUM(t.<measure_col>) AS metric
      FROM <fact_table> t
      INNER JOIN <lookup_table> r ON t.<fk_col> = r.<lookup_pk_col>
      GROUP BY r.<name_col>
      ORDER BY metric DESC
      LIMIT 10
  ```
- Display refs for `execute_sql` task outputs are the **plain SQL aliases** — `x: entity`, `y: metric` — no double underscore. Mixing `execute_sql` aliases with `__` refs will silently break charts.
- The `formats:` map on a table for `execute_sql` outputs uses the alias name directly (e.g. `metric: currency`).

Use this fallback only for entity-labeling. Do not switch to `execute_sql` for trend, time series, or any task that the semantic layer can express. Better to ship a labeled top/bottom and a semantic trend than a fully raw-SQL dashboard.

### Number formatting

Pick a `DisplayFormat` per measure column based on what the measure actually represents. This is a judgement call — the measure name, its `description` in the `.view.yml`, its `type` (sum / average / count / …), and the business concept of the topic all inform the right answer. Do not treat the keywords below as an exhaustive checklist; treat them as examples.

- `currency` — any monetary quantity. Common signals: mentions of money, revenue, spend, cost, price, fees, billing, payments, GMV, ARR/MRR, LTV, AOV, ARPU, ACV, gross/net, a currency symbol or code in the description, or a `sum`/`average` measure over a column that is obviously dollars / euros / etc. When a data-literate user would naturally read the value with a `$`, use `currency`.
- `percent` — rates, shares, completion ratios, margins, attach/churn/conversion rates. Only when the underlying value is already scaled to 0–100 (a 0–1 ratio would render as `0.25%` with our current implementation, which is wrong — prefer plain `number` in that case, or omit the format).
- `number` — high-magnitude counts or integers that benefit from thousands separators (page views, sessions, users, orders, transactions, units sold, clicks). Use this for any `count` / `count_distinct` measure where values typically reach five digits or more, so `1234567` reads as `1,234,567`.
- Omit the format — small integer counts, already-formatted strings, or anything where formatting adds no clarity.

For charts (`line_chart` / `bar_chart`) set `y_format: <format>`; for the pie chart `value_format`; for tables use a `formats:` map keyed by the output column name, one entry per measure column on display. When two interpretations are plausible, pick the one a finance-literate user would expect — `total_weekly_sales` reads as currency, `session_count` reads as number, `conversion_rate_pct` reads as percent.

## Phase 4 — Smoke-test the generated dashboard

Schema validation only catches structural YAML errors; it does NOT catch malformed SQL that fails at runtime (broken JOIN syntax, dialect type mismatches, missing `ON` clauses). Onboarding leaves no chance for the user to fix things before opening the app, so this step is non-negotiable.

After the `write_file` call is accepted, call `run_app(file_path: "apps/overview.app.yml", params_json: "{{}}")` once. The tool runs every task in the app exactly the way the dashboard will on first load and reports per-task pass/fail.

- **All tasks pass:** stop — the phase is done. Proceed to the "Sample Questions" output below.
- **Any task fails:** read the error message verbatim. Diagnose against `## SQL dialect notes` and `## Failure recovery` in the app-builder reference (already in your system context). Common fixes:
  - Re-state a malformed JOIN: `INNER JOIN <table> AS <alias> ON <alias>.<col> = <other>.<col>`. The `ON` keyword is mandatory; the alias goes immediately after `AS`, before `ON`.
  - Replace a dialect-specific function with the matrix entry that matches `{db_name}` (e.g. ClickHouse uses `toStartOfMonth(col)` and `stddevPop()`, not `DATE_TRUNC` and `STDDEV`).
  - Drop a measure / dimension that the view doesn't actually expose and pick a different one.

  Call `edit_file` ONCE with the corrective `old_string` / `new_string` pair, and re-run `run_app` ONCE. If the second attempt still fails, stop and surface the error to the user — do NOT keep iterating.

After all tasks pass (or you've stopped after one fix attempt), output **only** a "Sample Questions" section with 5 numbered questions users could ask the analytics agent about this data. Nothing else."#,
        )
    }

    // ── Phase 3b: Cross-topic Deep-dive Dashboard ──────────────────────────────

    fn build_app2_prompt(&self) -> String {
        let db_name = &self.warehouse_type;

        format!(
            r#"The semantic layer for {db_name} has been created, and `apps/overview.app.yml` already exists.

Your task: **create a second dashboard that complements — and does not duplicate — the overview**. This phase only runs when the workspace has multiple topics, so you can assume at least two `.topic.yml` files exist.

**Tool budget (every read counts):** read `apps/overview.app.yml` once (Phase 1), then at most 6 semantic files total across Phase 2 (counting both `.topic.yml` reads to find each view's owning topic AND `.view.yml` reads). Run at most 2 SQL queries total (1 join overlap check + at most 1 profiling query) and call `write_file` exactly once. You have a 30-round tool loop budget — aim to finish well within it.

## Phase 1 — Determine the overview topic

Read `apps/overview.app.yml`. Find the `topic:` field in its first task — that is the topic the overview already covers. You must NOT use the same topic for this dashboard.

## Phase 2 — Discover entity key relationships (cap: 4 views)

Read up to 4 `.view.yml` files in `semantics/`. Always include the overview's view, plus up to 3 others (alphabetical order). For each view, record:
- View name and the `entities[0].key` dimension name
- The `expr:` of that key dimension (the actual column used in SQL)
- The `table:` and `datasource:` (needed for raw SQL JOINs)
- Whether it has a name-like dimension (`name`, `location_name`, `display_name`, `title`, etc.) — this matters for entity labeling on the chart axis

Look for an FK match: does the overview's view share an entity key (same `key:` name OR same underlying `expr:` column) with another view? Example: overview view `sales` has `key: store_id` / `expr: store_id`, and `labor` view has `key: store_id` / `expr: store_id` — these can be joined.

**Also identify a labeling lookup view** (optional, but strongly preferred): if a third view's primary key (`entities[0].key`) matches the same FK AND it has a name-like dimension, you should plan a 3-way JOIN so the bar chart's `x:` axis is the human-readable name (e.g. `restaurants.location_name`) rather than the raw FK UUID. Skipping this step is the most common reason cross-topic dashboards ship UUIDs on the axis.

If a candidate FK is found, run **one join overlap check** using {db_name}-appropriate syntax:
```sql
SELECT COUNT(*) AS overlap
FROM <table1> t1
INNER JOIN <table2> t2 ON t1.<key_expr> = t2.<key_expr>
```
A join is **viable** if `overlap > 0`. If the query errors (different schema, type mismatch, etc.), treat the join as not viable and proceed to the single-topic path — do not retry more than once.

## Phase 3 — Choose cross-topic or single-topic

### Cross-topic path (viable FK join found)

**Important:** Onboarding views only declare primary entities, so the semantic engine cannot auto-join them. Cross-topic apps must use raw `execute_sql` tasks — not `semantic_query` tasks — and you do NOT create a combined `.topic.yml`. Just create the app file with one or two `execute_sql` tasks that JOIN the underlying tables directly.

Tell a **cross-table story** — a metric that genuinely needs both tables (e.g., labor cost as % of revenue, headcount per sales dollar, cost vs. output by entity). Then call `write_file` **exactly once** to write `apps/<topic1>_<topic2>.app.yml` (e.g. `apps/sales_labor.app.yml`), passing the full file contents in the `content` argument.

Cross-topic app skeleton — **use the 3-way variant when a labeling lookup view exists**, otherwise fall back to the 2-way variant:

**3-way (preferred when a lookup view with a name dimension is available):**
```yaml
title: "<Business-friendly cross-topic name>"
description: "<one-line description of the cross-table story>"

tasks:
  - name: cross_topic_ranking
    type: execute_sql
    database: <datasource_name>     # from the view's `datasource:` field
    sql_query: |
      SELECT
        r.<lookup_name_col> AS entity,
        SUM(t1.<measure_col>) AS metric_a,
        SUM(t2.<measure_col>) AS metric_b,
        SUM(t2.<measure_col>) / NULLIF(SUM(t1.<measure_col>), 0) AS ratio
      FROM <table1> t1
      INNER JOIN <table2> t2 ON t1.<key_expr> = t2.<key_expr>
      INNER JOIN <lookup_table> r ON t1.<key_expr> = r.<lookup_pk_col>
      GROUP BY r.<lookup_name_col>
      ORDER BY ratio DESC
      LIMIT 15
```

**2-way fallback (only when no lookup view with a name dimension was found):**
```yaml
tasks:
  - name: cross_topic_ranking
    type: execute_sql
    database: <datasource_name>
    sql_query: |
      SELECT
        t1.<entity_col> AS entity,
        SUM(t1.<measure_col>) AS metric_a,
        SUM(t2.<measure_col>) AS metric_b,
        SUM(t2.<measure_col>) / NULLIF(SUM(t1.<measure_col>), 0) AS ratio
      FROM <table1> t1
      INNER JOIN <table2> t2 ON t1.<key_expr> = t2.<key_expr>
      GROUP BY 1
      ORDER BY ratio DESC
      LIMIT 15
```

The display block is the same either way:
```yaml
display:
  - type: markdown
    content: |
      # <Cross-table story title>
      <one-paragraph summary of what the dashboard shows>
  - type: bar_chart
    title: "<Cross-topic metric>"
    data: cross_topic_ranking
    x: entity
    y: ratio
  - type: table
    title: "Detail"
    data: cross_topic_ranking
```

For execute_sql tasks, display refs use the **column alias** as written in the SELECT (no double underscore — that convention only applies to semantic_query output columns). When you use the 3-way JOIN, `entity` carries the human-readable name; when you use the 2-way fallback, `entity` carries the raw FK and the chart axis WILL show UUIDs — prefer the 3-way variant whenever a lookup view is available.

### Single-topic path (no viable FK join)

Pick the **first non-overview topic alphabetically** as the candidate (do not burn extra profiling queries comparing topics — the budget caps you at one). **Profile that candidate before committing**: build and run **one consolidated profiling query** for it, using {db_name}-appropriate syntax. The query should return:
- `COUNT(*) AS rows`
- `COUNT(DISTINCT <entity_expr>) AS entity_card` — for the best candidate entity dimension
- For the candidate primary measure: `MIN(<expr>)`, `MAX(<expr>)`, `STDDEV(<expr>) AS measure_stddev`

A topic qualifies for the single-topic deep-dive when ALL of these hold:
- `rows` ≥ 100
- `entity_card` between 5 and 500
- `measure_stddev` > 0 (not flat / all-same value)

If a profiling query errors due to dialect issues, follow `## Failure recovery` in the app-builder reference (simplify, retry once, skip). Never loop on the same broken query.

If no remaining topic qualifies, pick the first non-overview topic alphabetically and skip any block that cannot be made meaningful with the available data.

Derive a snake_case slug from the chosen topic name (strip `.topic.yml`). Write to `apps/<topic_slug>.app.yml`. Example: topic `customers` → `apps/customers.app.yml`.

## Template (single-topic path)

Fill in every `<placeholder>` with real field names. Do NOT leave words like "Topic" or "Category" in the output.

The `title:` field is the dashboard's human-readable name — infer it from the data's actual business meaning. NEVER just title-case the slug: `raw_orders` → "Orders Deep Dive", `customers` → "Customer Insights". Keep it short (2–4 words), title-cased, business-friendly, and distinct from the overview's title.

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

### Dimension rules

- Binary or boolean flags: `*_flag`, `is_*`, `has_*`, any dimension with only 2 distinct values (0/1, true/false, yes/no) — never use as entity dimension.
- Dimensions with fewer than 3 distinct meaningful values — skip.
- Raw surrogate keys that are not human-readable — pick a name/title/label instead.

### Rules (violations break the dashboard)

- The topic you pick must NOT be the same topic the overview dashboard uses.
- Filename: `apps/<topic_slug>.app.yml`. Do NOT name the file `apps/detail.app.yml`.
- Task `dimensions` and `measures` references use a single dot: `<view_name>.<field_name>`.
- Display chart refs (`x:`, `y:`) for `semantic_query` task outputs use DOUBLE UNDERSCORE: `<view_name>__<field_name>`. For `execute_sql` tasks (the cross-topic path), refs are the plain column aliases as written in the SELECT — no double underscore. Mixing the two breaks chart rendering.
- `table` blocks only take `data:`, `title:`, and optionally `formats:` — no `x` / `y`.
- Reuse the same `<primary_measure>` across both tasks so the dashboard is coherent.
- For the single-topic path: 2 tasks, 1 markdown + 1 chart + 1 table. Cross-topic may add one more task if the story requires it.

### Number formatting

Pick a `DisplayFormat` for the primary measure based on what it actually represents.

- `currency` — monetary quantities (revenue, sales, spend, cost, price, fees, billing, GMV, ARR/MRR, LTV, AOV, ARPU, payments, margins-as-dollars). When a finance-literate user would naturally read the value with a `$`, use currency.
- `percent` — rates, shares, or ratios already scaled to 0–100 (conversion rate, churn rate, margin percentage). Do not use `percent` for a 0–1 ratio — prefer `number` or omit.
- `number` — high-magnitude counts or integers that benefit from thousands separators (`count` / `count_distinct` measures where values reach five digits or more).
- Omit — small integer counts or measures where formatting adds no clarity.

Set `y_format: <format>` on the bar chart as a sibling of `title:` / `data:`, and add a `formats:` map to the table. The map key matches whatever the task actually emits — for `semantic_query` tasks (single-topic path) that's `<view_name>__<primary_measure>`; for `execute_sql` tasks (cross-topic path) that's the plain SELECT alias (e.g. `metric`):
```yaml
# semantic_query (single-topic):
formats:
  <view_name>__<primary_measure>: <format>
# execute_sql (cross-topic):
formats:
  <plain_alias>: <format>
```

## Phase 4 — Smoke-test the generated dashboard

The cross-topic `execute_sql` path is where runtime SQL errors hide most often: handwritten JOINs against {db_name} can parse as YAML but fail on first dashboard load (broken JOIN syntax, dialect type mismatches, missing `ON` clauses). Onboarding leaves no chance for the user to fix things before opening the app, so this step is non-negotiable.

After the `write_file` call is accepted, call `run_app(file_path: "<the file you just wrote>", params_json: "{{}}")` once. The tool runs every task in the app exactly the way the dashboard will on first load and reports per-task pass/fail.

- **All tasks pass:** stop — the phase is done.
- **Any task fails:** read the error message verbatim. Diagnose against `## SQL dialect notes` and `## Failure recovery` in the app-builder reference (already in your system context). Common fixes for the cross-topic path:
  - Re-state a malformed JOIN: `INNER JOIN <table> AS <alias> ON <alias>.<col> = <other>.<col>`. The `ON` keyword is mandatory; the alias goes immediately after `AS`, before `ON`. Each JOIN gets its own `ON` clause.
  - Replace a dialect-specific function with the matrix entry that matches `{db_name}` (e.g. ClickHouse uses `toStartOfMonth(col)` and `stddevPop()`, not `DATE_TRUNC` and `STDDEV`).
  - Drop a measure / dimension that the view doesn't actually expose and pick a different one.

  Call `edit_file` ONCE with the corrective `old_string` / `new_string` pair, and re-run `run_app` ONCE. If the second attempt still fails, stop and surface the error to the user — do NOT keep iterating.

After all tasks pass (or you've stopped after one fix attempt), STOP — do NOT write a summary or explanation."#,
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
    fn agent_prompt_tells_builder_to_use_write_file() {
        let prompt = ctx_with(OnboardingBuildStep::Agent).build_prompt();
        assert!(
            prompt.contains("write_file"),
            "prompt should instruct the builder to use the write_file tool"
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
    fn semantic_view_prompt_points_at_reference_card() {
        // The full view/topic schema is now carried in the cached
        // semantic-layer reference card (see `crate::prompts::full_reference_context`
        // and the `knowledge_files_are_embedded_and_non_empty` test in
        // prompts.rs which pins entity / dimension / measure presence).
        // The user-facing prompt only needs to nudge the agent at it.
        let prompt = ctx_with(OnboardingBuildStep::SemanticView).build_prompt();
        assert!(
            prompt.contains("Semantic layer reference"),
            "SemanticView prompt must defer to the cached semantic-layer reference card; got:\n{prompt}"
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

    #[test]
    fn semantic_view_prompt_lists_other_selected_tables() {
        // When the user selected multiple tables, each per-table SemanticView
        // run must show the agent the FULL list so it can declare foreign
        // entities for FK columns pointing at those other tables. Without
        // this, FK-shape declarations are inconsistent across the views and
        // the App phase ends up rendering raw UUIDs on chart axes.
        let ctx = ctx_with(OnboardingBuildStep::SemanticView);
        let prompt = ctx.build_prompt();
        // `ctx_with` seeds two tables: "public.orders" and "public.customers".
        // The first is the target; the prompt must explicitly reference the
        // second under the cross-table awareness section.
        assert!(
            prompt.contains("Cross-table FK awareness"),
            "SemanticView prompt must include a Cross-table FK awareness section when >1 table is selected"
        );
        assert!(
            prompt.contains("public.customers"),
            "SemanticView prompt must list the other selected tables (e.g. `public.customers`); got:\n{prompt}"
        );
        // Must NOT echo the target table inside the "other tables" list — the
        // target is the one being built, not a sibling reference.
        let cross_section_start = prompt
            .find("Cross-table FK awareness")
            .expect("section guarded above");
        let section_tail = &prompt[cross_section_start..];
        // Cheap proxy: the bullet line for the target table starts with
        // "  - `public.orders`". It must not appear as a SIBLING (it does
        // appear elsewhere in the prompt as the build target).
        assert!(
            !section_tail.contains("- `public.orders`"),
            "Cross-table FK awareness section must not list the target table as a sibling; got:\n{section_tail}"
        );
    }

    #[test]
    fn semantic_view_prompt_omits_cross_table_section_when_single_table() {
        // The cross-table FK section adds prompt tokens; it should be
        // suppressed when only one table is selected to avoid wasted context.
        let mut ctx = ctx_with(OnboardingBuildStep::SemanticView);
        ctx.tables = vec!["public.orders".to_string()];
        let prompt = ctx.build_prompt();
        assert!(
            !prompt.contains("Cross-table FK awareness"),
            "SemanticView prompt must skip the Cross-table FK awareness section when only one table is selected"
        );
    }

    #[test]
    fn semantic_view_prompt_instructs_foreign_entity_declaration() {
        // The cross-table section must explicitly tell the agent to declare
        // a `type: foreign` entity for FK columns and explain why (so it
        // unlocks labeled axes downstream). Round-3 of #2206 added this rule
        // to the cards but the SemanticView phase wasn't surfacing it on
        // every run; without explicit per-prompt mention the rule fires
        // inconsistently.
        let prompt = ctx_with(OnboardingBuildStep::SemanticView).build_prompt();
        assert!(
            prompt.contains("type: foreign"),
            "SemanticView prompt must explicitly mention `type: foreign` entities for FK columns"
        );
        assert!(
            prompt.contains("foreign key") || prompt.contains("FK"),
            "SemanticView prompt must explain that the foreign-entity rule applies to FK columns"
        );
        // The rule must motivate WHY — without the why the agent treats it
        // as boilerplate and skips it on long views.
        assert!(
            prompt.contains("UUID") || prompt.contains("label"),
            "SemanticView prompt must motivate the foreign-entity rule by referencing UUID/label downstream impact"
        );
    }

    #[test]
    fn semantic_view_topic_template_documents_fk_target_views() {
        // Topic files only ever including `views: [<single view>]` is the
        // SECOND structural reason FK→label resolution fails — even when
        // foreign entities are declared, semantic_query can't reach the
        // lookup view's name dimension if the topic doesn't include it.
        // The topic template must explicitly tell the agent to add FK-target
        // views to the topic's `views:` list.
        let prompt = ctx_with(OnboardingBuildStep::SemanticView).build_prompt();
        assert!(
            prompt.contains("FK-target views")
                || prompt.contains("lookup views here")
                || prompt.contains("lookup view"),
            "SemanticView topic template must instruct adding FK-target views to the topic's views: list"
        );
    }

    // ── App phase: semantic_query + display reference guards ───────────────

    #[test]
    fn app_prompts_use_semantic_query_task_type() {
        // The overview app and App2's single-topic deep-dive must use
        // `semantic_query` for trends and the primary tasks. The App phase
        // is allowed a narrow `execute_sql` escape hatch ONLY for the
        // entity-labeling fallback (when the only viable entity dim is an
        // FK and the topic doesn't include a lookup view with a name
        // dimension). App2's cross-topic path is allowed to use
        // `execute_sql` with a raw JOIN, since onboarding views declare
        // only primary entities and the semantic engine cannot auto-join
        // them without explicit foreign-entity relationships.
        let app_prompt = ctx_with(OnboardingBuildStep::App).build_prompt();
        assert!(
            app_prompt.contains("type: semantic_query"),
            "App phase must use `type: semantic_query` tasks for the primary cuts"
        );
        // The App phase prompt may mention `type: execute_sql` only inside
        // the explicit "Entity labeling — escape hatch" section. Guard
        // that the trend task is unambiguously instructed to be
        // `semantic_query` (the labeling fallback applies only to top/bottom).
        assert!(
            app_prompt.contains("Keep `trend_over_time` as `semantic_query`"),
            "App phase must keep the trend task on semantic_query even when the labeling fallback fires"
        );
        assert!(
            app_prompt.contains("entity-labeling") || app_prompt.contains("Entity labeling"),
            "App phase must scope any `execute_sql` mention to the entity-labeling fallback"
        );

        // App2: must mention semantic_query for the single-topic path.
        // execute_sql is permitted only as part of the cross-topic JOIN path.
        let app2_prompt = ctx_with(OnboardingBuildStep::App2).build_prompt();
        assert!(
            app2_prompt.contains("type: semantic_query"),
            "App2 single-topic path must use `type: semantic_query`"
        );
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
    fn app_prompt_documents_entity_labeling_fallback() {
        // When the only viable entity dim is an FK/UUID and the topic
        // doesn't include a lookup view exposing a name dimension, the App
        // phase must permit (and instruct) an `execute_sql` JOIN fallback
        // for top/bottom — but ONLY for that case, and ONLY for the
        // ranking tasks. Trends must stay on semantic_query.
        let prompt = ctx_with(OnboardingBuildStep::App).build_prompt();
        assert!(
            prompt.contains("Entity labeling")
                || prompt.contains("entity-labeling")
                || prompt.contains("entity labeling"),
            "App phase must include an entity-labeling section that documents the execute_sql fallback"
        );
        assert!(
            prompt.contains("execute_sql"),
            "App phase must mention `execute_sql` as the fallback task type for entity labeling"
        );
        assert!(
            prompt.contains("INNER JOIN"),
            "App phase fallback must show an explicit INNER JOIN to the lookup view"
        );
        assert!(
            prompt.contains("Keep `trend_over_time` as `semantic_query`"),
            "App phase must clamp the fallback to top/bottom — trend stays on semantic_query"
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
        // App2 must pivot on a DIFFERENT topic than the overview and its
        // filename must be derived from that topic — never `apps/detail.app.yml`.
        // App2 reads the already-generated overview.app.yml to determine which
        // topic is taken, then discovers entity key (FK) relationships to
        // enable cross-topic stories when two views share a join key.
        let prompt = ctx_with(OnboardingBuildStep::App2).build_prompt();
        assert!(
            prompt.contains("apps/overview.app.yml") || prompt.contains("overview.app.yml"),
            "app2 prompt must read overview.app.yml to determine which topic the overview uses"
        );
        assert!(
            prompt.contains("apps/<topic_slug>.app.yml")
                || prompt.contains("apps/<topic_name>.app.yml")
                || prompt.contains("apps/<topic1>_<topic2>.app.yml"),
            "app2 prompt must name the file after the topic, not `detail`"
        );
        assert!(
            !prompt.contains("apps/detail.app.yml")
                || prompt.contains("Do NOT name the file `apps/detail.app.yml`")
                || prompt.contains("not name the file"),
            "app2 prompt must avoid writing to apps/detail.app.yml (or explicitly forbid it)"
        );
        assert!(
            prompt.contains("different topic") || prompt.contains("NOT be the same topic"),
            "app2 prompt must explicitly require a different topic than the overview"
        );
        assert!(
            prompt.contains("entity key")
                || prompt.contains("entities[0].key")
                || prompt.contains("FK")
                || prompt.contains("join"),
            "app2 prompt must check entity key relationships to enable cross-topic stories"
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
        // The overview is the big "wow" artifact — 4 tasks, markdown + chart + row.
        // App2's single-topic template is a focused 2-task deep-dive so the two
        // dashboards feel distinct. Cross-topic App2 may add one extra task but
        // the prompt template still references fewer `- name:` tasks than overview.
        let overview = ctx_with(OnboardingBuildStep::App).build_prompt();
        let deep_dive = ctx_with(OnboardingBuildStep::App2).build_prompt();
        assert!(
            overview.matches("- name:").count() > deep_dive.matches("- name:").count(),
            "overview template must define more tasks than the deep-dive single-topic template (overview={}, deep-dive={})",
            overview.matches("- name:").count(),
            deep_dive.matches("- name:").count()
        );
    }

    // ── App phase: data-aware profiling guards ─────────────────────────────

    #[test]
    fn app_prompt_reads_all_topics_not_just_first() {
        // The old prompt only read the first topic alphabetically, producing
        // trivially wrong apps when the first topic was a sparse dimension table
        // (e.g., a jobs reference table with count = 1 per row). The new prompt
        // reads ALL topics and picks the best one based on profiling results.
        let prompt = ctx_with(OnboardingBuildStep::App).build_prompt();
        assert!(
            !prompt.contains("Read the **first** one and its matching"),
            "app phase must no longer hard-code reading only the first topic alphabetically"
        );
        assert!(
            prompt.contains("For **each** one")
                || prompt.contains("for each")
                || prompt.contains("For each"),
            "app phase must instruct reading every topic file, not just the first"
        );
    }

    #[test]
    fn app_prompt_requires_data_profiling_before_field_selection() {
        // Without profiling, the LLM picks fields by name alone and produces
        // flat charts (stddev ≈ 0) or useless top/bottom tables (cardinality < 5).
        // The prompt must now require a profiling SQL query per topic before
        // committing to any field selection.
        let prompt = ctx_with(OnboardingBuildStep::App).build_prompt();
        assert!(
            prompt.contains("STDDEV")
                || prompt.contains("stddev")
                || prompt.contains("measure_stddev"),
            "app phase must require a STDDEV profiling query to detect flat/all-same measures"
        );
        assert!(
            prompt.contains("profiling")
                || prompt.contains("consolidated")
                || prompt.contains("COUNT(DISTINCT"),
            "app phase must require a consolidated profiling query to check entity cardinality"
        );
        assert!(
            prompt.contains("DATE_TRUNC") || prompt.contains("month_count"),
            "app phase must require checking time coverage (distinct months) before adding a trend chart"
        );
    }

    #[test]
    fn app_prompt_rejects_flat_measures_and_sparse_topics() {
        // The key fix for the "restaurant jobs" failure case: the prompt must
        // explicitly tell the LLM to skip measures where stddev ≈ 0 (every entity
        // returns the same value) and topics that are reference/dimension tables.
        let prompt = ctx_with(OnboardingBuildStep::App).build_prompt();
        assert!(
            (prompt.contains("stddev") || prompt.contains("STDDEV"))
                && (prompt.contains("flat")
                    || prompt.contains("same value")
                    || prompt.contains("horizontal")),
            "app phase must reject measures with stddev ≈ 0 as producing flat/useless charts"
        );
        assert!(
            prompt.contains("dimension table")
                || prompt.contains("reference")
                || prompt.contains("count = 1"),
            "app phase must warn against using reference/dimension tables as the overview source"
        );
    }

    // ── App2 phase: cross-topic and FK discovery guards ─────────────────────

    #[test]
    fn app2_prompt_reads_overview_to_avoid_duplicate_topic() {
        // App2 must read the already-generated overview.app.yml to find which
        // topic is already covered, rather than relying on alphabetical ordering.
        // This prevents both apps from covering the same business concept.
        let prompt = ctx_with(OnboardingBuildStep::App2).build_prompt();
        assert!(
            prompt.contains("overview.app.yml"),
            "app2 phase must read apps/overview.app.yml to determine the overview topic"
        );
        assert!(
            prompt.contains("topic:") || prompt.contains("`topic:`"),
            "app2 phase must instruct the LLM to extract the topic field from overview.app.yml"
        );
    }

    #[test]
    fn app2_prompt_discovers_entity_key_relationships_for_cross_topic() {
        // App2 now checks for FK-style join opportunities between views so it
        // can produce a cross-topic story (e.g., revenue × labor cost) when the
        // data supports it, rather than always defaulting to a single-topic view.
        let prompt = ctx_with(OnboardingBuildStep::App2).build_prompt();
        assert!(
            prompt.contains("entity key")
                || prompt.contains("entities[0].key")
                || prompt.contains("FK"),
            "app2 phase must check entity key relationships across views"
        );
        assert!(
            prompt.contains("join") || prompt.contains("JOIN") || prompt.contains("overlap"),
            "app2 phase must verify join viability with a SQL overlap check"
        );
    }

    #[test]
    fn app2_cross_topic_skeleton_supports_lookup_join() {
        // The cross-topic execute_sql skeleton must show a 3-way JOIN
        // variant that pulls a name column from a labeling lookup view.
        // Without this, the fact-fact JOIN produces a chart with the FK
        // (often a UUID) on the x-axis. The 2-way fallback is still
        // documented for the case where no lookup view with a name dim
        // exists, but the 3-way must be the preferred shape.
        let prompt = ctx_with(OnboardingBuildStep::App2).build_prompt();
        assert!(
            prompt.contains("3-way") || prompt.contains("labeling lookup view"),
            "App2 cross-topic skeleton must document the 3-way JOIN variant for entity labeling"
        );
        // Look for the lookup-view JOIN line in the skeleton (the third
        // INNER JOIN with a `r.<lookup_pk_col>`-shaped clause).
        assert!(
            prompt.contains("lookup_table") || prompt.contains("<lookup_table>"),
            "App2 cross-topic skeleton must reference a `<lookup_table>` placeholder in the 3-way JOIN"
        );
        assert!(
            prompt.contains("name-like dimension")
                || prompt.contains("name_col")
                || prompt.contains("location_name"),
            "App2 cross-topic skeleton must reference a name-like lookup column for the entity axis"
        );
    }

    #[test]
    fn app2_cross_topic_uses_raw_sql_join_not_combined_topic() {
        // Onboarding views only declare primary entities, so the semantic
        // engine cannot auto-join them. The cross-topic path therefore uses
        // raw `execute_sql` with an INNER JOIN against the underlying tables —
        // NOT a combined `.topic.yml` with multiple views (which would fail
        // semantic-layer validation with "not reachable via joins").
        let prompt = ctx_with(OnboardingBuildStep::App2).build_prompt();
        assert!(
            prompt.contains("execute_sql")
                && (prompt.contains("INNER JOIN") || prompt.contains("JOIN")),
            "app2 cross-topic path must use execute_sql with a JOIN against underlying tables"
        );
        // The cross-topic path must NOT instruct creating a combined topic file.
        assert!(
            !prompt.contains("Create `semantics/<topic1>_<topic2>.topic.yml`"),
            "app2 cross-topic path must not instruct creating a combined .topic.yml — onboarding views lack the foreign-entity declarations needed for auto-join"
        );
        assert!(
            prompt.contains("primary entities")
                || prompt.contains("primary entity")
                || prompt.contains("cannot auto-join")
                || prompt.contains("auto-join them"),
            "app2 prompt must explain why semantic auto-join doesn't work for onboarding views"
        );
    }

    #[test]
    fn app2_prompt_caps_view_reads_for_tool_budget() {
        // App2's view-discovery loop must cap how many .view.yml files it reads
        // so it doesn't blow the 30-round tool budget on workspaces with many
        // tables. The cap should be explicit in the prompt.
        let prompt = ctx_with(OnboardingBuildStep::App2).build_prompt();
        assert!(
            prompt.contains("at most 4")
                || prompt.contains("up to 4")
                || prompt.contains("(cap: 4")
                || prompt.contains("Tool budget"),
            "app2 prompt must cap view-file reads (and announce a tool budget) to avoid runaway tool-loop usage"
        );
    }

    #[test]
    fn app2_single_topic_path_requires_measure_profiling() {
        // The single-topic deep-dive must verify its candidate measure has
        // STDDEV > 0 before committing — otherwise it can fall into the same
        // "all-1s flat chart" trap that motivated this whole prompt overhaul.
        let prompt = ctx_with(OnboardingBuildStep::App2).build_prompt();
        assert!(
            prompt.contains("STDDEV")
                || prompt.contains("measure_stddev")
                || prompt.contains("stddev"),
            "app2 single-topic path must run a STDDEV profiling query before committing to a measure"
        );
        assert!(
            prompt.contains("profiling")
                || prompt.contains("Profile")
                || prompt.contains("profile"),
            "app2 single-topic path must explicitly instruct profiling the candidate topic"
        );
    }

    #[test]
    fn app_prompts_mention_sql_dialect_awareness() {
        // The example profiling SQL is Postgres-flavored. Without dialect
        // guidance, the LLM may emit DATE_TRUNC('month', col) on BigQuery
        // (which uses column-first syntax) or ClickHouse (which uses
        // toStartOfMonth) and silently fail. Both prompts must mention
        // dialect awareness.
        for step in [OnboardingBuildStep::App, OnboardingBuildStep::App2] {
            let prompt = ctx_with(step.clone()).build_prompt();
            assert!(
                prompt.contains("BigQuery")
                    || prompt.contains("ClickHouse")
                    || prompt.contains("dialect")
                    || prompt.contains("appropriate syntax"),
                "phase {step:?} must mention SQL dialect concerns so profiling queries don't silently fail"
            );
        }
    }

    #[test]
    fn app_prompts_specify_profiling_failure_fallback() {
        // If a profiling query errors (dialect, type, missing function), the
        // LLM must NOT loop on the failing query. The prompt must give
        // explicit fallback guidance: simplify, retry once, then skip.
        let prompt = ctx_with(OnboardingBuildStep::App).build_prompt();
        assert!(
            prompt.contains("error") || prompt.contains("fail"),
            "app prompt must address profiling-query failures explicitly"
        );
        assert!(
            prompt.contains("skip") || prompt.contains("simplify") || prompt.contains("move on"),
            "app prompt must instruct the LLM to skip / simplify on profiling failure rather than loop"
        );
    }

    // ── Legacy SemanticLayer phase: keep it aligned with new schema ─────────

    // ── knowledge_cards() per phase ─────────────────────────────────────────

    #[test]
    fn onboarding_context_knowledge_cards_per_phase() {
        use KnowledgeCard::*;
        let cases: &[(OnboardingBuildStep, &[KnowledgeCard])] = &[
            (OnboardingBuildStep::Config, &[]),
            (OnboardingBuildStep::SemanticView, &[SemanticLayer]),
            (OnboardingBuildStep::SemanticLayer, &[SemanticLayer]),
            (OnboardingBuildStep::Agent, &[AgenticBuilder]),
            (OnboardingBuildStep::App, &[SemanticLayer, AppBuilder]),
            (OnboardingBuildStep::App2, &[SemanticLayer, AppBuilder]),
        ];
        for (step, expected) in cases {
            let cards = ctx_with(step.clone()).knowledge_cards();
            assert_eq!(
                cards.as_slice(),
                *expected,
                "phase {step:?} returned wrong knowledge_cards"
            );
        }
    }

    #[test]
    fn onboarding_tool_allowlist_drops_dbt_and_irrelevant_tools() {
        // Every phase MUST exclude the 15+ dbt/airform tools, search_text,
        // run_tests, and manage_directory.  These belong to the chat
        // builder's full kit and only confuse the onboarding model.
        let banned = [
            "search_text",
            "run_tests",
            "manage_directory",
            "list_dbt_projects",
            "list_dbt_nodes",
            "compile_dbt_model",
            "run_dbt_models",
            "test_dbt_models",
            "get_dbt_lineage",
            "analyze_dbt_project",
            "get_dbt_column_lineage",
            "parse_dbt_project",
            "seed_dbt_project",
            "debug_dbt_project",
            "clean_dbt_project",
            "docs_generate_dbt",
            "format_dbt_sql",
            "init_dbt_project",
        ];
        for step in [
            OnboardingBuildStep::Config,
            OnboardingBuildStep::SemanticView,
            OnboardingBuildStep::SemanticLayer,
            OnboardingBuildStep::Agent,
            OnboardingBuildStep::App,
            OnboardingBuildStep::App2,
        ] {
            let allowlist = ctx_with(step.clone()).tool_allowlist();
            for bad in &banned {
                assert!(
                    !allowlist.iter().any(|t| t == bad),
                    "phase {step:?} surfaces banned tool {bad}"
                );
            }
        }
    }

    #[test]
    fn onboarding_tool_allowlist_includes_common_tools_every_phase() {
        // The minimal common set: file authoring, schema/reference
        // lookup, validation, HITL escape hatch.  Every phase needs
        // these regardless of artifact type.
        let common = [
            "search_files",
            "read_file",
            "write_file",
            "edit_file",
            "delete_file",
            "validate_project",
            "lookup_reference",
            "lookup_schema",
            "ask_user",
        ];
        for step in [
            OnboardingBuildStep::Config,
            OnboardingBuildStep::SemanticView,
            OnboardingBuildStep::SemanticLayer,
            OnboardingBuildStep::Agent,
            OnboardingBuildStep::App,
            OnboardingBuildStep::App2,
        ] {
            let allowlist = ctx_with(step.clone()).tool_allowlist();
            for needed in &common {
                assert!(
                    allowlist.iter().any(|t| t == needed),
                    "phase {step:?} missing required tool {needed}"
                );
            }
        }
    }

    #[test]
    fn onboarding_tool_allowlist_warehouse_phases_get_sql_and_semantic_query() {
        // Phases that touch the warehouse need execute_sql (DESCRIBE,
        // smoke test, profiling) and semantic_query (smoke test,
        // verification).  Phases that don't touch the warehouse must
        // NOT be exposed to those tools.
        let warehouse_phases = [
            OnboardingBuildStep::SemanticView,
            OnboardingBuildStep::SemanticLayer,
            OnboardingBuildStep::App,
            OnboardingBuildStep::App2,
        ];
        let non_warehouse_phases = [OnboardingBuildStep::Config, OnboardingBuildStep::Agent];

        for step in warehouse_phases {
            let allowlist = ctx_with(step.clone()).tool_allowlist();
            assert!(allowlist.iter().any(|t| t == "execute_sql"));
            assert!(allowlist.iter().any(|t| t == "semantic_query"));
        }
        for step in non_warehouse_phases {
            let allowlist = ctx_with(step.clone()).tool_allowlist();
            assert!(!allowlist.iter().any(|t| t == "execute_sql"));
            assert!(!allowlist.iter().any(|t| t == "semantic_query"));
        }
    }

    // ── App phases: run_app smoke test ──────────────────────────────────────

    #[test]
    fn app_phases_allowlist_includes_run_app() {
        // The smoke-test tool catches runtime SQL errors that schema
        // validation misses (broken JOINs, dialect type mismatches).
        // Onboarding's App / App2 phases are the ones the user opens
        // immediately after onboarding, so they must surface this tool.
        for step in [OnboardingBuildStep::App, OnboardingBuildStep::App2] {
            let allowlist = ctx_with(step.clone()).tool_allowlist();
            assert!(
                allowlist.iter().any(|t| t == "run_app"),
                "phase {step:?} must expose run_app for the smoke test"
            );
        }
    }

    #[test]
    fn non_app_phases_do_not_expose_run_app() {
        // Other phases produce different artifact types — exposing the
        // app smoke-test tool there would be tool-list noise.
        for step in [
            OnboardingBuildStep::Config,
            OnboardingBuildStep::Agent,
            OnboardingBuildStep::SemanticView,
            OnboardingBuildStep::SemanticLayer,
        ] {
            let allowlist = ctx_with(step.clone()).tool_allowlist();
            assert!(
                !allowlist.iter().any(|t| t == "run_app"),
                "phase {step:?} must NOT expose run_app"
            );
        }
    }

    #[test]
    fn app_prompt_includes_smoke_test_phase() {
        let prompt = ctx_with(OnboardingBuildStep::App).build_prompt();
        assert!(
            prompt.contains("Phase 4"),
            "App prompt must include the Phase 4 smoke-test step"
        );
        assert!(
            prompt.contains("run_app"),
            "App prompt must instruct calling run_app"
        );
        // The smoke-test step pins the file path so the tool call doesn't
        // require the model to invent it.
        assert!(
            prompt.contains("apps/overview.app.yml"),
            "App prompt must name the file the smoke test should target"
        );
        // Bound iteration: one corrective change, one retry, then stop.
        assert!(
            prompt.contains("ONCE") && prompt.to_lowercase().contains("do not keep iterating"),
            "App prompt must bound the smoke-test retry loop"
        );
    }

    #[test]
    fn app2_prompt_includes_smoke_test_phase() {
        let prompt = ctx_with(OnboardingBuildStep::App2).build_prompt();
        assert!(
            prompt.contains("Phase 4"),
            "App2 prompt must include the Phase 4 smoke-test step"
        );
        assert!(
            prompt.contains("run_app"),
            "App2 prompt must instruct calling run_app"
        );
        // Bound iteration: one corrective change, one retry, then stop.
        assert!(
            prompt.contains("ONCE") && prompt.to_lowercase().contains("do not keep iterating"),
            "App2 prompt must bound the smoke-test retry loop"
        );
    }

    // ── Smoke-test step in SemanticView prompt ──────────────────────────────

    #[test]
    fn semantic_view_prompt_includes_smoke_test_step() {
        // The smoke-test step is the safety net for date type-mismatch
        // and similar runtime errors that pass structural validation
        // but fail at first analytics query.
        let prompt = ctx_with(OnboardingBuildStep::SemanticView).build_prompt();
        assert!(
            prompt.contains("Smoke-test"),
            "SemanticView prompt must include a smoke-test step"
        );
        assert!(
            prompt.contains("semantic_query"),
            "SemanticView prompt must instruct calling semantic_query for the smoke test"
        );
        assert!(
            prompt.to_lowercase().contains("type_mismatch") || prompt.contains("TYPE_MISMATCH"),
            "SemanticView prompt must name the TYPE_MISMATCH failure mode"
        );
    }

    #[test]
    fn semantic_view_prompt_with_prefetched_schema_renumbers_smoke_test() {
        // The pre-fetched-schema path inlines the column list and skips the
        // DESCRIBE TABLE step, so downstream steps shift down by one:
        // Step 1 = create view, Step 2 = create topic, Step 3 = smoke test.
        // ctx_with()'s default `table_schema: None` exercises the
        // DESCRIBE-first path; this test pins the renumbered path so the
        // smoke-test step survives and no DESCRIBE instruction bleeds in.
        let mut ctx = ctx_with(OnboardingBuildStep::SemanticView);
        ctx.table_schema = Some(vec![
            TableColumnDef {
                name: "order_id".to_string(),
                column_type: "VARCHAR".to_string(),
            },
            TableColumnDef {
                name: "order_date".to_string(),
                column_type: "DATE".to_string(),
            },
            TableColumnDef {
                name: "amount".to_string(),
                column_type: "DECIMAL(10,2)".to_string(),
            },
        ]);
        let prompt = ctx.build_prompt();

        // Pre-fetched schema is inlined, DESCRIBE is skipped.
        assert!(
            prompt.contains("Table schema (pre-fetched)"),
            "pre-fetched schema section must be present"
        );
        assert!(
            !prompt.contains("DESCRIBE TABLE"),
            "DESCRIBE TABLE instruction must NOT appear when table_schema is supplied"
        );
        assert!(
            prompt.contains("`order_date` (DATE)"),
            "pre-fetched column list must be inlined verbatim"
        );

        // Step renumbering: view = 1, topic = 2, smoke test = 3.
        assert!(
            prompt.contains("## Step 1: Create the view file"),
            "view-creation should be Step 1 on the pre-fetched path"
        );
        assert!(
            prompt.contains("## Step 2: Create the topic file"),
            "topic-creation should be Step 2 on the pre-fetched path"
        );
        assert!(
            prompt.contains("## Step 3: Smoke-test the view"),
            "smoke-test must remain at the end (Step 3) on the pre-fetched path"
        );

        // Sanity: the smoke-test substance still references semantic_query
        // and the TYPE_MISMATCH failure mode (the safety net is intact).
        assert!(prompt.contains("semantic_query"));
        assert!(prompt.contains("TYPE_MISMATCH"));
    }

    #[test]
    fn semantic_view_prompt_warns_about_date_columns() {
        // The most common silent foot-gun: integer-encoded date columns.
        let prompt = ctx_with(OnboardingBuildStep::SemanticView).build_prompt();
        assert!(
            prompt.contains("date") && prompt.contains("type:"),
            "SemanticView prompt must warn about date-column type/expr handling"
        );
    }

    #[test]
    fn legacy_semantic_layer_prompt_includes_smoke_test_step() {
        // Same safety net for the legacy all-in-one phase.
        let prompt = ctx_with(OnboardingBuildStep::SemanticLayer).build_prompt();
        assert!(
            prompt.contains("Smoke-test") || prompt.contains("smoke-test"),
            "legacy SemanticLayer prompt must include smoke-test step"
        );
        assert!(prompt.contains("semantic_query"));
    }

    #[test]
    fn agent_prompt_does_not_require_pre_write_file_reads() {
        // Onboarding just created the view + topic files in the prior
        // phase; instructing the agent to "read at least one .topic.yml
        // and one .view.yml" before authoring the agentic file is
        // wasted tool calls.  The instruction has been replaced with a
        // direct "go straight to creating the agentic file".
        let prompt = ctx_with(OnboardingBuildStep::Agent).build_prompt();
        assert!(
            !prompt.contains("Read at least one"),
            "Agent prompt should not require pre-write file reads"
        );
    }

    #[test]
    fn legacy_semantic_layer_prompt_points_at_reference_card() {
        // The legacy all-in-one phase is still the Default variant and can be
        // hit by older frontends or requests that omit the `step` field. It
        // must defer to the same cached reference card the per-phase path
        // does, so both branches produce schema-compliant views.
        let prompt = ctx_with(OnboardingBuildStep::SemanticLayer).build_prompt();
        assert!(
            prompt.contains("Semantic layer reference"),
            "legacy SemanticLayer prompt must defer to the cached semantic-layer reference card"
        );
        assert!(
            prompt.contains(".topic.yml"),
            "legacy SemanticLayer prompt must instruct topic creation alongside views"
        );
    }
}
