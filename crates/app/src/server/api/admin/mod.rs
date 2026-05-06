//! `/api/admin/*` — Oxy-staff-only endpoints, gated by an `OXY_OWNER`
//! match against the authenticated user's email. Mounted in
//! `router::global` outside the org-scoped subscription guard.

pub mod billing;

use axum::Router;

use crate::server::feature_flags;
use crate::server::router::AppState;

/// Admin routes are flat under `/api/admin/*` after the 2026-04-28 redesign.
/// Endpoints:
///   - GET    /admin/orgs?status=...
///   - GET    /admin/billing/prices
///   - GET    /admin/orgs/{org_id}/billing/subscription
///   - POST   /admin/orgs/{org_id}/billing/provision-subscription
///   - POST   /admin/orgs/{org_id}/billing/provision-checkout
///   - GET    /admin/orgs/{org_id}/billing/checkout
///   - POST   /admin/orgs/{org_id}/billing/checkout/resend
///   - POST   /admin/orgs/{org_id}/billing/checkout/cancel
///   - POST   /admin/orgs/{org_id}/billing/resync
///   - GET    /admin/feature-flags
///   - PATCH  /admin/feature-flags/{key}
pub(crate) fn router() -> Router<AppState> {
    billing::router().merge(feature_flags::routes::router())
}
