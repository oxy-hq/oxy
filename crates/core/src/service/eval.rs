use std::{collections::HashMap, path::Path};

use tqdm::{Pbar, pbar};

use crate::{
    config::constants::EVAL_SOURCE,
    errors::OxyError,
    eval::EvalLauncher,
    execute::{
        eval::run_eval_legacy,
        types::{Event, EventKind, ProgressType},
        writer::EventHandler,
    },
    utils::find_project_path,
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

struct EvalEventsHandler {
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
        log::debug!("Received event: {:?}", event);
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
            _ => log::debug!("Unknown source: {:?}", event),
        }

        Ok(())
    }
}

async fn run_eval_with_builders(eval_input: EvalInput) -> Result<(), OxyError> {
    let project_path = find_project_path()?;
    let quiet = eval_input.quiet;
    EvalLauncher::new()
        .with_project_path(project_path)
        .await?
        .launch(eval_input, EvalEventsHandler::new(quiet))
        .await?;
    Ok(())
}

pub async fn run_eval<P: AsRef<Path>>(path: P, quiet: bool) -> Result<(), OxyError> {
    #[cfg(not(feature = "builders"))]
    return run_eval_legacy(path, quiet).await;
    #[cfg(feature = "builders")]
    return run_eval_with_builders(EvalInput {
        target_ref: path.as_ref().to_string_lossy().to_string(),
        quiet,
    })
    .await;
}
