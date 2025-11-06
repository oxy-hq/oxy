use serde::{Deserialize, Deserializer, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

fn deserialize_optional_u64_from_string<'de, D>(deserializer: D) -> Result<Option<u64>, D::Error>
where
    D: Deserializer<'de>,
{
    let s: Option<String> = Option::deserialize(deserializer)?;
    match s {
        Some(s) => {
            if s.is_empty() {
                Ok(None)
            } else {
                s.parse::<u64>().map(Some).map_err(serde::de::Error::custom)
            }
        }
        None => Ok(None),
    }
}

#[derive(Deserialize, ToSchema)]
pub struct WorkspaceFormData {
    pub name: String,
    pub r#type: String,
}

#[derive(Deserialize, Serialize, ToSchema, Debug, Clone)]
pub struct PostgresConfig {
    pub host: Option<String>,
    pub port: Option<String>,
    pub user: Option<String>,
    pub password: Option<String>,
    pub password_var: Option<String>,
    pub database: Option<String>,
}

#[derive(Deserialize, Serialize, ToSchema, Debug, Clone)]
pub struct RedshiftConfig {
    pub host: Option<String>,
    pub port: Option<String>,
    pub user: Option<String>,
    pub password: Option<String>,
    pub password_var: Option<String>,
    pub database: Option<String>,
}

#[derive(Deserialize, Serialize, ToSchema, Debug, Clone)]
pub struct MysqlConfig {
    pub host: Option<String>,
    pub port: Option<String>,
    pub user: Option<String>,
    pub password: Option<String>,
    pub password_var: Option<String>,
    pub database: Option<String>,
}

#[derive(Deserialize, Serialize, ToSchema, Debug, Clone)]
pub struct ClickHouseConfig {
    pub host: Option<String>,
    pub user: Option<String>,
    pub password: Option<String>,
    pub password_var: Option<String>,
    pub database: Option<String>,
}

#[derive(Deserialize, Serialize, ToSchema, Debug, Clone)]
pub struct BigQueryConfig {
    pub key: Option<String>,
    pub dataset: Option<String>,
    #[serde(deserialize_with = "deserialize_optional_u64_from_string")]
    pub dry_run_limit: Option<u64>,
}

#[derive(Deserialize, Serialize, ToSchema, Debug, Clone)]
pub struct DuckDBConfig {
    pub file_search_path: Option<String>,
}

#[derive(Deserialize, Serialize, ToSchema, Debug, Clone)]
pub struct SnowflakeConfig {
    pub account: Option<String>,
    pub username: Option<String>,
    pub password: Option<String>,
    pub password_var: Option<String>,
    pub warehouse: Option<String>,
    pub database: Option<String>,
    pub schema: Option<String>,
    pub role: Option<String>,
    pub private_key_path: Option<String>,
}

#[derive(Deserialize, ToSchema)]
pub struct WarehouseConfig {
    pub r#type: String,
    pub name: Option<String>,
    pub config: serde_json::Value,
}

impl WarehouseConfig {
    pub fn get_postgres_config(&self) -> PostgresConfig {
        serde_json::from_value::<PostgresConfig>(self.config.clone()).unwrap_or({
            PostgresConfig {
                host: None,
                port: None,
                user: None,
                password: None,
                password_var: None,
                database: None,
            }
        })
    }

    pub fn get_redshift_config(&self) -> RedshiftConfig {
        serde_json::from_value::<RedshiftConfig>(self.config.clone()).unwrap_or({
            RedshiftConfig {
                host: None,
                port: None,
                user: None,
                password: None,
                password_var: None,
                database: None,
            }
        })
    }

    pub fn get_mysql_config(&self) -> MysqlConfig {
        serde_json::from_value::<MysqlConfig>(self.config.clone()).unwrap_or(MysqlConfig {
            host: None,
            port: None,
            user: None,
            password: None,
            password_var: None,
            database: None,
        })
    }

    pub fn get_clickhouse_config(&self) -> ClickHouseConfig {
        serde_json::from_value::<ClickHouseConfig>(self.config.clone()).unwrap_or({
            ClickHouseConfig {
                host: None,
                user: None,
                password: None,
                password_var: None,
                database: None,
            }
        })
    }

    pub fn get_bigquery_config(&self) -> BigQueryConfig {
        serde_json::from_value::<BigQueryConfig>(self.config.clone()).unwrap_or({
            BigQueryConfig {
                key: None,
                dataset: None,
                dry_run_limit: None,
            }
        })
    }

    pub fn get_duckdb_config(&self) -> DuckDBConfig {
        serde_json::from_value::<DuckDBConfig>(self.config.clone()).unwrap_or({
            DuckDBConfig {
                file_search_path: None,
            }
        })
    }

    pub fn get_snowflake_config(&self) -> SnowflakeConfig {
        serde_json::from_value::<SnowflakeConfig>(self.config.clone()).unwrap_or({
            SnowflakeConfig {
                account: None,
                username: None,
                password: None,
                password_var: None,
                warehouse: None,
                database: None,
                schema: None,
                role: None,
                private_key_path: None,
            }
        })
    }
}

#[derive(Deserialize, ToSchema)]
pub struct WarehousesFormData {
    pub warehouses: Vec<WarehouseConfig>,
}

#[derive(Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum ModelVendor {
    OpenAI,
    Google,
    Anthropic,
    Ollama,
}

#[derive(Deserialize, Serialize, ToSchema, Debug, Clone)]
pub struct OpenAIModelConfig {
    pub model_ref: Option<String>,
    pub api_key: Option<String>,
    pub api_url: Option<String>,
}

#[derive(Deserialize, Serialize, ToSchema, Debug, Clone)]
pub struct GoogleModelConfig {
    pub model_ref: Option<String>,
    pub api_key: Option<String>,
    pub project_id: Option<String>,
}

#[derive(Deserialize, Serialize, ToSchema, Debug, Clone)]
pub struct AnthropicModelConfig {
    pub model_ref: Option<String>,
    pub api_key: Option<String>,
    pub api_url: Option<String>,
}

#[derive(Deserialize, Serialize, ToSchema, Debug, Clone)]
pub struct OllamaModelConfig {
    pub model_ref: Option<String>,
    pub api_key: Option<String>,
    pub api_url: Option<String>,
}

#[derive(Deserialize, ToSchema)]
pub struct ModelConfig {
    pub vendor: ModelVendor,
    pub name: Option<String>,
    pub config: serde_json::Value,
}

impl ModelConfig {
    pub fn get_openai_config(&self) -> OpenAIModelConfig {
        serde_json::from_value::<OpenAIModelConfig>(self.config.clone()).unwrap_or({
            OpenAIModelConfig {
                model_ref: None,
                api_key: None,
                api_url: None,
            }
        })
    }

    pub fn get_google_config(&self) -> GoogleModelConfig {
        serde_json::from_value::<GoogleModelConfig>(self.config.clone()).unwrap_or({
            GoogleModelConfig {
                model_ref: None,
                api_key: None,
                project_id: None,
            }
        })
    }

    pub fn get_anthropic_config(&self) -> AnthropicModelConfig {
        serde_json::from_value::<AnthropicModelConfig>(self.config.clone()).unwrap_or({
            AnthropicModelConfig {
                model_ref: None,
                api_key: None,
                api_url: None,
            }
        })
    }

    pub fn get_ollama_config(&self) -> OllamaModelConfig {
        serde_json::from_value::<OllamaModelConfig>(self.config.clone()).unwrap_or({
            OllamaModelConfig {
                model_ref: None,
                api_key: None,
                api_url: None,
            }
        })
    }
}

#[derive(Deserialize, ToSchema)]
pub struct ModelsFormData {
    pub models: Vec<ModelConfig>,
}

#[derive(Deserialize, ToSchema)]
pub struct ToolConfig {
    pub r#type: String,
    pub name: String,
    pub description: String,
    pub database: Option<String>,
}

#[derive(Deserialize, ToSchema)]
pub struct AgentConfig {
    pub name: String,
    pub model: String,
    pub system_instructions: String,
    pub description: Option<String>,
    pub public: Option<bool>,
    pub tools: Option<Vec<ToolConfig>>,
}

#[derive(Deserialize, ToSchema)]
pub struct GitHubData {
    pub namespace_id: Uuid,
    pub repo_id: i64,
    pub branch: String,
}

#[derive(Deserialize, ToSchema)]
pub struct CreateWorkspaceRequest {
    pub workspace: WorkspaceFormData,
    pub warehouses: Option<WarehousesFormData>,
    pub model: Option<ModelsFormData>,
    pub agent: Option<AgentConfig>,
    pub github: Option<GitHubData>,
}
