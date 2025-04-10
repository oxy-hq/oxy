use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum ProgressType {
    Started(Option<usize>),
    Updated(usize),
    Finished,
}
