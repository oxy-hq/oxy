use async_trait::async_trait;
use schemars::{schema_for, JsonSchema};
use serde::Deserialize;
use serde_json::json;

#[async_trait]
pub trait Tool<S>
where
    S: for<'a> Deserialize<'a> + JsonSchema,
{
    fn name(&self) -> String;
    fn description(&self) -> String;
    fn param_spec(&self) -> serde_json::Value {
        json!(&schema_for!(S))
    }
    fn validate(&self, parameters: &String) -> Result<S, Box<dyn std::error::Error>> {
        serde_json::from_str::<S>(parameters).map_err(|e| e.into())
    }
    async fn call(&self, parameters: String) -> Result<String, Box<dyn std::error::Error>> {
        let params = self.validate(&parameters)?;
        self.call_internal(params).await
    }
    async fn setup(&mut self) {}
    async fn call_internal(&self, parameters: S) -> Result<String, Box<dyn std::error::Error>>;
}
