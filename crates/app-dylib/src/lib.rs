//! Dev-only dynamic-linking shim — the Bevy `bevy_dylib` pattern.
//!
//! This crate exists ONLY to be compiled as a `dylib` that statically absorbs
//! oxy-app and its ~1.4 GB of heavy deps (DuckDB / DataFusion / Arrow / deno).
//! When `oxy-server` is built with `--features dev-dynamic` + `-C prefer-dynamic`,
//! an `extern crate oxy_app_dylib` in the binary forces this dylib into the link,
//! so oxy-app's symbols resolve dynamically from `liboxy_app_dylib.dylib` instead
//! of being statically re-linked into the ~1 GB binary on every edit. Editing a
//! surface (e.g. `oxy-api-github`) then rebuilds only that small crate + a cheap
//! dynamic relink — ~17s instead of ~52s.
//!
//! It is NOT part of any normal or release build: CI/release never enable
//! `dev-dynamic`, so this crate (and the expensive dylib) is never compiled.
//! See `just dev-backend-dyn`.
//!
//! `recursion_limit` mirrors the binary's: re-exporting oxy-app forces this
//! crate to lay out the deep async future types reached through it, which
//! exceeds rustc's default query depth (same reason `oxy-server`'s main.rs sets
//! it) since SeaORM 2.0 deepened its query types.
#![recursion_limit = "256"]
#![allow(unused_imports)]
pub use oxy_app::*;
