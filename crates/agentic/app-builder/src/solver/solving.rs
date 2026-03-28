//! **Solving** pipeline stage for the app builder domain.
//!
//! Generates parameterized SQL for a single task via an LLM call.
//! Uses `execute_preview` as a validation tool.

use std::sync::Arc;

use agentic_core::{
    back_target::BackTarget,
    human_input::SuspendedRunData,
    orchestrator::{RunContext, SessionMemory, StateHandler, TransitionResult},
    state::ProblemState,
    HumanInputQuestion,
};
use agentic_llm::{InitialMessages, LlmError, ThinkingConfig, ToolLoopConfig};

use agentic_core::solver::DomainSolver;

use crate::events::AppBuilderEvent;
use crate::schemas::solve_response_schema;
use crate::tools::execute_solving_tool;
use crate::types::{
    AppBuilderDomain, AppBuilderError, AppSolution, AppSpec, ResolvedTask, ResultShape,
};

use super::{
    interpreting::infer_result_shape,
    prompts::{format_retry_section_str, SOLVING_SYSTEM_PROMPT},
    solver::AppBuilderSolver,
};

// ---------------------------------------------------------------------------
// Prompt builder
// ---------------------------------------------------------------------------

pub(crate) fn build_solve_user_prompt(
    spec: &AppSpec,
    task_name: &str,
    retry_error: Option<&str>,
    schema_summary: &str,
) -> String {
    let task = spec.tasks.iter().find(|t| t.name == task_name);
    let (description, is_control_source, control_deps) = task
        .map(|t| {
            (
                t.description.as_str(),
                t.is_control_source,
                t.control_deps.join(", "),
            )
        })
        .unwrap_or_default();

    let controls_section = if spec.controls.is_empty() {
        "(none)".to_string()
    } else {
        spec.controls
            .iter()
            .map(|c| {
                let source_info = if let Some(ref src) = c.source_task {
                    format!(", source: {src}")
                } else if !c.options.is_empty() {
                    format!(", options: [{}]", c.options.join(", "))
                } else {
                    String::new()
                };
                format!(
                    "  - name: {} (type: {}, default: {}{})",
                    c.name, c.control_type, c.default, source_info
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    };

    let control_ref_note = if is_control_source {
        "NOTE: This is a control-source task — do NOT use any {{ controls.X | sqlquote }} references."
    } else {
        "Use {{ controls.X | sqlquote }} Jinja syntax to reference controls in WHERE clauses."
    };

    let retry_section = retry_error
        .map(format_retry_section_str)
        .unwrap_or_default();

    format!(
        "Task: {task_name}\n\
         Description: {description}\n\
         Is control source: {is_control_source}\n\
         Control dependencies: {control_deps}\n\n\
         Available controls:\n{controls_section}\n\n\
         App: {app_name}\n\n\
         Available tables:\n{schema_summary}\n\n\
         {control_ref_note}\n\n\
         Validate your SQL with execute_preview before returning.{retry_section}",
        app_name = spec.app_name,
    )
}

// ---------------------------------------------------------------------------
// solve_impl
// ---------------------------------------------------------------------------

impl AppBuilderSolver {
    /// Generate SQL for a single task in the spec.
    pub(crate) async fn solve_impl(
        &mut self,
        spec: AppSpec,
        retry_error: Option<String>,
    ) -> Result<AppSolution, (AppBuilderError, BackTarget<AppBuilderDomain>)> {
        let connector = self
            .connectors
            .get(&spec.connector_name)
            .or_else(|| self.connectors.get(&self.default_connector))
            .or_else(|| self.connectors.values().next())
            .expect("AppBuilderSolver must have at least one connector")
            .clone();

        let schema_summary = self.catalog.to_table_summary();
        let system_prompt = self.build_system_prompt("solving", SOLVING_SYSTEM_PROMPT);
        let thinking = self.thinking_for_state("solving", ThinkingConfig::Adaptive);

        let mut resume_extra_rounds: u32 = 0;
        let mut resume_max_tokens_override: Option<u32> = None;
        let mut resume_initial: Option<InitialMessages> =
            if let Some(resume) = self.resume_data.take() {
                let prior: Vec<serde_json::Value> = resume.data.stage_data["conversation_history"]
                    .as_array()
                    .cloned()
                    .unwrap_or_default();
                match resume.data.stage_data["suspension_type"].as_str() {
                    Some("max_tokens") => {
                        resume_max_tokens_override = resume.data.stage_data["max_tokens_override"]
                            .as_u64()
                            .map(|v| v as u32);
                    }
                    _ => {
                        resume_extra_rounds =
                            resume.data.stage_data["extra_rounds"].as_u64().unwrap_or(0) as u32;
                    }
                }
                Some(InitialMessages::Messages(
                    agentic_llm::LlmClient::build_continue_messages(&prior),
                ))
            } else {
                None
            };
        let max_rounds = self.max_tool_rounds_for_state("solving", 10) + resume_extra_rounds;

        let tools = crate::tools::solving_tools();
        let mut resolved_tasks: Vec<ResolvedTask> = Vec::new();

        for task in &spec.tasks {
            let user_prompt =
                build_solve_user_prompt(&spec, &task.name, retry_error.as_deref(), &schema_summary);
            let connector_for_tool = Arc::clone(&connector);

            let output = match self
                .client
                .run_with_tools(
                    &system_prompt,
                    resume_initial
                        .take()
                        .unwrap_or(InitialMessages::User(user_prompt)),
                    &tools,
                    move |name: String, params| {
                        let conn = Arc::clone(&connector_for_tool);
                        Box::pin(async move { execute_solving_tool(&name, params, &*conn).await })
                    },
                    &self.event_tx,
                    ToolLoopConfig {
                        max_tool_rounds: max_rounds,
                        state: "solving".into(),
                        thinking: thinking.clone(),
                        response_schema: Some(solve_response_schema()),
                        max_tokens_override: resume_max_tokens_override.or(self.max_tokens),
                        sub_spec_index: None,
                    },
                )
                .await
            {
                Ok(v) => v,
                Err(LlmError::MaxToolRoundsReached {
                    rounds,
                    prior_messages,
                }) => {
                    let prompt = format!(
                        "The agent used all {rounds} allotted tool rounds while solving \
                         task '{}'. Continue with more rounds?",
                        task.name
                    );
                    let spec_value = serde_json::to_value(&spec).unwrap_or_default();
                    self.store_suspension_data(SuspendedRunData {
                        from_state: "solving".to_string(),
                        original_input: spec.intent.raw_request.clone(),
                        trace_id: String::new(),
                        stage_data: serde_json::json!({
                            "spec": spec_value,
                            "conversation_history": prior_messages,
                            "suspension_type": "max_tool_rounds",
                            "extra_rounds": rounds,
                            "completed_tasks": resolved_tasks,
                        }),
                        question: prompt.clone(),
                        suggestions: vec!["Continue".to_string()],
                    });
                    return Err((
                        AppBuilderError::NeedsUserInput {
                            prompt: prompt.clone(),
                        },
                        BackTarget::Suspend {
                            questions: vec![HumanInputQuestion {
                                prompt,
                                suggestions: vec!["Continue".to_string()],
                            }],
                        },
                    ));
                }
                Err(LlmError::MaxTokensReached {
                    current_max_tokens,
                    prior_messages,
                    ..
                }) => {
                    let doubled = current_max_tokens.saturating_mul(2);
                    let prompt = format!(
                        "The model ran out of token budget ({current_max_tokens} tokens) while \
                         solving task '{}'. Continue with double the budget ({doubled} tokens)?",
                        task.name
                    );
                    let spec_value = serde_json::to_value(&spec).unwrap_or_default();
                    self.store_suspension_data(SuspendedRunData {
                        from_state: "solving".to_string(),
                        original_input: spec.intent.raw_request.clone(),
                        trace_id: String::new(),
                        stage_data: serde_json::json!({
                            "spec": spec_value,
                            "conversation_history": prior_messages,
                            "suspension_type": "max_tokens",
                            "max_tokens_override": doubled,
                            "completed_tasks": resolved_tasks,
                        }),
                        question: prompt.clone(),
                        suggestions: vec!["Continue with double budget".to_string()],
                    });
                    return Err((
                        AppBuilderError::NeedsUserInput {
                            prompt: prompt.clone(),
                        },
                        BackTarget::Suspend {
                            questions: vec![HumanInputQuestion {
                                prompt,
                                suggestions: vec!["Continue with double budget".to_string()],
                            }],
                        },
                    ));
                }
                Err(e) => {
                    return Err((
                        AppBuilderError::NeedsUserInput {
                            prompt: format!(
                                "LLM call failed during solving task '{}': {e}",
                                task.name
                            ),
                        },
                        BackTarget::Solve(spec.clone(), Default::default()),
                    ));
                }
            };

            let sql = if let Some(structured) = output.structured_response {
                structured["sql"]
                    .as_str()
                    .unwrap_or_default()
                    .trim()
                    .to_string()
            } else {
                // Strip markdown fences if present.
                let raw = output.text.trim();
                let raw = raw
                    .strip_prefix("```sql")
                    .or_else(|| raw.strip_prefix("```"))
                    .unwrap_or(raw);
                let raw = raw.strip_suffix("```").unwrap_or(raw);
                raw.trim().to_string()
            };

            if sql.is_empty() {
                return Err((
                    AppBuilderError::SyntaxError {
                        query: String::new(),
                        message: format!("LLM returned empty SQL for task '{}'", task.name),
                    },
                    BackTarget::Solve(spec.clone(), Default::default()),
                ));
            }

            // Emit event.
            if let Some(tx) = &self.event_tx {
                let _ = tx
                    .send(agentic_core::events::Event::Domain(
                        AppBuilderEvent::TaskSqlResolved {
                            task_name: task.name.clone(),
                            sql: sql.clone(),
                        },
                    ))
                    .await;
            }

            // Run a lightweight preview to determine actual columns and infer shape.
            // Control-source tasks produce option lists — shape inference is irrelevant.
            let (expected_columns, expected_shape) = if !task.is_control_source {
                let preview_sql = format!("{} LIMIT 5", sql.trim_end_matches(';'));
                match connector.execute_query(&preview_sql, 5).await {
                    Ok(exec) => {
                        let shape = infer_result_shape(&exec.result.columns, &exec.result.rows);
                        (exec.result.columns, shape)
                    }
                    Err(_) => (vec![], ResultShape::default()),
                }
            } else {
                (vec![], ResultShape::default())
            };

            resolved_tasks.push(ResolvedTask {
                name: task.name.clone(),
                sql,
                is_control_source: task.is_control_source,
                expected_shape,
                expected_columns,
            });
        }

        Ok(AppSolution {
            tasks: resolved_tasks,
            controls: spec.controls,
            layout: spec.layout,
            connector_name: spec.connector_name,
        })
    }
}

// ---------------------------------------------------------------------------
// State handler
// ---------------------------------------------------------------------------

/// Build the `StateHandler` for the **solving** state.
pub(super) fn build_solving_handler(
) -> StateHandler<AppBuilderDomain, AppBuilderSolver, AppBuilderEvent> {
    StateHandler {
        next: "executing",
        execute: Arc::new(
            |solver: &mut AppBuilderSolver,
             state,
             _events,
             run_ctx: &RunContext<AppBuilderDomain>,
             _memory: &SessionMemory<AppBuilderDomain>| {
                Box::pin(async move {
                    let spec = match state {
                        ProblemState::Solving(s) => s,
                        _ => unreachable!("solving handler called with wrong state"),
                    };
                    let retry_error = run_ctx
                        .retry_ctx
                        .as_ref()
                        .and_then(|r| r.errors.first())
                        .filter(|s| !s.is_empty())
                        .cloned();
                    match solver.solve_impl(spec, retry_error).await {
                        Ok(solution) => TransitionResult::ok(ProblemState::Executing(solution)),
                        Err((err, back)) => {
                            TransitionResult::diagnosing(ProblemState::Diagnosing {
                                error: err,
                                back,
                            })
                        }
                    }
                })
            },
        ),
        diagnose: None,
    }
}
