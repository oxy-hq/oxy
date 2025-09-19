use arrow::datatypes::SchemaRef;
use arrow::ipc::reader::FileReader;
use arrow::record_batch::RecordBatch;
use clickhouse::Client;
use sqlparser::{dialect::ClickHouseDialect, parser::Parser};
use std::io::Cursor;

use crate::adapters::secrets::SecretsManager;
use crate::config::model::ClickHouse as ConfigClickHouse;
use crate::errors::OxyError;

use super::constants::LOAD_RESULT;
use super::engine::Engine;
use super::utils::connector_internal_error;

#[derive(Debug)]
pub(super) struct ClickHouse {
    pub config: ConfigClickHouse,
    pub secret_manager: SecretsManager,
}

impl ClickHouse {
    pub fn new(config: ConfigClickHouse, secret_manager: SecretsManager) -> Self {
        ClickHouse {
            config,
            secret_manager,
        }
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

        let cleaned_query = ClickHouse::strip_comments(query)?;
        let mut cursor = client
            .query(&cleaned_query)
            .fetch_bytes("arrow")
            .map_err(|err| OxyError::DBError(format!("ClickHouse query error: {err}")))?;
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
