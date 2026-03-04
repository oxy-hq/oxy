//! # Looker Integration Crate
//!
//! This crate provides integration with Looker's API, including:
//! - API client for interacting with Looker services
//! - Data models for Looker API responses and requests
//! - Metadata storage and management
//! - Error handling
//! - Metadata merging capabilities

pub mod client;
pub mod error;
pub mod metadata;
pub mod models;
pub mod storage;

// Re-export main types for convenience
pub use client::{AccessToken, LookerApiClient, LookerAuthConfig};
pub use error::LookerError;
pub use metadata::MetadataMerger;
pub use models::{InlineQueryRequest, QueryResponse};
pub use storage::MetadataStorage;
