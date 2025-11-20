use schemars::JsonSchema;
use sea_orm::FromQueryResult;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, Hash, ToSchema, FromQueryResult)]
pub struct Usage {
    /// Number of tokens in the prompt.
    #[serde(rename = "inputTokens")]
    pub input_tokens: i32,
    /// Number of tokens in the generated completion.
    #[serde(rename = "outputTokens")]
    pub output_tokens: i32,
}

impl Usage {
    pub fn new(input_tokens: i32, output_tokens: i32) -> Self {
        Usage {
            input_tokens,
            output_tokens,
        }
    }

    pub fn add(&mut self, other: &Usage) {
        self.input_tokens += other.input_tokens;
        self.output_tokens += other.output_tokens;
    }
}
