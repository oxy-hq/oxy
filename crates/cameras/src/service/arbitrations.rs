//! Operator-arbitrated verdicts on VLM ↔ YOLO disagreements (Phase 2
//! of "utilizing disagreements"). One row per `report_id`; re-submit
//! updates the existing verdict in place.
//!
//! ## Data model in one paragraph
//!
//! Compliance reports live in Airhouse (raw, immutable). Arbitrations
//! are operator labels overlaid in Postgres. We don't FK the two —
//! they live in different databases — but the route layer enforces
//! workspace ownership on the report_id before any insert lands here.
//! The Airhouse `oxy_cam_compliance_reports.agreement_status` column
//! drives the review queue filter; this table records the operator's
//! verdict on disagreements they investigated.

use crate::entities::compliance_arbitrations;
use crate::service::{ServiceError, ServiceResult};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, QueryOrder,
    QuerySelect, Set,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Operator verdict on which pipeline was right. Free-text on the
/// column for forward-compat (Phase 4's per-class verdicts may grow
/// `mixed` or `partial`); this enum is the v0 surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArbitrationVerdict {
    /// The VLM verdict was correct; YOLO was wrong.
    VlmRight,
    /// YOLO's detections were correct; the VLM verdict was wrong.
    YoloRight,
    /// Neither pipeline was right on this event. The arbiter usually
    /// follows up with an explicit verdict via notes or a custom UI
    /// (Phase 4); v0 records the "neither" intent so the training
    /// data pipeline can sample these for closer human review.
    Neither,
}

impl ArbitrationVerdict {
    pub fn as_str(&self) -> &'static str {
        match self {
            ArbitrationVerdict::VlmRight => "vlm_right",
            ArbitrationVerdict::YoloRight => "yolo_right",
            ArbitrationVerdict::Neither => "neither",
        }
    }

    pub fn parse(s: &str) -> ServiceResult<Self> {
        match s {
            "vlm_right" => Ok(Self::VlmRight),
            "yolo_right" => Ok(Self::YoloRight),
            "neither" => Ok(Self::Neither),
            other => Err(ServiceError::InvalidInput(format!(
                "unknown arbitration verdict `{other}` — expected one of vlm_right/yolo_right/neither"
            ))),
        }
    }
}

/// Wire shape returned by both the write endpoint and the read paths.
/// Mirrors the entity 1:1 — the route layer doesn't need to massage
/// fields, the Audit tab grew tired of bespoke DTOs.
#[derive(Debug, Clone, Serialize)]
pub struct ArbitrationRow {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub report_id: String,
    pub arbiter_user_id: Option<Uuid>,
    pub verdict: String,
    pub notes: Option<String>,
    pub created_at: chrono::DateTime<chrono::FixedOffset>,
    pub updated_at: chrono::DateTime<chrono::FixedOffset>,
}

impl From<compliance_arbitrations::Model> for ArbitrationRow {
    fn from(m: compliance_arbitrations::Model) -> Self {
        Self {
            id: m.id,
            workspace_id: m.workspace_id,
            report_id: m.report_id,
            arbiter_user_id: m.arbiter_user_id,
            verdict: m.verdict,
            notes: m.notes,
            created_at: m.created_at,
            updated_at: m.updated_at,
        }
    }
}

/// Upsert an arbitration for `report_id`. Idempotent — re-submitting
/// with the same verdict bumps `updated_at` only, which the UI uses
/// as a "this arbitration was reviewed at" signal. Different verdict
/// = the operator changed their mind; the table records the current
/// belief, not the history.
pub async fn upsert(
    db: &DatabaseConnection,
    workspace_id: Uuid,
    report_id: String,
    verdict: ArbitrationVerdict,
    notes: Option<String>,
    arbiter_user_id: Option<Uuid>,
) -> ServiceResult<ArbitrationRow> {
    let trimmed_report = report_id.trim().to_string();
    if trimmed_report.is_empty() {
        return Err(ServiceError::InvalidInput("report_id is required".into()));
    }
    let notes_trimmed = notes
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    let now = chrono::Utc::now().fixed_offset();

    let existing = compliance_arbitrations::Entity::find()
        .filter(compliance_arbitrations::Column::ReportId.eq(&trimmed_report))
        .one(db)
        .await?;

    let row = if let Some(row) = existing {
        // Cross-workspace guard: an existing arbitration for this
        // report_id but in a different workspace means either a
        // collision in Airhouse report ids (shouldn't happen — UUIDs)
        // or a malicious caller passing someone else's id. Refuse.
        if row.workspace_id != workspace_id {
            return Err(ServiceError::Forbidden(
                "report belongs to another workspace",
            ));
        }
        let mut am: compliance_arbitrations::ActiveModel = row.into();
        am.verdict = Set(verdict.as_str().to_string());
        am.notes = Set(notes_trimmed);
        am.arbiter_user_id = Set(arbiter_user_id);
        am.updated_at = Set(now);
        am.update(db).await?
    } else {
        compliance_arbitrations::ActiveModel {
            id: Set(Uuid::new_v4()),
            workspace_id: Set(workspace_id),
            report_id: Set(trimmed_report),
            arbiter_user_id: Set(arbiter_user_id),
            verdict: Set(verdict.as_str().to_string()),
            notes: Set(notes_trimmed),
            created_at: Set(now),
            updated_at: Set(now),
        }
        .insert(db)
        .await?
    };
    Ok(row.into())
}

/// Look up the arbitration for a specific report, if any.
pub async fn get_for_report(
    db: &DatabaseConnection,
    workspace_id: Uuid,
    report_id: &str,
) -> ServiceResult<Option<ArbitrationRow>> {
    let row = compliance_arbitrations::Entity::find()
        .filter(compliance_arbitrations::Column::WorkspaceId.eq(workspace_id))
        .filter(compliance_arbitrations::Column::ReportId.eq(report_id))
        .one(db)
        .await?;
    Ok(row.map(Into::into))
}

/// List recent arbitrations across the workspace, newest first. Used
/// by the dashboard "labeled events" feed + the future training-data
/// export.
pub async fn list_for_workspace(
    db: &DatabaseConnection,
    workspace_id: Uuid,
    limit: u32,
) -> ServiceResult<Vec<ArbitrationRow>> {
    let rows = compliance_arbitrations::Entity::find()
        .filter(compliance_arbitrations::Column::WorkspaceId.eq(workspace_id))
        .order_by_desc(compliance_arbitrations::Column::UpdatedAt)
        .limit(limit.clamp(1, 500) as u64)
        .all(db)
        .await?;
    Ok(rows.into_iter().map(Into::into).collect())
}

/// Delete an arbitration. No-op when none exists. The route layer
/// gates this behind workspace ownership so we don't pass the
/// workspace_id through the delete path here.
pub async fn delete_for_report(
    db: &DatabaseConnection,
    workspace_id: Uuid,
    report_id: &str,
) -> ServiceResult<()> {
    compliance_arbitrations::Entity::delete_many()
        .filter(compliance_arbitrations::Column::WorkspaceId.eq(workspace_id))
        .filter(compliance_arbitrations::Column::ReportId.eq(report_id))
        .exec(db)
        .await?;
    Ok(())
}
