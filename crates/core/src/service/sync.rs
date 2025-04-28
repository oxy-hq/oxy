use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::Instant,
};

use futures::StreamExt;
use itertools::Itertools;
use tokio::fs::create_dir_all;
use tqdm::pbar;

use crate::{
    adapters::connector::Connector,
    config::{ConfigManager, constants::DATABASE_SEMANTIC_PATH, model::Database},
    errors::OxyError,
    theme::StyledText,
};

#[derive(Debug, Clone)]
pub struct SyncMetrics {
    pub database_ref: String,
    pub sync_time_secs: f64,
    pub output_files: Vec<String>,
}

impl std::fmt::Display for SyncMetrics {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(
            f,
            "{}\nSync Time(seconds): {}\nOutput files:\n{}",
            format!("Database: {}", self.database_ref).success(),
            self.sync_time_secs,
            self.output_files
                .iter()
                .map(|f| format!("- {}", f))
                .join("\n"),
        )
    }
}

async fn write_db_info<P: AsRef<Path>>(path: P, ddl: &str) -> Result<(), OxyError> {
    create_dir_all(path.as_ref().parent().ok_or(OxyError::IOError(format!(
        "Failed to resolve database semantic path"
    )))?)
    .await
    .map_err(|err| {
        OxyError::IOError(format!(
            "Failed to create database dir:\n{}",
            err.to_string()
        ))
    })?;
    tokio::fs::write(path, ddl)
        .await
        .map_err(|e| OxyError::IOError(e.to_string()))
}

async fn sync_database(config: ConfigManager, database: Database) -> Result<SyncMetrics, OxyError> {
    // Simulate some database sync operation
    let start_time = Instant::now(); // Placeholder for actual sync time
    let connector = Connector::from_database(&database.name, &config, None).await?;

    let paths = async || -> Result<Vec<String>, OxyError> {
        let datasets = database.datasets();
        let mut paths = vec![];
        let db_infos = connector
            .database_info(datasets.into_iter().collect())
            .await?;
        for db_info in db_infos {
            let database_semantic_path = PathBuf::from(DATABASE_SEMANTIC_PATH).join(&database.name);
            let base_path = config.resolve_file(database_semantic_path).await?;
            let ddl_file_path =
                PathBuf::from(base_path).join(format!("{}.sql", &db_info.dataset()));
            write_db_info(ddl_file_path.clone(), &db_info.get_ddl()).await?;
            paths.push(ddl_file_path.to_string_lossy().to_string());
        }
        Result::<_, OxyError>::Ok(paths)
    }()
    .await
    .map_err(|e| {
        OxyError::RuntimeError(format!("Database: {}\nSync Error:\n{}", database.name, e))
    })?;

    Ok(SyncMetrics {
        database_ref: database.name.clone(),
        sync_time_secs: start_time.elapsed().as_secs_f64(),
        output_files: paths,
    })
}

pub async fn sync_databases<T: IntoIterator<Item = Database>>(
    config: ConfigManager,
    databases: T,
) -> Result<Vec<Result<SyncMetrics, OxyError>>, OxyError> {
    let iter = databases.into_iter();
    let progress = Arc::new(Mutex::new(pbar(Some(iter.size_hint().0))));
    let sync_metrics = async_stream::stream! {
        for database in iter {
            let config = config.clone();
            let progress = progress.clone();

            yield async move {
                let result = sync_database(config, database).await;
                let mut progress = progress.lock().unwrap();
                progress.update(1)?;
                result
            };
        }
    }
    .buffered(10)
    .collect::<Vec<_>>()
    .await;

    Ok(sync_metrics)
}
