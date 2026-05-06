//! Re-export shim. The user-table filter helpers live in `oxy-platform`;
//! this module preserves the legacy `oxy::database::filters` import path
//! so existing call sites compile unchanged.
pub use oxy_platform::filters::{UserFilters, UserQueryFilterExt};
