use std::{fmt::Debug, path::Path};

use minijinja::Value;
use tokio::sync::mpsc::Sender;

use crate::{
    adapters::checkpoint::CheckpointContext,
    config::{ConfigBuilder, ConfigManager},
    errors::OxyError,
    execute::renderer::Renderer,
};

use super::{
    renderer::TemplateRegister,
    types::{Chunk, Event, EventKind, ProgressType, Source},
    writer::Writer,
};

#[async_trait::async_trait]
pub trait Executable<I> {
    type Response: Send;

    async fn execute(
        &mut self,
        execution_context: &ExecutionContext,
        input: I,
    ) -> Result<Self::Response, OxyError>;
}

#[derive(Debug, Clone)]
pub struct ExecutionContext {
    pub source: Source,
    pub writer: Sender<Event>,
    pub renderer: Renderer,
    pub config: ConfigManager,
    pub checkpoint: Option<CheckpointContext>,
}

impl ExecutionContext {
    pub fn new(
        source: Source,
        renderer: Renderer,
        config: ConfigManager,
        writer: Sender<Event>,
        checkpoint: Option<CheckpointContext>,
    ) -> Self {
        ExecutionContext {
            source,
            writer,
            renderer,
            config,
            checkpoint,
        }
    }

    pub fn with_child_source(&self, source_id: String, kind: String) -> Self {
        ExecutionContext {
            source: Source {
                parent_id: Some(self.source.id.clone()),
                id: source_id,
                kind,
            },
            writer: self.writer.clone(),
            renderer: self.renderer.clone(),
            config: self.config.clone(),
            checkpoint: self.checkpoint.clone(),
        }
    }

    pub fn with_checkpoint(&self, checkpoint: CheckpointContext) -> Self {
        ExecutionContext {
            source: self.source.clone(),
            writer: self.writer.clone(),
            renderer: self.renderer.clone(),
            config: self.config.clone(),
            checkpoint: Some(checkpoint),
        }
    }

    pub fn wrap_writer(&self, writer: Sender<Event>) -> ExecutionContext {
        ExecutionContext {
            source: self.source.clone(),
            writer,
            renderer: self.renderer.clone(),
            config: self.config.clone(),
            checkpoint: self.checkpoint.clone(),
        }
    }

    pub fn wrap_renderer(&self, renderer: Renderer) -> Self {
        ExecutionContext {
            source: self.source.clone(),
            writer: self.writer.clone(),
            renderer,
            config: self.config.clone(),
            checkpoint: self.checkpoint.clone(),
        }
    }

    pub fn wrap_render_context(&self, context: &Value) -> Self {
        ExecutionContext {
            source: self.source.clone(),
            writer: self.writer.clone(),
            renderer: self.renderer.wrap(context),
            config: self.config.clone(),
            checkpoint: self.checkpoint.clone(),
        }
    }

    pub async fn write_kind(&self, event: EventKind) -> Result<(), OxyError> {
        self.write(Event {
            source: self.source.clone(),
            kind: event,
        })
        .await
    }

    pub async fn write_chunk(&self, chunk: Chunk) -> Result<(), OxyError> {
        self.write_kind(EventKind::Updated { chunk }).await
    }

    pub async fn write_data_app(
        &self,
        data_app: crate::execute::types::event::DataApp,
    ) -> Result<(), OxyError> {
        self.write_kind(EventKind::DataAppCreated { data_app })
            .await
    }

    pub async fn write_progress(&self, progress: ProgressType) -> Result<(), OxyError> {
        self.write_kind(EventKind::Progress { progress }).await
    }
}

#[async_trait::async_trait]
impl Writer for ExecutionContext {
    async fn write(&self, event: Event) -> Result<(), OxyError> {
        self.writer
            .send(event)
            .await
            .map_err(|err| OxyError::RuntimeError(format!("Failed to send event:\n{}", err)))
    }
}

pub struct ExecutionContextBuilder {
    source: Option<Source>,
    renderer: Option<Renderer>,
    config: Option<ConfigManager>,
    writer: Option<Sender<Event>>,
    checkpoint: Option<CheckpointContext>,
}

impl Default for ExecutionContextBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl ExecutionContextBuilder {
    pub fn new() -> Self {
        ExecutionContextBuilder {
            source: None,
            renderer: None,
            config: None,
            writer: None,
            checkpoint: None,
        }
    }

    pub fn with_template<T: TemplateRegister>(
        mut self,
        global_context: Value,
        template_register: &T,
    ) -> Result<Self, OxyError> {
        self.renderer = Some(Renderer::from_template(global_context, template_register)?);
        Ok(self)
    }

    pub fn with_global_context(mut self, global_context: Value) -> Self {
        self.renderer = Some(Renderer::new(global_context));
        self
    }

    pub fn with_config_manager(mut self, config: ConfigManager) -> Self {
        self.config = Some(config);
        self
    }

    pub async fn with_project_path<P: AsRef<Path>>(
        mut self,
        project_path: P,
    ) -> Result<Self, OxyError> {
        self.config = Some(
            ConfigBuilder::new()
                .with_project_path(project_path)?
                .build()
                .await?,
        );
        Ok(self)
    }

    pub fn with_source(mut self, source: Source) -> Self {
        self.source = Some(source);
        self
    }

    pub fn with_checkpoint(mut self, checkpoint: CheckpointContext) -> Self {
        self.checkpoint = Some(checkpoint);
        self
    }

    pub fn with_writer(mut self, tx: Sender<Event>) -> Self {
        self.writer = Some(tx);
        self
    }

    pub fn build(self) -> Result<ExecutionContext, OxyError> {
        let source = self
            .source
            .ok_or(OxyError::RuntimeError("Source is required".to_string()))?;
        let writer = self
            .writer
            .ok_or(OxyError::RuntimeError("Writer is required".to_string()))?;
        let renderer = self
            .renderer
            .ok_or(OxyError::RuntimeError("Renderer is required".to_string()))?;
        let config = self.config.ok_or(OxyError::RuntimeError(
            "ConfigManager is required".to_string(),
        ))?;

        Ok(ExecutionContext {
            source,
            writer,
            renderer,
            config,
            checkpoint: self.checkpoint,
        })
    }
}
