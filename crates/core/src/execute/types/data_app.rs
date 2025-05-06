use std::{hash::Hash, hash::Hasher, path::PathBuf};

use serde::{Deserialize, Serialize};

use super::{ReferenceKind, reference::DataAppReference};

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct DataApp {
    pub file_path: PathBuf,
}

impl Hash for DataApp {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.file_path.hash(state);
    }
}

impl DataApp {
    pub fn into_references(&self) -> ReferenceKind {
        let file_path: PathBuf = self.file_path.clone();
        ReferenceKind::DataApp(DataAppReference { file_path })
    }
}
