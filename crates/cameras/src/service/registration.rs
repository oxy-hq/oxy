//! Edge box registration.
//!
//! Operator pre-provisions an edge_box row for a specific site and gets
//! a one-shot bearer token to hand to the installer. The installer's
//! Jetson / N100 uses that token for every subsequent `/control/*`
//! request.
//!
//! Token rotation: call [`rotate_token`] to issue a new bearer for the
//! same edge_box. The old one stays in the DB with `revoked_at` set so
//! the audit trail survives.

use sea_orm::{
    ActiveModelTrait, ActiveValue::NotSet, ColumnTrait, DatabaseConnection, EntityTrait,
    QueryFilter, Set, prelude::Uuid as SeaUuid,
};
use uuid::Uuid;

use crate::auth::token::{IssuedToken, issue};
use crate::entities::{edge_box_tokens, edge_boxes, sites};

use super::{ServiceError, ServiceResult};

/// Operator-supplied parameters for pre-registering an edge box.
#[derive(Debug, Clone)]
pub struct RegisterEdgeBoxInput {
    pub site_id: Uuid,
    pub hardware_model: String,
    /// e.g. `"jetson-orin-nx-16gb"` / `"n100-mini-pc"` / `"unifi-controller"`.
    /// Free-form for now; canonicalize later if we want a fixed taxonomy.
    pub image_tag: Option<String>,
    /// Cohort label for OTA rollouts: `"stable"` (default) or `"canary"`.
    pub cohort: Option<String>,
    /// Human-readable label for the issued token; appears in the
    /// `edge_box_tokens.description` column. Useful when an operator
    /// reissues for the same box.
    pub token_description: Option<String>,
}

/// Result of a successful registration. The plaintext token is returned
/// **once** here; it cannot be recovered afterward (only the hash is
/// stored).
#[derive(Debug)]
pub struct RegisterEdgeBoxOutput {
    pub edge_box_id: Uuid,
    pub token: IssuedToken,
}

/// Pre-register an edge box bound to a site. Caller is responsible for
/// authorizing the operator (Workspace.Admin or higher) **and** the
/// route layer is responsible for resolving `workspace_id` from the
/// authenticated session (URL `Path<WorkspacePath>` behind
/// `workspace_middleware`).
///
/// This service fn enforces the cross-aggregate boundary: the site must
/// exist *and* be in `workspace_id`. Otherwise we'd let a caller with
/// access to workspace A pre-register an edge box bound to workspace
/// B's site (a real cross-workspace write leak — `sites.workspace_id`
/// is a loose Uuid column with no FK, so the DB doesn't catch this).
pub async fn register_edge_box(
    db: &DatabaseConnection,
    workspace_id: Uuid,
    input: RegisterEdgeBoxInput,
) -> ServiceResult<RegisterEdgeBoxOutput> {
    if input.hardware_model.trim().is_empty() {
        return Err(ServiceError::InvalidInput(
            "hardware_model is required".into(),
        ));
    }

    // Site must exist and belong to the caller's workspace.
    let site = sites::Entity::find_by_id::<SeaUuid>(input.site_id)
        .one(db)
        .await?
        .ok_or(ServiceError::NotFound)?;
    if site.workspace_id != workspace_id {
        return Err(ServiceError::Forbidden("site belongs to another workspace"));
    }

    let edge_box_id = Uuid::new_v4();
    let now = chrono::Utc::now();
    let cohort = input.cohort.unwrap_or_else(|| "stable".into());
    let image_tag = input.image_tag.unwrap_or_else(|| "unknown".into());

    let edge_box = edge_boxes::ActiveModel {
        id: Set(edge_box_id),
        site_id: Set(input.site_id),
        hardware_model: Set(input.hardware_model),
        image_tag: Set(image_tag),
        cohort: Set(cohort),
        tailscale_ip: NotSet,
        funnel_hostname: NotSet,
        bandwidth_5min_bytes: NotSet,
        bandwidth_reported_at: NotSet,
        target_image_tag: NotSet,
        current_image_tag: NotSet,
        held_until: NotSet,
        last_update_result: NotSet,
        last_update_at: NotSet,
        auth_mode: NotSet,
        edge_compatibility_json: NotSet,
        incompatible_reason: NotSet,
        status: Set("pending".into()),
        unifi_console_id: NotSet,
        unifi_public_ip: NotSet,
        unifi_rtsp_reachable: Set(false),
        registered_at: Set(now.into()),
        last_seen_at: NotSet,
        updated_at: Set(now.into()),
    };
    edge_box.insert(db).await?;

    let token = issue();
    let token_row = edge_box_tokens::ActiveModel {
        id: Set(Uuid::new_v4()),
        edge_box_id: Set(edge_box_id),
        token_hash: Set(token.hash.clone()),
        token_prefix: Set(token.prefix.clone()),
        description: Set(input.token_description),
        created_at: Set(now.into()),
        last_used_at: NotSet,
        revoked_at: NotSet,
    };
    token_row.insert(db).await?;

    // Camera-intent trigger: registering an edge box is an explicit
    // "this workspace runs camera infrastructure" signal. Eagerly
    // create the `oxy_cam_*` tables so the first event the box sends
    // lands fast. Soft-fail (logged) so a misconfigured Airhouse
    // doesn't break a Postgres registration — the lazy ensure on the
    // ingest path will retry next event.
    if let Err(e) = crate::airhouse::ensure_schema(workspace_id).await {
        tracing::warn!(
            workspace_id = %workspace_id,
            edge_box_id = %edge_box_id,
            error = %e,
            "ensure_schema failed during register_edge_box; lazy ensure will retry on first ingest"
        );
    }

    Ok(RegisterEdgeBoxOutput { edge_box_id, token })
}

/// Issue a new bearer for an existing edge box and revoke all prior
/// active tokens for the same box. Use after physical replacement of
/// the edge hardware or when a token is believed leaked.
pub async fn rotate_token(
    db: &DatabaseConnection,
    edge_box_id: Uuid,
    token_description: Option<String>,
) -> ServiceResult<IssuedToken> {
    let now = chrono::Utc::now();

    // Confirm the box exists.
    if edge_boxes::Entity::find_by_id::<SeaUuid>(edge_box_id)
        .one(db)
        .await?
        .is_none()
    {
        return Err(ServiceError::NotFound);
    }

    // Revoke all currently-active tokens for this box. We don't bother
    // batching this; in practice it's almost always exactly one row.
    edge_box_tokens::Entity::update_many()
        .col_expr(
            edge_box_tokens::Column::RevokedAt,
            sea_orm::sea_query::Expr::value(Some::<chrono::DateTime<chrono::Utc>>(now)),
        )
        .filter(edge_box_tokens::Column::EdgeBoxId.eq(edge_box_id))
        .filter(edge_box_tokens::Column::RevokedAt.is_null())
        .exec(db)
        .await?;

    let token = issue();
    let token_row = edge_box_tokens::ActiveModel {
        id: Set(Uuid::new_v4()),
        edge_box_id: Set(edge_box_id),
        token_hash: Set(token.hash.clone()),
        token_prefix: Set(token.prefix.clone()),
        description: Set(token_description),
        created_at: Set(now.into()),
        last_used_at: NotSet,
        revoked_at: NotSet,
    };
    token_row.insert(db).await?;
    Ok(token)
}

/// Revoke a single token by id. Idempotent — re-revoking a revoked
/// token returns `Ok(())` without changing `revoked_at`.
pub async fn revoke_token(db: &DatabaseConnection, token_id: Uuid) -> ServiceResult<()> {
    edge_box_tokens::Entity::update_many()
        .col_expr(
            edge_box_tokens::Column::RevokedAt,
            sea_orm::sea_query::Expr::value(Some::<chrono::DateTime<chrono::Utc>>(
                chrono::Utc::now(),
            )),
        )
        .filter(edge_box_tokens::Column::Id.eq(token_id))
        .filter(edge_box_tokens::Column::RevokedAt.is_null())
        .exec(db)
        .await?;
    Ok(())
}
