use std::env;

#[derive(Clone, Debug)]
pub struct StripeConfig {
    pub secret_key: String,
    pub webhook_signing_secret: String,
    pub public_url: String,
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("missing required env var: {0}")]
    MissingVar(&'static str),
}

impl StripeConfig {
    pub fn from_env() -> Result<Self, ConfigError> {
        fn required(k: &'static str) -> Result<String, ConfigError> {
            env::var(k).map_err(|_| ConfigError::MissingVar(k))
        }
        Ok(Self {
            secret_key: required("STRIPE_SECRET_KEY")?,
            webhook_signing_secret: required("STRIPE_WEBHOOK_SIGNING_SECRET")?,
            public_url: required("OXY_PUBLIC_URL")?,
        })
    }

    pub fn maybe_from_env() -> Option<Self> {
        Self::from_env().ok()
    }
}
