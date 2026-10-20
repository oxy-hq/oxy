//! Custom Apps domain logic: the modules of the custom-apps surface that need
//! nothing from `oxy-app` and no HTTP types.
//!
//! A domain crate in the sense of `internal-docs/domain-boundaries.md` rule S3:
//! it sits below `oxy-app` and every `api-*` surface, so a surface that needs
//! custom-app behavior imports it from here rather than reaching into `oxy-app`.
//! It holds no routes, no handlers and no HTTP types. Built as pure relocations
//! (S6 step 3); `oxy-app` keeps each module reachable at its old
//! `server::api::custom_apps_*` path through a re-export.
//!
//! - `auth`: who may open an app, app and org roles, the access verdict cache.
//! - `cache`: the bounded TTL caches of the serve chain and their invalidation.
//! - `env_resolve`: which build an app environment serves.
//! - `non_production`: may this caller open a non-production environment.
//! - `agent_built`: the mark of a build a sandbox agent token published.
//! - `audience`: who can reach an app (the rule `ctx.org.*` and `ctx.notify` share).
//! - `function_trigger`: why a one-off function job was queued, and what the queue reads back.
//! - `build_state`: what a build row says about whether it may go live.
//! - `environments`, `publish_authz`: pointer writes and publish authority.
//! - `staging_pin`: a draft build's semantic pin: read it, validate it at publish,
//!   resolve it for a non-production request, and the promote drift notice.

pub mod agent_built;
pub use oxy_app_core::audience;
pub mod auth;
pub mod build_state;
pub mod cache;
pub mod env_resolve;
pub mod environments;
pub mod function_trigger;
pub mod non_production;
pub mod publish_authz;
pub mod staging_pin;
