use std::path::Path;

use minijinja::Value;

use crate::{
    config::constants::EVAL_SOURCE_ROOT,
    errors::OxyError,
    execute::{
        Executable, ExecutionContext, ExecutionContextBuilder,
        types::Source,
        writer::{BufWriter, EventHandler},
    },
};
use eval::build_eval_executable;
use types::{EvalInput, EvalResult};

mod eval;
mod generator;
mod solver;
mod target;
pub mod types;

pub struct EvalLauncher {
    execution_context: Option<ExecutionContext>,
    buf_writer: BufWriter,
}

impl EvalLauncher {
    pub fn new() -> Self {
        Self {
            execution_context: None,
            buf_writer: BufWriter::new(),
        }
    }

    pub async fn with_project_path<P: AsRef<Path>>(
        mut self,
        project_path: P,
    ) -> Result<Self, OxyError> {
        self.execution_context = Some(
            ExecutionContextBuilder::new()
                .with_project_path(project_path)
                .await?
                .with_writer(self.buf_writer.create_writer(None)?)
                .with_global_context(Value::UNDEFINED)
                .with_source(Source {
                    parent_id: None,
                    id: "eval".to_string(),
                    kind: EVAL_SOURCE_ROOT.to_string(),
                })
                .build()?,
        );
        Ok(self)
    }

    pub async fn launch<H: EventHandler + Send + 'static>(
        self,
        eval_input: EvalInput,
        event_handler: H,
    ) -> Result<Vec<Result<EvalResult, OxyError>>, OxyError> {
        let execution_context = self.execution_context.ok_or(OxyError::RuntimeError(
            "ExecutionContext is required".to_string(),
        ))?;
        let mut eval_executable = build_eval_executable();
        let handle = tokio::spawn(async move {
            eval_executable
                .execute(&execution_context, eval_input)
                .await
        });
        let buf_writer = self.buf_writer;
        let event_handle =
            tokio::spawn(async move { buf_writer.write_to_handler(event_handler).await });
        let response = handle.await?;
        event_handle.await??;
        response
    }
}
