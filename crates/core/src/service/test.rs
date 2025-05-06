use std::collections::HashMap;
use std::path::Path;

use crate::config::constants::EVAL_SOURCE;
use crate::errors::OxyError;
use crate::eval::EvalResult;
use crate::execute::types::{Event, EventKind, ProgressType};
use crate::execute::writer::EventHandler;
use futures::Stream;
use serde::Serialize;
use tokio::task::JoinHandle;
use tokio_stream::wrappers::ReceiverStream;

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

pub async fn run_test<P: AsRef<Path> + Send + 'static>(
    project_path: P,
    target_ref: P,
    index: usize,
) -> Result<impl Stream<Item = TestStreamMessage>, OxyError> {
    let (tx, rx) = tokio::sync::mpsc::channel(100);
    let response_tx = tx.clone();
    let event_handler = EvalEventsHandler {
        tx,
        progress: HashMap::new(),
        total: HashMap::new(),
    };
    let _: JoinHandle<Result<Vec<EvalResult>, OxyError>> = tokio::spawn(async move {
        let response = run_eval(project_path, target_ref, Some(index), event_handler).await?;
        for metric in response.iter() {
            response_tx
                .send(TestStreamMessage {
                    error: None,
                    event: Some(EvalEvent::Finished {
                        metric: metric.clone(),
                    }),
                })
                .await
                .map_err(|_err| OxyError::RuntimeError("Failed to send eval event".to_string()))?;
        }
        Ok(response)
    });

    Ok(ReceiverStream::new(rx))
}
