use crate::server::router::AppState;
use crate::server::service::retrieval::EnumIndexManager;
use crate::server::service::secret_manager::SecretManagerService;
use agentic_semantic::refresh_key_cache::RefreshKeyCache;
use axum::extract::{FromRequestParts, OptionalFromRequestParts, Path};
use axum::extract::{Query, State};
use axum::http::request::Parts;
use axum::{
    http::{Request, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
};
use chrono::Utc;
use entity::workspace_members::WorkspaceRole;
use oxy::adapters::runs::RunsManager;
use oxy::adapters::secrets::SecretsManager;
use oxy::adapters::workspace::builder::WorkspaceBuilder;
use oxy::adapters::workspace::effective_workspace_path;
use oxy::adapters::workspace::manager::WorkspaceManager;
use oxy::database::client::establish_connection;
use oxy_auth::extractor::AuthenticatedUserExtractor;
use sea_orm::EntityTrait;
use std::future::Future;
use uuid::Uuid;

#[derive(Clone)]
pub struct WorkspaceManagerExtractor(pub WorkspaceManager);

/// Why the workspace manager wasn't attached to this request. The two
/// flavors carry different operator + UX semantics:
///
/// - `NotAvailable`: the workspace path / config.yml is unreachable. The
///   user needs to fix their setup or the operator needs to debug a real
///   FS problem.
/// - `NeedsRecompile`: a serve replica refused to fall through to the
///   workspace FS (which it doesn't have) because the compile boundary
///   couldn't produce a usable config. The middleware has already
///   lazy-enqueued a Compile TaskSpec; the FE should retry shortly.
///   Triggered by either an absent / stale revision, or a compile-blob
///   deserialise failure (e.g. workspace `5ce5c011` with a schema-
///   drifted `DuckDBOptions`). Response carries the
///   `X-Oxy-Needs-Recompile` header so the FE can distinguish this from
///   a generic 503.
pub struct WorkspaceManagerMissing {
    pub needs_recompile: Option<Uuid>,
}

impl IntoResponse for WorkspaceManagerMissing {
    fn into_response(self) -> Response {
        match self.needs_recompile {
            Some(workspace_id) => {
                let mut response = (
                    StatusCode::SERVICE_UNAVAILABLE,
                    axum::Json(serde_json::json!({
                        "error": "Workspace has no current compiled revision on this replica. A compile has been enqueued; please retry shortly.",
                        "workspace_id": workspace_id,
                        "needs_recompile": true,
                    })),
                )
                    .into_response();
                // FE checks this header to distinguish "compile is stale,
                // retry me" from "the platform is unhappy". Header value is
                // the workspace_id so an interceptor can correlate.
                if let Ok(value) = axum::http::HeaderValue::from_str(&workspace_id.to_string()) {
                    response
                        .headers_mut()
                        .insert("x-oxy-needs-recompile", value);
                }
                response
            }
            None => (
                StatusCode::SERVICE_UNAVAILABLE,
                axum::Json(serde_json::json!({
                    "error": "Workspace configuration is not available. Check that the workspace path is accessible and config.yml is valid."
                })),
            )
                .into_response(),
        }
    }
}

/// Why a workspace request was refused, in a shape the caller can act on.
///
/// `Status` is the ordinary path — an opaque code, exactly as before.
/// `AssumeRequired` is the one denial that is a **policy boundary, not a fault**:
/// Oxy staff get the ability to *assume* a tenant role, never ambient read access
/// to tenant data (`api::admin::assume`; this is the silent override closed in
/// #2710). A bare 403 there reads as a bug and sends operators hunting through
/// releases, so it explains itself instead — the reason, the org to assume, and an
/// `x-oxy-assume-required` header the frontend keys off to offer the assume dialog
/// in place of a dead error.
pub enum WorkspaceAccessError {
    Status(StatusCode),
    AssumeRequired {
        workspace_id: Uuid,
        org_id: Uuid,
        /// Best-effort: the FE opens the assume dialog pre-scoped with it, and
        /// the message names the tenant instead of a UUID. `None` when the
        /// lookup failed — never a reason to turn a 403 into a 500.
        org_name: Option<String>,
    },
}

impl From<StatusCode> for WorkspaceAccessError {
    fn from(status: StatusCode) -> Self {
        Self::Status(status)
    }
}

impl IntoResponse for WorkspaceAccessError {
    fn into_response(self) -> Response {
        match self {
            Self::Status(status) => status.into_response(),
            Self::AssumeRequired {
                workspace_id,
                org_id,
                org_name,
            } => {
                let tenant = org_name
                    .clone()
                    .unwrap_or_else(|| "the owning org".to_string());
                let mut response = (
                    StatusCode::FORBIDDEN,
                    axum::Json(serde_json::json!({
                        "error": "workspace_access_denied",
                        "reason": "assume_role_required",
                        "message": format!(
                            "Oxy staff do not get ambient access to tenant data. Viewing this \
                             workspace requires an explicit assume-role session for {tenant} — \
                             time-boxed and audited."
                        ),
                        "workspace_id": workspace_id,
                        "org_id": org_id,
                        "org_name": org_name,
                    })),
                )
                    .into_response();
                // FE interceptor keys off this to swap the dead 403 for the
                // assume-role dialog, scoped to the org that must be assumed.
                if let Ok(value) = axum::http::HeaderValue::from_str(&org_id.to_string()) {
                    response
                        .headers_mut()
                        .insert("x-oxy-assume-required", value);
                }
                response
            }
        }
    }
}

/// Marker inserted by the workspace middleware when a serve replica
/// refuses to fall through to FS. The `WorkspaceManagerExtractor`
/// promotes it to a structured rejection so handlers don't have to
/// know about role-aware short-circuiting.
#[derive(Clone)]
struct NeedsRecompileMarker {
    workspace_id: Uuid,
}

impl<S> FromRequestParts<S> for WorkspaceManagerExtractor
where
    S: Send + Sync,
{
    type Rejection = WorkspaceManagerMissing;

    fn from_request_parts(
        parts: &mut Parts,
        _state: &S,
    ) -> impl Future<Output = Result<Self, Self::Rejection>> + Send {
        let result = parts
            .extensions
            .get::<WorkspaceManager>()
            .cloned()
            .map(WorkspaceManagerExtractor)
            .ok_or_else(|| {
                // A serve replica that short-circuited the FS fallback left
                // this marker behind; promote it to a structured rejection.
                let needs_recompile = parts
                    .extensions
                    .get::<NeedsRecompileMarker>()
                    .map(|m| m.workspace_id);
                WorkspaceManagerMissing { needs_recompile }
            });

        async move { result }
    }
}

/// `true` when the caller's workspace role came from the **global-operator
/// override** (a Global Owner / Global Admin who is not a real member of the org
/// gets a synthesized org-Owner membership) rather than a real membership.
///
/// Guards for tenant-sovereign decisions MUST reject this — otherwise an Oxy
/// operator can act as the customer. The Oxy-access lockdown switch is exactly
/// such a decision: staff must not be able to unlock themselves.
#[derive(Debug, Clone, Copy)]
pub struct WorkspaceGlobalOverride(pub bool);

impl<S> FromRequestParts<S> for WorkspaceGlobalOverride
where
    S: Send + Sync,
{
    type Rejection = StatusCode;

    fn from_request_parts(
        parts: &mut Parts,
        _state: &S,
    ) -> impl Future<Output = Result<Self, Self::Rejection>> + Send {
        // Absent (e.g. local mode) => not an override.
        let v = parts
            .extensions
            .get::<WorkspaceGlobalOverride>()
            .copied()
            .unwrap_or(WorkspaceGlobalOverride(false));
        async move { Ok(v) }
    }
}

/// The resolved workspace role for the current user.
#[derive(Clone, Debug)]
pub struct EffectiveWorkspaceRole(pub WorkspaceRole);

impl<S> FromRequestParts<S> for EffectiveWorkspaceRole
where
    S: Send + Sync,
{
    type Rejection = StatusCode;

    fn from_request_parts(
        parts: &mut Parts,
        _state: &S,
    ) -> impl Future<Output = Result<Self, Self::Rejection>> + Send {
        let result = parts
            .extensions
            .get::<EffectiveWorkspaceRole>()
            .cloned()
            .ok_or(StatusCode::INTERNAL_SERVER_ERROR);

        async move { result }
    }
}

/// Local mode doesn't attach an `EffectiveWorkspaceRole`, so handlers that work
/// in both modes (e.g. `list_apps`) read it as `Option<EffectiveWorkspaceRole>`.
impl<S> OptionalFromRequestParts<S> for EffectiveWorkspaceRole
where
    S: Send + Sync,
{
    type Rejection = std::convert::Infallible;

    fn from_request_parts(
        parts: &mut Parts,
        _state: &S,
    ) -> impl Future<Output = Result<Option<Self>, Self::Rejection>> + Send {
        let result = parts.extensions.get::<EffectiveWorkspaceRole>().cloned();
        async move { Ok(result) }
    }
}

/// Layer-1 preagg cache + renewal threshold, attached by the workspace
/// middleware so handlers can compile semantic queries through the same
/// preagg-aware path the background worker uses. Both fields are `None`
/// when no preagg worker is running (CLI, tests, internal API router).
#[derive(Clone, Default)]
pub struct PreaggCacheCtx {
    pub cache: Option<std::sync::Arc<std::sync::RwLock<RefreshKeyCache>>>,
    pub renewal_threshold_secs: Option<u64>,
}

impl<S> FromRequestParts<S> for PreaggCacheCtx
where
    S: Send + Sync,
{
    type Rejection = std::convert::Infallible;

    fn from_request_parts(
        parts: &mut Parts,
        _state: &S,
    ) -> impl Future<Output = Result<Self, Self::Rejection>> + Send {
        let ctx = parts
            .extensions
            .get::<PreaggCacheCtx>()
            .cloned()
            .unwrap_or_default();
        async move { Ok(ctx) }
    }
}

/// Per-workspace airlayer SemanticLayer cache. Attached by workspace_middleware
/// so handlers avoid re-parsing all `.view.yml`/`.topic.yml` files on every
/// request. Call `get_or_load` for a cached layer, `invalidate` after writes.
#[derive(Clone)]
pub struct SemanticLayerCacheCtx {
    pub cache: std::sync::Arc<crate::server::router::workspace_cache::SemanticLayerCache>,
    pub workspace_id: Uuid,
}

impl SemanticLayerCacheCtx {
    /// Returns a cached `Arc<airlayer::SemanticLayer>`, loading from disk on the
    /// first call per workspace (or after `invalidate`). The load is offloaded to
    /// a blocking thread so it does not stall the Tokio worker pool.
    pub async fn get_or_load(
        &self,
        scan_path: std::path::PathBuf,
    ) -> Result<std::sync::Arc<airlayer::SemanticLayer>, oxy_airlayer_compat::SemanticError> {
        if let Some(layer) = self.cache.lookup(self.workspace_id) {
            tracing::debug!(workspace_id = %self.workspace_id, "semantic layer cache hit");
            return Ok(layer);
        }
        tracing::info!(workspace_id = %self.workspace_id, path = ?scan_path, "semantic layer cache miss — loading from disk");
        let t0 = std::time::Instant::now();
        let layer = tokio::task::spawn_blocking(move || {
            oxy_airlayer_compat::load_layer_from_dir(&scan_path)
        })
        .await
        .map_err(|e| {
            oxy_airlayer_compat::SemanticError::Engine(format!("blocking task failed: {e}"))
        })??;
        tracing::info!(workspace_id = %self.workspace_id, elapsed_ms = t0.elapsed().as_millis(), "semantic layer loaded");
        let arc_layer = std::sync::Arc::new(layer);
        self.cache.insert(self.workspace_id, arc_layer.clone());
        Ok(arc_layer)
    }

    /// Evicts the cached layer so the next `get_or_load` reloads from disk.
    pub fn invalidate(&self) {
        self.cache.remove(self.workspace_id);
    }
}

impl<S> FromRequestParts<S> for SemanticLayerCacheCtx
where
    S: Send + Sync,
{
    type Rejection = StatusCode;

    fn from_request_parts(
        parts: &mut Parts,
        _state: &S,
    ) -> impl Future<Output = Result<Self, Self::Rejection>> + Send {
        let result = parts
            .extensions
            .get::<SemanticLayerCacheCtx>()
            .cloned()
            .ok_or(StatusCode::INTERNAL_SERVER_ERROR);
        async move { result }
    }
}

/// Cached compiled `SemanticEngine` for a workspace, injected by workspace middleware.
#[derive(Clone)]
pub struct SemanticEngineCacheCtx {
    pub cache: std::sync::Arc<crate::server::router::workspace_cache::SemanticEngineCache>,
    pub workspace_id: Uuid,
}

impl SemanticEngineCacheCtx {
    /// Returns a cached engine, building it from `layer` + `databases` on the first call.
    /// The engine is `Send` but not `Sync`; it lives behind a `Mutex` so callers must
    /// compile inside a `spawn_blocking` that locks, compiles, and immediately drops the guard.
    pub async fn get_or_build(
        &self,
        layer: std::sync::Arc<airlayer::SemanticLayer>,
        databases: Vec<airlayer::DatabaseConfig>,
    ) -> Option<std::sync::Arc<std::sync::Mutex<airlayer::SemanticEngine>>> {
        let workspace_id = self.workspace_id;
        self.cache
            .get_or_build(workspace_id, || async move {
                tracing::info!(%workspace_id, "semantic engine cache miss — building engine");
                let t0 = std::time::Instant::now();
                let result = tokio::task::spawn_blocking(move || {
                    let dialects =
                        airlayer::DatasourceDialectMap::from_config_databases(&databases);
                    airlayer::SemanticEngine::from_semantic_layer((*layer).clone(), dialects)
                        .ok()
                        .map(|engine| std::sync::Arc::new(std::sync::Mutex::new(engine)))
                })
                .await
                .ok()
                .flatten();
                if result.is_some() {
                    tracing::info!(%workspace_id, elapsed_ms = t0.elapsed().as_millis(), "semantic engine built and cached");
                } else {
                    tracing::warn!(%workspace_id, elapsed_ms = t0.elapsed().as_millis(), "semantic engine build failed");
                }
                result
            })
            .await
    }

    /// Evicts the cached engine (call alongside `SemanticLayerCacheCtx::invalidate`).
    pub fn invalidate(&self) {
        self.cache.remove(self.workspace_id);
    }
}

impl<S> FromRequestParts<S> for SemanticEngineCacheCtx
where
    S: Send + Sync,
{
    type Rejection = StatusCode;

    fn from_request_parts(
        parts: &mut Parts,
        _state: &S,
    ) -> impl Future<Output = Result<Self, Self::Rejection>> + Send {
        let result = parts
            .extensions
            .get::<SemanticEngineCacheCtx>()
            .cloned()
            .ok_or(StatusCode::INTERNAL_SERVER_ERROR);
        async move { result }
    }
}

/// The caller's org membership, inserted by workspace_middleware when the workspace belongs to an org.
#[derive(Clone)]
pub struct OrgMembershipExtractor(pub entity::org_members::Model);

impl<S> FromRequestParts<S> for OrgMembershipExtractor
where
    S: Send + Sync,
{
    type Rejection = StatusCode;

    fn from_request_parts(
        parts: &mut Parts,
        _state: &S,
    ) -> impl Future<Output = Result<Self, Self::Rejection>> + Send {
        let result = parts
            .extensions
            .get::<entity::org_members::Model>()
            .cloned()
            .map(OrgMembershipExtractor)
            .ok_or(StatusCode::FORBIDDEN);

        async move { result }
    }
}

#[derive(serde::Deserialize)]
pub struct WorkspacePath {
    pub workspace_id: Uuid,
}

#[derive(serde::Deserialize)]
pub struct BranchQuery {
    pub branch: Option<String>,
}

pub async fn workspace_middleware(
    State(app_state): State<AppState>,
    Path(WorkspacePath { workspace_id }): Path<WorkspacePath>,
    Query(query): Query<BranchQuery>,
    AuthenticatedUserExtractor(user): AuthenticatedUserExtractor,
    mut request: Request<axum::body::Body>,
    next: Next,
) -> Result<Response, WorkspaceAccessError> {
    if workspace_id == Uuid::nil() {
        tracing::warn!("Nil-UUID workspace path is not allowed");
        return Err(StatusCode::NOT_FOUND.into());
    }

    let branch_id = Uuid::nil();

    let agentic_db = app_state
        .agentic_state
        .as_ref()
        .map(|s| std::sync::Arc::new(s.db.clone()));

    // Resolve the one revision this request reads — ONCE — and pin it for the
    // whole downstream (config resolution below + every compiled reader the
    // handler calls). Without this, each reader re-resolves `current_revision_id`
    // independently and a promotion landing mid-request yields a torn read.
    let pinned_revision = crate::server::api::compiled_reader::resolve_request_revision(
        workspace_id,
        query.branch.as_deref(),
    )
    .await;

    crate::server::api::compiled_reader::with_pinned_revision(pinned_revision, async move {
        match authorize_workspace(workspace_id, user.id, &user.email, &mut request).await? {
            Some(workspace_row) => {
                try_attach_workspace_manager(
                    &workspace_row,
                    query.branch.as_deref(),
                    workspace_id,
                    branch_id,
                    user.id,
                    app_state.preagg_cache,
                    app_state.preagg_renewal_threshold_secs,
                    agentic_db,
                    app_state.semantic_layer_cache,
                    app_state.semantic_engine_cache,
                    &mut request,
                )
                .await?;
            }
            None => {
                tracing::warn!(
                    "No workspace path available for workspace {}, continuing without workspace manager",
                    workspace_id
                );
            }
        }

        // ── Fail-safe fallback (oxygen-internal#2528 follow-up) ────────────
        // If this serve replica has no compiled config for the workspace,
        // `try_attach_workspace_manager` set `NeedsRecompileMarker` (and
        // lazy-enqueued a compile). Rather than 503 the request, forward it to
        // the ide node — it owns the working copy and serves the workspace from
        // disk — so the workspace stays AVAILABLE (one in-cluster hop, degraded)
        // until the compile lands and the fleet can serve it from Postgres. This
        // turns "uncompiled / mid-compile = outage" into "= slower", which is the
        // whole point of an HA fleet. Loop-guarded; a no-op without
        // OXY_IDE_UPSTREAM (local / single-node), where the marker stays a 503.
        if request.extensions().get::<NeedsRecompileMarker>().is_some()
            && let Some(upstream) = crate::server::ide_proxy::ide_upstream()
            && !crate::server::ide_proxy::already_forwarded(&request)
        {
            tracing::info!(
                workspace_id = %workspace_id,
                "serve replica: no compiled config — forwarding to ide node (fail-safe) \
                 while the lazy compile lands"
            );
            // This middleware runs INSIDE the `/api/{workspace_id}` nest, where
            // axum has rewritten the request URI to the stripped remainder
            // (`/agents`, not `/api/{ws}/agents`). `forward_to_ide` proxies
            // `req.uri()` verbatim, so restore the original full URI first — else
            // the ide receives the bare `/agents` and serves the SPA fallback.
            // (The enforce_role self-proxy needs no such fix: it runs at the top
            // level where the URI is already the full path.)
            if let Some(original) = request
                .extensions()
                .get::<axum::extract::OriginalUri>()
                .map(|o| o.0.clone())
            {
                *request.uri_mut() = original;
            }
            return Ok(crate::server::ide_proxy::forward_to_ide(upstream, request).await);
        }

        Ok(next.run(request).await)
    })
    .await
}

/// Looks up the workspace, authorizes the caller, and inserts request extensions
/// (workspace row, effective role, org membership). Returns the workspace row
/// only when it has a configured path — i.e. when builder construction should follow.
///
/// `Ok(None)`: workspace has no configured path (builder construction skipped).
/// Fatal: DB unreachable (SERVICE_UNAVAILABLE), workspace not found (NOT_FOUND),
/// workspace with no `org_id` (FORBIDDEN), caller not in org (FORBIDDEN),
/// query errors (INTERNAL_SERVER_ERROR).
async fn authorize_workspace(
    workspace_id: Uuid,
    user_id: Uuid,
    user_email: &str,
    request: &mut Request<axum::body::Body>,
) -> Result<Option<entity::workspaces::Model>, WorkspaceAccessError> {
    use entity::prelude::Workspaces;

    let db = establish_connection().await.map_err(|e| {
        tracing::error!(
            "Could not connect to DB to resolve workspace {}: {}",
            workspace_id,
            e
        );
        StatusCode::SERVICE_UNAVAILABLE
    })?;

    let workspace_row = Workspaces::find_by_id(workspace_id)
        .one(&db)
        .await
        .map_err(|e| {
            tracing::error!("Failed to query workspace {}: {}", workspace_id, e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or_else(|| {
            tracing::warn!("Workspace {} not found in DB", workspace_id);
            StatusCode::NOT_FOUND
        })?;

    // Every workspace must belong to an org.
    let org_id = workspace_row.org_id.ok_or_else(|| {
        tracing::warn!("Workspace {} has no org_id — access denied", workspace_id);
        StatusCode::FORBIDDEN
    })?;

    request.extensions_mut().insert(workspace_row.clone());

    let (org_membership, effective_role, is_global_override) =
        resolve_effective_role(&db, workspace_id, org_id, user_id, user_email).await?;

    request
        .extensions_mut()
        .insert(EffectiveWorkspaceRole(effective_role));
    // Whether the Owner role above is REAL or a synthesized operator override.
    // Surfaces that distinction to guards that must never accept an Oxy operator
    // acting as the tenant (e.g. the Oxy-access lockdown switch).
    request
        .extensions_mut()
        .insert(WorkspaceGlobalOverride(is_global_override));
    request.extensions_mut().insert(org_membership);

    if workspace_row.path.is_none() {
        tracing::warn!(
            "Workspace {} has no path configured — continuing without workspace manager",
            workspace_id
        );
        return Ok(None);
    }

    Ok(Some(workspace_row))
}

async fn resolve_effective_role(
    db: &sea_orm::DatabaseConnection,
    workspace_id: Uuid,
    org_id: Uuid,
    user_id: Uuid,
    user_email: &str,
) -> Result<(entity::org_members::Model, WorkspaceRole, bool), WorkspaceAccessError> {
    use entity::org_members::Column as OrgMemberCol;
    use entity::prelude::{OrgMembers, WorkspaceMembers};
    use entity::workspace_members::Column as WsMemberCol;
    use sea_orm::{ColumnTrait, QueryFilter};

    let real_membership = OrgMembers::find()
        .filter(OrgMemberCol::OrgId.eq(org_id))
        .filter(OrgMemberCol::UserId.eq(user_id))
        .one(db)
        .await
        .map_err(|e| {
            tracing::error!(
                "Failed to query org membership (org={}, user={}, workspace={}): {}",
                org_id,
                user_id,
                workspace_id,
                e
            );
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    // Mirror `org_middleware`'s global-operator override: a Global Owner
    // (OXY_OWNER) or Global Admin (`app_admins`) who is not a real member of
    // the org gets a synthesized Owner membership so they can open a tenant's
    // workspace for support/triage (e.g. the admin "Open /home" on a workspace
    // that granted Oxy access). Without this, `/{workspace_id}/details` 403s
    // even though the parallel `/orgs/{id}/*` routes already allow operators
    // through — the inconsistency that bounced operators out of granted
    // workspaces.
    let mut is_global_override = false;
    let org_membership = match real_membership {
        Some(m) => m,
        None => {
            // Staff alone is not enough — an explicit, live assume-role session for
            // THIS org is required (see `api::admin::assume`). Without it the
            // operator is a plain non-member. This closes the silent override that
            // (among other things) let staff self-grant the old Oxy-access toggle.
            //
            // Same two populations as `org_context`: staff, or a partner acting as
            // an assigned client with `develop_apps`. Building a client's app means
            // opening their workspace, so this is exactly where the data-plane
            // capability has to be honoured.
            use crate::server::api::admin::assume;
            // Capability is independent of an active session — `may_act_as` reads
            // platform standing / partner scope only. Compute it FIRST so the
            // refusal can be shaped for the population that can actually act on
            // it: a plain non-member must stay opaque (naming the org to anyone
            // who guesses a workspace id is disclosure), while staff and partners
            // get the explanation and the way through.
            let authority = assume::may_act_as(db, user_id, user_email, org_id).await;
            let live = authority.is_some() && assume::is_session_live(db, user_id, org_id).await;

            if let (Some(authority), true) = (authority, live) {
                let now = Utc::now().into();
                let role = authority.org_role();
                let role_label = role.as_str();
                tracing::info!(
                    actor_email = %user_email,
                    org_id = %org_id,
                    workspace_id = %workspace_id,
                    ?authority,
                    role = %role_label,
                    "workspace_context: assume-role session active"
                );
                is_global_override = true;
                entity::org_members::Model {
                    id: Uuid::nil(),
                    org_id,
                    user_id,
                    role,
                    created_at: now,
                    updated_at: now,
                }
            } else if authority.is_none() {
                // A plain non-member. Nothing here is actionable for them and the
                // org's existence/name is not theirs to learn, so this stays the
                // opaque 403 it has always been.
                tracing::warn!(
                    actor = %user_id,
                    workspace_id = %workspace_id,
                    org_id = %org_id,
                    "workspace_context: denied — not a member of this org"
                );
                return Err(StatusCode::FORBIDDEN.into());
            } else {
                // Staff or an assigned partner, without a live session for this
                // org. They CAN act here, so the wall explains itself rather than
                // reading as a bug — see `WorkspaceAccessError::AssumeRequired`.
                tracing::warn!(
                    actor = %user_id,
                    workspace_id = %workspace_id,
                    org_id = %org_id,
                    "workspace_context: denied — has authority for this org but no \
                     live assume-role session"
                );
                // Name the tenant so the message and the assume dialog read
                // "Pokehouse", not a UUID. Denial-only path, so the extra read
                // costs nothing on the hot path — and a failed lookup just
                // degrades the wording.
                let org_name = entity::prelude::Organizations::find_by_id(org_id)
                    .one(db)
                    .await
                    .ok()
                    .flatten()
                    .map(|org| org.name);
                return Err(WorkspaceAccessError::AssumeRequired {
                    workspace_id,
                    org_id,
                    org_name,
                });
            }
        }
    };

    let ws_override = WorkspaceMembers::find()
        .filter(WsMemberCol::WorkspaceId.eq(workspace_id))
        .filter(WsMemberCol::UserId.eq(user_id))
        .one(db)
        .await
        .map_err(|e| {
            tracing::error!(
                "Failed to query workspace member override (workspace={}, user={}): {}",
                workspace_id,
                user_id,
                e
            );
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    let org_derived_role = match org_membership.role {
        entity::org_members::OrgRole::Owner => WorkspaceRole::Owner,
        entity::org_members::OrgRole::Admin => WorkspaceRole::Admin,
        entity::org_members::OrgRole::Member => WorkspaceRole::Member,
    };

    // Workspace-member override can only elevate, never downgrade below org-derived role.
    let effective_role = match ws_override {
        Some(ws_member) => std::cmp::max(org_derived_role, ws_member.role),
        None => org_derived_role,
    };

    Ok((org_membership, effective_role, is_global_override))
}

/// Enqueue a promoting compile for an uncompiled workspace, deduped against any
/// compile already queued/claimed for it. Best-effort — failures are logged,
/// never surfaced (the caller is on a request hot path).
/// How long to wait after a FAILED compile before the lazy self-heal will
/// auto-retry the same workspace. Without this, a persistently-broken workspace
/// (e.g. a config the round-trip gate rejects) becomes a recompile storm: every
/// failed compile clears the in-flight dedup, so the very next request
/// re-enqueues. Operators can still force an immediate compile via the admin
/// "Run compile now". A const, not an env flag — keep the surface small.
const LAZY_COMPILE_BACKOFF_SECS: i64 = 300;

pub(crate) async fn enqueue_lazy_compile(db: &sea_orm::DatabaseConnection, workspace_id: Uuid) {
    enqueue_compile_deduped(db, workspace_id, None, None, "lazy self-heal").await
}

/// Shared enqueue path for every *automatic* compile trigger: the lazy
/// self-heal above and the post-pull / post-restore triggers in
/// `server::compile_trigger`.
///
/// `git_sha` decides idempotency, and the two callers genuinely differ:
///
///   * `None` (self-heal) — the working tree is the identity. `oxy-compile`
///     mints a unique `local-<uuid>` and deliberately opts out of the
///     idempotency index, so every call produces a fresh revision.
///   * `Some(sha)` (content change) — an addressable commit. The
///     `(workspace_id, git_sha)` lookup then short-circuits a redundant
///     compile down to a cheap promote. Passing `None` here instead would
///     mint a new revision row on *every* pull, including no-op pulls.
pub(crate) async fn enqueue_compile_deduped(
    db: &sea_orm::DatabaseConnection,
    workspace_id: Uuid,
    git_sha: Option<String>,
    branch: Option<String>,
    reason: &str,
) {
    use sea_orm::{
        ColumnTrait, ConnectionTrait, DatabaseBackend, QueryFilter, Statement, TransactionTrait,
    };

    // Serialise concurrent self-heal enqueues for the SAME workspace across
    // every replica. A plain "SELECT in-flight? then INSERT" is a TOCTOU
    // race: N first-hits to an uncompiled/drifted workspace (across N serve
    // replicas, or a single page's parallel requests) all pass the SELECT
    // before any INSERT commits, and all enqueue a redundant compile + a
    // bloat row in agentic_runs. We close the window with a transaction-
    // scoped advisory lock keyed on the workspace, taken NON-blocking: only
    // the lock holder runs the dedup-check + inserts; a concurrent caller
    // that can't get the lock immediately knows someone else is already
    // doing it and just returns. Using `pg_try_advisory_xact_lock` (not the
    // blocking variant) matters precisely in the thundering-herd case this
    // guards — we don't want N request-hot-path connections parked on a lock
    // for the one workspace that's already unhealthy. The lock auto-releases
    // on commit/rollback; distinct workspaces hash to distinct keys.
    let txn = match db.begin().await {
        Ok(t) => t,
        Err(e) => {
            tracing::warn!(?e, %workspace_id, "lazy compile: begin txn failed");
            return;
        }
    };

    // Deterministic per-workspace lock key (low 63 bits of the UUID). A
    // collision with another advisory-lock user only adds harmless extra
    // serialisation; it can't cause incorrectness.
    let lock_key = (workspace_id.as_u128() as u64 & 0x7fff_ffff_ffff_ffff) as i64;
    let got_lock = txn
        .query_one(Statement::from_sql_and_values(
            DatabaseBackend::Postgres,
            "SELECT pg_try_advisory_xact_lock($1) AS locked",
            [lock_key.into()],
        ))
        .await;
    match got_lock {
        Ok(Some(row)) if row.try_get::<bool>("", "locked").unwrap_or(false) => {}
        Ok(_) => {
            // Lock held by another caller → it owns the check+insert. Skip.
            let _ = txn.rollback().await;
            return;
        }
        Err(e) => {
            tracing::warn!(?e, %workspace_id, "lazy compile: try-advisory-lock failed");
            let _ = txn.rollback().await;
            return;
        }
    }

    // Backoff: if a compile for this workspace FAILED recently, don't auto-retry
    // on every request. A persistently-broken workspace would otherwise become a
    // recompile storm (each failed compile clears the in-flight dedup below, so
    // the next request re-enqueues). Wait out the window. Checked INSIDE the lock
    // alongside the in-flight dedup so concurrent first-hits agree.
    let backoff_cutoff =
        (Utc::now() - chrono::Duration::seconds(LAZY_COMPILE_BACKOFF_SECS)).fixed_offset();
    let recently_failed = entity::revisions::Entity::find()
        .filter(entity::revisions::Column::WorkspaceId.eq(workspace_id))
        .filter(entity::revisions::Column::Kind.eq("main"))
        .filter(entity::revisions::Column::Status.eq("failed"))
        .filter(entity::revisions::Column::FinishedAt.gte(backoff_cutoff))
        .one(&txn)
        .await;
    if matches!(recently_failed, Ok(Some(_))) {
        tracing::debug!(
            %workspace_id,
            "lazy compile: backing off (a compile failed within the backoff window)"
        );
        let _ = txn.rollback().await;
        return;
    }

    // Re-check for an in-flight compile INSIDE the lock.
    let already = txn
        .query_one(Statement::from_sql_and_values(
            DatabaseBackend::Postgres,
            "SELECT 1 FROM agentic_task_queue \
             WHERE queue_status IN ('queued', 'claimed') \
               AND spec->>'type' = 'compile' \
               AND spec->>'workspace_id' = $1 \
             LIMIT 1",
            [workspace_id.to_string().into()],
        ))
        .await;
    if matches!(already, Ok(Some(_))) {
        let _ = txn.rollback().await;
        return;
    }

    let task_id = Uuid::new_v4().to_string();
    // `agentic_task_queue.run_id` FKs to `agentic_runs.id`, so the run row must
    // exist before the task insert (mirrors `api::compile`). Both run on the
    // same txn so the advisory lock covers them.
    if let Err(e) = agentic_runtime::crud::insert_run(
        &txn,
        &task_id,
        &format!("compile main ({reason})"),
        None,
        "compile",
        Some(serde_json::json!({
            "workspace_id": workspace_id,
            "lazy": true,
            "reason": reason,
            "git_sha": git_sha,
        })),
        workspace_id,
    )
    .await
    {
        tracing::warn!(?e, %workspace_id, %reason, "auto compile insert_run failed");
        let _ = txn.rollback().await;
        return;
    }
    let spec = agentic_core::delegation::TaskSpec::Compile {
        workspace_id,
        git_sha,
        branch,
        promote: true,
        kind: Some("main".to_string()),
        owner_user_id: None,
    };
    if let Err(e) = agentic_runtime::crud::enqueue_task(
        &txn,
        &task_id,
        &task_id,
        None,
        &spec,
        None,
        agentic_runtime::orchestrator::crud::queue::TaskScope::Global,
    )
    .await
    {
        tracing::warn!(?e, %workspace_id, %reason, "auto compile enqueue failed");
        let _ = txn.rollback().await;
        return;
    }

    match txn.commit().await {
        Ok(_) => tracing::info!(%workspace_id, %reason, "enqueued auto compile (deduped)"),
        Err(e) => tracing::warn!(?e, %workspace_id, %reason, "auto compile commit failed"),
    }
}

/// Best-effort: builds the `WorkspaceManager` (with secrets, runs, intent classifier)
/// and inserts it into request extensions. The only fatal outcome is an invalid
/// branch query parameter, which yields BAD_REQUEST.
async fn try_attach_workspace_manager(
    workspace_row: &entity::workspaces::Model,
    branch_name: Option<&str>,
    workspace_id: Uuid,
    branch_id: Uuid,
    user_id: Uuid,
    preagg_cache: Option<std::sync::Arc<std::sync::RwLock<RefreshKeyCache>>>,
    preagg_renewal_threshold_secs: Option<u64>,
    db: Option<std::sync::Arc<sea_orm::DatabaseConnection>>,
    semantic_layer_cache: std::sync::Arc<
        crate::server::router::workspace_cache::SemanticLayerCache,
    >,
    semantic_engine_cache: std::sync::Arc<
        crate::server::router::workspace_cache::SemanticEngineCache,
    >,
    request: &mut Request<axum::body::Body>,
) -> Result<(), StatusCode> {
    // Branch name is validated inside `effective_workspace_path`. The helper
    // rejects ".." / leading "-" / non-allowed chars via OxyError::RuntimeError —
    // we map that to 400 before the string reaches any shell-out downstream.
    let effective_path = effective_workspace_path(workspace_row, branch_name)
        .await
        .map_err(|e| {
            tracing::warn!(
                "Invalid branch or missing path for workspace {}: {}",
                workspace_id,
                e
            );
            StatusCode::BAD_REQUEST
        })?;

    // Record worktree access for the lifecycle reaper (ide-local; see
    // server::worktree_registry). Only worktree paths are tracked — the main
    // working copy (default branch) resolves to the workspace root, which is
    // never reaped.
    if effective_path
        .components()
        .any(|c| c.as_os_str().to_str() == Some(oxy_git::cli::worktree::WORKTREES_DIR))
    {
        crate::server::worktree_registry::registry().touch(&effective_path);
    }

    // Compile-boundary fast path: when the workspace has a promoted revision,
    // hydrate the workspace `Config` from `workspace_compiled_configs` instead
    // of re-parsing `config.yml` from disk on every request. This is the
    // largest single FS hit on the customer hot path — every chat / thread /
    // data app / automation request reads config.yml today.
    //
    // Thread the active branch through so the IDE on a feature branch sees its
    // working-copy `config.yml` edits via FS, matching the branch-aware
    // contract every other compiled reader honours. On any miss (no promoted
    // revision, non-default branch, deserialise fails) we fall through to FS.
    let compiled_config: Option<oxy::config::model::Config> =
        match crate::server::api::compiled_reader::resolve_workspace_config(
            workspace_id,
            branch_name,
        )
        .await
        {
            Ok(Some(json)) => {
                match serde_json::from_value::<oxy::config::model::Config>(json) {
                    Ok(mut cfg) => {
                        // `workspace_path` is `#[serde(skip)]` on Config so we
                        // populate it manually — downstream resolvers depend on it.
                        cfg.workspace_path = effective_path.clone();
                        tracing::debug!(
                            workspace_id = %workspace_id,
                            "workspace_context: config.yml served from compile boundary"
                        );
                        Some(cfg)
                    }
                    Err(e) => {
                        tracing::warn!(
                            workspace_id = %workspace_id,
                            error = ?e,
                            "compiled config deserialise failed; falling through to FS"
                        );
                        None
                    }
                }
            }
            Ok(None) => None,
            Err(e) => {
                tracing::warn!(
                    workspace_id = %workspace_id,
                    error = ?e,
                    "compiled config lookup failed; falling through to FS"
                );
                None
            }
        };

    // ── Stateless-fleet short-circuit ──────────────────────────────────────
    // A serve replica has no workspace working copy (emptyDir OXY_STATE_DIR,
    // no PVC). If the compile boundary couldn't produce a config (no
    // current_revision_id, schema-drift deserialise failure, DB hiccup),
    // we must NOT fall through to the FS path below — the FS read would
    // either return stale data from a different workspace or, more likely,
    // 500 with "workspace data directory not found". Either is worse than
    // a structured 503 that tells the FE to retry after the compile lands.
    //
    // We still lazy-enqueue a Compile TaskSpec on the way out so the next
    // request can succeed without operator action. The
    // `WorkspaceManagerExtractor` promotes `NeedsRecompileMarker` to a 503
    // with `X-Oxy-Needs-Recompile: <workspace_id>` so the FE can render a
    // proper "retrying compile" toast instead of a generic platform error —
    // UNLESS `OXY_IDE_UPSTREAM` is set (the serve fleet), in which case
    // `workspace_middleware` intercepts this marker and forwards the request to
    // the ide node (which serves from disk) instead of 503ing. The 503 is the
    // local / single-node fallback when there's no ide upstream to forward to.
    //
    // Regression context: oxy-hq/oxygen-internal#1619.
    if compiled_config.is_none()
        && crate::server::role_manifest::current_process_role()
            == crate::server::role_manifest::Role::Serve
    {
        // Lazy self-heal: enqueue a (deduped) compile whenever the boundary
        // couldn't produce a usable config for a compilable workspace —
        // NOT only when `current_revision_id` is null. A workspace that HAS
        // a revision whose compiled config won't deserialise (a schema
        // drift, e.g. a stale `DuckDBOptions` shape) also lands here, and it
        // needs a recompile just as much as a never-compiled one. Gating on
        // `current_revision_id.is_none()` left drifted workspaces 503ing
        // forever with no recovery. `path.is_some()` is the real
        // precondition (a pathless workspace can't be compiled).
        if workspace_row.path.is_some()
            && let Some(agentic_db) = db.as_ref()
        {
            enqueue_lazy_compile(agentic_db, workspace_id).await;
        }
        request
            .extensions_mut()
            .insert(NeedsRecompileMarker { workspace_id });
        tracing::warn!(
            workspace_id = %workspace_id,
            "serve replica: compile boundary missed; refusing FS fallback (NeedsRecompile)"
        );
        return Ok(());
    }

    let builder_init = if let Some(cfg) = compiled_config {
        WorkspaceBuilder::new(workspace_id)
            .with_workspace_path_and_compiled_config(&effective_path, cfg)
    } else {
        WorkspaceBuilder::new(workspace_id)
            .with_workspace_path_and_fallback_config(&effective_path)
            .await
    };
    let mut builder = match builder_init {
        Ok(b) => b,
        Err(e) => {
            tracing::warn!(
                "Failed to set workspace path in workspace builder for workspace {}: {}, continuing without workspace manager",
                workspace_id,
                e
            );
            // Lazy self-heal for a workspace that has never been compiled and
            // whose FS config build also failed. NOTE: this path is only
            // reached by Ide/All/Worker — a serve replica already
            // short-circuited above (the `Role::Serve` block) with the WIDER
            // `path.is_some()` gate that also covers schema drift. This
            // narrower `current_revision_id.is_none()` gate is correct HERE
            // (these roles have a working copy, so a drifted-but-promoted
            // workspace still serves from the FS) but is deliberately
            // different from the serve-mode gate above — don't "align" one
            // without re-checking the other.
            if workspace_row.current_revision_id.is_none()
                && workspace_row.path.is_some()
                && let Some(agentic_db) = db.as_ref()
            {
                enqueue_lazy_compile(agentic_db, workspace_id).await;
            }
            return Ok(());
        }
    };

    // DB-first with env fallback so DB secrets hot-reload and override env vars
    // without a server restart.
    match SecretsManager::from_database_with_env_fallback(SecretManagerService::new(workspace_id)) {
        Ok(secrets_manager) => builder = builder.with_secrets_manager(secrets_manager),
        Err(_) => tracing::warn!(
            "Failed to create secrets manager for workspace {}, continuing without it",
            workspace_id
        ),
    }

    match RunsManager::default(workspace_id, branch_id).await {
        Ok(runs_manager) => builder = builder.with_runs_manager(runs_manager),
        Err(e) => tracing::warn!(
            "Failed to create runs manager for workspace {}: {}, continuing without it",
            workspace_id,
            e
        ),
    }

    builder = builder.try_with_intent_classifier().await;

    let workspace_manager = match builder.build().await {
        Ok(m) => m,
        Err(e) => {
            tracing::warn!(
                "Failed to build workspace manager for workspace {}: {}, continuing without it",
                workspace_id,
                e
            );
            return Ok(());
        }
    };

    match EnumIndexManager::init_from_config(workspace_manager.config_manager.clone()).await {
        Ok(_) => tracing::debug!(
            "Enum index initialized successfully for workspace {}",
            workspace_id
        ),
        Err(e) => tracing::debug!(
            "Enum index initialization skipped for workspace {}: {}",
            workspace_id,
            e
        ),
    }

    let mut ctx = crate::agentic_wiring::OxyProjectContext::new(workspace_manager.clone())
        .with_subject(user_id);
    // The effective role was inserted into extensions by `authorize_workspace`
    // a few lines up; thread it through so the airhouse_managed builder can
    // mint with the user's mapped airhouse role.
    if let Some(EffectiveWorkspaceRole(role)) = request.extensions().get::<EffectiveWorkspaceRole>()
    {
        ctx = ctx.with_role(role.clone());
    }
    // Share the same cache Arc with the background preagg worker so Layer 1
    // (per-query freshness) and Layer 2 (background rebuild) use the same state.
    if let Some(cache) = preagg_cache.clone() {
        ctx = ctx.with_preagg_cache(cache);
    }
    if let Some(secs) = preagg_renewal_threshold_secs {
        ctx = ctx.with_preagg_renewal_threshold_secs(secs);
    }
    if let Some(db) = db {
        ctx = ctx.with_db(db);
    }
    // Also expose the cache + threshold directly to handlers via a typed
    // extension so endpoints like POST /semantic can resolve preagg without
    // routing through OxyProjectContext.
    request.extensions_mut().insert(PreaggCacheCtx {
        cache: preagg_cache,
        renewal_threshold_secs: preagg_renewal_threshold_secs,
    });
    request.extensions_mut().insert(SemanticLayerCacheCtx {
        cache: semantic_layer_cache,
        workspace_id,
    });
    request.extensions_mut().insert(SemanticEngineCacheCtx {
        cache: semantic_engine_cache,
        workspace_id,
    });
    let project_ctx = std::sync::Arc::new(ctx);
    let platform: std::sync::Arc<dyn agentic_pipeline::platform::PlatformContext> =
        project_ctx.clone();
    let bridges = crate::agentic_wiring::build_builder_bridges(project_ctx.clone());
    request.extensions_mut().insert(workspace_manager);
    request.extensions_mut().insert(platform);
    request.extensions_mut().insert(project_ctx);
    request.extensions_mut().insert(bridges);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::StatusCode;
    use axum::response::IntoResponse;

    /// An ordinary status denial stays opaque — same bytes as before this
    /// type existed, so nothing that mapped a bare code changes shape.
    #[test]
    fn access_error_status_passes_through_unchanged() {
        let response = WorkspaceAccessError::Status(StatusCode::NOT_FOUND).into_response();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        assert!(
            response.headers().get("x-oxy-assume-required").is_none(),
            "a plain status denial must not advertise assume-role"
        );
    }

    /// The staff denial explains itself. This is the contract the FE keys off
    /// to swap a dead 403 for the assume-role dialog — the header carries the
    /// org that must be assumed, so the dialog opens pre-scoped.
    #[test]
    fn access_error_assume_required_advertises_org_to_assume() {
        let workspace_id = Uuid::new_v4();
        let org_id = Uuid::new_v4();
        let response = WorkspaceAccessError::AssumeRequired {
            workspace_id,
            org_id,
            org_name: Some("Pokehouse".to_string()),
        }
        .into_response();

        assert_eq!(response.status(), StatusCode::FORBIDDEN);
        let header = response
            .headers()
            .get("x-oxy-assume-required")
            .expect("assume-required denial must carry the org header")
            .to_str()
            .expect("header is ascii");
        assert_eq!(
            header,
            org_id.to_string(),
            "header must name the org to assume, not the workspace"
        );
    }

    /// Generic 503 — no header, no workspace_id in the body — so legacy
    /// behavior is preserved when the workspace genuinely has no path.
    #[test]
    fn rejection_not_available_has_no_recompile_header() {
        let response = WorkspaceManagerMissing {
            needs_recompile: None,
        }
        .into_response();
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        assert!(
            response.headers().get("x-oxy-needs-recompile").is_none(),
            "legacy NotAvailable must not carry the recompile header"
        );
    }

    /// Serve-mode short-circuit carries the `X-Oxy-Needs-Recompile`
    /// header keyed by workspace_id so the FE can route the toast and
    /// reload-after-compile UX accordingly.
    #[test]
    fn rejection_needs_recompile_carries_workspace_header() {
        let workspace_id = Uuid::new_v4();
        let response = WorkspaceManagerMissing {
            needs_recompile: Some(workspace_id),
        }
        .into_response();
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        let header = response
            .headers()
            .get("x-oxy-needs-recompile")
            .expect("NeedsRecompile rejection must set the header");
        assert_eq!(header.to_str().unwrap(), workspace_id.to_string());
    }
}
