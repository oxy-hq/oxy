use garde::Validate;
use oxy::config::model::Display;
use oxy::config::validate::ValidationContext;
use oxy_shared::errors::OxyError;
use serde::{Deserialize, Serialize};

pub const APP_FILE_EXTENSION: &str = ".app.yml";
pub const APP_DATA_EXTENSION: &str = ".app.data.yml";
pub const DATA_DIR_NAME: &str = "data";
pub const TASKS_KEY: &str = "tasks";
pub const DISPLAY_KEY: &str = "display";

pub type AppResult<T> = Result<T, OxyError>;

#[derive(Serialize, Deserialize, Debug, Clone, Validate)]
#[garde(context(ValidationContext))]
pub struct ErrorDisplay {
    #[garde(length(min = 1))]
    pub title: String,
    #[garde(length(min = 1))]
    pub error: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, Validate)]
#[serde(tag = "type")]
#[garde(context(ValidationContext))]
pub enum DisplayWithError {
    #[serde(rename = "error")]
    Error(#[garde(dive)] ErrorDisplay),
    #[serde(rename = "display")]
    Display(#[garde(dive)] Display),
}
