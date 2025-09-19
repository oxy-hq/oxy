use crate::config::auth::Authentication;
use garde::Validate;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::sync::OnceLock;

use crate::errors::OxyError;

static OXY_CONFIG: OnceLock<Result<OxyConfig, OxyError>> = OnceLock::new();

#[derive(Serialize, Deserialize, Validate, Debug, Clone, JsonSchema, Default)]
pub struct OxyConfig {
    #[garde(dive)]
    pub authentication: Option<Authentication>,
}

fn load_oxy_config() -> Result<OxyConfig, OxyError> {
    let authentication = Authentication::from_env()?;
    Ok(OxyConfig {
        authentication: Some(authentication),
    })
}

pub fn get_oxy_config() -> Result<OxyConfig, OxyError> {
    let config_result =
        OXY_CONFIG.get_or_init(|| load_oxy_config().or_else(|_| Ok(OxyConfig::default())));

    match config_result {
        Ok(config) => Ok(config.clone()),
        Err(e) => Err(OxyError::RuntimeError(e.to_string())),
    }
}
