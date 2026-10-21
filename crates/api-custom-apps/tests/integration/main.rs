//! `oxy-api-custom-apps`'s integration tests — one binary, per the workspace rule
//! (add a case as a `mod` here, never a new top-level `tests/*.rs`).
//!
//! These open no database. The tests that drive the serve route against a seeded
//! app, a published build or the function runtime stay in `oxy-app`'s
//! `tests/custom_apps/` group, because the seed, `publish()` and the runtime are
//! still in `oxy-app`; they move here with the second lift
//! (`internal-docs/custom-apps-architecture.md` §5, Wave 3).

mod public_routes;
mod route_roles;
