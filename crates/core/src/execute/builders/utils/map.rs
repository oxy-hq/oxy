use crate::{
    errors::OxyError,
    execute::{ExecutionContext, builders::map::ParamMapper},
};

#[derive(Clone)]
pub struct ConsistencyMapper {
    pub sample_size: usize,
}

#[async_trait::async_trait]
impl<P> ParamMapper<P, Vec<P>> for ConsistencyMapper
where
    P: Clone + Send + 'static,
{
    async fn map(
        &self,
        _execution_context: &ExecutionContext,
        input: P,
    ) -> Result<(Vec<P>, Option<ExecutionContext>), OxyError> {
        let inputs = (0..self.sample_size)
            .map(|_| input.clone())
            .collect::<Vec<_>>();
        Ok((inputs, None))
    }
}
