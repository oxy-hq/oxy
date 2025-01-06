use pyo3::{
    prelude::*,
    types::{PyDict, PyList, PyNone, PyString},
};
use pyo3_arrow::PyRecordBatch;

use crate::execute::core::value::ContextValue;

pub mod executor;

#[pyclass(module = "onyx_py")]
#[derive(Debug, Clone)]
pub struct WorkflowResultStep {
    #[pyo3(get)]
    pub name: String,
    #[pyo3(get)]
    pub output: String,
}

#[pyclass(module = "onyx_py")]
pub struct WorkflowResult {
    #[pyo3(get)]
    pub output: ContextValue,
    #[pyo3(get)]
    pub steps: Vec<WorkflowResultStep>,
}

pub fn convert_output_to_python<'py>(py: Python<'py>, output: &ContextValue) -> Bound<'py, PyAny> {
    match output {
        ContextValue::Text(s) => {
            return PyString::new(py, s).into_any();
        }
        ContextValue::Map(m) => {
            let dict = PyDict::new(py);
            for (k, v) in &m.0 {
                dict.set_item(k, convert_output_to_python(py, v)).unwrap();
            }
            dict.into_any()
        }
        ContextValue::Array(a) => {
            let elements = a.0.iter().map(|v| convert_output_to_python(py, v));
            let list = PyList::new(py, elements).unwrap();
            list.into_any()
        }
        ContextValue::Table(table) => {
            let mut record_batchs = vec![];
            let iterator = table.0.clone().into_iter();
            for batch in iterator {
                let rb = PyRecordBatch::new(batch);
                record_batchs.push(rb.to_pyarrow(py).unwrap());
            }
            return PyList::new(py, record_batchs).unwrap().into_any();
        }
        _ => {
            return <pyo3::Bound<'_, PyNone> as Clone>::clone(&PyNone::get(py)).into_any();
        }
    }
}

impl<'py> IntoPyObject<'py> for ContextValue {
    type Target = PyAny;

    type Output = Bound<'py, Self::Target>;

    type Error = PyErr;

    fn into_pyobject(self, py: Python<'py>) -> Result<Self::Output, Self::Error> {
        let output = convert_output_to_python(py, &self);
        Ok(output)
    }
}

// @todo: The temporary conversion functions for the Workflow struct
// will be removed once we unify the output.
impl Into<ContextValue> for WorkflowResult {
    fn into(self) -> ContextValue {
        ContextValue::Map(
            [
                ("output".to_string(), self.output),
                (
                    "steps".to_string(),
                    ContextValue::Array(
                        self.steps
                            .into_iter()
                            .map(|step| {
                                ContextValue::Map(
                                    [
                                        ("name".to_string(), ContextValue::Text(step.name)),
                                        ("output".to_string(), ContextValue::Text(step.output)),
                                    ]
                                    .iter()
                                    .collect(),
                                )
                            })
                            .collect::<Vec<ContextValue>>()
                            .iter()
                            .collect(),
                    ),
                ),
            ]
            .iter()
            .collect(),
        )
    }
}

impl From<ContextValue> for WorkflowResult {
    fn from(result: ContextValue) -> Self {
        match result {
            ContextValue::Map(map) => {
                let output = map.get_value("output").unwrap();
                let steps = match map.get_value("steps").unwrap() {
                    ContextValue::Array(steps) => steps
                        .0
                        .iter()
                        .map(|step| {
                            let step = match step {
                                ContextValue::Map(step) => step,
                                _ => panic!("Expected a map"),
                            };
                            let name = match step.get_value("name").unwrap() {
                                ContextValue::Text(name) => name.to_string(),
                                _ => panic!("Expected a text"),
                            };
                            let output = match step.get_value("output").unwrap() {
                                ContextValue::Text(output) => output.to_string(),
                                _ => panic!("Expected a text"),
                            };
                            WorkflowResultStep { name, output }
                        })
                        .collect(),
                    _ => panic!("Expected an array"),
                };
                Self {
                    output: output.clone(),
                    steps,
                }
            }
            _ => panic!("Expected a map"),
        }
    }
}
