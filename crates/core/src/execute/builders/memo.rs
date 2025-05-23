use crate::{
    errors::OxyError,
    execute::{Executable, ExecutionContext},
};

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

#[async_trait::async_trait]
impl<I, E> Executable<Vec<I>> for Memo<E, Vec<I>>
where
    E: Executable<Vec<I>> + Send,
    I: Clone + Send,
{
    type Response = E::Response;

    async fn execute(
        &mut self,
        execution_context: &ExecutionContext,
        input: Vec<I>,
    ) -> Result<Self::Response, OxyError> {
        self.memo.extend(input);

        self.inner
            .execute(execution_context, self.memo.clone())
            .await
    }
}
