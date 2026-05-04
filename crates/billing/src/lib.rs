//! `oxy-billing` — Stripe-backed per-seat subscription logic for Oxy orgs.
//!
//! Layered so platform crates never import this one directly; only `oxy-app`
//! depends on it. See `internal-docs/2026-04-24-stripe-billing-design.md` for
//! the authoritative design.

pub mod client;
pub mod config;
pub mod errors;
pub mod service;
pub mod state;
pub mod webhook;

pub use config::StripeConfig;
pub use errors::BillingError;
pub use service::{BillingService, SubscriptionOverview};
