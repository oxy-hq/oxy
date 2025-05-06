use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::config::model::AppConfig;

#[derive(Deserialize, Debug, JsonSchema, Serialize)]
pub struct CreateDataAppParams {
    #[schemars(description = "The file name of the data app file without the extension")]
    pub file_name: String,

    #[schemars(description = "The data app config")]
    pub app_config: AppConfig,
}
