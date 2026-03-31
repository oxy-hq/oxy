//! **Clarifying** pipeline stage for the app builder domain.
//!
//! Two sub-phases:
//! - **Triage**: fast LLM pass — no tools, sees only table names.
//! - **Ground**: tool loop — searches catalog and previews data.

use std::sync::Arc;

use agentic_analytics::{Catalog, ConversationTurn, SemanticCatalog};
use agentic_core::{
    HumanInputQuestion,
    back_target::BackTarget,
    human_input::SuspendedRunData,
    orchestrator::{RunContext, SessionMemory, StateHandler, TransitionResult},
    solver::DomainSolver,
    state::ProblemState,
};
use agentic_llm::{InitialMessages, LlmError, ThinkingConfig, ToolLoopConfig};

use crate::events::AppBuilderEvent;
use crate::schemas::triage_response_schema;
use crate::tools::{clarifying_tools, execute_clarifying_tool_with_connector};
use crate::types::{AppBuilderDomain, AppBuilderError, AppIntent};

use super::{
    prompts::{
        CLARIFYING_GROUND_SYSTEM_PROMPT, CLARIFYING_TRIAGE_SYSTEM_PROMPT, format_history_section,
    },
    solver::AppBuilderSolver,
};

// ---------------------------------------------------------------------------
// Prompt builders
// ---------------------------------------------------------------------------

fn build_triage_user_prompt(intent: &AppIntent, catalog: &SemanticCatalog) -> String {
    let history = format_history_section(&intent.history);
    let table_names = Catalog::table_names(catalog);
    let tables_line = if table_names.is_empty() {
        "(no tables available)".to_string()
    } else {
        table_names.join(", ")
    };
    let schema_summary = catalog.to_table_summary();
    format!(
        "{history}Request: {request}\n\nAvailable tables: {tables_line}\n\n\
         Schema summary:\n{schema_summary}\n\n\
         Identify the app name, metrics, controls, tables, and any ambiguities.",
        request = intent.raw_request,
    )
}

fn build_ground_user_prompt(intent: &AppIntent, catalog: &SemanticCatalog) -> String {
    let history = format_history_section(&intent.history);
    let app_name = intent.app_name.as_deref().unwrap_or("(unknown)");
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
    let schema_summary = catalog.to_table_summary();
    format!(
        "{history}Request: {request}\n\n\
         Triage summary:\n\
         - App name: {app_name}\n\
         - Desired metrics: {metrics}\n\
         - Desired controls: {controls}\n\
         - Mentioned tables: {tables}\n\n\
         Available tables:\n{schema_summary}\n\n\
         Use the available tools to explore the catalog and confirm the data sources, \
         then return a grounded intent.",
        request = intent.raw_request,
    )
}

// ---------------------------------------------------------------------------
// Triage response type
// ---------------------------------------------------------------------------

#[derive(serde::Deserialize, Debug)]
struct TriageResult {
    app_name: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    desired_metrics: Vec<String>,
    #[serde(default)]
    desired_controls: Vec<String>,
    #[serde(default)]
    mentioned_tables: Vec<String>,
    #[serde(default)]
    ambiguities: Vec<String>,
    #[serde(default)]
    key_findings: Vec<String>,
}

// ---------------------------------------------------------------------------
// clarify_impl
// ---------------------------------------------------------------------------

impl AppBuilderSolver {
    /// Clarify the user's app-building request using triage + ground sub-phases.
    ///
    /// On the resume path (when `resume_data` is set), triage is skipped: the
    /// previously stored intent is reconstructed from `stage_data`, the user's
    /// answer is appended to the conversation history, and only the ground phase
    /// runs so the LLM can incorporate the clarification.
    pub(crate) async fn clarify_impl(
        &mut self,
        mut intent: AppIntent,
    ) -> Result<AppIntent, (AppBuilderError, BackTarget<AppBuilderDomain>)> {
        // ── Resume path: skip triage, inject user answer ─────────────────────
        if let Some(resume) = self.resume_data.take() {
            if matches!(
                resume.data.stage_data["suspension_type"].as_str(),
                Some("max_tool_rounds" | "max_tokens")
            ) {
                // Resume from a max_tool_rounds suspension in the ground phase.
                // Reconstruct intent from stage_data, put resume_data back so
                // clarify_ground_phase can detect it and use InitialMessages::Messages.
                let intent_val = resume.data.stage_data["intent"].clone();
                intent = serde_json::from_value(intent_val).unwrap_or_else(|_| AppIntent {
                    raw_request: resume.data.original_input.clone(),
                    ..Default::default()
                });
                self.resume_data = Some(resume);
                return self.clarify_ground_phase(intent).await;
            } else {
                // Resume from a triage ambiguity suspension — push user answer and
                // run the ground phase fresh.
                let intent_val = resume.data.stage_data["intent"].clone();
                intent = serde_json::from_value(intent_val).unwrap_or_else(|_| AppIntent {
                    raw_request: resume.data.original_input.clone(),
                    ..Default::default()
                });
                intent.history.push(ConversationTurn {
                    question: resume.data.question.clone(),
                    answer: resume.answer.clone(),
                });
                return self.clarify_ground_phase(intent).await;
            }
        }

        // ── Triage ──────────────────────────────────────────────────────────
        let triage_prompt = build_triage_user_prompt(&intent, &self.catalog);
        let triage_system = self.build_system_prompt("clarifying", CLARIFYING_TRIAGE_SYSTEM_PROMPT);
        let thinking = self.thinking_for_state("clarifying", ThinkingConfig::Disabled);

        let triage_output = self
            .client
            .run_with_tools(
                &triage_system,
                InitialMessages::User(triage_prompt),
                &[],
                |name: String, _params| {
                    Box::pin(async move {
                        Err(agentic_core::tools::ToolError::UnknownTool(format!(
                            "no tools in triage: {name}"
                        )))
                    })
                },
                &self.event_tx,
                ToolLoopConfig {
                    max_tool_rounds: 0,
                    state: "clarifying".into(),
                    thinking: thinking.clone(),
                    response_schema: Some(triage_response_schema()),
                    max_tokens_override: self.max_tokens,
                    sub_spec_index: None,
                },
            )
            .await
            .map_err(|e| {
                (
                    AppBuilderError::NeedsUserInput {
                        prompt: format!("LLM triage failed: {e}"),
                    },
                    BackTarget::Clarify(intent.clone(), Default::default()),
                )
            })?;

        let triage: TriageResult = if let Some(structured) = triage_output.structured_response {
            serde_json::from_value(structured).map_err(|e| {
                (
                    AppBuilderError::NeedsUserInput {
                        prompt: format!("failed to parse triage response: {e}"),
                    },
                    BackTarget::Clarify(intent.clone(), Default::default()),
                )
            })?
        } else {
            let raw = crate::solver::strip_json_fences(&triage_output.text).to_owned();
            serde_json::from_str(&raw).map_err(|e| {
                (
                    AppBuilderError::NeedsUserInput {
                        prompt: format!(
                            "failed to parse triage text: {e}\nRaw: {}",
                            triage_output.text
                        ),
                    },
                    BackTarget::Clarify(intent.clone(), Default::default()),
                )
            })?
        };

        // If there are ambiguities, suspend for HITL.
        if !triage.ambiguities.is_empty() {
            let prompt = triage.ambiguities.join("; ");
            // Store intent with triage results so the resume path can skip triage.
            let intent_with_triage = AppIntent {
                raw_request: intent.raw_request.clone(),
                app_name: Some(triage.app_name.clone()),
                desired_metrics: triage.desired_metrics.clone(),
                desired_controls: triage.desired_controls.clone(),
                mentioned_tables: triage.mentioned_tables.clone(),
                history: intent.history.clone(),
                ..Default::default()
            };
            self.store_suspension_data(agentic_core::human_input::SuspendedRunData {
                from_state: "clarifying".to_string(),
                original_input: intent.raw_request.clone(),
                trace_id: String::new(),
                stage_data: serde_json::json!({
                    "intent": serde_json::to_value(&intent_with_triage)
                        .unwrap_or(serde_json::json!({}))
                }),
                question: prompt.clone(),
                suggestions: vec![],
            });
            return Err((
                AppBuilderError::NeedsUserInput {
                    prompt: prompt.clone(),
                },
                BackTarget::Suspend {
                    questions: triage
                        .ambiguities
                        .iter()
                        .map(|a| agentic_core::HumanInputQuestion {
                            prompt: a.clone(),
                            suggestions: vec![],
                        })
                        .collect(),
                },
            ));
        }

        // Update intent with triage results.
        intent.app_name = Some(triage.app_name.clone());
        intent.desired_metrics = triage.desired_metrics.clone();
        intent.desired_controls = triage.desired_controls.clone();
        intent.mentioned_tables = triage.mentioned_tables.clone();

        self.clarify_ground_phase(intent).await
    }

    /// Run the ground phase of clarification (catalog exploration + intent finalisation).
    ///
    /// Called both from the normal triage → ground path and directly from the
    /// resume path (when triage is skipped after a HITL clarification).
    pub(crate) async fn clarify_ground_phase(
        &mut self,
        intent: AppIntent,
    ) -> Result<AppIntent, (AppBuilderError, BackTarget<AppBuilderDomain>)> {
        let connector = self
            .connectors
            .get(&self.default_connector)
            .cloned()
            .expect("default connector must be registered");
        let catalog_arc = Arc::clone(&self.catalog);
        let tools = clarifying_tools();

        let ground_system = self.build_system_prompt("clarifying", CLARIFYING_GROUND_SYSTEM_PROMPT);
        let thinking_ground = self.thinking_for_state("clarifying", ThinkingConfig::Disabled);

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
            let ground_prompt = build_ground_user_prompt(&intent, &self.catalog);
            InitialMessages::User(ground_prompt)
        };
        let max_rounds = self.max_tool_rounds_for_state("clarifying", 5) + resume_extra_rounds;

        let ground_output = match self
            .client
            .run_with_tools(
                &ground_system,
                initial,
                &tools,
                move |name: String, params| {
                    let cat = Arc::clone(&catalog_arc);
                    let conn = Arc::clone(&connector);
                    Box::pin(async move {
                        execute_clarifying_tool_with_connector(&name, params, &cat, &*conn).await
                    })
                },
                &self.event_tx,
                ToolLoopConfig {
                    max_tool_rounds: max_rounds,
                    state: "clarifying".into(),
                    thinking: thinking_ground,
                    response_schema: Some(triage_response_schema()),
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
                    "The agent used all {rounds} allotted tool rounds during clarification. \
                     Continue with more rounds?"
                );
                let intent_value = serde_json::to_value(&intent).unwrap_or_default();
                self.store_suspension_data(SuspendedRunData {
                    from_state: "clarifying".to_string(),
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
                     clarification. Continue with double the budget ({doubled} tokens)?"
                );
                let intent_value = serde_json::to_value(&intent).unwrap_or_default();
                self.store_suspension_data(SuspendedRunData {
                    from_state: "clarifying".to_string(),
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
                        prompt: format!("LLM ground pass failed: {e}"),
                    },
                    BackTarget::Clarify(intent.clone(), Default::default()),
                ));
            }
        };

        // Parse the grounded result (same schema as triage).
        let grounded: TriageResult = if let Some(structured) = ground_output.structured_response {
            serde_json::from_value(structured).map_err(|e| {
                (
                    AppBuilderError::NeedsUserInput {
                        prompt: format!("failed to parse ground response: {e}"),
                    },
                    BackTarget::Clarify(intent.clone(), Default::default()),
                )
            })?
        } else {
            let raw = crate::solver::strip_json_fences(&ground_output.text).to_owned();
            serde_json::from_str(&raw).map_err(|e| {
                (
                    AppBuilderError::NeedsUserInput {
                        prompt: format!(
                            "failed to parse ground text: {e}\nRaw: {}",
                            ground_output.text
                        ),
                    },
                    BackTarget::Clarify(intent.clone(), Default::default()),
                )
            })?
        };

        // Merge grounded data back into intent.
        let mut intent = intent;
        intent.app_name = Some(grounded.app_name);
        intent.desired_metrics = grounded.desired_metrics;
        intent.desired_controls = grounded.desired_controls;
        intent.mentioned_tables = grounded.mentioned_tables;
        intent.key_findings = grounded.key_findings;

        Ok(intent)
    }
}

// ---------------------------------------------------------------------------
// State handler
// ---------------------------------------------------------------------------

/// Build the `StateHandler` for the **clarifying** state.
pub(super) fn build_clarifying_handler()
-> StateHandler<AppBuilderDomain, AppBuilderSolver, AppBuilderEvent> {
    StateHandler {
        next: "specifying",
        execute: Arc::new(
            |solver: &mut AppBuilderSolver,
             state,
             _events,
             _run_ctx: &RunContext<AppBuilderDomain>,
             _memory: &SessionMemory<AppBuilderDomain>| {
                Box::pin(async move {
                    let intent = match state {
                        ProblemState::Clarifying(i) => i,
                        _ => unreachable!("clarifying handler called with wrong state"),
                    };
                    match solver.clarify_impl(intent).await {
                        Ok(clarified) => TransitionResult::ok(ProblemState::Specifying(clarified)),
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
