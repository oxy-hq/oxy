//! Route handlers:
//!   POST   /runs           — create a run, start pipeline in background
//!   GET    /runs/:id/events — SSE stream (live + postgres catch-up)
//!   POST   /runs/:id/answer — deliver user answer to a suspended run

use serde::{Deserialize, Serialize};

use crate::sse;

// ── Request / response types ──────────────────────────────────────────────────

pub use agentic_pipeline::ThinkingMode;

#[derive(Deserialize)]
pub struct CreateRunRequest {
    pub agent_id: String,
    pub question: String,
    pub thread_id: Option<String>,
    #[serde(default)]
    pub domain: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub thinking_mode: ThinkingMode,
}

#[derive(Serialize)]
pub struct CreateRunResponse {
    pub run_id: String,
    pub thread_id: Option<String>,
}

#[derive(Deserialize)]
pub struct AnswerRequest {
    pub answer: String,
}

#[derive(Deserialize)]
pub struct RunIdPath {
    id: String,
}

#[derive(Deserialize)]
pub struct ThreadIdPath {
    thread_id: String,
}

#[derive(Serialize)]
pub struct RunSummary {
    pub run_id: String,
    pub status: String,
    pub agent_id: String,
    pub question: String,
    pub answer: Option<String>,
    pub error_message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thinking_mode: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ui_events: Option<Vec<sse::UiEvent>>,
}

pub mod run;
pub mod thread;

pub use run::{
    UpdateThinkingModeRequest, answer_run, cancel_run, create_run, stream_events,
    update_thinking_mode,
};
pub use thread::{get_run_by_thread, list_runs_by_thread};
