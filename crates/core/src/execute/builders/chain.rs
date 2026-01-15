use crate::execute::{
    builders::map::ParamMapper,
    context::{Executable, ExecutionContext},
};
use oxy_shared::errors::OxyError;

use super::wrap::Wrap;

#[async_trait::async_trait]
pub trait ContextMapper<I, V> {
    async fn map_reduce(
        &self,
        execution_context: &ExecutionContext,
        memo: V,
        input: I,
        value: V,
    ) -> Result<(V, Option<ExecutionContext>), OxyError>;
}

#[derive(Clone)]
pub struct NoopMapper;

#[async_trait::async_trait]
impl<I, V> ContextMapper<I, V> for NoopMapper
where
    I: Send + 'static,
    V: Send + 'static,
{
    async fn map_reduce(
        &self,
        _execution_context: &ExecutionContext,
        _memo: V,
        _input: I,
        value: V,
    ) -> Result<(V, Option<ExecutionContext>), OxyError> {
        Ok((value, None))
    }
}

pub trait IntoChain<I, V> {
    fn into_chain(self) -> (Vec<I>, V);
}

pub trait UpdateInput<V> {
    fn update_input(self, input: &V) -> Self;
}

pub struct ChainWrapper<M, I, V, T> {
    mapper: M,
    _initial_input: std::marker::PhantomData<T>,
    _input: std::marker::PhantomData<I>,
    _memo: std::marker::PhantomData<V>,
}

impl<M, I, V, T> ChainWrapper<M, I, V, T> {
    pub fn new(mapper: M) -> Self {
        Self {
            mapper,
            _initial_input: std::marker::PhantomData,
            _input: std::marker::PhantomData,
            _memo: std::marker::PhantomData,
        }
    }
}

impl<E, M, I, V, T> Wrap<E> for ChainWrapper<M, I, V, T>
where
    M: Clone,
{
    type Wrapper = Chain<E, M, I, V, T>;

    fn wrap(&self, inner: E) -> Chain<E, M, I, V, T> {
        Chain::new(inner, self.mapper.clone())
    }
}

pub struct Chain<E, M, I, V, T> {
    inner: E,
    mapper: M,
    _initial_input: std::marker::PhantomData<T>,
    _input: std::marker::PhantomData<I>,
    _memo: std::marker::PhantomData<V>,
}

impl<E, M, I, V, T> Chain<E, M, I, V, T> {
    pub fn new(inner: E, mapper: M) -> Self {
        Self {
            inner,
            mapper,
            _initial_input: std::marker::PhantomData,
            _input: std::marker::PhantomData,
            _memo: std::marker::PhantomData,
        }
    }
}

impl<E, M, I, V, T> Clone for Chain<E, M, I, V, T>
where
    E: Clone,
    M: Clone,
{
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            mapper: self.mapper.clone(),
            _initial_input: std::marker::PhantomData,
            _input: std::marker::PhantomData,
            _memo: std::marker::PhantomData,
        }
    }
}

#[async_trait::async_trait]
impl<IT, E, M, I, V, T> Executable<IT> for Chain<E, M, I, V, T>
where
    IT: IntoChain<T, V> + Send + 'static,
    T: Clone + Send + 'static,
    I: UpdateInput<V> + Clone + Send + 'static,
    E: Executable<I, Response = V> + Send,
    M: ContextMapper<I, V> + ParamMapper<T, I> + Send,
    V: Clone + Send + 'static,
{
    type Response = E::Response;

    async fn execute(
        &mut self,
        execution_context: &ExecutionContext,
        input: IT,
    ) -> Result<Self::Response, OxyError> {
        let (items, mut memo) = input.into_chain();
        let mut execution_context = execution_context.clone();
        for item in items.into_iter().collect::<Vec<_>>() {
            let (mapped_input, mapped_context) = self.mapper.map(&execution_context, item).await?;
            if let Some(new_context) = mapped_context {
                execution_context = new_context;
            }

            let output = self
                .inner
                .execute(&execution_context, mapped_input.clone().update_input(&memo))
                .await?;
            let (new_output, new_context) = self
                .mapper
                .map_reduce(&execution_context, memo, mapped_input, output)
                .await?;
            if let Some(new_context) = new_context {
                execution_context = new_context;
            }
            memo = new_output;
        }
        Ok(memo)
    }
}
