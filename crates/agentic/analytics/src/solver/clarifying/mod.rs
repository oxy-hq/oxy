//! **Clarifying** pipeline stage.
//!
//! Owns:
//! - [`AnalyticsSolver::clarify_impl`] — classifies the question, attempts semantic shortcut, or forwards to Specifying
//! - [`AnalyticsSolver::general_inquiry_impl`] — GeneralInquiry short-circuit
//! - [`build_clarifying_handler`] — `StateHandler` factory

use std::sync::Arc;

use agentic_core::{
    HumanInputQuestion, SuspendReason,
    back_target::{BackTarget, RetryContext},
    human_input::SuspendedRunData,
    orchestrator::{CompletedTurn, RunContext, SessionMemory, StateHandler, TransitionResult},
    solver::DomainSolver,
    state::ProblemState,
    tools::ToolError,
};

use crate::catalog::Catalog;
use crate::events::AnalyticsEvent;
use crate::llm::{LlmError, ThinkingConfig, ToolLoopConfig};
use crate::schemas::triage_response_schema;
use crate::tools::execute_clarifying_tool;
use crate::types::{
    DomainHypothesis, QueryRequestItem, QuestionType, SolutionPayload, SolutionSource,
};
use crate::{AnalyticsAnswer, AnalyticsDomain, AnalyticsError, AnalyticsIntent, AnalyticsSolution};

use super::{
    AnalyticsSolver, emit_domain,
    prompts::{
        GENERAL_INQUIRY_SYSTEM_PROMPT, TRIAGE_SYSTEM_PROMPT, format_history_section,
        format_session_turns_section,
    },
    resuming::{ask_user_tool_def, handle_ask_user},
};

// ---------------------------------------------------------------------------
// Prompt builders
// ---------------------------------------------------------------------------

mod prompts;
pub(super) use prompts::{build_delegation_request, build_triage_user_prompt};

pub(crate) enum ClarifyOutcome {
    /// Normal path: pass the intent to Specifying.
    Intent(AnalyticsIntent),
    /// Fast path: airlayer compiled SQL during Clarifying — go straight to Executing.
    SemanticShortcut(AnalyticsSolution),
}

// ---------------------------------------------------------------------------
// Solver impl methods
// ---------------------------------------------------------------------------

impl AnalyticsSolver {
    /// Core clarify logic — classifies the question type, detects ambiguities,
    /// attempts a semantic shortcut, then forwards to Specifying.
    #[tracing::instrument(
        skip_all,
        fields(
            oxy.name = "analytics.clarify",
            oxy.span_type = "analytics",
            question_type = tracing::field::Empty,
            semantic_confidence = tracing::field::Empty,
        )
    )]
    pub(crate) async fn clarify_impl(
        &mut self,
        intent: AnalyticsIntent,
        retry_ctx: Option<&RetryContext>,
        session_turns: &[CompletedTurn<AnalyticsDomain>],
    ) -> Result<ClarifyOutcome, (AnalyticsError, BackTarget<AnalyticsDomain>)> {
        let topics_section = self.catalog.topics_summary();
        let user_prompt = build_triage_user_prompt(&intent, session_turns, &topics_section);
        let system_prompt = self.build_system_prompt("clarifying", TRIAGE_SYSTEM_PROMPT, None);
        let thinking = self.thinking_for_state("clarifying", ThinkingConfig::Disabled);

        // Build tool list: catalog/procedure tools + ask_user for mid-loop suspension.
        let mut tools = crate::tools::triage_tools();
        tools.push(ask_user_tool_def());

        let procedure_runner = self.procedure_runner.clone();
        let catalog = Arc::clone(&self.catalog);
        let human_input = Arc::clone(&self.human_input);

        // Shared slot for the semantic query proposed via the
        // `propose_semantic_query` tool call.  The closure captures a clone
        // of the Arc; after the tool loop we read it out.
        let proposed_query: Arc<std::sync::Mutex<Option<(QueryRequestItem, f32)>>> =
            Arc::new(std::sync::Mutex::new(None));

        // On resume after ask_user suspension, rebuild messages from the
        // persisted conversation snapshot so the LLM continues with full
        // context (catalog results already present).
        let had_user_answer = self.resume_data.is_some();
        let initial: crate::llm::InitialMessages = if let Some(resume) = self.resume_data.take() {
            let prior: Vec<serde_json::Value> = resume.data.stage_data["conversation_history"]
                .as_array()
                .cloned()
                .unwrap_or_default();
            if !prior.is_empty() {
                let msgs = self.client.build_resume_messages(
                    &prior,
                    &resume.data.question,
                    &resume.data.suggestions,
                    &resume.answer,
                );
                crate::llm::InitialMessages::Messages(msgs)
            } else {
                // No prior conversation — start fresh with the user's answer appended.
                crate::llm::InitialMessages::User(format!(
                    "{user_prompt}\n\nUser answered the clarifying question \"{q}\": {a}",
                    q = resume.data.question,
                    a = resume.answer,
                ))
            }
        } else if retry_ctx.is_some() {
            crate::llm::InitialMessages::User(user_prompt.clone())
        } else {
            crate::llm::InitialMessages::User(user_prompt.clone())
        };

        let output = self
            .client_for_state("clarifying")
            .run_with_tools(
                &system_prompt,
                initial,
                &tools,
                |name: String, params| {
                    let procedure_runner = procedure_runner.clone();
                    let catalog = Arc::clone(&catalog);
                    let human_input = Arc::clone(&human_input);
                    let proposed_query = Arc::clone(&proposed_query);
                    Box::pin(async move {
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
                        } else if name == "search_catalog" {
                            execute_clarifying_tool(&name, params, &*catalog)
                        } else if name == "propose_semantic_query" {
                            let confidence = params["confidence"].as_f64().unwrap_or(0.0) as f32;
                            let item: QueryRequestItem =
                                serde_json::from_value(params).unwrap_or_default();
                            *proposed_query.lock().expect("poisoned") = Some((item, confidence));
                            Ok(serde_json::json!({ "status": "accepted" }))
                        } else {
                            Err(ToolError::UnknownTool(name))
                        }
                    })
                },
                &self.event_tx,
                ToolLoopConfig {
                    max_tool_rounds: 5,
                    state: "clarifying".into(),
                    thinking,
                    response_schema: Some(triage_response_schema()),
                    max_tokens_override: self.max_tokens,
                    sub_spec_index: None,
                },
            )
            .await;

        // Handle ask_user suspension: store prior_messages so we can resume
        // the LLM conversation with full context (catalog results etc.).
        let output = match output {
            Err(LlmError::Suspended {
                prompt,
                suggestions,
                prior_messages,
            }) => {
                self.store_suspension_data(SuspendedRunData {
                    from_state: "clarifying".to_string(),
                    original_input: intent.raw_question.clone(),
                    trace_id: String::new(),
                    stage_data: serde_json::json!({
                        "conversation_history": prior_messages,
                    }),
                    question: prompt.clone(),
                    suggestions: suggestions.clone(),
                });
                let questions = vec![HumanInputQuestion {
                    prompt: prompt.clone(),
                    suggestions,
                }];
                return Err((
                    AnalyticsError::NeedsUserInput { prompt },
                    BackTarget::Suspend {
                        reason: SuspendReason::HumanInput { questions },
                    },
                ));
            }
            other => other.map_err(|e| {
                let msg = format!("LLM call failed during clarifying: {e}");
                (
                    AnalyticsError::NeedsUserInput { prompt: msg },
                    BackTarget::Clarify(intent.clone(), Default::default()),
                )
            })?,
        };

        let hypothesis: DomainHypothesis = if let Some(structured) = output.structured_response {
            serde_json::from_value(structured).map_err(|e| {
                let msg = format!("failed to deserialise clarifying response: {e}");
                (
                    AnalyticsError::NeedsUserInput { prompt: msg },
                    BackTarget::Clarify(intent.clone(), Default::default()),
                )
            })?
        } else {
            if output.text.trim().is_empty() {
                let msg = "clarifying: LLM returned empty text".to_string();
                return Err((
                    AnalyticsError::NeedsUserInput { prompt: msg },
                    BackTarget::Clarify(intent.clone(), Default::default()),
                ));
            }
            DomainHypothesis {
                summary: output.text.trim().to_string(),
                question_type: QuestionType::GeneralInquiry, // default to general inquiry if no structured response
                confidence: 0.0,
                ambiguities: vec![],
                time_scope: None,
                ambiguity_questions: vec![],
                semantic_query: None,
                semantic_confidence: 0.0,
                selected_procedure_path: None,
                missing_members: vec![],
            }
        };

        emit_domain(
            &self.event_tx,
            AnalyticsEvent::TriageCompleted {
                summary: hypothesis.summary.clone(),
                question_type: format!("{:?}", hypothesis.question_type),
                confidence: hypothesis.confidence,
                ambiguities: hypothesis.ambiguities.clone(),
            },
        )
        .await;

        // Extract the semantic query proposed via the `propose_semantic_query`
        // tool call (if any).  This replaces the former `hypothesis.semantic_query`
        // field which was removed from the triage response schema to reduce
        // grammar size for strict-mode providers.
        let (semantic_query, semantic_confidence) = proposed_query
            .lock()
            .expect("poisoned")
            .take()
            .map(|(q, c)| (q, c))
            .unwrap_or_default();

        let span = tracing::Span::current();
        span.record("question_type", format!("{:?}", hypothesis.question_type));
        span.record("semantic_confidence", semantic_confidence);

        if hypothesis.question_type == QuestionType::GeneralInquiry {
            return Ok(ClarifyOutcome::Intent(AnalyticsIntent {
                raw_question: intent.raw_question,
                summary: hypothesis.summary.clone(),
                question_type: QuestionType::GeneralInquiry,
                metrics: vec![],
                dimensions: vec![],
                filters: vec![],
                history: intent.history,
                spec_hint: None,
                selected_procedure: None,
                semantic_query: semantic_query.clone(),
                semantic_confidence,
            }));
        }

        // Attempt semantic shortcut: if the LLM called `propose_semantic_query`
        // with high confidence, try to compile it locally (fast, no LLM) and
        // skip Specifying/Solving.  semantic_query is always carried forward on
        // the intent regardless of whether the shortcut fires.
        const SEMANTIC_CONFIDENCE_THRESHOLD: f32 = 0.85;

        if semantic_confidence >= SEMANTIC_CONFIDENCE_THRESHOLD
            && !semantic_query.measures.is_empty()
        {
            let measures = semantic_query.measures.clone();
            let dimensions = semantic_query.dimensions.clone();

            emit_domain(
                &self.event_tx,
                AnalyticsEvent::SemanticShortcutAttempted {
                    measures: measures.clone(),
                    dimensions: dimensions.clone(),
                    filters: semantic_query.filters.clone(),
                    time_dimensions: semantic_query.time_dimensions.clone(),
                    confidence: semantic_confidence,
                },
            )
            .await;

            let query_request = semantic_query.to_query_request();
            match self.catalog.engine().compile_query(&query_request) {
                Ok(result) => {
                    let sql =
                        crate::airlayer_compat::substitute_params(&result.sql, &result.params);

                    emit_domain(
                        &self.event_tx,
                        AnalyticsEvent::SemanticShortcutResolved { sql: sql.clone() },
                    )
                    .await;

                    // Determine connector from resolved tables.
                    let translation = self.catalog.translate_to_raw_context(&query_request, "");
                    let connector_name = translation
                        .resolved_tables
                        .iter()
                        .find_map(|t| self.catalog.connector_for_table(t).map(|s| s.to_string()))
                        .unwrap_or_else(|| self.default_connector.clone());

                    return Ok(ClarifyOutcome::SemanticShortcut(AnalyticsSolution {
                        payload: SolutionPayload::Sql(sql),
                        solution_source: SolutionSource::SemanticLayer,
                        connector_name,
                        semantic_query: Some(semantic_query.clone()),
                    }));
                }
                Err(e) => {
                    // Silent fallback — log the error but proceed to Specifying.
                    tracing::info!(
                        "[clarifying] semantic shortcut compile failed, falling through to Specifying: {e}"
                    );
                }
            }
        }

        // ── Builder delegation: ask the builder agent to create missing members ──
        //
        // When the triage LLM reports missing semantic members (measures or
        // dimensions that the catalog doesn't have) and confidence is below the
        // shortcut threshold, suspend the pipeline and delegate to the builder
        // agent.  On success the pipeline resumes into Clarifying with an
        // updated catalog; on failure it falls through to Specifying as before.
        //
        // Guard: skip delegation when resuming (`had_user_answer`).  If we are
        // re-entering Clarifying after a builder delegation that failed, the
        // triage LLM will report the same missing members — without this guard
        // we would delegate again in an infinite loop.  When the builder
        // succeeded, the catalog is fresh and the members should be found, so
        // `missing_members` will be empty and this branch won't fire anyway.
        if !had_user_answer
            && !hypothesis.missing_members.is_empty()
            && semantic_confidence < SEMANTIC_CONFIDENCE_THRESHOLD
        {
            let (request, context) =
                build_delegation_request(&intent.raw_question, &hypothesis.missing_members);

            self.store_suspension_data(SuspendedRunData {
                from_state: "clarifying".to_string(),
                original_input: intent.raw_question.clone(),
                trace_id: String::new(), // filled by orchestrator
                stage_data: serde_json::json!({}),
                question: request.clone(),
                suggestions: vec![],
            });

            return Err((
                AnalyticsError::NeedsUserInput {
                    prompt: format!(
                        "Delegating to builder: creating {} missing semantic member(s)",
                        hypothesis.missing_members.len()
                    ),
                },
                BackTarget::Suspend {
                    reason: SuspendReason::Delegation {
                        target: agentic_core::delegation::DelegationTarget::Agent {
                            agent_id: "__builder__".to_string(),
                        },
                        request,
                        context,
                        policy: None,
                    },
                },
            ));
        }

        // Ground is dropped: pass the raw question and triage-derived question_type
        // directly to Specifying, which now owns catalog discovery + resolution in one loop.
        // Propagate any procedure selected during triage.
        //
        // When the user answered a clarifying question, the hypothesis summary
        // captures the disambiguated intent (e.g. "running performance" instead
        // of just "performance").  Enrich raw_question so Specifying sees the
        // full context — without this the user's answer is lost between stages.
        let enriched_question = if had_user_answer {
            format!(
                "{}\n\nClarified intent: {}",
                intent.raw_question, hypothesis.summary
            )
        } else {
            intent.raw_question
        };
        Ok(ClarifyOutcome::Intent(AnalyticsIntent {
            raw_question: enriched_question,
            summary: hypothesis.summary,
            question_type: hypothesis.question_type,
            metrics: vec![],
            dimensions: vec![],
            filters: vec![],
            history: intent.history,
            spec_hint: None,
            selected_procedure: hypothesis
                .selected_procedure_path
                .map(std::path::PathBuf::from),
            semantic_query,
            semantic_confidence,
        }))
    }

    /// Answer a [`QuestionType::GeneralInquiry`] directly without SQL.
    #[tracing::instrument(
        skip_all,
        fields(
            oxy.name = "analytics.general_inquiry",
            oxy.span_type = "analytics",
        )
    )]
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
            .client_for_state("clarifying")
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
            spec_hint: None,
        })
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
                        Ok(ClarifyOutcome::SemanticShortcut(solution)) => {
                            TransitionResult::ok_to(ProblemState::Executing(solution), "executing")
                        }
                        Ok(ClarifyOutcome::Intent(clarified))
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
                        Ok(ClarifyOutcome::Intent(clarified)) => {
                            TransitionResult::ok(ProblemState::Specifying(clarified))
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
