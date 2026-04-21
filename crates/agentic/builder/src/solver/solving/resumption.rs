//! [`BuilderSolver::resume_initial_messages`] — rebuild conversation history on resume.

use std::collections::HashMap;

use agentic_core::BackTarget;
use agentic_llm::InitialMessages;

use crate::tools::{apply_change_blocks, delete_file, write_file_content};
use crate::types::{BuilderDomain, BuilderError, BuilderSpec, ToolExchange};

use super::super::solver::BuilderSolver;
use super::{ProposeChangePayload, extract_all_propose_changes};

impl BuilderSolver {
    pub(super) async fn resume_initial_messages(
        &self,
        _spec: &BuilderSpec,
        resume: agentic_core::human_input::ResumeInput,
    ) -> Result<
        (InitialMessages, Vec<ToolExchange>, Option<u32>),
        (BuilderError, BackTarget<BuilderDomain>),
    > {
        let prior_messages = resume
            .data
            .stage_data
            .get("prior_messages")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        let question = resume.data.question.clone();
        let suggestions = resume.data.suggestions.clone();

        match resume
            .data
            .stage_data
            .get("suspension_type")
            .and_then(|v| v.as_str())
        {
            Some("tool_suspended") => {
                // Determine the suspension sub-type from the prompt JSON.
                let prompt_type = serde_json::from_str::<serde_json::Value>(&question)
                    .ok()
                    .and_then(|v| v["type"].as_str().map(String::from));

                // IMPORTANT: if a new suspending tool is added, add a branch here to
                // apply its side effects across ALL batched calls in the last assistant
                // turn (not just the first).  The LLM can batch multiple calls of the
                // same suspending tool in one response; `build_resume_messages` already
                // generates tool_results for all unmatched IDs, but the side effects
                // (e.g. writing files for `propose_change`) must be handled here.
                // See `extract_all_propose_changes` for the pattern to follow.
                let (exchanges, resume_answer) = match prompt_type.as_deref() {
                    Some("propose_change") => {
                        let answer_lower = resume.answer.to_lowercase();
                        let accepting = answer_lower.contains("accept");

                        let all_changes = extract_all_propose_changes(&prior_messages);
                        // Fall back to `question` if prior_messages didn't yield any payloads.
                        let changes_to_apply: Vec<ProposeChangePayload> = if all_changes.is_empty()
                        {
                            serde_json::from_str::<ProposeChangePayload>(&question)
                                .ok()
                                .into_iter()
                                .collect()
                        } else {
                            all_changes
                        };

                        if accepting {
                            let spec_fallback =
                                serde_json::from_value(resume.data.stage_data["spec"].clone())
                                    .unwrap_or(BuilderSpec {
                                        question: resume.data.original_input.clone(),
                                        history: vec![],
                                    });
                            // The suspended prompt JSON already contains the pre-computed
                            // `new_content` for the file that triggered the suspension.
                            // `precomputed_changes` holds pre-computed content for any
                            // additional files batched in the same LLM turn (computed by
                            // `precompute_batch_changes` at suspension time to close the
                            // TOCTOU window for multi-file batches).
                            let question_payload: Option<ProposeChangePayload> =
                                serde_json::from_str(&question).ok();
                            let batch_precomputed: HashMap<String, String> = resume
                                .data
                                .stage_data
                                .get("precomputed_changes")
                                .and_then(|v| serde_json::from_value(v.clone()).ok())
                                .unwrap_or_default();
                            for change in &changes_to_apply {
                                // Tier 1: `new_content` embedded directly in the payload
                                //         (question-fallback path).
                                // Tier 2: `new_content` from the suspended prompt JSON
                                //         (first file in a batched turn).
                                // Tier 3: pre-computed content stored at suspension time
                                //         for other files in the same batched turn.
                                // Tier 4 (last resort): re-apply blocks from disk —
                                //         TOCTOU window, only reached if none of the above
                                //         sources are available.
                                let precomputed = change
                                    .new_content
                                    .as_deref()
                                    .or_else(|| {
                                        question_payload
                                            .as_ref()
                                            .filter(|q| q.file_path == change.file_path)
                                            .and_then(|q| q.new_content.as_deref())
                                    })
                                    .or_else(|| {
                                        batch_precomputed.get(&change.file_path).map(String::as_str)
                                    });
                                let apply_result = if change.delete {
                                    delete_file(&self.project_root, &change.file_path).await
                                } else if let Some(content) = precomputed {
                                    write_file_content(
                                        &self.project_root,
                                        &change.file_path,
                                        content,
                                    )
                                    .await
                                } else if let Some(blocks) = &change.changes {
                                    apply_change_blocks(
                                        &self.project_root,
                                        &change.file_path,
                                        blocks,
                                    )
                                    .await
                                } else {
                                    Err(format!(
                                        "propose_change for '{}' has no blocks and is not a deletion",
                                        change.file_path
                                    ))
                                };
                                if let Err(err) = apply_result {
                                    // TODO: files applied before this one are not rolled back.
                                    // apply_change/delete_file should be made transactional, or
                                    // the loop should collect all errors before failing.
                                    return Err((
                                        BuilderError::Resume(err),
                                        BackTarget::Solve(spec_fallback, Default::default()),
                                    ));
                                }
                            }
                        }

                        let resume_answer = if accepting {
                            let paths: Vec<&str> = changes_to_apply
                                .iter()
                                .map(|c| c.file_path.as_str())
                                .collect();
                            if paths.is_empty() {
                                "The user accepted the proposed change.".to_string()
                            } else {
                                format!(
                                    "The user accepted the proposed change{}. The {} been updated.",
                                    if paths.len() == 1 {
                                        format!(" to '{}'", paths[0])
                                    } else {
                                        format!(
                                            "s to {}",
                                            paths
                                                .iter()
                                                .map(|p| format!("'{p}'"))
                                                .collect::<Vec<_>>()
                                                .join(", ")
                                        )
                                    },
                                    if paths.len() == 1 {
                                        "file has"
                                    } else {
                                        "files have"
                                    }
                                )
                            }
                        } else {
                            "The user rejected the proposed change. Please reconsider or propose an alternative approach."
                                .to_string()
                        };

                        let tool_exchanges: Vec<ToolExchange> = changes_to_apply
                            .iter()
                            .map(|c| ToolExchange {
                                name: "propose_change".to_string(),
                                input: serde_json::to_string(c).unwrap_or_default(),
                                output: resume_answer.clone(),
                            })
                            .collect();

                        (tool_exchanges, resume_answer)
                    }
                    _ => {
                        let exchange = ToolExchange {
                            name: "ask_user".to_string(),
                            input: serde_json::json!({
                                "prompt": question,
                                "suggestions": suggestions,
                            })
                            .to_string(),
                            output: resume.answer.clone(),
                        };
                        (vec![exchange], resume.answer.clone())
                    }
                };

                Ok((
                    InitialMessages::Messages(self.client.build_resume_messages(
                        &prior_messages,
                        &question,
                        &suggestions,
                        &resume_answer,
                    )),
                    exchanges,
                    None,
                ))
            }
            Some("max_tool_rounds") | Some("max_tokens") => {
                let max_tokens_override = resume
                    .data
                    .stage_data
                    .get("max_tokens_override")
                    .and_then(|v| v.as_u64())
                    .map(|v| v as u32);
                Ok((
                    InitialMessages::Messages(agentic_llm::LlmClient::build_continue_messages(
                        &prior_messages,
                    )),
                    vec![],
                    max_tokens_override,
                ))
            }
            other => Err((
                BuilderError::Resume(format!(
                    "unsupported solving resume type: {}",
                    other.unwrap_or("unknown")
                )),
                BackTarget::Solve(
                    serde_json::from_value(resume.data.stage_data["spec"].clone()).unwrap_or(
                        BuilderSpec {
                            question: resume.data.original_input.clone(),
                            history: vec![],
                        },
                    ),
                    Default::default(),
                ),
            )),
        }
    }
}
