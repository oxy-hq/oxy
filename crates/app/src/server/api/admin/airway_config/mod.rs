//! `/api/admin/airway/config` — staff read/write of Airway admission policy
//! config (`airway_source_config`): the global per-source-kind row plus any
//! per-workspace overrides.
//!
//! Gated by `cap(Action::PlatformOperate)` — the capability the other
//! deployment-wide operational surfaces (`workspace_health` / `routing` /
//! `metrics`) already take — **not** strict `OXY_OWNER`. The capability is the
//! door; scope is the fence, and this surface carries its own because
//! `Resource::platform()` has no org for a guard to check. Every route that
//! returns or writes per-tenant rows fences: see [`handlers`]' module doc for
//! the two override writes and the listing, and [`preview`]'s for the scan.
//! `admin::mod`'s note at the mount point covers the one reach deliberately
//! left fleet-wide — the **global row**, which belongs to no org.
//!
//! That sentence used to point at the mount-point note for "the one reach left
//! fleet-wide" while [`preview`] was also unfenced, and unfenced over strictly
//! more than the listing returned: workspace ids, real `.airway.yml` paths,
//! resource names and parse errors from every tenant. The note covers a *row*,
//! not a route; a doc that reads as coverage is worse than none, so the claim
//! is now specific enough to be falsifiable.
//!
//! Stage 2 added the `airway_source_config` table and
//! `agentic_pipeline::airway_config::resolve_admission`, with no way to
//! edit those rows except SQL. Task 1 shipped the read side, Task 2 the
//! four write endpoints (upsert/delete, global and per-workspace), and
//! Task 3 the [`preview`] endpoint that says which resources a stricter
//! policy would reject before it is saved. The frontend (Tasks 4-6)
//! builds on all three.
//!
//! **Every route here is `FleetOk`** — Postgres-only, reads and writes
//! alike. That includes [`preview`], which reads workspace *content* but
//! reads it from the compile boundary (`airway_pipelines`, scoped to each
//! workspace's promoted revision) rather than the working copy, so it needs
//! no ide node. See [`preview`]'s module doc for why the compiled rows beat
//! the "but `airway_run` reads the working copy" argument.
//!
//! Scope is the policy tier only: `contract_policy` and `environment`, the
//! two fields stage 2's resolver actually honours. See
//! `crates/entity/src/airway_source_config.rs` for why nothing else (e.g.
//! `max_rewind`, a `Deployment` region) belongs here yet — a knob nothing
//! reads is the failure airway's own plan calls out.

pub(crate) mod handlers;
pub(crate) mod preview;
pub(crate) mod preview_scan;

use axum::Router;
use axum::routing::{get, put};

use crate::server::router::AppState;

/// Every source kind the admin surface shows a card for, even with no
/// config row yet — matches `AirwayPipelineSpec::source.kind` spellings
/// (`crates/agentic/airway/src/config.rs`). Task 3's preview and Task 5's
/// frontend cards must read this same list so the surface can't drift
/// kind-by-kind. The write handlers also reuse this to reject a typo'd
/// `source_kind`: a row under an unknown kind would silently never appear
/// in [`handlers::get_config`]'s response, since that groups strictly by
/// this list.
pub(crate) const KNOWN_SOURCE_KINDS: &[&str] = &["toast", "quickbooks", "weather", "rest_api"];

pub(crate) fn router() -> Router<AppState> {
    Router::new().nest(
        "/airway",
        Router::new()
            .route("/config", get(handlers::get_config))
            .route(
                "/config/{source_kind}",
                put(handlers::put_global_config).delete(handlers::delete_global_config),
            )
            .route(
                "/config/{source_kind}/workspaces/{workspace_id}",
                put(handlers::put_workspace_override).delete(handlers::delete_workspace_override),
            )
            // Distinct path depth from the `PUT`/`DELETE` route above, so
            // axum matches them separately.
            .route(
                "/config/{source_kind}/preview",
                get(preview::preview_policy),
            ),
    )
}
