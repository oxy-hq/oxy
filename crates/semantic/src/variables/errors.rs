use crate::errors::SemanticLayerError;
use std::fmt::{Display, Formatter};

/// Errors that can occur during variable processing
#[derive(Debug, Clone, PartialEq)]
pub enum VariableError {
    /// Variable was referenced but not found in the context
    VariableNotFound(String),
    /// Variable syntax is invalid or malformed  
    InvalidSyntax(String),
    /// Circular reference detected in variable resolution
    CircularReference(String),
    /// Variable type does not match expected type
    TypeMismatch(String),
    /// Variable encoding/decoding failed
    InvalidEncoding(String),
}

impl Display for VariableError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            VariableError::VariableNotFound(var) => {
                write!(f, "Variable '{}' not found in context", var)
            }
            VariableError::InvalidSyntax(msg) => {
                write!(f, "Invalid variable syntax: {}", msg)
            }
            VariableError::CircularReference(var) => {
                write!(f, "Circular reference detected for variable '{}'", var)
            }
            VariableError::TypeMismatch(msg) => {
                write!(f, "Variable type mismatch: {}", msg)
            }
            VariableError::InvalidEncoding(msg) => {
                write!(f, "Variable encoding error: {}", msg)
            }
        }
    }
}

impl std::error::Error for VariableError {}

impl From<VariableError> for SemanticLayerError {
    fn from(err: VariableError) -> Self {
        SemanticLayerError::VariableError(err.to_string())
    }
}
