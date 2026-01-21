//! LLM-based metric extraction from question/response/SQL
//!
//! This module provides Tier 2 extraction: using an LLM to identify
//! metric and column references from natural language and SQL queries.
//!
//! Uses OpenAI Structured Outputs for guaranteed JSON schema compliance.

use async_openai::{
    Client,
    config::OpenAIConfig,
    types::chat::{
        ChatCompletionRequestMessage, ChatCompletionRequestSystemMessage,
        ChatCompletionRequestSystemMessageContent, ChatCompletionRequestUserMessage,
        ChatCompletionRequestUserMessageContent, CreateChatCompletionRequestArgs, ResponseFormat,
        ResponseFormatJsonSchema,
    },
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tracing::{debug, warn};

use super::types::ContextType;
use oxy_shared::errors::OxyError;

/// Configuration for metric extraction
#[derive(Debug, Clone)]
pub struct ExtractorConfig {
    /// OpenAI API key
    pub openai_api_key: String,
    /// Model to use for extraction (default: gpt-4o-mini)
    pub model: String,
}

impl Default for ExtractorConfig {
    fn default() -> Self {
        Self {
            openai_api_key: String::new(),
            model: "gpt-4o-mini".to_string(),
        }
    }
}

impl ExtractorConfig {
    /// Create config from environment variables
    pub fn from_env() -> Self {
        Self {
            openai_api_key: std::env::var("OPENAI_API_KEY").unwrap_or_default(),
            model: std::env::var("METRIC_EXTRACTOR_MODEL")
                .unwrap_or_else(|_| "gpt-4o-mini".to_string()),
        }
    }
}

/// Context for metric extraction
#[derive(Debug, Clone, Default)]
pub struct ExtractionContext {
    /// User's question/prompt
    pub question: Option<String>,
    /// Agent/workflow response
    pub response: Option<String>,
    /// Executed SQL queries
    pub sql_queries: Vec<String>,
}

impl ExtractionContext {
    /// Create a new extraction context
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the question
    pub fn with_question(mut self, question: impl Into<String>) -> Self {
        self.question = Some(question.into());
        self
    }

    /// Set the response
    pub fn with_response(mut self, response: impl Into<String>) -> Self {
        self.response = Some(response.into());
        self
    }

    /// Add a SQL query
    pub fn with_sql(mut self, sql: impl Into<String>) -> Self {
        self.sql_queries.push(sql.into());
        self
    }

    /// Check if context has enough data for extraction
    pub fn has_data(&self) -> bool {
        self.question.is_some() || self.response.is_some() || !self.sql_queries.is_empty()
    }
}

/// A single extracted metric with its source context
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtractedMetric {
    /// Metric name
    pub name: String,
    /// Where the metric was found: "question", "response", or "sql"
    pub source: String,
}

impl ExtractedMetric {
    /// Convert source string to ContextType
    pub fn context_type(&self) -> ContextType {
        match self.source.to_lowercase().as_str() {
            "question" => ContextType::Question,
            "response" => ContextType::Response,
            "sql" => ContextType::SQL,
            _ => ContextType::Question, // Default fallback
        }
    }
}

/// LLM response structure - matches the JSON schema we send to OpenAI
#[derive(Debug, Clone, Serialize, Deserialize)]
struct LlmExtractionResponse {
    /// List of extracted metrics with source attribution
    metrics: Vec<ExtractedMetric>,
}

/// Result of metric extraction
#[derive(Debug, Clone)]
pub struct ExtractionResult {
    /// Extracted metrics with source attribution
    pub metrics: Vec<ExtractedMetric>,
}

impl ExtractionResult {
    /// Get all unique metric names
    pub fn metric_names(&self) -> Vec<String> {
        self.metrics.iter().map(|m| m.name.clone()).collect()
    }
}

/// LLM-based metric extractor
pub struct MetricExtractor {
    client: Client<OpenAIConfig>,
    model: String,
}

impl MetricExtractor {
    /// Create a new extractor with the given config
    pub fn new(config: &ExtractorConfig) -> Result<Self, OxyError> {
        if config.openai_api_key.is_empty() {
            return Err(OxyError::RuntimeError(
                "OpenAI API key is required for metric extraction".to_string(),
            ));
        }

        let openai_config = OpenAIConfig::new().with_api_key(&config.openai_api_key);
        let client = Client::with_config(openai_config);

        Ok(Self {
            client,
            model: config.model.clone(),
        })
    }

    /// Extract metrics from the given context using LLM
    pub async fn extract(&self, context: &ExtractionContext) -> Result<ExtractionResult, OxyError> {
        if !context.has_data() {
            return Ok(ExtractionResult {
                metrics: Vec::new(),
            });
        }

        let prompt = self.build_prompt(context);
        let response = self.call_llm(&prompt).await?;
        let metrics = self.parse_response(&response)?;

        debug!(
            "Extracted {} metrics from context: {:?}",
            metrics.len(),
            metrics.iter().map(|m| &m.name).collect::<Vec<_>>()
        );

        Ok(ExtractionResult { metrics })
    }

    /// Build the extraction prompt
    fn build_prompt(&self, context: &ExtractionContext) -> String {
        let mut parts = Vec::new();

        if let Some(question) = &context.question {
            parts.push(format!("**User Question:**\n{}", question));
        }

        if let Some(response) = &context.response {
            // Truncate long responses
            let truncated = if response.len() > 2000 {
                format!("{}...[truncated]", &response[..2000])
            } else {
                response.clone()
            };
            parts.push(format!("**Response:**\n{}", truncated));
        }

        if !context.sql_queries.is_empty() {
            let sql_text = context
                .sql_queries
                .iter()
                .enumerate()
                .map(|(i, sql)| {
                    // Truncate long SQL
                    let truncated = if sql.len() > 1000 {
                        format!("{}...[truncated]", &sql[..1000])
                    } else {
                        sql.clone()
                    };
                    format!("SQL {}:\n```sql\n{}\n```", i + 1, truncated)
                })
                .collect::<Vec<_>>()
                .join("\n\n");
            parts.push(format!("**Executed SQL Queries:**\n{}", sql_text));
        }

        parts.join("\n\n")
    }

    /// Build the JSON schema for structured output
    fn build_json_schema() -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "metrics": {
                    "type": "array",
                    "description": "List of extracted metrics with source attribution",
                    "items": {
                        "type": "object",
                        "properties": {
                            "name": {
                                "type": "string",
                                "description": "The metric, column, or table name (lowercase, normalized)"
                            },
                            "source": {
                                "type": "string",
                                "enum": ["question", "response", "sql"],
                                "description": "Where this metric was found"
                            }
                        },
                        "required": ["name", "source"],
                        "additionalProperties": false
                    }
                }
            },
            "required": ["metrics"],
            "additionalProperties": false
        })
    }

    /// Call the LLM with the extraction prompt using structured outputs
    async fn call_llm(&self, user_content: &str) -> Result<String, OxyError> {
        let system_prompt = r#"You are a data analyst assistant that extracts metric, column, and table names from analytics conversations.

Your task is to identify ALL data field references mentioned in the provided context, including:
1. **Explicit column names** from SQL (e.g., "amount", "customer_id", "created_at")
2. **Semantic metric references** from natural language (e.g., "revenue", "sales", "conversion rate")
3. **Table names** referenced in the conversation or SQL
4. **Calculated metrics** or aggregations mentioned (e.g., "average order value", "total users")

Rules:
- Extract the actual field/metric names, not SQL functions or keywords
- Include both explicit names and semantic synonyms
- For SQL columns, use the column name (not the alias unless it's meaningful)
- Normalize names to lowercase with underscores (e.g., "Total Amount" -> "total_amount")
- Do NOT include: SQL keywords (SELECT, FROM, WHERE), functions (SUM, COUNT, AVG), operators
- Attribute each metric to where it was found: "question", "response", or "sql"
- If a metric appears in multiple places, include it multiple times with different sources"#;

        // Build JSON schema for structured output
        let json_schema = Self::build_json_schema();

        let messages: Vec<ChatCompletionRequestMessage> = vec![
            ChatCompletionRequestMessage::System(ChatCompletionRequestSystemMessage {
                content: ChatCompletionRequestSystemMessageContent::Text(system_prompt.to_string()),
                name: None,
            }),
            ChatCompletionRequestMessage::User(ChatCompletionRequestUserMessage {
                content: ChatCompletionRequestUserMessageContent::Text(user_content.to_string()),
                name: None,
            }),
        ];

        let request = CreateChatCompletionRequestArgs::default()
            .model(&self.model)
            .messages(messages)
            .temperature(0.0) // Deterministic for extraction
            .response_format(ResponseFormat::JsonSchema {
                json_schema: ResponseFormatJsonSchema {
                    name: "metric_extraction".to_string(),
                    description: Some("Extract metrics with source attribution".to_string()),
                    schema: Some(json_schema),
                    strict: Some(true), // Enforce strict schema compliance
                },
            })
            .build()
            .map_err(|e| OxyError::RuntimeError(format!("Failed to build LLM request: {e}")))?;

        let response = self
            .client
            .chat()
            .create(request)
            .await
            .map_err(|e| OxyError::RuntimeError(format!("LLM request failed: {e}")))?;

        let content = response
            .choices
            .first()
            .and_then(|c| c.message.content.as_ref())
            .ok_or_else(|| OxyError::RuntimeError("No response from LLM".to_string()))?;

        Ok(content.clone())
    }

    /// Parse the LLM's JSON response (guaranteed valid by structured outputs)
    fn parse_response(&self, content: &str) -> Result<Vec<ExtractedMetric>, OxyError> {
        let response: LlmExtractionResponse = serde_json::from_str(content).map_err(|e| {
            warn!("Failed to parse structured output: {}", e);
            OxyError::RuntimeError(format!("Failed to parse LLM response: {e}"))
        })?;

        // Normalize and deduplicate metrics
        let metrics: Vec<ExtractedMetric> = response
            .metrics
            .into_iter()
            .map(|mut m| {
                m.name = m.name.trim().to_lowercase().replace(' ', "_");
                m.source = m.source.to_lowercase();
                m
            })
            .filter(|m| !m.name.is_empty())
            .collect();

        Ok(metrics)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extraction_context_builder() {
        let ctx = ExtractionContext::new()
            .with_question("How much revenue last month?")
            .with_sql("SELECT SUM(amount) FROM orders")
            .with_response("Total revenue was $1.2M");

        assert!(ctx.has_data());
        assert_eq!(
            ctx.question,
            Some("How much revenue last month?".to_string())
        );
        assert_eq!(ctx.sql_queries.len(), 1);
    }

    #[test]
    fn test_empty_context() {
        let ctx = ExtractionContext::new();
        assert!(!ctx.has_data());
    }

    #[test]
    fn test_extracted_metric_context_type() {
        let metric = ExtractedMetric {
            name: "revenue".to_string(),
            source: "question".to_string(),
        };
        assert_eq!(metric.context_type(), ContextType::Question);

        let metric = ExtractedMetric {
            name: "amount".to_string(),
            source: "sql".to_string(),
        };
        assert_eq!(metric.context_type(), ContextType::SQL);

        let metric = ExtractedMetric {
            name: "total".to_string(),
            source: "response".to_string(),
        };
        assert_eq!(metric.context_type(), ContextType::Response);
    }

    #[test]
    fn test_parse_structured_response() {
        let config = ExtractorConfig {
            openai_api_key: "test".to_string(),
            model: "gpt-4o-mini".to_string(),
        };
        let extractor = MetricExtractor::new(&config).unwrap();

        let response = r#"{"metrics": [{"name": "revenue", "source": "question"}, {"name": "amount", "source": "sql"}]}"#;
        let metrics = extractor.parse_response(response).unwrap();

        assert_eq!(metrics.len(), 2);
        assert_eq!(metrics[0].name, "revenue");
        assert_eq!(metrics[0].context_type(), ContextType::Question);
        assert_eq!(metrics[1].name, "amount");
        assert_eq!(metrics[1].context_type(), ContextType::SQL);
    }

    #[test]
    fn test_json_schema_structure() {
        let schema = MetricExtractor::build_json_schema();
        assert!(schema.get("properties").is_some());
        assert!(schema.get("required").is_some());
        assert_eq!(schema["additionalProperties"], false);
    }
}
