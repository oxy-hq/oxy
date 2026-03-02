use async_trait::async_trait;
use oxy_shared::errors::OxyError;

pub mod local_test;
pub mod ses;

pub struct EmailMessage {
    pub subject: String,
    pub html_body: String,
    pub text_body: String,
}

#[async_trait]
pub trait EmailProvider: Send + Sync {
    async fn send(&self, from: &str, to: &str, message: EmailMessage) -> Result<(), OxyError>;
}
