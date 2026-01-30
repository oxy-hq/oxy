use crate::adapters::secrets::SecretsManager;
use crate::config::validate::ValidationContext;
use crate::connector::{
    ConnectionStringFormatter, ConnectionStringParser, PostgresConnectionString,
};
use crate::service::secret_manager::ManagedSecret;
use garde::Validate;
use oxy_shared::errors::OxyError;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Validate, Clone, JsonSchema)]
#[garde(context(ValidationContext))]
#[serde(tag = "s3_secret_type", rename_all = "snake_case")]
pub enum S3StorageSecret {
    Config {
        #[garde(length(min = 1))]
        key_id: String,
        /// The secret key, referenced by a key variable name stored in SecretManager.
        /// In YAML, specify the secret key name: `secret: AWS_S3_SECRET`
        #[serde(default = "default_s3_secret_var")]
        #[garde(skip)]
        secret: ManagedSecret,
        #[serde(default = "default_s3_region")]
        #[garde(length(min = 1))]
        region: String,
        #[garde(length(min = 1))]
        endpoint_url: Option<String>,
        #[serde(default = "default_use_ssl")]
        #[garde(skip)]
        use_ssl: bool,
        #[serde(default = "default_url_style")]
        #[garde(length(min = 1))]
        url_style: String,
    },
    CredentialChain {
        #[serde(default = "default_s3_chain")]
        #[garde(length(min = 1))]
        chain: Option<String>,
        #[serde(default = "default_s3_region")]
        #[garde(length(min = 1))]
        region: String,
    },
}

impl S3StorageSecret {
    /// Generate the DuckDB secret statement.
    ///
    /// For `Config` variant, the secret value is retrieved from the `SecretsManager`.
    pub async fn to_duckdb_secret_stmt(
        &self,
        secrets_manager: &SecretsManager,
    ) -> Result<String, OxyError> {
        match self {
            S3StorageSecret::Config {
                key_id,
                secret,
                region,
                endpoint_url,
                url_style,
                use_ssl,
            } => {
                // Expose the secret value from the secrets manager
                let secret_value = secret.expose_str_with_adapter(secrets_manager).await?;

                Ok(format!(
                    "
CREATE OR REPLACE SECRET s3_secret (
    TYPE s3,
    PROVIDER config,
    URL_STYLE '{}',
    ENDPOINT '{}',
    USE_SSL {},
    KEY_ID '{}',
    SECRET '{}',
    REGION '{}'
)",
                    url_style,
                    endpoint_url
                        .clone()
                        .unwrap_or(format!("s3.{}.amazonaws.com", region)),
                    use_ssl,
                    key_id,
                    secret_value,
                    region
                ))
            }
            S3StorageSecret::CredentialChain { chain, region } => {
                let chain_stmt = if let Some(chain_str) = chain {
                    format!("CHAIN '{}'", chain_str)
                } else {
                    "".to_string()
                };
                Ok(format!(
                    "
CREATE OR REPLACE SECRET s3_secret (
    TYPE s3,
    PROVIDER credential_chain,
    {},
    REGION '{}'
)",
                    chain_stmt, region
                ))
            }
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Validate, Clone, JsonSchema)]
#[garde(context(ValidationContext))]
pub struct CatalogConfig {
    /// The catalog path, referenced by a key variable name stored in SecretManager.
    /// The secret value should be a postgres connection string URL like:
    /// `postgres://user:pass@localhost:5432/catalog_db`
    #[serde(default = "default_catalog_path")]
    #[garde(skip)]
    catalog_path: ManagedSecret,
    #[garde(length(min = 1))]
    schema_name: String,
}

impl CatalogConfig {
    /// Get the catalog path formatted for DuckDB.
    ///
    /// Retrieves the postgres connection string from the secrets manager,
    /// parses it, and formats it for DuckDB's postgres extension.
    async fn to_duckdb_attach_path(
        &self,
        secrets_manager: &SecretsManager,
    ) -> Result<String, OxyError> {
        // Retrieve the connection string from secrets manager
        let connection_string = self
            .catalog_path
            .expose_str_with_adapter(secrets_manager)
            .await?;

        // Parse the postgres URL and format it for DuckDB
        let parsed = PostgresConnectionString::parse(&connection_string).map_err(|e| {
            OxyError::ConfigurationError(format!("Invalid catalog_path connection string: {}", e))
        })?;

        Ok(parsed.to_duckdb_format())
    }

    fn to_duckdb_attach_params(&self) -> Vec<String> {
        vec![format!("METADATA_SCHEMA '{}'", self.schema_name)]
    }
}

#[derive(Serialize, Deserialize, Debug, Validate, Clone, JsonSchema)]
#[garde(context(ValidationContext))]
pub struct StorageConfig {
    #[garde(length(min = 1))]
    pub data_path: String,
    #[serde(flatten)]
    #[garde(dive)]
    pub storage_secret: S3StorageSecret,
}

impl StorageConfig {
    fn to_duckdb_attach_params(&self) -> Vec<String> {
        vec![format!("DATA_PATH '{}'", self.data_path)]
    }
}

#[derive(Serialize, Deserialize, Debug, Validate, Clone, JsonSchema)]
#[garde(context(ValidationContext))]
pub struct DuckLakeConfig {
    #[garde(dive)]
    #[serde(flatten)]
    pub catalog: CatalogConfig,
    #[garde(dive)]
    #[serde(flatten)]
    pub storage: StorageConfig,
}

impl DuckLakeConfig {
    /// Generate the DuckDB attach statements.
    ///
    /// The secret values are retrieved from the `SecretsManager`.
    pub async fn to_duckdb_attach_stmt(
        &self,
        secrets_manager: &SecretsManager,
    ) -> Result<Vec<String>, OxyError> {
        // Retrieve and format the catalog path (postgres connection string)
        let catalog_path = self.catalog.to_duckdb_attach_path(secrets_manager).await?;
        let catalog_param = self.catalog.to_duckdb_attach_params();
        let storage_param = self.storage.to_duckdb_attach_params();
        let params = catalog_param
            .iter()
            .chain(storage_param.iter())
            .cloned()
            .collect::<Vec<String>>();
        let storage_secret_stmt = self
            .storage
            .storage_secret
            .to_duckdb_secret_stmt(secrets_manager)
            .await?;
        Ok(vec![
            storage_secret_stmt,
            format!(
                "ATTACH 'ducklake:{}' AS ducklake ({})",
                catalog_path,
                params.join(", ")
            ),
            "USE ducklake".to_string(),
        ])
    }
}

#[derive(Serialize, Deserialize, Debug, Validate, Clone, JsonSchema)]
#[garde(context(ValidationContext))]
#[serde(untagged)]
pub enum DuckDBOptions {
    Local {
        #[serde(alias = "dataset", rename = "dataset")]
        #[garde(length(min = 1))]
        file_search_path: String,
    },
    DuckLake(#[garde(dive)] DuckLakeConfig),
}

fn default_use_ssl() -> bool {
    false
}

fn default_s3_region() -> String {
    "us-east-1".to_string()
}

fn default_s3_secret_var() -> ManagedSecret {
    ManagedSecret::new("DUCKLAKE_S3_SECRET".to_string())
}

fn default_s3_chain() -> Option<String> {
    Some("sso;config".to_string())
}

fn default_catalog_path() -> ManagedSecret {
    ManagedSecret::new("DUCKLAKE_CATALOG_PATH".to_string())
}

fn default_url_style() -> String {
    "path".to_string()
}
