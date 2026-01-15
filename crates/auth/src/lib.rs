//! Authentication and authorization for Oxy
//!
//! This crate provides:
//! - User authentication (built-in and external providers)
//! - API key management
//! - Authorization middleware
//! - JWT token handling

pub mod api_key_domain;
pub mod api_key_infra;
pub mod authenticator;
pub mod built_in;
pub mod extractor;
pub mod middleware;
pub mod types;
pub mod user;

// Re-export commonly used items
pub use api_key_domain::*;
pub use api_key_infra::*;
pub use authenticator::*;
pub use built_in::*;
pub use extractor::*;
pub use middleware::*;
pub use types::*;
pub use user::*;
