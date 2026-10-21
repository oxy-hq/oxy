//! Reach lives in `oxy_app_core::operating_graph::reach`; re-exported at the old
//! path. [`reach_for_viewer`] (the viewer's reach with their app role resolved)
//! is `oxy_custom_apps::reach`, beside the access decisions it reads.

pub use oxy_app_core::operating_graph::reach::*;
pub use oxy_custom_apps::reach::reach_for_viewer;
