use std::marker::PhantomData;

use crate::{
    errors::OxyError,
    execute::context::{Executable, ExecutionContext},
};

use super::wrap::Wrap;

pub struct MapInputWrapper<M, T> {
    mapper: M,
    _input: PhantomData<T>,
}

impl<M, T> MapInputWrapper<M, T> {
    pub fn new(mapper: M) -> Self {
        Self {
            mapper,
            _input: PhantomData,
        }
    }
}

impl<E, M, T> Wrap<E> for MapInputWrapper<M, T>
where
    M: Clone,
{
    type Wrapper = MapInput<E, M, T>;

    fn wrap(&self, inner: E) -> MapInput<E, M, T> {
        MapInput::new(inner, self.mapper.clone())
    }
}

#[async_trait::async_trait]
pub trait ParamMapper<P, T> {
    async fn map(
        &self,
        execution_context: &ExecutionContext,
        input: P,
    ) -> Result<(T, Option<ExecutionContext>), OxyError>;
}

pub struct MapInput<E, M, T> {
    inner: E,
    mapper: M,
    _input: PhantomData<T>,
}

impl<E, M, T> Clone for MapInput<E, M, T>
where
    E: Clone,
    M: Clone,
{
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            mapper: self.mapper.clone(),
            _input: PhantomData,
        }
    }
}

impl<E, M, T> MapInput<E, M, T> {
    pub fn new(inner: E, mapper: M) -> Self {
        Self {
            inner,
            mapper,
            _input: PhantomData,
        }
    }
}

#[async_trait::async_trait]
impl<E, M, P, T> Executable<P> for MapInput<E, M, T>
where
    E: Executable<T> + Send,
    M: ParamMapper<P, T> + Send + Sync,
    P: Send + 'static,
    T: Send + 'static,
{
    type Response = E::Response;

    async fn execute(
        &mut self,
        execution_context: &ExecutionContext,
        input: P,
    ) -> Result<Self::Response, OxyError> {
        let (mapped_input, mapped_context) = self.mapper.map(execution_context, input).await?;
        match mapped_context {
            Some(context) => self.inner.execute(&context, mapped_input).await,
            None => self.inner.execute(execution_context, mapped_input).await,
        }
    }
}

#[derive(Clone)]
pub struct IntoMapper;

#[async_trait::async_trait]
impl<P, T> ParamMapper<P, T> for IntoMapper
where
    P: Into<T> + Send + 'static,
{
    async fn map(
        &self,
        _execution_context: &ExecutionContext,
        input: P,
    ) -> Result<(T, Option<ExecutionContext>), OxyError> {
        Ok((input.into(), None))
    }
}
