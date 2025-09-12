use std::fmt;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum SemanticLayerError {
    ConfigurationError(String),
    IOError(String),
    ParsingError(String),
    UnknownError(String),
}

impl fmt::Display for SemanticLayerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SemanticLayerError::ConfigurationError(msg) => {
                write!(f, "Configuration error: {}", msg)
            }
            SemanticLayerError::IOError(msg) => write!(f, "IO error: {}", msg),
            SemanticLayerError::ParsingError(msg) => write!(f, "Parsing error: {}", msg),
            SemanticLayerError::UnknownError(msg) => write!(f, "Unknown error: {}", msg),
        }
    }
}
