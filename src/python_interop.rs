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
    println!("Poetry environment path: {}", poetry_env_path);

    // Construct site-packages path
    let mut site_packages_path = PathBuf::from(poetry_env_path);
    site_packages_path.push("lib");
    site_packages_path.push("python3.12");  // Adjust this to your Python version
    site_packages_path.push("site-packages");

    Python::with_gil(|py| -> PyResult<()> {
        // Add the site-packages directory to sys.path
        let sys = py.import("sys")?;
        sys.getattr("path")?.call_method1(
            "append", 
            (site_packages_path.to_str().unwrap(),)
        )?;

        // Print sys.path
        println!("sys.path: {:?}", sys.getattr("path")?.extract::<Vec<String>>()?);

        match py.import("google") {
            Ok(_) => println!("Successfully imported google"),
            Err(e) => println!("Failed to import google: {:?}", e),
        }

        // Try to import onyx
        match py.import("onyx") {
            Ok(_) => println!("Successfully imported onyx"),
            Err(e) => println!("Failed to import onyx: {:?}", e),
        }

        Ok(())
    }).map_err(|e| e.into())
}

