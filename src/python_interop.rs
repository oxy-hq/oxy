use pyo3::prelude::*;
use pyo3::types::PyDict;
use std::error::Error;

pub fn execute_bigquery_query(
    credentials_key: &str,
    database: &str,
    dataset: &str,
    query: &str,
) -> Result<Vec<Vec<String>>, Box<dyn Error>> {
    Python::with_gil(|py| {
        let sys = py.import("sys")?;
        let path = sys.getattr("path")?;
        path.call_method1("insert", (0, "../onyx/"))?;
        
        println!("Python version: {}", sys.getattr("version")?);
        println!("Python path: {:?}", sys.getattr("path")?);

        // Print virtual environment information
        let prefix = sys.getattr("prefix")?;
        println!("Python prefix (potential venv path): {}", prefix);

        let onyx = py.import("onyx.catalog.adapters.connector.bigquery")?;
        
        let connector_class = onyx.getattr("BigQueryConnector")?;
        let connection_config = PyDict::new(py);
        connection_config.set_item("credentials_key", credentials_key)?;
        connection_config.set_item("database", database)?;
        connection_config.set_item("dataset", dataset)?;

        let connector = connector_class.call1((
            "organization_id",
            "connection_id",
            connection_config,
        ))?;

        let connected_connector = connector.call_method0("connect")?;
        let result = connected_connector.call_method1("query", (query,))?;
     
        // Convert the Python result to a Vec<Vec<String>>
        let result: Vec<Vec<String>> = result.extract()?;

        Ok(result)
    })
}
