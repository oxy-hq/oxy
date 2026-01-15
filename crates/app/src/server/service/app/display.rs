use super::app_service::AppService;
use super::types::{AppResult, DISPLAY_KEY, DisplayWithError, ErrorDisplay, TASKS_KEY};
use oxy::adapters::project::manager::ProjectManager;
use oxy::config::model::{Display, Task, TaskType};
use std::path::PathBuf;

pub async fn get_app_displays(
    project_manager: ProjectManager,
    path: &PathBuf,
) -> AppResult<Vec<DisplayWithError>> {
    let app_service = AppService::new(project_manager);
    let mut displays = Vec::new();

    let yaml_content = match app_service.read_yaml_file(path).await {
        Ok(content) => content,
        Err(e) => {
            displays.push(create_error_display("App config", &e.to_string()));
            return Ok(displays);
        }
    };

    let root_map = match parse_yaml_to_mapping(&yaml_content) {
        Ok(map) => map,
        Err(e) => {
            displays.push(create_error_display("App config", &e.to_string()));
            return Ok(displays);
        }
    };

    validate_tasks_section(&root_map, &mut displays);
    process_displays_section(&root_map, &mut displays);

    Ok(displays)
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
