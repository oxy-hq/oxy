//! Shared semantic-query compile + preagg execution helpers.
//!
//! Extracted out of `agentic-workflow` so both the analytics domain and
//! the workflow domain can call the same preagg-aware compile path
//! without violating the domain-to-domain dependency ban.

pub mod compile;
pub mod config;
pub mod error;
pub mod preagg;
pub mod refresh_key_cache;

pub use compile::{CompiledQuery, get_database_from_views, resolve_and_compile};
pub use error::SemanticError;
