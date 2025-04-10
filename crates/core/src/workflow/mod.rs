use pyo3::prelude::*;

use crate::execute::core::value::ContextValue;

mod builders;
pub mod executor;

pub mod cache;

pub use builders::{WorkflowInput, WorkflowLauncher, build_workflow_executable};

#[pyclass(module = "oxy_py")]
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct WorkflowResult {
    #[pyo3(get)]
    pub output: ContextValue,
}
