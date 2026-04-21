//! Observability crate for Oxy.
//!
//! Provides the `ObservabilityStore` trait plus pluggable backends (DuckDB,
//! Postgres, ClickHouse), a tracing `SpanCollectorLayer`, and the
//! `init_observability` bridge that wires them together.

pub mod backends;
pub mod duration;
pub mod global;
pub mod intent_types;
pub mod layer;
pub mod store;
pub mod telemetry;
pub mod types;

pub use duration::{DURATIONS, DurationWindow, RETENTION_DAYS};
pub use global::{get_global, set_global};
pub use layer::{SpanCollectorLayer, current_trace_id};
pub use store::ObservabilityStore;
pub use telemetry::{
    build_layer_and_receiver, build_observability_layer, init_observability, init_stdout,
    observability_filter, shutdown, spawn_bridge, spawn_retention_cleanup,
};
pub use types::*;
