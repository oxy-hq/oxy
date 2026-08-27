use super::app_service::read_app_yaml_file;
use super::types::{AppResult, DISPLAY_KEY, DisplayWithError, ErrorDisplay, TASKS_KEY};
use oxy::adapters::workspace::manager::WorkspaceManager;
use oxy::config::model::{ControlConfig, Display, Task, TaskType};
use oxy::config::{ConfigManager, DiskSlot, ResolveWorkspaceFile};
use oxy_shared::errors::OxyError;
use std::path::PathBuf;

const CONTROLS_KEY: &str = "controls";

/// Displays, controls, and the parsed task list, from ONE read of the app YAML.
///
/// The tasks come back with the rest because the caller needs them and this has
/// already parsed them. Fetching them separately meant a second resolution
/// through `AppService`, which is pinned to a working copy — so a handler that
/// otherwise reads only the compile boundary had to claim a disk for a list it
/// was holding.
pub async fn get_app_displays<S: DiskSlot + Send + Sync>(
    workspace_manager: WorkspaceManager<S>,
    path: &PathBuf,
) -> AppResult<(Vec<DisplayWithError>, Vec<ControlConfig>, Vec<Task>)>
where
    ConfigManager<S>: ResolveWorkspaceFile,
{
    let mut displays = Vec::new();

    let yaml_content = match resolve_app_yaml(&workspace_manager, path).await {
        Ok(content) => content,
        Err(e) => {
            displays.push(create_error_display("App config", &e.to_string()));
            return Ok((displays, vec![], vec![]));
        }
    };

    let root_map = match parse_yaml_to_mapping(&yaml_content) {
        Ok(map) => map,
        Err(e) => {
            displays.push(create_error_display("App config", &e.to_string()));
            return Ok((displays, vec![], vec![]));
        }
    };

    let tasks = validate_tasks_section(&root_map, &mut displays);
    process_displays_section(&root_map, &mut displays);
    let mut controls = parse_controls_section(&root_map, &mut displays);

    // Extract any inline `type: controls` blocks from the display list.
    // Their items are merged into the controls vec and the blocks are removed
    // from displays so clients never see them as raw display items.
    let mut inline_controls: Vec<ControlConfig> = Vec::new();
    displays.retain(|d| match d {
        DisplayWithError::Display(Display::Controls(c)) => {
            inline_controls.extend(c.items.iter().cloned());
            false
        }
        DisplayWithError::Display(Display::Control(c)) => {
            inline_controls.push(ControlConfig::from(c.clone()));
            false
        }
        _ => true,
    });
    controls.extend(inline_controls);

    Ok((displays, controls, tasks))
}

/// Resolve the app's YAML body, preferring the compile boundary so the
/// stateless serve fleet (which has no working copy on disk) can render
/// displays, falling back to the filesystem on any miss/error. The compiled
/// definition is re-serialised to YAML so the tolerant per-block parser below
/// is unchanged — one malformed display still degrades to a single error card
/// instead of blanking the dashboard.
async fn resolve_app_yaml<S: DiskSlot + Send + Sync>(
    workspace_manager: &WorkspaceManager<S>,
    path: &PathBuf,
) -> AppResult<String>
where
    ConfigManager<S>: ResolveWorkspaceFile,
{
    let path_str = path.to_string_lossy().to_string();
    // `app_definition` already falls back to the working copy, so `Err` here
    // means there was no source to read — not "not compiled". `unwrap_or(None)`
    // used to collapse those and hand the miss to `read_app_yaml_file`, which
    // on a replica reads a directory that is not there.
    if let Some(definition) = workspace_manager
        .config_manager
        .app_definition(&path_str)
        .await
        .map_err(|e| OxyError::RuntimeError(format!("{path_str}: {e}")))?
        && let Ok(yaml) = serde_yaml::to_string(&definition)
    {
        return Ok(yaml);
    }
    read_app_yaml_file(workspace_manager, path).await
}

fn parse_controls_section(
    root_map: &serde_yaml::Mapping,
    displays: &mut Vec<DisplayWithError>,
) -> Vec<ControlConfig> {
    let mut controls = Vec::new();
    if let Some(serde_yaml::Value::Sequence(seq)) = root_map.get(yaml_string_value(CONTROLS_KEY)) {
        for (index, v) in seq.iter().enumerate() {
            match serde_yaml::from_value::<ControlConfig>(v.clone()) {
                Ok(c) => controls.push(c),
                Err(e) => {
                    tracing::warn!("Skipping malformed control at index {index}: {e}");
                    displays.push(create_error_display(
                        &format!("Control at index {index}"),
                        &e.to_string(),
                    ));
                }
            }
        }
    }
    controls
}

fn create_error_display(title: &str, error: &str) -> DisplayWithError {
    DisplayWithError::Error(ErrorDisplay {
        title: title.to_string(),
        error: error.to_string(),
    })
}

fn parse_yaml_to_mapping(yaml_content: &str) -> Result<serde_yaml::Mapping, String> {
    let yaml_value: serde_yaml::Value =
        serde_yaml::from_str(yaml_content).map_err(|e| format!("Failed to parse YAML: {e}"))?;

    match yaml_value {
        serde_yaml::Value::Mapping(map) => Ok(map),
        _ => Err("Expected YAML object at root".to_string()),
    }
}

fn yaml_string_value(s: &str) -> serde_yaml::Value {
    serde_yaml::Value::String(s.to_string())
}

fn process_sequence_with_error_handling<T, F>(
    root_map: &serde_yaml::Mapping,
    key: &str,
    displays: &mut Vec<DisplayWithError>,
    item_name: &str,
    processor: F,
) where
    F: Fn(&serde_yaml::Value, usize) -> Result<Option<T>, String>,
    T: Into<DisplayWithError>,
{
    if let Some(serde_yaml::Value::Sequence(seq)) = root_map.get(yaml_string_value(key)) {
        for (index, item_value) in seq.iter().enumerate() {
            match processor(item_value, index) {
                Ok(Some(item)) => {
                    displays.push(item.into());
                }
                Ok(None) => {}
                Err(error) => {
                    displays.push(create_error_display(
                        &format!("{item_name} at index {index}"),
                        &error,
                    ));
                }
            }
        }
    }
}

/// Validate the `tasks:` section AND hand back what it parsed.
///
/// It was already deserialising every task to check the type; discarding the
/// result meant the only other caller had to read the file again through a
/// filesystem-bound path.
fn validate_tasks_section(
    root_map: &serde_yaml::Mapping,
    displays: &mut Vec<DisplayWithError>,
) -> Vec<Task> {
    let mut tasks = Vec::new();
    let Some(serde_yaml::Value::Sequence(seq)) = root_map.get(yaml_string_value(TASKS_KEY)) else {
        return tasks;
    };
    for (index, task_value) in seq.iter().enumerate() {
        match serde_yaml::from_value::<Task>(task_value.clone()) {
            Ok(task) if matches!(task.task_type, TaskType::Unknown) => displays.push(
                create_error_display(&format!("Task at index {index}"), "Unknown task type"),
            ),
            Ok(task) => tasks.push(task),
            Err(e) => displays.push(create_error_display(
                &format!("Task at index {index}"),
                &e.to_string(),
            )),
        }
    }
    tasks
}

fn process_displays_section(root_map: &serde_yaml::Mapping, displays: &mut Vec<DisplayWithError>) {
    process_sequence_with_error_handling(
        root_map,
        DISPLAY_KEY,
        displays,
        "Display",
        |display_value, _index| match serde_yaml::from_value::<Display>(display_value.clone()) {
            Ok(display) => Ok(Some(DisplayWithError::Display(display))),
            Err(e) => Err(format!("{e:?}")),
        },
    );
}
