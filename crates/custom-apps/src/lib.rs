//! Custom Apps domain logic: the modules of the custom-apps surface that import
//! nothing from `oxy-app`.
//!
//! A domain crate in the sense of `internal-docs/domain-boundaries.md` rule S3:
//! it sits below `oxy-app` and every `api-*` surface, so a surface that needs
//! custom-app behavior imports it from here rather than reaching into `oxy-app`.
//! It holds no routes and no handlers. Seeded as a pure relocation (S6 step 3);
//! `oxy-app` keeps each module reachable at its old `server::api::custom_apps_*`
//! path through a re-export.

pub mod environments;
pub mod publish_authz;
