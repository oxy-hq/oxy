//! Storage modules for various backends

pub mod clickhouse;

pub use clickhouse::{ClickHouseConfig, ClickHouseStorage};
