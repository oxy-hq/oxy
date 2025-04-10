use serde::{Deserialize, Serialize};

#[derive(Clone, Serialize, Deserialize, Hash)]
pub struct SQL(pub String);

impl SQL {
    pub fn new(sql: String) -> Self {
        SQL(sql)
    }
}

impl std::fmt::Debug for SQL {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::fmt::Display for SQL {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "```{}```", self.0)
    }
}
