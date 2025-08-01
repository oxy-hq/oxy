use crate::errors::OxyError;

pub fn map_config_error<E: std::fmt::Display>(context: &str) -> impl Fn(E) -> OxyError + '_ {
    move |e| OxyError::ConfigurationError(format!("{}: {}", context, e))
}

pub fn map_runtime_error<E: std::fmt::Display>(context: &str) -> impl Fn(E) -> OxyError + '_ {
    move |e| OxyError::RuntimeError(format!("{}: {}", context, e))
}

pub fn yaml_string_value(s: &str) -> serde_yaml::Value {
    serde_yaml::Value::String(s.to_string())
}

pub fn generate_task_hash(tasks: &[crate::config::model::Task]) -> super::types::AppResult<String> {
    use xxhash_rust::xxh3::xxh3_64;

    let tasks_serialized =
        serde_json::to_string(tasks).map_err(map_runtime_error("Failed to serialize tasks"))?;
    let tasks_hash = xxh3_64(tasks_serialized.as_bytes());
    Ok(format!("{:x}", tasks_hash))
}
