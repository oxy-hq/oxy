//! Sea-ORM entity models for the airhouse-owned tables (`airhouse_tenants`
//! + `airhouse_users`).
//!
//! Lifted out of the central `entity` crate when the airhouse migrations
//! moved here. Cross-table `Related` impls to `entity::workspaces` /
//! `entity::users` were dropped in the move because they were dead code
//! (no `find_related` call sites in the workspace) and importing them
//! would force this crate to depend on the central `entity`'s internal
//! modules. The FK constraints themselves still exist at the database
//! level — see `crate::migration`.

pub mod tenants;
pub mod users;

// Convenience aliases mirroring the entity crate's prelude pattern.
pub use tenants::Entity as Tenants;
pub use users::Entity as Users;
