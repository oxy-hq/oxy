use serde::{Serialize, de::DeserializeOwned};

use crate::{
    config::constants::CACHE_SOURCE,
    errors::OxyError,
    execute::{Executable, ExecutionContext, types::EventKind},
    theme::StyledText,
};

use super::wrap::Wrap;

#[async_trait::async_trait]
pub trait Cacheable<I> {
    async fn cache_key(&self, execution_context: &ExecutionContext, input: &I) -> Option<String>;
}

#[async_trait::async_trait]
pub trait CacheWriter {
    async fn read<R: DeserializeOwned>(&self, key: &str) -> Option<R>;
    async fn write<R: Serialize + Sync>(&self, key: &str, value: &R) -> Result<(), OxyError>;
}

#[async_trait::async_trait]
pub trait CacheStorage<I, R> {
    async fn read(&self, execution_context: &ExecutionContext, input: &I) -> Option<R>;
    async fn write(
        &self,
        execution_context: &ExecutionContext,
        input: &I,
        value: &R,
    ) -> Result<(), OxyError>;
}

pub struct CacheWrapper<S> {
    storage: S,
}

impl<S> CacheWrapper<S> {
    pub fn new(storage: S) -> Self {
        Self { storage }
    }
}

impl<E, S> Wrap<E> for CacheWrapper<S>
where
    S: Clone,
{
    type Wrapper = Cache<E, S>;

    fn wrap(&self, inner: E) -> Cache<E, S> {
        Cache::new(inner, self.storage.clone())
    }
}

pub struct Cache<E, S> {
    inner: E,
    storage: S,
}

impl<E, S> Cache<E, S> {
    fn new(inner: E, storage: S) -> Self {
        Self { inner, storage }
    }
}

impl<E, S> Clone for Cache<E, S>
where
    E: Clone,
    S: Clone,
{
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            storage: self.storage.clone(),
        }
    }
}

#[async_trait::async_trait]
impl<I, E, S, R> Executable<I> for Cache<E, S>
where
    I: Clone + Send + 'static,
    E: Executable<I, Response = R> + Send,
    S: CacheStorage<I, R> + Send,
    R: Send + Sync,
{
    type Response = E::Response;

    async fn execute(
        &mut self,
        execution_context: &ExecutionContext,
        input: I,
    ) -> Result<Self::Response, OxyError> {
        let cache_context = execution_context.with_child_source(
            CACHE_SOURCE.to_string(),
            format!("{}-{}", execution_context.source.id, CACHE_SOURCE),
        );
        if let Some(value) = self.storage.read(&cache_context, &input).await {
            cache_context
                .write_kind(EventKind::Message {
                    message: "Cache detected. Using cache.".primary().to_string(),
                })
                .await?;
            return Ok(value);
        };

        let result = self.inner.execute(execution_context, input.clone()).await?;
        self.storage.write(&cache_context, &input, &result).await?;
        Ok(result)
    }
}
