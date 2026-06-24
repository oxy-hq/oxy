//! Thread and conversation management for Oxy
//!
//! Threads serve as execution containers that store the input/output of
//! various operations including automation executions, agent runs, and SQL queries.
//!
//! ## Architecture Notes
//!
//! While automations and other components write results to thread entities,
//! this crate does not depend on those components. The relationship flows
//! through the entity layer:
//!
//! - `oxy-workflow` → `entity::threads` (writes execution results)
//! - `oxy-thread` → `entity::threads` (domain models and operations)
//!
//! This keeps the thread crate focused on thread lifecycle management
//! without coupling to specific execution engines.
