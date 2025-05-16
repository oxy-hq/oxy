use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::{
    agent::types::AgentInput,
    errors::OxyError,
    execute::types::{Output, TargetOutput},
    theme::StyledText,
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

#[derive(Clone, Debug, Deserialize)]
pub(super) struct EvalRecord {
    pub query: String,
    pub response: String,
    pub relevant_contexts: Vec<String>,
}

impl From<EvalRecord> for TargetOutput {
    fn from(val: EvalRecord) -> Self {
        TargetOutput {
            output: val.response,
            task_description: Some(val.query),
            relevant_contexts: val.relevant_contexts.clone(),
        }
    }
}

impl EvalRecord {
    pub(super) fn as_target(
        &self,
        target: &EvalTarget,
        workflow_variable_name: &Option<String>,
    ) -> EvalTarget {
        match target {
            EvalTarget::Workflow(workflow_input) => EvalTarget::Workflow(WorkflowInput {
                restore_from_checkpoint: false,
                workflow_ref: workflow_input.workflow_ref.clone(),
                variables: Some(HashMap::from_iter([(
                    workflow_variable_name.clone().unwrap_or_default(),
                    serde_json::to_value(&self.query).unwrap(),
                )])),
            }),
            EvalTarget::Agent(agent_input) => EvalTarget::Agent(AgentInput {
                agent_ref: agent_input.agent_ref.clone(),
                prompt: self.query.clone(),
            }),
        }
    }
}

#[enum_dispatch::enum_dispatch]
trait Verbose {
    fn verbose(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result;
}

#[derive(Clone, Debug, Serialize)]
#[serde(tag = "type")]
#[enum_dispatch::enum_dispatch(Verbose)]
pub enum MetricKind {
    Similarity(Similarity),
    Recall(Recall),
}

#[derive(Clone, Debug, Serialize)]
pub struct Similarity {
    pub score: f32,
    pub records: Vec<Record>,
}

impl Similarity {
    pub fn new(score: f32, records: Vec<Record>) -> Self {
        Self { score, records }
    }
}

impl Verbose for Similarity {
    fn verbose(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
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

#[derive(Clone, Debug, Serialize)]
pub struct RecallRecord {
    pub score: f32,
    pub pass: bool,
    pub retrieved_contexts: Vec<String>,
    pub reference_contexts: Vec<String>,
}

impl std::fmt::Display for RecallRecord {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "Distance score: {}", self.score)?;
        writeln!(f, "Retrieved Contexts:")?;
        for context in &self.retrieved_contexts {
            writeln!(f, "- {:?}", context)?;
        }
        writeln!(f, "Reference Contexts:")?;
        for context in &self.reference_contexts {
            writeln!(f, "- {:?}", context)?;
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct Recall {
    pub score: f32,
    pub records: Vec<RecallRecord>,
}

impl Verbose for Recall {
    fn verbose(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut is_header_printed = false;
        for record in &self.records {
            if !record.pass {
                if !is_header_printed {
                    writeln!(f, "{}\n", "RECALL FAILURES:".error())?;
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

impl std::fmt::Display for MetricKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MetricKind::Similarity(Similarity { score, .. }) => {
                writeln!(f, "Accuracy: {:.2}%", score * 100.0)
            }
            MetricKind::Recall(Recall { score, .. }) => {
                writeln!(f, "Recall: {:.2}%", score * 100.0)
            }
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

#[derive(Serialize, Clone)]
pub struct EvalResult {
    errors: Vec<String>,
    pub metrics: Vec<MetricKind>,
}

impl EvalResult {
    pub fn new(errors: Vec<String>, metrics: Vec<MetricKind>) -> Self {
        Self { errors, metrics }
    }
}

impl std::fmt::Debug for EvalResult {
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
        for metric in &self.metrics {
            metric.verbose(f)?;
        }
        Ok(())
    }
}

impl std::fmt::Display for EvalResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "{}", "✅Eval finished with metrics:".primary())?;
        for metric in &self.metrics {
            writeln!(f, "{}", format!("{}", metric).primary())?;
        }
        Ok(())
    }
}

impl FromIterator<Record> for MetricKind {
    fn from_iter<T: IntoIterator<Item = Record>>(iter: T) -> Self {
        let records = iter.into_iter().collect::<Vec<_>>();
        let score = records.iter().map(|r| r.score).sum::<f32>() / records.len() as f32;
        MetricKind::Similarity(Similarity { score, records })
    }
}

impl FromIterator<RecallRecord> for MetricKind {
    fn from_iter<T: IntoIterator<Item = RecallRecord>>(iter: T) -> Self {
        let records = iter.into_iter().collect::<Vec<_>>();
        let score = records.iter().map(|r| r.score).sum::<f32>() / records.len() as f32;
        MetricKind::Recall(Recall { score, records })
    }
}
