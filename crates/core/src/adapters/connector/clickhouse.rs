use arrow::datatypes::SchemaRef;
use arrow::ipc::reader::FileReader;
use arrow::record_batch::RecordBatch;
use clickhouse::Client;
use sqlparser::{dialect::ClickHouseDialect, parser::Parser};
use std::io::Cursor;

use crate::adapters::secrets::SecretsManager;
use crate::adapters::session_filters::{FilterProcessor, SessionFilters};
use crate::config::model::ClickHouse as ConfigClickHouse;
use crate::errors::OxyError;

use super::constants::LOAD_RESULT;
use super::engine::Engine;
use super::utils::connector_internal_error;

#[derive(Debug)]
pub(super) struct ClickHouse {
    pub config: ConfigClickHouse,
    pub secret_manager: SecretsManager,
    pub filters: Option<SessionFilters>,
}

impl ClickHouse {
    pub fn new(config: ConfigClickHouse, secret_manager: SecretsManager) -> Self {
        ClickHouse {
            config,
            secret_manager,
            filters: None,
        }
    }

    pub fn with_filters(mut self, filters: SessionFilters) -> Self {
        self.filters = Some(filters);
        self
    }

    /// Apply filters and role to a query client
    fn apply_filters(&self, client: Client) -> Client {
        let mut client = client;

        // Apply role if configured
        if let Some(role) = &self.config.role {
            tracing::debug!(
                role = %role,
                database = %self.config.database,
                "Applying role to ClickHouse query"
            );
            client = client.with_option("role", role);
        }

        // Apply filters as session variables
        if let Some(filters) = &self.filters {
            tracing::info!(
                database = %self.config.database,
                filter_count = filters.len(),
                filters = ?filters.keys().collect::<Vec<_>>(),
                settings_prefix = ?self.config.settings_prefix,
                "Applying filters as session variables to ClickHouse query"
            );

            for (key, value) in filters.iter() {
                // Skip null values - optional filters that weren't provided
                if value.is_null() {
                    tracing::debug!(
                        filter_name = %key,
                        "Skipping null filter value - optional filter not provided"
                    );
                    continue;
                }

                let session_key = if let Some(prefix) = &self.config.settings_prefix {
                    format!("{}{}", prefix, key)
                } else {
                    key.clone()
                };

                let session_value = FilterProcessor::to_session_value(value);

                tracing::debug!(
                    filter_name = %key,
                    session_key = %session_key,
                    session_value = %session_value,
                    "Setting ClickHouse session variable for filter"
                );

                client = client.with_option(&session_key, &session_value);
            }
        } else {
            tracing::debug!(
                database = %self.config.database,
                "No filters to apply to ClickHouse query"
            );
        }

        client
    }

    pub fn strip_comments(query: &str) -> Result<String, OxyError> {
        let ast = Parser::parse_sql(&ClickHouseDialect {}, query)
            .map_err(|err| OxyError::DBError(format!("Failed to parse ClickHouse query: {err}")))?;
        Ok(ast
            .iter()
            .map(|stmt| stmt.to_string())
            .collect::<Vec<_>>()
            .join("\n"))
    }
}

impl Engine for ClickHouse {
    async fn run_query_with_limit(
        &self,
        query: &str,
        _dry_run_limit: Option<u64>,
    ) -> Result<(Vec<RecordBatch>, SchemaRef), OxyError> {
        let client = Client::default()
            .with_url(self.config.host.clone())
            .with_user(self.config.user.clone())
            .with_password(self.config.get_password(&self.secret_manager).await?)
            .with_database(self.config.database.clone());

        // Apply filters (role + session variables) before executing query
        let client = self.apply_filters(client);

        // Log query execution with filter context for audit trail
        tracing::info!(
            database = %self.config.database,
            has_filters = self.filters.is_some(),
            filter_count = self.filters.as_ref().map(|f| f.len()).unwrap_or(0),
            role = ?self.config.role,
            "Executing ClickHouse query with filters applied"
        );

        let cleaned_query = ClickHouse::strip_comments(query)?;
        let mut cursor = client
            .query(&cleaned_query)
            .fetch_bytes("arrow")
            .map_err(|err| {
                // Log query execution failure with filter context
                tracing::error!(
                    database = %self.config.database,
                    has_filters = self.filters.is_some(),
                    error = %err,
                    "ClickHouse query execution failed"
                );
                OxyError::DBError(format!("ClickHouse query error: {err}"))
            })?;
        let chunks = cursor.collect().await;
        match chunks {
            Ok(chunks) => {
                let cursor = Cursor::new(chunks);
                let reader = FileReader::try_new(cursor, None).unwrap();
                let batches: Vec<RecordBatch> = reader
                    .map(|result| result.map_err(|e| connector_internal_error(LOAD_RESULT, &e)))
                    .collect::<Result<_, _>>()?;

                let schema = batches[0].schema();
                Ok((batches, schema))
            }
            Err(e) => Err(OxyError::DBError(format!("Error fetching data: {e}")))?,
        }
    }
}
