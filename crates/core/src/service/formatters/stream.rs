use tokio::sync::mpsc::Sender;

use crate::{
    errors::OxyError,
    execute::types::{Usage, event::ArtifactKind},
    service::types::{AnswerContent, AnswerStream, ArtifactValue},
};

pub struct StreamDispatcher {
    sender: Sender<AnswerStream>,
}

impl StreamDispatcher {
    pub fn new(sender: Sender<AnswerStream>) -> Self {
        Self { sender }
    }

    pub async fn send_text(&self, content: String, step: &str) -> Result<(), OxyError> {
        let _ = self
            .sender
            .send(AnswerStream {
                content: AnswerContent::Text { content },
                references: vec![],
                is_error: false,
                step: step.to_string(),
            })
            .await
            .map_err(|_| ());
        Ok(())
    }

    pub async fn send_artifact_started(
        &self,
        id: &str,
        title: &str,
        kind: &ArtifactKind,
        is_verified: bool,
        step: &str,
    ) -> Result<(), OxyError> {
        let _ = self
            .sender
            .send(AnswerStream {
                content: AnswerContent::ArtifactStarted {
                    id: id.to_string(),
                    title: title.to_string(),
                    kind: kind.clone(),
                    is_verified,
                },
                references: vec![],
                is_error: false,
                step: step.to_string(),
            })
            .await
            .map_err(|_| ());
        Ok(())
    }

    pub async fn send_artifact_done(
        &self,
        id: &str,
        error: Option<String>,
        step: &str,
    ) -> Result<(), OxyError> {
        let _ = self
            .sender
            .send(AnswerStream {
                content: AnswerContent::ArtifactDone {
                    id: id.to_string(),
                    error,
                },
                references: vec![],
                is_error: false,
                step: step.to_string(),
            })
            .await
            .map_err(|_| ());
        Ok(())
    }

    pub async fn send_artifact_value(
        &self,
        id: &str,
        value: ArtifactValue,
        step: &str,
    ) -> Result<(), OxyError> {
        let _ = self
            .sender
            .send(AnswerStream {
                content: AnswerContent::ArtifactValue {
                    id: id.to_string(),
                    value,
                },
                references: vec![],
                is_error: false,
                step: step.to_string(),
            })
            .await
            .map_err(|_| ());
        Ok(())
    }

    pub async fn send_usage(&self, usage: Usage, step: &str) -> Result<(), OxyError> {
        let _ = self
            .sender
            .send(AnswerStream {
                content: AnswerContent::Usage { usage },
                references: vec![],
                is_error: false,
                step: step.to_string(),
            })
            .await
            .map_err(|_| ());
        Ok(())
    }
}
