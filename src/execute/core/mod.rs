use async_trait::async_trait;
use executor::{LoopExecutor, MapExecutor};
use minijinja::{context, Value};
use value::ContextValue;

use crate::errors::OnyxError;

use super::renderer::Renderer;

pub mod arrow_table;
pub mod executor;
pub mod value;

#[async_trait]
pub trait Executable: Send + Sync {
    async fn execute(&self, execution_context: &mut ExecutionContext<'_>) -> Result<(), OnyxError>;
}

pub trait Write: Send + Sync {
    fn write(&mut self, value: ContextValue);
}

pub struct ExecutionContext<'writer> {
    key: Option<String>,
    context: Value,
    global_context: &'writer Value,
    writer: &'writer mut (dyn Write + 'writer),
    pub renderer: &'writer mut Renderer,
}

impl<'writer> ExecutionContext<'writer> {
    fn wrap<'current, 'wrapped, F>(
        &'current mut self,
        wrap: F,
        key: Option<String>,
        context: Value,
    ) -> ExecutionContext<'wrapped>
    where
        'current: 'wrapped,
        F: FnOnce(&'current mut (dyn Write + 'current)) -> &'wrapped mut (dyn Write + 'wrapped),
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
        writer: &'writer mut (dyn Write + 'writer),
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

    pub fn map_executor<'context>(&'context mut self) -> MapExecutor<'context, 'writer> {
        MapExecutor::new(self)
    }

    pub fn loop_executor<'context>(&'context mut self) -> LoopExecutor<'context, 'writer> {
        LoopExecutor::new(self)
    }
}

impl Write for ExecutionContext<'_> {
    fn write(&mut self, value: ContextValue) {
        self.writer.write(value);
    }
}

#[derive(Debug, Default)]
pub struct OutputCollector {
    pub output: Option<ContextValue>,
}

impl Write for OutputCollector {
    fn write(&mut self, value: ContextValue) {
        self.output = Some(value);
    }
}
