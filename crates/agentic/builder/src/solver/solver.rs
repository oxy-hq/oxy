use std::path::{Path, PathBuf};
use std::sync::Arc;

use agentic_core::{
    events::{Event, EventStream},
    human_input::{DeferredInputProvider, HumanInputHandle, ResumeInput, SuspendedRunData},
    tools::{ToolError, ToolOutput},
};
use agentic_llm::{InitialMessages, LlmClient, ToolLoopConfig};

use crate::{
    app_runner::BuilderAppRunner,
    database::BuilderDatabaseProvider,
    events::BuilderEvent,
    schema_provider::BuilderSchemaProvider,
    secrets::BuilderSecretsProvider,
    semantic::BuilderSemanticCompiler,
    test_runner::BuilderTestRunner,
    tools::{
        execute_analyze_dbt_project, execute_clean_dbt_project, execute_compile_dbt_model_all,
        execute_compile_dbt_model_single, execute_debug_dbt_project, execute_delete_file,
        execute_docs_generate_dbt, execute_edit_file, execute_execute_sql, execute_format_dbt_sql,
        execute_get_dbt_column_lineage, execute_get_dbt_lineage, execute_init_dbt_project,
        execute_list_dbt_nodes, execute_list_dbt_projects, execute_lookup_schema,
        execute_manage_directory, execute_parse_dbt_project, execute_read_file, execute_run_app,
        execute_run_dbt_models, execute_run_tests, execute_search_files, execute_search_text,
        execute_seed_dbt_project, execute_semantic_query, execute_test_dbt_models,
        execute_validate_project, execute_write_file,
    },
    types::{BuilderSpec, ConversationTurn, ToolExchange},
    validator::BuilderProjectValidator,
};

pub struct BuilderSolver {
    pub(crate) client: LlmClient,
    pub(crate) project_root: PathBuf,
    pub(crate) event_tx: Option<EventStream<BuilderEvent>>,
    pub(crate) test_runner: Option<Arc<dyn BuilderTestRunner>>,
    pub(crate) human_input: HumanInputHandle,
    pub(crate) suspension_data: Option<SuspendedRunData>,
    pub(crate) resume_data: Option<ResumeInput>,
    pub(crate) db_provider: Option<Arc<dyn BuilderDatabaseProvider>>,
    pub(crate) project_validator: Option<Arc<dyn BuilderProjectValidator>>,
    pub(crate) schema_provider: Option<Arc<dyn BuilderSchemaProvider>>,
    pub(crate) semantic_compiler: Option<Arc<dyn BuilderSemanticCompiler>>,
    pub(crate) secrets_provider: Option<Arc<dyn BuilderSecretsProvider>>,
    pub(crate) app_runner: Option<Arc<dyn BuilderAppRunner>>,
}

impl BuilderSolver {
    pub fn new(client: LlmClient, project_root: PathBuf) -> Self {
        Self {
            client,
            project_root,
            event_tx: None,
            test_runner: None,
            human_input: Arc::new(DeferredInputProvider),
            suspension_data: None,
            resume_data: None,
            db_provider: None,
            project_validator: None,
            schema_provider: None,
            semantic_compiler: None,
            secrets_provider: None,
            app_runner: None,
        }
    }

    pub fn with_db_provider(mut self, provider: Arc<dyn BuilderDatabaseProvider>) -> Self {
        self.db_provider = Some(provider);
        self
    }

    pub fn with_project_validator(mut self, validator: Arc<dyn BuilderProjectValidator>) -> Self {
        self.project_validator = Some(validator);
        self
    }

    pub fn with_schema_provider(mut self, provider: Arc<dyn BuilderSchemaProvider>) -> Self {
        self.schema_provider = Some(provider);
        self
    }

    pub fn with_semantic_compiler(mut self, compiler: Arc<dyn BuilderSemanticCompiler>) -> Self {
        self.semantic_compiler = Some(compiler);
        self
    }

    pub fn with_events(mut self, tx: EventStream<BuilderEvent>) -> Self {
        self.event_tx = Some(tx);
        self
    }

    pub fn with_test_runner(mut self, runner: Arc<dyn BuilderTestRunner>) -> Self {
        self.test_runner = Some(runner);
        self
    }

    pub fn with_human_input(mut self, provider: HumanInputHandle) -> Self {
        self.human_input = provider;
        self
    }

    pub fn with_secrets_provider(mut self, provider: Arc<dyn BuilderSecretsProvider>) -> Self {
        self.secrets_provider = Some(provider);
        self
    }

    pub fn with_app_runner(mut self, runner: Arc<dyn BuilderAppRunner>) -> Self {
        self.app_runner = Some(runner);
        self
    }

    pub(crate) fn build_solving_system_prompt(&self) -> String {
        let root = self.project_root.to_string_lossy();
        format!(
            r#"You are a copilot for an Oxygen data project located at: {root}

Oxygen is a data platform. A project is a directory of YAML configuration files that define
agents, workflows, semantic models, and data apps. You help users read, understand, and
modify these files.

## Project file types

- config.yml — Main config: database connections, LLM models, default settings, integrations (Slack, MCP, A2A)
- <name>.agent.yml — LLM agent: model, system_instructions, tools (execute_sql, visualize, retrieval, workflow, semantic_search), context files, tests.
- <name>.procedure.yml / <name>.workflow.yml / <name>.automation.yml — Multi-step workflow: variables (JSON Schema), tasks (execute_sql, agent, formatter, loop_sequential…), tests.
- <name>.aw.yml — FSM-based agentic workflow: model, start/end states, transitions (query, semantic_query, visualize, insight, save_automation), optional routing.
- <name>.app.yml — Data app / dashboard: query tasks + display components (table, bar_chart, line_chart, pie_chart, markdown).
- <name>.topic.yml — Semantic topic: groups related views into a domain. Lives in semantics/.
- <name>.view.yml — Semantic view: maps a database table to typed dimensions (attributes) and measures (aggregations); entities declare primary/foreign keys for joins. Lives in semantics/.
- *.sql — SQL query files referenced by agents or workflows.
- <name>.test.yml — Test suite for an agent or agentic workflow: target file, settings (runs, judge_model), and test cases (prompt, expected, tags).

## Available tools

- search_files(pattern): find files by glob pattern (e.g. "agents/*.agent.yml", "**/*.view.yml", "**/*.test.yml")
- read_file(file_path, offset?, limit?): read file content; offset is the 1-indexed line to start from, limit is the max number of lines to return
- search_text(pattern, glob?, output_mode?): grep-like text search; output_mode is "content" (default, file:line:text), "files_with_matches", or "count"
- write_file(file_path, content, description): create a new file or fully overwrite an existing one. Use for new files or when replacing the entire content. HITL-gated.
- edit_file(file_path, old_string, new_string, description, replace_all?): replace an exact string in an existing file. old_string must match character-for-character including whitespace. Fails if old_string is not found. Set replace_all=true to replace all occurrences. Prefer this over write_file for targeted edits. HITL-gated.
- delete_file(file_path, description): delete an existing file. HITL-gated.
- manage_directory(operation, path, description, new_path?): create, delete, or rename a directory and ask the user for confirmation. operation must be "create", "delete", or "rename". new_path is required for "rename". delete removes the directory and all its contents recursively.
- validate_project(file_path?): validate all project files (or a single file) against the Oxy schema; returns any errors
- lookup_schema(object_name): look up the JSON schema for any Oxy object type — semantic (Dimension, Measure, View, Topic…), agent (AgentConfig, AgentType, ToolType…), FSM workflow (AgenticConfig), workflow tasks (Workflow, Task, ExecuteSQLTask, AgentTask…), app (AppConfig, Display…), test (TestFileConfig, TestSettings, TestCase), or config (Config, Database, DatabaseType)
- run_tests(file_path?): run a specific .test.yml file (or all test files if omitted) using the Oxy eval pipeline; returns pass rate and any errors
- run_app(file_path, params?): execute a .app.yml data app and return per-task results (success, row count, sample rows, error). Always runs fresh — bypasses the result cache. Use after editing an app file to verify all tasks execute without error.
- execute_sql(sql, database?): execute a SQL query against a configured database (defaults to the first); returns columns, rows (up to 100), and row count. Use to verify SQL before proposing file changes.
- semantic_query(topic, dimensions?, measures?, filters?, limit?): compile and run a semantic layer query; validates against .view.yml/.topic.yml, returns generated SQL and results. Use to verify semantic definitions before proposing changes to .view.yml or .topic.yml files.
- ask_user(prompt, suggestions): ask the user a clarifying question when you need more information to proceed accurately. Always provide 2–4 concrete suggestions.

## Data transformation / modeling tools (airform / dbt)

Oxy supports dbt-style data transformation projects under `modeling/`. Each project
has a `dbt_project.yml`, SQL model files, and a `oxy.yml` file that maps dbt profile
outputs to Oxy database names. These tools let you inspect, compile, run, and test models.

IMPORTANT: All dbt/airform operations are handled entirely by Oxy through the tools below.
Never tell the user to run `dbt` CLI commands (e.g. `dbt run`, `dbt test`, `dbt compile`,
`dbt seed`, `dbt docs generate`) or install dbt. Use the built-in tools instead.

Example of `oxy.yml`:
```yaml
mappings:
  # mapping dbt target `dev` to oxy database `local`
  dev: local
```

IMPORTANT: Responsibility split across the three config files:
- `oxy.yml` mappings handle **connection routing only** — they map a dbt target name to an
  Oxy database name defined in `config.yml` (e.g. `dev: local`).
- `config.yml` owns **all connection credentials and adapter settings** (host, user, password,
  warehouse, key file, etc.). This is the authoritative source for database connections.
- `profiles.yml` owns **output schema configuration only**: the active `target:` name and the
  `schema:` (or `dataset:` for BigQuery) field that controls where model output is written.
  It does NOT contain credentials — those come from `config.yml` via the `oxy.yml` mapping.

When a user asks to configure a dbt project connection, edit the credentials in `config.yml`
and add the target→database mapping in `oxy.yml`. Only edit `profiles.yml` to change the
output schema or default target.

IMPORTANT: The dbt target type in `profiles.yml` (the `type:` field) must match the Oxy
database type in `config.yml`. Mismatched types cause a `DatabaseTypeMismatch` error at
run/test time. The valid pairings are:

| dbt target `type:` | Required Oxy database type |
|--------------------|---------------------------|
| snowflake          | snowflake                 |
| bigquery           | bigquery                  |
| duckdb             | duckdb or motherduck      |
| postgres           | postgres                  |
| redshift           | redshift                  |
| mysql              | mysql                     |
| clickhouse         | clickhouse                |

When creating or editing `oxy.yml`, always check both `profiles.yml` (for the dbt target
type) and `config.yml` (for the Oxy database type) to confirm they are compatible before
writing the mapping.

- list_dbt_projects(): list all transformation projects in this workspace
- list_dbt_nodes(project): list all models, seeds, tests, and sources with their SQL and column definitions
- compile_dbt_model(project, model?): compile one model (or all) to final SQL, resolving `{{ ref() }}` and `{{ source() }}` macros
- run_dbt_models(project, selector?): execute models and write Parquet outputs to the configured output directory
- test_dbt_models(project, selector?): run dbt data-quality tests (not_null, unique, accepted_values, etc.)
- get_dbt_lineage(project): return the directed dependency graph as nodes + edges

## DBMS compatibility

SQL dialects differ significantly. Always check `profiles.yml` for the active `type:` before writing model SQL.

**Schema / table referencing**
- DuckDB: omit the schema prefix — write `FROM my_table`, never `FROM public.my_table`. DuckDB's in-memory context registers tables without a schema qualifier.
- PostgreSQL: use explicit `schema.table` references (e.g. `FROM public.orders`) when the search path is not guaranteed.
- BigQuery: use three-part names — `project.dataset.table` — or two-part `dataset.table` when a default project is set.
- Snowflake: use `database.schema.table`; fall back to `schema.table` when the database is set in the connection.

**Function portability — known gaps and substitutes**

| Function | DuckDB | BigQuery | PostgreSQL | Snowflake |
|----------|--------|----------|------------|-----------|
| `INITCAP` | NOT available — use `regexp_replace(lower(col), '(^|[^a-zA-Z])([a-z])', '\1' || upper('\2'))` or write a macro | `INITCAP(col)` | `INITCAP(col)` | `INITCAP(col)` |
| `STRING_AGG` | `string_agg(col, sep)` | `STRING_AGG(col, sep)` | `string_agg(col, sep)` | `LISTAGG(col, sep)` |
| `DATE_TRUNC` | `date_trunc('month', col)` | `DATE_TRUNC(col, MONTH)` — note reversed arg order | `date_trunc('month', col)` | `DATE_TRUNC('month', col)` |
| `EPOCH` extraction | `epoch(col)` or `extract(epoch FROM col)` | `UNIX_SECONDS(col)` | `extract(epoch FROM col)` | `DATE_PART('epoch', col)` |
| `GENERATE_SERIES` | `generate_series(start, stop, step)` | `GENERATE_ARRAY(start, stop)` | `generate_series(start, stop, step)` | not built-in |
| `TYPEOF` / type inspection | `typeof(col)` | n/a | `pg_typeof(col)` | `TYPEOF(col)` |

**Type casting in DuckDB**
DuckDB requires explicit casts inside `CASE`/`WHEN` branches and string operations when operand types differ.
- Always cast to `VARCHAR` before concatenation: `CAST(col AS VARCHAR) || ' suffix'`
- Inside a `CASE` expression, all `THEN` branches must return the same type; use `CAST(... AS VARCHAR)` on every branch when mixing types.
- Use `TRY_CAST(col AS INTEGER)` to return `NULL` on conversion failure instead of raising an error.

## Troubleshooting common errors

**`function does not exist` / `Unknown function`**
1. Identify the dialect from `profiles.yml type:`.
2. Check the portability table above for a substitute.
3. If no built-in substitute exists, write a dbt macro in `macros/` (e.g. `macros/initcap.sql`) and call it with `{{ initcap(col) }}`.
4. After fixing, recompile with `compile_dbt_model` to confirm the error is gone before running.

**`explicit type cast required` / type mismatch**
1. Wrap the offending column: `CAST(col AS VARCHAR)` for string ops, or the target numeric type for arithmetic.
2. In DuckDB `CASE`/`WHEN`, cast every `THEN` branch to a common type.
3. For date arithmetic, use `col::DATE` (DuckDB/Postgres) or `CAST(col AS DATE)` (portable) rather than relying on implicit coercion.

**`relation does not exist` / `Table not found`**
1. Check that the seed CSV is present in `seeds/` and run `seed_dbt_project` before building models that reference it.
2. Verify `{{ ref('model_name') }}` spelling matches the `.sql` filename exactly (case-sensitive on some platforms).
3. Run `parse_dbt_project` to confirm all DAG nodes resolved without errors.

## File naming and directory conventions

Follow this layout strictly so automation and scripts can rely on predictable paths:

```
modeling/<project>/
  seeds/           # raw CSV inputs — filename becomes the seed name, e.g. seeds/raw_books.csv → ref('raw_books')
  models/
    staging/       # one-to-one cleaning models, prefix: stg_  (e.g. stg_books.sql)
    marts/         # aggregated/business models, prefix: fct_ or dim_  (e.g. fct_orders.sql)
  macros/          # reusable Jinja macros
  tests/           # custom data tests
  schema.yml       # column descriptions and dbt tests
  dbt_project.yml
  profiles.yml
  oxy.yml
```

- Name seeds after the raw source: `raw_<entity>.csv` → `raw_<entity>` seed → `stg_<entity>.sql` staging model.
- All raw CSVs that models depend on must exist as seeds in `seeds/`. Never reference a CSV path directly in model SQL.
- Use lowercase snake_case for all file and model names.

## Reusable cleaning model template

When creating a `stg_<entity>.sql` model, start from this pattern and adapt to the actual columns:

```sql
WITH source AS (
    SELECT * FROM {{ ref('raw_<entity>') }}
),
cleaned AS (
    SELECT
        -- deduplicate on natural key
        ROW_NUMBER() OVER (PARTITION BY id ORDER BY updated_at DESC) AS row_num,

        -- normalize dates: coerce empty string / NULL to NULL, then cast
        CASE
            WHEN TRIM(CAST(event_date AS VARCHAR)) = '' THEN NULL
            ELSE TRY_CAST(event_date AS DATE)
        END AS event_date,

        -- coalesce NULLs to safe defaults
        COALESCE(CAST(quantity AS INTEGER), 0) AS quantity,
        COALESCE(TRIM(CAST(name AS VARCHAR)), 'unknown') AS name
    FROM source
)
SELECT * EXCLUDE (row_num) FROM cleaned WHERE row_num = 1
```

Replace `id`, `updated_at`, and column names to match the actual seed schema. Remove sections that don't apply.

## No manual data change policy

Never suggest editing raw CSV files, running UPDATE statements against source tables, or modifying data outside a dbt model to work around a data quality issue. Always:
1. Fix the issue in a staging model (type cast, COALESCE, dedup, filter).
2. If the raw CSV itself has structural errors (wrong delimiter, encoding), ask the user to fix the file and re-seed — do not suggest rewriting data rows manually.

## Model dependency order

Before building models, verify the dependency chain is intact:
1. Run `parse_dbt_project` — confirm all nodes resolve and there are no DAG errors.
2. Run `seed_dbt_project` — load all seed CSVs before running any model that uses `{{ ref('raw_...') }}`.
3. Run `run_dbt_models` with a selector starting from the most upstream model first (or omit selector to run all in dependency order).
4. Run `test_dbt_models` after a successful run to catch data-quality regressions.

When a user asks to build a specific model, always check its lineage with `get_dbt_lineage` first and ensure upstream seeds and models are present before running.

## SQL testing after transformation

After any model change:
1. Use `compile_dbt_model` to inspect the final SQL before running.
2. Use `run_dbt_models` then `test_dbt_models` to execute dbt schema tests (`not_null`, `unique`, `accepted_values`).
3. Use `execute_sql` for ad-hoc spot checks, e.g.:
   - NULL audit: `SELECT COUNT(*) FROM <model> WHERE <key_col> IS NULL`
   - Duplicate check: `SELECT <key>, COUNT(*) FROM <model> GROUP BY 1 HAVING COUNT(*) > 1`
   - Range check: `SELECT MIN(amount), MAX(amount), AVG(amount) FROM <model>`
4. Report any rows that fail these checks to the user before declaring the model clean.

## Schema evolution guidance

When raw CSV columns change or cleaning logic is updated:
1. Run `analyze_dbt_project` to detect contract violations — columns declared in `schema.yml` that are missing or type-mismatched in the actual output.
2. Update `schema.yml` column definitions to match the new output schema.
3. Check downstream models with `get_dbt_lineage` — identify all models that `{{ ref() }}` the changed model and review whether their column references are still valid.
4. Run the full test suite with `test_dbt_models` after any schema change to surface broken assumptions early.

## Guidelines

- Restrict emoji usages.
- Always read config.yml first to understand available databases and models before making changes
- Read the relevant files before proposing any changes — never guess at existing content
- Always use write_file, edit_file, or delete_file before writing, modifying, or deleting any file — never assume permission
- Prefer edit_file for targeted changes; use write_file only when creating a new file or replacing the entire content
- Use delete_file to delete a file
- Use file paths relative to the project root in all tool calls and responses
- When proposing changes, explain what you are changing and why
- After a change is accepted, run validate_project on the modified file to confirm it is schema-valid
- Use execute_sql to test SQL queries before embedding them in workflow or agent files
- Use semantic_query to verify semantic layer definitions (views, topics, dimensions, measures) before proposing changes to .view.yml or .topic.yml files
- Test files (.test.yml) must reference a valid target (an .agent.yml or .aw.yml file path relative to the project root)
- Use lookup_schema(TestFileConfig) to see the full test file schema before writing tests
- After writing a test file, use run_tests to execute it and report the results to the user
- After writing or editing a .app.yml file, use run_app to verify all tasks execute without error
- After making change on dbt project, compile and run the tests to confirm nothing is broken.

## CRITICAL INSTRUCTION

After your last tool call, output NOTHING. No summary, no confirmation, no closing message.
A separate step reads your tool results and writes the reply to the user.
Any text you output after the final tool call is wasted tokens and will be discarded."#
        )
    }

    pub(crate) fn build_interpreting_system_prompt(&self) -> String {
        r#"You are the final response synthesizer for the Oxygen builder agent.
You receive the user's original request and the full tool exchange log from the solving phase.
Your job is to write a short, direct reply.
State what was done and call out any notable outcome or follow-up the user must know.
Skip listing every file, field, or test step unless something went wrong.
No emoji. Do not invent results not present in the tool exchange log.
Do not call any tools."#
            .to_string()
    }

    /// Build the day-only date hint that is appended to the system prompt as
    /// a separate, uncached content block.  Kept here so the format stays
    /// in sync between Solving and Interpreting calls.
    pub(crate) fn current_date_hint() -> String {
        chrono::Utc::now().format("Today is %Y-%m-%d.").to_string()
    }

    pub(crate) fn build_initial_messages(
        &self,
        question: &str,
        history: &[ConversationTurn],
    ) -> InitialMessages {
        if history.is_empty() {
            return InitialMessages::User(question.to_string());
        }

        let mut messages: Vec<serde_json::Value> = Vec::new();
        for (turn_idx, turn) in history.iter().enumerate() {
            messages.push(serde_json::json!({ "role": "user", "content": turn.question }));
            for (tool_idx, exchange) in turn.tool_exchanges.iter().enumerate() {
                let tool_use_id = format!("hist_t{turn_idx}_tc{tool_idx}");
                let input_val: serde_json::Value =
                    match serde_json::from_str::<serde_json::Value>(&exchange.input) {
                        Ok(serde_json::Value::Object(obj)) => serde_json::Value::Object(obj),
                        _ => serde_json::json!({}),
                    };
                messages.push(serde_json::json!({
                    "role": "assistant",
                    "content": [{"type": "tool_use", "id": tool_use_id, "name": exchange.name, "input": input_val}]
                }));
                messages.push(serde_json::json!({
                    "role": "user",
                    "content": [{"type": "tool_result", "tool_use_id": tool_use_id, "content": exchange.output, "is_error": false}]
                }));
            }
            messages.push(serde_json::json!({ "role": "assistant", "content": turn.answer }));
        }
        messages.push(serde_json::json!({ "role": "user", "content": question }));
        InitialMessages::Messages(messages)
    }

    pub(crate) fn solving_loop_config() -> ToolLoopConfig {
        ToolLoopConfig {
            max_tool_rounds: 30,
            state: "solving".to_string(),
            thinking: agentic_llm::ThinkingConfig::Disabled,
            response_schema: None,
            max_tokens_override: Some(16384),
            sub_spec_index: None,
            system_date_hint: Some(Self::current_date_hint()),
        }
    }
}

pub(crate) async fn emit_domain(tx: &Option<EventStream<BuilderEvent>>, event: BuilderEvent) {
    if let Some(tx) = tx {
        let _ = tx.send(Event::Domain(event)).await;
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn dispatch_tool(
    name: &str,
    params: &serde_json::Value,
    project_root: &Path,
    event_tx: &Option<EventStream<BuilderEvent>>,
    test_runner: Option<Arc<dyn BuilderTestRunner>>,
    human_input: HumanInputHandle,
    db_provider: Option<&Arc<dyn BuilderDatabaseProvider>>,
    project_validator: Option<&Arc<dyn BuilderProjectValidator>>,
    schema_provider: Option<&Arc<dyn BuilderSchemaProvider>>,
    semantic_compiler: Option<&Arc<dyn BuilderSemanticCompiler>>,
    secrets_provider: Option<&Arc<dyn BuilderSecretsProvider>>,
    app_runner: Option<&Arc<dyn BuilderAppRunner>>,
) -> Result<Box<dyn ToolOutput>, ToolError> {
    match name {
        "search_files" => {
            let r = execute_search_files(project_root, params);
            if let Ok(ref v) = r {
                let count = v["count"].as_u64().unwrap_or(0);
                emit_domain(
                    event_tx,
                    BuilderEvent::ToolUsed {
                        tool_name: "search_files".into(),
                        summary: format!(
                            "Found {count} files matching '{}'",
                            params["pattern"].as_str().unwrap_or("")
                        ),
                    },
                )
                .await;
            }
            r.map(|v| Box::new(v) as Box<dyn ToolOutput>)
        }
        "read_file" => {
            let r = execute_read_file(project_root, params).await;
            if let Ok(ref v) = r {
                let lines = v["total_lines"].as_u64().unwrap_or(0);
                emit_domain(
                    event_tx,
                    BuilderEvent::ToolUsed {
                        tool_name: "read_file".into(),
                        summary: format!(
                            "Read '{}' ({lines} lines)",
                            params["file_path"].as_str().unwrap_or("")
                        ),
                    },
                )
                .await;
            }
            r.map(|v| Box::new(v) as Box<dyn ToolOutput>)
        }
        "search_text" => {
            let r = execute_search_text(project_root, params).await;
            if let Ok(ref v) = r {
                let count = v["count"].as_u64().unwrap_or(0);
                emit_domain(
                    event_tx,
                    BuilderEvent::ToolUsed {
                        tool_name: "search_text".into(),
                        summary: format!(
                            "Found {count} matches for '{}'",
                            params["pattern"].as_str().unwrap_or("")
                        ),
                    },
                )
                .await;
            }
            r.map(|v| Box::new(v) as Box<dyn ToolOutput>)
        }
        "validate_project" => {
            let validator = project_validator.ok_or_else(|| {
                ToolError::Execution("project validator is not configured".into())
            })?;
            let r = execute_validate_project(project_root, params, validator.as_ref()).await;
            if let Ok(ref v) = r {
                let summary = if v["valid"].as_bool().unwrap_or(false) {
                    format!(
                        "All {} file(s) valid",
                        v["valid_count"].as_u64().unwrap_or(0)
                    )
                } else {
                    let n = v["error_count"].as_u64().unwrap_or(0);
                    format!("{n} validation error(s) found")
                };
                emit_domain(
                    event_tx,
                    BuilderEvent::ToolUsed {
                        tool_name: "validate_project".into(),
                        summary,
                    },
                )
                .await;
            }
            r.map(|v| Box::new(v) as Box<dyn ToolOutput>)
        }
        "write_file" => {
            let file_path = params["file_path"].as_str().unwrap_or("").to_string();
            let description = params["description"].as_str().unwrap_or("").to_string();
            let result = execute_write_file(project_root, params, human_input.as_ref()).await;
            if let Err(ToolError::Suspended { ref prompt, .. }) = result {
                let (new_content, old_content) = serde_json::from_str::<serde_json::Value>(prompt)
                    .ok()
                    .map(|v| {
                        (
                            v["new_content"].as_str().unwrap_or("").to_string(),
                            v["old_content"].as_str().unwrap_or("").to_string(),
                        )
                    })
                    .unwrap_or_default();
                emit_domain(
                    event_tx,
                    BuilderEvent::FileChangePending {
                        file_path,
                        description,
                        new_content,
                        old_content,
                    },
                )
                .await;
            }
            result.map(|v| Box::new(v) as Box<dyn ToolOutput>)
        }
        "edit_file" => {
            let file_path = params["file_path"].as_str().unwrap_or("").to_string();
            let description = params["description"].as_str().unwrap_or("").to_string();
            let result = execute_edit_file(project_root, params, human_input.as_ref()).await;
            if let Err(ToolError::Suspended { ref prompt, .. }) = result {
                let (new_content, old_content) = serde_json::from_str::<serde_json::Value>(prompt)
                    .ok()
                    .map(|v| {
                        (
                            v["new_content"].as_str().unwrap_or("").to_string(),
                            v["old_content"].as_str().unwrap_or("").to_string(),
                        )
                    })
                    .unwrap_or_default();
                emit_domain(
                    event_tx,
                    BuilderEvent::FileChangePending {
                        file_path,
                        description,
                        new_content,
                        old_content,
                    },
                )
                .await;
            }
            result.map(|v| Box::new(v) as Box<dyn ToolOutput>)
        }
        "delete_file" => {
            let file_path = params["file_path"].as_str().unwrap_or("").to_string();
            let description = params["description"].as_str().unwrap_or("").to_string();
            let result = execute_delete_file(project_root, params, human_input.as_ref()).await;
            if let Err(ToolError::Suspended { ref prompt, .. }) = result {
                let old_content = serde_json::from_str::<serde_json::Value>(prompt)
                    .ok()
                    .and_then(|v| v["old_content"].as_str().map(String::from))
                    .unwrap_or_default();
                emit_domain(
                    event_tx,
                    BuilderEvent::FileChangePending {
                        file_path,
                        description,
                        new_content: String::new(),
                        old_content,
                    },
                )
                .await;
            }
            result.map(|v| Box::new(v) as Box<dyn ToolOutput>)
        }
        "manage_directory" => {
            let path = params["path"].as_str().unwrap_or("").to_string();
            let operation = params["operation"].as_str().unwrap_or("").to_string();
            let result = execute_manage_directory(project_root, params, human_input.as_ref()).await;
            if result.is_ok() {
                emit_domain(
                    event_tx,
                    BuilderEvent::ToolUsed {
                        tool_name: "manage_directory".into(),
                        summary: format!("{operation} directory '{path}'"),
                    },
                )
                .await;
            }
            result.map(|v| Box::new(v) as Box<dyn ToolOutput>)
        }
        "ask_user" => agentic_core::tools::handle_ask_user(params, human_input.as_ref())
            .map(|v| Box::new(v) as Box<dyn ToolOutput>),
        "lookup_schema" => {
            let provider = schema_provider
                .ok_or_else(|| ToolError::Execution("schema provider is not configured".into()))?;
            let r = execute_lookup_schema(params, provider.as_ref());
            if let Ok(ref v) = r {
                let object_name = v["object_name"].as_str().unwrap_or("");
                emit_domain(
                    event_tx,
                    BuilderEvent::ToolUsed {
                        tool_name: "lookup_schema".into(),
                        summary: format!("Retrieved schema for '{object_name}'"),
                    },
                )
                .await;
            }
            r.map(|v| Box::new(v) as Box<dyn ToolOutput>)
        }
        "run_tests" => match test_runner {
            Some(runner) => {
                let file_label = params["file_path"]
                    .as_str()
                    .unwrap_or("<all tests>")
                    .to_string();
                let r = execute_run_tests(project_root, params, runner).await;
                if let Ok(ref v) = r {
                    let summary = if let Some(n) = v["tests_run"].as_u64() {
                        format!("Ran {n} test file(s)")
                    } else {
                        format!("Ran tests for '{file_label}'")
                    };
                    emit_domain(
                        event_tx,
                        BuilderEvent::ToolUsed {
                            tool_name: "run_tests".into(),
                            summary,
                        },
                    )
                    .await;
                }
                r.map(|v| Box::new(v) as Box<dyn ToolOutput>)
            }
            None => Err(ToolError::Execution(
                "test runner is not configured for this builder instance".into(),
            )),
        },
        "execute_sql" => {
            let provider = db_provider.ok_or_else(|| {
                ToolError::Execution("database provider is not configured".into())
            })?;
            let r = execute_execute_sql(params, provider.as_ref()).await;
            if let Ok(ref v) = r {
                let db = v["database"].as_str().unwrap_or("");
                let rows = v["row_count"].as_u64().unwrap_or(0);
                emit_domain(
                    event_tx,
                    BuilderEvent::ToolUsed {
                        tool_name: "execute_sql".into(),
                        summary: format!("Ran SQL on '{db}' - {rows} row(s)"),
                    },
                )
                .await;
            }
            r.map(|v| Box::new(v) as Box<dyn ToolOutput>)
        }
        "semantic_query" => {
            let provider = db_provider.ok_or_else(|| {
                ToolError::Execution("database provider is not configured".into())
            })?;
            let compiler = semantic_compiler.ok_or_else(|| {
                ToolError::Execution("semantic compiler is not configured".into())
            })?;
            let r = execute_semantic_query(params, provider.as_ref(), compiler.as_ref()).await;
            if let Ok(ref v) = r {
                let topic = params["topic"].as_str().unwrap_or("");
                let rows = v["row_count"].as_u64().unwrap_or(0);
                emit_domain(
                    event_tx,
                    BuilderEvent::ToolUsed {
                        tool_name: "semantic_query".into(),
                        summary: format!("Ran semantic query on topic '{topic}' - {rows} row(s)"),
                    },
                )
                .await;
            }
            r.map(|v| Box::new(v) as Box<dyn ToolOutput>)
        }
        "list_dbt_projects" => {
            let r = execute_list_dbt_projects(project_root, params);
            if let Ok(ref v) = r {
                emit_domain(
                    event_tx,
                    BuilderEvent::ToolUsed {
                        tool_name: "list_dbt_projects".into(),
                        summary: format!("Found {} dbt project(s)", v.projects.len()),
                    },
                )
                .await;
            }
            r.map(|v| Box::new(v) as Box<dyn ToolOutput>)
        }
        "list_dbt_nodes" => {
            let r = execute_list_dbt_nodes(project_root, params);
            if let Ok(ref v) = r {
                emit_domain(
                    event_tx,
                    BuilderEvent::ToolUsed {
                        tool_name: "list_dbt_nodes".into(),
                        summary: format!("Listed {} node(s) in '{}'", v.nodes.len(), v.project),
                    },
                )
                .await;
            }
            r.map(|v| Box::new(v) as Box<dyn ToolOutput>)
        }
        "compile_dbt_model" => {
            let project_name = params["project"].as_str().unwrap_or("");
            if let Some(model) = params["model"].as_str().filter(|s| !s.is_empty()) {
                let r = execute_compile_dbt_model_single(project_root, project_name, model);
                if r.is_ok() {
                    emit_domain(
                        event_tx,
                        BuilderEvent::ToolUsed {
                            tool_name: "compile_dbt_model".into(),
                            summary: format!("Compiled model '{model}' in '{project_name}'"),
                        },
                    )
                    .await;
                }
                r.map(|v| Box::new(v) as Box<dyn ToolOutput>)
            } else {
                let r = execute_compile_dbt_model_all(project_root, project_name);
                if let Ok(ref v) = r {
                    emit_domain(
                        event_tx,
                        BuilderEvent::ToolUsed {
                            tool_name: "compile_dbt_model".into(),
                            summary: format!(
                                "Compiled {} model(s) in '{project_name}'",
                                v.models_compiled
                            ),
                        },
                    )
                    .await;
                }
                r.map(|v| Box::new(v) as Box<dyn ToolOutput>)
            }
        }
        "run_dbt_models" => {
            let sm = secrets_provider.map(|p| p.secrets_manager().clone());
            let r = execute_run_dbt_models(project_root, params, sm.as_ref()).await;
            if let Ok(ref v) = r {
                emit_domain(
                    event_tx,
                    BuilderEvent::ToolUsed {
                        tool_name: "run_dbt_models".into(),
                        summary: format!(
                            "Ran {} model(s) in '{}' — {}",
                            v.results.len(),
                            v.project,
                            v.status
                        ),
                    },
                )
                .await;
            }
            r.map(|v| Box::new(v) as Box<dyn ToolOutput>)
        }
        "test_dbt_models" => {
            let sm = secrets_provider.map(|p| p.secrets_manager().clone());
            let r = execute_test_dbt_models(project_root, params, sm.as_ref()).await;
            if let Ok(ref v) = r {
                emit_domain(
                    event_tx,
                    BuilderEvent::ToolUsed {
                        tool_name: "test_dbt_models".into(),
                        summary: format!(
                            "Tests for '{}': {} passed, {} failed",
                            v.project, v.passed, v.failed
                        ),
                    },
                )
                .await;
            }
            r.map(|v| Box::new(v) as Box<dyn ToolOutput>)
        }
        "get_dbt_lineage" => {
            let r = execute_get_dbt_lineage(project_root, params);
            if let Ok(ref v) = r {
                emit_domain(
                    event_tx,
                    BuilderEvent::ToolUsed {
                        tool_name: "get_dbt_lineage".into(),
                        summary: format!(
                            "Lineage for '{}': {} nodes, {} edges",
                            v.project,
                            v.nodes.len(),
                            v.edges.len()
                        ),
                    },
                )
                .await;
            }
            r.map(|v| Box::new(v) as Box<dyn ToolOutput>)
        }
        "analyze_dbt_project" => {
            let r = execute_analyze_dbt_project(project_root, params).await;
            if let Ok(ref v) = r {
                emit_domain(
                    event_tx,
                    BuilderEvent::ToolUsed {
                        tool_name: "analyze_dbt_project".into(),
                        summary: format!(
                            "Analyzed {} model(s) in '{}' — {} contract violation(s)",
                            v.models_analyzed,
                            v.project,
                            v.contract_violations.len()
                        ),
                    },
                )
                .await;
            }
            r.map(|v| Box::new(v) as Box<dyn ToolOutput>)
        }
        "get_dbt_column_lineage" => {
            let r = execute_get_dbt_column_lineage(project_root, params);
            if let Ok(ref v) = r {
                emit_domain(
                    event_tx,
                    BuilderEvent::ToolUsed {
                        tool_name: "get_dbt_column_lineage".into(),
                        summary: format!(
                            "Column lineage for '{}': {} edge(s)",
                            v.project,
                            v.edges.len()
                        ),
                    },
                )
                .await;
            }
            r.map(|v| Box::new(v) as Box<dyn ToolOutput>)
        }
        "parse_dbt_project" => {
            let r = execute_parse_dbt_project(project_root, params);
            if let Ok(ref v) = r {
                emit_domain(
                    event_tx,
                    BuilderEvent::ToolUsed {
                        tool_name: "parse_dbt_project".into(),
                        summary: format!(
                            "Parsed '{}': {} model(s), {} source(s)",
                            v.project, v.models, v.sources
                        ),
                    },
                )
                .await;
            }
            r.map(|v| Box::new(v) as Box<dyn ToolOutput>)
        }
        "seed_dbt_project" => {
            let sm = secrets_provider.map(|p| p.secrets_manager().clone());
            let r = execute_seed_dbt_project(project_root, params, sm.as_ref()).await;
            if let Ok(ref v) = r {
                emit_domain(
                    event_tx,
                    BuilderEvent::ToolUsed {
                        tool_name: "seed_dbt_project".into(),
                        summary: format!("Loaded {} seed(s) in '{}'", v.seeds_loaded, v.project),
                    },
                )
                .await;
            }
            r.map(|v| Box::new(v) as Box<dyn ToolOutput>)
        }
        "debug_dbt_project" => {
            let r = execute_debug_dbt_project(project_root, params);
            if let Ok(ref v) = r {
                let status = if v.all_ok {
                    "all checks passed"
                } else {
                    "issues found"
                };
                emit_domain(
                    event_tx,
                    BuilderEvent::ToolUsed {
                        tool_name: "debug_dbt_project".into(),
                        summary: format!("Debug '{}': {status}", v.project_name),
                    },
                )
                .await;
            }
            r.map(|v| Box::new(v) as Box<dyn ToolOutput>)
        }
        "clean_dbt_project" => {
            let r = execute_clean_dbt_project(project_root, params);
            if let Ok(ref v) = r {
                emit_domain(
                    event_tx,
                    BuilderEvent::ToolUsed {
                        tool_name: "clean_dbt_project".into(),
                        summary: format!(
                            "Cleaned {} director(y/ies) in '{}'",
                            v.cleaned.len(),
                            v.project
                        ),
                    },
                )
                .await;
            }
            r.map(|v| Box::new(v) as Box<dyn ToolOutput>)
        }
        "docs_generate_dbt" => {
            let r = execute_docs_generate_dbt(project_root, params);
            if let Ok(ref v) = r {
                emit_domain(
                    event_tx,
                    BuilderEvent::ToolUsed {
                        tool_name: "docs_generate_dbt".into(),
                        summary: format!("Generated docs for '{}': {} node(s)", v.project, v.nodes),
                    },
                )
                .await;
            }
            r.map(|v| Box::new(v) as Box<dyn ToolOutput>)
        }
        "format_dbt_sql" => {
            let r = execute_format_dbt_sql(project_root, params);
            if let Ok(ref v) = r {
                emit_domain(
                    event_tx,
                    BuilderEvent::ToolUsed {
                        tool_name: "format_dbt_sql".into(),
                        summary: format!(
                            "Formatted '{}': {}/{} file(s) changed",
                            v.project, v.files_changed, v.files_checked
                        ),
                    },
                )
                .await;
            }
            r.map(|v| Box::new(v) as Box<dyn ToolOutput>)
        }
        "init_dbt_project" => {
            let name = params["name"].as_str().unwrap_or("").to_string();

            let prompt = serde_json::json!({
                "type": "init_dbt_project",
                "project_name": name,
                "description": format!("Initialize new dbt project '{name}'")
            })
            .to_string();
            let suggestions = vec!["Accept".to_string(), "Reject".to_string()];

            match human_input.as_ref().request_sync(&prompt, &suggestions) {
                Ok(_) => {
                    let r = execute_init_dbt_project(project_root, params);
                    if let Ok(ref val) = r {
                        for (file_path, new_content, description) in &val.files {
                            emit_domain(
                                event_tx,
                                BuilderEvent::FileChanged {
                                    file_path: file_path.clone(),
                                    description: description.clone(),
                                    new_content: new_content.clone(),
                                    old_content: String::new(),
                                    is_deletion: false,
                                },
                            )
                            .await;
                        }
                        emit_domain(
                            event_tx,
                            BuilderEvent::ToolUsed {
                                tool_name: "init_dbt_project".into(),
                                summary: format!("Initialized new project '{name}'"),
                            },
                        )
                        .await;
                    }
                    r.map(|v| Box::new(v) as Box<dyn ToolOutput>)
                }
                Err(()) => Err(ToolError::Suspended {
                    prompt,
                    suggestions,
                }),
            }
        }
        "run_app" => match app_runner {
            Some(runner) => {
                let file_path = params["file_path"].as_str().unwrap_or("").to_string();
                let r = execute_run_app(project_root, params, runner.clone()).await;
                if let Ok(ref v) = r {
                    let tasks_run = v["tasks_run"].as_u64().unwrap_or(0);
                    let succeeded = v["tasks_succeeded"].as_u64().unwrap_or(0);
                    let failed = v["tasks_failed"].as_u64().unwrap_or(0);
                    emit_domain(
                        event_tx,
                        BuilderEvent::ToolUsed {
                            tool_name: "run_app".into(),
                            summary: format!(
                                "Ran app '{file_path}' — {tasks_run} task(s), {succeeded} succeeded, {failed} failed"
                            ),
                        },
                    )
                    .await;
                }
                r.map(|v| Box::new(v) as Box<dyn ToolOutput>)
            }
            None => Err(ToolError::Execution(
                "app runner is not configured for this builder instance".into(),
            )),
        },
        other => Err(ToolError::UnknownTool(other.to_string())),
    }
}

/// Maximum number of tool exchanges to retain in memory per run.
/// Older entries are dropped to bound memory when tools return large payloads
/// (e.g. full file contents). 20 is well above the typical useful window while
/// staying well below max_tool_rounds (30) × worst-case payload size.
const MAX_TOOL_EXCHANGES: usize = 20;

pub(crate) fn record_tool_exchange(
    exchanges: &mut Vec<ToolExchange>,
    name: &str,
    params: &serde_json::Value,
    result: &Result<Box<dyn ToolOutput>, ToolError>,
) {
    if matches!(result, Err(ToolError::Suspended { .. })) {
        return;
    }

    let output = match result {
        Ok(value) => value.to_value().to_string(),
        Err(err) => err.to_string(),
    };
    exchanges.push(ToolExchange {
        name: name.to_string(),
        input: params.to_string(),
        output,
    });
    if exchanges.len() > MAX_TOOL_EXCHANGES {
        exchanges.drain(..exchanges.len() - MAX_TOOL_EXCHANGES);
    }
}

pub(crate) fn make_resume_stage_data(
    spec: &BuilderSpec,
    prior_messages: &[serde_json::Value],
    suspension_type: &str,
    question: &str,
    suggestions: &[String],
    tool_exchanges: &[ToolExchange],
) -> serde_json::Value {
    serde_json::json!({
        "spec": spec,
        "prior_messages": prior_messages,
        "suspension_type": suspension_type,
        "question": question,
        "suggestions": suggestions,
        "tool_exchanges": tool_exchanges,
    })
}
