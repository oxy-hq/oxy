use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Subflow {
    pub name: String,
    pub description: String,
    pub src: String,
}
