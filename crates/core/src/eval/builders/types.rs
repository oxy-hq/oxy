use std::collections::HashMap;

use serde::Serialize;

use crate::{
    agent::types::AgentInput, errors::OxyError, execute::types::Output, theme::StyledText,
    workflow::WorkflowInput,
};

pub struct EvalInput {
    pub index: Option<usize>,
    pub target_ref: String,
}

#[derive(Clone, Debug)]
pub(super) enum EvalTarget {
    Workflow(WorkflowInput),
    Agent(AgentInput),
}

impl std::fmt::Display for EvalTarget {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EvalTarget::Workflow(workflow_input) => write!(f, "{}", workflow_input.workflow_ref),
            EvalTarget::Agent(agent_input) => write!(f, "{}", agent_input.agent_ref),
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub enum MetricKind {
    Accuracy(f32),
}

impl std::fmt::Display for MetricKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MetricKind::Accuracy(accuracy) => write!(f, "Accuracy: {:.2}%", accuracy * 100.0),
        }
    }
}

#[derive(Debug, Serialize, Clone)]
pub struct Record {
    pub cot: String,
    pub choice: String,
    pub score: f32,
}

impl std::fmt::Display for Record {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let reason = self
            .cot
            .replace("---", &format!("{}", "---".error()))
            .replace("+++", &format!("{}", "+++".success()));

        if self.score < 1.0 {
            writeln!(f, "{}", "Inconsistent result detected.".warning())?;
        }
        writeln!(f, "{}", &reason)
    }
}

impl Record {
    pub fn fill_score(&mut self, scores: &HashMap<String, f32>) {
        if let Some(score) = scores.get(&self.choice) {
            self.score = *score;
        }
    }
}

impl TryFrom<Output> for Record {
    type Error = OxyError;

    fn try_from(value: Output) -> Result<Self, Self::Error> {
        let record = Record {
            cot: String::new(),
            choice: String::new(),
            score: 0.0,
        };
        let response = match value {
            Output::Text(text) => text,
            _ => {
                return Err(OxyError::RuntimeError(
                    "Unsupported output type".to_string(),
                ));
            }
        };
        let record = response.trim().lines().fold(record, |mut record, part| {
            record.cot.push_str(&record.choice);
            record.cot.push('\n');
            record.choice = part.trim().to_string();
            record
        });
        Ok(record)
    }
}

#[derive(Debug, Serialize, Clone)]
pub struct Metric {
    errors: Vec<String>,
    records: Vec<Record>,
    pub kind: MetricKind,
}

impl Metric {
    pub fn set_errors(&mut self, errors: Vec<String>) {
        self.errors = errors;
    }
}

impl std::fmt::Display for Metric {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if !self.errors.is_empty() {
            writeln!(
                f,
                "{}",
                format!("\nFailed to generate {} outputs:\n", self.errors.len()).warning()
            )?;
            writeln!(f, "**********\n")?;
            for error in &self.errors {
                writeln!(f, "{}", error)?;
                writeln!(f, "**********\n")?;
            }
            writeln!(f)?;
        }
        let mut is_header_printed = false;
        for record in &self.records {
            if record.score < 1.0 {
                if !is_header_printed {
                    writeln!(f, "{}\n", "FAILURES:".error())?;
                    writeln!(f, "**********\n")?;
                    is_header_printed = true;
                }
                write!(f, "{}", record)?;
                writeln!(f, "**********\n")?;
            }
        }
        Ok(())
    }
}

impl FromIterator<Record> for Metric {
    fn from_iter<T: IntoIterator<Item = Record>>(iter: T) -> Self {
        let records = iter.into_iter().collect::<Vec<_>>();
        let accuracy = records.iter().map(|r| r.score).sum::<f32>() / records.len() as f32;
        Metric {
            errors: vec![],
            records,
            kind: MetricKind::Accuracy(accuracy),
        }
    }
}
