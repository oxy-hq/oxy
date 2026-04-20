//! Backend implementations of [`crate::store::ObservabilityStore`].
//!
//! Each backend is behind a Cargo feature flag; at minimum one backend
//! feature should be enabled for the store to be useful.

#[cfg(feature = "duckdb")]
pub mod duckdb;

#[cfg(feature = "postgres")]
pub mod postgres;

#[cfg(feature = "clickhouse")]
pub mod clickhouse;
