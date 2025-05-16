use itertools::Itertools;

use crate::{
    config::model::EvalKind,
    errors::OxyError,
    eval::builders::types::EvalRecord,
    execute::{
        Executable, ExecutionContext,
        builders::{ExecutableBuilder, utils::ConsistencyMapper},
        types::{RelevantContextGetter, TargetOutput},
    },
    utils::asyncify,
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
                    .executable(TargetExecutable::new(task_ref, RelevantContextGetter::Id));
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
                let dataset_path = execution_context
                    .config
                    .resolve_file(&custom.dataset)
                    .await?;

                let records = asyncify(move || {
                    let rdr = std::fs::File::open(dataset_path).map_err(|err| {
                        OxyError::RuntimeError(format!("Failed to open file: {}", err))
                    })?;
                    let records: Vec<EvalRecord> = serde_yaml::from_reader(rdr).map_err(|err| {
                        OxyError::SerializerError(format!(
                            "Failed to deserialize EvalRecord: {}",
                            err
                        ))
                    })?;
                    Ok(records)
                })
                .await?;
                let relevant_context_getter = if custom.is_context_id {
                    RelevantContextGetter::Id
                } else {
                    RelevantContextGetter::Content
                };
                let mut target_executable = ExecutableBuilder::new()
                    .concurrency(self.concurrency)
                    .executable(TargetExecutable::new(task_ref, relevant_context_getter));
                let inputs = records
                    .iter()
                    .map(|record| record.as_target(&eval_target, &custom.workflow_variable_name))
                    .collect::<Vec<_>>();
                let results = target_executable
                    .execute(execution_context, inputs)
                    .await?
                    .into_iter()
                    .zip(records.iter())
                    .map(|(res, record)| {
                        res.map(|outputs| {
                            outputs
                                .into_iter()
                                .map(|output| (output, Into::<TargetOutput>::into(record.clone())))
                                .collect::<Vec<_>>()
                        })
                    })
                    .collect::<Vec<_>>();
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
                    .collect::<Vec<_>>();

                Ok((outputs, errors))
            }
        }
    }
}
