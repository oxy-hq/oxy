mod automation_templates;
pub mod builders;
mod context;
pub mod formatters;
pub mod renderer;
// `types` relocated to `crate::exec_types` (Phase 3) so the type vocabulary
// survives the deletion of this module in Phase 4. `pub(crate)` because every
// out-of-crate consumer already imports `oxy::exec_types` directly — scoping the
// shim to the crate makes the compiler enforce that (a stray external
// `oxy::execute::types` no longer resolves). The remaining in-crate `super::types`
// refs are entangled with a separate `service::formatters::types`, so they ride
// the shim until their own modules relocate.
pub(crate) use crate::exec_types as types;
pub mod writer;

pub use context::{Executable, ExecutionContext, ExecutionContextBuilder};
use tracing::Instrument;
use writer::{BufWriter, EventHandler};

use oxy_shared::errors::OxyError;

pub async fn execute_with_handler<I, R>(
    executable: impl Executable<I, Response = R>,
    execution_context: &ExecutionContext,
    input: I,
    handler: impl EventHandler + Send + 'static,
) -> Result<R, OxyError> {
    let mut buf_writer = BufWriter::new();
    let mut executable = executable;
    let writer = buf_writer
        .create_writer(None)
        .map_err(|e| OxyError::RuntimeError(format!("Failed to create writer: {e}")))?;

    // Capture the current span to propagate trace context to the spawned task
    let current_span = tracing::Span::current();

    let event_handle = tokio::spawn(
        async move { buf_writer.write_to_handler(handler).await }.instrument(current_span.clone()),
    );
    let output = {
        let execution_context: ExecutionContext = execution_context.wrap_writer(writer);
        executable.execute(&execution_context, input).await
    }?;
    event_handle.await??;
    Ok(output)
}
