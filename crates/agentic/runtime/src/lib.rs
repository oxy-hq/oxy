//! Transport-agnostic execution infrastructure for agentic pipelines.
//!
//! Provides run lifecycle management, event persistence, SSE streaming support,
//! and the `EventRegistry` for domain-aware event processing. Used by both the
//! HTTP server (`agentic-http`) and CLI (`oxy agentic`).
//!
//! This crate is **domain-agnostic** — it never imports analytics, builder, or
//! any domain-specific types. Domain behavior is injected via callbacks and
//! the `EventRegistry` pattern.

pub mod bridge;
pub mod circuit_breaker;
pub mod coordinator;
pub mod crud;
pub mod entity;
pub mod event_registry;
pub mod handle;
pub mod migration;
pub mod state;
pub mod transport;
pub mod worker;
