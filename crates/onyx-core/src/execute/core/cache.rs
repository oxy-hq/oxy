use crate::errors::OnyxError;

use super::{value::ContextValue, write::Write, Executable, ExecutionContext};

pub trait Cache: Sync {
    fn read(&self, key: &str) -> Option<ContextValue>;
    fn write(&self, key: &str, value: &ContextValue) -> Result<(), OnyxError>;
}

#[async_trait::async_trait]
pub trait Cacheable<Input, Event>: Executable<Input, Event> {
    async fn cache_key(
        &self,
        execution_context: &mut ExecutionContext<'_, Event>,
        input: &Input,
    ) -> Option<String>;
    fn hit_event(&self, _key: &str) -> Option<Event> {
        None
    }
    fn write_event(&self, _key: &str) -> Option<Event> {
        None
    }
    fn write_event_failed(&self, _key: &str, _err: OnyxError) -> Option<Event> {
        None
    }
}

pub struct CacheAdapterState {
    result: ContextValue,
}

pub struct CacheAdapter<'state> {
    state: &'state mut CacheAdapterState,
}

impl Write for CacheAdapter<'_> {
    fn write(&mut self, value: ContextValue) {
        self.state.result = value;
    }
}

pub struct CacheExecutor<'context, 'writer: 'context, Event> {
    execution_context: &'context mut ExecutionContext<'writer, Event>,
    cache_state: CacheAdapterState,
}

impl<'context, 'writer: 'context, Event> CacheExecutor<'context, 'writer, Event>
where
    Event: Send + 'static,
{
    pub fn new(execution_context: &'context mut ExecutionContext<'writer, Event>) -> Self {
        Self {
            execution_context,
            cache_state: CacheAdapterState {
                result: Default::default(),
            },
        }
    }

    pub async fn execute<I>(
        &mut self,
        entry: &dyn Executable<I, Event>,
        cacheable: &dyn Cacheable<I, Event>,
        input: I,
        cache: &dyn Cache,
    ) -> Result<(), OnyxError> {
        let mut cache_adapter = CacheAdapter {
            state: &mut self.cache_state,
        };
        let mut cache_context =
            ExecutionContext::from_parts(self.execution_context.clone_parts(), &mut cache_adapter);

        if let Some(key) = cacheable.cache_key(&mut cache_context, &input).await {
            if let Some(cached_value) = cache.read(&key) {
                cache_adapter.write(cached_value);
                if let Some(on_hit) = cacheable.hit_event(&key) {
                    self.execution_context.notify(on_hit).await?;
                }
                return Ok(());
            }

            entry.execute(&mut cache_context, input).await?;
            if let Err(err) = cache.write(&key, &self.cache_state.result) {
                if let Some(on_failed) = cacheable.write_event_failed(&key, err) {
                    self.execution_context.notify(on_failed).await?;
                }
            } else if let Some(on_write) = cacheable.write_event(&key) {
                self.execution_context.notify(on_write).await?;
            }
        } else {
            entry.execute(&mut cache_context, input).await?;
        }

        Ok(())
    }

    pub fn finish(self) {
        self.execution_context.write(self.cache_state.result);
    }
}
