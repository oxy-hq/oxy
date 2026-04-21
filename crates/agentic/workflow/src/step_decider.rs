//! Stateless workflow decision task.
//!
//! Replaces the long-lived `WorkflowStepOrchestrator` actor. Each call to
//! `WorkflowDecider::decide` loads state, folds in any completed child answer,
//! decides the next action, and returns. No in-memory channels survive a crash.

use std::sync::Arc;

use crate::config::TaskType;
use crate::extension::WorkflowRunState;
use crate::step_orchestrator::{build_minijinja_context, to_column_oriented};
use agentic_core::delegation::{
    ChildCompletion, DelegationItem, DelegationTarget, FanoutFailurePolicy, TaskSpec,
};
use agentic_core::evaluator::ConsistencyEvaluator;
use serde_json::{Value, json};

/// What the decider decided to do next.
#[derive(Debug)]
pub enum WorkflowDecision {
    /// Delegate a single child task and wait for its answer.
    DelegateStep {
        step_index: usize,
        step_name: String,
        spec: TaskSpec,
        trace_id: String,
        emitted_events: Vec<(String, Value)>,
    },
    /// Fan-out parallel delegation (consistency runs or sequential loops).
    DelegateParallel {
        step_index: usize,
        step_name: String,
        items: Vec<DelegationItem>,
        failure_policy: FanoutFailurePolicy,
        trace_id: String,
        emitted_events: Vec<(String, Value)>,
    },
    /// Inline step (formatter/conditional) was executed; chain to next decision.
    StepExecutedInline {
        step_name: String,
        emitted_events: Vec<(String, Value)>,
    },
    /// Parallel siblings still in flight — do nothing until another sibling completes.
    WaitForMoreChildren,
    /// All steps done — workflow is complete.
    Complete {
        final_answer: String,
        emitted_events: Vec<(String, Value)>,
    },
    /// Unrecoverable error.
    Fail(String),
}

/// Stateless workflow decider.
///
/// Call [`decide`] with the current DB state and an optional completed child
/// answer. The function returns the updated state and the next action to take.
/// The caller (executor) persists the updated state and acts on the decision.
pub struct WorkflowDecider {
    #[allow(dead_code)]
    evaluator: Option<Arc<dyn ConsistencyEvaluator>>,
}

impl WorkflowDecider {
    pub fn new(evaluator: Option<Arc<dyn ConsistencyEvaluator>>) -> Self {
        Self { evaluator }
    }

    /// Core decision function.
    ///
    /// - `state`: loaded from `agentic_workflow_state`.
    /// - `pending_child_answer`: a just-completed child task, if any.
    ///
    /// Returns `(updated_state, decision)`. The caller must persist `updated_state`
    /// before acting on the decision (optimistic CC via `decision_version`).
    pub async fn decide(
        &self,
        mut state: WorkflowRunState,
        pending_child_answer: Option<ChildCompletion>,
    ) -> (WorkflowRunState, WorkflowDecision) {
        // Events emitted during the fold phase — prepended to the decision's events.
        let mut fold_events: Vec<(String, Value)> = Vec::new();

        // ── 1. Fold in child answer if present ────────────────────────────
        if let Some(child) = pending_child_answer {
            let step_key = child.step_index.to_string();
            let step_name = child.step_name.clone();
            let answer_value = serde_json::from_str::<Value>(&child.answer)
                .unwrap_or_else(|_| json!({"text": child.answer}));
            state.results.insert(step_name.clone(), answer_value);

            // Remove this child from pending_children.
            if let Some(siblings) = state.pending_children.get_mut(&step_key) {
                siblings.retain(|id| id != &child.child_task_id);
                if siblings.is_empty() {
                    state.pending_children.remove(&step_key);
                }
            }

            // Still waiting on sibling tasks for this step?
            if state.pending_children.contains_key(&step_key) {
                return (state, WorkflowDecision::WaitForMoreChildren);
            }

            // Step complete: emit event, rebuild render context, advance.
            let success = child.status == "done";
            fold_events.push((
                "procedure_step_completed".to_string(),
                json!({ "step": step_name, "success": success }),
            ));
            update_render_context(&mut state);
            state.current_step = child.step_index + 1;
        }

        // ── 2. Check for workflow completion ──────────────────────────────
        if state.current_step >= state.workflow.tasks.len() {
            let final_answer = build_final_answer(&state);
            let mut events = fold_events;
            events.push((
                "procedure_completed".to_string(),
                json!({
                    "procedure_name": state.workflow.name,
                    "success": true,
                }),
            ));
            return (
                state,
                WorkflowDecision::Complete {
                    final_answer,
                    emitted_events: events,
                },
            );
        }

        // ── 3. Decide on the current step ─────────────────────────────────
        let step_index = state.current_step;
        let task = state.workflow.tasks[step_index].clone();
        let step_name = task.name.clone();
        let trace_id = state.trace_id.clone();
        let wf_name = state.workflow.name.clone();

        let mut events: Vec<(String, Value)> = fold_events;

        // Emit procedure_started on the very first step.
        if step_index == 0 {
            let steps: Vec<Value> = state
                .workflow
                .tasks
                .iter()
                .map(|t| {
                    json!({
                        "name": t.name,
                        "task_type": format!("{:?}", t.task_type).to_lowercase(),
                    })
                })
                .collect();
            events.push((
                "procedure_started".to_string(),
                json!({ "procedure_name": wf_name, "steps": steps }),
            ));
        }
        events.push((
            "procedure_step_started".to_string(),
            json!({ "step": step_name }),
        ));

        match classify_step(&state, &task.task_type) {
            StepKind::Inline => match execute_inline(&state, &task.task_type) {
                Ok(output) => {
                    state.results.insert(step_name.clone(), output);
                    update_render_context(&mut state);
                    events.push((
                        "procedure_step_completed".to_string(),
                        json!({ "step": step_name, "success": true }),
                    ));
                    state.current_step += 1;
                    (
                        state,
                        WorkflowDecision::StepExecutedInline {
                            step_name,
                            emitted_events: events,
                        },
                    )
                }
                Err(e) => {
                    events.push((
                        "procedure_step_completed".to_string(),
                        json!({ "step": step_name, "success": false, "error": e }),
                    ));
                    (state, WorkflowDecision::Fail(e))
                }
            },

            StepKind::Delegated => {
                let step_config =
                    serde_json::to_value(&task).unwrap_or_else(|_| json!({"name": step_name}));
                let spec = TaskSpec::WorkflowStep {
                    step_config,
                    render_context: state.render_context.clone(),
                    workflow_context: state.workflow_context.clone(),
                };
                (
                    state,
                    WorkflowDecision::DelegateStep {
                        step_index,
                        step_name,
                        spec,
                        trace_id,
                        emitted_events: events,
                    },
                )
            }

            StepKind::Agent {
                agent_ref,
                prompt,
                consistency_run,
                ..
            } => {
                if consistency_run > 1 {
                    let items = (0..consistency_run)
                        .map(|_| DelegationItem {
                            target: DelegationTarget::Agent {
                                agent_id: agent_ref.clone(),
                            },
                            request: prompt.clone(),
                            context: json!({}),
                        })
                        .collect();
                    (
                        state,
                        WorkflowDecision::DelegateParallel {
                            step_index,
                            step_name,
                            items,
                            failure_policy: FanoutFailurePolicy::BestEffort,
                            trace_id,
                            emitted_events: events,
                        },
                    )
                } else {
                    let spec = TaskSpec::Agent {
                        agent_id: agent_ref,
                        question: prompt,
                    };
                    (
                        state,
                        WorkflowDecision::DelegateStep {
                            step_index,
                            step_name,
                            spec,
                            trace_id,
                            emitted_events: events,
                        },
                    )
                }
            }

            StepKind::SubWorkflow { src, variables } => {
                let spec = TaskSpec::Workflow {
                    workflow_ref: src,
                    variables,
                };
                (
                    state,
                    WorkflowDecision::DelegateStep {
                        step_index,
                        step_name,
                        spec,
                        trace_id,
                        emitted_events: events,
                    },
                )
            }

            StepKind::Loop {
                values,
                tasks,
                concurrency,
            } => {
                let items_arr = match values.as_array() {
                    Some(a) => a.clone(),
                    None => {
                        return (
                            state,
                            WorkflowDecision::Fail(format!(
                                "loop {step_name}: values must be an array"
                            )),
                        );
                    }
                };

                if items_arr.is_empty() {
                    state.results.insert(step_name.clone(), json!([]));
                    update_render_context(&mut state);
                    events.push((
                        "procedure_step_completed".to_string(),
                        json!({ "step": step_name, "success": true }),
                    ));
                    state.current_step += 1;
                    return (
                        state,
                        WorkflowDecision::StepExecutedInline {
                            step_name,
                            emitted_events: events,
                        },
                    );
                }

                let delegation_items: Vec<DelegationItem> = items_arr
                    .iter()
                    .enumerate()
                    .map(|(i, item)| {
                        let mut iter_context = state.render_context.clone();
                        if let Some(obj) = iter_context.as_object_mut() {
                            obj.insert(
                                step_name.clone(),
                                json!({ "value": item, "index": i }),
                            );
                        }
                        DelegationItem {
                            target: DelegationTarget::Workflow {
                                workflow_ref: "__workflow_step__".to_string(),
                            },
                            request: format!("{step_name}[{i}]"),
                            context: json!({
                                "step_config": { "name": format!("{step_name}_{i}"), "tasks": &tasks },
                                "render_context": iter_context,
                                "workflow_context": &state.workflow_context,
                                "loop_item": item,
                                "loop_index": i,
                            }),
                        }
                    })
                    .collect();

                let failure_policy = if concurrency > 1 && items_arr.len() > 1 {
                    FanoutFailurePolicy::FailFast
                } else {
                    FanoutFailurePolicy::BestEffort
                };

                (
                    state,
                    WorkflowDecision::DelegateParallel {
                        step_index,
                        step_name,
                        items: delegation_items,
                        failure_policy,
                        trace_id,
                        emitted_events: events,
                    },
                )
            }
        }
    }
}

// ── Internal helpers ──────────────────────────────────────────────────────────

enum StepKind {
    Inline,
    Delegated,
    Agent {
        agent_ref: String,
        prompt: String,
        consistency_run: usize,
        #[allow(dead_code)]
        consistency_prompt: Option<String>,
    },
    SubWorkflow {
        src: String,
        variables: Option<Value>,
    },
    Loop {
        values: Value,
        tasks: Value,
        concurrency: usize,
    },
}

fn classify_step(state: &WorkflowRunState, task_type: &TaskType) -> StepKind {
    match task_type {
        TaskType::Formatter(_) | TaskType::Conditional(_) => StepKind::Inline,

        TaskType::Agent(agent_task) => StepKind::Agent {
            agent_ref: agent_task.agent_ref.clone(),
            prompt: agent_task.prompt.clone(),
            consistency_run: agent_task.consistency_run,
            consistency_prompt: agent_task
                .consistency_prompt
                .clone()
                .or_else(|| state.workflow.consistency_prompt.clone()),
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

        TaskType::ExecuteSql(_)
        | TaskType::SemanticQuery(_)
        | TaskType::OmniQuery(_)
        | TaskType::LookerQuery(_)
        | TaskType::Visualize(_)
        | TaskType::Unknown => StepKind::Delegated,
    }
}

fn execute_inline(state: &WorkflowRunState, task_type: &TaskType) -> Result<Value, String> {
    match task_type {
        TaskType::Formatter(fmt) => execute_formatter(&state.render_context, &fmt.template),
        TaskType::Conditional(cond) => execute_conditional(&state.render_context, cond),
        _ => Err("not an inline step".to_string()),
    }
}

fn execute_formatter(render_context: &Value, template: &str) -> Result<Value, String> {
    let mut env = minijinja::Environment::new();
    env.set_undefined_behavior(minijinja::UndefinedBehavior::Chainable);
    let tmpl = env
        .template_from_str(template)
        .map_err(|e| format!("template parse error: {e}"))?;
    let ctx = build_minijinja_context(render_context);
    let rendered = tmpl
        .render(&ctx)
        .map_err(|e| format!("template render error: {e}"))?;
    Ok(json!({ "text": rendered }))
}

fn execute_conditional(
    render_context: &Value,
    cond: &crate::config::ConditionalConfig,
) -> Result<Value, String> {
    let mut env = minijinja::Environment::new();
    env.set_undefined_behavior(minijinja::UndefinedBehavior::Chainable);
    let ctx = build_minijinja_context(render_context);

    for branch in &cond.conditions {
        let expr_template = format!("{{{{{}}}}}", branch.condition);
        let tmpl = env
            .template_from_str(&expr_template)
            .map_err(|e| format!("condition parse error: {e}"))?;
        let result = tmpl.render(ctx.clone()).unwrap_or_default();
        let trimmed = result.trim();
        let is_truthy = !trimmed.is_empty()
            && trimmed != "false"
            && trimmed != "0"
            && trimmed.to_lowercase() != "none";
        if is_truthy {
            let task_names: Vec<String> = branch.tasks.iter().map(|t| t.name.clone()).collect();
            return Ok(json!({
                "branch": "matched",
                "condition": branch.condition,
                "tasks": task_names,
            }));
        }
    }
    if let Some(else_tasks) = &cond.else_tasks {
        let task_names: Vec<String> = else_tasks.iter().map(|t| t.name.clone()).collect();
        Ok(json!({ "branch": "else", "tasks": task_names }))
    } else {
        Ok(json!({ "branch": "none_matched" }))
    }
}

fn update_render_context(state: &mut WorkflowRunState) {
    let mut ctx = if let Some(obj) = state.render_context.as_object() {
        obj.clone()
    } else {
        serde_json::Map::new()
    };
    for (name, value) in &state.results {
        let context_value = to_column_oriented(value);
        ctx.insert(name.clone(), context_value);
    }
    state.render_context = Value::Object(ctx);
}

fn build_final_answer(state: &WorkflowRunState) -> String {
    let final_output: Vec<Value> = state
        .workflow
        .tasks
        .iter()
        .filter_map(|t| state.results.get(&t.name))
        .cloned()
        .collect();
    serde_json::to_string(&final_output).unwrap_or_else(|_| "[]".to_string())
}
