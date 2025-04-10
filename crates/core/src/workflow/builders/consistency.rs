use std::collections::HashMap;

use itertools::Itertools;
use minijinja::Value;
use tokio::task::JoinHandle;

use crate::{
    agent::{OpenAIExecutableResponse, build_openai_executable},
    errors::OxyError,
    execute::{
        Executable, ExecutionContext,
        builders::{
            ExecutableBuilder, concurrency::ConcurrencyControl, consistency::ConsistencyPicker,
            map::ParamMapper,
        },
        consistency::{PROMPT, parse_consistency_response},
        types::Output,
        writer::OrderedWriter,
    },
};

#[derive(Clone)]
pub struct AgentPicker {
    pub task_description: String,
    pub agent_ref: String,
}

#[derive(Clone)]
pub struct AgentScoreControl {
    pub comparison_idx_pairs: Vec<(usize, usize)>,
    pub outputs: HashMap<usize, Output>,
}

#[async_trait::async_trait]
impl ConcurrencyControl<OpenAIExecutableResponse> for AgentScoreControl {
    type Response = (usize, Output, f32);

    async fn handle(
        &self,
        execution_context: &ExecutionContext,
        results_handle: JoinHandle<
            Result<Vec<Result<OpenAIExecutableResponse, OxyError>>, OxyError>,
        >,
        ordered_writer: OrderedWriter,
    ) -> Result<Self::Response, OxyError> {
        let results = {
            let sender = execution_context.writer.clone();
            let events_handle =
                tokio::spawn(async move { ordered_writer.write_sender(sender).await });
            let results = results_handle.await??;
            events_handle.await??;
            results
        };
        // Because we use array_combinations the total comparison for each record is n-1
        let comparison_times = self.outputs.len() - 1;
        let acc = results
            .into_iter()
            .enumerate()
            .filter_map(|(idx, r)| match r {
                Ok(OpenAIExecutableResponse {
                    content: Output::Text(text),
                    ..
                }) => Some((idx, parse_consistency_response(&text) == "A")),
                _ => None,
            })
            .fold(HashMap::new(), |memo, (idx, is_consistent)| {
                let mut memo = memo;
                let (left_idx, right_idx) = self.comparison_idx_pairs[idx];
                memo.entry(left_idx).or_insert(0);
                memo.entry(right_idx).or_insert(0);
                if is_consistent {
                    let left = memo.entry(left_idx).or_insert(0);
                    *left += 1;
                    let right = memo.entry(right_idx).or_insert(0);
                    *right += 1;
                }
                memo
            });
        log::debug!("Consistency results: {:?}", acc);
        let value = acc
            .into_iter()
            .sorted_by_key(|(_, count)| *count)
            .last()
            .map(|(idx, count)| {
                let output = self.outputs.get(&idx).unwrap();
                let output = output.clone();
                let score: f32 = count as f32 / comparison_times as f32;
                (idx, output, score)
            })
            .ok_or_else(|| OxyError::RuntimeError("No successful results".to_string()))?;
        log::debug!(
            "Consistency score: {:?}, times: {:?}",
            value,
            comparison_times
        );

        Ok(value)
    }
}

#[derive(Clone)]
pub struct AgentPromptMapper {
    task_description: String,
}

#[async_trait::async_trait]
impl ParamMapper<(Output, Output), String> for AgentPromptMapper {
    async fn map(
        &self,
        execution_context: &ExecutionContext,
        input: (Output, Output),
    ) -> Result<(String, Option<ExecutionContext>), OxyError> {
        let (left, right) = input;
        let context = minijinja::context! {
            submission_1 => Value::from_object(left).to_string(),
            submission_2 => Value::from_object(right).to_string(),
            task_description => self.task_description.to_string(),
        };
        let prompt = execution_context.renderer.render_once(PROMPT, context)?;
        Ok((prompt, None))
    }
}

#[async_trait::async_trait]
impl ConsistencyPicker<Output> for AgentPicker {
    async fn pick(
        &self,
        execution_context: &ExecutionContext,
        results: Vec<Result<Output, OxyError>>,
    ) -> Result<(usize, Output, f32), OxyError> {
        let outputs = results
            .into_iter()
            .enumerate()
            .filter(|(_, result)| result.is_ok())
            .map(|(idx, result)| (idx, result.unwrap()))
            .collect::<Vec<_>>();
        if outputs.is_empty() {
            return Err(OxyError::RuntimeError("No successful results".to_string()));
        }
        if outputs.len() == 1 {
            let (idx, output) = outputs.into_iter().next().unwrap();
            return Ok((idx, output, 1.0));
        }
        let (comparison_idx_pairs, output_pairs): (Vec<_>, Vec<_>) = outputs
            .clone()
            .into_iter()
            .array_combinations::<2>()
            .map(|[(idx_left, output_left), (idx_right, output_right)]| {
                ((idx_left, idx_right), (output_left, output_right))
            })
            .unzip();
        let agent_config = execution_context
            .config
            .resolve_agent(self.agent_ref.clone())
            .await?;
        let model = execution_context
            .config
            .resolve_model(&agent_config.model)?;
        let agent = build_openai_executable(model);
        let mut consistency_evaluator = ExecutableBuilder::new()
            .concurrency_control(
                10,
                AgentScoreControl {
                    outputs: outputs.into_iter().collect(),
                    comparison_idx_pairs,
                },
            )
            .map(AgentPromptMapper {
                task_description: self.task_description.clone(),
            })
            .executable(agent);
        consistency_evaluator
            .execute(execution_context, output_pairs)
            .await
    }
}
