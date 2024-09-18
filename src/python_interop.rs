use log::debug;
use pyo3::prelude::*;
use pyo3::types::PyDict;
use std::error::Error;
use std::path::PathBuf;

// Include the generated file
include!(concat!(env!("OUT_DIR"), "/poetry_path.rs"));

pub fn get_poetry_env_path() -> &'static str {
    POETRY_ENV_PATH
}

pub fn execute_bigquery_query(
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
    site_packages_path.push("python3.11");  // TODO: Adjust to be dynamically determined
    site_packages_path.push("site-packages");

    Python::with_gil(|py| -> PyResult<()> {
        // Add the site-packages directory to sys.path
        let sys = py.import("sys")?;
        sys.getattr("path")?.call_method1(
            "append", 
            (site_packages_path.to_str().unwrap(),)
        )?;

        // Print sys.path
        debug!("sys.path: {:?}", sys.getattr("path")?.extract::<Vec<String>>()?);

        // Try to import onyx
        match py.import("onyx") {
            Ok(_) => println!("Successfully imported onyx"),
            Err(e) => println!("Failed to import onyx: {:?}", e),
        }

        // Try to import onyx
        // I have no idea why the import is messed up like this, but it is.
        // The bigquery module isn't working, so need to figure out what's wrong, but for now will try using a simpler connector.
        match py.import("onyx.catalog.src.onyx.catalog.adapters.connector") {
            Ok(_) => println!("Successfully imported onyx sub-library"),
            Err(e) => println!("Failed to import onyx: {:?}", e),
        }

        Ok(())
    }).map_err(|e| e.into())
}

