use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

fn default_test_concurrency() -> usize {
    5
}

fn default_test_runs() -> usize {
    3
}

#[derive(Serialize, Deserialize, Debug, Clone, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TestFileConfig {
    pub name: Option<String>,
    pub target: Option<String>,
    #[serde(default)]
    pub settings: TestSettings,
    pub cases: Vec<TestCase>,
}

#[derive(Serialize, Deserialize, Debug, Clone, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TestSettings {
    #[serde(default = "default_test_concurrency")]
    pub concurrency: usize,
    #[serde(default = "default_test_runs")]
    pub runs: usize,
    pub judge_model: Option<String>,
}

impl Default for TestSettings {
    fn default() -> Self {
        Self {
            concurrency: default_test_concurrency(),
            runs: default_test_runs(),
            judge_model: None,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TestCase {
    #[serde(default)]
    pub name: Option<String>,
    pub prompt: String,
    pub expected: String,
    #[serde(default)]
    pub tags: Vec<String>,
    /// Optional: assert that this tool was invoked (deterministic check).
    /// TODO: implement tool-use verification in the eval pipeline.
    pub tool: Option<String>,
}
