//! Sea-ORM entity model for the airhouse-owned `airhouse_tenants` table.
//!
//! `airhouse_users` was removed in Phase 6 of the SA migration; the
//! ephemeral-only flow no longer persists per-user state. The migration
//! that dropped it is `m20260508_000001_drop_airhouse_users`. Cross-table
//! `Related` impls to `entity::workspaces` were intentionally not added
//! when the airhouse migrations moved here — they were dead code, and
//! the FK constraints still exist at the database level
//! (see `crate::migration`).

pub mod tenants;

// Convenience alias mirroring the entity crate's prelude pattern.
pub use tenants::Entity as Tenants;
