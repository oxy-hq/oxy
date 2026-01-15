use crate::execute::{Executable, ExecutionContext};
use oxy_shared::errors::OxyError;

use super::wrap::Wrap;

pub struct MemoWrapper<M> {
    memo: M,
}

impl<M> MemoWrapper<M> {
    pub fn new(memo: M) -> Self {
        Self { memo }
    }
}

impl<E, M> Wrap<E> for MemoWrapper<M>
where
    M: Clone,
{
    type Wrapper = Memo<E, M>;

    fn wrap(&self, inner: E) -> Memo<E, M> {
        Memo {
            inner,
            memo: self.memo.clone(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Memo<E, M> {
    inner: E,
    memo: M,
}

pub trait Memorable {
    fn memo(&mut self, input: Self) -> &mut Self;
}

#[async_trait::async_trait]
impl<E, M> Executable<M> for Memo<E, M>
where
    E: Executable<M> + Send,
    M: Memorable + Clone + Send,
{
    type Response = E::Response;

    async fn execute(
        &mut self,
        execution_context: &ExecutionContext,
        input: M,
    ) -> Result<Self::Response, OxyError> {
        self.memo.memo(input);

        self.inner
            .execute(execution_context, self.memo.clone())
            .await
    }
}

impl<I> Memorable for Vec<I> {
    fn memo(&mut self, input: Self) -> &mut std::vec::Vec<I> {
        self.extend(input);
        self
    }
}
