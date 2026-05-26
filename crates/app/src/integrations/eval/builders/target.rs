use std::sync::{Arc, Mutex};

use itertools::Itertools;

use agentic_analytics::AnalyticsEvent;
use agentic_core::events::{CoreEvent, Event as AgenticEvent};
use oxy::execute::{
    Executable, ExecutionContext, execute_with_handler,
    types::{
        Event, EventKind, Output, OutputContainer, OutputGetter, RelevantContextGetter,
        TargetOutput, Usage,
    },
    writer::EventHandler,
};
use oxy_shared::errors::OxyError;
use tokio::sync::mpsc;

use super::types::AgenticInput;

// Accumulates token usage from EventKind::Usage events, forwarding all events to the inner handler.
struct UsageAccumulatorHandler<H> {
    inner: H,
    usage_in: Arc<Mutex<i32>>,
    usage_out: Arc<Mutex<i32>>,
}

impl<H> UsageAccumulatorHandler<H> {
    fn new(inner: H) -> Self {
        Self {
            inner,
            usage_in: Arc::new(Mutex::new(0)),
            usage_out: Arc::new(Mutex::new(0)),
        }
    }
}

#[async_trait::async_trait]
impl<H: EventHandler + Send + 'static> EventHandler for UsageAccumulatorHandler<H> {
    async fn handle_event(&mut self, event: Event) -> Result<(), OxyError> {
        if let EventKind::Usage { usage } = &event.kind {
            *self.usage_in.lock().unwrap() += usage.input_tokens;
            *self.usage_out.lock().unwrap() += usage.output_tokens;
        }
        self.inner.handle_event(event).await
    }
}

// Runs an agentic eval target via the agentic pipeline.
struct AgenticTargetWrapper;

#[async_trait::async_trait]
impl Executable<AgenticInput> for AgenticTargetWrapper {
    type Response = OutputContainer;

    async fn execute(
        &mut self,
        ctx: &ExecutionContext,
        input: AgenticInput,
    ) -> Result<Self::Response, OxyError> {
        // Resolve the config path to absolute via the project manager so
        // AgentConfig::from_file reads from the right location regardless
        // of the process CWD.
        let resolved = ctx
            .workspace
            .config_manager
            .resolve_file(&input.config_path)
            .await
            .map_err(|e| {
                OxyError::ConfigurationError(format!(
                    "Failed to resolve agentic config path '{}': {e}",
                    input.config_path
                ))
            })?;
        let project_ctx = std::sync::Arc::new(crate::agentic_wiring::OxyProjectContext::new(
            ctx.workspace.clone(),
        ));
        let platform: std::sync::Arc<dyn agentic_pipeline::platform::PlatformContext> = project_ctx;

        // Capture token usage from CoreEvent::LlmStart / LlmEnd as the
        // pipeline runs so the Test Dashboard's per-case "in / out
        // tokens" columns don't report 0 for every agentic test case.
        // Single Usage event is emitted via the writer at the end so
        // `UsageAccumulatorHandler` (wrapping `ctx.writer`) sees the
        // total and forwards it onto the Record.
        let (event_tx, mut event_rx) = mpsc::channel::<AgenticEvent<AnalyticsEvent>>(256);
        let usage_consumer = tokio::spawn(async move {
            let mut input_tokens: i32 = 0;
            let mut output_tokens: i32 = 0;
            while let Some(ev) = event_rx.recv().await {
                if let AgenticEvent::Core(core) = ev {
                    match core {
                        CoreEvent::LlmStart { prompt_tokens, .. } => {
                            input_tokens = input_tokens.saturating_add(prompt_tokens as i32);
                        }
                        CoreEvent::LlmEnd {
                            output_tokens: out, ..
                        } => {
                            output_tokens = output_tokens.saturating_add(out as i32);
                        }
                        _ => {}
                    }
                }
            }
            (input_tokens, output_tokens)
        });

        let answer_text = agentic_pipeline::run_agentic_streaming(
            platform,
            std::path::Path::new(&resolved),
            input.prompt,
            event_tx,
        )
        .await
        .map_err(OxyError::RuntimeError)?;

        let (input_tokens, output_tokens) = usage_consumer
            .await
            .map_err(|e| OxyError::RuntimeError(format!("usage consumer: {e}")))?;
        ctx.write_kind(EventKind::Usage {
            usage: Usage::new(input_tokens, output_tokens),
        })
        .await?;

        Ok(OutputContainer::Single(Output::Text(answer_text)))
    }
}

#[derive(Clone, Debug)]
pub(super) struct TargetExecutable {
    task_ref: Option<String>,
    relevant_context_getter: RelevantContextGetter,
}

impl TargetExecutable {
    pub fn new(task_ref: Option<String>, relevant_context_getter: RelevantContextGetter) -> Self {
        Self {
            task_ref,
            relevant_context_getter,
        }
    }
}

#[async_trait::async_trait]
impl Executable<AgenticInput> for TargetExecutable {
    type Response = Vec<TargetOutput>;

    async fn execute(
        &mut self,
        execution_context: &ExecutionContext,
        input: AgenticInput,
    ) -> Result<Self::Response, OxyError> {
        let start = std::time::Instant::now();

        let handler = UsageAccumulatorHandler::new(execution_context.writer.clone());
        let usage_in = handler.usage_in.clone();
        let usage_out = handler.usage_out.clone();

        let output_container =
            execute_with_handler(AgenticTargetWrapper, execution_context, input, handler).await?;

        let duration_ms = start.elapsed().as_secs_f64() * 1000.0;
        let input_tokens = *usage_in.lock().unwrap();
        let output_tokens = *usage_out.lock().unwrap();

        let mut outputs: Vec<TargetOutput> = match &self.task_ref {
            Some(task_ref) => {
                let output = output_container.project_ref(task_ref)?;
                output
                    .into_iter()
                    .map(|item| {
                        OutputGetter {
                            value: item,
                            relevant_context_getter: &self.relevant_context_getter,
                        }
                        .try_into()
                    })
                    .try_collect()
            }
            None => {
                let output = OutputGetter {
                    value: &output_container,
                    relevant_context_getter: &self.relevant_context_getter,
                }
                .try_into();
                output.map(|item| vec![item])
            }
        }?;

        for out in &mut outputs {
            out.duration_ms = duration_ms;
            out.input_tokens = input_tokens;
            out.output_tokens = output_tokens;
        }

        Ok(outputs)
    }
}
