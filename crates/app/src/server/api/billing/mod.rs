//! Billing HTTP surface. Mounted only when Stripe is configured — see
//! `global.rs` for the mount point and `public.rs` for the webhook route.

pub mod handlers;
pub mod notify;
pub mod webhook;

pub use handlers::{billing_disabled, billing_service};

use axum::Router;
use axum::routing::{get, post};

use crate::server::router::AppState;

/// Routes nested under `/orgs/{org_id}/billing/*`. Webhook endpoint is
/// mounted via `public.rs`; `/config/stripe` was removed in the
/// 2026-04-28 sales-gated redesign (no Stripe.js consumer).
///
/// `/` (full summary), `/portal-session`, and `/invoices` are admin-only.
/// `/status` returns the minimum needed to drive the FE paywall and is
/// readable by any org member.
pub(crate) fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(handlers::get_billing))
        .route("/status", get(handlers::get_billing_status))
        .route("/portal-session", post(handlers::post_portal))
        .route("/invoices", get(handlers::get_invoices))
        .route(
            "/checkout-sessions/{session_id}",
            get(handlers::get_checkout_session),
        )
}
