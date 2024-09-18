use pyo3::prelude::*;
use pyo3::types::PyDict;
use std::error::Error;
use std::path::PathBuf;
use std::process::Command;

pub fn execute_bigquery_query(
    credentials_key: &str,
    database: &str,
    dataset: &str,
    query: &str,
) -> Result<Vec<Vec<String>>, Box<dyn Error>> {
    
    // Get Poetry environment path
    let output = Command::new("poetry")
        .args(&["env", "info", "--path"])
        .output()?;
    let poetry_env_path = String::from_utf8(output.stdout)?.trim().to_string();

    // Construct site-packages path
    let mut site_packages_path = PathBuf::from(poetry_env_path);
    site_packages_path.push("lib");
    site_packages_path.push("python3.9");  // Adjust this to your Python version
    site_packages_path.push("site-packages");

    Python::with_gil(|py| -> PyResult<Vec<Vec<String>>> {
        // Add the site-packages directory to sys.path
        let sys = py.import("sys")?;
        sys.getattr("path")?.call_method1(
            "append", 
            (site_packages_path.to_str().unwrap(),)
        )?;

        let onyx = py.import("onyx")?;
        // let onyx = py.import("onyx.catalog.adapters.connector.bigquery")?;
        // 
        // let connector_class = onyx.getattr("BigQueryConnector")?;
        // let connection_config = PyDict::new(py);
        // connection_config.set_item("credentials_key", credentials_key)?;
        // connection_config.set_item("database", database)?;
        // connection_config.set_item("dataset", dataset)?;

        // let connector = connector_class.call1((
        //     "organization_id",
        //     "connection_id",
        //     connection_config,
        // ))?;

        // let connected_connector = connector.call_method0("connect")?;
        // let result = connected_connector.call_method1("query", (query,))?;
     
        // // Convert the Python result to a Vec<Vec<String>>
        // let result: Vec<Vec<String>> = result.extract()?;

        Ok(result)
    }).map_err(|e| e.into())
}

