use std::fmt::{Debug, Formatter};

use async_trait::async_trait;
use cache::CacheExecutor;
use event::{Handler, consume};
use executor::{ChildExecutor, LoopExecutor, MapExecutor};
use tokio::sync::mpsc::Sender;
use value::ContextValue;
use write::{OutputCollector, Write};

use crate::{config::ConfigManager, errors::OxyError};

use super::renderer::{Renderer, TemplateRegister};

pub mod arrow_table;
pub mod cache;
pub mod event;
pub mod executor;
pub mod value;
pub mod write;

#[async_trait]
pub trait Executable<Input, Event>: Send + Sync {
    async fn execute(
        &self,
        execution_context: &mut ExecutionContext<'_, Event>,
        input: Input,
    ) -> Result<(), OxyError>;
}

pub struct ExecutionContext<'writer, Event> {
    writer: &'writer mut (dyn Write + Send + Sync + 'writer),
    sender: Sender<Event>,
    pub renderer: Renderer,
    pub config: ConfigManager,
}

impl<'writer, Event> ExecutionContext<'writer, Event>
where
    Event: Send + 'static,
{
    pub fn new(
        renderer: Renderer,
        writer: &'writer mut (dyn Write + Send + Sync + 'writer),
        config: ConfigManager,
        sender: Sender<Event>,
    ) -> Self {
        ExecutionContext {
            writer,
            renderer,
            config,
            sender,
        }
    }

    pub fn from_parts(
        parts: ExecutionContextParts<Event>,
        writer: &'writer mut (dyn Write + Send + Sync + 'writer),
    ) -> Self {
        ExecutionContext {
            writer,
            renderer: parts.renderer,
            config: parts.config,
            sender: parts.sender,
        }
    }

    pub fn map_executor<'context>(&'context mut self) -> MapExecutor<'context, 'writer, Event> {
        MapExecutor::new(self)
    }

    pub fn loop_executor<'context>(&'context mut self) -> LoopExecutor<'context, 'writer, Event> {
        LoopExecutor::new(self)
    }

    pub fn child_executor<'context>(&'context mut self) -> ChildExecutor<'context, 'writer, Event> {
        ChildExecutor::new(self)
    }

    pub fn cache_executor<'context>(&'context mut self) -> CacheExecutor<'context, 'writer, Event> {
        CacheExecutor::new(self)
    }

    pub async fn notify(&self, event: Event) -> Result<(), OxyError> {
        self.sender.send(event).await?;
        Ok(())
    }

    pub fn clone_parts(&self) -> ExecutionContextParts<Event> {
        ExecutionContextParts {
            sender: self.sender.clone(),
            renderer: self.renderer.clone(),
            config: self.config.clone(),
        }
    }

    pub fn get_sender(&self) -> Sender<Event> {
        self.sender.clone()
    }
}

impl<Event> Write for ExecutionContext<'_, Event> {
    fn write(&mut self, value: ContextValue) {
        self.writer.write(value);
    }
}

impl<Event> Debug for ExecutionContext<'_, Event> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ExecutionContext").finish()
    }
}

#[derive(Debug)]
pub struct ExecutionContextParts<Event> {
    pub sender: Sender<Event>,
    pub renderer: Renderer,
    pub config: ConfigManager,
}

impl<Event> ExecutionContextParts<Event> {
    pub fn with_renderer(&mut self, renderer: Renderer) -> &mut Self {
        self.renderer = renderer;
        self
    }

    pub fn with_sender<NewEvent>(
        self,
        sender: Sender<NewEvent>,
    ) -> ExecutionContextParts<NewEvent> {
        ExecutionContextParts {
            sender,
            renderer: self.renderer,
            config: self.config,
        }
    }
}

pub async fn run<Input, Event, T>(
    executable: &dyn Executable<Input, Event>,
    input: Input,
    config: ConfigManager,
    global_context: minijinja::Value,
    template_register: Option<&T>,
    handler: impl Handler<Event = Event> + 'static,
) -> Result<ContextValue, OxyError>
where
    Event: Send + 'static,
    T: TemplateRegister,
{
    let (tx, rx) = tokio::sync::mpsc::channel(100);
    let mut output_collector = OutputCollector::new();
    let events_handle = consume(rx, handler);
    {
        let renderer = match template_register {
            Some(template_register) => Renderer::from_template(global_context, template_register)?,
            None => Renderer::new(global_context),
        };
        let mut execution_context =
            ExecutionContext::new(renderer, &mut output_collector, config, tx);
        executable.execute(&mut execution_context, input).await?;
    }
    events_handle.await?;
    Ok(output_collector.output)
}
