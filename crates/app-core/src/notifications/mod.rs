//! The notification service below the HTTP layer: the input a custom app's
//! `ctx.notify` sends, who it resolves to, the catalogue of kinds, the content
//! and the write path (inbox row, send, signals, queued delivery task).
//!
//! A service module of the prelude, not a crate: one surface calls it and
//! nothing needs a crate boundary to break a cycle (the decision is recorded in
//! `internal-docs/custom-apps-architecture.md`). The axum handlers, the delivery
//! transport (push, email, Slack: secrets and HTTP clients from `AppState`) and
//! the user-facing settings stay in `oxy-app` (`server::api::notifications`),
//! which re-exports these at their old paths.

pub mod catalog;
pub mod compose;
pub mod default_email;
pub mod input;
pub mod links;
pub mod recipients;
pub mod send;
pub mod task;
