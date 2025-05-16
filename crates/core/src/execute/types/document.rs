use serde::{Deserialize, Serialize};

#[derive(Clone, Serialize, Deserialize, Hash)]
pub struct Document {
    pub id: String,
    pub kind: String,
    pub content: String,
}

impl std::fmt::Debug for Document {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "{}", self.content)
    }
}

impl std::fmt::Display for Document {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.content)
    }
}
