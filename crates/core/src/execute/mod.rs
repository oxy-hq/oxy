pub mod builders;
mod context;
pub mod renderer;
pub mod types;
pub mod writer;

pub use context::{Executable, ExecutionContext, ExecutionContextBuilder};
use writer::{BufWriter, EventHandler};

use crate::errors::OxyError;

pub async fn execute_with_handler<I, R>(
    executable: impl Executable<I, Response = R>,
    execution_context: &ExecutionContext,
    input: I,
    handler: impl EventHandler + Send + 'static,
) -> Result<R, OxyError> {
    let mut buf_writer = BufWriter::new();
    let mut executable = executable;
    let writer = buf_writer.create_writer(None)?;
    let event_handle = tokio::spawn(async move { buf_writer.write_to_handler(handler).await });
    let output = {
        let execution_context = execution_context.wrap_writer(writer);
        executable.execute(&execution_context, input).await
    }?;
    event_handle.await??;
    Ok(output)
}
