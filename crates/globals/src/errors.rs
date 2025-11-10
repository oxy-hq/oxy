use thiserror::Error;

/// Result type for global operations
pub type GlobalResult<T> = Result<T, GlobalError>;

/// Errors that can occur when working with global objects
#[derive(Error, Debug)]
pub enum GlobalError {
    #[error("Invalid global reference syntax: {0}")]
    InvalidReferenceSyntax(String),

    #[error("Global file not found: {0}")]
    FileNotFound(String),

    #[error("Failed to read global file: {0}")]
    FileReadError(String),

    #[error("Failed to parse YAML file '{file}': {error}")]
    YamlParseError { file: String, error: String },

    #[error("Invalid YAML structure in '{file}': {error}")]
    InvalidYamlStructure { file: String, error: String },

    #[error("Global object not found: {0}")]
    ObjectNotFound(String),

    #[error("Invalid object path: {0}")]
    InvalidObjectPath(String),

    #[error("Missing required field '{field}' in {location}")]
    MissingRequiredField { field: String, location: String },

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("YAML error: {0}")]
    YamlError(#[from] serde_yaml::Error),
}
