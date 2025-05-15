use std::{collections::HashMap, path::Path};

use itertools::Itertools;
use tqdm::{Pbar, pbar};

use crate::{
    config::constants::EVAL_SOURCE,
    errors::OxyError,
    eval::{EvalLauncher, EvalResult},
    execute::{
        types::{Event, EventKind, ProgressType},
        writer::EventHandler,
    },
};

pub use crate::eval::EvalInput;

pub struct PBarsHandler {
    bars: HashMap<String, Pbar>,
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

    pub fn get_or_create_bar(&mut self, name: &str, total: Option<usize>) -> &mut Pbar {
        let bar = self.bars.entry(name.to_string()).or_insert(pbar(total));
        bar
    }

    pub fn update_bar(&mut self, name: &str, progress: usize) -> Result<(), OxyError> {
        if let Some(bar) = self.bars.get_mut(name) {
            bar.update(progress).map_err(|err| {
                OxyError::RuntimeError(format!("Failed to update progress bar:\n{:?}", err))
            })?;
        }
        Ok(())
    }

    pub fn remove_bar(&mut self, name: &str) {
        self.bars.remove_entry(name);
    }
}

pub struct EvalEventsHandler {
    quiet: bool,
    pbar_handler: PBarsHandler,
}

impl EvalEventsHandler {
    pub fn new(quiet: bool) -> Self {
        EvalEventsHandler {
            quiet,
            pbar_handler: PBarsHandler::new(),
        }
    }
}

#[async_trait::async_trait]
impl EventHandler for EvalEventsHandler {
    async fn handle_event(&mut self, event: Event) -> Result<(), OxyError> {
        tracing::debug!("Received event: {:?}", event);
        match event.source.kind.as_str() {
            EVAL_SOURCE => match event.kind {
                EventKind::Started { name } => {
                    println!("⏳Starting {}", name);
                }
                EventKind::Finished { message } => {
                    if !self.quiet {
                        println!("{}", message);
                    }
                }
                EventKind::Progress { progress } => match progress {
                    ProgressType::Started(total) => {
                        self.pbar_handler.get_or_create_bar(&event.source.id, total);
                    }
                    ProgressType::Updated(progress) => {
                        self.pbar_handler.update_bar(&event.source.id, progress)?;
                    }
                    ProgressType::Finished => {
                        self.pbar_handler.remove_bar(&event.source.id);
                    }
                },
                EventKind::Message { message } => {
                    println!("{}", message);
                }
                _ => {}
            },
            _ => tracing::debug!("Unknown source: {:?}", event),
        }

        Ok(())
    }
}

pub async fn run_eval<P: AsRef<Path>, H: EventHandler + Send + 'static>(
    project_path: P,
    path: P,
    index: Option<usize>,
    event_handler: H,
) -> Result<Vec<EvalResult>, OxyError> {
    let result = EvalLauncher::new()
        .with_project_path(project_path)
        .await?
        .launch(
            EvalInput {
                index,
                target_ref: path.as_ref().to_string_lossy().to_string(),
            },
            event_handler,
        )
        .await;
    result.and_then(|r| {
        r.into_iter()
            .try_collect::<EvalResult, Vec<EvalResult>, OxyError>()
    })
}
