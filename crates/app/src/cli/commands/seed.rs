//! `oxy seed` — seed the demo project so local dev / multi-instance demos
//! can compile + serve a workspace immediately after migrations.
//!
//! Seeds:
//! - Local org (nil-UUID, shared with local-mode for consistency)
//! - Demo workspace at a deterministic non-nil UUID, with `path` set to
//!   `./examples` (or `--workspace-path`). Non-nil so the enterprise
//!   `workspace_context` middleware accepts it — that guard 404s nil-UUID
//!   workspaces because nil is the local-mode-only convention.
//! - Every email in `OXY_GLOBAL_ADMINS` (or legacy `OXY_APP_ADMINS`) as
//!   Owner of Local org, so OAuth login (GitHub / Google) lands in a
//!   workspace already without the user clicking through a setup wizard.
//!
//! Idempotent — safe to re-run. Already-bound emails are skipped.

use std::path::PathBuf;

use airhouse::LOCAL_ORG_ID;
use chrono::Utc;
use entity::org_members::{self, OrgRole};
use entity::organizations;
use entity::prelude::{OrgMembers, Organizations, Workspaces};
use entity::workspaces::{self, WorkspaceStatus};
use oxy::database::client::establish_connection;
use oxy::theme::StyledText;
use oxy_auth::types::Identity;
use oxy_auth::user::UserService;
use oxy_shared::errors::OxyError;
use sea_orm::{
    ActiveModelTrait, ActiveValue, ColumnTrait, EntityTrait, IntoActiveModel, QueryFilter,
};
use uuid::Uuid;

/// Deterministic UUID for the demo workspace. UUID v5 over a fixed name in
/// the DNS namespace — same input → same output across machines, so a fresh
/// clone + `oxy seed` lands on the same workspace_id and any saved IDE
/// state / bookmarks stay valid.
fn demo_workspace_id() -> Uuid {
    Uuid::new_v5(&Uuid::NAMESPACE_DNS, b"demo.oxy.local")
}

/// Run the full demo seed. `workspace_path` defaults to `./examples`.
///
/// Also binds every email in `OXY_GLOBAL_ADMINS` (or legacy `OXY_APP_ADMINS`)
/// as Owner of the Local org so OAuth login (GitHub / Google) lands in a
/// workspace already. Skips emails already bound. Skips silently when
/// neither env var is set — the guest user still gets seeded so the
/// workspace is usable from a fresh login.
pub async fn seed_demo(workspace_path: Option<PathBuf>) -> Result<(), OxyError> {
    let resolved = resolve_workspace_path(workspace_path)?;
    let resolved_str = resolved.to_string_lossy().to_string();
    let workspace_id = demo_workspace_id();

    println!(
        "{} seeding demo (workspace_id={}, path={})",
        "🌱".info(),
        workspace_id,
        resolved_str
    );

    let conn = establish_connection().await?;
    ensure_local_org(&conn).await?;
    ensure_demo_workspace(&conn, workspace_id, &resolved_str).await?;

    println!(
        "{} workspace {} → {}",
        "✅".success(),
        workspace_id,
        resolved_str
    );

    bind_org_admin_emails(&conn).await?;

    println!();
    println!("Next:");
    println!("  cargo run -p oxy-app -- compile --workspace-path {resolved_str}");
    println!("  OXY_ROLE=ide cargo run -p oxy-app -- serve --enterprise");
    Ok(())
}

/// Ensure the Local organization exists at LOCAL_ORG_ID (nil). Shared with
/// local-mode's seed for consistency; FK constraints on airhouse_tenants
/// reference this id.
async fn ensure_local_org(conn: &sea_orm::DatabaseConnection) -> Result<(), OxyError> {
    if Organizations::find_by_id(LOCAL_ORG_ID)
        .one(conn)
        .await
        .map_err(|e| OxyError::DBError(format!("query Local org: {e}")))?
        .is_some()
    {
        return Ok(());
    }
    let now = Utc::now().fixed_offset();
    organizations::ActiveModel {
        id: ActiveValue::Set(LOCAL_ORG_ID),
        name: ActiveValue::Set("Local".to_string()),
        slug: ActiveValue::Set("local".to_string()),
        logo: ActiveValue::NotSet,
        logo_content_type: ActiveValue::NotSet,
        created_at: ActiveValue::Set(now),
        updated_at: ActiveValue::Set(now),
    }
    .insert(conn)
    .await
    .map_err(|e| OxyError::DBError(format!("insert Local org: {e}")))?;
    Ok(())
}

/// Ensure the demo workspace row exists at the given (non-nil) UUID,
/// pointing at the resolved path. On re-runs, patches the `path` if the
/// row exists but the path differs.
async fn ensure_demo_workspace(
    conn: &sea_orm::DatabaseConnection,
    workspace_id: Uuid,
    path: &str,
) -> Result<(), OxyError> {
    let existing = Workspaces::find_by_id(workspace_id)
        .one(conn)
        .await
        .map_err(|e| OxyError::DBError(format!("query demo workspace: {e}")))?;
    let now = Utc::now().fixed_offset();
    if let Some(row) = existing {
        if row.path.as_deref() == Some(path) {
            return Ok(());
        }
        let mut active = row.into_active_model();
        active.path = ActiveValue::Set(Some(path.to_string()));
        active.updated_at = ActiveValue::Set(now);
        active
            .update(conn)
            .await
            .map_err(|e| OxyError::DBError(format!("update demo workspace path: {e}")))?;
        return Ok(());
    }
    workspaces::ActiveModel {
        id: ActiveValue::Set(workspace_id),
        name: ActiveValue::Set("Demo".to_string()),
        git_namespace_id: ActiveValue::Set(None),
        git_remote_url: ActiveValue::Set(None),
        created_at: ActiveValue::Set(now),
        updated_at: ActiveValue::Set(now),
        path: ActiveValue::Set(Some(path.to_string())),
        last_opened_at: ActiveValue::Set(None),
        created_by: ActiveValue::Set(None),
        org_id: ActiveValue::Set(Some(LOCAL_ORG_ID)),
        status: ActiveValue::Set(WorkspaceStatus::Ready),
        error: ActiveValue::Set(None),
        monthly_vlm_budget_micros: ActiveValue::Set(None),
        current_revision_id: ActiveValue::Set(None),
    }
    .insert(conn)
    .await
    .map_err(|e| OxyError::DBError(format!("insert demo workspace: {e}")))?;
    Ok(())
}

/// Bind every email in OXY_GLOBAL_ADMINS (or legacy OXY_APP_ADMINS) as
/// Owner of the Local org. Idempotent — already-bound emails are skipped.
/// When neither env is set, returns Ok without action.
async fn bind_org_admin_emails(conn: &sea_orm::DatabaseConnection) -> Result<(), OxyError> {
    let raw = std::env::var("OXY_GLOBAL_ADMINS")
        .ok()
        .or_else(|| std::env::var("OXY_APP_ADMINS").ok())
        .unwrap_or_default();
    let parsed: Vec<String> = raw
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    if parsed.is_empty() {
        return Ok(());
    }
    println!(
        "{} binding {} email{} from OXY_GLOBAL_ADMINS as Owner of Local",
        "🔗".info(),
        parsed.len(),
        if parsed.len() == 1 { "" } else { "s" }
    );
    let mut bound = 0u32;
    let mut skipped = 0u32;
    for email in &parsed {
        let user = UserService::get_or_create_user(&Identity {
            email: email.clone(),
            name: Some(email.split('@').next().unwrap_or(email).to_string()),
            picture: None,
        })
        .await?;

        let existing = OrgMembers::find()
            .filter(org_members::Column::OrgId.eq(LOCAL_ORG_ID))
            .filter(org_members::Column::UserId.eq(user.id))
            .one(conn)
            .await
            .map_err(|e| OxyError::DBError(format!("query membership for {email}: {e}")))?;
        if existing.is_some() {
            skipped += 1;
            continue;
        }

        let now = Utc::now().fixed_offset();
        org_members::ActiveModel {
            id: ActiveValue::Set(Uuid::new_v4()),
            org_id: ActiveValue::Set(LOCAL_ORG_ID),
            user_id: ActiveValue::Set(user.id),
            role: ActiveValue::Set(OrgRole::Owner),
            created_at: ActiveValue::Set(now),
            updated_at: ActiveValue::Set(now),
        }
        .insert(conn)
        .await
        .map_err(|e| OxyError::DBError(format!("insert membership for {email}: {e}")))?;
        bound += 1;
    }
    println!(
        "  {} {bound} newly bound, {skipped} already Owner",
        "✅".success()
    );
    Ok(())
}

/// Drop the demo workspace row. Org + guest user are left in place since
/// other code paths (Airhouse provision, customer-apps demos) depend on
/// the nil-UUID org existing.
pub async fn clear_demo() -> Result<(), OxyError> {
    let conn = establish_connection().await?;
    let deleted = Workspaces::delete_by_id(demo_workspace_id())
        .exec(&conn)
        .await
        .map_err(|e| OxyError::DBError(format!("delete demo workspace: {e}")))?;
    println!(
        "{} cleared demo workspace ({} row{})",
        "🧹".info(),
        deleted.rows_affected,
        if deleted.rows_affected == 1 { "" } else { "s" }
    );
    Ok(())
}

fn resolve_workspace_path(path: Option<PathBuf>) -> Result<PathBuf, OxyError> {
    let raw = path.unwrap_or_else(|| PathBuf::from("./examples"));
    let absolute = std::path::absolute(&raw)
        .map_err(|e| OxyError::RuntimeError(format!("absolute path for {raw:?}: {e}")))?;
    if !absolute.exists() {
        return Err(OxyError::RuntimeError(format!(
            "workspace path does not exist: {}",
            absolute.display()
        )));
    }
    Ok(absolute)
}
