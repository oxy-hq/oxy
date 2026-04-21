//! [`ConfigError`] — errors returned during config loading and solver construction.

/// Errors returned during config loading and solver construction.
#[derive(Debug)]
pub enum ConfigError {
    /// The YAML file could not be read.
    Io(std::io::Error),
    /// The YAML could not be parsed.
    Yaml(serde_yaml::Error),
    /// A glob pattern was invalid.
    Glob(glob::PatternError),
    /// No databases were configured.
    NoDatabases,
    /// The database type is unsupported (only `sqlite` is built in).
    UnsupportedConnector(String),
    /// The connector could not be opened.
    ConnectorError(String),
    /// Semantic files could not be loaded.
    SemanticError(Box<dyn std::error::Error + Send + Sync>),
    /// The same table name exists in more than one configured database.
    AmbiguousTable(String),
    /// A validation rule name is unknown or its parameters are invalid.
    ValidationError(String),
    /// The `semantic_engine.vendor` value is not a known bundled adapter.
    UnsupportedEngine(String),
    /// The vendor engine could not be reached during the startup health-check.
    ///
    /// Hard failure — the solver is never constructed when this fires.
    EngineConnectionError(String),
    /// A `${VAR}` placeholder in the config references an environment variable
    /// that is not set.  Fail fast rather than sending an empty credential.
    MissingEnvVar(String),
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConfigError::Io(e) => write!(f, "IO error: {e}"),
            ConfigError::Yaml(e) => write!(f, "YAML parse error: {e}"),
            ConfigError::Glob(e) => write!(f, "glob pattern error: {e}"),
            ConfigError::NoDatabases => write!(f, "no databases configured"),
            ConfigError::UnsupportedConnector(t) => {
                write!(f, "unsupported connector type: '{t}'")
            }
            ConfigError::ConnectorError(e) => write!(f, "connector error: {e}"),
            ConfigError::SemanticError(e) => write!(f, "semantic catalog error: {e}"),
            ConfigError::AmbiguousTable(e) => write!(f, "ambiguous table: {e}"),
            ConfigError::ValidationError(e) => write!(f, "validation config error: {e}"),
            ConfigError::UnsupportedEngine(v) => {
                write!(f, "unsupported semantic engine vendor: '{v}'")
            }
            ConfigError::EngineConnectionError(e) => {
                write!(f, "semantic engine connection error: {e}")
            }
            ConfigError::MissingEnvVar(name) => {
                write!(
                    f,
                    "environment variable '${{{name}}}' referenced in config is not set; \
                     set it before starting the server"
                )
            }
        }
    }
}

impl std::error::Error for ConfigError {}

impl From<std::io::Error> for ConfigError {
    fn from(e: std::io::Error) -> Self {
        ConfigError::Io(e)
    }
}

impl From<serde_yaml::Error> for ConfigError {
    fn from(e: serde_yaml::Error) -> Self {
        ConfigError::Yaml(e)
    }
}

impl From<glob::PatternError> for ConfigError {
    fn from(e: glob::PatternError) -> Self {
        ConfigError::Glob(e)
    }
}
