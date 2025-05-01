use arrow::datatypes::SchemaRef;
use arrow::ipc::reader::FileReader;
use arrow::record_batch::RecordBatch;
use clickhouse::Client;
use std::io::Cursor;

use crate::config::model::ClickHouse as ConfigClickHouse;
use crate::errors::OxyError;

use super::constants::LOAD_RESULT;
use super::engine::Engine;
use super::utils::connector_internal_error;

#[derive(Debug)]
pub(super) struct ClickHouse {
    pub config: ConfigClickHouse,
}

impl ClickHouse {
    pub fn new(config: ConfigClickHouse) -> Self {
        ClickHouse { config }
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
            .with_password(self.config.get_password().unwrap_or_default())
            .with_database(self.config.database.clone());

        let mut cursor = client.query(query).fetch_bytes("arrow").unwrap();
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
            Err(e) => Err(OxyError::DBError(format!("Error fetching data: {}", e)))?,
        }
    }
}
