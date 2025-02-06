use pyo3::prelude::*;

use crate::execute::core::value::ContextValue;

pub mod executor;

pub mod cache;

#[pyclass(module = "onyx_py")]
pub struct WorkflowResult {
    #[pyo3(get)]
    pub output: ContextValue,
}
