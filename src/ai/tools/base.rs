use async_trait::async_trait;
use schemars::{schema_for, JsonSchema};
use serde::de::DeserializeOwned;
use serde_json::json;

#[async_trait]
pub trait Tool {
    type Input: DeserializeOwned + JsonSchema + Send + Sync;

    fn name(&self) -> String;
    fn description(&self) -> String;
    fn param_spec(&self) -> anyhow::Result<serde_json::Value> {
        let spec = json!(&schema_for!(Self::Input));
        Ok(spec)
    }
    fn validate(&self, parameters: &str) -> anyhow::Result<Self::Input> {
        serde_json::from_str::<Self::Input>(parameters).map_err(|e| e.into())
    }
    async fn call(&self, parameters: &str) -> anyhow::Result<String> {
        let params = self.validate(parameters)?;
        self.call_internal(&params).await
    }
    async fn call_internal(&self, parameters: &Self::Input) -> anyhow::Result<String>;
}
