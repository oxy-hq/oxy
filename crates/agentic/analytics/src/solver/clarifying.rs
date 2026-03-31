//! **Clarifying** pipeline stage.
//!
//! Owns:
//! - Prompt builders for the Triage and Ground sub-phases
//! - [`AnalyticsSolver::triage_impl`] — triage sub-phase (no tools)
//! - [`AnalyticsSolver::ground_impl`] — ground sub-phase (tool loop)
//! - [`AnalyticsSolver::clarify_impl`] — orchestrates triage + ground
//! - [`AnalyticsSolver::general_inquiry_impl`] — GeneralInquiry short-circuit
//! - [`build_clarifying_handler`] — `StateHandler` factory

use std::sync::Arc;

use agentic_core::{
    HumanInputQuestion,
    back_target::{BackTarget, RetryContext},
    human_input::SuspendedRunData,
    orchestrator::{CompletedTurn, RunContext, SessionMemory, StateHandler, TransitionResult},
    solver::DomainSolver,
    state::ProblemState,
    tools::ToolError,
};

use crate::catalog::Catalog;
use crate::events::AnalyticsEvent;
use crate::llm::{ThinkingConfig, ToolLoopConfig};
use crate::schemas::{clarify_response_schema, triage_response_schema};
use crate::semantic::SemanticCatalog;
use crate::tools::{execute_clarifying_tool, execute_database_lookup_tool};
use crate::types::{DomainHypothesis, QuestionType};
use crate::{AnalyticsAnswer, AnalyticsDomain, AnalyticsError, AnalyticsIntent};

use super::{
    AnalyticsSolver, emit_domain,
    prompts::{
        GENERAL_INQUIRY_SYSTEM_PROMPT, GROUND_SYSTEM_PROMPT, TRIAGE_SYSTEM_PROMPT,
        format_history_section, format_retry_section, format_session_turns_section,
    },
    resuming::{ask_user_tool_def, handle_ask_user},
    strip_json_fences,
};

// ---------------------------------------------------------------------------
// Prompt builders
// ---------------------------------------------------------------------------

pub(super) fn build_triage_user_prompt(
    intent: &AnalyticsIntent,
    catalog: &SemanticCatalog,
    session_turns: &[CompletedTurn<AnalyticsDomain>],
) -> String {
    let session_section = format_session_turns_section(session_turns);
    let history_section = format_history_section(&intent.history);
    let table_names = Catalog::table_names(catalog);
    let tables_line = if table_names.is_empty() {
        "(no tables)".to_string()
    } else {
        table_names.join(", ")
    };
    format!(
        "{session_section}{history_section}Question: {raw_question}\n\nAvailable tables: {tables_line}\n\nIdentify the topic, question type, relevant tables, and your confidence.",
        raw_question = intent.raw_question,
    )
}

pub(super) fn build_ground_user_prompt(
    intent: &AnalyticsIntent,
    hypothesis: &DomainHypothesis,
    catalog: &SemanticCatalog,
    retry_ctx: Option<&RetryContext>,
    session_turns: &[CompletedTurn<AnalyticsDomain>],
) -> String {
    let history_section = format_history_section(&intent.history);
    let retry_section = format_retry_section(retry_ctx);
    let session_section = format_session_turns_section(session_turns);
    let reference_note = if !session_turns.is_empty() {
        " If the current question references something from the conversation \
(e.g. \"same metric\", \"break it down differently\", \"how about X instead\"), \
resolve those references using the previous turns."
    } else {
        ""
    };
    let time_scope_line = hypothesis
        .time_scope
        .as_deref()
        .map(|ts| format!("\nInferred time scope: {ts}"))
        .unwrap_or_default();
    format!(
        "{session_section}{history_section}Question: {raw_question}\n\n\
         Triage summary: {summary}\n\
         Question type: {qt:?}\n\
         Relevant tables: {tables}{time_scope_line}\n\n\
         Available tables:\n{schema}\n\n\
         Use the tools to explore metrics and dimensions, then return the structured intent.{reference_note}{retry_section}",
        raw_question = intent.raw_question,
        summary = hypothesis.summary,
        qt = hypothesis.question_type,
        tables = hypothesis.relevant_tables.join(", "),
        schema = catalog.to_table_summary(),
    )
}

// ---------------------------------------------------------------------------
// Solver impl methods
// ---------------------------------------------------------------------------

impl AnalyticsSolver {
    /// **Triage** sub-phase: identify topic, question type, and relevant tables.
    pub(super) async fn triage_impl(
        &mut self,
        intent: &AnalyticsIntent,
        session_turns: &[CompletedTurn<AnalyticsDomain>],
    ) -> Result<DomainHypothesis, (AnalyticsError, BackTarget<AnalyticsDomain>)> {
        let user_prompt = build_triage_user_prompt(intent, &self.catalog, session_turns);
        let system_prompt = self.build_system_prompt("clarifying", TRIAGE_SYSTEM_PROMPT, None);
        let thinking = self.thinking_for_state("clarifying", ThinkingConfig::Disabled);

        let output = self
            .client
            .run_with_tools(
                &system_prompt,
                &user_prompt,
                &[],
                |_name: String, _params| {
                    Box::pin(async { Err(ToolError::UnknownTool("no tools in triage".into())) })
                },
                &self.event_tx,
                ToolLoopConfig {
                    max_tool_rounds: 0,
                    state: "clarifying".into(),
                    thinking,
                    response_schema: Some(triage_response_schema()),
                    max_tokens_override: self.max_tokens,
                    sub_spec_index: None,
                },
            )
            .await
            .map_err(|e| {
                let msg = format!("LLM call failed during triage: {e}");
                (
                    AnalyticsError::NeedsUserInput { prompt: msg },
                    BackTarget::Clarify(intent.clone(), Default::default()),
                )
            })?;

        let hypothesis: DomainHypothesis = if let Some(structured) = output.structured_response {
            serde_json::from_value(structured).map_err(|e| {
                let msg = format!("failed to deserialise triage response: {e}");
                (
                    AnalyticsError::NeedsUserInput { prompt: msg },
                    BackTarget::Clarify(intent.clone(), Default::default()),
                )
            })?
        } else {
            if output.text.trim().is_empty() {
                let msg = "triage: LLM returned empty text".to_string();
                return Err((
                    AnalyticsError::NeedsUserInput { prompt: msg },
                    BackTarget::Clarify(intent.clone(), Default::default()),
                ));
            }
            let raw = strip_json_fences(&output.text).to_owned();
            serde_json::from_str(&raw).map_err(|e| {
                let msg = format!(
                    "failed to parse triage response: {e}\nRaw: {}",
                    &output.text
                );
                (
                    AnalyticsError::NeedsUserInput { prompt: msg },
                    BackTarget::Clarify(intent.clone(), Default::default()),
                )
            })?
        };

        emit_domain(
            &self.event_tx,
            AnalyticsEvent::TriageCompleted {
                summary: hypothesis.summary.clone(),
                relevant_tables: hypothesis.relevant_tables.clone(),
                question_type: format!("{:?}", hypothesis.question_type),
                confidence: hypothesis.confidence,
                ambiguities: hypothesis.ambiguities.clone(),
            },
        )
        .await;

        Ok(hypothesis)
    }

    /// **Ground** sub-phase: explore the schema with tools and produce a
    /// structured [`AnalyticsIntent`].
    pub(super) async fn ground_impl(
        &mut self,
        intent: AnalyticsIntent,
        hypothesis: &DomainHypothesis,
        retry_ctx: Option<&RetryContext>,
        session_turns: &[CompletedTurn<AnalyticsDomain>],
    ) -> Result<AnalyticsIntent, (AnalyticsError, BackTarget<AnalyticsDomain>)> {
        let user_prompt =
            build_ground_user_prompt(&intent, hypothesis, &self.catalog, retry_ctx, session_turns);

        // On resume there are two distinct suspension origins:
        //
        // 1. Pre-ground ambiguity suspension (from `clarify_impl`):
        //    `stage_data` is `{}` (no conversation_history).  The user's
        //    answer disambiguates the original question; ground should start
        //    fresh with the answer appended as extra context.
        //
        // 2. In-ground `ask_user` suspension (from this function):
        //    `stage_data["conversation_history"]` holds the full LLM message
        //    history up to the `ask_user` call.  The user's answer must be
        //    fed back as a tool result so the LLM can continue.
        //
        // Case 1 with an empty prior list would cause `build_resume_messages`
        // to fall back to the synthetic `ask_user_0` id, producing a
        // `tool_result` with no matching `tool_use` block — rejected by the
        // Anthropic API.
        let mut resume_max_tokens_override: Option<u32> = None;
        let mut resume_extra_rounds: u32 = 0;
        let initial: crate::llm::InitialMessages = if let Some(resume) = self.resume_data.take() {
            let prior: Vec<serde_json::Value> = resume.data.stage_data["conversation_history"]
                .as_array()
                .cloned()
                .unwrap_or_default();
            match resume.data.stage_data["suspension_type"].as_str() {
                Some("max_tokens") => {
                    // Resume with doubled token budget; append "please continue".
                    resume_max_tokens_override = resume.data.stage_data["max_tokens_override"]
                        .as_u64()
                        .map(|v| v as u32);
                    crate::llm::InitialMessages::Messages(
                        crate::llm::LlmClient::build_continue_messages(&prior),
                    )
                }
                Some("max_tool_rounds") => {
                    // Resume with extra tool rounds; append "please continue".
                    resume_extra_rounds =
                        resume.data.stage_data["extra_rounds"].as_u64().unwrap_or(0) as u32;
                    crate::llm::InitialMessages::Messages(
                        crate::llm::LlmClient::build_continue_messages(&prior),
                    )
                }
                _ => {
                    // ask_user or legacy pre-ground suspension.
                    if !prior.is_empty() {
                        // Case 2: continue the in-progress tool loop.
                        let msgs = self.client.build_resume_messages(
                            &prior,
                            &resume.data.question,
                            &resume.data.suggestions,
                            &resume.answer,
                        );
                        crate::llm::InitialMessages::Messages(msgs)
                    } else {
                        // Case 1: fresh ground pass; embed the user's clarification.
                        crate::llm::InitialMessages::User(format!(
                            "{user_prompt}\n\nUser answered the clarifying question \"{q}\": {a}",
                            q = resume.data.question,
                            a = resume.answer,
                        ))
                    }
                }
            }
        } else {
            crate::llm::InitialMessages::User(user_prompt)
        };

        let tools = self.tools_for_state_clarifying();
        let system_prompt = self.build_system_prompt("clarifying", GROUND_SYSTEM_PROMPT, None);
        let thinking = self.thinking_for_state("clarifying", ThinkingConfig::Disabled);
        let max_rounds = self.max_tool_rounds_for_state("clarifying", 5) + resume_extra_rounds;
        let catalog = Arc::clone(&self.catalog);
        let human_input = Arc::clone(&self.human_input);
        let procedure_runner = self.procedure_runner.clone();
        let connectors = self.connectors.clone();
        let default_connector = self.default_connector.clone();
        let schema_cache = Arc::clone(&self.schema_cache);
        let output = match self
            .client
            .run_with_tools(
                &system_prompt,
                initial,
                &tools,
                |name: String, params| {
                    let catalog = Arc::clone(&catalog);
                    let human_input = Arc::clone(&human_input);
                    let procedure_runner = procedure_runner.clone();
                    let connectors = connectors.clone();
                    let default_connector = default_connector.clone();
                    let schema_cache = Arc::clone(&schema_cache);
                    Box::pin(async move {
                        // `ask_user` is intercepted here before the generic tool dispatcher.
                        // See resuming.rs module doc for why this asymmetry exists.
                        if name == "ask_user" {
                            handle_ask_user(&params, human_input.as_ref())
                        } else if name == "search_procedures" {
                            let query = params["query"].as_str().unwrap_or("").to_string();
                            let refs = match procedure_runner.as_ref() {
                                Some(runner) => runner.search(&query).await,
                                None => vec![],
                            };
                            let items: Vec<serde_json::Value> = refs
                                .iter()
                                .map(|r| {
                                    serde_json::json!({
                                        "name": r.name,
                                        "path": r.path.display().to_string(),
                                        "description": r.description,
                                    })
                                })
                                .collect();
                            Ok(serde_json::json!({ "procedures": items }))
                        } else if name == "list_tables" || name == "describe_table" {
                            execute_database_lookup_tool(
                                &name,
                                params,
                                &connectors,
                                &default_connector,
                                &schema_cache,
                            )
                            .await
                        } else {
                            execute_clarifying_tool(&name, params, &*catalog)
                        }
                    })
                },
                &self.event_tx,
                ToolLoopConfig {
                    max_tool_rounds: max_rounds,
                    state: "clarifying".into(),
                    thinking,
                    response_schema: Some(clarify_response_schema()),
                    max_tokens_override: resume_max_tokens_override.or(self.max_tokens),
                    sub_spec_index: None,
                },
            )
            .await
        {
            Ok(v) => v,
            Err(crate::llm::LlmError::Suspended {
                prompt,
                suggestions,
                prior_messages,
            }) => {
                self.store_suspension_data(SuspendedRunData {
                    from_state: "clarifying".to_string(),
                    original_input: intent.raw_question.clone(),
                    trace_id: String::new(),
                    stage_data: serde_json::json!({ "conversation_history": prior_messages }),
                    question: prompt.clone(),
                    suggestions: suggestions.clone(),
                });
                return Err((
                    AnalyticsError::NeedsUserInput {
                        prompt: prompt.clone(),
                    },
                    BackTarget::Suspend {
                        questions: vec![HumanInputQuestion {
                            prompt,
                            suggestions,
                        }],
                    },
                ));
            }
            Err(crate::llm::LlmError::MaxTokensReached {
                current_max_tokens,
                prior_messages,
                ..
            }) => {
                let doubled = current_max_tokens.saturating_mul(2);
                let prompt = format!(
                    "The model ran out of token budget ({current_max_tokens} tokens). \
                     Continue with double the budget ({doubled} tokens)?"
                );
                self.store_suspension_data(SuspendedRunData {
                    from_state: "clarifying".to_string(),
                    original_input: intent.raw_question.clone(),
                    trace_id: String::new(),
                    stage_data: serde_json::json!({
                        "conversation_history": prior_messages,
                        "suspension_type": "max_tokens",
                        "max_tokens_override": doubled,
                    }),
                    question: prompt.clone(),
                    suggestions: vec!["Continue with double budget".to_string()],
                });
                return Err((
                    AnalyticsError::NeedsUserInput {
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
            Err(crate::llm::LlmError::MaxToolRoundsReached {
                rounds,
                prior_messages,
            }) => {
                let prompt = format!(
                    "The agent used all {rounds} allotted tool rounds. \
                     Continue with more rounds?"
                );
                self.store_suspension_data(SuspendedRunData {
                    from_state: "clarifying".to_string(),
                    original_input: intent.raw_question.clone(),
                    trace_id: String::new(),
                    stage_data: serde_json::json!({
                        "conversation_history": prior_messages,
                        "suspension_type": "max_tool_rounds",
                        "extra_rounds": rounds,
                    }),
                    question: prompt.clone(),
                    suggestions: vec!["Continue".to_string()],
                });
                return Err((
                    AnalyticsError::NeedsUserInput {
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
            Err(e) => {
                let msg = format!("LLM call failed during ground: {e}");
                return Err((
                    AnalyticsError::NeedsUserInput { prompt: msg },
                    BackTarget::Clarify(intent.clone(), Default::default()),
                ));
            }
        };

        #[derive(serde::Deserialize)]
        struct ClarifyResponse {
            question_type: QuestionType,
            metrics: Vec<String>,
            dimensions: Vec<String>,
            filters: Vec<String>,
            #[serde(default)]
            selected_procedure_path: Option<String>,
        }

        let resp: ClarifyResponse = if let Some(structured) = output.structured_response {
            serde_json::from_value(structured).map_err(|e| {
                let msg = format!("failed to deserialise ground response: {e}");
                (
                    AnalyticsError::NeedsUserInput { prompt: msg },
                    BackTarget::Clarify(intent.clone(), Default::default()),
                )
            })?
        } else {
            if output.text.trim().is_empty() {
                let msg = "ground: LLM returned empty text (no structured response); retrying"
                    .to_string();
                return Err((
                    AnalyticsError::NeedsUserInput { prompt: msg },
                    BackTarget::Clarify(intent.clone(), Default::default()),
                ));
            }
            let raw = strip_json_fences(&output.text).to_owned();
            serde_json::from_str(&raw).map_err(|e| {
                let raw_full = &output.text;
                let msg = format!("failed to parse ground response as JSON: {e}\nRaw: {raw_full}");
                (
                    AnalyticsError::NeedsUserInput { prompt: msg },
                    BackTarget::Clarify(intent.clone(), Default::default()),
                )
            })?
        };

        let clarified = AnalyticsIntent {
            raw_question: intent.raw_question,
            question_type: resp.question_type,
            metrics: resp.metrics,
            dimensions: resp.dimensions,
            filters: resp.filters,
            history: intent.history,
            spec_hint: None,
            selected_procedure: resp.selected_procedure_path.map(std::path::PathBuf::from),
        };
        emit_domain(
            &self.event_tx,
            AnalyticsEvent::IntentClarified {
                question_type: format!("{:?}", clarified.question_type),
                metrics: clarified.metrics.clone(),
                dimensions: clarified.dimensions.clone(),
                filters: clarified.filters.clone(),
                selected_procedure: clarified
                    .selected_procedure
                    .as_ref()
                    .map(|p| p.display().to_string()),
            },
        )
        .await;
        Ok(clarified)
    }

    /// Core clarify logic — runs **Triage** then **Ground** sequentially.
    pub(crate) async fn clarify_impl(
        &mut self,
        intent: AnalyticsIntent,
        retry_ctx: Option<&RetryContext>,
        session_turns: &[CompletedTurn<AnalyticsDomain>],
    ) -> Result<AnalyticsIntent, (AnalyticsError, BackTarget<AnalyticsDomain>)> {
        emit_domain(
            &self.event_tx,
            AnalyticsEvent::SchemaResolved {
                tables: Catalog::table_names(&*self.catalog),
            },
        )
        .await;

        let hypothesis = if retry_ctx.is_some() || self.resume_data.is_some() {
            DomainHypothesis {
                summary: format!("(retry) {}", intent.raw_question),
                relevant_tables: Catalog::table_names(&*self.catalog),
                question_type: intent.question_type.clone(),
                time_scope: None,
                confidence: 0.8,
                ambiguities: vec![],
                ambiguity_questions: vec![],
            }
        } else {
            self.triage_impl(&intent, session_turns).await?
        };

        const AMBIGUITY_CONFIDENCE_THRESHOLD: f32 = 0.5;
        if !hypothesis.ambiguities.is_empty()
            && hypothesis.confidence < AMBIGUITY_CONFIDENCE_THRESHOLD
        {
            // Prefer structured per-question suggestions from the LLM; fall back
            // to constructing questions from plain ambiguity strings.
            let questions: Vec<HumanInputQuestion> = if !hypothesis.ambiguity_questions.is_empty() {
                hypothesis.ambiguity_questions.clone()
            } else {
                hypothesis
                    .ambiguities
                    .iter()
                    .map(|a| HumanInputQuestion {
                        prompt: a.clone(),
                        suggestions: vec![],
                    })
                    .collect()
            };
            let combined_prompt = questions
                .iter()
                .map(|q| q.prompt.as_str())
                .collect::<Vec<_>>()
                .join("\n");
            self.store_suspension_data(SuspendedRunData {
                from_state: "clarifying".to_string(),
                original_input: intent.raw_question.clone(),
                trace_id: String::new(),
                stage_data: serde_json::json!({}),
                question: combined_prompt.clone(),
                suggestions: vec![],
            });
            return Err((
                AnalyticsError::NeedsUserInput {
                    prompt: combined_prompt,
                },
                BackTarget::Suspend { questions },
            ));
        }

        if hypothesis.question_type == QuestionType::GeneralInquiry {
            return Ok(AnalyticsIntent {
                raw_question: intent.raw_question,
                question_type: QuestionType::GeneralInquiry,
                metrics: vec![],
                dimensions: vec![],
                filters: vec![],
                history: intent.history,
                spec_hint: None,
                selected_procedure: None,
            });
        }

        self.ground_impl(intent, &hypothesis, retry_ctx, session_turns)
            .await
    }

    /// Answer a [`QuestionType::GeneralInquiry`] directly without SQL.
    pub(crate) async fn general_inquiry_impl(
        &mut self,
        intent: &AnalyticsIntent,
        session_turns: &[CompletedTurn<AnalyticsDomain>],
    ) -> Result<AnalyticsAnswer, (AnalyticsError, BackTarget<AnalyticsDomain>)> {
        let table_names = Catalog::table_names(&*self.catalog);
        let schema_context = self.catalog.to_prompt_string();
        let session_section = format_session_turns_section(session_turns);
        let history_section = format_history_section(&intent.history);

        let user_prompt = format!(
            "{session_section}{history_section}Question: {raw_question}\n\n\
             Available tables: {tables}\n\n\
             Schema overview:\n{schema}",
            raw_question = intent.raw_question,
            tables = if table_names.is_empty() {
                "(none)".to_string()
            } else {
                table_names.join(", ")
            },
            schema = schema_context,
        );

        let system_prompt =
            self.build_system_prompt("clarifying", GENERAL_INQUIRY_SYSTEM_PROMPT, None);
        let thinking = self.thinking_for_state("clarifying", ThinkingConfig::Disabled);
        let output = self
            .client
            .run_with_tools(
                &system_prompt,
                &user_prompt,
                &[],
                |_name: String, _params| {
                    Box::pin(async {
                        Err(ToolError::UnknownTool("no tools in general inquiry".into()))
                    })
                },
                &self.event_tx,
                ToolLoopConfig {
                    max_tool_rounds: 0,
                    state: "clarifying".into(),
                    thinking,
                    response_schema: None,
                    max_tokens_override: self.max_tokens,
                    sub_spec_index: None,
                },
            )
            .await
            .map_err(|e| {
                let msg = format!("LLM call failed during general inquiry: {e}");
                (
                    AnalyticsError::NeedsUserInput { prompt: msg },
                    BackTarget::Clarify(intent.clone(), Default::default()),
                )
            })?;

        Ok(AnalyticsAnswer {
            text: output.text,
            display_blocks: vec![],
        })
    }

    /// Returns the tool list for the clarifying state.
    ///
    /// `ask_user` is listed so the LLM can invoke it, but it is intercepted
    /// inside the tool loop in [`ground_impl`] before `execute_tool` is reached.
    /// See `resuming.rs` module doc for details.
    pub(super) fn tools_for_state_clarifying(&self) -> Vec<agentic_core::tools::ToolDef> {
        let has_semantic = !self.catalog.is_empty();
        let mut tools = crate::tools::clarifying_tools(has_semantic);
        tools.push(ask_user_tool_def());
        tools
    }
}

// ---------------------------------------------------------------------------
// State handler
// ---------------------------------------------------------------------------

/// Build the `StateHandler` for the **clarifying** state.
pub(super) fn build_clarifying_handler()
-> StateHandler<AnalyticsDomain, AnalyticsSolver, AnalyticsEvent> {
    StateHandler {
        next: "specifying",
        execute: Arc::new(
            |solver: &mut AnalyticsSolver,
             state,
             _events,
             run_ctx: &RunContext<AnalyticsDomain>,
             memory: &SessionMemory<AnalyticsDomain>| {
                Box::pin(async move {
                    let intent = match state {
                        ProblemState::Clarifying(i) => i,
                        _ => unreachable!("clarifying handler called with wrong state"),
                    };
                    let retry_ctx = run_ctx.retry_ctx.clone();
                    match solver
                        .clarify_impl(intent, retry_ctx.as_ref(), memory.turns())
                        .await
                    {
                        Ok(clarified)
                            if clarified.question_type == QuestionType::GeneralInquiry =>
                        {
                            match solver
                                .general_inquiry_impl(&clarified, memory.turns())
                                .await
                            {
                                Ok(answer) => {
                                    TransitionResult::ok_to(ProblemState::Done(answer), "done")
                                }
                                Err((err, back)) => {
                                    TransitionResult::diagnosing(ProblemState::Diagnosing {
                                        error: err,
                                        back,
                                    })
                                }
                            }
                        }
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
