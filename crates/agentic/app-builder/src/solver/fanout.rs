//! [`FanoutWorker`] implementation for the app builder domain.
//!
//! Each fanned-out spec contains exactly 1 task.  The worker runs
//! solve (LLM SQL generation) then execute (connector dispatch) for
//! that single task, tagging all emitted events with `sub_spec_index`.

use std::sync::Arc;

use async_trait::async_trait;

use agentic_core::{
    back_target::BackTarget,
    events::{CoreEvent, DomainEvents, Event, EventStream, Outcome},
    orchestrator::{RunContext, SessionMemory},
    solver::FanoutWorker,
};
use agentic_llm::{InitialMessages, LlmError, ThinkingConfig, ToolLoopConfig};

use crate::events::AppBuilderEvent;
use crate::schemas::solve_response_schema;
use crate::tools::execute_solving_tool;
use crate::types::{
    AppBuilderDomain, AppBuilderError, AppResult, AppSpec, ResolvedTask, TaskResult,
};

use super::executing::{check_expected_columns, substitute_defaults, validate_control_refs};
use super::interpreting::infer_result_shape;
use super::prompts::SOLVING_SYSTEM_PROMPT;
use super::solver::AppBuilderFanoutWorker;
use super::solving::build_solve_user_prompt;

// ── Emit helper ──────────────────────────────────────────────────────────────

async fn emit<Ev: DomainEvents>(tx: &Option<EventStream<Ev>>, event: CoreEvent) {
    if let Some(tx) = tx {
        let _ = tx.send(Event::Core(event)).await;
    }
}

// ── FanoutWorker impl ────────────────────────────────────────────────────────

#[async_trait]
impl<Ev: DomainEvents> FanoutWorker<AppBuilderDomain, Ev> for AppBuilderFanoutWorker {
    async fn solve_and_execute(
        &self,
        spec: AppSpec,
        index: usize,
        _total: usize,
        _events: &Option<EventStream<Ev>>,
        ctx: &RunContext<AppBuilderDomain>,
        _mem: &SessionMemory<AppBuilderDomain>,
    ) -> Result<AppResult, (AppBuilderError, BackTarget<AppBuilderDomain>)> {
        // Use the worker's own typed event stream rather than the generic parameter.
        let events = &self.event_tx;
        // ── Resolve connector ────────────────────────────────────────────
        let connector = self
            .connectors
            .get(&spec.connector_name)
            .or_else(|| self.connectors.get(&self.default_connector))
            .or_else(|| self.connectors.values().next())
            .expect("AppBuilderFanoutWorker must have at least one connector")
            .clone();

        let sub = Some(index);

        // Each fanned-out spec has exactly 1 task.
        let task = spec
            .tasks
            .first()
            .expect("fanned-out spec must have exactly 1 task");

        // ==================================================================
        // SOLVING
        // ==================================================================

        // Retry path: skip LLM solving when pre-solved SQL is available.
        let resolved = if let Some(pre_sql) = self.pre_solved_sqls.get(&index) {
            emit(
                events,
                CoreEvent::StateEnter {
                    state: "solving".into(),
                    revision: 0,
                    trace_id: String::new(),
                    sub_spec_index: sub,
                },
            )
            .await;

            if let Some(tx) = events {
                let _ = tx
                    .send(Event::Domain(AppBuilderEvent::TaskSqlResolved {
                        task_name: task.name.clone(),
                        sql: pre_sql.clone(),
                    }))
                    .await;
            }

            let resolved = ResolvedTask {
                name: task.name.clone(),
                sql: pre_sql.clone(),
                is_control_source: task.is_control_source,
                expected_shape: task.expected_shape.clone(),
                expected_columns: task.expected_columns.clone(),
            };

            emit(
                events,
                CoreEvent::StateExit {
                    state: "solving".into(),
                    outcome: Outcome::Advanced,
                    trace_id: String::new(),
                    sub_spec_index: sub,
                },
            )
            .await;

            resolved
        } else {
            // Normal path: LLM solving.
            emit(
                events,
                CoreEvent::StateEnter {
                    state: "solving".into(),
                    revision: 0,
                    trace_id: String::new(),
                    sub_spec_index: sub,
                },
            )
            .await;

            let schema_summary = self.catalog.to_table_summary();
            let system_prompt = self.build_system_prompt("solving", SOLVING_SYSTEM_PROMPT);
            let thinking = self.thinking_for_state("solving", ThinkingConfig::Adaptive);
            let max_rounds = self.max_tool_rounds_for_state("solving", 10);
            let tools = crate::tools::solving_tools();

            let retry_error = ctx
                .retry_ctx
                .as_ref()
                .and_then(|r| r.errors.first())
                .filter(|s| !s.is_empty())
                .cloned();

            let user_prompt =
                build_solve_user_prompt(&spec, &task.name, retry_error.as_deref(), &schema_summary);
            let connector_for_tool = Arc::clone(&connector);

            let output = match self
                .client
                .run_with_tools(
                    &system_prompt,
                    InitialMessages::User(user_prompt),
                    &tools,
                    move |name: String, params| {
                        let conn = Arc::clone(&connector_for_tool);
                        Box::pin(async move { execute_solving_tool(&name, params, &*conn).await })
                    },
                    events,
                    ToolLoopConfig {
                        max_tool_rounds: max_rounds,
                        state: "solving".into(),
                        thinking: thinking.clone(),
                        response_schema: Some(solve_response_schema()),
                        max_tokens_override: self.max_tokens,
                        sub_spec_index: sub,
                    },
                )
                .await
            {
                Ok(v) => v,
                Err(LlmError::MaxToolRoundsReached { .. })
                | Err(LlmError::MaxTokensReached { .. }) => {
                    emit(
                        events,
                        CoreEvent::StateExit {
                            state: "solving".into(),
                            outcome: Outcome::Failed,
                            trace_id: String::new(),
                            sub_spec_index: sub,
                        },
                    )
                    .await;
                    return Err((
                        AppBuilderError::SyntaxError {
                            query: String::new(),
                            message: format!(
                                "LLM exhausted budget while solving task '{}'",
                                task.name
                            ),
                        },
                        BackTarget::Solve(spec.clone(), Default::default()),
                    ));
                }
                Err(e) => {
                    emit(
                        events,
                        CoreEvent::StateExit {
                            state: "solving".into(),
                            outcome: Outcome::Failed,
                            trace_id: String::new(),
                            sub_spec_index: sub,
                        },
                    )
                    .await;
                    return Err((
                        AppBuilderError::SyntaxError {
                            query: String::new(),
                            message: format!(
                                "LLM call failed during solving task '{}': {e}",
                                task.name
                            ),
                        },
                        BackTarget::Solve(spec.clone(), Default::default()),
                    ));
                }
            };

            // Extract SQL from the structured response or raw text.
            let sql = if let Some(structured) = output.structured_response {
                structured["sql"]
                    .as_str()
                    .unwrap_or_default()
                    .trim()
                    .to_string()
            } else {
                let raw = output.text.trim();
                let raw = raw
                    .strip_prefix("```sql")
                    .or_else(|| raw.strip_prefix("```"))
                    .unwrap_or(raw);
                let raw = raw.strip_suffix("```").unwrap_or(raw);
                raw.trim().to_string()
            };

            if sql.is_empty() {
                emit(
                    events,
                    CoreEvent::StateExit {
                        state: "solving".into(),
                        outcome: Outcome::Failed,
                        trace_id: String::new(),
                        sub_spec_index: sub,
                    },
                )
                .await;
                return Err((
                    AppBuilderError::SyntaxError {
                        query: String::new(),
                        message: format!("LLM returned empty SQL for task '{}'", task.name),
                    },
                    BackTarget::Solve(spec.clone(), Default::default()),
                ));
            }

            // Emit domain event for SQL resolved.
            if let Some(tx) = events {
                let _ = tx
                    .send(Event::Domain(AppBuilderEvent::TaskSqlResolved {
                        task_name: task.name.clone(),
                        sql: sql.clone(),
                    }))
                    .await;
            }

            let resolved = ResolvedTask {
                name: task.name.clone(),
                sql,
                is_control_source: task.is_control_source,
                expected_shape: task.expected_shape.clone(),
                expected_columns: task.expected_columns.clone(),
            };

            emit(
                events,
                CoreEvent::StateExit {
                    state: "solving".into(),
                    outcome: Outcome::Advanced,
                    trace_id: String::new(),
                    sub_spec_index: sub,
                },
            )
            .await;

            resolved
        };

        // ==================================================================
        // EXECUTING
        // ==================================================================
        emit(
            events,
            CoreEvent::StateEnter {
                state: "executing".into(),
                revision: 0,
                trace_id: String::new(),
                sub_spec_index: sub,
            },
        )
        .await;

        // Determine which pass this task belongs to and execute accordingly.
        let task_result = if resolved.is_control_source {
            // Pass 1 — control-source task (no template substitution).
            match connector.execute_query(&resolved.sql, 200).await {
                Ok(exec) => {
                    let row_count = exec.result.total_row_count;
                    if row_count == 0 {
                        emit(
                            events,
                            CoreEvent::StateExit {
                                state: "executing".into(),
                                outcome: Outcome::Failed,
                                trace_id: String::new(),
                                sub_spec_index: sub,
                            },
                        )
                        .await;
                        let solution = crate::types::AppSolution {
                            tasks: vec![resolved],
                            controls: spec.controls,
                            layout: spec.layout,
                            connector_name: spec.connector_name,
                        };
                        return Err((
                            AppBuilderError::EmptyResults {
                                task_name: task.name.clone(),
                            },
                            BackTarget::Execute(solution, Default::default()),
                        ));
                    }
                    if let Some(shape_err) = check_expected_columns(&resolved, &exec.result.columns)
                    {
                        emit(
                            events,
                            CoreEvent::StateExit {
                                state: "executing".into(),
                                outcome: Outcome::Failed,
                                trace_id: String::new(),
                                sub_spec_index: sub,
                            },
                        )
                        .await;
                        let solution = crate::types::AppSolution {
                            tasks: vec![resolved],
                            controls: spec.controls,
                            layout: spec.layout,
                            connector_name: spec.connector_name,
                        };
                        return Err((shape_err, BackTarget::Execute(solution, Default::default())));
                    }
                    if let Some(tx) = events {
                        let _ = tx
                            .send(Event::Domain(AppBuilderEvent::TaskExecuted {
                                task_name: task.name.clone(),
                                sql: resolved.sql.clone(),
                                row_count: row_count as usize,
                                columns: exec.result.columns.clone(),
                                sample_rows: super::executing::query_result_sample_rows(
                                    &exec.result,
                                ),
                            }))
                            .await;
                    }
                    let column_types: Vec<Option<String>> = exec
                        .summary
                        .columns
                        .iter()
                        .map(|c| c.data_type.clone())
                        .collect();
                    TaskResult {
                        name: resolved.name.clone(),
                        sql: resolved.sql.clone(),
                        columns: exec.result.columns.clone(),
                        column_types,
                        row_count: row_count as usize,
                        is_control_source: true,
                        expected_shape: infer_result_shape(&exec.result.columns, &exec.result.rows),
                        expected_columns: exec.result.columns.clone(),
                        sample: exec.result,
                    }
                }
                Err(e) => {
                    emit(
                        events,
                        CoreEvent::StateExit {
                            state: "executing".into(),
                            outcome: Outcome::Failed,
                            trace_id: String::new(),
                            sub_spec_index: sub,
                        },
                    )
                    .await;
                    let solution = crate::types::AppSolution {
                        tasks: vec![resolved],
                        controls: spec.controls,
                        layout: spec.layout,
                        connector_name: spec.connector_name,
                    };
                    return Err((
                        AppBuilderError::SyntaxError {
                            query: task.name.clone(),
                            message: e.to_string(),
                        },
                        BackTarget::Execute(solution, Default::default()),
                    ));
                }
            }
        } else {
            // Pass 2 — display task (substitute default control values).
            // Validate control references first.
            let missing = validate_control_refs(&resolved.sql, &spec.controls);
            if !missing.is_empty() {
                let errors: Vec<String> = missing
                    .into_iter()
                    .map(|name| {
                        format!(
                            "task '{}' references unknown control '{name}'",
                            resolved.name
                        )
                    })
                    .collect();
                emit(
                    events,
                    CoreEvent::StateExit {
                        state: "executing".into(),
                        outcome: Outcome::Failed,
                        trace_id: String::new(),
                        sub_spec_index: sub,
                    },
                )
                .await;
                let solution = crate::types::AppSolution {
                    tasks: vec![resolved],
                    controls: spec.controls,
                    layout: spec.layout,
                    connector_name: spec.connector_name,
                };
                return Err((
                    AppBuilderError::InvalidSpec { errors },
                    BackTarget::Execute(solution, Default::default()),
                ));
            }

            let substituted = substitute_defaults(&resolved.sql, &spec.controls);
            match connector.execute_query(&substituted, 100).await {
                Ok(exec) => {
                    let row_count = exec.result.total_row_count;
                    if row_count == 0 {
                        emit(
                            events,
                            CoreEvent::StateExit {
                                state: "executing".into(),
                                outcome: Outcome::Failed,
                                trace_id: String::new(),
                                sub_spec_index: sub,
                            },
                        )
                        .await;
                        let solution = crate::types::AppSolution {
                            tasks: vec![resolved],
                            controls: spec.controls,
                            layout: spec.layout,
                            connector_name: spec.connector_name,
                        };
                        return Err((
                            AppBuilderError::EmptyResults {
                                task_name: task.name.clone(),
                            },
                            BackTarget::Execute(solution, Default::default()),
                        ));
                    }
                    if let Some(shape_err) = check_expected_columns(&resolved, &exec.result.columns)
                    {
                        emit(
                            events,
                            CoreEvent::StateExit {
                                state: "executing".into(),
                                outcome: Outcome::Failed,
                                trace_id: String::new(),
                                sub_spec_index: sub,
                            },
                        )
                        .await;
                        let solution = crate::types::AppSolution {
                            tasks: vec![resolved],
                            controls: spec.controls,
                            layout: spec.layout,
                            connector_name: spec.connector_name,
                        };
                        return Err((shape_err, BackTarget::Execute(solution, Default::default())));
                    }
                    if let Some(tx) = events {
                        let _ = tx
                            .send(Event::Domain(AppBuilderEvent::TaskExecuted {
                                task_name: task.name.clone(),
                                sql: substituted.clone(),
                                row_count: row_count as usize,
                                columns: exec.result.columns.clone(),
                                sample_rows: super::executing::query_result_sample_rows(
                                    &exec.result,
                                ),
                            }))
                            .await;
                    }
                    let column_types: Vec<Option<String>> = exec
                        .summary
                        .columns
                        .iter()
                        .map(|c| c.data_type.clone())
                        .collect();
                    TaskResult {
                        name: resolved.name.clone(),
                        sql: resolved.sql.clone(),
                        columns: exec.result.columns.clone(),
                        column_types,
                        row_count: row_count as usize,
                        is_control_source: false,
                        expected_shape: infer_result_shape(&exec.result.columns, &exec.result.rows),
                        expected_columns: exec.result.columns.clone(),
                        sample: exec.result,
                    }
                }
                Err(e) => {
                    emit(
                        events,
                        CoreEvent::StateExit {
                            state: "executing".into(),
                            outcome: Outcome::Failed,
                            trace_id: String::new(),
                            sub_spec_index: sub,
                        },
                    )
                    .await;
                    let solution = crate::types::AppSolution {
                        tasks: vec![resolved],
                        controls: spec.controls,
                        layout: spec.layout,
                        connector_name: spec.connector_name,
                    };
                    return Err((
                        AppBuilderError::SyntaxError {
                            query: task.name.clone(),
                            message: e.to_string(),
                        },
                        BackTarget::Execute(solution, Default::default()),
                    ));
                }
            }
        };

        emit(
            events,
            CoreEvent::StateExit {
                state: "executing".into(),
                outcome: Outcome::Advanced,
                trace_id: String::new(),
                sub_spec_index: sub,
            },
        )
        .await;

        Ok(AppResult {
            task_results: vec![task_result],
            controls: spec.controls,
            layout: spec.layout,
            connector_name: spec.connector_name,
        })
    }
}
