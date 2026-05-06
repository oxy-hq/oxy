//! Platform glue: database connection pooling + encrypted org secret store.
//!
//! Extracted from `oxy` core so leaf crates (`airhouse`, future integrations)
//! can use these primitives without depending on the full `oxy` crate. The
//! `oxy` crate re-exports these symbols at their original paths for source
//! compatibility — see `oxy::database::client` and `oxy::adapters::secrets`.

pub mod db;
pub mod filters;
pub mod secrets;
