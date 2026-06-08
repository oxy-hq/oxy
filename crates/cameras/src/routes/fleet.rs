//! Public IoT fleet endpoints (device-facing).
//!
//! Mounted at the *root* of the app router — not behind the
//! `/{workspace_id}/` prefix or the device-token bearer middleware
//! used by `/control/*`. Devices that hit these routes either
//! don't yet have a workspace assignment (announce) or don't yet
//! have a bearer (bootstrap); the routes carry their own HMAC
//! auth derived from the device secret written into the box at
//! provisioning time.
//!
//! - `POST /fleet/announce/{device_id}` — HMAC-signed by the
//!   device's secret. Verifies, stamps `last_announce_at`,
//!   returns the current claim status so the device knows
//!   whether to keep polling or call bootstrap.
//!
//! - `POST /fleet/bootstrap/{device_id}` — graduates a
//!   `pending_claim` device into the runtime fleet: materializes
//!   an `edge_boxes` row + bearer, returns the bearer for
//!   `/control/*` use.
//!
//! The factory-enroll endpoint (`POST /fleet/factory-enroll`)
//! that used to gate device registration behind a shared
//! `OXY_FACTORY_ENROLL_SECRET` was removed. We don't run a
//! factory; the `POST /{wid}/fleet/prepared-devices` operator
//! endpoint ("Add device" wizard) writes the device_registry
//! row + pending_claim atomically server-side.

use axum::extract::Path;
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use axum::{Extension, Json, Router};
use sea_orm::DatabaseConnection;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::routes::errors::map as map_err;
use crate::service::fleet::{
    self, AnnounceInput, AnnounceOutcome, BootstrapInput, decode_signature,
};

pub fn routes<S>() -> Router<S>
where
    S: Clone + Send + Sync + 'static,
{
    Router::new()
        .route("/fleet/announce/{device_id}", post(announce))
        .route("/fleet/bootstrap/{device_id}", post(bootstrap))
}

// ── Announce ────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct AnnounceBody {
    /// Device's own clock at request time. Server enforces a ±5min
    /// replay window.
    timestamp: i64,
    /// Base64 (no padding) HMAC-SHA256 over
    /// `device_id || "." || timestamp` keyed on device_secret.
    signature_b64: String,
}

#[derive(Debug, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
enum AnnounceResponse {
    NotClaimed,
    PendingClaim,
    Claimed {
        workspace_id: Uuid,
        edge_box_id: Uuid,
    },
}

async fn announce(
    Extension(db): Extension<DatabaseConnection>,
    Path(device_id): Path<Uuid>,
    Json(body): Json<AnnounceBody>,
) -> Response {
    let sig = match decode_signature(&body.signature_b64) {
        Ok(b) => b,
        Err(_) => {
            return map_err(crate::service::ServiceError::InvalidInput(
                "signature_b64 must be base64 (no padding)".into(),
            ));
        }
    };

    let input = AnnounceInput {
        device_id,
        timestamp: body.timestamp,
        signature: &sig,
    };

    match fleet::announce_device(&db, input).await {
        Ok(AnnounceOutcome::NotClaimed) => Json(AnnounceResponse::NotClaimed).into_response(),
        Ok(AnnounceOutcome::PendingClaim) => Json(AnnounceResponse::PendingClaim).into_response(),
        Ok(AnnounceOutcome::Claimed {
            workspace_id,
            edge_box_id,
        }) => Json(AnnounceResponse::Claimed {
            workspace_id,
            edge_box_id,
        })
        .into_response(),
        Err(e) => map_err(e),
    }
}

// ── Bootstrap ───────────────────────────────────────────────────────────────

/// Same wire shape as announce, but a different endpoint and a
/// stronger outcome — exchanging a verified HMAC for an
/// `edge_boxes` row + bearer token. The device polls `/fleet/announce`
/// until it sees `pending_claim`, then calls this once to graduate
/// to the normal `/control/*` flow.
#[derive(Debug, Deserialize)]
struct BootstrapBody {
    timestamp: i64,
    signature_b64: String,
}

#[derive(Debug, Serialize)]
struct BootstrapResponse {
    workspace_id: Uuid,
    edge_box_id: Uuid,
}

async fn bootstrap(
    Extension(db): Extension<DatabaseConnection>,
    Path(device_id): Path<Uuid>,
    Json(body): Json<BootstrapBody>,
) -> Response {
    let sig = match decode_signature(&body.signature_b64) {
        Ok(b) => b,
        Err(_) => {
            return map_err(crate::service::ServiceError::InvalidInput(
                "signature_b64 must be base64 (no padding)".into(),
            ));
        }
    };

    let input = BootstrapInput {
        device_id,
        timestamp: body.timestamp,
        signature: &sig,
    };
    match fleet::bootstrap_device(&db, input).await {
        Ok(out) => Json(BootstrapResponse {
            workspace_id: out.workspace_id,
            edge_box_id: out.edge_box_id,
        })
        .into_response(),
        Err(e) => map_err(e),
    }
}
