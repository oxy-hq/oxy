use futures::stream::StreamExt;

use oxy::{
    adapters::openai::{IntoOpenAIConfig, OpenAIClient},
    config::{
        constants::{EVAL_METRICS_POSTFIX, EVAL_SOURCE},
        model::SolverKind,
    },
    exec_runtime::ExecutionContext,
    exec_types::{ProgressType, TargetOutput},
};
use oxy_shared::errors::OxyError;

use super::{
    correctness_solver::{build_correctness_input, parse_correctness_record},
    one_shot::OneShotJudge,
    types::{Correctness, MetricKind, Record},
};

/// Score one metric over the target outputs, on the plain-async path (no
/// old-executor pipeline). The `.test.yml` entry point only ever builds
/// `SolverKind::Correctness`; the `Similarity` / `ContextRecall` solvers went
/// with the classic-agent eval retirement and are rejected here.
pub(super) async fn run_solver(
    execution_context: &ExecutionContext,
    solver_kind: SolverKind,
    outputs: Vec<(TargetOutput, TargetOutput)>,
    errors_with_expected: Vec<(String, TargetOutput)>,
    concurrency: usize,
) -> Result<MetricKind, OxyError> {
    let SolverKind::Correctness(correctness_solver) = solver_kind else {
        return Err(OxyError::ConfigurationError(
            "Only the correctness solver is supported; the similarity / context-recall solvers \
             were removed with the classic-agent eval retirement."
                .to_string(),
        ));
    };

    let metric_context = execution_context.with_child_source(
        format!("{}-{}", execution_context.source.id, EVAL_METRICS_POSTFIX),
        EVAL_SOURCE.to_string(),
    );
    let config_manager = &execution_context.workspace.config_manager;
    let secret_manager = &execution_context.workspace.secrets_manager;

    let model_ref = match &correctness_solver.model_ref {
        Some(model_ref) => model_ref,
        None => config_manager
            .default_model()
            .ok_or_else(|| OxyError::ConfigurationError("No default model found".to_string()))?,
    };
    let model = config_manager.resolve_model(model_ref)?;
    let client = OpenAIClient::with_config(model.into_openai_config(secret_manager).await?);
    let judge = OneShotJudge::new(client, model.model_name().to_string());
    let prompt_template = correctness_solver.prompt.to_string();

    // Judge each (actual, expected) pair concurrently. `buffered` preserves
    // input order, though this arm no longer relies on it (each future pairs its
    // own record). Progress events are re-emitted on `metric_context` — the old
    // `ExecutableBuilder` concurrency wrapper drove the "Judging responses" bar
    // (service/eval.rs) and the Test Dashboard SSE series (service/test.rs) via
    // exactly these. Telemetry writes are best-effort.
    let total = outputs.len();
    if let Err(e) = metric_context
        .write_progress(ProgressType::Started(Some(total)))
        .await
    {
        tracing::warn!("eval: failed to emit judging progress Started: {e}");
    }
    let mut stream = futures::stream::iter(outputs)
        .map(|(actual, expected)| {
            let judge = judge.clone();
            let prompt_template = prompt_template.clone();
            let metric_context = metric_context.clone();
            async move {
                let input =
                    build_correctness_input(&metric_context, &prompt_template, &actual, &expected)?;
                let output = judge.run(input).await?;
                let mut record = parse_correctness_record(output.content)?;
                record.prompt = expected.task_description.clone();
                record.expected = Some(expected.output.clone());
                record.actual_output = Some(actual.output.clone());
                record.references = actual.references.clone();
                record.duration_ms = actual.duration_ms;
                record.input_tokens = actual.input_tokens;
                record.output_tokens = actual.output_tokens;
                Ok(record)
            }
        })
        .buffered(concurrency.max(1));
    let mut judged: Vec<Result<Record, OxyError>> = Vec::with_capacity(total);
    while let Some(record) = stream.next().await {
        if let Err(e) = metric_context
            .write_progress(ProgressType::Updated(1))
            .await
        {
            tracing::warn!("eval: failed to emit judging progress Updated: {e}");
        }
        judged.push(record);
    }
    if let Err(e) = metric_context.write_progress(ProgressType::Finished).await {
        tracing::warn!("eval: failed to emit judging progress Finished: {e}");
    }
    let mut records = judged
        .into_iter()
        .collect::<Result<Vec<Record>, OxyError>>()?;

    // Errored runs count as FAILs so the denominator is correct.
    for (error_msg, expected) in &errors_with_expected {
        records.push(Record {
            cot: format!("Run failed with error: {error_msg}"),
            choice: "FAIL".to_string(),
            score: 0.0,
            prompt: expected.task_description.clone(),
            expected: Some(expected.output.clone()),
            actual_output: Some(format!("[ERROR] {error_msg}")),
            references: vec![],
            duration_ms: 0.0,
            input_tokens: 0,
            output_tokens: 0,
        });
    }

    Ok(MetricKind::Correctness(Correctness::from_records(records)))
}
