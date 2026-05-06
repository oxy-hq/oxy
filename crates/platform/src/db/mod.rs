//! Database connection: pool initialization, auth-mode dispatch, RDS IAM
//! token refresher.
//!
//! Public surface is intentionally tiny — most users only need
//! [`establish_connection`]. The [`auth_mode`] and [`iam`] submodules are
//! kept `pub(crate)` because they're implementation details of the
//! connection setup; nothing outside this crate has called them historically.

pub(crate) mod auth_mode;
mod client;
pub(crate) mod iam;

pub use client::establish_connection;
