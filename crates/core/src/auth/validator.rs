use axum::http::{HeaderMap, StatusCode};

use super::types::Identity;

pub trait Validator {
    type Error: std::error::Error + Into<StatusCode>;

    fn extract_token(&self, header: &HeaderMap) -> Result<String, Self::Error>;
    fn validate(&self, value: &str) -> Result<Identity, Self::Error>;
    fn verify(&self, header: &HeaderMap) -> Result<Identity, Self::Error> {
        let token = self.extract_token(header)?;
        self.validate(&token)
    }
}
