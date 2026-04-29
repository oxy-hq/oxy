//! [`BuilderSolver::resume_initial_messages`] — rebuild conversation history on resume.

use std::collections::HashMap;

use agentic_core::BackTarget;
use agentic_llm::InitialMessages;

use crate::events::BuilderEvent;
use crate::tools::{
    apply_change_blocks, delete_file, execute_init_dbt_project, safe_path, write_file_content,
};
use crate::types::{BuilderDomain, BuilderError, BuilderSpec, ToolExchange};

use super::super::solver::{BuilderSolver, emit_domain};
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
                    Some("manage_directory") => {
                        let answer_lower = resume.answer.to_lowercase();
                        let accepting = answer_lower.contains("accept");

                        let prompt_json: serde_json::Value =
                            serde_json::from_str(&question).unwrap_or_default();
                        let operation = prompt_json["operation"].as_str().unwrap_or("").to_string();
                        let path = prompt_json["path"].as_str().unwrap_or("").to_string();

                        let (tool_output, resume_answer) = if accepting {
                            let apply_result: Result<(), String> = (async {
                                match operation.as_str() {
                                    "create" => {
                                        let abs = safe_path(&self.project_root, &path)
                                            .map_err(|e| e.to_string())?;
                                        tokio::fs::create_dir_all(&abs)
                                            .await
                                            .map_err(|e| format!("failed to create '{path}': {e}"))
                                    }
                                    "delete" => {
                                        let abs = safe_path(&self.project_root, &path)
                                            .map_err(|e| e.to_string())?;
                                        tokio::fs::remove_dir_all(&abs)
                                            .await
                                            .map_err(|e| format!("failed to delete '{path}': {e}"))
                                    }
                                    "rename" => {
                                        let new_path = prompt_json["new_path"]
                                            .as_str()
                                            .unwrap_or("")
                                            .to_string();
                                        if new_path.is_empty() {
                                            return Err(
                                                "new_path is required for rename".to_string()
                                            );
                                        }
                                        let abs = safe_path(&self.project_root, &path)
                                            .map_err(|e| e.to_string())?;
                                        let abs_new = safe_path(&self.project_root, &new_path)
                                            .map_err(|e| e.to_string())?;
                                        if let Some(parent) = abs_new.parent() {
                                            tokio::fs::create_dir_all(parent).await.map_err(
                                                |e| format!("failed to create parent dirs: {e}"),
                                            )?;
                                        }
                                        tokio::fs::rename(&abs, &abs_new).await.map_err(|e| {
                                            format!(
                                                "failed to rename '{path}' to '{new_path}': {e}"
                                            )
                                        })
                                    }
                                    other => Err(format!("unknown operation '{other}'")),
                                }
                            })
                            .await;

                            match apply_result {
                                Ok(()) => {
                                    let output =
                                        serde_json::json!({ "answer": "Accept" }).to_string();
                                    let msg = format!(
                                        "Directory operation '{operation}' on '{path}' completed successfully."
                                    );
                                    (output, msg)
                                }
                                Err(err) => {
                                    let output =
                                        serde_json::json!({ "answer": "Accept", "error": err })
                                            .to_string();
                                    let msg = format!(
                                        "Directory operation '{operation}' on '{path}' failed: {err}"
                                    );
                                    (output, msg)
                                }
                            }
                        } else {
                            let output = serde_json::json!({ "answer": "Reject" }).to_string();
                            let msg = format!(
                                "The user rejected the directory operation '{operation}' on '{path}'."
                            );
                            (output, msg)
                        };

                        let exchange = ToolExchange {
                            name: "manage_directory".to_string(),
                            input: serde_json::json!({
                                "operation": &operation,
                                "path": &path,
                            })
                            .to_string(),
                            output: tool_output,
                        };
                        (vec![exchange], resume_answer)
                    }
                    Some("init_dbt_project") => {
                        let answer_lower = resume.answer.to_lowercase();
                        let accepting = answer_lower.contains("accept");

                        let prompt_json: serde_json::Value =
                            serde_json::from_str(&question).unwrap_or_default();
                        let project_name = prompt_json["project_name"]
                            .as_str()
                            .unwrap_or("")
                            .to_string();

                        if project_name.contains('/')
                            || project_name.contains('\\')
                            || project_name.contains("..")
                        {
                            tracing::warn!(
                                "init_dbt_project resume: invalid project_name '{project_name}', rejecting"
                            );
                            return Err((
                                BuilderError::Resume(format!(
                                    "invalid project name: {project_name}"
                                )),
                                BackTarget::Solve(
                                    BuilderSpec {
                                        question: resume.data.original_input.clone(),
                                        history: vec![],
                                    },
                                    Default::default(),
                                ),
                            ));
                        }

                        let project_root = format!("modeling/{project_name}");

                        let tool_input = serde_json::json!({ "name": project_name }).to_string();

                        let (tool_output, resume_answer) = if accepting {
                            let params = serde_json::json!({ "name": project_name });
                            let result = execute_init_dbt_project(&self.project_root, &params);
                            match result {
                                Ok(ref val) if val["ok"] == true => {
                                    if let Some(files) = val["files"].as_array() {
                                        for file in files {
                                            let file_path =
                                                file[0].as_str().unwrap_or("").to_string();
                                            let new_content =
                                                file[1].as_str().unwrap_or("").to_string();
                                            let description =
                                                file[2].as_str().unwrap_or("").to_string();
                                            emit_domain(
                                                &self.event_tx,
                                                BuilderEvent::FileChanged {
                                                    file_path,
                                                    description,
                                                    new_content,
                                                    old_content: String::new(),
                                                    is_deletion: false,
                                                },
                                            )
                                            .await;
                                        }
                                    }
                                    let output = serde_json::json!({
                                        "ok": true,
                                        "project_name": project_name,
                                        "project_dir": project_root,
                                    })
                                    .to_string();
                                    let msg = format!(
                                        "dbt project '{project_name}' has been initialized successfully."
                                    );
                                    (output, msg)
                                }
                                Ok(val) => {
                                    let error = val["error"]
                                        .as_str()
                                        .unwrap_or("unknown error")
                                        .to_string();
                                    emit_domain(
                                        &self.event_tx,
                                        BuilderEvent::ToolUsed {
                                            tool_name: "init_dbt_project".into(),
                                            summary: format!("error:{project_name}:{error}"),
                                        },
                                    )
                                    .await;
                                    let output = serde_json::json!({
                                        "ok": false,
                                        "error": error,
                                    })
                                    .to_string();
                                    let msg = format!(
                                        "Failed to initialize dbt project '{project_name}': {error}"
                                    );
                                    (output, msg)
                                }
                                Err(e) => {
                                    let error = e.to_string();
                                    emit_domain(
                                        &self.event_tx,
                                        BuilderEvent::ToolUsed {
                                            tool_name: "init_dbt_project".into(),
                                            summary: format!("error:{project_name}:{error}"),
                                        },
                                    )
                                    .await;
                                    let output = serde_json::json!({
                                        "ok": false,
                                        "error": error,
                                    })
                                    .to_string();
                                    let msg = format!("Failed to initialize dbt project: {error}");
                                    (output, msg)
                                }
                            }
                        } else {
                            let output =
                                serde_json::json!({ "ok": false, "rejected": true }).to_string();
                            let msg = format!(
                                "The user rejected the initialization of dbt project '{project_name}'."
                            );
                            (output, msg)
                        };

                        let exchange = ToolExchange {
                            name: "init_dbt_project".to_string(),
                            input: tool_input,
                            output: tool_output,
                        };
                        (vec![exchange], resume_answer)
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
