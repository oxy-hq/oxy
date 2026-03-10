use minijinja::{Value, context};

use oxy::execute::{
    ExecutionContext,
    builders::map::ParamMapper,
    types::{Output, TargetOutput},
};
use oxy_agent::agent::openai::OneShotInput;
use oxy_shared::errors::OxyError;

use super::types::Record;

#[derive(Clone, Debug)]
pub(super) struct CorrectnessSolverMapper {
    pub prompt_template: String,
}

#[async_trait::async_trait]
impl ParamMapper<(TargetOutput, TargetOutput), OneShotInput> for CorrectnessSolverMapper {
    async fn map(
        &self,
        execution_context: &ExecutionContext,
        input: (TargetOutput, TargetOutput),
    ) -> Result<(OneShotInput, Option<ExecutionContext>), OxyError> {
        let (submission_1, submission_2) = input;
        // submission_1 = agent's actual answer
        // submission_2 = expected answer (from test case)
        let actual = &submission_1.output;
        let expected = &submission_2.output;
        let prompt = submission_2.task_description.as_deref().unwrap_or("");

        let ctx = context! {
            actual => Value::from_safe_string(actual.to_string()),
            expected => Value::from_safe_string(expected.to_string()),
            prompt => Value::from_safe_string(prompt.to_string()),
        };
        let system_instructions = execution_context
            .renderer
            .render_once(&self.prompt_template, ctx)
            .map_err(|_| {
                OxyError::RuntimeError("Failed to render correctness evaluation prompt".to_string())
            })?;
        Ok((
            OneShotInput {
                system_instructions,
                user_input: None,
                memory: vec![],
            },
            None,
        ))
    }
}

/// Parse a correctness judge response into a Record.
/// Scans the last few lines for "PASS" or "FAIL" (case-insensitive).
pub(super) fn parse_correctness_record(output: Output) -> Result<Record, OxyError> {
    let response = match output {
        Output::Text(text) => text,
        _ => {
            return Err(OxyError::RuntimeError(
                "Unsupported output type for correctness solver".to_string(),
            ));
        }
    };

    let trimmed = response.trim();
    let lines: Vec<&str> = trimmed.lines().collect();

    // Scan last 5 lines for PASS/FAIL verdict
    let mut verdict = None;
    for line in lines.iter().rev().take(5) {
        let upper = line.to_uppercase();
        // Strip common formatting characters
        let cleaned = upper.replace('*', "").replace('#', "").replace('_', " ");
        if cleaned.contains("PASS") && !cleaned.contains("FAIL") {
            verdict = Some("PASS");
            break;
        } else if cleaned.contains("FAIL") {
            verdict = Some("FAIL");
            break;
        }
    }

    let choice = match verdict {
        Some(v) => v.to_string(),
        None => {
            tracing::warn!(
                "Could not parse PASS/FAIL verdict from judge response, defaulting to FAIL. \
                 Last 5 lines: {:?}",
                lines.iter().rev().take(5).collect::<Vec<_>>()
            );
            "FAIL".to_string()
        }
    };
    let score = if choice == "PASS" { 1.0 } else { 0.0 };

    // Full response is stored as CoT reasoning (includes verdict line)
    let cot = trimmed.to_string();

    Ok(Record {
        cot,
        choice,
        score,
        prompt: None,
        expected: None,
        actual_output: None,
        references: vec![],
        duration_ms: 0.0,
        input_tokens: 0,
        output_tokens: 0,
    })
}
