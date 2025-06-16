use garde::Validate;
use serde::{Deserialize, Serialize};

use crate::config::{constants::DEFAULT_API_KEY_HEADER, validate::ValidationContext};
use schemars::JsonSchema;

#[derive(Serialize, Deserialize, Validate, Debug, Clone, JsonSchema)]
#[garde(context(ValidationContext))]
pub struct Authentication {
    #[garde(dive)]
    pub basic: Option<BasicAuth>,
    #[garde(dive)]
    pub google: Option<GoogleAuth>,
    #[garde(dive)]
    #[serde(default = "default_api_key_config")]
    pub api_key: Option<ApiKeyAuth>,
}

#[derive(Serialize, Deserialize, Validate, Debug, Clone, JsonSchema)]
#[garde(context(ValidationContext))]
pub struct BasicAuth {
    #[garde(length(min = 1))]
    pub smtp_user: String,
    #[garde(length(min = 1))]
    pub smtp_password_var: String,

    #[garde(length(min = 1))]
    pub smtp_server: Option<String>,
    #[garde(range(min = 1, max = 65535))]
    pub smtp_port: Option<u16>,
}

#[derive(Serialize, Deserialize, Validate, Debug, Clone, JsonSchema)]
#[garde(context(ValidationContext))]
pub struct GoogleAuth {
    #[garde(length(min = 1))]
    pub client_id: String,
    #[garde(length(min = 1))]
    pub client_secret_var: String,
}

#[derive(Serialize, Deserialize, Validate, Debug, Clone, JsonSchema)]
#[garde(context(ValidationContext))]
pub struct ApiKeyAuth {
    #[garde(length(min = 1))]
    #[serde(default = "default_api_key_header")]
    pub header: String,
}

fn default_api_key_header() -> String {
    DEFAULT_API_KEY_HEADER.to_string()
}

fn default_api_key_config() -> Option<ApiKeyAuth> {
    Some(ApiKeyAuth {
        header: default_api_key_header(),
    })
}
