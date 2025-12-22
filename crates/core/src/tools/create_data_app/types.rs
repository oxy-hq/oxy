use garde::Validate;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::config::model::AppConfig;
use crate::config::validate::ValidationContext;

#[derive(Deserialize, Debug, JsonSchema, Serialize, Validate)]
#[garde(context(ValidationContext))]
pub struct CreateDataAppParams {
    #[schemars(description = "The file name of the data app file without the extension")]
    #[garde(length(min = 1, max = 255))]
    #[garde(custom(validate_safe_filename))]
    pub file_name: String,

    #[schemars(description = "The data app config")]
    #[garde(dive)]
    pub app_config: AppConfig,
}

fn validate_safe_filename(value: &str, _ctx: &ValidationContext) -> garde::Result {
    if value.contains('/') || value.contains('\\') {
        return Err(garde::Error::new("filename cannot contain path separators"));
    }
    if value == "." || value == ".." {
        return Err(garde::Error::new("filename cannot be '.' or '..'"));
    }
    if value.contains('\0') {
        return Err(garde::Error::new("filename cannot contain null bytes"));
    }
    Ok(())
}
