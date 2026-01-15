use std::path::{Path, PathBuf};

use serde::{Serialize, de::DeserializeOwned};
use tokio::{fs::create_dir_all, io::AsyncWriteExt};

use crate::config::constants::{CHECKPOINT_DATA_PATH, CHECKPOINT_SUCCESS_MARKER};
use oxy_shared::errors::OxyError;

use super::{CheckpointData, CheckpointStorage, RunInfo};

#[derive(Debug, Clone)]
pub(super) struct FileStorage {
    dir: PathBuf,
    data_path: String,
}

impl FileStorage {
    #[allow(dead_code)]
    pub fn new<P: AsRef<Path>>(dir: P) -> Self {
        FileStorage {
            dir: dir.as_ref().to_path_buf(),
            data_path: CHECKPOINT_DATA_PATH.to_string(),
        }
    }

    #[allow(dead_code)]
    async fn get_root_path(&self, root_id: &str) -> Result<PathBuf, OxyError> {
        let root_path = self.dir.join(slugify::slugify(root_id, "", "_", None));
        create_dir_all(&root_path).await.map_err(|err| {
            OxyError::IOError(format!(
                "Failed to create root checkpoint directory({root_path:?}) :\n{err}"
            ))
        })?;
        Ok(root_path)
    }

    async fn get_base_path(&self, run_info: &RunInfo) -> Result<PathBuf, OxyError> {
        let base_path = self
            .dir
            .join(slugify::slugify(&run_info.source_id, "", "_", None))
            .join(run_info.get_run_index().to_string());
        create_dir_all(&base_path).await.map_err(|err| {
            OxyError::IOError(format!(
                "Failed to create base checkpoint directory({base_path:?}) :\n{err}"
            ))
        })?;
        Ok(base_path)
    }

    async fn get_checkpoint_path(
        &self,
        run_info: &RunInfo,
        checkpoint_id: &str,
    ) -> Result<PathBuf, OxyError> {
        let data_path = self.get_base_path(run_info).await?.join(&self.data_path);
        create_dir_all(&data_path).await.map_err(|err| {
            OxyError::IOError(format!(
                "Failed to create checkpoint directory({data_path:?}) :\n{err}"
            ))
        })?;
        Ok(data_path.join(checkpoint_id))
    }
}

impl CheckpointStorage for FileStorage {
    async fn create_checkpoint<T: Serialize + Send>(
        &self,
        run_info: &RunInfo,
        checkpoint: CheckpointData<T>,
    ) -> Result<(), OxyError> {
        let checkpoint_path = self
            .get_checkpoint_path(run_info, &checkpoint.replay_id)
            .await?;
        let file = tokio::fs::File::create(checkpoint_path)
            .await
            .map_err(|err| OxyError::IOError(format!("Failed to create checkpoint:\n{err}")))?;
        let mut writer = tokio::io::BufWriter::new(file);
        let mut bytes: Vec<u8> = Vec::new();
        serde_json::to_writer(&mut bytes, &checkpoint)?;
        writer
            .write_all(&bytes)
            .await
            .map_err(|err| OxyError::IOError(format!("Failed to write checkpoint:\n{err}")))?;
        writer
            .flush()
            .await
            .map_err(|err| OxyError::IOError(format!("Failed to flush checkpoint:\n{err}")))?;
        Ok(())
    }

    async fn read_checkpoint<T: DeserializeOwned>(
        &self,
        run_info: &RunInfo,
        replay_id: &str,
    ) -> Result<CheckpointData<T>, OxyError> {
        let path = self.get_checkpoint_path(run_info, replay_id).await?;
        let bytes = tokio::fs::read(path)
            .await
            .map_err(|err| OxyError::IOError(format!("Failed to read checkpoint:\n{err}")))?;
        let checkpoint: CheckpointData<T> = serde_json::from_slice(&bytes)?;
        Ok(checkpoint)
    }

    async fn write_success_marker(&self, run_info: &RunInfo) -> Result<(), OxyError> {
        let success_marker_file = self
            .get_base_path(run_info)
            .await?
            .join(CHECKPOINT_SUCCESS_MARKER);
        tokio::fs::File::create(success_marker_file)
            .await
            .map_err(|err| OxyError::IOError(format!("Failed to create success marker:\n{err}")))?;
        Ok(())
    }
}
