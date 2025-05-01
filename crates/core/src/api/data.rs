use crate::adapters::connector::{Connector, load_result};
use crate::config::ConfigBuilder;
use crate::execute::types::utils::record_batches_to_2d_array;
use crate::utils::find_project_path;
use axum::extract::{self, Path};
use axum::http::StatusCode;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Serialize, Deserialize, Clone, ToSchema)]
pub struct SQLParams {
    pub sql: String,
    pub database: String,
}

pub async fn execute_sql(
    Path(pathb64): Path<String>,
    extract::Json(payload): extract::Json<SQLParams>,
) -> Result<extract::Json<Vec<Vec<String>>>, StatusCode> {
    let project_path = find_project_path().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let config_builder = ConfigBuilder::new()
        .with_project_path(&project_path)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let config = config_builder
        .build()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let connector = Connector::from_database(&payload.database, &config, None).await?;
    let file_path = connector.run_query(&payload.sql).await?;

    let (batches, schema) =
        load_result(&file_path).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let data = record_batches_to_2d_array(&batches, &schema)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(extract::Json(data))
}

pub async fn list_databases() -> Result<extract::Json<Vec<String>>, StatusCode> {
    let project_path = find_project_path().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let config_builder = ConfigBuilder::new()
        .with_project_path(&project_path)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let config = config_builder
        .build()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let databases = config
        .list_databases()
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .iter()
        .map(|db| db.name.clone())
        .collect::<Vec<String>>();

    Ok(extract::Json(databases))
}
