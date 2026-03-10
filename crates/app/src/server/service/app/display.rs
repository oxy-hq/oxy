use super::app_service::AppService;
use super::types::{AppResult, DISPLAY_KEY, DisplayWithError, ErrorDisplay, TASKS_KEY};
use oxy::adapters::project::manager::ProjectManager;
use oxy::config::model::{ControlConfig, Display, Task, TaskType};
use std::path::PathBuf;

const CONTROLS_KEY: &str = "controls";

pub async fn get_app_displays(
    project_manager: ProjectManager,
    path: &PathBuf,
) -> AppResult<(Vec<DisplayWithError>, Vec<ControlConfig>)> {
    let app_service = AppService::new(project_manager);
    let mut displays = Vec::new();

    let yaml_content = match app_service.read_yaml_file(path).await {
        Ok(content) => content,
        Err(e) => {
            displays.push(create_error_display("App config", &e.to_string()));
            return Ok((displays, vec![]));
        }
    };

    let root_map = match parse_yaml_to_mapping(&yaml_content) {
        Ok(map) => map,
        Err(e) => {
            displays.push(create_error_display("App config", &e.to_string()));
            return Ok((displays, vec![]));
        }
    };

    validate_tasks_section(&root_map, &mut displays);
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

    Ok((displays, controls))
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

fn validate_tasks_section(root_map: &serde_yaml::Mapping, displays: &mut Vec<DisplayWithError>) {
    process_sequence_with_error_handling(
        root_map,
        TASKS_KEY,
        displays,
        "Task",
        |task_value, _index| match serde_yaml::from_value::<Task>(task_value.clone()) {
            Ok(task) => {
                if matches!(task.task_type, TaskType::Unknown) {
                    Err("Unknown task type".to_string())
                } else {
                    Ok(None::<DisplayWithError>)
                }
            }
            Err(e) => Err(e.to_string()),
        },
    );
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
