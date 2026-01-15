//! Thread and conversation management for Oxy
//!
//! Threads serve as execution containers that store the input/output of
//! various operations including workflow executions, agent runs, and SQL queries.
//!
//! ## Architecture Notes
//!
//! While workflows and other components write results to thread entities,
//! this crate does not depend on those components. The relationship flows
//! through the entity layer:
//!
//! - `oxy-workflow` → `entity::threads` (writes execution results)
//! - `oxy-agent` → `entity::threads` (writes agent outputs)
//! - `oxy-thread` → `entity::threads` (domain models and operations)
//!
//! This keeps the thread crate focused on thread lifecycle management
//! without coupling to specific execution engines.
