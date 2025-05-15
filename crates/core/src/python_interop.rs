// TODO: This file is unused but left as an example of Python imports to pyo3.
// Leveraged with the `oxy` Python directory and the build.rs file to register the venv.

use pyo3::prelude::*;
use pyo3::types::PyDict;
use std::error::Error;
use std::path::PathBuf;
use tracing::debug;

// Include the generated file
include!(concat!(env!("OUT_DIR"), "/poetry_path.rs"));

pub fn get_poetry_env_path() -> &'static str {
    POETRY_ENV_PATH
}

pub fn execute_python_code(
    credentials_key: &str,
    database: &str,
    dataset: &str,
    query: &str,
) -> Result<(), Box<dyn Error>> {
    let poetry_env_path = get_poetry_env_path();
    debug!("Poetry environment path: {}", poetry_env_path);

    // Construct site-packages path
    let mut site_packages_path = PathBuf::from(poetry_env_path);
    site_packages_path.push("lib");
    site_packages_path.push("python3.11"); // TODO: Adjust to be dynamically determined
    site_packages_path.push("site-packages");

    Python::with_gil(|py| -> PyResult<()> {
        // Add the site-packages directory to sys.path
        let sys = py.import("sys")?;
        sys.getattr("path")?
            .call_method1("append", (site_packages_path.to_str().unwrap(),))?;

        // Print sys.path
        debug!(
            "sys.path: {:?}",
            sys.getattr("path")?.extract::<Vec<String>>()?
        );

        // Try to import oxy
        match py.import("oxy") {
            Ok(_) => println!("Successfully imported oxy"),
            Err(e) => println!("Failed to import oxy: {:?}", e),
        }

        Ok(())
    })
    .map_err(|e| e.into())
}
