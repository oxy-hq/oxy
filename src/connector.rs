use arrow::array::as_string_array;
use connectorx::prelude::{get_arrow, CXQuery, SourceConn};

use crate::yaml_parsers::config_parser::Warehouse;

pub struct Connector {
    config: Warehouse,
}

#[derive(serde::Serialize)]
pub struct WarehouseInfo {
    name: String,
    dialect: String,
    tables: Vec<String>,
}

impl Connector {
    pub fn new(config: Warehouse) -> Self {
        Connector { config }
    }

    pub async fn load_warehouse_info(&self) -> WarehouseInfo {
        let tables = self.get_schemas().await;
        let name = self.config.name.clone();
        let dialect = self.config.r#type.clone();
        WarehouseInfo {
            name,
            dialect,
            tables,
        }
    }

    pub async fn list_datasets(&self) -> Vec<String> {
        let result = self.run_query(
            "SELECT schema_name FROM INFORMATION_SCHEMA.SCHEMATA",
        )
        .await
        .unwrap();
        let result_iter = result
            .iter()
            .flat_map(|batch| as_string_array(batch.column(0)).iter());
        let datasets = result_iter
            .map(|name| name.map(|s| s.to_string()))
            .collect::<Option<Vec<String>>>()
            .unwrap_or_default();
        datasets
    }

    pub async fn get_schemas(&self) -> Vec<String> {
        let result = self.run_query(
            &format!(
                "SELECT ddl FROM `{}`.INFORMATION_SCHEMA.TABLES",
                self.config.dataset
            ),
        )
        .await
        .unwrap();
        let result_iter = result
            .iter()
            .flat_map(|batch| as_string_array(batch.column(0)).iter());
        let ddls = result_iter
            .map(|name| name.map(|s| s.to_string()))
            .collect::<Option<Vec<String>>>()
            .unwrap_or_default();
        ddls
    }

    pub async fn run_query(
        &self,
        query: &str,
    ) -> Result<Vec<arrow::record_batch::RecordBatch>, Box<dyn std::error::Error>> {
        let conn_string = format!("bigquery://{}", self.config.key_path);
        let query = query.to_string(); // convert to owned string for closure
        let result = tokio::task::spawn_blocking(move || {
            let source_conn = SourceConn::try_from(conn_string.as_str())?;
            let queries = &[CXQuery::from(query.as_str())];
            let destination = get_arrow(&source_conn, None, queries).expect("Run failed at get_arrow.");
            destination.arrow()
        })
        .await??;

        Ok(result)
    }

}

