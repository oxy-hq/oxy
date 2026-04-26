//! Workflow step orchestrator that drives the step DAG via coordinator
//! suspend/resume.
//!
//! The orchestrator runs as a long-lived task. For each workflow step it
//! either executes inline (formatter, conditional) or suspends to delegate
//! execution to the coordinator which dispatches it to a worker.

use std::collections::HashMap;
use std::sync::Arc;

use crate::config::{TaskType, WorkflowConfig};
use agentic_analytics::ProcedureStepInfo;
use agentic_core::delegation::{
    DelegationItem, DelegationTarget, FanoutFailurePolicy, SuspendReason, TaskOutcome, TaskSpec,
};
use agentic_core::evaluator::ConsistencyEvaluator;
use agentic_core::human_input::SuspendedRunData;
use serde_json::{Value, json};
use tokio::sync::mpsc;

/// Classifies a task type into an execution strategy.
enum StepKind {
    /// Execute directly in the orchestrator (no I/O, no coordinator round-trip).
    Inline,
    /// Delegate to coordinator as a single `WorkflowStep` child task.
    Delegated,
    /// Delegate to coordinator as a `TaskSpec::Agent`.
    Agent {
        agent_ref: String,
        prompt: String,
        consistency_run: usize,
        consistency_prompt: Option<String>,
    },
    /// Delegate to coordinator as a `TaskSpec::Workflow` (sub-workflow).
    SubWorkflow {
        src: String,
        variables: Option<Value>,
    },
    /// Fan-out loop iterations via `ParallelDelegation`.
    Loop {
        values: Value,
        tasks: Value,
        concurrency: usize,
    },
}

/// Drives a workflow's steps using the coordinator's suspend/resume mechanism.
///
/// For each I/O step, the orchestrator suspends with a `Delegation` or
/// `ParallelDelegation`, which the coordinator dispatches to a worker.
/// The worker executes the step and returns the result. The orchestrator
/// then merges the result into the render context and moves to the next step.
pub struct WorkflowStepOrchestrator {
    workflow: WorkflowConfig,
    /// Serialized render context — accumulated step outputs as JSON.
    render_context: Value,
    /// Workflow-level context (workspace path, global settings, etc.).
    workflow_context: Value,
    /// Step name → serialized OutputContainer result.
    results: HashMap<String, Value>,
    /// Current step index (for crash recovery).
    current_step: usize,
    /// Trace ID for event correlation.
    trace_id: String,
    /// Optional LLM-based consistency evaluator for pairwise answer comparison.
    evaluator: Option<Arc<dyn ConsistencyEvaluator>>,
}

impl WorkflowStepOrchestrator {
    pub fn new(
        workflow: WorkflowConfig,
        workflow_context: Value,
        variables: Option<Value>,
        trace_id: String,
        evaluator: Option<Arc<dyn ConsistencyEvaluator>>,
    ) -> Self {
        // Seed render context with workflow-level variables if provided.
        let render_context = variables.unwrap_or(json!({}));
        Self {
            workflow,
            render_context,
            workflow_context,
            results: HashMap::new(),
            current_step: 0,
            trace_id,
            evaluator,
        }
    }

    /// Run the orchestrator loop.
    ///
    /// Iterates over workflow steps, emitting events and outcomes on the
    /// provided channels. Suspends for delegated steps and waits for the
    /// coordinator to resume with the child's answer.
    pub async fn run(
        &mut self,
        event_tx: mpsc::Sender<(String, Value)>,
        outcome_tx: mpsc::Sender<TaskOutcome>,
        mut answer_rx: mpsc::Receiver<String>,
    ) -> Result<(), String> {
        let procedure_name = self.workflow.name.clone();

        // Emit ProcedureStarted with the full step list.
        let steps: Vec<ProcedureStepInfo> = self
            .workflow
            .tasks
            .iter()
            .map(|t| ProcedureStepInfo {
                name: t.name.clone(),
                task_type: format!("{:?}", t.task_type).to_lowercase(),
            })
            .collect();
        self.emit_event(
            &event_tx,
            "procedure_started",
            json!({
                "procedure_name": &procedure_name,
                "steps": steps.iter().map(|s| json!({"name": &s.name, "task_type": &s.task_type})).collect::<Vec<_>>(),
            }),
        )
        .await;

        // Process each step.
        while self.current_step < self.workflow.tasks.len() {
            let task = self.workflow.tasks[self.current_step].clone();
            let step_name = task.name.clone();
            let kind = self.classify_step(&task.task_type);

            // Emit step started.
            self.emit_event(
                &event_tx,
                "procedure_step_started",
                json!({ "step": &step_name }),
            )
            .await;

            let result = match kind {
                StepKind::Inline => {
                    // Execute inline (formatter, conditional).
                    self.execute_inline(&task.task_type)
                }

                StepKind::Delegated => {
                    // Suspend for WorkflowStep delegation.
                    let step_config = serde_json::to_value(&task)
                        .map_err(|e| format!("failed to serialize step config: {e}"))?;

                    self.suspend_for_step(
                        &outcome_tx,
                        &mut answer_rx,
                        &step_name,
                        TaskSpec::WorkflowStep {
                            step_config,
                            render_context: self.render_context.clone(),
                            workflow_context: self.workflow_context.clone(),
                        },
                    )
                    .await
                }

                StepKind::Agent {
                    agent_ref,
                    prompt,
                    consistency_run,
                    consistency_prompt,
                } => {
                    // Render the prompt with current context.
                    // NOTE: Full minijinja rendering happens on the step worker side.
                    // Here we pass the raw prompt; the agent pipeline handles it.
                    if consistency_run > 1 {
                        self.suspend_for_consistency_agents(
                            &outcome_tx,
                            &mut answer_rx,
                            &step_name,
                            &agent_ref,
                            &prompt,
                            consistency_run,
                            consistency_prompt.as_deref(),
                        )
                        .await
                    } else {
                        self.suspend_for_step(
                            &outcome_tx,
                            &mut answer_rx,
                            &step_name,
                            TaskSpec::Agent {
                                agent_id: agent_ref,
                                question: prompt,
                            },
                        )
                        .await
                    }
                }

                StepKind::SubWorkflow { src, variables } => {
                    self.suspend_for_step(
                        &outcome_tx,
                        &mut answer_rx,
                        &step_name,
                        TaskSpec::Workflow {
                            workflow_ref: src,
                            variables,
                        },
                    )
                    .await
                }

                StepKind::Loop {
                    values,
                    tasks,
                    concurrency,
                } => {
                    self.suspend_for_loop(
                        &outcome_tx,
                        &mut answer_rx,
                        &step_name,
                        values,
                        tasks,
                        concurrency,
                    )
                    .await
                }
            };

            match result {
                Ok(output) => {
                    // Merge result into context.
                    self.results.insert(step_name.clone(), output.clone());
                    self.update_render_context();

                    self.emit_event(
                        &event_tx,
                        "procedure_step_completed",
                        json!({ "step": &step_name, "success": true }),
                    )
                    .await;
                }
                Err(e) => {
                    self.emit_event(
                        &event_tx,
                        "procedure_step_completed",
                        json!({ "step": &step_name, "success": false, "error": &e }),
                    )
                    .await;

                    self.emit_event(
                        &event_tx,
                        "procedure_completed",
                        json!({
                            "procedure_name": &procedure_name,
                            "success": false,
                            "error": &e,
                        }),
                    )
                    .await;

                    return Err(e);
                }
            }

            self.current_step += 1;
        }

        // All steps done.
        self.emit_event(
            &event_tx,
            "procedure_completed",
            json!({
                "procedure_name": &procedure_name,
                "success": true,
            }),
        )
        .await;

        // Emit final Done outcome with aggregated results as a JSON array.
        // The analytics Interpreting stage's `parse_delegation_answer` expects
        // `[{columns: [...], rows: [...]}, ...]` — one entry per step.
        let final_output: Vec<Value> = self
            .workflow
            .tasks
            .iter()
            .filter_map(|t| self.results.get(&t.name))
            .cloned()
            .collect();
        let _ = outcome_tx
            .send(TaskOutcome::Done {
                answer: serde_json::to_string(&final_output).unwrap_or_else(|_| "[]".to_string()),
                metadata: None,
            })
            .await;

        Ok(())
    }

    // ── Step classification ─────────────────────────────────────────────

    fn classify_step(&self, task_type: &TaskType) -> StepKind {
        match task_type {
            TaskType::Formatter(_) | TaskType::Conditional(_) => StepKind::Inline,

            TaskType::Agent(agent_task) => StepKind::Agent {
                agent_ref: agent_task.agent_ref.clone(),
                prompt: agent_task.prompt.clone(),
                consistency_run: agent_task.consistency_run,
                consistency_prompt: agent_task
                    .consistency_prompt
                    .clone()
                    .or_else(|| self.workflow.consistency_prompt.clone()),
            },

            TaskType::SubWorkflow(wf_task) => StepKind::SubWorkflow {
                src: wf_task.src.to_string_lossy().to_string(),
                variables: wf_task
                    .variables
                    .as_ref()
                    .map(|v| serde_json::to_value(v).unwrap_or_default()),
            },

            TaskType::LoopSequential(loop_task) => StepKind::Loop {
                values: serde_json::to_value(&loop_task.values).unwrap_or_default(),
                tasks: serde_json::to_value(&loop_task.tasks).unwrap_or_default(),
                concurrency: loop_task.concurrency,
            },

            // All I/O task types: delegate to coordinator.
            TaskType::ExecuteSql(_)
            | TaskType::SemanticQuery(_)
            | TaskType::OmniQuery(_)
            | TaskType::LookerQuery(_)
            | TaskType::Visualize(_)
            | TaskType::Unknown => StepKind::Delegated,
        }
    }

    // ── Inline execution ────────────────────────────────────────────────

    fn execute_inline(&self, task_type: &TaskType) -> Result<Value, String> {
        match task_type {
            TaskType::Formatter(fmt) => self.execute_formatter(&fmt.template),
            TaskType::Conditional(cond) => self.execute_conditional(cond),
            _ => Err("not an inline step".to_string()),
        }
    }

    /// Render a Jinja2 template with the accumulated render context.
    fn execute_formatter(&self, template: &str) -> Result<Value, String> {
        let mut env = minijinja::Environment::new();
        env.set_undefined_behavior(minijinja::UndefinedBehavior::Chainable);

        let tmpl = env
            .template_from_str(template)
            .map_err(|e| format!("template parse error: {e}"))?;

        // Build context with ColumnTable wrappers for table step results.
        let ctx = build_minijinja_context(&self.render_context);
        let rendered = tmpl.render(&ctx).map_err(|e| {
            let available_keys: Vec<String> = self
                .render_context
                .as_object()
                .map(|obj| obj.keys().cloned().collect())
                .unwrap_or_default();
            format!(
                "template render error: {e}\n\
                 Template (first 200 chars): {}\n\
                 Available context keys: {:?}",
                &template[..template.len().min(200)],
                available_keys,
            )
        })?;

        Ok(json!({ "text": rendered }))
    }

    /// Evaluate conditional branches and return the first matching branch's
    /// placeholder result, or the else branch.
    fn execute_conditional(
        &self,
        cond: &crate::config::ConditionalConfig,
    ) -> Result<Value, String> {
        let mut env = minijinja::Environment::new();
        env.set_undefined_behavior(minijinja::UndefinedBehavior::Chainable);
        let ctx = build_minijinja_context(&self.render_context);

        for branch in &cond.conditions {
            let expr_template = format!("{{{{{}}}}}", branch.condition);
            let tmpl = env
                .template_from_str(&expr_template)
                .map_err(|e| format!("condition parse error: {e}"))?;
            let result = tmpl.render(ctx.clone()).unwrap_or_default();
            let trimmed = result.trim();

            // Truthy: non-empty, not "false", not "0", not "none"
            let is_truthy = !trimmed.is_empty()
                && trimmed != "false"
                && trimmed != "0"
                && trimmed.to_lowercase() != "none";

            if is_truthy {
                // Return branch task names as the result — the actual
                // execution of branch tasks would require delegation.
                let task_names: Vec<String> = branch.tasks.iter().map(|t| t.name.clone()).collect();
                return Ok(json!({
                    "branch": "matched",
                    "condition": &branch.condition,
                    "tasks": task_names,
                }));
            }
        }

        // No condition matched — use else branch if present.
        if let Some(else_tasks) = &cond.else_tasks {
            let task_names: Vec<String> = else_tasks.iter().map(|t| t.name.clone()).collect();
            Ok(json!({
                "branch": "else",
                "tasks": task_names,
            }))
        } else {
            Ok(json!({ "branch": "none_matched" }))
        }
    }

    // ── Delegation helpers ──────────────────────────────────────────────

    /// Suspend for a single child task and wait for the answer.
    async fn suspend_for_step(
        &self,
        outcome_tx: &mpsc::Sender<TaskOutcome>,
        answer_rx: &mut mpsc::Receiver<String>,
        step_name: &str,
        spec: TaskSpec,
    ) -> Result<Value, String> {
        let suspend_data = self.build_suspend_data(step_name);

        // Determine delegation target from spec.
        let (target, request, context) = match &spec {
            TaskSpec::Agent { agent_id, question } => (
                DelegationTarget::Agent {
                    agent_id: agent_id.clone(),
                },
                question.clone(),
                json!({}),
            ),
            TaskSpec::Workflow {
                workflow_ref,
                variables,
            } => (
                DelegationTarget::Workflow {
                    workflow_ref: workflow_ref.clone(),
                },
                format!("Execute sub-workflow: {workflow_ref}"),
                variables.clone().unwrap_or(json!({})),
            ),
            TaskSpec::WorkflowStep {
                step_config,
                render_context,
                workflow_context,
            } => (
                DelegationTarget::Workflow {
                    workflow_ref: "__workflow_step__".to_string(),
                },
                step_name.to_string(),
                json!({
                    "step_config": step_config,
                    "render_context": render_context,
                    "workflow_context": workflow_context,
                }),
            ),
            _ => {
                return Err(format!("unexpected spec type for step {step_name}"));
            }
        };

        outcome_tx
            .send(TaskOutcome::Suspended {
                reason: SuspendReason::Delegation {
                    target,
                    request,
                    context,
                    policy: None,
                },
                resume_data: suspend_data,
                trace_id: self.trace_id.clone(),
            })
            .await
            .map_err(|_| "outcome channel closed".to_string())?;

        // Wait for coordinator to resume with child's answer.
        let answer = answer_rx
            .recv()
            .await
            .ok_or_else(|| "answer channel closed".to_string())?;

        // Parse the answer as JSON (OutputContainer or plain text).
        serde_json::from_str::<Value>(&answer).or_else(|_| Ok(json!({ "text": answer })))
    }

    /// Suspend for N parallel agent tasks (consistency run).
    async fn suspend_for_consistency_agents(
        &self,
        outcome_tx: &mpsc::Sender<TaskOutcome>,
        answer_rx: &mut mpsc::Receiver<String>,
        step_name: &str,
        agent_ref: &str,
        prompt: &str,
        n: usize,
        consistency_prompt: Option<&str>,
    ) -> Result<Value, String> {
        let targets: Vec<DelegationItem> = (0..n)
            .map(|_| DelegationItem {
                target: DelegationTarget::Agent {
                    agent_id: agent_ref.to_string(),
                },
                request: prompt.to_string(),
                context: json!({}),
            })
            .collect();

        let suspend_data = self.build_suspend_data(step_name);

        outcome_tx
            .send(TaskOutcome::Suspended {
                reason: SuspendReason::ParallelDelegation {
                    targets,
                    failure_policy: FanoutFailurePolicy::BestEffort,
                },
                resume_data: suspend_data,
                trace_id: self.trace_id.clone(),
            })
            .await
            .map_err(|_| "outcome channel closed".to_string())?;

        let answer = answer_rx
            .recv()
            .await
            .ok_or_else(|| "answer channel closed".to_string())?;

        // Parse the aggregated results from the coordinator.
        // ParallelDelegation with BestEffort returns a JSON object keyed by child_id:
        // { "child_id_1": { "status": "done", "answer": "..." }, ... }
        let aggregated: Value = serde_json::from_str(&answer).unwrap_or(json!({ "text": answer }));

        // Extract individual answers.
        let mut answers: Vec<String> = Vec::new();
        if let Some(obj) = aggregated.as_object() {
            for (_child_id, result) in obj {
                if result.get("status").and_then(|s| s.as_str()) == Some("done")
                    && let Some(a) = result.get("answer").and_then(|a| a.as_str())
                {
                    answers.push(a.to_string());
                }
            }
        }

        if answers.is_empty() {
            // All children failed or no parseable results — return the raw answer.
            return Ok(json!({
                "value": json!({ "text": answer }),
                "score": 0.0,
                "consistency_run": n,
            }));
        }

        // Pick the best answer using the consistency evaluator if available,
        // otherwise fall back to majority-vote by exact string equality.
        let (best_answer, score) = if let Some(evaluator) = &self.evaluator {
            match evaluator
                .evaluate(prompt, &answers, consistency_prompt)
                .await
            {
                Ok(result) => {
                    let selected = answers
                        .get(result.selected_index)
                        .cloned()
                        .unwrap_or_else(|| answers[0].clone());
                    tracing::info!(
                        selected_index = result.selected_index,
                        score = result.score,
                        reasoning = %result.reasoning,
                        "consistency evaluator picked answer"
                    );
                    (selected, result.score)
                }
                Err(e) => {
                    tracing::warn!(error = %e, "consistency evaluator failed, falling back to majority-vote");
                    majority_vote(&answers)
                }
            }
        } else {
            majority_vote(&answers)
        };

        // Parse the winning answer as JSON if possible.
        let value: Value =
            serde_json::from_str(&best_answer).unwrap_or_else(|_| json!({ "text": best_answer }));

        Ok(json!({
            "value": value,
            "score": score,
            "consistency_run": n,
        }))
    }

    /// Suspend for loop iterations via ParallelDelegation or sequential Delegation.
    async fn suspend_for_loop(
        &self,
        outcome_tx: &mpsc::Sender<TaskOutcome>,
        answer_rx: &mut mpsc::Receiver<String>,
        step_name: &str,
        values: Value,
        tasks: Value,
        concurrency: usize,
    ) -> Result<Value, String> {
        let items = values
            .as_array()
            .ok_or_else(|| format!("loop {step_name}: values must be an array"))?;

        if items.is_empty() {
            return Ok(json!([]));
        }

        // Build a WorkflowStep for each loop iteration.
        // Each iteration gets the loop variable injected into its render context.
        let targets: Vec<DelegationItem> = items
            .iter()
            .enumerate()
            .map(|(i, item)| {
                // Inject loop variable into render context for this iteration.
                let mut iter_context = self.render_context.clone();
                if let Some(obj) = iter_context.as_object_mut() {
                    obj.insert(step_name.to_string(), json!({ "value": item, "index": i }));
                }

                DelegationItem {
                    target: DelegationTarget::Workflow {
                        workflow_ref: "__workflow_step__".to_string(),
                    },
                    request: format!("{step_name}[{i}]"),
                    context: json!({
                        "step_config": {
                            "name": format!("{step_name}_{i}"),
                            "tasks": &tasks,
                        },
                        "render_context": iter_context,
                        "workflow_context": &self.workflow_context,
                        "loop_item": item,
                        "loop_index": i,
                    }),
                }
            })
            .collect();

        let suspend_data = self.build_suspend_data(step_name);

        if concurrency > 1 && items.len() > 1 {
            // Parallel fan-out.
            outcome_tx
                .send(TaskOutcome::Suspended {
                    reason: SuspendReason::ParallelDelegation {
                        targets,
                        failure_policy: FanoutFailurePolicy::FailFast,
                    },
                    resume_data: suspend_data,
                    trace_id: self.trace_id.clone(),
                })
                .await
                .map_err(|_| "outcome channel closed".to_string())?;

            let answer = answer_rx
                .recv()
                .await
                .ok_or_else(|| "answer channel closed".to_string())?;

            serde_json::from_str::<Value>(&answer).or_else(|_| Ok(json!({ "text": answer })))
        } else {
            // Sequential: delegate one at a time.
            let mut loop_results = Vec::new();
            for target in targets {
                let iter_spec = TaskSpec::WorkflowStep {
                    step_config: target.context["step_config"].clone(),
                    render_context: target.context["render_context"].clone(),
                    workflow_context: target.context["workflow_context"].clone(),
                };

                let result = self
                    .suspend_for_step(outcome_tx, answer_rx, &target.request, iter_spec)
                    .await?;
                loop_results.push(result);
            }
            Ok(json!(loop_results))
        }
    }

    // ── State management ────────────────────────────────────────────────

    fn build_suspend_data(&self, step_name: &str) -> SuspendedRunData {
        SuspendedRunData {
            from_state: "workflow".to_string(),
            original_input: self.workflow.name.clone(),
            trace_id: self.trace_id.clone(),
            // Use full to_state() so from_state() can reconstruct the
            // orchestrator after a crash. Includes workflow config,
            // render_context, results, current_step, etc.
            stage_data: self.to_state(),
            question: format!("Executing step: {step_name}"),
            suggestions: vec![],
        }
    }

    fn update_render_context(&mut self) {
        // Rebuild render context from accumulated results.
        let mut ctx = if let Some(obj) = self.render_context.as_object() {
            obj.clone()
        } else {
            serde_json::Map::new()
        };
        for (name, value) in &self.results {
            // Convert row-oriented {columns, rows} to column-oriented
            // {col_name: [val, ...]} for template access like
            // {{ step_name.column_name[i] }}.
            let context_value = to_column_oriented(value);
            ctx.insert(name.clone(), context_value);
        }
        self.render_context = Value::Object(ctx);
    }

    /// Serialize orchestrator state for crash recovery.
    pub fn to_state(&self) -> Value {
        json!({
            "current_step": self.current_step,
            "results": self.results,
            "render_context": self.render_context,
            "workflow": serde_json::to_value(&self.workflow).unwrap_or_default(),
            "workflow_context": self.workflow_context,
            "trace_id": self.trace_id,
        })
    }

    /// Restore orchestrator from serialized state.
    pub fn from_state(state: Value) -> Result<Self, String> {
        let workflow: WorkflowConfig = serde_json::from_value(state["workflow"].clone())
            .map_err(|e| format!("failed to deserialize workflow: {e}"))?;
        let results: HashMap<String, Value> =
            serde_json::from_value(state["results"].clone()).unwrap_or_default();
        let current_step = state["current_step"].as_u64().unwrap_or(0) as usize;
        let render_context = state["render_context"].clone();
        let workflow_context = state["workflow_context"].clone();
        let trace_id = state["trace_id"].as_str().unwrap_or("unknown").to_string();

        Ok(Self {
            workflow,
            render_context,
            workflow_context,
            results,
            current_step,
            trace_id,
            evaluator: None, // Evaluator is set via set_evaluator() after recovery.
        })
    }

    /// Set the consistency evaluator (used after crash recovery via `from_state`).
    pub fn set_evaluator(&mut self, evaluator: Option<Arc<dyn ConsistencyEvaluator>>) {
        self.evaluator = evaluator;
    }

    /// Access the workflow configuration (used by pipeline to build evaluator on resume).
    pub fn workflow_config(&self) -> &WorkflowConfig {
        &self.workflow
    }

    // ── Event emission ──────────────────────────────────────────────────

    async fn emit_event(
        &self,
        event_tx: &mpsc::Sender<(String, Value)>,
        event_type: &str,
        payload: Value,
    ) {
        let _ = event_tx.send((event_type.to_string(), payload)).await;
    }
}

pub mod minijinja_helpers;

#[cfg(test)]
mod tests;

use minijinja_helpers::majority_vote;
pub(crate) use minijinja_helpers::{build_minijinja_context, to_column_oriented};
