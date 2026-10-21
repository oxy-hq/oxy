//! A sandbox agent token on the custom-app serving paths. The admission rules are
//! in `oxy_custom_apps::agent`; the usage count that wraps `/fn` and `/logs`
//! (which names an axum `Response`) is `oxy_app_core::usage_probe`. Both stay
//! reachable here, at the path every call site already uses.

pub use oxy_app_core::usage_probe::{UsageProbe, seen};
pub use oxy_custom_apps::agent::*;
