use std::sync::Arc;

use super::artifact_tracker::ArtifactTracker;
use super::block_content::ContentProcessor;
use super::block_manager::BlockManager;
use super::block_reader::BlockHandlerReader;
use super::stream::StreamDispatcher;
use crate::config::constants::{
    AGENT_SOURCE, CONCURRENCY_SOURCE, CONSISTENCY_SOURCE, TASK_SOURCE, WORKFLOW_SOURCE,
};
use crate::errors::OxyError;
use crate::execute::formatters::{FormatterResult, SourceHandler};
use crate::execute::types::event::{ArtifactKind, SandboxInfo};
use crate::execute::types::{EventKind, Output, Source, Usage};
use crate::service::formatters::logs_persister::LogsPersister;
use crate::service::formatters::streaming_message_persister::StreamingMessagePersister;
use crate::service::types::{
    AnswerStream, ArtifactValue, ContainerKind, Content, ExecuteSQL, OmniQuery,
};
use crate::workflow::loggers::types::LogItem;
use tokio::sync::Mutex;
use tokio::sync::mpsc::Sender;

pub struct BlockHandler {
    block_manager: BlockManager,
    content_processor: ContentProcessor,
    stream_dispatcher: StreamDispatcher,
    artifact_tracker: ArtifactTracker,
    pub usage: Arc<Mutex<Usage>>,
    streaming_message_persister: Option<Arc<StreamingMessagePersister>>,
    logs_persister: Option<Arc<LogsPersister>>,
    current_omni_query: Option<crate::tools::types::OmniQueryParams>,
}

impl BlockHandler {
    pub fn new(sender: Sender<AnswerStream>) -> Self {
        Self {
            block_manager: BlockManager::new(),
            content_processor: ContentProcessor::new(),
            stream_dispatcher: StreamDispatcher::new(sender.clone()),
            artifact_tracker: ArtifactTracker::new(),
            usage: Arc::new(Mutex::new(Usage::new(0, 0))),
            streaming_message_persister: None,
            logs_persister: None,
            current_omni_query: None,
        }
    }

    pub fn with_streaming_persister(mut self, handler: Arc<StreamingMessagePersister>) -> Self {
        self.streaming_message_persister = Some(handler);
        self
    }

    pub fn with_logs_persister(mut self, handler: Arc<LogsPersister>) -> Self {
        self.logs_persister = Some(handler);
        self
    }

    pub fn get_reader(&self) -> BlockHandlerReader {
        BlockHandlerReader::new(
            self.block_manager.get_blocks_clone(),
            self.artifact_tracker.get_artifacts_clone(),
            self.usage.clone(),
            self.artifact_tracker.get_sandbox_info_clone(),
        )
    }

    async fn handle_artifact_started(
        &mut self,
        source: &Source,
        kind: &ArtifactKind,
        title: &str,
        is_verified: bool,
    ) -> Result<(), OxyError> {
        // Create container kind
        let container_kind = ContainerKind::Artifact {
            artifact_id: source.id.to_string(),
            kind: kind.to_string(),
            title: title.to_string(),
            is_verified,
        };

        // Add the block to the manager
        self.handle_container_started(source, &container_kind)
            .await?;

        // Register artifact
        self.artifact_tracker
            .start_artifact(source.id.clone(), kind.clone());

        // Send notification about artifact start
        self.stream_dispatcher
            .send_artifact_started(&source.id, title, kind, is_verified, &source.kind)
            .await?;

        Ok(())
    }

    async fn handle_artifact_finished(
        &mut self,
        source: &Source,
        error: Option<String>,
    ) -> Result<(), OxyError> {
        tracing::info!(
            "Handling artifact finished for source {}({}) with error: {:?}",
            source.kind,
            source.id,
            error
        );

        // Get the last block before it's closed (for artifact storage)
        let last_block = self.block_manager.last_block();
        if let Some(active_block) = last_block
            && active_block.is_artifact()
        {
            tracing::info!("Storing artifact for source {}({})", source.kind, source.id);
            // Store artifact
            self.artifact_tracker.store_artifact(active_block).await?;
        } else {
            tracing::warn!(
                "Skipping artifact storage for source {}({}): last_block exists={}, is_artifact={}",
                source.kind,
                source.id,
                last_block.is_some(),
                last_block.map(|b| b.is_artifact()).unwrap_or(false)
            );
        }

        // Finish the artifact
        if let Some(artifact_id) = self.artifact_tracker.finish_artifact() {
            self.stream_dispatcher
                .send_artifact_done(&artifact_id, error, &source.kind)
                .await?;
        }

        // Close the block
        self.handle_container_finished(source).await?;

        Ok(())
    }

    async fn handle_container_started(
        &mut self,
        source: &Source,
        kind: &ContainerKind,
    ) -> Result<(), OxyError> {
        // Prepare and send container opener
        let (opener, _) = self.content_processor.prepare_container(kind);
        let text = format!("\n{opener}\n");
        self.stream_dispatcher
            .send_text(text.clone(), &source.kind)
            .await?;

        if let Some(streaming_handler) = &self.streaming_message_persister {
            streaming_handler.append_content(&text).await?;
        }

        // If there's an active artifact, send artifact value
        if let Some((artifact_id, artifact_kind)) = self.artifact_tracker.get_active_artifact() {
            let is_artifact_active = self.block_manager.is_artifact_active(artifact_id); // Simplified for this example

            let artifact_value = match (artifact_kind, is_artifact_active) {
                (ArtifactKind::Agent { .. }, false) => {
                    Some(ArtifactValue::Content(format!("\n{opener}\n")))
                }
                (ArtifactKind::Workflow { .. }, false) => {
                    Some(ArtifactValue::LogItem(LogItem::info(kind.to_string())))
                }
                _ => None,
            };

            if let Some(value) = artifact_value {
                self.stream_dispatcher
                    .send_artifact_value(artifact_id, value, &source.kind)
                    .await?;
            }
        }

        // Add the block to the manager
        self.block_manager.add_container_block(source, kind).await?;

        Ok(())
    }

    async fn handle_container_finished(&mut self, source: &Source) -> Result<(), OxyError> {
        // Close the block
        let is_closed = self.block_manager.finish_block(source).await?;
        tracing::info!(
            "Block finished for source {}({}): is_closed={}",
            source.kind,
            source.id,
            is_closed
        );
        if !is_closed {
            return Ok(());
        }

        // Send the closing marker
        if let Some(closer) = self.content_processor.get_next_closer() {
            let text = format!("\n{closer}\n");
            self.stream_dispatcher
                .send_text(text.clone(), &source.kind)
                .await?;

            if let Some(streaming_handler) = &self.streaming_message_persister {
                streaming_handler.append_content(&text).await?;
            }

            // If there's an active artifact, send container closer
            if let Some((artifact_id, artifact_kind)) = self.artifact_tracker.get_active_artifact()
            {
                let is_artifact_active = self.block_manager.is_artifact_active(artifact_id); // Simplified for this example

                let artifact_value = match (artifact_kind, is_artifact_active) {
                    (ArtifactKind::Agent { .. }, false) => {
                        Some(ArtifactValue::Content(format!("\n{closer}\n")))
                    }
                    _ => None,
                };

                if let Some(value) = artifact_value {
                    self.stream_dispatcher
                        .send_artifact_value(artifact_id, value, &source.kind)
                        .await?;
                }
            }
        }

        Ok(())
    }

    async fn handle_content_update(
        &mut self,
        source: &Source,
        chunk: &crate::execute::types::Chunk,
    ) -> Result<(), OxyError> {
        tracing::info!(
            "Handling content update for source {}({}): finished={}",
            source.kind,
            source.id,
            chunk.finished
        );
        if chunk.finished {
            // Process the final chunk
            if let Some(content) = self.block_manager.finalize_content(&chunk.delta)
                && let Some(processed_content) = self.content_processor.output_to_content(&content)
            {
                // Add the content to our blocks
                self.block_manager
                    .add_content(source, processed_content)
                    .await?;
            }
        } else {
            // Update the active content with this chunk
            self.block_manager.update_content(&chunk.delta);
        }

        // Convert the output to text format for streaming
        if let Some(text) = self.content_processor.output_to_text(&chunk.delta) {
            // Send to the main output stream
            self.stream_dispatcher
                .send_text(text.clone(), &source.kind)
                .await?;

            if let Some(streaming_handler) = &self.streaming_message_persister {
                streaming_handler.append_content(&text).await?;
            }

            // If there's an active artifact, send to artifact stream
            if let Some((artifact_id, artifact_kind)) = self.artifact_tracker.get_active_artifact()
            {
                match artifact_kind {
                    ArtifactKind::Workflow { .. } => {
                        let log_item = if chunk.finished {
                            LogItem::info(text)
                        } else {
                            LogItem::append(text)
                        };

                        self.stream_dispatcher
                            .send_artifact_value(
                                artifact_id,
                                ArtifactValue::LogItem(log_item),
                                &source.kind,
                            )
                            .await?;
                    }
                    ArtifactKind::Agent { .. } => {
                        self.stream_dispatcher
                            .send_artifact_value(
                                artifact_id,
                                ArtifactValue::Content(text),
                                &source.kind,
                            )
                            .await?;
                    }
                    ArtifactKind::ExecuteSQL { .. } => match &chunk.delta {
                        Output::SQL(sql) => {
                            self.stream_dispatcher
                                .send_artifact_value(
                                    artifact_id,
                                    ArtifactValue::ExecuteSQL(ExecuteSQL {
                                        database: "".to_string(),
                                        is_result_truncated: false,
                                        result: vec![],
                                        sql_query: sql.to_string(),
                                    }),
                                    &source.kind,
                                )
                                .await?;
                        }
                        Output::Table(table) => {
                            if let Some(reference) = &table.reference {
                                let (table_2d_array, is_truncated) = table.to_2d_array()?;
                                self.stream_dispatcher
                                    .send_artifact_value(
                                        artifact_id,
                                        ArtifactValue::ExecuteSQL(ExecuteSQL {
                                            database: reference.database_ref.to_string(),
                                            is_result_truncated: is_truncated,
                                            result: table_2d_array,
                                            sql_query: reference.sql.to_string(),
                                        }),
                                        &source.kind,
                                    )
                                    .await?;
                            }
                        }
                        _ => {}
                    },
                    ArtifactKind::SemanticQuery {} => match &chunk.delta {
                        Output::SemanticQuery(data) => {
                            // Emit semantic query params
                            self.stream_dispatcher
                                .send_artifact_value(
                                    artifact_id,
                                    ArtifactValue::SemanticQuery(data.clone()),
                                    &source.kind,
                                )
                                .await?;
                        }
                        _ => {}
                    },
                    ArtifactKind::OmniQuery { topic, .. } => {
                        if let Output::Table(table) = &chunk.delta {
                            let (table_2d_array, is_truncated) = table.to_2d_array()?;
                            let query_params =
                                self.current_omni_query.clone().unwrap_or_else(|| {
                                    crate::tools::types::OmniQueryParams {
                                        fields: vec![],
                                        limit: None,
                                        sorts: None,
                                    }
                                });

                            let sorts_map = query_params.sorts.as_ref().map(|sorts| {
                                sorts
                                    .iter()
                                    .map(|(k, v)| {
                                        (
                                            k.clone(),
                                            match v {
                                                crate::tools::types::OrderType::Ascending => {
                                                    "asc".to_string()
                                                }
                                                crate::tools::types::OrderType::Descending => {
                                                    "desc".to_string()
                                                }
                                            },
                                        )
                                    })
                                    .collect()
                            });

                            self.stream_dispatcher
                                .send_artifact_value(
                                    artifact_id,
                                    ArtifactValue::OmniQuery(OmniQuery {
                                        result: table_2d_array,
                                        is_result_truncated: is_truncated,
                                        topic: topic.clone(),
                                        fields: query_params.fields,
                                        limit: query_params.limit,
                                        sorts: sorts_map,
                                    }),
                                    &source.kind,
                                )
                                .await?;
                        }
                    }
                    ArtifactKind::SandboxApp { .. } => {
                        // Sandbox apps don't have streaming content
                        // The preview_url is known upfront when the artifact is created
                    }
                }
            }
        }

        Ok(())
    }
}

#[async_trait::async_trait]
impl SourceHandler for BlockHandler {
    fn excluded_source_kinds(&self) -> Vec<String> {
        vec![
            CONCURRENCY_SOURCE.to_string(),
            CONSISTENCY_SOURCE.to_string(),
        ]
    }

    async fn handle_event(&mut self, source: &Source, event_kind: &EventKind) -> FormatterResult {
        if let Some(logs_persister) = &self.logs_persister {
            logs_persister.save_log(source, event_kind).await?;
        }

        match event_kind {
            EventKind::ArtifactStarted {
                kind,
                title,
                is_verified,
            } => {
                tracing::info!(
                    "Handling artifact started: source_id={}, kind={}, title={}, is_verified={}",
                    source.id,
                    kind,
                    title,
                    is_verified
                );
                self.handle_artifact_started(source, kind, title, *is_verified)
                    .await?;
            }

            EventKind::ArtifactFinished { error } => {
                tracing::info!(
                    "Handling artifact finished: source_id={}, kind={}, error={:?}",
                    source.id,
                    source.kind,
                    error
                );
                self.handle_artifact_finished(source, error.clone()).await?;
            }

            EventKind::Started {
                name,
                attributes: _,
            } => {
                // Process the block event
                let container_kind = match source.kind.as_str() {
                    WORKFLOW_SOURCE => Some(ContainerKind::Workflow {
                        r#ref: source.id.clone(),
                    }),
                    AGENT_SOURCE => Some(ContainerKind::Agent {
                        r#ref: source.id.clone(),
                    }),
                    TASK_SOURCE => Some(ContainerKind::Task { name: name.clone() }),
                    _ => None,
                };

                if let Some(kind) = container_kind
                    && self.artifact_tracker.has_active_artifact()
                {
                    tracing::info!(
                        "Handling container started for active artifact: source_id={}, kind={}",
                        source.id,
                        kind
                    );
                    self.handle_container_started(source, &kind).await?;
                }
            }

            EventKind::Finished { .. } => {
                self.handle_container_finished(source).await?;
            }

            EventKind::Updated { chunk } => {
                self.handle_content_update(source, chunk).await?;
            }

            EventKind::Usage { usage } => {
                // Update the usage statistics
                let mut current_usage = self.usage.lock().await;
                current_usage.add(usage);
                self.stream_dispatcher
                    .send_usage(usage.clone(), &source.kind)
                    .await?;

                if let Some(streaming_handler) = &self.streaming_message_persister {
                    streaming_handler
                        .update_usage(current_usage.input_tokens, current_usage.output_tokens)
                        .await?;
                }
            }

            EventKind::SandboxAppCreated { preview_url, kind } => {
                // Store the sandbox preview URL for later use in artifact creation
                self.artifact_tracker
                    .set_sandbox_info(kind.clone(), preview_url.clone())
                    .await;
                self.block_manager
                    .add_content(
                        source,
                        Content::SandboxInfo(SandboxInfo {
                            preview_url: preview_url.clone(),
                            kind: kind.clone(),
                        }),
                    )
                    .await?;
                if let Some((artifact_id, artifact_kind)) =
                    self.artifact_tracker.get_active_artifact()
                    && let ArtifactKind::SandboxApp { .. } = artifact_kind
                {
                    self.stream_dispatcher
                        .send_artifact_value(
                            artifact_id,
                            ArtifactValue::SandboxInfo(SandboxInfo {
                                preview_url: preview_url.clone(),
                                kind: kind.clone(),
                            }),
                            &source.kind,
                        )
                        .await?;
                }
            }
            _ => {}
        }

        Ok(())
    }
}
