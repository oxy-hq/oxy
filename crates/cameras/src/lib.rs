//! Camera fleet domain — sites, edge boxes, cameras, and the control-plane
//! endpoints that connect them to the rest of Oxy.
//!
//! See `crates/cameras/CLAUDE.md` for the data split (Postgres vs Airhouse)
//! and `internal-docs/video-processing-fleet-architecture.md` for the full design.

// SeaORM 2.0's query types nest deeper generically than 1.1's, and rustc's
// default query depth is not enough to lay out the async fns that build
// queries here. Raising the limit is the fix rustc itself suggests.
#![recursion_limit = "256"]

pub mod airhouse;
pub mod auth;
pub mod entities;
pub mod migration;
pub mod routes;
pub mod secrets;
pub mod service;

// Re-export the migrator at the crate root so callers can `oxy_cameras::CamerasMigrator`
// without descending into the module — matches `agentic_runtime::RuntimeMigrator`.
pub use migration::CamerasMigrator;
