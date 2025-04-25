use crate::tools::types::ExecuteOmniParams;

pub trait SqlGenerationEngine {
    fn generate_sql(&self, params: &ExecuteOmniParams) -> anyhow::Result<String>;
}
