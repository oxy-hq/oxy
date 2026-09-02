//! Shared security gates for custom-app HTTP endpoints.
//!
//! Today only `POST /api/projects/:id/query` exists, but the
//! customer-apps platform is growing to three more endpoints
//! (`/semantic-query`, `/agents/.../asks`, `/automations/.../runs`).
//! Each shares the same prelude: cookie / API-key auth → Origin
//! allowlist → user lookup → DB connection → workspace lookup →
//! org-membership check. Inlining that in every handler would mean
//! the second endpoint silently drifts on which gates apply.
//!
//! Extract it once. Every handler then becomes:
//!
//! ```ignore
//! pub async fn run_semantic_query(
//!     Path(project_id): Path<Uuid>,
//!     headers: HeaderMap,
//!     body: Bytes,
//! ) -> Response {
//!     let ctx = match check_custom_app_gates(&headers, project_id).await {
//!         Ok(c) => c,
//!         Err(r) => return r,
//!     };
//!     let req: SemanticQueryRequest = match parse_versioned_body(&body) {
//!         Ok(r) => r,
//!         Err(r) => return r,
//!     };
//!     // … domain-specific work
//! }
//! ```
//!
//! Auth still runs BEFORE body parse. Bodies are taken as raw bytes
//! by the handler and parsed via [`parse_versioned_body`] only after
//! the gates pass; without that order an unauthenticated 422 from
//! axum's `Json<T>` extractor would leak that the route exists and
//! what body fields are expected.

use axum::Json;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use entity::prelude::Workspaces;
use oxy::adapters::secrets::SecretsManager;
use oxy::adapters::workspace::{builder::WorkspaceBuilder, effective_workspace_path};
use oxy::database::client::establish_connection;
use oxy_auth::authenticator::Authenticator;
use oxy_auth::built_in::BuiltInAuthenticator;
use oxy_auth::user::UserService;
use sea_orm::{DatabaseConnection, EntityTrait};
use serde::Deserialize;
use tracing::{error, warn};
use uuid::Uuid;

use oxy_auth::types::AuthenticatedUser;

use crate::agentic_wiring::OxyProjectContext;
use crate::server::api::custom_apps_auth::is_org_member;
use crate::server::router::is_allowed_origin;
use crate::server::service::secret_manager::SecretManagerService;
use oxy_server_authz as authz;

/// Resolved context produced by [`check_custom_app_gates`]. The
/// handler unpacks whichever fields it needs; the struct is the
/// minimum set of state every custom-app endpoint needs.
pub struct CustomAppContext {
    /// Authenticated user (id, email, name, status). Derived from the
    /// subject of the project-scoped app token during the gate chain.
    pub user: AuthenticatedUser,
    /// Project UUID (= workspace id) the request is scoped to.
    pub project_id: Uuid,
    /// Workspace row resolved from `project_id`.
    pub workspace: entity::workspaces::Model,
    /// Organization that owns the workspace. Guaranteed non-`None`
    /// here — the gate rejects workspaces without an `org_id`.
    pub org_id: Uuid,
    /// Live DB connection — reused by handlers that need additional
    /// queries (e.g. agent lookup) so we don't pay for a second
    /// `establish_connection()` round-trip per request.
    pub db: DatabaseConnection,
}

impl CustomAppContext {
    /// Build the per-request `OxyProjectContext` derived from the
    /// resolved workspace. Each call constructs a fresh
    /// `WorkspaceManager` keyed off the workspace's effective path;
    /// callers wanting to reuse should hold the result.
    pub async fn build_project_context(&self) -> Result<OxyProjectContext, Response> {
        build_project_context(&self.workspace, self.user.id, self.project_id).await
    }
}

/// Common error-body shape returned by the gate chain. Keep in sync
/// with `query.rs`'s `ApiErr` — both deliberately use the same
/// `{ message, code? }` JSON envelope so callers can render errors
/// uniformly across endpoints.
#[derive(serde::Serialize)]
struct GateErr {
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    code: Option<&'static str>,
}

fn err(status: StatusCode, msg: impl Into<String>) -> Response {
    (
        status,
        Json(GateErr {
            message: msg.into(),
            code: None,
        }),
    )
        .into_response()
}

fn err_with_code(status: StatusCode, msg: impl Into<String>, code: &'static str) -> Response {
    (
        status,
        Json(GateErr {
            message: msg.into(),
            code: Some(code),
        }),
    )
        .into_response()
}

/// Run the gate chain. Returns the resolved context on success or a
/// fully-formed HTTP error response on any failure. Handlers should
/// pattern-match the `Err` arm and early-return.
///
/// Context-based access: when the app is served by oxy (in-workspace or
/// admin preview) the request carries the session cookie; external/dev
/// callers send a bearer token. Either way the caller must be able to
/// access the project (org member or oxy app-admin). Auth runs first so
/// unauthenticated callers can't probe routes via axum extractor errors.
pub async fn check_custom_app_gates(
    headers: &HeaderMap,
    project_id: Uuid,
) -> Result<CustomAppContext, Response> {
    // ── 1. Authenticate ───────────────────────────────────────────────
    // Session cookie (served by oxy) or a bearer token (external/dev).
    let identity = match BuiltInAuthenticator::new().authenticate(headers).await {
        Ok(id) => id,
        Err(_) => return Err(err(StatusCode::UNAUTHORIZED, "authentication required")),
    };

    // ── 2. Origin allowlist ───────────────────────────────────────────
    if !is_allowed_origin(headers) {
        warn!("custom-app gate: request rejected — origin not in allowlist");
        return Err(err(StatusCode::FORBIDDEN, "origin not allowed"));
    }

    // ── 3. User lookup ────────────────────────────────────────────────
    let user = match UserService::find_user_by_identity(&identity).await {
        Ok(Some(u)) => u,
        Ok(None) => return Err(err(StatusCode::UNAUTHORIZED, "user not found")),
        Err(e) => {
            error!("user lookup failed: {e}");
            return Err(err(StatusCode::INTERNAL_SERVER_ERROR, "user lookup failed"));
        }
    };

    // ── 4. DB connection ──────────────────────────────────────────────
    let db = match establish_connection().await {
        Ok(d) => d,
        Err(e) => {
            error!("DB connection failed: {e}");
            return Err(err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "database unavailable",
            ));
        }
    };

    // ── 5. Workspace lookup ──────────────────────────────────────────
    let workspace = match Workspaces::find_by_id(project_id).one(&db).await {
        Ok(Some(ws)) => ws,
        Ok(None) => return Err(err(StatusCode::NOT_FOUND, "project not found")),
        Err(e) => {
            error!("workspace lookup failed: {e}");
            return Err(err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "workspace lookup failed",
            ));
        }
    };

    // ── 5. Project access check ──────────────────────────────────────
    //
    // The access boundary: the caller must be a member of the project's
    // org, OR an oxy app-admin (so staff can use the admin-panel preview,
    // which is served by oxy and authenticated by the same cookie). Same-
    // origin bundles can issue XHRs at any project, so this membership
    // check — not the Origin allowlist — is what gates cross-project data.
    let org_id = match workspace.org_id {
        Some(id) => id,
        None => {
            return Err(err(
                StatusCode::FORBIDDEN,
                "workspace has no owning organization",
            ));
        }
    };
    // Access: a real org member, an Oxy app-admin, or a partner operator whose
    // ceiling grants `develop_apps` over the org's managing partner. NOTE — this is
    // the DATA PLANE. `develop_apps` is the
    // read-the-app's-data capability, deliberately DISTINCT from `manage_apps`
    // (app lifecycle: publish / unpublish). A partner that can toggle an app's
    // visibility cannot read its data unless its ceiling ALSO grants
    // `develop_apps`; the split is what keeps "manage the app" from silently
    // meaning "read the client's data". Granularity is deliberately org-wide (a
    // developing partner works across the client's workspaces) and the grant is
    // the ceiling itself (staff-set), so the data plane is not separately
    // consent-gated the way publishing is. See
    // `partner_authz::partner_grants_app_access` (called below), and the property is
    // pinned by `oxy_authz`'s `app_access_is_member_global_admin_or_develop_apps_partner`
    // — a manage_apps-only ceiling is denied AppAccess there.
    let allowed = match is_org_member(&db, user.id, org_id).await {
        Ok(true) => true,
        Ok(false) => {
            authz::globals::platform_standing(&db, user.email.as_deref().unwrap_or(""))
                .await
                .is_staff()
                || authz::partner_authz::partner_grants_app_access(
                    &db,
                    user.id,
                    user.email.as_deref().unwrap_or(""),
                    org_id,
                )
                .await
        }
        Err(e) => {
            error!("org membership check failed: {e}");
            return Err(err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "org membership check failed",
            ));
        }
    };
    // ENFORCE the unified AppAccess ring alongside the check above: the decision is
    // `allowed && unified`, so the model can only ever tighten it — a mis-modeled ring
    // can't hand a partner another tenant's data. Cannot be sampled: an authorization
    // decision has to be deterministic. The load is scoped (no
    // workspace-override query, which AppAccess never reads) so on this hot path it
    // costs ~one extra short-circuiting partner query over the is_org_member +
    // app-admin lookups the gate already does.
    // Unknown facts (a lookup errored) defer to the gate's own verdict rather than
    // denying — the conjunction only subtracts, so deferring can't open a hole, and a
    // blip must not 403 every legitimate app user.
    let allowed = match authz::loader::load_principal_facts_scoped(
        &db,
        user.id,
        user.email.as_deref().unwrap_or(""),
        false,
    )
    .await
    {
        Some(facts) => authz::enforce(
            "gate.custom_app",
            &facts,
            authz::Action::AppAccess,
            &authz::Resource::app(project_id, org_id),
            allowed,
        ),
        None => allowed,
    };

    if !allowed {
        return Err(err(
            StatusCode::FORBIDDEN,
            "not a member of the owning organization",
        ));
    }

    Ok(CustomAppContext {
        user,
        project_id,
        workspace,
        org_id,
        db,
    })
}

/// Parse a custom-app endpoint's JSON body, enforcing the
/// `"v": 1` versioning convention.
///
/// Versioning policy:
///   - If `v` is absent in the body, the request is treated as
///     v1 (backwards-compat for the existing `/query` SDK which
///     pre-dates the versioning convention).
///   - If `v` is present and non-`1`, the request is rejected with
///     400 + `code: "unsupported_version"`.
///
/// New SDK hooks always send `v: 1` explicitly so a future v2 client
/// hitting an old v1 server gets a clear error instead of silently
/// downgrading.
pub fn parse_versioned_body<T: for<'de> Deserialize<'de>>(body: &[u8]) -> Result<T, Response> {
    // Peek at `v` before deserializing the typed payload. We can't
    // declare `v` on every request struct because the existing
    // `QueryRequest` uses `deny_unknown_fields` and we don't want to
    // require it on shipped SDK clients.
    if let Ok(envelope) = serde_json::from_slice::<VersionEnvelope>(body)
        && let Some(v) = envelope.v
        && v != 1
    {
        return Err(err_with_code(
            StatusCode::BAD_REQUEST,
            format!("unsupported body version: {v} (only 1 is supported)"),
            "unsupported_version",
        ));
    }

    // Strip the `v` field before passing to the typed deserializer —
    // request types that use `deny_unknown_fields` would otherwise
    // reject it. Only do the strip when the field is present, so we
    // avoid an unconditional re-serialize for the common case.
    let payload_slice = strip_version_field(body);
    let payload: T = match payload_slice {
        Cow::Borrowed(b) => match serde_json::from_slice(b) {
            Ok(p) => p,
            Err(e) => {
                return Err(err(
                    StatusCode::BAD_REQUEST,
                    format!("invalid request body: {e}"),
                ));
            }
        },
        Cow::Owned(bytes) => match serde_json::from_slice(&bytes) {
            Ok(p) => p,
            Err(e) => {
                return Err(err(
                    StatusCode::BAD_REQUEST,
                    format!("invalid request body: {e}"),
                ));
            }
        },
    };
    Ok(payload)
}

#[derive(Deserialize)]
struct VersionEnvelope {
    #[serde(default)]
    v: Option<u32>,
}

/// If the body has a top-level `"v"` field, produce a copy of the
/// body with that field removed. Otherwise return the input slice
/// unchanged (zero-copy on the common path).
fn strip_version_field(body: &[u8]) -> Cow<'_, [u8]> {
    // Cheap reject: if there's no `"v"` substring at all, skip the
    // full JSON re-parse. False positives are fine (will fall
    // through to the parse-and-rewrite path); false negatives are
    // not possible (any v field MUST contain the literal `"v"`).
    if !body.windows(3).any(|w| w == b"\"v\"") {
        return Cow::Borrowed(body);
    }
    let Ok(mut value) = serde_json::from_slice::<serde_json::Value>(body) else {
        return Cow::Borrowed(body);
    };
    if let Some(obj) = value.as_object_mut()
        && obj.remove("v").is_some()
    {
        return Cow::Owned(serde_json::to_vec(&value).unwrap_or_else(|_| body.to_vec()));
    }
    Cow::Borrowed(body)
}

use std::borrow::Cow;

/// Build a `WorkspaceManager` + `OxyProjectContext` for the given
/// workspace. Pulled out so handlers that need the project context
/// (semantic compile, agent run) don't each re-implement the
/// builder dance. Returns a fully-formed HTTP error response on
/// failure so handlers can early-return.
pub(crate) async fn build_project_context(
    workspace: &entity::workspaces::Model,
    user_id: Uuid,
    project_id: Uuid,
) -> Result<OxyProjectContext, Response> {
    let branch_opt: Option<&str> = None;
    // The nil-UUID local workspace is a synthetic row with no `path` —
    // its directory is resolved from the server's cwd at request time
    // (config.yml walk-up), same as `resolve_workspace_path` and the
    // workspace middleware do. Only registered cloud workspaces carry a
    // DB path, so going straight to the row 500s every bundle endpoint
    // on a local-mode workspace.
    let effective_path = if project_id.is_nil() {
        match oxy::config::resolve_local_workspace_path() {
            Ok(p) => p,
            Err(e) => {
                error!("local workspace path resolution failed: {e}");
                return Err(err(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "could not resolve workspace path",
                ));
            }
        }
    } else {
        match effective_workspace_path(workspace, branch_opt).await {
            Ok(p) => p,
            Err(e) => {
                error!("effective_workspace_path failed: {e}");
                return Err(err(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "could not resolve workspace path",
                ));
            }
        }
    };
    // Compile boundary first, filesystem second — the same order the workspace
    // middleware and the Slack entry point use.
    //
    // This went straight to the working copy, which is the reason nine
    // `/api/projects/*` routes are pinned IdeOnly: the custom-app data plane
    // could only be served by the one pod holding a checkout. Reading the
    // promoted revision here is what lets them run on a replica, so a custom app
    // survives an ide restart instead of going down with it.
    //
    // `Origin` is recorded, so every boundary read downstream resolves at the
    // revision picked here rather than deriving its own.
    let revision_id =
        crate::server::api::compiled_reader::resolve_request_revision(project_id, branch_opt).await;
    let init = WorkspaceBuilder::new(project_id)
        .with_working_copy(&effective_path, revision_id, oxy::config::OnMissing::Empty)
        .await;
    let mut builder = match init {
        Ok(b) => b,
        Err(e) => {
            error!("workspace builder failed: {e}");
            return Err(err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "could not configure workspace",
            ));
        }
    };
    // Wire the project-scoped DB secrets manager (DB-first, env fallback) so
    // custom-app requests — including procedure-run automations — can read the
    // project's UI-managed secrets: the ClickHouse connection (CLICKHOUSE_HOST
    // etc.) and any http_request `secrets:`. Without this, WorkspaceBuilder falls
    // back to an env-only SecretsManager, so on the stateless serve fleet those
    // secrets resolve to nothing and e.g. the ClickHouse connector defaults to
    // localhost. Same DB-first+env-fallback wiring as workspace_context.rs /
    // local_context.rs, but deliberately fails CLOSED here (500) rather than their
    // warn-and-continue: this path exists specifically to reach the project's data
    // + secrets, so proceeding without them would silently reintroduce the env-only
    // bug. The Err arm is defensive — from_database_with_env_fallback is currently
    // infallible.
    match SecretsManager::from_database_with_env_fallback(SecretManagerService::new(project_id)) {
        Ok(secrets_manager) => builder = builder.with_secrets_manager(secrets_manager),
        Err(e) => {
            error!("project secrets manager failed: {e}");
            return Err(err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "could not configure workspace secrets",
            ));
        }
    }
    let workspace_manager = match builder.build().await {
        Ok(m) => m,
        Err(e) => {
            error!("workspace build failed: {e}");
            return Err(err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "could not build workspace manager",
            ));
        }
    };
    Ok(OxyProjectContext::new(workspace_manager).with_subject(user_id))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;
    use serde_json::json;

    #[derive(Debug, Deserialize, PartialEq)]
    #[serde(deny_unknown_fields)]
    struct DummyReq {
        topic: String,
    }

    #[test]
    fn parse_versioned_body_accepts_v_present() {
        let body = serde_json::to_vec(&json!({ "v": 1, "topic": "sales" })).unwrap();
        let req: DummyReq = parse_versioned_body(&body).expect("should parse");
        assert_eq!(req.topic, "sales");
    }

    #[test]
    fn parse_versioned_body_accepts_v_absent() {
        // Backwards-compat: existing /query SDK doesn't send `v`.
        let body = serde_json::to_vec(&json!({ "topic": "sales" })).unwrap();
        let req: DummyReq = parse_versioned_body(&body).expect("should parse");
        assert_eq!(req.topic, "sales");
    }

    #[test]
    fn parse_versioned_body_rejects_v_two() {
        let body = serde_json::to_vec(&json!({ "v": 2, "topic": "sales" })).unwrap();
        let res: Result<DummyReq, _> = parse_versioned_body(&body);
        assert!(res.is_err(), "v=2 should be rejected");
    }

    #[test]
    fn parse_versioned_body_rejects_invalid_json() {
        let res: Result<DummyReq, _> = parse_versioned_body(b"{not-json");
        assert!(res.is_err());
    }

    #[test]
    fn parse_versioned_body_rejects_missing_required_field() {
        let body = serde_json::to_vec(&json!({ "v": 1 })).unwrap();
        let res: Result<DummyReq, _> = parse_versioned_body(&body);
        assert!(
            res.is_err(),
            "missing `topic` should fail typed deserialize"
        );
    }

    #[test]
    fn parse_versioned_body_strips_v_for_deny_unknown_fields() {
        // `DummyReq` uses `deny_unknown_fields`. The presence of `v`
        // in the raw body must not trip that — the strip step
        // removes it before the typed parse.
        let body = serde_json::to_vec(&json!({ "v": 1, "topic": "sales" })).unwrap();
        let req: DummyReq = parse_versioned_body(&body).expect("v should be stripped");
        assert_eq!(req.topic, "sales");
    }

    #[test]
    fn strip_version_field_noop_when_absent() {
        let body = br#"{"topic":"sales"}"#;
        match strip_version_field(body) {
            Cow::Borrowed(slice) => assert_eq!(slice, body),
            Cow::Owned(_) => panic!("expected borrowed (zero-copy)"),
        }
    }

    #[test]
    fn strip_version_field_strips_when_present() {
        let body = br#"{"v":1,"topic":"sales"}"#;
        match strip_version_field(body) {
            Cow::Owned(out) => {
                let parsed: serde_json::Value = serde_json::from_slice(&out).unwrap();
                assert!(parsed.as_object().unwrap().get("v").is_none());
                assert_eq!(parsed["topic"], "sales");
            }
            Cow::Borrowed(_) => panic!("expected owned (v should have been stripped)"),
        }
    }
}
