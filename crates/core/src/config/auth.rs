use garde::Validate;
use serde::{Deserialize, Serialize};

use crate::config::validate::{ValidationContext, validate_env_var};
use schemars::JsonSchema;

#[derive(Serialize, Deserialize, Validate, Debug, Clone, JsonSchema)]
#[garde(context(ValidationContext))]
pub struct Authentication {
    #[garde(dive)]
    pub basic: Option<BasicAuth>,
    #[garde(dive)]
    pub google: Option<GoogleAuth>,
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
