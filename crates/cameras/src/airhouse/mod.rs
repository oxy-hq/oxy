//! Airhouse-side writer + schema management for the camera fleet.
//!
//! Three concerns lived here:
//!
//! 1. **Schema DDL** (`schema`) — `CREATE TABLE IF NOT EXISTS` for the four
//!    per-tenant tables (`oxy_cam_events`, `oxy_cam_camera_health`,
//!    `oxy_cam_box_health`, `oxy_cam_compliance_reports`).
//! 2. **Connection** (`connection`) — mint a per-workspace, per-purpose
//!    ephemeral credential via the SA broker (`SystemPurpose::EdgeIngest`)
//!    and open a `tokio_postgres::Client` against the Airhouse pgwire
//!    endpoint. Matches the pattern in `observability/airhouse`.
//! 3. **Ensure cache** — a lazy "we've already created the tables in this
//!    tenant" set so the DDL only runs on the first ingest per
//!    (workspace_id, process lifetime).
//!
//! Service-layer entry points (`service::ingest`, `service::compliance`)
//! call `connect_and_ensure(workspace_id)` to get a ready-to-use `Client`,
//! then build a multi-row INSERT via simple-query (DuckLake doesn't speak
//! prepared statements / `$N` placeholders).

pub mod escape;
pub mod schema;

use std::collections::HashSet;
use std::sync::OnceLock;
use std::time::Duration;

use thiserror::Error;
use tokio::sync::RwLock;
use tokio_postgres::{Client, NoTls};
use tokio_postgres_rustls::MakeRustlsConnect;
use uuid::Uuid;

use airhouse::{AirhouseConfig, SystemPurpose, UserRole};

/// Errors from the airhouse write path. Service-layer code maps these
/// into `ServiceError` for the route layer.
#[derive(Debug, Error)]
pub enum AirhouseError {
    #[error("airhouse is not configured (env vars unset)")]
    Disabled,
    #[error("broker mint failed: {0}")]
    Mint(String),
    #[error("connect failed: {0}")]
    Connect(String),
    #[error("schema DDL failed: {0}")]
    Ddl(String),
    #[error("INSERT failed: {0}")]
    Insert(String),
}

/// Cap on how long an SA-minted credential is good for when we hand it
/// straight to a one-shot client. Plenty of headroom for an ingest
/// burst; well under the airhouse max (`SYSTEM_MAX_TTL_SECS = 86400`).
const INGEST_TTL: Duration = Duration::from_secs(15 * 60);

/// TTL for read-side credentials. Shorter than ingest: a UI request is
/// over in milliseconds, and reuse comes from the broker's per-purpose
/// cache, not from a long-lived token.
const READ_TTL: Duration = Duration::from_secs(5 * 60);

/// In-process record of which workspace tenants have had the
/// camera-fleet DDL applied this process lifetime. Keyed by
/// `workspace_id`. Eviction happens only on process restart; that's
/// fine because the DDL is idempotent (`IF NOT EXISTS`).
fn ensured() -> &'static RwLock<HashSet<Uuid>> {
    static C: OnceLock<RwLock<HashSet<Uuid>>> = OnceLock::new();
    C.get_or_init(|| RwLock::new(HashSet::new()))
}

/// Mint a Writer credential, open a pgwire client for ingest, and
/// ensure the camera-fleet DDL has been applied to this workspace's
/// tenant at least once this process.
///
/// **Where the DDL actually runs:** in production, the
/// `airhouse::register_post_provision_hook` registered at app startup
/// (see `crates/app/src/cli/commands/serve.rs`) fires this same
/// `ensure_schema` from `TenantProvisioner::provision`, so by the time
/// any edge box hits us the tables already exist. This function's
/// `ensured()` short-circuit + idempotent `CREATE TABLE IF NOT EXISTS`
/// then makes the ingest path's contribution effectively zero on the
/// hot path.
///
/// **The fallback case:** kept for safety in two scenarios:
///   1. Tenants provisioned before the hook was registered (this is a
///      one-time issue — solved automatically the first time ingest
///      runs against any such tenant).
///   2. A post-provision hook failure (logged but non-fatal — see
///      `airhouse::post_provision::invoke_all`).
///
/// **Role split**: DDL runs with **Admin** (Writer cannot `CREATE
/// TABLE` in DuckLake — fails with `42501 Permission denied`). INSERTs
/// run with **Writer** (least privilege for ingest). The two roles are
/// separate cache keys in the broker, so the Writer mint doesn't churn.
pub async fn connect_and_ensure(workspace_id: Uuid) -> Result<Client, AirhouseError> {
    // Fast path: schema already ensured for this workspace this process.
    let already_ensured = ensured().read().await.contains(&workspace_id);
    if !already_ensured {
        ensure_schema(workspace_id).await?;
        ensured().write().await.insert(workspace_id);
    }
    connect(
        workspace_id,
        UserRole::Writer,
        SystemPurpose::EdgeIngest,
        INGEST_TTL,
    )
    .await
}

/// Read-side companion to [`connect_and_ensure`]. Used by service-layer
/// SELECTs (e.g. the Compliance tab pulling reports for a camera).
///
/// Differences from the write path:
///   - Audited as [`SystemPurpose::ComplianceReportsRead`] so the
///     audit log can separate UI traffic from bulk edge ingest.
///   - Mints with [`UserRole::Reader`] — read-only, can SELECT but not
///     INSERT / DDL.
///   - Shorter TTL ([`READ_TTL`]); reuse comes from the broker cache.
///   - Does NOT run schema DDL. If the table doesn't exist (no edge box
///     ever wrote to it), the SELECT just returns an empty set or
///     errors as `undefined_table`, both of which are accurate.
pub async fn connect_for_reads(workspace_id: Uuid) -> Result<Client, AirhouseError> {
    connect(
        workspace_id,
        UserRole::Reader,
        SystemPurpose::ComplianceReportsRead,
        READ_TTL,
    )
    .await
}

/// One-shot Admin-credentialled DDL run. The Admin client is created,
/// used for the 4 `CREATE TABLE IF NOT EXISTS` statements, and dropped
/// immediately — no long-lived Admin handle in process memory.
pub async fn ensure_schema(workspace_id: Uuid) -> Result<(), AirhouseError> {
    let admin_client = connect(
        workspace_id,
        UserRole::Admin,
        SystemPurpose::EdgeIngest,
        INGEST_TTL,
    )
    .await?;
    schema::ensure(&admin_client).await?;
    drop(admin_client);
    Ok(())
}

/// Open a fresh tokio-postgres client to the workspace's Airhouse
/// tenant at the requested role. Caller drops the client when done; the
/// detached connection task exits with it.
///
/// `purpose` is what shows up in Airhouse's audit log on the broker
/// row — keep it specific so operators can separate bulk ingest from
/// human-driven SELECTs. `ttl` is the credential lifetime; the broker
/// caches by `(workspace_id, role, purpose)` so churn is bounded by
/// the longest-lived caller for each tuple.
pub async fn connect(
    workspace_id: Uuid,
    role: UserRole,
    purpose: SystemPurpose,
    ttl: Duration,
) -> Result<Client, AirhouseError> {
    let cfg = match AirhouseConfig::from_env() {
        AirhouseConfig::Enabled(c) => c,
        _ => return Err(AirhouseError::Disabled),
    };

    let broker = airhouse::token_broker().ok_or(AirhouseError::Disabled)?;
    let cred = broker
        .mint_for_system(workspace_id, purpose, role, ttl)
        .await
        .map_err(|e| AirhouseError::Mint(e.to_string()))?;

    let mut pg = tokio_postgres::Config::new();
    pg.host(&cfg.wire_host);
    pg.port(cfg.wire_port);
    pg.user(&cred.username);
    pg.password(&cred.password);
    pg.dbname(&cred.tenant);

    // Match observability's TLS-on-by-default + opt-out env var pattern.
    let insecure = std::env::var("OXY_AIRHOUSE_OBS_INSECURE")
        .map(|v| v == "true" || v == "1")
        .unwrap_or(false);

    let (client, conn_fut) = if insecure {
        let (c, conn) = pg
            .connect(NoTls)
            .await
            .map_err(|e| AirhouseError::Connect(e.to_string()))?;
        (
            c,
            Box::pin(conn)
                as std::pin::Pin<
                    Box<dyn std::future::Future<Output = Result<(), tokio_postgres::Error>> + Send>,
                >,
        )
    } else {
        let connector = tls_connector();
        let (c, conn) = pg
            .connect(connector)
            .await
            .map_err(|e| AirhouseError::Connect(e.to_string()))?;
        (
            c,
            Box::pin(conn)
                as std::pin::Pin<
                    Box<dyn std::future::Future<Output = Result<(), tokio_postgres::Error>> + Send>,
                >,
        )
    };

    // Drive the pgwire connection on a detached task. When the Client
    // drops, this future completes and the task exits.
    tokio::spawn(async move {
        if let Err(e) = conn_fut.await {
            tracing::warn!("cameras airhouse connection ended: {e}");
        }
    });

    Ok(client)
}

fn tls_connector() -> MakeRustlsConnect {
    use std::sync::OnceLock;
    static TLS: OnceLock<MakeRustlsConnect> = OnceLock::new();
    TLS.get_or_init(|| {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let mut roots = rustls::RootCertStore::empty();
        roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
        let cfg = rustls::ClientConfig::builder()
            .with_root_certificates(roots)
            .with_no_client_auth();
        MakeRustlsConnect::new(cfg)
    })
    .clone()
}
