//! Domain events for the app builder pipeline.

use agentic_core::events::DomainEvents;
use serde::{Deserialize, Serialize};

use crate::types::AppSpec;

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "event_type", rename_all = "snake_case")]
pub enum AppBuilderEvent {
    TaskPlanReady {
        task_count: usize,
        control_count: usize,
        spec: AppSpec,
    },
    TaskSqlResolved {
        task_name: String,
        sql: String,
    },
    TaskExecuted {
        task_name: String,
        sql: String,
        row_count: usize,
        columns: Vec<String>,
        sample_rows: Vec<Vec<String>>,
    },
    /// A task execution failed — SQL error, empty results, shape mismatch, etc.
    /// Emitted so the frontend can surface the failing query and reason.
    TaskExecutionFailed {
        task_name: String,
        sql: String,
        error: String,
        will_retry: bool,
    },
    AppYamlReady {
        char_count: usize,
    },
}

impl DomainEvents for AppBuilderEvent {}

impl AppBuilderEvent {
    /// Whether this event should be accumulated into the `step_end` metadata
    /// payload for frontend debugging tooltips.
    pub fn is_accumulated(&self) -> bool {
        matches!(
            self,
            Self::TaskPlanReady { .. }
                | Self::TaskSqlResolved { .. }
                | Self::TaskExecuted { .. }
                | Self::TaskExecutionFailed { .. }
                | Self::AppYamlReady { .. }
        )
    }
}
