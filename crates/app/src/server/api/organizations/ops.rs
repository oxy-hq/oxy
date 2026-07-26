use std::collections::HashMap;

use axum::http::StatusCode;
use chrono::Utc;
use email_address::EmailAddress;
use entity::org_members;
use entity::org_members::OrgRole;
use entity::organizations;
use entity::prelude::*;
use entity::workspaces;
use handlebars::Handlebars;
use once_cell::sync::Lazy;
use sea_orm::sea_query::Expr;
use sea_orm::{ColumnTrait, EntityTrait, FromQueryResult, QueryFilter, QuerySelect};
use uuid::Uuid;

use oxy_shared::errors::OxyError;

use super::dto::OrgResponse;

pub(super) fn org_response(org: &organizations::Model, role: &OrgRole) -> OrgResponse {
    OrgResponse {
        id: org.id,
        name: org.name.clone(),
        slug: org.slug.clone(),
        role: role.as_str().to_string(),
        created_at: org.created_at.to_rfc3339(),
        updated_at: org.updated_at.to_rfc3339(),
        workspace_count: None,
        member_count: None,
    }
}

/// Canonical slug generation. The frontend has a preview slugify for UX, but
/// this function is the source of truth — the stored slug always comes from here.
pub(crate) fn slugify_name(name: &str) -> String {
    slugify::slugify(name, "", "-", None)
}

/// Slugs that collide with top-level frontend routes. An org with one of these
/// slugs would be unreachable at `/{slug}` because React Router resolves the
/// static path first. Keep in sync with the routes declared in
/// `web-app/src/App.tsx` and any future top-level additions.
const RESERVED_ORG_SLUGS: &[&str] = &[
    "admin",
    "api",
    "app",
    "apps",
    "auth",
    "github",
    "invite",
    "invitations",
    "login",
    "logout",
    "onboarding",
    "orgs",
    "settings",
    "signin",
    "signup",
    "static",
    "workspace",
    "workspaces",
];

pub(crate) fn is_reserved_slug(slug: &str) -> bool {
    RESERVED_ORG_SLUGS.contains(&slug)
}

/// Trims, lowercases, and validates an invitee email. Returns the normalized
/// form on success or `BAD_REQUEST` for empty / malformed input. Centralizing
/// here keeps the single-invite and bulk-invite paths in sync.
pub(crate) fn normalize_invite_email(raw: &str) -> Result<String, StatusCode> {
    let normalized = raw.trim().to_lowercase();
    if normalized.is_empty() || !EmailAddress::is_valid(&normalized) {
        return Err(StatusCode::BAD_REQUEST);
    }
    Ok(normalized)
}

#[derive(FromQueryResult)]
struct OrgCountRow {
    org_id: Uuid,
    count: i64,
}

pub(super) async fn count_members_per_org(
    db: &sea_orm::DatabaseConnection,
    org_ids: &[Uuid],
) -> Result<HashMap<Uuid, i64>, StatusCode> {
    let rows: Vec<OrgCountRow> = OrgMembers::find()
        .filter(org_members::Column::OrgId.is_in(org_ids.to_vec()))
        .select_only()
        .column(org_members::Column::OrgId)
        .column_as(Expr::col(org_members::Column::Id).count(), "count")
        .group_by(org_members::Column::OrgId)
        .into_model::<OrgCountRow>()
        .all(db)
        .await
        .map_err(|e| {
            tracing::error!("Failed to count members per org: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    Ok(rows.into_iter().map(|r| (r.org_id, r.count)).collect())
}

pub(super) async fn count_workspaces_per_org(
    db: &sea_orm::DatabaseConnection,
    org_ids: &[Uuid],
) -> Result<HashMap<Uuid, i64>, StatusCode> {
    let rows: Vec<OrgCountRow> = Workspaces::find()
        .filter(workspaces::Column::OrgId.is_in(org_ids.to_vec()))
        .select_only()
        .column(workspaces::Column::OrgId)
        .column_as(Expr::col(workspaces::Column::Id).count(), "count")
        .group_by(workspaces::Column::OrgId)
        .into_model::<OrgCountRow>()
        .all(db)
        .await
        .map_err(|e| {
            tracing::error!("Failed to count workspaces per org: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    Ok(rows.into_iter().map(|r| (r.org_id, r.count)).collect())
}

static INVITATION_TEMPLATE: Lazy<Handlebars<'static>> = Lazy::new(|| {
    let mut hbs = Handlebars::new();
    hbs.register_template_string("invitation", include_str!("../../../emails/invitation.hbs"))
        .expect("invitation.hbs is valid");
    hbs
});

/// Sends an invitation email. Piggybacks on the magic-link SES config for the
/// sender identity; if magic-link auth is not configured, this is a no-op so
/// the admin can still share the copy-able invite token manually.
pub(crate) async fn send_invitation_email(
    to_email: &str,
    token: &str,
    base_url: &str,
    inviter_name: &str,
    inviter_email: &str,
    org_name: &str,
) -> Result<(), OxyError> {
    use crate::emails::{
        EmailMessage, EmailProvider, local_test::LocalTestEmailProvider, ses::SesEmailProvider,
    };

    let magic_link_config = oxy::config::oxy::get_oxy_config()
        .ok()
        .and_then(|c| c.authentication)
        .and_then(|a| a.magic_link);

    let Some(config) = magic_link_config else {
        tracing::warn!(
            "Invitation email not sent — magic-link email config missing. Token can still be shared via the Pending Invitations UI."
        );
        return Ok(());
    };

    let invite_url = format!("{base_url}/invite/{token}");
    let subject = format!("You've been invited to {org_name} on Oxygen");
    let text_body = format!(
        "{inviter_name} ({inviter_email}) has invited you to join {org_name} on Oxygen.\n\nAccept the invitation:\n{invite_url}\n\nThis invitation expires in 7 days. If you weren't expecting this, you can safely ignore this email."
    );
    let message = EmailMessage {
        subject,
        html_body: build_invitation_email_html(
            &invite_url,
            to_email,
            inviter_name,
            inviter_email,
            org_name,
        )?,
        text_body,
    };

    if std::env::var("MAGIC_LINK_LOCAL_TEST").is_ok() {
        LocalTestEmailProvider
            .send(&config.from_email, to_email, message)
            .await
    } else {
        SesEmailProvider::new(config.aws_region.as_deref())
            .await
            .send(&config.from_email, to_email, message)
            .await
    }
}

fn build_invitation_email_html(
    invite_url: &str,
    to_email: &str,
    inviter_name: &str,
    inviter_email: &str,
    org_name: &str,
) -> Result<String, OxyError> {
    let data = serde_json::json!({
        "invite_url": invite_url,
        "to_email": to_email,
        "invited_by_name": inviter_name,
        "invited_by_email": inviter_email,
        "org_name": org_name,
        "year": Utc::now().format("%Y").to_string(),
    });

    INVITATION_TEMPLATE
        .render("invitation", &data)
        .map_err(|e| OxyError::RuntimeError(format!("Failed to render invitation template: {e}")))
}
