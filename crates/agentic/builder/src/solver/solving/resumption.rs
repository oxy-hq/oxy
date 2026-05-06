//! [`BuilderSolver::resume_initial_messages`] — rebuild conversation history on resume.

use agentic_core::BackTarget;
use agentic_core::solver::DomainSolver;
use agentic_core::{HumanInputQuestion, SuspendReason, human_input::SuspendedRunData};
use agentic_llm::InitialMessages;

use crate::events::BuilderEvent;
use crate::tools::{execute_init_dbt_project, remove_file, safe_path, write_file_content};
use crate::types::{BuilderDomain, BuilderError, BuilderSpec, ToolExchange};

use super::super::solver::{BuilderSolver, emit_domain};
use super::{WriteOp, extract_all_write_ops};

impl BuilderSolver {
    pub(super) async fn resume_initial_messages(
        &mut self,
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
                // (e.g. writing files for `file_change`) must be handled here.
                // See `extract_all_file_changes` for the pattern to follow.
                let (exchanges, resume_answer) = match prompt_type.as_deref() {
                    Some("write_file") | Some("edit_file") | Some("delete_file") => {
                        let answer_lower = resume.answer.to_lowercase();
                        let accepting = answer_lower.contains("accept");

                        let prompt_json: serde_json::Value =
                            serde_json::from_str(&question).unwrap_or_default();
                        let current_file_path =
                            prompt_json["file_path"].as_str().unwrap_or("").to_string();
                        let current_new_content: Option<String> =
                            prompt_json["new_content"].as_str().map(String::from);
                        let current_description = prompt_json["description"]
                            .as_str()
                            .unwrap_or("")
                            .to_string();
                        let current_old_content = prompt_json["old_content"]
                            .as_str()
                            .unwrap_or("")
                            .to_string();

                        // Sequential state: remaining ops + accumulated results from prior rounds.
                        let mut pending_op_prompts: Vec<String> = resume
                            .data
                            .stage_data
                            .get("pending_op_prompts")
                            .and_then(|v| serde_json::from_value(v.clone()).ok())
                            .unwrap_or_default();
                        let mut op_results: Vec<String> = resume
                            .data
                            .stage_data
                            .get("op_results")
                            .and_then(|v| serde_json::from_value(v.clone()).ok())
                            .unwrap_or_default();

                        // Apply or reject the current file op only.
                        let current_result: String = if accepting {
                            let apply_result: Result<(), String> = match prompt_type.as_deref() {
                                Some("write_file") | Some("edit_file") => {
                                    write_file_content(
                                        &self.project_root,
                                        &current_file_path,
                                        &current_new_content.clone().unwrap_or_default(),
                                    )
                                    .await
                                }
                                Some("delete_file") => {
                                    remove_file(&self.project_root, &current_file_path).await
                                }
                                _ => Ok(()),
                            };
                            match apply_result {
                                Ok(()) => format!(
                                    "The user accepted the proposed change to '{}'. The file has been updated.",
                                    current_file_path
                                ),
                                Err(err) => {
                                    tracing::warn!(
                                        "failed to apply accepted change to '{}': {err}",
                                        current_file_path
                                    );
                                    format!(
                                        "The user accepted the proposed change to '{}', but applying it failed: {err}",
                                        current_file_path
                                    )
                                }
                            }
                        } else {
                            format!(
                                "The user rejected the proposed change to '{current_file_path}'. \
                                 Please reconsider or propose an alternative approach."
                            )
                        };

                        // Emit FileChanged after acceptance so the activity panel reflects the final status.
                        if accepting {
                            emit_domain(
                                &self.event_tx,
                                BuilderEvent::FileChanged {
                                    file_path: current_file_path.clone(),
                                    description: current_description,
                                    new_content: current_new_content.clone().unwrap_or_default(),
                                    old_content: current_old_content,
                                    is_deletion: prompt_type.as_deref() == Some("delete_file"),
                                },
                            )
                            .await;
                        }

                        op_results.push(current_result);

                        if !pending_op_prompts.is_empty() {
                            // More files remain — re-suspend for the next one.
                            let next_prompt = pending_op_prompts.remove(0);
                            let next_json: serde_json::Value =
                                serde_json::from_str(&next_prompt).unwrap_or_default();

                            emit_domain(
                                &self.event_tx,
                                BuilderEvent::FileChangePending {
                                    file_path: next_json["file_path"]
                                        .as_str()
                                        .unwrap_or("")
                                        .to_string(),
                                    description: next_json["description"]
                                        .as_str()
                                        .unwrap_or("")
                                        .to_string(),
                                    new_content: next_json["new_content"]
                                        .as_str()
                                        .unwrap_or("")
                                        .to_string(),
                                    old_content: next_json["old_content"]
                                        .as_str()
                                        .unwrap_or("")
                                        .to_string(),
                                },
                            )
                            .await;

                            let next_stage_data = serde_json::json!({
                                "spec": resume.data.stage_data.get("spec"),
                                "prior_messages": resume.data.stage_data.get("prior_messages"),
                                "suspension_type": "tool_suspended",
                                "question": &next_prompt,
                                "suggestions": &suggestions,
                                "tool_exchanges": resume.data.stage_data.get("tool_exchanges"),
                                "pending_op_prompts": pending_op_prompts,
                                "op_results": op_results,
                            });

                            <BuilderSolver as DomainSolver<BuilderDomain>>::store_suspension_data(
                                self,
                                SuspendedRunData {
                                    from_state: "solving".to_string(),
                                    original_input: resume.data.original_input.clone(),
                                    trace_id: String::new(),
                                    stage_data: next_stage_data,
                                    question: next_prompt.clone(),
                                    suggestions: suggestions.clone(),
                                },
                            );

                            return Err((
                                BuilderError::NeedsUserInput {
                                    prompt: next_prompt.clone(),
                                },
                                BackTarget::Suspend {
                                    reason: SuspendReason::HumanInput {
                                        questions: vec![HumanInputQuestion {
                                            prompt: next_prompt,
                                            suggestions,
                                        }],
                                    },
                                },
                            ));
                        }

                        // All files handled — build combined LLM resume message.
                        let resume_answer = op_results.join("\n");

                        let all_ops = extract_all_write_ops(&prior_messages);
                        let mut tool_exchanges: Vec<ToolExchange> = all_ops
                            .iter()
                            .map(|op| {
                                let (name, input) = match op {
                                    WriteOp::Write(a) => {
                                        ("write_file", serde_json::to_string(a).unwrap_or_default())
                                    }
                                    WriteOp::Edit(a) => {
                                        ("edit_file", serde_json::to_string(a).unwrap_or_default())
                                    }
                                    WriteOp::Delete(a) => (
                                        "delete_file",
                                        serde_json::to_string(a).unwrap_or_default(),
                                    ),
                                };
                                ToolExchange {
                                    name: name.to_string(),
                                    input,
                                    output: resume_answer.clone(),
                                }
                            })
                            .collect();
                        if tool_exchanges.is_empty() {
                            tool_exchanges.push(ToolExchange {
                                name: prompt_type.as_deref().unwrap_or("edit_file").to_string(),
                                input: question.clone(),
                                output: resume_answer.clone(),
                            });
                        }

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
                                Ok(val) => {
                                    for (file_path, new_content, description) in &val.files {
                                        emit_domain(
                                            &self.event_tx,
                                            BuilderEvent::FileChanged {
                                                file_path: file_path.clone(),
                                                description: description.clone(),
                                                new_content: new_content.clone(),
                                                old_content: String::new(),
                                                is_deletion: false,
                                            },
                                        )
                                        .await;
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
