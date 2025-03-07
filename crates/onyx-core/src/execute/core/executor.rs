use std::sync::{Arc, RwLock};

use async_stream::stream;
use futures::StreamExt;
use minijinja::Value;
use tokio::sync::mpsc::Sender;

use crate::{errors::OnyxError, execute::renderer::TemplateRegister};

use super::{
    event::propagate,
    value::{Array, ContextValue, Map},
    Executable, ExecutionContext, Write,
};

#[derive(Debug, Default)]
struct MapAdapterState {
    result: Map,
}

struct MapAdapter<'state> {
    key: String,
    state: &'state mut MapAdapterState,
}

impl Write for MapAdapter<'_> {
    fn write(&mut self, value: ContextValue) {
        log::info!(
            "MapAdapter.write to key `{}` with value: {:?}",
            self.key,
            value
        );
        self.state.result.set_value(&self.key, value.clone());
    }
}

pub struct MapExecutor<'context, 'writer: 'context, Event> {
    execution_context: &'context mut ExecutionContext<'writer, Event>,
    map_state: MapAdapterState,
}

impl<'context, 'writer: 'context, Event> MapExecutor<'context, 'writer, Event>
where
    Event: Send + 'static,
{
    pub fn new(execution_context: &'context mut ExecutionContext<'writer, Event>) -> Self {
        Self {
            execution_context,
            map_state: Default::default(),
        }
    }

    pub async fn entries<I, E, IT>(&mut self, entries: IT) -> Result<(), OnyxError>
    where
        E: Executable<I, Event>,
        IT: IntoIterator<Item = (String, E, I)>,
    {
        for (key, entry, input) in entries {
            self.entry(&key, &entry, input).await?;
        }
        Ok(())
    }

    pub async fn entry<I>(
        &mut self,
        key: &str,
        entry: &dyn Executable<I, Event>,
        input: I,
    ) -> Result<(), OnyxError> {
        let mut parts = self.execution_context.clone_parts();
        let current_output = self.map_state.result.to_owned();
        parts.with_renderer(
            self.execution_context
                .renderer
                .wrap(Value::from_object(current_output)),
        );
        let mut map_adapter = MapAdapter {
            key: key.to_string(),
            state: &mut self.map_state,
        };
        let mut execution_context = ExecutionContext::from_parts(parts, &mut map_adapter);
        entry.execute(&mut execution_context, input).await?;
        Ok(())
    }

    pub fn prefill(&mut self, key: &str, value: ContextValue) {
        self.map_state.result.set_value(key, value);
    }

    pub fn finish(self) {
        self.execution_context
            .write(ContextValue::Map(self.map_state.result));
    }
}

#[derive(Debug, Default)]
pub struct LoopAdapterState {
    result: Vec<(usize, ContextValue)>,
}

pub struct LoopAdapter {
    state: Arc<RwLock<LoopAdapterState>>,
    index: usize,
}

impl Write for LoopAdapter {
    fn write(&mut self, value: ContextValue) {
        self.state.write().unwrap().result.push((self.index, value));
    }
}

pub struct LoopExecutor<'context, 'writer: 'context, Event> {
    execution_context: &'context mut ExecutionContext<'writer, Event>,
    loop_state: Arc<RwLock<LoopAdapterState>>,
}

impl<'context, 'writer: 'context, Event> LoopExecutor<'context, 'writer, Event>
where
    Event: Send + 'static,
{
    pub fn new(execution_context: &'context mut ExecutionContext<'writer, Event>) -> Self {
        Self {
            execution_context,
            loop_state: Arc::new(RwLock::new(LoopAdapterState {
                result: Default::default(),
            })),
        }
    }

    pub async fn params<I, F, T>(
        &mut self,
        params: &mut Vec<I>,
        entry: &dyn Executable<I, Event>,
        context_map: F,
        concurrency: usize,
        progress_tracker: Option<T>,
        event_sender: Option<Sender<(usize, Event)>>,
    ) -> Result<(), OnyxError>
    where
        F: Fn(&I) -> Value,
        T: Fn() + Copy,
    {
        let mut result_indices: Vec<usize> = Vec::new();

        let results = stream! {
            for (index, param) in params.drain(..).enumerate() {
                let loop_state = self.loop_state.clone();
                let mut parts = self.execution_context.clone_parts();
                parts.with_renderer(
                    self.execution_context.renderer.wrap(context_map(&param))
                );

                if let Some(event_sender) = &event_sender {
                    log::debug!("Using event_sender for index: {}", index);
                    let index_clone = index;
                    let event_sender_clone = event_sender.clone();

                    let (sender, mut receiver) = tokio::sync::mpsc::channel::<Event>(100);


                    tokio::spawn(async move {
                        while let Some(event) = receiver.recv().await {
                            if let Err(e) = event_sender_clone.send((index_clone, event)).await {
                                log::error!("Failed to send indexed event: {}", e);
                                break;
                            }
                        }
                    });

                    parts = parts.with_sender(sender);
                }

                result_indices.push(index);

                yield async move {
                    let mut loop_adapter = LoopAdapter {
                        state: loop_state,
                        index,
                    };
                    let mut loop_context =
                        ExecutionContext::from_parts(parts, &mut loop_adapter);

                    let output = entry.execute(&mut loop_context, param).await;

                    if let Some(progress_tracker) = progress_tracker {
                         progress_tracker();
                    }
                    output
                };
            }
        }
        .buffered(concurrency)
        .collect::<Vec<_>>()
        .await;

        log::info!(
            "LoopExecutor: collected results {}",
            results.iter().fold(0, |acc, r| acc + r.is_ok() as i32)
        );

        let errs = results
            .into_iter()
            .filter_map(|r| r.err())
            .collect::<Vec<OnyxError>>();

        if !errs.is_empty() {
            return Err(OnyxError::RuntimeError(format!(
                "----------\n{}\n----------",
                errs.iter()
                    .map(|e| e.to_string())
                    .collect::<Vec<String>>()
                    .join("\n**********\n")
            )));
        }

        Ok(())
    }

    pub fn finish(self) -> Result<(), OnyxError> {
        let lock = Arc::try_unwrap(self.loop_state)
            .map_err(|_| OnyxError::RuntimeError("Failed to eject value from loop".to_string()))?;
        let inner = lock.into_inner()?;
        self.execution_context.write(ContextValue::Array(Array(
            inner.result.iter().map(|(_, v)| v.clone()).collect(),
        )));
        Ok(())
    }

    pub fn eject(self) -> Result<Vec<ContextValue>, OnyxError> {
        let lock = Arc::try_unwrap(self.loop_state)
            .map_err(|_| OnyxError::RuntimeError("Failed to eject value from loop".to_string()))?;
        let inner = lock.into_inner()?;
        Ok(inner.result.iter().map(|(_, v)| v.clone()).collect())
    }

    pub fn eject_results(self) -> Result<Vec<(usize, ContextValue)>, OnyxError> {
        let lock = Arc::try_unwrap(self.loop_state)
            .map_err(|_| OnyxError::RuntimeError("Failed to eject value from loop".to_string()))?;
        let inner = lock.into_inner()?;
        Ok(inner.result)
    }
}

pub struct ChildAdapterState {
    result: ContextValue,
}

pub struct ChildAdapter<'state> {
    state: &'state mut ChildAdapterState,
}

impl Write for ChildAdapter<'_> {
    fn write(&mut self, value: ContextValue) {
        self.state.result = value;
    }
}

pub struct ChildExecutor<'context, 'writer: 'context, Event> {
    execution_context: &'context mut ExecutionContext<'writer, Event>,
    child_state: ChildAdapterState,
}

impl<'context, 'writer: 'context, Event> ChildExecutor<'context, 'writer, Event>
where
    Event: Send + 'static,
{
    pub fn new(execution_context: &'context mut ExecutionContext<'writer, Event>) -> Self {
        Self {
            execution_context,
            child_state: ChildAdapterState {
                result: Default::default(),
            },
        }
    }

    pub async fn execute<'executor, I, F, ChildEvent: 'executor>(
        &'executor mut self,
        entry: &dyn Executable<I, ChildEvent>,
        input: I,
        map_event: F,
        global_context: Value,
        context: Value,
        template: &'executor dyn TemplateRegister,
    ) -> Result<(), OnyxError>
    where
        F: Fn(ChildEvent) -> Event + Send + 'static,
        ChildEvent: Send + 'static,
    {
        let (child_sender, child_receiver) = tokio::sync::mpsc::channel::<ChildEvent>(10);
        let parent_sender = self.execution_context.get_sender();
        let propagate_handle = propagate(child_receiver, parent_sender, map_event);
        {
            let mut parts = self.execution_context.clone_parts();
            parts.with_renderer(
                self.execution_context
                    .renderer
                    .switch_context(global_context, context),
            );
            let parts = parts.with_sender(child_sender);
            let mut child_adapter = ChildAdapter {
                state: &mut self.child_state,
            };
            let mut child_context = ExecutionContext::from_parts(parts, &mut child_adapter);
            child_context.renderer.register(template)?;
            entry.execute(&mut child_context, input).await?;
        }
        propagate_handle
            .await
            .map_err(|err| OnyxError::RuntimeError(err.to_string()))?;
        Ok(())
    }

    pub fn finish(self) {
        self.execution_context.write(self.child_state.result);
    }
}
