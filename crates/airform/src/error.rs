use thiserror::Error;

#[derive(Error, Debug)]
pub enum AirformIntegrationError {
    #[error("Airform error: {0}")]
    Airform(#[from] airform_core::AirformError),

    #[error("No dbt project found at {0}")]
    NoDbtProject(String),

    #[error(
        "No 'oxy.yml' found in {0}. \
        An 'oxy.yml' mapping file is required to run models — it maps dbt target names \
        to Oxy database names so the runner knows where to read sources and write results.\n\n\
        Example oxy.yml:\n\
        \x20 mappings:\n\
        \x20   dev: my_local_db   # dbt target 'dev' maps to Oxy database 'my_local_db'"
    )]
    MissingOxyConfig(String),

    #[error(
        "The following dbt targets are not mapped in oxy.yml: {unmapped}.\n\n\
        Add entries under 'mappings:' in {config_path} to map each target to an Oxy database name.\n\n\
        Example oxy.yml:\n\
        \x20 mappings:\n\
        \x20   dev: my_local_db   # dbt target 'dev' maps to Oxy database 'my_local_db'"
    )]
    UnmappedDbtDatabases {
        unmapped: String,
        config_path: String,
    },

    #[error(
        "Oxy context (ConfigManager / SecretsManager) is not attached to this AirformService. \
        Call AirformService::with_oxy_context() before running models."
    )]
    MissingOxyContext,

    #[error(
        "Database type mismatch in oxy.yml mapping: {mismatches}.\n\n\
        Each dbt target must map to an Oxy database of the same type \
        (e.g. a dbt 'snowflake' target must map to a Snowflake database in config.yml).\n\n\
        Check the 'mappings:' section in {config_path}."
    )]
    DatabaseTypeMismatch {
        mismatches: String,
        config_path: String,
    },

    #[error(
        "No dbt profile loaded for project at {0}. \
        Ensure profiles.yml exists and the profile name in dbt_project.yml is correct."
    )]
    MissingProfile(String),

    #[error(
        "Oxy database '{db_name}' is a DuckDB directory source (file_search_path), which is \
        read-only and cannot be used as a dbt output target.\n\n\
        dbt needs a persistent DuckDB file to materialize models into. \
        Configure a DuckDB file database in config.yml:\n\n\
        \x20 - name: {db_name}\n\
        \x20   type: duckdb\n\
        \x20   path: path/to/your.duckdb"
    )]
    DuckDbLocalNotSupported { db_name: String },

    #[error("{0}")]
    Other(String),
}

impl From<anyhow::Error> for AirformIntegrationError {
    fn from(err: anyhow::Error) -> Self {
        // Check if the underlying error is an AirformError
        match err.downcast::<airform_core::AirformError>() {
            Ok(airform_err) => AirformIntegrationError::Airform(airform_err),
            Err(other) => AirformIntegrationError::Other(other.to_string()),
        }
    }
}
