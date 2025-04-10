use serde::{Deserialize, Serialize};

#[derive(Clone, Serialize, Deserialize, Hash)]
pub struct Prompt(pub String);

impl Prompt {
    pub fn new(prompt: String) -> Self {
        Self(prompt)
    }
}

impl std::fmt::Debug for Prompt {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::fmt::Display for Prompt {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}
