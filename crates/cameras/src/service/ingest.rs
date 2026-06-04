//! Event + health ingest into the workspace's Airhouse tenant.
//!
//! Each `write_*` builds a multi-row INSERT and runs it via
//! tokio-postgres simple-query (DuckLake doesn't support prepared
//! statements / `$N` placeholders). Schema is ensured lazily by
//! [`crate::airhouse::connect_and_ensure`] — first call per process per
//! workspace runs `CREATE TABLE IF NOT EXISTS` once, subsequent calls
//! skip straight to the INSERT.

use std::fmt::Write as _;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::airhouse::escape::{
    sql_opt_f32, sql_opt_i32, sql_opt_i64, sql_opt_str, sql_opt_ts, sql_str, sql_ts,
};

use super::{ServiceError, ServiceResult};

// ── Payload shapes (unchanged from the stub version) ────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventPayload {
    pub event_id: Uuid,
    pub ts: chrono::DateTime<chrono::Utc>,
    pub camera_id: Uuid,
    pub event_type: String,
    pub zone_id: Option<String>,
    pub line_id: Option<String>,
    pub track_id: String,
    pub dwell_seconds: Option<f32>,
    pub confidence: Option<f32>,
    pub frame_uri: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CameraHealthPayload {
    pub camera_id: Uuid,
    pub ts: chrono::DateTime<chrono::Utc>,
    pub fps: Option<f32>,
    pub bitrate_kbps: Option<f32>,
    pub last_frame_at: Option<chrono::DateTime<chrono::Utc>>,
    pub decoder_errors: i32,
    pub reconnect_count: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BoxHealthPayload {
    pub box_id: Uuid,
    pub ts: chrono::DateTime<chrono::Utc>,
    pub cpu_pct: Option<f32>,
    pub gpu_pct: Option<f32>,
    pub mem_pct: Option<f32>,
    pub temp_c: Option<f32>,
    pub uptime_s: Option<i64>,
    pub image_tag: Option<String>,
    pub containers_running: Option<i32>,
}

#[derive(Debug, Default)]
pub struct IngestResult {
    pub accepted: usize,
}

// ── Writers ─────────────────────────────────────────────────────────────────

pub async fn write_events(
    workspace_id: Uuid,
    events: Vec<EventPayload>,
) -> ServiceResult<IngestResult> {
    if events.is_empty() {
        return Ok(IngestResult { accepted: 0 });
    }
    let client = crate::airhouse::connect_and_ensure(workspace_id).await?;

    let mut sql = String::from(
        "INSERT INTO oxy_cam_events \
         (event_id, ts, camera_id, event_type, zone_id, line_id, track_id, \
          dwell_seconds, confidence, frame_uri) VALUES ",
    );
    for (i, e) in events.iter().enumerate() {
        if i > 0 {
            sql.push_str(", ");
        }
        let _ = write!(
            sql,
            "({}, {}, {}, {}, {}, {}, {}, {}, {}, {})",
            sql_str(&e.event_id.to_string()),
            sql_ts(e.ts),
            sql_str(&e.camera_id.to_string()),
            sql_str(&e.event_type),
            sql_opt_str(e.zone_id.as_deref()),
            sql_opt_str(e.line_id.as_deref()),
            sql_str(&e.track_id),
            sql_opt_f32(e.dwell_seconds),
            sql_opt_f32(e.confidence),
            sql_opt_str(e.frame_uri.as_deref()),
        );
    }
    client.simple_query(&sql).await.map_err(|e| {
        ServiceError::Airhouse(crate::airhouse::AirhouseError::Insert(e.to_string()))
    })?;
    Ok(IngestResult {
        accepted: events.len(),
    })
}

pub async fn write_camera_health(
    workspace_id: Uuid,
    rows: Vec<CameraHealthPayload>,
) -> ServiceResult<IngestResult> {
    if rows.is_empty() {
        return Ok(IngestResult { accepted: 0 });
    }
    let client = crate::airhouse::connect_and_ensure(workspace_id).await?;

    let mut sql = String::from(
        "INSERT INTO oxy_cam_camera_health \
         (camera_id, ts, fps, bitrate_kbps, last_frame_at, decoder_errors, \
          reconnect_count) VALUES ",
    );
    for (i, r) in rows.iter().enumerate() {
        if i > 0 {
            sql.push_str(", ");
        }
        let _ = write!(
            sql,
            "({}, {}, {}, {}, {}, {}, {})",
            sql_str(&r.camera_id.to_string()),
            sql_ts(r.ts),
            sql_opt_f32(r.fps),
            sql_opt_f32(r.bitrate_kbps),
            sql_opt_ts(r.last_frame_at),
            sql_opt_i32(Some(r.decoder_errors)),
            sql_opt_i32(Some(r.reconnect_count)),
        );
    }
    client.simple_query(&sql).await.map_err(|e| {
        ServiceError::Airhouse(crate::airhouse::AirhouseError::Insert(e.to_string()))
    })?;
    Ok(IngestResult {
        accepted: rows.len(),
    })
}

pub async fn write_box_health(
    workspace_id: Uuid,
    rows: Vec<BoxHealthPayload>,
) -> ServiceResult<IngestResult> {
    if rows.is_empty() {
        return Ok(IngestResult { accepted: 0 });
    }
    let client = crate::airhouse::connect_and_ensure(workspace_id).await?;

    let mut sql = String::from(
        "INSERT INTO oxy_cam_box_health \
         (box_id, ts, cpu_pct, gpu_pct, mem_pct, temp_c, uptime_s, image_tag, \
          containers_running) VALUES ",
    );
    for (i, r) in rows.iter().enumerate() {
        if i > 0 {
            sql.push_str(", ");
        }
        let _ = write!(
            sql,
            "({}, {}, {}, {}, {}, {}, {}, {}, {})",
            sql_str(&r.box_id.to_string()),
            sql_ts(r.ts),
            sql_opt_f32(r.cpu_pct),
            sql_opt_f32(r.gpu_pct),
            sql_opt_f32(r.mem_pct),
            sql_opt_f32(r.temp_c),
            sql_opt_i64(r.uptime_s),
            sql_opt_str(r.image_tag.as_deref()),
            sql_opt_i32(r.containers_running),
        );
    }
    client.simple_query(&sql).await.map_err(|e| {
        ServiceError::Airhouse(crate::airhouse::AirhouseError::Insert(e.to_string()))
    })?;
    Ok(IngestResult {
        accepted: rows.len(),
    })
}
