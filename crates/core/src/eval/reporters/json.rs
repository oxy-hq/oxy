use std::io::Write;

use serde_json::json;

use crate::{
    errors::OxyError,
    eval::builders::types::{EvalResult, MetricKind},
};

use super::Reporter;

pub struct JsonReporter;

impl JsonReporter {
    fn extract_metrics(results: &[EvalResult]) -> serde_json::Value {
        let mut accuracy_scores = Vec::new();
        let mut recall_scores = Vec::new();

        for result in results {
            // Write errors to stderr (not in JSON output)
            if !result.errors.is_empty() {
                eprintln!("Errors occurred during evaluation:");
                for error in &result.errors {
                    eprintln!("  {error}");
                }
            }

            // Extract metrics
            for metric in &result.metrics {
                match metric {
                    MetricKind::Similarity(similarity) => {
                        accuracy_scores.push(similarity.score);
                    }
                    MetricKind::Recall(recall) => {
                        recall_scores.push(recall.score);
                    }
                }
            }
        }

        let mut json_metrics = serde_json::Map::new();

        // Output single value if only one test, array if multiple
        if !accuracy_scores.is_empty() {
            if accuracy_scores.len() == 1 {
                json_metrics.insert("accuracy".to_string(), json!(accuracy_scores[0]));
            } else {
                json_metrics.insert("accuracy".to_string(), json!(accuracy_scores));
            }
        }

        if !recall_scores.is_empty() {
            if recall_scores.len() == 1 {
                json_metrics.insert("recall".to_string(), json!(recall_scores[0]));
            } else {
                json_metrics.insert("recall".to_string(), json!(recall_scores));
            }
        }

        json!(json_metrics)
    }
}

impl Reporter for JsonReporter {
    fn report(&self, results: &[EvalResult], writer: &mut dyn Write) -> Result<(), OxyError> {
        let json_output = Self::extract_metrics(results);
        writeln!(
            writer,
            "{}",
            serde_json::to_string(&json_output)
                .map_err(|e| OxyError::RuntimeError(format!("Failed to serialize JSON: {e}")))?
        )?;
        Ok(())
    }
}
