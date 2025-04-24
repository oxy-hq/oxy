use itertools::Itertools;

use crate::{
    config::model::EvalKind,
    errors::OxyError,
    execute::{
        Executable, ExecutionContext,
        builders::{ExecutableBuilder, utils::ConsistencyMapper},
        types::TargetOutput,
    },
};

use super::{target::TargetExecutable, types::EvalTarget};

#[derive(Clone, Debug)]
pub(super) struct GeneratorExecutable {
    concurrency: usize,
}

impl GeneratorExecutable {
    pub fn new(concurrency: usize) -> Self {
        Self { concurrency }
    }
}

#[async_trait::async_trait]
impl Executable<(EvalKind, EvalTarget, Option<String>)> for GeneratorExecutable {
    type Response = (Vec<(TargetOutput, TargetOutput)>, Vec<String>);

    async fn execute(
        &mut self,
        execution_context: &ExecutionContext,
        (eval_kind, eval_target, task_ref): (EvalKind, EvalTarget, Option<String>),
    ) -> Result<Self::Response, OxyError> {
        match &eval_kind {
            EvalKind::Consistency(consistency) => {
                let mut consistency_executable = ExecutableBuilder::new()
                    .map(ConsistencyMapper {
                        sample_size: consistency.n,
                    })
                    .concurrency(self.concurrency)
                    .executable(TargetExecutable::new(task_ref));
                let results = consistency_executable
                    .execute(execution_context, eval_target)
                    .await?;
                let errors = results
                    .iter()
                    .filter_map(|res| match res {
                        Ok(_) => None,
                        Err(err) => Some(err.to_string()),
                    })
                    .collect::<Vec<_>>();
                let outputs = results
                    .into_iter()
                    .filter_map(|res| res.ok())
                    .flatten()
                    .collect::<Vec<_>>()
                    .into_iter()
                    .circular_tuple_windows::<(_, _)>()
                    .collect::<Vec<_>>();
                Ok((outputs, errors))
            }
            EvalKind::Custom(custom) => {
                todo!()
            }
        }
    }
}
