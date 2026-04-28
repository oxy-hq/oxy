use crate::integrations::slack::client::SlackClient;
use crate::integrations::slack::services::installations::InstallationsService;
use crate::server::api::middlewares::org_context::OrgContextExtractor;
use axum::http::StatusCode;
use entity::org_members::OrgRole;

pub async fn disconnect(
    OrgContextExtractor(ctx): OrgContextExtractor,
) -> Result<StatusCode, (StatusCode, String)> {
    if !matches!(ctx.membership.role, OrgRole::Owner | OrgRole::Admin) {
        return Err((StatusCode::FORBIDDEN, "admin role required".into()));
    }
    let inst = InstallationsService::find_active_by_org(ctx.org.id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let inst = match inst {
        Some(i) => i,
        None => return Ok(StatusCode::NO_CONTENT),
    };
    let bot_token = InstallationsService::decrypt_bot_token(&inst)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    // Best-effort revoke — if Slack returns an error we still mark locally revoked.
    let _ = SlackClient::new().auth_revoke(&bot_token).await;
    InstallationsService::revoke(inst.id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(StatusCode::NO_CONTENT)
}
