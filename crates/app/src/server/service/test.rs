use std::collections::HashMap;
use std::path::Path;

use crate::integrations::eval::EvalResult;
use crate::server::service::test_runs::{InsertCaseData, TestRunsManager};
use futures::Stream;
use oxy::adapters::project::manager::ProjectManager;
use oxy::config::constants::EVAL_SOURCE;
use oxy::execute::types::{Event, EventKind, ProgressType};
use oxy::execute::writer::EventHandler;
use oxy_shared::errors::OxyError;
use serde::Serialize;
use tokio::task::JoinHandle;
use tokio_stream::wrappers::ReceiverStream;
use uuid::Uuid;

use super::eval::run_eval;

#[derive(Serialize, Clone)]
#[serde(tag = "type")]
pub enum EvalEvent {
    Started,
    Progress {
        id: String,
        progress: usize,
        total: usize,
    },
    Finished {
        metric: EvalResult,
    },
}

#[derive(Serialize, Clone)]
pub struct TestStreamMessage {
    pub error: Option<String>,
    pub event: Option<EvalEvent>,
}

struct EvalEventsHandler {
    tx: tokio::sync::mpsc::Sender<TestStreamMessage>,
    progress: HashMap<String, usize>,
    total: HashMap<String, Option<usize>>,
}

#[async_trait::async_trait]
impl EventHandler for EvalEventsHandler {
    async fn handle_event(&mut self, event: Event) -> Result<(), OxyError> {
        if event.source.kind.as_str() == EVAL_SOURCE {
            match event.kind {
                EventKind::Started { .. } => {
                    self.tx
                        .send(TestStreamMessage {
                            error: None,
                            event: Some(EvalEvent::Started),
                        })
                        .await?;
                }
                EventKind::Progress { progress } => {
                    let (id, progress, total) = match progress {
                        ProgressType::Started(total) => {
                            let id = event.source.id;
                            self.progress.insert(id.clone(), 0);
                            self.total.insert(id.clone(), total);
                            (id, 0, total.unwrap_or(0))
                        }
                        ProgressType::Updated(inc) => {
                            let id = event.source.id;
                            let progress = self.progress.entry(id.clone()).or_insert(0);
                            *progress += inc;
                            let total = self.total.get(&id).cloned().unwrap_or(None);
                            (id, *progress, total.unwrap_or(*progress))
                        }
                        ProgressType::Finished => {
                            let id = event.source.id;
                            let progress = self.progress.remove(&id).unwrap_or(0);
                            let total = self.total.remove(&id).unwrap_or(None);
                            (id, progress, total.unwrap_or(progress))
                        }
                    };
                    self.tx
                        .send(TestStreamMessage {
                            error: None,
                            event: Some(EvalEvent::Progress {
                                id,
                                progress,
                                total,
                            }),
                        })
                        .await?;
                }
                _ => {}
            }
        }
        Ok(())
    }
}

pub struct TestCasePersistContext {
    pub project_id: Uuid,
    pub test_run_id: Uuid,
    pub case_index: usize,
    pub prompt: String,
    pub expected: String,
}

pub async fn run_test<P: AsRef<Path> + Send + 'static>(
    project_manager: ProjectManager,
    target_ref: P,
    index: usize,
    persist: Option<TestCasePersistContext>,
) -> Result<impl Stream<Item = TestStreamMessage>, OxyError> {
    let (tx, rx) = tokio::sync::mpsc::channel(100);
    let response_tx = tx.clone();
    let event_handler = EvalEventsHandler {
        tx,
        progress: HashMap::new(),
        total: HashMap::new(),
    };
    let _: JoinHandle<()> = tokio::spawn(async move {
        match run_eval(project_manager, target_ref, Some(index), event_handler).await {
            Ok(response) => {
                for metric in response.iter() {
                    // Persist case result if a run context was provided
                    if let Some(ref ctx) = persist
                        && let Err(e) = persist_case_result(ctx, metric).await
                    {
                        tracing::warn!("Failed to persist test case result: {e}");
                    }
                    if response_tx
                        .send(TestStreamMessage {
                            error: None,
                            event: Some(EvalEvent::Finished {
                                metric: metric.clone(),
                            }),
                        })
                        .await
                        .is_err()
                    {
                        break;
                    }
                }
            }
            Err(e) => {
                let error_msg = e.to_string();
                tracing::error!("Test eval failed: {error_msg}");
                let _ = response_tx
                    .send(TestStreamMessage {
                        error: Some(error_msg),
                        event: None,
                    })
                    .await;
            }
        }
    });

    Ok(ReceiverStream::new(rx))
}

async fn persist_case_result(
    ctx: &TestCasePersistContext,
    result: &EvalResult,
) -> Result<(), OxyError> {
    use crate::integrations::eval::MetricKind;

    // The correctness judge emits binary scores (1.0 = PASS, 0.0 = FAIL),
    // so any value in (0.0, 1.0) works here. This only gates the
    // passing_runs / total_runs consistency counters, not the displayed score.
    const PASS_THRESHOLD: f32 = 0.5;

    let manager = TestRunsManager::new(ctx.project_id).await?;

    // Extract metrics from the first metric entry (Correctness or Similarity)
    let (
        score,
        passing_runs,
        total_runs,
        avg_duration_ms,
        input_tokens,
        output_tokens,
        reasoning,
        actual_output,
    ) = if let Some(metric) = result.metrics.first() {
        match metric {
            MetricKind::Correctness(c) => {
                let records = &c.records;
                let total = records.len() as i32;
                let passing = records.iter().filter(|r| r.score >= PASS_THRESHOLD).count() as i32;
                let avg_dur = if records.is_empty() {
                    None
                } else {
                    Some(records.iter().map(|r| r.duration_ms).sum::<f64>() / records.len() as f64)
                };
                let in_tok: i32 = records.iter().map(|r| r.input_tokens).sum();
                let out_tok: i32 = records.iter().map(|r| r.output_tokens).sum();
                let cots: Vec<String> = records.iter().map(|r| r.cot.clone()).collect();
                let actual = records.first().and_then(|r| r.actual_output.clone());
                (
                    c.score as f64,
                    passing,
                    total,
                    avg_dur,
                    Some(in_tok),
                    Some(out_tok),
                    Some(serde_json::to_value(cots).unwrap_or_default()),
                    actual,
                )
            }
            MetricKind::Similarity(s) => {
                let records = &s.records;
                let total = records.len() as i32;
                let passing = records.iter().filter(|r| r.score >= PASS_THRESHOLD).count() as i32;
                let avg_dur = if records.is_empty() {
                    None
                } else {
                    Some(records.iter().map(|r| r.duration_ms).sum::<f64>() / records.len() as f64)
                };
                let in_tok: i32 = records.iter().map(|r| r.input_tokens).sum();
                let out_tok: i32 = records.iter().map(|r| r.output_tokens).sum();
                let cots: Vec<String> = records.iter().map(|r| r.cot.clone()).collect();
                let actual = records.first().and_then(|r| r.actual_output.clone());
                (
                    s.score as f64,
                    passing,
                    total,
                    avg_dur,
                    Some(in_tok),
                    Some(out_tok),
                    Some(serde_json::to_value(cots).unwrap_or_default()),
                    actual,
                )
            }
            MetricKind::Recall(r) => {
                let passing = r.records.iter().filter(|rec| rec.pass).count() as i32;
                let total = r.records.len() as i32;
                (r.score as f64, passing, total, None, None, None, None, None)
            }
        }
    } else {
        (0.0, 0, 0, None, None, None, None, None)
    };

    let verdict = if total_runs == 0 {
        "fail".to_string()
    } else if passing_runs == total_runs {
        "pass".to_string()
    } else if passing_runs == 0 {
        "fail".to_string()
    } else {
        "flaky".to_string()
    };

    let errors_json = if result.errors.is_empty() {
        None
    } else {
        serde_json::to_value(&result.errors).ok()
    };

    manager
        .insert_case(
            ctx.test_run_id,
            InsertCaseData {
                case_index: ctx.case_index as i32,
                prompt: ctx.prompt.clone(),
                expected: ctx.expected.clone(),
                actual_output,
                score,
                verdict,
                passing_runs,
                total_runs,
                avg_duration_ms,
                input_tokens,
                output_tokens,
                judge_reasoning: reasoning,
                errors: errors_json,
            },
        )
        .await
}
