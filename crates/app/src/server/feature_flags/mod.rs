//! Backend feature flag system.
//!
//! - `registry` — code-declared list of flags + defaults.
//! - `store` — Sea-ORM helpers for the `feature_flags` table.
//! - `cache` — in-memory cache, preloaded at startup, write-through on
//!   admin PATCH. Exposes `is_enabled(key)` for read-site gating.
//! - `routes` — admin API: list + patch.

pub mod cache;
pub mod registry;
pub mod routes;
pub mod store;

pub use cache::is_enabled;
