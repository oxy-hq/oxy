use std::path::{Path, PathBuf};
use std::sync::Arc;

use agentic_core::{
    events::{Event, EventStream},
    human_input::{DeferredInputProvider, HumanInputHandle, ResumeInput, SuspendedRunData},
    tools::ToolError,
};
use agentic_llm::{InitialMessages, LlmClient, ToolLoopConfig};
use oxy::adapters::secrets::SecretsManager;

use crate::{
    events::BuilderEvent,
    test_runner::BuilderTestRunner,
    tools::{
        execute_execute_sql, execute_lookup_schema, execute_propose_change, execute_read_file,
        execute_run_tests, execute_search_files, execute_search_text, execute_semantic_query,
        execute_validate_project,
    },
    types::{BuilderSpec, ConversationTurn, ToolExchange},
};

pub struct BuilderSolver {
    pub(crate) client: LlmClient,
    pub(crate) project_root: PathBuf,
    pub(crate) event_tx: Option<EventStream<BuilderEvent>>,
    pub(crate) test_runner: Option<Arc<dyn BuilderTestRunner>>,
    pub(crate) human_input: HumanInputHandle,
    pub(crate) suspension_data: Option<SuspendedRunData>,
    pub(crate) resume_data: Option<ResumeInput>,
    pub(crate) secrets_manager: Option<SecretsManager>,
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
            secrets_manager: None,
        }
    }

    pub fn with_secrets_manager(mut self, secrets_manager: SecretsManager) -> Self {
        self.secrets_manager = Some(secrets_manager);
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

    pub(crate) fn build_solving_system_prompt(&self) -> String {
        let root = self.project_root.to_string_lossy();
        format!(
            r#"You are a copilot for an Oxy data project located at: {root}

Oxy is a data platform. A project is a directory of YAML configuration files that define
agents, workflows, semantic models, and data apps. You help users read, understand, and
modify these files.

## Project file types

- config.yml — Main config: database connections, LLM models, default settings, integrations (Slack, MCP, A2A)
- <name>.agent.yml — LLM agent: model, system_instructions, tools (execute_sql, visualize, retrieval, workflow, semantic_search), context files, tests.
- <name>.procedure.yml / <name>.workflow.yml / <name>.automation.yml — Multi-step workflow: variables (JSON Schema), tasks (execute_sql, agent, formatter, loop_sequential…), tests.
- <name>.aw.yml — FSM-based agentic workflow: model, start/end states, transitions (query, semantic_query, visualize, insight, save_automation), optional routing.
- <name>.app.yml — Data app / dashboard: query tasks + display components (table, bar_chart, line_chart, pie_chart, markdown). Lives in apps/.
- <name>.topic.yml — Semantic topic: groups related views into a domain. Lives in semantics/.
- <name>.view.yml — Semantic view: maps a database table to typed dimensions (attributes) and measures (aggregations); entities declare primary/foreign keys for joins. Lives in semantics/.
- globals/semantics.yml — Shared global semantic definitions that views can inherit from.
- *.sql — SQL query files referenced by agents or workflows.
- <name>.test.yml — Test suite for an agent or agentic workflow: target file, settings (runs, judge_model), and test cases (prompt, expected, tags).

## Available tools

- search_files(pattern): find files by glob pattern (e.g. "agents/*.agent.yml", "**/*.view.yml", "**/*.test.yml")
- read_file(path, start_line?, end_line?): read file content with optional line range
- search_text(pattern, file_glob?): grep-like text search across files
- propose_change(file_path, description, new_content?, delete?): propose a file change or deletion and ask the user for confirmation. Set delete=true to delete a file; omit new_content when deleting.
- validate_project(file_path?): validate all project files (or a single file) against the Oxy schema; returns any errors
- lookup_schema(object_name): look up the JSON schema for any Oxy object type — semantic (Dimension, Measure, View, Topic…), agent (AgentConfig, AgentType, ToolType…), FSM workflow (AgenticConfig), workflow tasks (Workflow, Task, ExecuteSQLTask, AgentTask…), app (AppConfig, Display…), test (TestFileConfig, TestSettings, TestCase), or config (Config, Database, DatabaseType)
- run_tests(file_path?): run a specific .test.yml file (or all test files if omitted) using the Oxy eval pipeline; returns pass rate and any errors
- execute_sql(sql, database?): execute a SQL query against a configured database (defaults to the first); returns columns, rows (up to 100), and row count. Use to verify SQL before proposing file changes.
- semantic_query(topic, dimensions?, measures?, filters?, limit?): compile and run a semantic layer query; validates against .view.yml/.topic.yml, returns generated SQL and results. Use to verify semantic definitions before proposing changes to .view.yml or .topic.yml files.
- ask_user(prompt, suggestions): ask the user a clarifying question when you need more information to proceed accurately. Always provide 2–4 concrete suggestions.

## Guidelines

- Restrict emoji usages.
- Always read config.yml first to understand available databases and models before making changes
- Read the relevant files before proposing any changes — never guess at existing content
- Always use propose_change before writing, modifying, or deleting any file — never assume permission
- Write the complete new file content when proposing a change (propose_change replaces the whole file)
- Use propose_change with delete=true to delete a file
- Use file paths relative to the project root in all tool calls and responses
- When proposing changes, explain what you are changing and why
- After a change is accepted, run validate_project on the modified file to confirm it is schema-valid
- Use execute_sql to test SQL queries before embedding them in workflow or agent files
- Use semantic_query to verify semantic layer definitions (views, topics, dimensions, measures) before proposing changes to .view.yml or .topic.yml files
- Test files (.test.yml) must reference a valid target (an .agent.yml or .aw.yml file path relative to the project root)
- Use lookup_schema(TestFileConfig) to see the full test file schema before writing tests
- After writing a test file, use run_tests to execute it and report the results to the user

## Output format

When you have finished using tools, output a structured internal work summary — NOT a user-facing message.
This summary is consumed by a separate synthesis step that produces the final reply.

Use this format:

FINDINGS:
<what you discovered about the project — files read, schemas inspected, SQL/semantic results>

CHANGES:
<for each propose_change call: file path, whether the user accepted or rejected it, and what changed>

VALIDATION:
<results of any validate_project or run_tests calls>

OPEN_ISSUES:
<any errors, schema violations, test failures, or unresolved questions>"#
        )
    }

    pub(crate) fn build_interpreting_system_prompt(&self) -> &'static str {
        r#"You are the final response synthesizer for the Oxy builder agent.
You receive a structured internal work summary (FINDINGS / CHANGES / VALIDATION / OPEN_ISSUES)
produced by the solving phase, plus the raw tool exchange log.
Your job is to turn this into a concise, friendly reply to the user.
Cover: what was found or done, which file changes were accepted or rejected,
validation and test outcomes, and any follow-up the user should be aware of.
Be specific — reference file names and key results.
Restrict emoji usage.
Do not invent tool results or file changes not present in the summary."#
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
            max_tokens_override: None,
            sub_spec_index: None,
        }
    }
}

pub(crate) async fn emit_domain(tx: &Option<EventStream<BuilderEvent>>, event: BuilderEvent) {
    if let Some(tx) = tx {
        let _ = tx.send(Event::Domain(event)).await;
    }
}

pub(crate) async fn dispatch_tool(
    name: &str,
    params: &serde_json::Value,
    project_root: &Path,
    event_tx: &Option<EventStream<BuilderEvent>>,
    test_runner: Option<Arc<dyn BuilderTestRunner>>,
    human_input: HumanInputHandle,
    secrets_manager: Option<&SecretsManager>,
) -> Result<serde_json::Value, ToolError> {
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
            r
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
                            params["path"].as_str().unwrap_or("")
                        ),
                    },
                )
                .await;
            }
            r
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
            r
        }
        "validate_project" => {
            let r = execute_validate_project(project_root, params).await;
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
            r
        }
        "propose_change" => {
            let file_path = params["file_path"].as_str().unwrap_or("").to_string();
            let description = params["description"].as_str().unwrap_or("").to_string();
            let new_content = params["new_content"].as_str().unwrap_or("").to_string();
            emit_domain(
                event_tx,
                BuilderEvent::ProposedChange {
                    file_path,
                    description,
                    new_content,
                },
            )
            .await;
            execute_propose_change(project_root, params, human_input.as_ref()).await
        }
        "ask_user" => agentic_core::tools::handle_ask_user(params, human_input.as_ref()),
        "lookup_schema" => {
            let r = execute_lookup_schema(params);
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
            r
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
                r
            }
            None => Err(ToolError::Execution(
                "test runner is not configured for this builder instance".into(),
            )),
        },
        "execute_sql" => {
            let r = execute_execute_sql(project_root, params, secrets_manager).await;
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
            r
        }
        "semantic_query" => {
            let r = execute_semantic_query(project_root, params, secrets_manager).await;
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
            r
        }
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
    result: &Result<serde_json::Value, ToolError>,
) {
    if matches!(result, Err(ToolError::Suspended { .. })) {
        return;
    }

    let output = match result {
        Ok(value) => value.to_string(),
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
