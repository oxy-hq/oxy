//! Shared security gates for custom-app HTTP endpoints — the request-facing half.
//!
//! The customer-apps platform has several data endpoints (`/query`,
//! `/semantic-query`, `/agents/.../asks`, `/automations/.../runs`). Each shares
//! the same prelude: cookie / API-key auth → Origin allowlist → user lookup →
//! DB connection → workspace lookup → org-membership check. Inlining that in
//! every handler would mean the second endpoint silently drifts on which gates
//! apply, so it is extracted once. Every handler then becomes:
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
//!
//! ## Where the halves live
//!
//! The decisions — the user lookup, the workspace row, the shipped access
//! check, the ring's term, [`CustomAppContext`] — are in
//! `oxy_custom_apps::gate`, which has no HTTP types and reports a failure as a
//! `GateError`. Its module doc says what the chain remembers and what it reads
//! fresh on every call; read that before changing the order of the steps here.
//! What stays here needs `HeaderMap`, the authenticator, the Sentry surface
//! mark, `connect()` or `OxyProjectContext`: [`check_custom_app_gates`] (the
//! entry that runs the steps and maps a `GateError` onto the response),
//! [`parse_versioned_body`], and `build_project_context*`, which compose the
//! data plane's project context. The moved items are re-exported at the old
//! path.

use std::borrow::Cow;

use axum::Json;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use oxy::adapters::secrets::SecretsManager;
use oxy::adapters::workspace::{builder::WorkspaceBuilder, effective_workspace_path};
use oxy::database::client::{DbFailure, establish_connection};
use oxy_auth::authenticator::Authenticator;
use oxy_auth::built_in::BuiltInAuthenticator;
use oxy_custom_apps::gate::{GateError, gate_user, gate_verdict};
use sea_orm::DatabaseConnection;
use serde::Deserialize;
use tracing::{error, warn};
use uuid::Uuid;

use crate::agentic_wiring::OxyProjectContext;
use crate::server::api::middlewares::sentry_surface::mark_custom_app_surface;
use oxy::service::secret_manager::SecretManagerService;
use oxy_app_core::origin::is_allowed_origin;
use oxy_server_authz as authz;

pub use oxy_custom_apps::gate::{CustomAppContext, frontline_worker_with_app_grant};

/// What [`CustomAppContext`] needs from `oxy-app`: the project context built
/// from its workspace. It is a trait because the type lives below
/// `OxyProjectContext`; import it to call `ctx.build_project_context()`.
pub trait CustomAppContextExt {
    /// Build the per-request `OxyProjectContext` derived from the
    /// resolved workspace. Each call constructs a fresh
    /// `WorkspaceManager` keyed off the workspace's effective path;
    /// callers wanting to reuse should hold the result.
    ///
    /// On a pinned staging request the context resolves at the pin: the
    /// manager records the revision it was built at, so every
    /// compile-boundary read downstream (semantic scan, config) reads the
    /// branch's revision rather than the promoted one.
    fn build_project_context(
        &self,
    ) -> impl std::future::Future<Output = Result<OxyProjectContext, Response>> + Send;
}

impl CustomAppContextExt for CustomAppContext {
    async fn build_project_context(&self) -> Result<OxyProjectContext, Response> {
        build_caller_context(
            &self.workspace,
            self.user.id,
            self.project_id,
            self.staging_pin,
        )
        .await
    }
}

/// Common error-body shape returned by the gate chain. Keep in sync
/// with `query.rs`'s `ApiErr` — both deliberately use the same
/// `{ message, code? }` JSON envelope so callers can render errors
/// uniformly across endpoints.
///
/// Every 500 this chain raises off a `DbErr` carries a `code` from
/// [`DbFailure`]; see `oxy_custom_apps::gate::GateError` for why.
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

/// The response for a refusal or failure the decision half reports.
fn gate_error_response(e: GateError) -> Response {
    let status = StatusCode::from_u16(e.status()).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
    match e.code() {
        Some(code) => err_with_code(status, e.message(), code),
        None => err(status, e.message()),
    }
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
///
/// Warm, an org member pays two queries here — the workspace row and
/// `is_org_member`, both asked fresh on every request by design: the user
/// comes from the 60 s cache, the ring's term from the `(user_id,
/// project_id, org_id)` cache, and on a production request the staging pin is
/// answered from the headers alone. See `oxy_custom_apps::gate` for what is
/// remembered and why.
pub async fn check_custom_app_gates(
    headers: &HeaderMap,
    project_id: Uuid,
) -> Result<CustomAppContext, Response> {
    // ── 0. Sentry, barrier 2: this request is a custom app's ──────────
    //
    // Entering this gate IS the identity. Every route behind it is a
    // custom-app data-plane route and the web app calls none of them, so
    // the hub can be tagged here without guessing at a URL — which the URL
    // rules cannot do for this plane anyway: a bundle fetches same-origin
    // with an empty `apiBaseUrl`, and its origin is `app.oxygen-hq.com`
    // (canonical `/customer-apps/<org>/<app>/` URL) or an org subdomain,
    // neither of which carries a custom-app host label or a
    // `/customer-apps/**` path. See `sentry_surface::mark_custom_app_surface`.
    //
    // FIRST, before authentication, so the tag covers everything captured
    // for the rest of this request: the handler's own `error!` on a
    // warehouse failure (whose target is `…::projects::query`, which
    // barrier 1 does not drop), `OxyError::capture_to_sentry`, and a panic.
    mark_custom_app_surface();

    // ── 1. Authenticate ───────────────────────────────────────────────
    // Session cookie (served by oxy) or a bearer token (external/dev).
    //
    // With the credential: an API token's grants and standing flags decide the
    // access check below exactly as they do on `/api`.
    let (identity, credential) =
        match BuiltInAuthenticator::new(oxy_auth::token::SandboxAgent::Refuse)
            .authenticate_with_credential(headers)
            .await
        {
            Ok(authenticated) => authenticated,
            Err(_) => return Err(err(StatusCode::UNAUTHORIZED, "authentication required")),
        };

    // ── 2. Origin allowlist ───────────────────────────────────────────
    if !is_allowed_origin(headers) {
        warn!("custom-app gate: request rejected — origin not in allowlist");
        return Err(err(StatusCode::FORBIDDEN, "origin not allowed"));
    }

    // ── 3. User lookup, through the 60 s user cache ───────────────────
    // Attached AFTER the cache: the cached row is the user, never the
    // credential one request happened to arrive with.
    let user = gate_user(&identity)
        .await
        .map_err(gate_error_response)?
        .with_credential(credential);
    let caller = authz::Caller::from_user(&user);

    // ── 4. DB connection ──────────────────────────────────────────────
    let db = connect().await?;

    // ── 5. Workspace + access: the row and the gate fresh, the term cached ─
    let verdict = gate_verdict(&db, &caller, project_id)
        .await
        .map_err(gate_error_response)?;
    if !verdict.allowed {
        return Err(err(
            StatusCode::FORBIDDEN,
            "not a member of the owning organization",
        ));
    }

    // ── 6. Staging pin, per request ───────────────────────────────────
    // The staging build's semantic pin, for a request on the app's staging
    // host, decided by the same reach rule that serves that viewer the
    // staging bundle. After the access decision, so it can only narrow WHICH
    // revision an already authorized request reads, never whether it may
    // read. Never cached: it is a property of this request's host and
    // headers, and a production request answers it from them alone.
    let staging_pin = crate::server::api::custom_apps_staging_pin::staging_pin_for_data_request(
        &db, headers, &caller, project_id,
    )
    .await;

    Ok(CustomAppContext {
        user,
        project_id,
        workspace: verdict.workspace,
        org_id: verdict.org_id,
        db,
        staging_pin,
    })
}

/// Step 4: the pool checkout. No `DbErr` to classify — failing to build the
/// pool at all is unreachable by construction, so state the same code the
/// lookups behind it reach by inspection.
async fn connect() -> Result<DatabaseConnection, Response> {
    establish_connection().await.map_err(|e| {
        error!(
            code = DbFailure::Unreachable.code(),
            error = %e,
            "DB connection failed"
        );
        err_with_code(
            StatusCode::INTERNAL_SERVER_ERROR,
            "database unavailable",
            DbFailure::Unreachable.code(),
        )
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

/// The context a custom-app caller's work runs with: `user_id` as its subject,
/// no role, resolved at `staging_pin` when the request was a staging one.
///
/// One function for both places that build it — the request
/// ([`CustomAppContext::build_project_context`]) and a driver executing work
/// that request queued (`projects::automation_run::executor`). The driver has
/// no request, so identity reaches it as these arguments; going through the
/// same body is what keeps a queued run on the credentials the request would
/// have used. A context with **no** subject is not a lesser version of this
/// one: `airhouse_managed` mints a system Admin for it, where a subject with
/// no role mints the caller's Reader.
pub async fn build_caller_context(
    workspace: &entity::workspaces::Model,
    user_id: Uuid,
    project_id: Uuid,
    staging_pin: Option<Uuid>,
) -> Result<OxyProjectContext, Response> {
    crate::server::api::custom_apps_staging_pin::with_staging_pin(
        staging_pin,
        build_project_context(workspace, user_id, project_id),
    )
    .await
}

/// Build a `WorkspaceManager` + `OxyProjectContext` for the given
/// workspace. Pulled out so handlers that need the project context
/// (semantic compile, agent run) don't each re-implement the
/// builder dance. Returns a fully-formed HTTP error response on
/// failure so handlers can early-return.
pub async fn build_project_context(
    workspace: &entity::workspaces::Model,
    user_id: Uuid,
    project_id: Uuid,
) -> Result<OxyProjectContext, Response> {
    build_project_context_with_role(workspace, user_id, project_id, None).await
}

/// As [`build_project_context`], plus the caller's resolved workspace role.
///
/// Only `airhouse_managed` consults it, and only to pick the role of the
/// credential the broker mints (Owner→Admin, Admin→Writer, Member/Viewer→
/// Reader). Every other backend ignores it.
///
/// Why this exists as a separate entry point rather than a parameter on the
/// one above: passing `None` is the conservative default and every existing
/// caller wants it. A connector built without a role can only ever READ
/// airhouse, which is the right answer for a path that has not thought about
/// write authority — and the wrong one to acquire silently by a signature
/// change.
pub(crate) async fn build_project_context_with_role(
    workspace: &entity::workspaces::Model,
    user_id: Uuid,
    project_id: Uuid,
    role: Option<entity::workspace_members::WorkspaceRole>,
) -> Result<OxyProjectContext, Response> {
    build_project_context_inner(workspace, user_id, project_id, role, false).await
}

/// As [`build_project_context_with_role`], for a caller that read `workspace`
/// **during this request**: its `current_revision_id` picks the revision, so the
/// context build does not read the same row a second time.
///
/// A row remembered from an earlier request must not come through here, because
/// it would pin that request's revision. The function runner reads the row fresh
/// just before the call, which is why it can.
#[cfg(feature = "custom-app-functions")]
pub(crate) async fn build_project_context_with_fresh_workspace(
    workspace: &entity::workspaces::Model,
    user_id: Uuid,
    project_id: Uuid,
    role: Option<entity::workspace_members::WorkspaceRole>,
) -> Result<OxyProjectContext, Response> {
    build_project_context_inner(workspace, user_id, project_id, role, true).await
}

async fn build_project_context_inner(
    workspace: &entity::workspaces::Model,
    user_id: Uuid,
    project_id: Uuid,
    role: Option<entity::workspace_members::WorkspaceRole>,
    workspace_is_fresh: bool,
) -> Result<OxyProjectContext, Response> {
    let branch_opt: Option<&str> = None;
    // Compile boundary first, filesystem second — the same order the workspace
    // middleware and the Slack entry point use.
    //
    // This went straight to the working copy, which is the reason nine
    // `/api/projects/*` routes were pinned IdeOnly: the custom-app data plane
    // could only be served by the one pod holding a checkout. Reading the
    // promoted revision here is what lets them — and a function invocation —
    // run on a replica, so a custom app survives an ide restart instead of
    // going down with it.
    //
    // `Origin` is recorded, so every boundary read downstream resolves at the
    // revision picked here rather than deriving its own.
    let revision_id = if workspace_is_fresh {
        crate::server::api::compiled_reader::resolve_request_revision_for_workspace(
            workspace, branch_opt,
        )
        .await
    } else {
        crate::server::api::compiled_reader::resolve_request_revision(project_id, branch_opt).await
    };
    let effective_path = context_root(workspace, project_id, branch_opt).await?;
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
    let ctx = OxyProjectContext::new(workspace_manager).with_subject(user_id);
    // `with_role` is what decides whether a minted airhouse credential can
    // write. Absent it the broker defaults to Reader, which denies all DDL/DML
    // at the database — so a caller that means to write must say who it is.
    Ok(match role {
        Some(r) => ctx.with_role(r),
        None => ctx,
    })
}

/// The root the context's manager is built around.
///
/// On a pod that holds the working copy this is the directory the fallback
/// arms read. On one that does not (`serve`, `worker`) there is no directory:
/// the `workspaces.path` column is carried as a LABEL, because a compiled
/// `Config` comes back with `workspace_path` empty and the manager cannot be
/// built without one. With a promoted revision nothing is read through it: the
/// config, the semantic model and every artifact come from the boundary.
///
/// What still resolves against it and FAILS there, loudly, is a database that
/// is itself a file in the checkout — a local DuckDB with no S3 mirror, a
/// BigQuery key file (`DuckDB 'local': cannot resolve path '.db/'`). A
/// function invocation is kept off a replica for such a workspace
/// (`server::invocation_placement`); the `/api/projects/*` data plane is
/// not, and answers with that error.
///
/// So that pod does not go through `effective_workspace_path`. That resolver
/// is the working-copy door, and `workspace_fs_probe` counts every pass
/// through it on a diskless process as a leak: one per request, for a route
/// that reaches for no disk, which is how a real leak gets lost in the noise.
async fn context_root(
    workspace: &entity::workspaces::Model,
    project_id: Uuid,
    branch_opt: Option<&str>,
) -> Result<std::path::PathBuf, Response> {
    // The nil-UUID local workspace is a synthetic row with no `path` — its
    // directory is resolved from the server's cwd at request time (config.yml
    // walk-up), same as `resolve_workspace_path` and the workspace middleware
    // do. Only registered cloud workspaces carry a DB path, so going straight
    // to the row 500s every bundle endpoint on a local-mode workspace.
    let resolved = if project_id.is_nil() {
        oxy::config::resolve_local_workspace_path().map_err(|e| e.to_string())
    } else if !oxy::workspace_fs_probe::process_owns_workspace_files() {
        workspace
            .path
            .as_deref()
            .map(std::path::PathBuf::from)
            .ok_or_else(|| format!("workspace {} has no path configured", workspace.id))
    } else {
        effective_workspace_path(workspace, branch_opt)
            .await
            .map_err(|e| e.to_string())
    };
    resolved.map_err(|e| {
        error!("workspace path resolution failed: {e}");
        err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "could not resolve workspace path",
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;
    use serde_json::json;

    /// The decision half states a status, a message and a code as plain values;
    /// the response built from them must put exactly those on the wire, in the
    /// `{ message, code? }` envelope the SDK reads.
    #[tokio::test]
    async fn a_gate_error_becomes_the_response_it_always_was() {
        let unreachable = DbFailure::Unreachable.code();
        let cases = [
            (
                GateError::UserNotFound,
                StatusCode::UNAUTHORIZED,
                json!({ "message": "user not found" }),
            ),
            (
                GateError::UserLookupFailed { code: unreachable },
                StatusCode::INTERNAL_SERVER_ERROR,
                json!({ "message": "user lookup failed", "code": "database_unreachable" }),
            ),
            (
                GateError::ProjectNotFound,
                StatusCode::NOT_FOUND,
                json!({ "message": "project not found" }),
            ),
            (
                GateError::WorkspaceLookupFailed {
                    code: DbFailure::Query.code(),
                },
                StatusCode::INTERNAL_SERVER_ERROR,
                json!({ "message": "workspace lookup failed", "code": "database_query_failed" }),
            ),
            (
                GateError::NoOwningOrganization,
                StatusCode::FORBIDDEN,
                json!({ "message": "workspace has no owning organization" }),
            ),
            (
                GateError::MembershipCheckFailed { code: unreachable },
                StatusCode::INTERNAL_SERVER_ERROR,
                json!({ "message": "org membership check failed", "code": "database_unreachable" }),
            ),
        ];
        for (error, status, body) in cases {
            let response = gate_error_response(error);
            assert_eq!(response.status(), status, "{error:?}");
            let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
                .await
                .unwrap();
            let sent: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(sent, body, "{error:?}");
        }
    }

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
