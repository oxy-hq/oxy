use crate::errors::OxyError;

pub trait Storage {
    async fn list(&self, key: &str) -> Result<Vec<String>, OxyError>;
    async fn load(&self, key: &str) -> Result<Vec<u8>, OxyError>;
    async fn save(&self, key: &str, value: &[u8]) -> Result<String, OxyError>;
    async fn remove(&self, key: &str) -> Result<(), OxyError>;
    async fn glob(&self, pattern: &str) -> Result<Vec<String>, OxyError>;
}
