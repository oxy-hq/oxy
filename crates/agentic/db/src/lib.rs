//! Shared SeaORM entities and migrations for the agentic pipeline.
//!
//! Other crates in the workspace that need to read/write agentic run state or
//! run the schema migrations should depend on this crate instead of duplicating
//! the definitions.

pub mod entity;
pub mod migration;
