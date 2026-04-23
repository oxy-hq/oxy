//! Typed role-guard extractors. Each guard reads an upstream extension
//! populated by `org_middleware` / `workspace_middleware` /
//! `local_context_middleware` and fails extraction with `403 FORBIDDEN`
//! if the caller's role is insufficient.
//!
//! Using a guard type as a handler parameter is the check — it cannot be
//! forgotten. Handlers that need the full context can destructure:
//!
//! ```ignore
//! pub async fn delete_org(OrgOwner(ctx): OrgOwner) { ... }
//! pub async fn force_push(_: WorkspaceAdmin) { ... }
//! ```
//!
//! The guards assume the relevant middleware has already inserted the
//! extension; missing extensions yield `500` (a wiring bug, not a caller
//! error).

use axum::extract::FromRequestParts;
use axum::http::StatusCode;
use axum::http::request::Parts;
use entity::org_members::OrgRole;
use entity::workspace_members::WorkspaceRole;
use std::future::Future;

use super::org_context::OrgContext;
use super::workspace_context::EffectiveWorkspaceRole;

/// Caller is the Org Owner. Only Owners pass; Admins and Members are rejected.
#[derive(Debug)]
pub struct OrgOwner(pub OrgContext);

impl<S> FromRequestParts<S> for OrgOwner
where
    S: Send + Sync,
{
    type Rejection = StatusCode;

    fn from_request_parts(
        parts: &mut Parts,
        _state: &S,
    ) -> impl Future<Output = Result<Self, Self::Rejection>> + Send {
        let result = match parts.extensions.get::<OrgContext>().cloned() {
            None => Err(StatusCode::INTERNAL_SERVER_ERROR),
            Some(ctx) if ctx.membership.role == OrgRole::Owner => Ok(OrgOwner(ctx)),
            Some(_) => Err(StatusCode::FORBIDDEN),
        };
        async move { result }
    }
}

/// Caller is an Org Owner or Admin.
#[derive(Debug)]
pub struct OrgAdmin(pub OrgContext);

impl<S> FromRequestParts<S> for OrgAdmin
where
    S: Send + Sync,
{
    type Rejection = StatusCode;

    fn from_request_parts(
        parts: &mut Parts,
        _state: &S,
    ) -> impl Future<Output = Result<Self, Self::Rejection>> + Send {
        let result = match parts.extensions.get::<OrgContext>().cloned() {
            None => Err(StatusCode::INTERNAL_SERVER_ERROR),
            Some(ctx) if matches!(ctx.membership.role, OrgRole::Owner | OrgRole::Admin) => {
                Ok(OrgAdmin(ctx))
            }
            Some(_) => Err(StatusCode::FORBIDDEN),
        };
        async move { result }
    }
}

/// Caller's effective workspace role is Owner or Admin.
/// Use for destructive or settings-changing workspace actions.
#[derive(Debug)]
pub struct WorkspaceAdmin(pub WorkspaceRole);

impl<S> FromRequestParts<S> for WorkspaceAdmin
where
    S: Send + Sync,
{
    type Rejection = StatusCode;

    fn from_request_parts(
        parts: &mut Parts,
        _state: &S,
    ) -> impl Future<Output = Result<Self, Self::Rejection>> + Send {
        let result = match parts.extensions.get::<EffectiveWorkspaceRole>().cloned() {
            None => Err(StatusCode::INTERNAL_SERVER_ERROR),
            Some(EffectiveWorkspaceRole(role))
                if matches!(role, WorkspaceRole::Owner | WorkspaceRole::Admin) =>
            {
                Ok(WorkspaceAdmin(role))
            }
            Some(_) => Err(StatusCode::FORBIDDEN),
        };
        async move { result }
    }
}

/// Permission helper for workspace-rename: callers must be Org Owner/Admin
/// OR the user that originally created the workspace. The rule depends on
/// workspace data not present in request extensions, so it can't be a pure
/// typed extractor — keeping it here preserves the "all role checks live
/// in role_guards" convention so future readers can grep one place.
pub fn ensure_org_admin_or_workspace_creator(
    ctx: &OrgContext,
    workspace: &entity::workspaces::Model,
) -> Result<(), (StatusCode, String)> {
    let is_admin = matches!(ctx.membership.role, OrgRole::Owner | OrgRole::Admin);
    let is_creator = workspace.created_by == Some(ctx.membership.user_id);
    if !is_admin && !is_creator {
        return Err((
            StatusCode::FORBIDDEN,
            "Only admins or workspace creator can rename".to_string(),
        ));
    }
    Ok(())
}

/// Caller can edit workspace contents (Owner/Admin/Member). Rejects Viewer.
/// Use for contributor actions: commit, push, pull, file edit.
#[derive(Debug)]
pub struct WorkspaceEditor(pub WorkspaceRole);

impl<S> FromRequestParts<S> for WorkspaceEditor
where
    S: Send + Sync,
{
    type Rejection = StatusCode;

    fn from_request_parts(
        parts: &mut Parts,
        _state: &S,
    ) -> impl Future<Output = Result<Self, Self::Rejection>> + Send {
        let result = match parts.extensions.get::<EffectiveWorkspaceRole>().cloned() {
            None => Err(StatusCode::INTERNAL_SERVER_ERROR),
            Some(EffectiveWorkspaceRole(role)) if role > WorkspaceRole::Viewer => {
                Ok(WorkspaceEditor(role))
            }
            Some(_) => Err(StatusCode::FORBIDDEN),
        };
        async move { result }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::Request;

    fn make_parts_with<T: Clone + Send + Sync + 'static>(ext: T) -> Parts {
        let mut req = Request::builder().body(Body::empty()).unwrap();
        req.extensions_mut().insert(ext);
        req.into_parts().0
    }

    fn make_ctx(role: OrgRole) -> OrgContext {
        let now = chrono::Utc::now().fixed_offset();
        let org_id = uuid::Uuid::new_v4();
        OrgContext {
            org: entity::organizations::Model {
                id: org_id,
                name: "test".into(),
                slug: "test".into(),
                created_at: now,
                updated_at: now,
            },
            membership: entity::org_members::Model {
                id: uuid::Uuid::new_v4(),
                org_id,
                user_id: uuid::Uuid::new_v4(),
                role,
                created_at: now,
                updated_at: now,
            },
        }
    }

    fn make_workspace(created_by: Option<uuid::Uuid>) -> entity::workspaces::Model {
        let now = chrono::Utc::now().fixed_offset();
        entity::workspaces::Model {
            id: uuid::Uuid::new_v4(),
            name: "test-ws".into(),
            git_namespace_id: None,
            git_remote_url: None,
            created_at: now,
            updated_at: now,
            path: None,
            last_opened_at: None,
            created_by,
            org_id: None,
            status: entity::workspaces::WorkspaceStatus::Ready,
            error: None,
        }
    }

    #[tokio::test]
    async fn org_owner_admits_owner() {
        let mut parts = make_parts_with(make_ctx(OrgRole::Owner));
        assert!(OrgOwner::from_request_parts(&mut parts, &()).await.is_ok());
    }

    #[tokio::test]
    async fn org_owner_rejects_admin() {
        let mut parts = make_parts_with(make_ctx(OrgRole::Admin));
        let err = OrgOwner::from_request_parts(&mut parts, &())
            .await
            .unwrap_err();
        assert_eq!(err, StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn org_admin_admits_admin_and_owner() {
        let mut p1 = make_parts_with(make_ctx(OrgRole::Owner));
        assert!(OrgAdmin::from_request_parts(&mut p1, &()).await.is_ok());
        let mut p2 = make_parts_with(make_ctx(OrgRole::Admin));
        assert!(OrgAdmin::from_request_parts(&mut p2, &()).await.is_ok());
    }

    #[tokio::test]
    async fn org_admin_rejects_member() {
        let mut parts = make_parts_with(make_ctx(OrgRole::Member));
        let err = OrgAdmin::from_request_parts(&mut parts, &())
            .await
            .unwrap_err();
        assert_eq!(err, StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn workspace_admin_rejects_member() {
        let mut parts = make_parts_with(EffectiveWorkspaceRole(WorkspaceRole::Member));
        let err = WorkspaceAdmin::from_request_parts(&mut parts, &())
            .await
            .unwrap_err();
        assert_eq!(err, StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn workspace_admin_admits_admin() {
        let mut parts = make_parts_with(EffectiveWorkspaceRole(WorkspaceRole::Admin));
        assert!(
            WorkspaceAdmin::from_request_parts(&mut parts, &())
                .await
                .is_ok()
        );
    }

    #[tokio::test]
    async fn workspace_editor_rejects_viewer() {
        let mut parts = make_parts_with(EffectiveWorkspaceRole(WorkspaceRole::Viewer));
        let err = WorkspaceEditor::from_request_parts(&mut parts, &())
            .await
            .unwrap_err();
        assert_eq!(err, StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn workspace_editor_admits_member() {
        let mut parts = make_parts_with(EffectiveWorkspaceRole(WorkspaceRole::Member));
        assert!(
            WorkspaceEditor::from_request_parts(&mut parts, &())
                .await
                .is_ok()
        );
    }

    #[test]
    fn rename_helper_admits_org_owner_and_admin() {
        for role in [OrgRole::Owner, OrgRole::Admin] {
            let ctx = make_ctx(role);
            let ws = make_workspace(None);
            assert!(ensure_org_admin_or_workspace_creator(&ctx, &ws).is_ok());
        }
    }

    #[test]
    fn rename_helper_admits_member_when_creator() {
        let ctx = make_ctx(OrgRole::Member);
        let ws = make_workspace(Some(ctx.membership.user_id));
        assert!(ensure_org_admin_or_workspace_creator(&ctx, &ws).is_ok());
    }

    #[test]
    fn rename_helper_rejects_member_when_not_creator() {
        let ctx = make_ctx(OrgRole::Member);
        let ws = make_workspace(Some(uuid::Uuid::new_v4()));
        let err = ensure_org_admin_or_workspace_creator(&ctx, &ws).unwrap_err();
        assert_eq!(err.0, StatusCode::FORBIDDEN);
    }

    #[test]
    fn rename_helper_rejects_member_when_creator_unknown() {
        let ctx = make_ctx(OrgRole::Member);
        let ws = make_workspace(None);
        let err = ensure_org_admin_or_workspace_creator(&ctx, &ws).unwrap_err();
        assert_eq!(err.0, StatusCode::FORBIDDEN);
    }
}
