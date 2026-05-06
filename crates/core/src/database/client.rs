//! Re-export shim. The connection-pool implementation lives in
//! `oxy-platform`; this module preserves the legacy `oxy::database::client`
//! import path so existing call sites compile unchanged.
pub use oxy_platform::db::establish_connection;
