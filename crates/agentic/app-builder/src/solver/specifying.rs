//! **Specifying** pipeline stage for the app builder domain.
//!
//! Produces an [`AppSpec`] from the clarified [`AppIntent`] via an LLM
//! tool-loop that explores the catalog schema before finalising the spec.

use std::sync::Arc;

use agentic_analytics::SemanticCatalog;
use agentic_core::{
    back_target::BackTarget,
    human_input::SuspendedRunData,
    orchestrator::{RunContext, SessionMemory, StateHandler, TransitionResult},
    solver::DomainSolver,
    state::ProblemState,
    HumanInputQuestion,
};
use agentic_llm::{InitialMessages, LlmError, ThinkingConfig, ToolLoopConfig};

use std::collections::HashSet;

use crate::events::AppBuilderEvent;
use crate::schemas::specify_response_schema;
use crate::tools::{execute_specifying_tool, specifying_tools};
use crate::types::{
    AppBuilderDomain, AppBuilderError, AppIntent, AppSpec, ControlPlan, ControlType, LayoutNode,
    TaskPlan,
};

use super::{
    prompts::{format_history_section, APP_FORMAT_EXAMPLE, SPECIFYING_SYSTEM_PROMPT},
    solver::AppBuilderSolver,
};

// ---------------------------------------------------------------------------
// Prompt builder
// ---------------------------------------------------------------------------

fn build_specify_user_prompt(intent: &AppIntent, catalog: &SemanticCatalog) -> String {
    let history = format_history_section(&intent.history);

    let schema_summary = catalog.to_table_summary();

    let metrics = if intent.desired_metrics.is_empty() {
        "(none identified)".to_string()
    } else {
        intent.desired_metrics.join(", ")
    };
    let controls = if intent.desired_controls.is_empty() {
        "(none identified)".to_string()
    } else {
        intent.desired_controls.join(", ")
    };
    let tables = if intent.mentioned_tables.is_empty() {
        "(none identified)".to_string()
    } else {
        intent.mentioned_tables.join(", ")
    };

    let findings_section = if intent.key_findings.is_empty() {
        String::new()
    } else {
        format!(
            "Key findings from grounding:\n{}\n\n",
            intent
                .key_findings
                .iter()
                .map(|f| format!("- {f}"))
                .collect::<Vec<_>>()
                .join("\n")
        )
    };

    format!(
        "{history}Request: {request}\n\
         App name: {app_name}\n\
         Desired metrics: {metrics}\n\
         Desired controls: {controls}\n\
         Mentioned tables: {tables}\n\n\
         {findings_section}\
         Schema:\n{schema_summary}\n\n\
         Use tools only if you need column values or ranges not covered by the schema or findings above, \
         then produce a complete app specification including tasks, controls, and layout.",
        request = intent.raw_request,
        app_name = intent.app_name.as_deref().unwrap_or("(unknown)"),
    )
}

// ---------------------------------------------------------------------------
// Response parsing helpers
// ---------------------------------------------------------------------------

/// Parse a `control_type` string into [`ControlType`].
fn parse_control_type(s: &str) -> ControlType {
    match s.to_lowercase().as_str() {
        "date" => ControlType::Date,
        "toggle" => ControlType::Toggle,
        _ => ControlType::Select,
    }
}

/// Convert the structured LLM response into an [`AppSpec`].
fn parse_spec_from_value(
    value: serde_json::Value,
    intent: &AppIntent,
    connector_name: &str,
) -> Result<AppSpec, String> {
    let app_name = value["app_name"]
        .as_str()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| intent.app_name.as_deref().unwrap_or("My App"))
        .to_string();
    let description = value["description"]
        .as_str()
        .unwrap_or(&intent.raw_request)
        .to_string();

    // Tasks
    let tasks: Vec<TaskPlan> = value["tasks"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|t| {
                    let name = t["name"].as_str()?.to_string();
                    let description = t["description"].as_str().unwrap_or("").to_string();
                    let control_deps: Vec<String> = t["control_deps"]
                        .as_array()
                        .map(|a| {
                            a.iter()
                                .filter_map(|v| v.as_str().map(String::from))
                                .collect()
                        })
                        .unwrap_or_default();
                    let is_control_source = t["is_control_source"].as_bool().unwrap_or(false);
                    Some(TaskPlan {
                        name,
                        description,
                        control_deps,
                        is_control_source,
                        ..Default::default()
                    })
                })
                .collect()
        })
        .unwrap_or_default();

    // Controls
    let controls: Vec<ControlPlan> = value["controls"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|c| {
                    let name = c["name"].as_str()?.to_string();
                    let label = c["label"].as_str().unwrap_or(&name).to_string();
                    let control_type =
                        parse_control_type(c["control_type"].as_str().unwrap_or("select"));
                    let source_task = c["source_task"].as_str().map(String::from);
                    let options: Vec<String> = c["options"]
                        .as_array()
                        .map(|a| {
                            a.iter()
                                .filter_map(|v| v.as_str().map(String::from))
                                .collect()
                        })
                        .unwrap_or_default();
                    let default = c["default"].as_str().unwrap_or("").to_string();
                    Some(ControlPlan {
                        name,
                        label,
                        control_type,
                        source_task,
                        options,
                        default,
                    })
                })
                .collect()
        })
        .unwrap_or_default();

    // Layout — derive a simple default from tasks/controls if not provided
    let layout: Vec<LayoutNode> = {
        let raw_layout = value["layout"].as_array();
        if let Some(arr) = raw_layout.filter(|a| !a.is_empty()) {
            arr.iter()
                .filter_map(|node| serde_json::from_value(node.clone()).ok())
                .collect()
        } else {
            // Fall back: insight summary + one chart per display task
            let mut nodes: Vec<LayoutNode> = Vec::new();
            let display_tasks: Vec<&TaskPlan> =
                tasks.iter().filter(|t| !t.is_control_source).collect();
            // Add an insight node referencing all display tasks for a data summary.
            if !display_tasks.is_empty() {
                nodes.push(LayoutNode::Insight {
                    tasks: display_tasks.iter().map(|t| t.name.clone()).collect(),
                    focus: Some("highlights".into()),
                });
            }
            for task in &display_tasks {
                nodes.push(LayoutNode::Chart {
                    task: task.name.clone(),
                    preferred: crate::types::ChartPreference::Auto,
                });
            }
            nodes
        }
    };

    Ok(AppSpec {
        intent: intent.clone(),
        app_name,
        description,
        tasks,
        controls,
        layout,
        connector_name: connector_name.to_string(),
    })
}

// ---------------------------------------------------------------------------
// Spec validation
// ---------------------------------------------------------------------------

/// Collect layout node references to task names.
fn collect_layout_refs(nodes: &[LayoutNode], task_refs: &mut Vec<String>) {
    for node in nodes {
        match node {
            LayoutNode::Chart { task, .. } | LayoutNode::Table { task, .. } => {
                task_refs.push(task.clone());
            }
            LayoutNode::Row { children, .. } => {
                collect_layout_refs(children, task_refs);
            }
            LayoutNode::Markdown { .. } => {}
            LayoutNode::Insight { tasks, .. } => {
                task_refs.extend(tasks.iter().cloned());
            }
        }
    }
}

/// Validate internal consistency of an [`AppSpec`]. Returns a list of violation
/// messages (empty = valid).
fn validate_spec(spec: &AppSpec) -> Vec<String> {
    let mut errors = Vec::new();

    let task_names: HashSet<&str> = spec.tasks.iter().map(|t| t.name.as_str()).collect();
    let control_names: HashSet<&str> = spec.controls.iter().map(|c| c.name.as_str()).collect();

    // 1. Task name uniqueness.
    {
        let mut seen = HashSet::new();
        for t in &spec.tasks {
            if !seen.insert(&t.name) {
                errors.push(format!("duplicate task name: '{}'", t.name));
            }
        }
    }

    // 2. Control name uniqueness.
    {
        let mut seen = HashSet::new();
        for c in &spec.controls {
            if !seen.insert(&c.name) {
                errors.push(format!("duplicate control name: '{}'", c.name));
            }
        }
    }

    // 3. At least one display task.
    if !spec.tasks.iter().any(|t| !t.is_control_source) {
        errors.push("spec has no display tasks (all tasks are control-source)".into());
    }

    // 4. Layout references valid tasks.
    let mut layout_task_refs = Vec::new();
    collect_layout_refs(&spec.layout, &mut layout_task_refs);

    for name in &layout_task_refs {
        if !task_names.contains(name.as_str()) {
            errors.push(format!("layout references non-existent task: '{name}'"));
        }
    }

    // 6. Control source_task references valid control-source tasks.
    for ctrl in &spec.controls {
        if let Some(ref src) = ctrl.source_task {
            match spec.tasks.iter().find(|t| &t.name == src) {
                None => errors.push(format!(
                    "control '{}' references non-existent source_task: '{src}'",
                    ctrl.name
                )),
                Some(t) if !t.is_control_source => errors.push(format!(
                    "control '{}' source_task '{src}' is not marked as control-source",
                    ctrl.name
                )),
                _ => {}
            }
        }
    }

    // 7. Task control_deps reference valid controls.
    for task in &spec.tasks {
        for dep in &task.control_deps {
            if !control_names.contains(dep.as_str()) {
                errors.push(format!(
                    "task '{}' references non-existent control dependency: '{dep}'",
                    task.name
                ));
            }
        }
    }

    // 8. Select controls must have either source_task or non-empty options.
    for ctrl in &spec.controls {
        if matches!(ctrl.control_type, ControlType::Select)
            && ctrl.source_task.is_none()
            && ctrl.options.is_empty()
        {
            errors.push(format!(
                "select control '{}' has no source_task and no static options",
                ctrl.name
            ));
        }
    }

    // 9. Select control default must be covered by source or options.
    //    If the control has static options, the default must be in the list.
    for ctrl in &spec.controls {
        if matches!(ctrl.control_type, ControlType::Select)
            && !ctrl.options.is_empty()
            && !ctrl.default.is_empty()
            && !ctrl.options.iter().any(|o| o == &ctrl.default)
        {
            errors.push(format!(
                "select control '{}' default '{}' is not in its options list",
                ctrl.name, ctrl.default
            ));
        }
    }

    errors
}

// ---------------------------------------------------------------------------
// specify_impl
// ---------------------------------------------------------------------------

impl AppBuilderSolver {
    /// Produce an [`AppSpec`] from the clarified intent via an LLM tool-loop.
    pub(crate) async fn specify_impl(
        &mut self,
        intent: AppIntent,
    ) -> Result<AppSpec, (AppBuilderError, BackTarget<AppBuilderDomain>)> {
        let connector_name = self.default_connector.clone();
        let connector = self
            .connectors
            .get(&connector_name)
            .cloned()
            .expect("default connector must be registered");
        let catalog_arc = Arc::clone(&self.catalog);

        let user_prompt = build_specify_user_prompt(&intent, &self.catalog);
        let base_prompt = format!("{SPECIFYING_SYSTEM_PROMPT}\n{APP_FORMAT_EXAMPLE}");
        let system_prompt = self.build_system_prompt("specifying", &base_prompt);
        let thinking = self.thinking_for_state("specifying", ThinkingConfig::Disabled);
        let base_rounds = self.max_tool_rounds_for_state("specifying", 8);
        let tools = specifying_tools();

        // On resume, rebuild the full message history from the persisted
        // conversation snapshot and append a "please continue" message.
        let mut resume_extra_rounds: u32 = 0;
        let mut resume_max_tokens_override: Option<u32> = None;
        let initial = if let Some(resume) = self.resume_data.take() {
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
            InitialMessages::Messages(agentic_llm::LlmClient::build_continue_messages(&prior))
        } else {
            InitialMessages::User(user_prompt)
        };
        let max_rounds = base_rounds + resume_extra_rounds;

        let output = match self
            .client
            .run_with_tools(
                &system_prompt,
                initial,
                &tools,
                move |name: String, params| {
                    let cat = Arc::clone(&catalog_arc);
                    let conn = Arc::clone(&connector);
                    Box::pin(
                        async move { execute_specifying_tool(&name, params, &*cat, &*conn).await },
                    )
                },
                &self.event_tx,
                ToolLoopConfig {
                    max_tool_rounds: max_rounds,
                    state: "specifying".into(),
                    thinking,
                    response_schema: Some(specify_response_schema()),
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
                    "The agent used all {rounds} allotted tool rounds during specifying. \
                     Continue with more rounds?"
                );
                let intent_value = serde_json::to_value(&intent).unwrap_or_default();
                self.store_suspension_data(SuspendedRunData {
                    from_state: "specifying".to_string(),
                    original_input: intent.raw_request.clone(),
                    trace_id: String::new(),
                    stage_data: serde_json::json!({
                        "intent": intent_value,
                        "conversation_history": prior_messages,
                        "suspension_type": "max_tool_rounds",
                        "extra_rounds": rounds,
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
                    "The model ran out of token budget ({current_max_tokens} tokens) during \
                     specifying. Continue with double the budget ({doubled} tokens)?"
                );
                let intent_value = serde_json::to_value(&intent).unwrap_or_default();
                self.store_suspension_data(SuspendedRunData {
                    from_state: "specifying".to_string(),
                    original_input: intent.raw_request.clone(),
                    trace_id: String::new(),
                    stage_data: serde_json::json!({
                        "intent": intent_value,
                        "conversation_history": prior_messages,
                        "suspension_type": "max_tokens",
                        "max_tokens_override": doubled,
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
                        prompt: format!("LLM specifying failed: {e}"),
                    },
                    BackTarget::Specify(intent.clone(), Default::default()),
                ));
            }
        };

        // Parse structured response.
        let spec_value = if let Some(structured) = output.structured_response {
            structured
        } else {
            let raw = crate::solver::strip_json_fences(&output.text).to_owned();
            serde_json::from_str(&raw).map_err(|e| {
                (
                    AppBuilderError::NeedsUserInput {
                        prompt: format!("failed to parse spec text: {e}\nRaw: {}", output.text),
                    },
                    BackTarget::Specify(intent.clone(), Default::default()),
                )
            })?
        };

        let spec = parse_spec_from_value(spec_value, &intent, &connector_name).map_err(|e| {
            (
                AppBuilderError::NeedsUserInput {
                    prompt: format!("failed to build AppSpec: {e}"),
                },
                BackTarget::Specify(intent.clone(), Default::default()),
            )
        })?;

        // Validate internal consistency.
        let violations = validate_spec(&spec);
        if !violations.is_empty() {
            return Err((
                AppBuilderError::InvalidSpec { errors: violations },
                BackTarget::Specify(intent, Default::default()),
            ));
        }

        // Emit updated event with actual task count.
        if let Some(tx) = &self.event_tx {
            let _ = tx
                .send(agentic_core::events::Event::Domain(
                    AppBuilderEvent::TaskPlanReady {
                        task_count: spec.tasks.len(),
                        control_count: spec.controls.len(),
                        spec: spec.clone(),
                    },
                ))
                .await;
        }

        Ok(spec)
    }
}

// ---------------------------------------------------------------------------
// State handler
// ---------------------------------------------------------------------------

/// Build the `StateHandler` for the **specifying** state.
pub(super) fn build_specifying_handler(
) -> StateHandler<AppBuilderDomain, AppBuilderSolver, AppBuilderEvent> {
    StateHandler {
        next: "solving",
        execute: Arc::new(
            |solver: &mut AppBuilderSolver,
             state,
             _events,
             _run_ctx: &RunContext<AppBuilderDomain>,
             _memory: &SessionMemory<AppBuilderDomain>| {
                Box::pin(async move {
                    let intent = match state {
                        ProblemState::Specifying(i) => i,
                        _ => unreachable!("specifying handler called with wrong state"),
                    };
                    let fallback_intent = intent.clone();
                    match solver.specify_impl(intent).await {
                        Ok(spec) => {
                            // Fan out: one sub-spec per task so the orchestrator
                            // tracks solve→execute for each task independently.
                            let per_task_specs: Vec<AppSpec> = spec
                                .tasks
                                .iter()
                                .map(|task| AppSpec {
                                    intent: spec.intent.clone(),
                                    app_name: spec.app_name.clone(),
                                    description: spec.description.clone(),
                                    tasks: vec![task.clone()],
                                    controls: spec.controls.clone(),
                                    layout: spec.layout.clone(),
                                    connector_name: spec.connector_name.clone(),
                                })
                                .collect();

                            if per_task_specs.len() == 1 {
                                TransitionResult::ok(ProblemState::Solving(
                                    per_task_specs.into_iter().next().unwrap(),
                                ))
                            } else {
                                TransitionResult::pending_fan_out(
                                    per_task_specs,
                                    ProblemState::Specifying(fallback_intent),
                                )
                            }
                        }
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

// ── Unit tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{ChartPreference, ResultShape};

    fn make_intent() -> AppIntent {
        AppIntent {
            raw_request: "test".into(),
            app_name: Some("Test App".into()),
            ..Default::default()
        }
    }

    fn make_valid_spec() -> AppSpec {
        AppSpec {
            intent: make_intent(),
            app_name: "Test".into(),
            description: "test".into(),
            tasks: vec![
                TaskPlan {
                    name: "revenue".into(),
                    description: "".into(),
                    expected_shape: ResultShape::TimeSeries,
                    expected_columns: vec![],
                    control_deps: vec!["store".into()],
                    is_control_source: false,
                },
                TaskPlan {
                    name: "stores_list".into(),
                    description: "".into(),
                    expected_shape: ResultShape::Series,
                    expected_columns: vec![],
                    control_deps: vec![],
                    is_control_source: true,
                },
            ],
            controls: vec![ControlPlan {
                name: "store".into(),
                label: "Store".into(),
                control_type: ControlType::Select,
                source_task: Some("stores_list".into()),
                options: vec![],
                default: "All".into(),
            }],
            layout: vec![LayoutNode::Chart {
                task: "revenue".into(),
                preferred: ChartPreference::Auto,
            }],
            connector_name: "db".into(),
        }
    }

    #[test]
    fn test_validate_spec_valid() {
        let spec = make_valid_spec();
        assert!(validate_spec(&spec).is_empty());
    }

    #[test]
    fn test_validate_spec_duplicate_task_names() {
        let mut spec = make_valid_spec();
        spec.tasks.push(spec.tasks[0].clone());
        let errors = validate_spec(&spec);
        assert!(errors.iter().any(|e| e.contains("duplicate task name")));
    }

    #[test]
    fn test_validate_spec_duplicate_control_names() {
        let mut spec = make_valid_spec();
        spec.controls.push(spec.controls[0].clone());
        let errors = validate_spec(&spec);
        assert!(errors.iter().any(|e| e.contains("duplicate control name")));
    }

    #[test]
    fn test_validate_spec_no_display_tasks() {
        let mut spec = make_valid_spec();
        for t in &mut spec.tasks {
            t.is_control_source = true;
        }
        let errors = validate_spec(&spec);
        assert!(errors.iter().any(|e| e.contains("no display tasks")));
    }

    #[test]
    fn test_validate_spec_layout_dangling_task_ref() {
        let mut spec = make_valid_spec();
        spec.layout.push(LayoutNode::Chart {
            task: "nonexistent".into(),
            preferred: ChartPreference::Auto,
        });
        let errors = validate_spec(&spec);
        assert!(errors
            .iter()
            .any(|e| e.contains("non-existent task: 'nonexistent'")));
    }

    #[test]
    fn test_validate_spec_bad_source_task() {
        let mut spec = make_valid_spec();
        spec.controls[0].source_task = Some("revenue".into()); // not a control-source task
        let errors = validate_spec(&spec);
        assert!(errors
            .iter()
            .any(|e| e.contains("not marked as control-source")));
    }

    #[test]
    fn test_validate_spec_bad_control_dep() {
        let mut spec = make_valid_spec();
        spec.tasks[0].control_deps = vec!["nonexistent_ctrl".into()];
        let errors = validate_spec(&spec);
        assert!(errors
            .iter()
            .any(|e| e.contains("non-existent control dependency")));
    }

    #[test]
    fn test_validate_spec_select_without_source_or_options() {
        let mut spec = make_valid_spec();
        spec.controls[0].source_task = None;
        spec.controls[0].options = vec![];
        let errors = validate_spec(&spec);
        assert!(errors
            .iter()
            .any(|e| e.contains("no source_task and no static options")));
    }

    #[test]
    fn test_validate_spec_select_with_static_options_valid() {
        let mut spec = make_valid_spec();
        spec.controls[0].source_task = None;
        spec.controls[0].options = vec!["All".into(), "Store A".into(), "Store B".into()];
        spec.controls[0].default = "All".into();
        let errors = validate_spec(&spec);
        assert!(errors.is_empty(), "expected no errors, got: {errors:?}");
    }

    #[test]
    fn test_validate_spec_select_default_not_in_options() {
        let mut spec = make_valid_spec();
        spec.controls[0].source_task = None;
        spec.controls[0].options = vec!["Store A".into(), "Store B".into()];
        spec.controls[0].default = "All".into();
        let errors = validate_spec(&spec);
        assert!(errors
            .iter()
            .any(|e| e.contains("default 'All' is not in its options list")));
    }

    // ── Insight validation tests ────────────────────────────────────────

    #[test]
    fn test_validate_spec_insight_references_valid_tasks() {
        let mut spec = make_valid_spec();
        spec.layout.push(LayoutNode::Insight {
            tasks: vec!["revenue".into()],
            focus: Some("trends".into()),
        });
        let errors = validate_spec(&spec);
        assert!(errors.is_empty(), "expected no errors, got: {errors:?}");
    }

    #[test]
    fn test_validate_spec_insight_dangling_task_ref() {
        let mut spec = make_valid_spec();
        spec.layout.push(LayoutNode::Insight {
            tasks: vec!["nonexistent".into()],
            focus: None,
        });
        let errors = validate_spec(&spec);
        assert!(errors
            .iter()
            .any(|e| e.contains("non-existent task: 'nonexistent'")));
    }
}
