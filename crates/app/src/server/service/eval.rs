use std::{collections::HashMap, path::Path, sync::Arc};

use indicatif::{ProgressBar, ProgressStyle};
use itertools::Itertools;
use tokio::sync::Mutex;

use oxy::{
    adapters::project::manager::ProjectManager,
    config::constants::EVAL_SOURCE,
    execute::{
        types::{Event, EventKind, ProgressType},
        writer::EventHandler,
    },
};
use oxy_shared::errors::OxyError;

use crate::integrations::eval::{EvalInput, EvalLauncher, EvalResult};

/// Global token stats accumulated across all eval runs, shared between
/// the event handler and the calling command.
#[derive(Debug, Default, Clone)]
pub struct TokenStats {
    pub total_input_tokens: i64,
    pub total_output_tokens: i64,
}

pub type SharedTokenStats = Arc<Mutex<TokenStats>>;

/// General-purpose progress bar handler using indicatif.
/// Used by agent, workflow, and eval event handlers.
pub struct PBarsHandler {
    bars: HashMap<String, ProgressBar>,
}

impl Default for PBarsHandler {
    fn default() -> Self {
        Self::new()
    }
}

impl PBarsHandler {
    pub fn new() -> Self {
        PBarsHandler {
            bars: HashMap::new(),
        }
    }

    pub fn get_or_create_bar(&mut self, name: &str, total: Option<usize>) -> &mut ProgressBar {
        self.bars.entry(name.to_string()).or_insert_with(|| {
            let pb = match total {
                Some(total) => {
                    let pb = ProgressBar::new(total as u64);
                    pb.set_style(
                        ProgressStyle::with_template(
                            "  {spinner:.green} {msg} [{bar:30.green/white}] {pos}/{len}",
                        )
                        .unwrap()
                        .tick_chars("|/-\\")
                        .progress_chars("## "),
                    );
                    pb
                }
                None => {
                    let pb = ProgressBar::new_spinner();
                    pb.set_style(
                        ProgressStyle::with_template("  {spinner:.green} {msg}")
                            .unwrap()
                            .tick_chars("|/-\\"),
                    );
                    pb
                }
            };
            pb.enable_steady_tick(std::time::Duration::from_millis(80));
            pb
        })
    }

    pub fn update_bar(&mut self, name: &str, progress: usize) -> Result<(), OxyError> {
        if let Some(bar) = self.bars.get_mut(name) {
            bar.inc(progress as u64);
        }
        Ok(())
    }

    pub fn remove_bar(&mut self, name: &str) {
        if let Some(bar) = self.bars.remove(name) {
            bar.finish_and_clear();
        }
    }
}

const SPINNER_CHARS: &str = "⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏";
const TICK_MS: u64 = 80;

enum Phase {
    Idle,
    Generating,
    Evaluating,
}

pub struct EvalEventsHandler {
    quiet: bool,
    phase: Phase,
    progress_bar: Option<ProgressBar>,
    eval_total: u64,
    eval_completed: u64,
    token_stats: SharedTokenStats,
    test_label: Option<String>,
}

impl EvalEventsHandler {
    pub fn new(quiet: bool, token_stats: SharedTokenStats) -> Self {
        EvalEventsHandler {
            quiet,
            phase: Phase::Idle,
            progress_bar: None,
            eval_total: 0,
            eval_completed: 0,
            token_stats,
            test_label: None,
        }
    }

    pub fn with_test_label(mut self, label: String) -> Self {
        self.test_label = Some(label);
        self
    }

    fn set_spinner(&mut self, msg: String) {
        if self.quiet {
            return;
        }
        if let Some(pb) = self.progress_bar.take() {
            pb.finish_and_clear();
        }
        let pb = ProgressBar::new_spinner();
        pb.set_style(
            ProgressStyle::with_template("  {spinner:.green} {msg}")
                .unwrap()
                .tick_chars(SPINNER_CHARS),
        );
        pb.set_message(msg);
        pb.enable_steady_tick(std::time::Duration::from_millis(TICK_MS));
        self.progress_bar = Some(pb);
    }

    fn set_progress_bar(&mut self, msg: String, total: u64, pos: u64) {
        if self.quiet {
            return;
        }
        if let Some(pb) = self.progress_bar.take() {
            pb.finish_and_clear();
        }
        let pb = ProgressBar::new(total);
        pb.set_style(
            ProgressStyle::with_template(
                "  {spinner:.green} {msg} [{bar:30.green}] {pos} of {len}",
            )
            .unwrap()
            .tick_chars(SPINNER_CHARS)
            .progress_chars("█░"),
        );
        pb.set_message(msg);
        pb.set_position(pos);
        pb.enable_steady_tick(std::time::Duration::from_millis(TICK_MS));
        self.progress_bar = Some(pb);
    }

    fn finish_progress(&mut self) {
        if let Some(pb) = self.progress_bar.take() {
            pb.finish_and_clear();
        }
    }
}

#[async_trait::async_trait]
impl EventHandler for EvalEventsHandler {
    async fn handle_event(&mut self, event: Event) -> Result<(), OxyError> {
        tracing::debug!("Received event: {:?}", event);

        // Capture token usage from any source (agent runs + judge LLM calls)
        if let EventKind::Usage { usage } = &event.kind {
            let mut stats = self.token_stats.lock().await;
            stats.total_input_tokens += usage.input_tokens as i64;
            stats.total_output_tokens += usage.output_tokens as i64;
            return Ok(());
        }

        match event.source.kind.as_str() {
            EVAL_SOURCE => match event.kind {
                EventKind::Started { name, .. } => {
                    self.set_spinner(format!("Starting {name}"));
                }
                EventKind::Finished { .. } => {
                    self.finish_progress();
                }
                EventKind::Progress { progress } => {
                    let phase_msg = match &self.phase {
                        Phase::Generating => "Running test cases...",
                        Phase::Evaluating => "Judging responses...",
                        Phase::Idle => "Processing...",
                    };
                    let msg = match &self.test_label {
                        Some(label) => format!("{phase_msg} [{label}]"),
                        None => phase_msg.to_string(),
                    };
                    match progress {
                        ProgressType::Started(total) => {
                            let total = total.unwrap_or(0) as u64;
                            self.eval_total = total;
                            self.eval_completed = 0;
                            self.set_progress_bar(msg, total, 0);
                        }
                        ProgressType::Updated(n) => {
                            self.eval_completed += n as u64;
                            if let Some(pb) = &self.progress_bar {
                                pb.set_position(self.eval_completed);
                            }
                        }
                        ProgressType::Finished => {
                            self.finish_progress();
                        }
                    }
                }
                EventKind::Message { message } => {
                    let stripped = strip_ansi_escapes::strip_str(&message);
                    if stripped.contains("Generating outputs") {
                        self.phase = Phase::Generating;
                        let msg = match &self.test_label {
                            Some(label) => format!("Running test cases... [{label}]"),
                            None => "Running test cases...".to_string(),
                        };
                        self.set_spinner(msg);
                    } else if stripped.contains("Evaluating records") {
                        self.phase = Phase::Evaluating;
                        self.eval_total = 0;
                        self.eval_completed = 0;
                        let msg = match &self.test_label {
                            Some(label) => format!("Judging responses... [{label}]"),
                            None => "Judging responses...".to_string(),
                        };
                        self.set_spinner(msg);
                    }
                    // Suppress all other messages.
                }
                _ => {}
            },
            _ => {
                tracing::debug!("Non-eval event: {:?}", event);
            }
        }

        Ok(())
    }
}

pub async fn run_eval<P: AsRef<Path>, H: EventHandler + Send + 'static>(
    project_manager: ProjectManager,
    path: P,
    index: Option<usize>,
    event_handler: H,
) -> Result<Vec<EvalResult>, OxyError> {
    run_eval_with_tag(project_manager, path, index, None, event_handler).await
}

pub async fn run_eval_with_tag<P: AsRef<Path>, H: EventHandler + Send + 'static>(
    project_manager: ProjectManager,
    path: P,
    index: Option<usize>,
    tag: Option<String>,
    event_handler: H,
) -> Result<Vec<EvalResult>, OxyError> {
    let result = EvalLauncher::new()
        .with_project(project_manager)
        .await?
        .launch(
            EvalInput {
                index,
                target_ref: path.as_ref().to_string_lossy().to_string(),
                tag,
            },
            event_handler,
        )
        .await;
    result.and_then(|r| {
        r.into_iter()
            .try_collect::<EvalResult, Vec<EvalResult>, OxyError>()
    })
}
