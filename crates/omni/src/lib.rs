//! # Omni Integration Crate
//!
//! This crate provides integration with Omni's semantic layer API, including:
//! - API client for interacting with Omni services
//! - Data models for Omni API responses and requests
//! - Metadata storage and management
//! - Error handling and resilience features
//! - Metadata merging capabilities

pub mod client;
pub mod error;
pub mod metadata;
pub mod models;
pub mod resilience;
pub mod storage;

// Re-export main types for convenience
pub use client::OmniApiClient;
pub use error::OmniError;
pub use metadata::MetadataMerger;
pub use models::*;
pub use resilience::{ConnectionHealthChecker, RetryConfig, RetryPolicy, TimeoutWrapper};
pub use storage::MetadataStorage;
