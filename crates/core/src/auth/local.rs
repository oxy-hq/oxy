use crate::errors::OxyError;

use super::{types::Identity, validator::Validator};

pub struct LocalValidator;

impl Default for LocalValidator {
    fn default() -> Self {
        Self::new()
    }
}

impl LocalValidator {
    pub fn new() -> Self {
        Self {}
    }
}

impl Validator for LocalValidator {
    type Error = OxyError;

    fn extract_token(&self, _header: &axum::http::HeaderMap) -> Result<String, Self::Error> {
        Ok("".to_string())
    }

    fn validate(&self, _value: &str) -> Result<Identity, Self::Error> {
        Ok(Identity {
            idp_id: None,
            picture: None,
            email: "guest@oxy.local".to_string(),
            name: Some("Guest".to_string()),
        })
    }
}
