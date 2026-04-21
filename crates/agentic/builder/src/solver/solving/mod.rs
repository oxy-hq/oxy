use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};

use agentic_core::{
    HumanInputQuestion, SuspendReason, back_target::BackTarget, human_input::SuspendedRunData,
};
use agentic_llm::{LlmError, LlmOutput};

use agentic_core::orchestrator::RunContext;

use crate::{
    tools::{ChangeBlock, all_tools, apply_blocks_to_content, safe_path},
    types::{BuilderDomain, BuilderError, BuilderSolution, BuilderSpec, ToolExchange},
};

use super::solver::{BuilderSolver, dispatch_tool, make_resume_stage_data, record_tool_exchange};
use agentic_core::solver::DomainSolver;

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub(super) struct ProposeChangePayload {
    file_path: String,
    #[serde(default)]
    changes: Option<Vec<ChangeBlock>>,
    #[serde(default)]
    delete: bool,
    /// Pre-computed file content stored in the suspended prompt JSON.
    /// When present, written directly to avoid a TOCTOU re-read.
    #[serde(default)]
    new_content: Option<String>,
}

/// For each `propose_change` call in `prior_messages` whose `file_path` is NOT
/// already covered by the suspended `prompt` (i.e. already has a pre-computed
/// `new_content`), read the file and apply the change blocks **now** (at
/// suspension time) so that [`resume_initial_messages`] can write the exact
/// pre-computed content without a TOCTOU re-read.
///
/// Returns a map from `file_path` → `new_content`.  Delete proposals are
/// skipped (no content to pre-compute).
async fn precompute_batch_changes(
    workspace_root: &Path,
    prior_messages: &[serde_json::Value],
    suspended_prompt: &str,
) -> HashMap<String, String> {
    let suspended_file: Option<String> =
        serde_json::from_str::<serde_json::Value>(suspended_prompt)
            .ok()
            .and_then(|v| v["file_path"].as_str().map(String::from));

    let mut result = HashMap::new();
    for payload in extract_all_propose_changes(prior_messages) {
        if payload.delete {
            continue;
        }
        if suspended_file.as_deref() == Some(payload.file_path.as_str()) {
            continue; // already stored in the suspended prompt JSON
        }
        if result.contains_key(&payload.file_path) {
            continue; // deduplicate
        }
        if let Some(blocks) = &payload.changes {
            let abs = match safe_path(workspace_root, &payload.file_path) {
                Ok(p) => p,
                Err(_) => continue,
            };
            let old_content = tokio::fs::read_to_string(&abs).await.unwrap_or_default();
            if let Ok(new_content) = apply_blocks_to_content(&old_content, blocks) {
                result.insert(payload.file_path, new_content);
            }
        }
    }
    result
}

/// Extract all `propose_change` payloads from the most recent assistant turn.
/// Handles all three provider message formats:
/// - Anthropic: `{"role":"assistant","content":[{"type":"tool_use",...}]}`
/// - OpenAI Responses API: `[{"type":"function_call","name":...,"arguments":...}]`
/// - OpenAI Chat Completions: `{"role":"assistant","tool_calls":[...]}`
///
/// Unlike `find_all_unmatched_tool_ids`, this does not filter by matched status.
/// `propose_change` always suspends immediately, so no call in the last turn
/// will ever have a result yet — all of them need to be applied.
pub(super) fn extract_all_propose_changes(
    prior_messages: &[serde_json::Value],
) -> Vec<ProposeChangePayload> {
    let mut result = Vec::new();

    let parse_args = |args: &str| serde_json::from_str::<ProposeChangePayload>(args).ok();

    for m in prior_messages.iter().rev() {
        // Anthropic: role:"assistant", content array with tool_use blocks.
        if m["role"].as_str() == Some("assistant") {
            if let Some(blocks) = m["content"].as_array() {
                for block in blocks {
                    if block["type"].as_str() == Some("tool_use")
                        && block["name"].as_str() == Some("propose_change")
                        && let Ok(payload) =
                            serde_json::from_value::<ProposeChangePayload>(block["input"].clone())
                    {
                        result.push(payload);
                    }
                }
            }
            // OpenAI Chat Completions: tool_calls array.
            if let Some(tool_calls) = m["tool_calls"].as_array() {
                for tc in tool_calls {
                    if tc["function"]["name"].as_str() == Some("propose_change")
                        && let Some(p) = tc["function"]["arguments"].as_str().and_then(parse_args)
                    {
                        result.push(p);
                    }
                }
            }
            break; // only inspect the most recent assistant turn
        }
        // OpenAI Responses API: Value::Array of flat function_call items.
        if let Some(items) = m.as_array() {
            for item in items {
                if item["type"].as_str() == Some("function_call")
                    && item["name"].as_str() == Some("propose_change")
                    && let Some(p) = item["arguments"].as_str().and_then(parse_args)
                {
                    result.push(p);
                }
            }
            if !result.is_empty() {
                break;
            }
        }
    }
    result
}

/// Maximum number of transient-error retries before surfacing the failure to
/// the user.  Prevents infinite retry loops when the LLM keeps returning
/// recoverable errors.
const MAX_SOLVE_RETRIES: u32 = 3;

impl BuilderSolver {
    pub(crate) async fn solve_impl(
        &mut self,
        spec: BuilderSpec,
        run_ctx: &RunContext<BuilderDomain>,
    ) -> Result<BuilderSolution, (BuilderError, BackTarget<BuilderDomain>)> {
        let system = self.build_solving_system_prompt();
        let default_schema = crate::schema_provider::EmptySchemaProvider;
        let schema_ref: &dyn crate::schema_provider::BuilderSchemaProvider =
            match self.schema_provider.as_ref() {
                Some(p) => p.as_ref(),
                None => &default_schema,
            };
        let tools = all_tools(schema_ref);
        let (current_initial, prior_tool_exchanges, resume_max_tokens_override) =
            match self.resume_data.take() {
                Some(resume) => {
                    let mut prior_tool_exchanges = resume
                        .data
                        .stage_data
                        .get("tool_exchanges")
                        .cloned()
                        .and_then(|value| serde_json::from_value::<Vec<ToolExchange>>(value).ok())
                        .unwrap_or_default();
                    let (initial_messages, resumed_exchanges, max_tokens_override) =
                        self.resume_initial_messages(&spec, resume).await?;
                    prior_tool_exchanges.extend(resumed_exchanges);
                    (initial_messages, prior_tool_exchanges, max_tokens_override)
                }
                None => (
                    self.build_initial_messages(&spec.question, &spec.history),
                    Vec::new(),
                    None,
                ),
            };
        let exchanges = Arc::new(Mutex::new(prior_tool_exchanges));

        let test_runner = self.test_runner.clone();
        let workspace_root = self.project_root.clone();
        let event_tx = self.event_tx.clone();
        let human_input = self.human_input.clone();
        let db_provider = self.db_provider.clone();
        let project_validator = self.project_validator.clone();
        let schema_provider = self.schema_provider.clone();
        let semantic_compiler = self.semantic_compiler.clone();
        let exchanges_for_tools = Arc::clone(&exchanges);

        let result = self
            .client
            .run_with_tools(
                &system,
                current_initial,
                &tools,
                move |name, params| {
                    let workspace_root = workspace_root.clone();
                    let event_tx = event_tx.clone();
                    let test_runner = test_runner.clone();
                    let human_input = human_input.clone();
                    let db_provider = db_provider.clone();
                    let project_validator = project_validator.clone();
                    let schema_provider = schema_provider.clone();
                    let semantic_compiler = semantic_compiler.clone();
                    let exchanges = Arc::clone(&exchanges_for_tools);
                    Box::pin(async move {
                        let result = dispatch_tool(
                            &name,
                            &params,
                            &workspace_root,
                            &event_tx,
                            test_runner,
                            human_input,
                            db_provider.as_ref(),
                            project_validator.as_ref(),
                            schema_provider.as_ref(),
                            semantic_compiler.as_ref(),
                        )
                        .await;
                        let mut guard = exchanges.lock().unwrap_or_else(|e| e.into_inner());
                        record_tool_exchange(&mut guard, &name, &params, &result);
                        result
                    })
                },
                &self.event_tx,
                {
                    let mut config = BuilderSolver::solving_loop_config();
                    config.max_tokens_override = resume_max_tokens_override;
                    config
                },
            )
            .await;

        match result {
            Ok(output) => Ok(self.to_solution(spec, output, exchanges)),
            Err(LlmError::Suspended {
                prompt,
                suggestions,
                prior_messages,
            }) => {
                let tool_exchanges = exchanges.lock().unwrap_or_else(|e| e.into_inner()).clone();
                // Pre-compute new_content for every batched propose_change call in
                // this turn whose file isn't already covered by the suspended prompt.
                // Stored in stage_data so resume can write exact content without a
                // TOCTOU re-read (adjacent gap to the TODO at the apply loop below).
                let batch_precomputed =
                    precompute_batch_changes(&self.project_root, &prior_messages, &prompt).await;
                let mut stage_data = make_resume_stage_data(
                    &spec,
                    &prior_messages,
                    "tool_suspended",
                    &prompt,
                    &suggestions,
                    &tool_exchanges,
                );
                if !batch_precomputed.is_empty() {
                    stage_data["precomputed_changes"] =
                        serde_json::to_value(&batch_precomputed).unwrap_or_default();
                }
                <BuilderSolver as DomainSolver<BuilderDomain>>::store_suspension_data(
                    self,
                    SuspendedRunData {
                        from_state: "solving".to_string(),
                        original_input: spec.question.clone(),
                        trace_id: String::new(),
                        stage_data,
                        question: prompt.clone(),
                        suggestions: suggestions.clone(),
                    },
                );
                Err((
                    BuilderError::NeedsUserInput {
                        prompt: prompt.clone(),
                    },
                    BackTarget::Suspend {
                        reason: SuspendReason::HumanInput {
                            questions: vec![HumanInputQuestion {
                                prompt,
                                suggestions,
                            }],
                        },
                    },
                ))
            }
            Err(LlmError::MaxToolRoundsReached {
                rounds,
                prior_messages,
            }) => {
                let tool_exchanges = exchanges.lock().unwrap_or_else(|e| e.into_inner()).clone();
                let prompt = format!(
                    "The builder used all {rounds} allotted tool rounds. Continue with more rounds?"
                );
                let suggestions = vec!["Continue".to_string()];
                <BuilderSolver as DomainSolver<BuilderDomain>>::store_suspension_data(
                    self,
                    SuspendedRunData {
                        from_state: "solving".to_string(),
                        original_input: spec.question.clone(),
                        trace_id: String::new(),
                        stage_data: make_resume_stage_data(
                            &spec,
                            &prior_messages,
                            "max_tool_rounds",
                            &prompt,
                            &suggestions,
                            &tool_exchanges,
                        ),
                        question: prompt.clone(),
                        suggestions: suggestions.clone(),
                    },
                );
                Err((
                    BuilderError::NeedsUserInput {
                        prompt: prompt.clone(),
                    },
                    BackTarget::Suspend {
                        reason: SuspendReason::HumanInput {
                            questions: vec![HumanInputQuestion {
                                prompt,
                                suggestions,
                            }],
                        },
                    },
                ))
            }
            Err(LlmError::MaxTokensReached {
                current_max_tokens,
                prior_messages,
                ..
            }) => {
                let tool_exchanges = exchanges.lock().unwrap_or_else(|e| e.into_inner()).clone();
                let doubled = current_max_tokens.saturating_mul(2);
                let prompt = format!(
                    "The builder ran out of token budget ({current_max_tokens} tokens). \
                     Continue with double the budget ({doubled} tokens)?"
                );
                let suggestions = vec!["Continue with double budget".to_string()];
                let mut stage_data = make_resume_stage_data(
                    &spec,
                    &prior_messages,
                    "max_tokens",
                    &prompt,
                    &suggestions,
                    &tool_exchanges,
                );
                stage_data["max_tokens_override"] = serde_json::json!(doubled);
                <BuilderSolver as DomainSolver<BuilderDomain>>::store_suspension_data(
                    self,
                    SuspendedRunData {
                        from_state: "solving".to_string(),
                        original_input: spec.question.clone(),
                        trace_id: String::new(),
                        stage_data,
                        question: prompt.clone(),
                        suggestions: suggestions.clone(),
                    },
                );
                Err((
                    BuilderError::NeedsUserInput {
                        prompt: prompt.clone(),
                    },
                    BackTarget::Suspend {
                        reason: SuspendReason::HumanInput {
                            questions: vec![HumanInputQuestion {
                                prompt,
                                suggestions,
                            }],
                        },
                    },
                ))
            }
            Err(ref err @ (LlmError::Http(_) | LlmError::Auth(_))) => {
                // Non-transient errors — retrying won't help.  Surface the
                // error to the user via Suspend so the pipeline stops.
                let msg = err.to_string();
                tracing::error!("builder solving fatal LLM error: {msg}");
                <BuilderSolver as DomainSolver<BuilderDomain>>::store_suspension_data(
                    self,
                    SuspendedRunData {
                        from_state: "solving".to_string(),
                        original_input: spec.question.clone(),
                        trace_id: String::new(),
                        stage_data: Default::default(),
                        question: msg.clone(),
                        suggestions: vec![],
                    },
                );
                Err((
                    BuilderError::Llm(msg.clone()),
                    BackTarget::Suspend {
                        reason: SuspendReason::HumanInput {
                            questions: vec![HumanInputQuestion {
                                prompt: format!("Builder encountered a fatal error: {msg}"),
                                suggestions: vec![],
                            }],
                        },
                    },
                ))
            }
            Err(err) => {
                let current_attempt = run_ctx.retry_ctx.as_ref().map_or(0, |ctx| ctx.attempt);
                if current_attempt >= MAX_SOLVE_RETRIES {
                    let msg = format!("Builder failed after {} retries: {err}", current_attempt);
                    tracing::error!("{msg}");
                    <BuilderSolver as DomainSolver<BuilderDomain>>::store_suspension_data(
                        self,
                        SuspendedRunData {
                            from_state: "solving".to_string(),
                            original_input: spec.question.clone(),
                            trace_id: String::new(),
                            stage_data: Default::default(),
                            question: msg.clone(),
                            suggestions: vec![],
                        },
                    );
                    Err((
                        BuilderError::Llm(msg.clone()),
                        BackTarget::Suspend {
                            reason: SuspendReason::HumanInput {
                                questions: vec![HumanInputQuestion {
                                    prompt: msg,
                                    suggestions: vec![],
                                }],
                            },
                        },
                    ))
                } else {
                    tracing::warn!(
                        "builder solving LLM error (attempt {}, will retry): {err}",
                        current_attempt + 1
                    );
                    let retry_ctx = run_ctx
                        .retry_ctx
                        .clone()
                        .unwrap_or_default()
                        .advance(err.to_string());
                    Err((
                        BuilderError::Llm(err.to_string()),
                        BackTarget::Solve(spec, retry_ctx),
                    ))
                }
            }
        }
    }

    fn to_solution(
        &self,
        spec: BuilderSpec,
        output: LlmOutput,
        exchanges: Arc<Mutex<Vec<ToolExchange>>>,
    ) -> BuilderSolution {
        let tool_exchanges = exchanges.lock().unwrap_or_else(|e| e.into_inner()).clone();
        BuilderSolution {
            question: spec.question,
            history: spec.history,
            draft_text: output.text,
            tool_exchanges,
        }
    }
}

mod resumption;
