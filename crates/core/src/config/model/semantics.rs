use itertools::Itertools;
use schemars::{JsonSchema, schema::SchemaObject};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, Default)]
pub struct SemanticDimension {
    pub name: String,
    pub targets: Vec<String>,
    #[serde(flatten)]
    pub schema: SchemaObject,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, Default)]
pub struct Semantics {
    pub dimensions: Vec<SemanticDimension>,
}

impl Semantics {
    pub fn new(dimensions: Vec<SemanticDimension>) -> Self {
        Semantics { dimensions }
    }

    pub fn list_targets(&self) -> Vec<String> {
        self.dimensions
            .iter()
            .flat_map(|dim| dim.targets.clone())
            .unique()
            .collect()
    }
}
