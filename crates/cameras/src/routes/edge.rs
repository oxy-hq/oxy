//! Device-token-authenticated routes for edge boxes.
//!
//! Every handler in this module pulls the resolved
//! [`EdgeContext`](crate::auth::EdgeContext) via
//! [`EdgeContextExtractor`](crate::auth::EdgeContextExtractor). The
//! middleware in [`crate::routes::router`] runs before each handler.

use axum::extract::Path;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Extension, Json, Router};
use sea_orm::DatabaseConnection;
use uuid::Uuid;

use crate::auth::EdgeContextExtractor;
use crate::entities::edge_boxes;
use crate::routes::dto::{
    AcceptedResponse, BoxBandwidthBody, BoxHealthBody, CameraConfigDto, CameraHealthBody,
    ComplianceBatch, EdgeConfigDto, EventBatch, LogBatch, SignClipBody,
    TargetView as TargetViewDto,
};
use crate::routes::errors::map as map_err;
use crate::service::{clips, compliance, config, ingest, logs, packs, updates};
use sea_orm::{EntityTrait, Set};

pub fn routes<S>() -> Router<S>
where
    S: Clone + Send + Sync + 'static,
{
    Router::new()
        .route("/control/boxes/{box_id}/config", get(get_config))
        .route("/control/boxes/{box_id}/target", get(get_target))
        .route("/control/boxes/{box_id}/health", post(post_box_health))
        .route(
            "/control/boxes/{box_id}/bandwidth",
            post(post_box_bandwidth),
        )
        .route("/control/cameras/{cam_id}/health", post(post_camera_health))
        .route("/control/events", post(post_events))
        .route("/control/compliance-reports", post(post_compliance_reports))
        .route("/control/clips/sign", post(post_sign_clip))
        .route("/control/boxes/{box_id}/logs", post(post_logs))
}

// ── Config ──────────────────────────────────────────────────────────────────

async fn get_config(
    Extension(db): Extension<DatabaseConnection>,
    EdgeContextExtractor(ctx): EdgeContextExtractor,
    Path(box_id): Path<Uuid>,
) -> Response {
    // URL box_id must match the token's box_id. Prevents one edge from
    // reading another's config even with a valid (different) bearer.
    if box_id != ctx.edge_box_id {
        return StatusCode::FORBIDDEN.into_response();
    }
    let cams = match config::fetch_cameras_for_edge(&db, ctx.edge_box_id).await {
        Ok(c) => c,
        Err(e) => return map_err(e),
    };
    let cameras: Vec<CameraConfigDto> = cams
        .into_iter()
        .map(|c| CameraConfigDto {
            id: c.id,
            name: c.name,
            rtsp_url: c.rtsp_url,
            substream_url: c.substream_url,
            zones_json: c.zones_json,
            lines_json: c.lines_json,
            analytics_consent: c.analytics_consent,
            role: c.role,
        })
        .collect();

    // Fetch the workspace's active domain pack and inline its body.
    // A pack-fetch failure shouldn't take down config polling — the
    // worker degrades to built-in prompts if `domain_pack` is null,
    // so we log + send None rather than 500 the whole response.
    let domain_pack = match packs::get_active(&db, ctx.workspace_id).await {
        Ok(row) => Some(row.body),
        Err(e) => {
            tracing::warn!(
                workspace_id = %ctx.workspace_id,
                edge_box_id = %ctx.edge_box_id,
                error = %e,
                "domain pack fetch failed; edge will use built-in prompts"
            );
            None
        }
    };

    Json(EdgeConfigDto {
        cameras,
        domain_pack,
        workspace_id: ctx.workspace_id,
    })
    .into_response()
}

// ── Health (stub — pending step 10) ─────────────────────────────────────────

async fn post_box_health(
    Extension(db): Extension<sea_orm::DatabaseConnection>,
    EdgeContextExtractor(ctx): EdgeContextExtractor,
    Path(box_id): Path<Uuid>,
    Json(body): Json<BoxHealthBody>,
) -> Response {
    if box_id != ctx.edge_box_id {
        return StatusCode::FORBIDDEN.into_response();
    }

    // IoT Phase 2 — stamp current_image_tag + last_update_result on
    // the edge_boxes row so the operator UI can show the post-
    // update state. Done first because the high-volume Airhouse
    // write below may fail (broker not configured locally) and we
    // want the update state recorded either way.
    if let Err(e) = updates::record_health_update(
        &db,
        ctx.edge_box_id,
        body.image_tag.clone(),
        body.last_update_result.clone(),
        body.compatibility.clone(),
    )
    .await
    {
        // Don't fail the health ping if the update-state write
        // errors — log + continue so the metrics path keeps flowing.
        tracing::warn!(
            edge_box_id = %ctx.edge_box_id,
            error = %e,
            "record_health_update failed; continuing to ingest"
        );
    }

    let payload = ingest::BoxHealthPayload {
        box_id: ctx.edge_box_id,
        ts: chrono::Utc::now(),
        cpu_pct: body.cpu_pct,
        gpu_pct: body.gpu_pct,
        mem_pct: body.mem_pct,
        temp_c: body.temp_c,
        uptime_s: body.uptime_s,
        image_tag: body.image_tag,
        containers_running: body.containers_running,
    };
    match ingest::write_box_health(ctx.workspace_id, vec![payload]).await {
        Ok(r) => Json(AcceptedResponse {
            accepted: r.accepted,
        })
        .into_response(),
        Err(e) => map_err(e),
    }
}

/// `GET /control/boxes/{id}/target` — device's update agent polls
/// this every ~5 minutes. Returns target image tag + current
/// image tag + an optional hold window. The agent compares
/// target vs the locally-running image and decides whether to
/// pull + restart. See `service::updates::get_target_for_edge`.
async fn get_target(
    Extension(db): Extension<sea_orm::DatabaseConnection>,
    EdgeContextExtractor(ctx): EdgeContextExtractor,
    Path(box_id): Path<Uuid>,
) -> Response {
    if box_id != ctx.edge_box_id {
        return StatusCode::FORBIDDEN.into_response();
    }
    match updates::get_target_for_edge(&db, ctx.edge_box_id).await {
        Ok(view) => Json(TargetViewDto::from(view)).into_response(),
        Err(e) => map_err(e),
    }
}

/// `POST /control/boxes/{id}/bandwidth` — stamp the box's most-
/// recent outbound delta on Postgres so operators can see who's
/// eating their Tailscale Funnel quota in the cameras dashboard.
///
/// Updates `edge_boxes.bandwidth_5min_bytes` + `…_reported_at`
/// directly (low cardinality + UI needs latest, not history; no
/// Airhouse round-trip needed). The Tailscale admin API gives the
/// precise per-tailnet figure when we need an exact number;
/// what we store here is MTX's totalsSent delta which overcounts
/// but is the safe-side error for early warning.
async fn post_box_bandwidth(
    Extension(db): Extension<sea_orm::DatabaseConnection>,
    EdgeContextExtractor(ctx): EdgeContextExtractor,
    Path(box_id): Path<Uuid>,
    Json(body): Json<BoxBandwidthBody>,
) -> Response {
    if box_id != ctx.edge_box_id {
        return StatusCode::FORBIDDEN.into_response();
    }
    // i64 vs u64: Postgres BIGINT is i64. A single 5-min interval
    // would have to push >9 EB to overflow, so the cast is safe
    // in practice; clamp at i64::MAX to keep things honest.
    let bytes = body.delta_bytes.min(i64::MAX as u64) as i64;
    let now = chrono::Utc::now();
    let am = edge_boxes::ActiveModel {
        id: Set(ctx.edge_box_id),
        bandwidth_5min_bytes: Set(Some(bytes)),
        bandwidth_reported_at: Set(Some(now.into())),
        updated_at: Set(now.into()),
        ..Default::default()
    };
    match edge_boxes::Entity::update(am).exec(&db).await {
        Ok(_) => Json(AcceptedResponse { accepted: 1 }).into_response(),
        Err(e) => {
            tracing::warn!(
                edge_box_id = %ctx.edge_box_id,
                error = %e,
                "bandwidth stamp failed"
            );
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("update failed: {e}"),
            )
                .into_response()
        }
    }
}

async fn post_camera_health(
    EdgeContextExtractor(_ctx): EdgeContextExtractor,
    Path(cam_id): Path<Uuid>,
    Json(body): Json<CameraHealthBody>,
) -> Response {
    let payload = ingest::CameraHealthPayload {
        camera_id: cam_id,
        ts: chrono::Utc::now(),
        fps: body.fps,
        bitrate_kbps: body.bitrate_kbps,
        last_frame_at: body.last_frame_at,
        decoder_errors: body.decoder_errors,
        reconnect_count: body.reconnect_count,
    };
    let workspace_id = _ctx.workspace_id;
    match ingest::write_camera_health(workspace_id, vec![payload]).await {
        Ok(r) => Json(AcceptedResponse {
            accepted: r.accepted,
        })
        .into_response(),
        Err(e) => map_err(e),
    }
}

// ── Events (stub — pending step 10) ─────────────────────────────────────────

async fn post_events(
    EdgeContextExtractor(ctx): EdgeContextExtractor,
    Json(batch): Json<EventBatch>,
) -> Response {
    match ingest::write_events(ctx.workspace_id, batch.events).await {
        Ok(r) => Json(AcceptedResponse {
            accepted: r.accepted,
        })
        .into_response(),
        Err(e) => map_err(e),
    }
}

// ── Compliance reports (stub — pending step 10) ─────────────────────────────

async fn post_compliance_reports(
    EdgeContextExtractor(ctx): EdgeContextExtractor,
    Json(batch): Json<ComplianceBatch>,
) -> Response {
    match compliance::write_reports(ctx.workspace_id, batch.reports).await {
        Ok(r) => Json(AcceptedResponse {
            accepted: r.accepted,
        })
        .into_response(),
        Err(e) => map_err(e),
    }
}

/// `POST /control/clips/sign` — worker requests a presigned S3
/// PUT URL for a violation clip (Tier B #6 follow-up).
///
/// The URL is bound to bucket + key + content length + content
/// type + TTL, so even if it leaks it can't be re-used for a
/// different blob or path. The worker uses the returned `key`
/// as the `evidence_s3_key` on the corresponding compliance
/// report.
async fn post_sign_clip(
    EdgeContextExtractor(ctx): EdgeContextExtractor,
    Json(body): Json<SignClipBody>,
) -> Response {
    let input = clips::MintPutInput {
        workspace_id: ctx.workspace_id,
        report_id: body.report_id,
        segment_start: body.segment_start,
        content_length: body.content_length,
    };
    match clips::mint_put_url(input).await {
        Ok(out) => Json(out).into_response(),
        Err(e) => map_err(e),
    }
}

/// `POST /control/boxes/{id}/logs` — device's log shipper drops a
/// batch here every ~30s (or 50 lines, whichever hits first).
/// URL `box_id` must match the bearer's `edge_box_id` — same
/// defense-in-depth check as the other per-box endpoints; a
/// rogue or buggy box can't dump rows attributed to a neighbor.
async fn post_logs(
    EdgeContextExtractor(ctx): EdgeContextExtractor,
    Path(box_id): Path<Uuid>,
    Json(batch): Json<LogBatch>,
) -> Response {
    if box_id != ctx.edge_box_id {
        return StatusCode::FORBIDDEN.into_response();
    }
    match logs::write_batch(ctx.workspace_id, ctx.edge_box_id, batch.logs).await {
        Ok(r) => Json(AcceptedResponse {
            accepted: r.accepted,
        })
        .into_response(),
        Err(e) => map_err(e),
    }
}
