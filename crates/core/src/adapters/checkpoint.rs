use std::{
    hash::Hash,
    path::{Path, PathBuf},
};

use fxhash::hash;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use tokio::{
    fs::{OpenOptions, create_dir_all},
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    sync::mpsc::{Receiver, Sender},
};

use crate::{
    config::{
        ConfigManager,
        constants::{
            CHECKPOINT_DATA_PATH, CHECKPOINT_EVENTS_FILE, CHECKPOINT_ROOT_PATH,
            CHECKPOINT_SUCCESS_MARKER,
        },
    },
    errors::OxyError,
    execute::{types::Event, writer::Writer},
};

pub struct CheckpointBuilder {
    storage: Option<CheckpointStorageImpl>,
}

impl CheckpointBuilder {
    pub fn new() -> Self {
        CheckpointBuilder { storage: None }
    }

    pub async fn from_config(config: &ConfigManager) -> Result<CheckpointManager, OxyError> {
        let storage_path = config.resolve_file(CHECKPOINT_ROOT_PATH).await?;
        let storage = CheckpointStorageImpl::FileStorage(FileStorage {
            dir: PathBuf::from(storage_path),
            data_path: CHECKPOINT_DATA_PATH.to_string(),
        });
        CheckpointBuilder {
            storage: Some(storage),
        }
        .build()
    }

    pub fn with_local_path<P: AsRef<Path>>(mut self, path: P) -> Self {
        self.storage = Some(CheckpointStorageImpl::FileStorage(FileStorage {
            dir: path.as_ref().to_path_buf(),
            data_path: CHECKPOINT_DATA_PATH.to_string(),
        }));
        self
    }

    pub fn build(self) -> Result<CheckpointManager, OxyError> {
        let storage = self.storage.ok_or(OxyError::RuntimeError(
            "Storage source is required".to_string(),
        ))?;

        Ok(CheckpointManager { storage })
    }
}

#[derive(Debug, Clone)]
pub struct CheckpointManager {
    storage: CheckpointStorageImpl,
}

impl CheckpointManager {
    pub fn checkpoint_id<I: Hash>(&self, input: &I) -> String {
        self.storage.checkpoint_id(input)
    }

    pub async fn last_run(&self, root_id: &str) -> Result<RunInfo, OxyError> {
        self.storage.last_run(root_id).await
    }

    pub async fn create_run(&self, root_id: &str) -> Result<RunInfo, OxyError> {
        self.storage.create_run(root_id).await
    }

    pub async fn read_events<W: Writer>(
        &self,
        run_info: &RunInfo,
        writer: W,
    ) -> Result<(), OxyError> {
        for event in self.storage.read_events(run_info).await? {
            writer.write(event).await?;
        }
        Ok(())
    }

    pub async fn write_events(
        &self,
        run_info: &RunInfo,
        receiver: Receiver<Vec<Event>>,
    ) -> Result<(), OxyError> {
        self.storage.write_events(run_info, receiver).await
    }

    pub async fn write_success_marker(&self, run_info: &RunInfo) -> Result<(), OxyError> {
        self.storage.write_success_marker(run_info).await
    }

    pub fn new_context(&self, run_info: RunInfo, tx: Sender<Vec<Event>>) -> CheckpointContext {
        CheckpointContext::new(run_info, tx, self.storage.clone())
    }
}

#[derive(Debug, Clone)]
pub struct CheckpointContext {
    run_info: RunInfo,
    tx: Sender<Vec<Event>>,
    storage: CheckpointStorageImpl,
}

impl CheckpointContext {
    fn new(run_info: RunInfo, tx: Sender<Vec<Event>>, storage: CheckpointStorageImpl) -> Self {
        CheckpointContext {
            run_info,
            tx,
            storage,
        }
    }

    pub fn checkpoint_id<I: Hash>(&self, input: &I) -> String {
        self.storage.checkpoint_id(input)
    }

    pub async fn create_checkpoint<T: Serialize + Send>(
        &self,
        checkpoint: CheckpointData<T>,
    ) -> Result<(), OxyError> {
        self.storage
            .create_checkpoint(&self.run_info, checkpoint)
            .await
    }

    pub async fn read_checkpoint<T: DeserializeOwned>(
        &self,
        checkpoint_id: &str,
    ) -> Result<CheckpointData<T>, OxyError> {
        self.storage
            .read_checkpoint(&self.run_info, checkpoint_id)
            .await
    }

    pub async fn write_events(&self, events: Vec<Event>) -> Result<(), OxyError> {
        self.tx.send(events).await.map_err(|err| {
            OxyError::IOError(format!(
                "Failed to send events to checkpoint writer:\n{}",
                err
            ))
        })?;
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct RunInfo {
    pub root_id: String,
    pub run_id: String,
    pub success: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckpointData<T> {
    pub checkpoint_id: String,
    pub output: T,
}

#[enum_dispatch::enum_dispatch]
pub trait CheckpointStorage {
    fn checkpoint_id<I: Hash>(&self, input: &I) -> String {
        let output = hash(input);
        format!("{:x}", output)
    }
    async fn last_run(&self, root_id: &str) -> Result<RunInfo, OxyError>;
    async fn create_run(&self, root_id: &str) -> Result<RunInfo, OxyError>;
    async fn read_events(&self, run_info: &RunInfo) -> Result<Vec<Event>, OxyError>;
    async fn write_events(
        &self,
        run_info: &RunInfo,
        receiver: Receiver<Vec<Event>>,
    ) -> Result<(), OxyError>;
    async fn write_success_marker(&self, run_info: &RunInfo) -> Result<(), OxyError>;
    async fn create_checkpoint<T: Serialize + Send>(
        &self,
        run_info: &RunInfo,
        checkpoint: CheckpointData<T>,
    ) -> Result<(), OxyError>;
    async fn read_checkpoint<T: DeserializeOwned>(
        &self,
        run_info: &RunInfo,
        checkpoint_id: &str,
    ) -> Result<CheckpointData<T>, OxyError>;
}

#[enum_dispatch::enum_dispatch(CheckpointStorage)]
#[derive(Debug, Clone)]
enum CheckpointStorageImpl {
    FileStorage(FileStorage),
}

#[derive(Debug, Clone)]
struct FileStorage {
    dir: PathBuf,
    data_path: String,
}

impl FileStorage {
    async fn get_root_path(&self, root_id: &str) -> Result<PathBuf, OxyError> {
        let root_path = self.dir.join(root_id);
        create_dir_all(&root_path).await.map_err(|err| {
            OxyError::IOError(format!("Failed to create checkpoint directory:\n{}", err))
        })?;
        Ok(root_path)
    }

    async fn get_base_path(&self, run_info: &RunInfo) -> Result<PathBuf, OxyError> {
        let base_path = self.dir.join(&run_info.root_id).join(&run_info.run_id);
        create_dir_all(&base_path).await.map_err(|err| {
            OxyError::IOError(format!("Failed to create checkpoint directory:\n{}", err))
        })?;
        Ok(base_path)
    }

    async fn get_data_path(&self, run_info: &RunInfo) -> Result<PathBuf, OxyError> {
        let data_path = self.get_base_path(run_info).await?.join(&self.data_path);
        create_dir_all(&data_path).await.map_err(|err| {
            OxyError::IOError(format!("Failed to create checkpoint directory:\n{}", err))
        })?;
        Ok(data_path)
    }
}

impl CheckpointStorage for FileStorage {
    async fn create_checkpoint<T: Serialize + Send>(
        &self,
        run_info: &RunInfo,
        checkpoint: CheckpointData<T>,
    ) -> Result<(), OxyError> {
        let data_path = self
            .get_data_path(run_info)
            .await?
            .join(&checkpoint.checkpoint_id);
        let file = tokio::fs::File::create(data_path)
            .await
            .map_err(|err| OxyError::IOError(format!("Failed to create checkpoint:\n{}", err)))?;
        let mut writer = tokio::io::BufWriter::new(file);
        let mut bytes: Vec<u8> = Vec::new();
        serde_json::to_writer(&mut bytes, &checkpoint)?;
        writer
            .write_all(&bytes)
            .await
            .map_err(|err| OxyError::IOError(format!("Failed to write checkpoint:\n{}", err)))?;
        writer
            .flush()
            .await
            .map_err(|err| OxyError::IOError(format!("Failed to flush checkpoint:\n{}", err)))?;
        Ok(())
    }

    async fn read_checkpoint<T: DeserializeOwned>(
        &self,
        run_info: &RunInfo,
        checkpoint_id: &str,
    ) -> Result<CheckpointData<T>, OxyError> {
        let path = self.get_data_path(run_info).await?.join(checkpoint_id);
        let bytes = tokio::fs::read(path)
            .await
            .map_err(|err| OxyError::IOError(format!("Failed to read checkpoint:\n{}", err)))?;
        let checkpoint: CheckpointData<T> = serde_json::from_slice(&bytes)?;
        Ok(checkpoint)
    }

    async fn last_run(&self, root_id: &str) -> Result<RunInfo, OxyError> {
        let root_dir = self.get_root_path(root_id).await?;
        let mut read_dir = tokio::fs::read_dir(&root_dir)
            .await
            .map_err(|err| OxyError::IOError(format!("Failed to read root directory:\n{}", err)))?;
        let mut run_ids = vec![];
        while let Some(entry) = read_dir.next_entry().await.map_err(|err| {
            OxyError::IOError(format!("Failed to traverse {:?} dir:\n{}", root_dir, err))
        })? {
            let entry_path = entry.path();
            if let (true, Some(run_id)) = (
                entry_path.is_dir(),
                entry_path.components().next_back().and_then(|c| {
                    c.as_os_str()
                        .to_string_lossy()
                        .to_string()
                        .parse::<u64>()
                        .ok()
                }),
            ) {
                run_ids.push((run_id, entry_path));
            }
        }
        run_ids.sort_by_key(|id| id.0);
        let (run_id, run_path) = run_ids
            .pop()
            .ok_or_else(|| OxyError::IOError("No runs found in root directory".to_string()))?;
        let success = tokio::fs::metadata(run_path.join(CHECKPOINT_SUCCESS_MARKER))
            .await
            .is_ok();

        Ok(RunInfo {
            root_id: root_id.to_string(),
            run_id: run_id.to_string(),
            success,
        })
    }

    async fn create_run(&self, root_id: &str) -> Result<RunInfo, OxyError> {
        let run_id = self.last_run(root_id).await.ok().map_or(0, |run_info| {
            // Safe to unwrap because last_run always returns a u64 run_id
            run_info.run_id.parse::<u64>().map(|i| i + 1).unwrap()
        });
        Ok(RunInfo {
            root_id: root_id.to_string(),
            run_id: run_id.to_string(),
            success: false,
        })
    }

    async fn read_events(&self, run_info: &RunInfo) -> Result<Vec<Event>, OxyError> {
        let events_file = self
            .get_base_path(run_info)
            .await?
            .join(CHECKPOINT_EVENTS_FILE);
        let file = OpenOptions::new()
            .read(true)
            .open(&events_file)
            .await
            .map_err(|err| OxyError::IOError(format!("Failed to open events file:\n{:?}", err)))?;
        let buf_reader = BufReader::new(file);
        let mut lines = buf_reader.lines();
        let mut events = vec![];
        while let Some(line) = lines
            .next_line()
            .await
            .map_err(|err| OxyError::IOError(format!("Failed to read events file:\n{:?}", err)))?
        {
            let event: Event = serde_json::from_str(&line)?;
            events.push(event);
        }
        Ok(events)
    }

    async fn write_events(
        &self,
        run_info: &RunInfo,
        receiver: Receiver<Vec<Event>>,
    ) -> Result<(), OxyError> {
        let events_file = self
            .get_base_path(run_info)
            .await?
            .join(CHECKPOINT_EVENTS_FILE);
        let mut receiver = receiver;
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&events_file)
            .await
            .map_err(|err| OxyError::IOError(format!("Failed to open events file:\n{}", err)))?;
        while let Some(events) = receiver.recv().await {
            let mut buffer = vec![];
            for event in events {
                serde_json::to_writer(&mut buffer, &event)?;
                buffer.extend_from_slice(b"\r\n");
            }
            file.write_all(&buffer).await.map_err(|err| {
                OxyError::IOError(format!("Failed to write events file:\n{}", err))
            })?;
        }
        Ok(())
    }

    async fn write_success_marker(&self, run_info: &RunInfo) -> Result<(), OxyError> {
        let success_marker_file = self
            .get_base_path(run_info)
            .await?
            .join(CHECKPOINT_SUCCESS_MARKER);
        tokio::fs::File::create(success_marker_file)
            .await
            .map_err(|err| {
                OxyError::IOError(format!("Failed to create success marker:\n{}", err))
            })?;
        Ok(())
    }
}
