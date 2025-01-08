use std::fmt::{Debug, Formatter};

use async_trait::async_trait;
use executor::{LoopExecutor, MapExecutor};
use minijinja::{context, Value};
use value::ContextValue;
use write::Write;

use crate::errors::OnyxError;

use super::renderer::Renderer;

pub mod arrow_table;
pub mod event;
pub mod executor;
pub mod value;
pub mod write;

#[async_trait]
pub trait Executable<Event>: Send + Sync {
    async fn execute(
        &self,
        execution_context: &mut ExecutionContext<'_, Event>,
    ) -> Result<(), OnyxError>;
}

pub struct ExecutionContext<'writer, Event> {
    key: Option<String>,
    context: Value,
    global_context: &'writer Value,
    writer: &'writer mut (dyn Write<Event> + 'writer),
    pub renderer: &'writer mut Renderer,
}

impl<'writer, Event> ExecutionContext<'writer, Event> {
    fn wrap<'current, 'wrapped, F>(
        &'current mut self,
        wrap: F,
        key: Option<String>,
        context: Value,
    ) -> ExecutionContext<'wrapped, Event>
    where
        'current: 'wrapped,
        F: FnOnce(
            &'current mut (dyn Write<Event> + 'current),
        ) -> &'wrapped mut (dyn Write<Event> + 'wrapped),
    {
        ExecutionContext {
            key,
            writer: wrap(self.writer),
            global_context: self.global_context,
            renderer: self.renderer,
            context,
        }
    }

    pub fn new(
        context: Value,
        renderer: &'writer mut Renderer,
        global_context: &'writer Value,
        writer: &'writer mut (dyn Write<Event> + 'writer),
    ) -> Self {
        ExecutionContext {
            key: None,
            context,
            global_context,
            writer,
            renderer,
        }
    }

    pub fn get_context(&self) -> Value {
        context! {
          ..Value::from_serialize(self.global_context),
          ..Value::from_serialize(&self.context),
        }
    }

    pub fn get_context_str(&self) -> String {
        self.context.to_string()
    }

    pub fn map_executor<'context>(&'context mut self) -> MapExecutor<'context, 'writer, Event> {
        MapExecutor::new(self)
    }

    pub fn loop_executor<'context>(&'context mut self) -> LoopExecutor<'context, 'writer, Event> {
        LoopExecutor::new(self)
    }
}

impl<T> Write<T> for ExecutionContext<'_, T> {
    fn write(&mut self, value: ContextValue) {
        self.writer.write(value);
    }

    fn notify(&self, event: T) {
        self.writer.notify(event);
    }
}

impl<T> Debug for ExecutionContext<'_, T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ExecutionContext")
            .field("key", &self.key)
            .field("context", &self.context)
            .finish()
    }
}
