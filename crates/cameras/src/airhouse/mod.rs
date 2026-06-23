//! Airhouse-side writer + schema management for the camera fleet.
//!
//! Three concerns live here:
//!
//! 1. **Schema DDL** (`schema`) — `CREATE TABLE IF NOT EXISTS` for the
//!    per-tenant tables (`oxy_cam_events`, `oxy_cam_camera_health`,
//!    `oxy_cam_box_health`, `oxy_cam_compliance_reports`, `oxy_cam_device_logs`).
//! 2. **Connections** (`client`) — **persistent, per-tenant** pgwire clients.
//!    Cameras are tenant-based (each workspace is its own Airhouse tenant with
//!    its own minted credentials), so the [`client`] registry keeps one
//!    long-lived `tokio_postgres::Client` per `(workspace_id, role, purpose)`
//!    and reuses it across writes. This is the fix for the edge-ingest session
//!    churn that OOM'd Airhouse: a fresh connection per write spawned a fresh
//!    server-side DuckDB session each time. The ephemeral credential is still
//!    minted per-tenant via the SA broker (`SystemPurpose::EdgeIngest`).
//! 3. **Ensure cache** — a lazy "we've already created the tables in this
//!    tenant" set so the DDL only runs on the first ingest per
//!    (workspace_id, process lifetime).
//!
//! Service-layer entry points (`service::ingest`, `service::compliance`)
//! call `connect_and_ensure(workspace_id)` to get a ready-to-use, reused
//! [`TenantClient`], then build a multi-row INSERT via simple-query (DuckLake
//! doesn't speak prepared statements / `$N` placeholders).

pub mod client;
pub mod escape;
pub mod schema;

use std::collections::HashSet;
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use thiserror::Error;
use tokio::sync::RwLock;
use tokio_postgres::Client;
use uuid::Uuid;

use airhouse::{AirhouseConfig, SystemPurpose, UserRole};

pub use client::TenantClient;

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

// ── Tunables (env-configurable) ─────────────────────────────────────────────
//
// NOTE: per-connection tunables (`ingest_ttl`, `read_ttl`,
// `max_reconnect_attempts`, the TLS opt-out) are read when a tenant's
// persistent connection is FIRST opened and captured for that connection's
// lifetime (including its background reconnects). Changing the env later only
// affects tenants connected afterward, not already-open ones. `insert_chunk_rows`
// is read per write, so it takes effect immediately.

/// Default credential TTL for the ingest (Writer) + DDL (Admin) paths.
/// Override with `OXY_CAMERAS_AIRHOUSE_INGEST_TTL_SECS`.
const DEFAULT_INGEST_TTL_SECS: u64 = 15 * 60;

/// Default credential TTL for the read (Reader) path.
/// Override with `OXY_CAMERAS_AIRHOUSE_READ_TTL_SECS`.
const DEFAULT_READ_TTL_SECS: u64 = 5 * 60;

/// Default max rows per INSERT statement; large ingest batches are split into
/// chunks of this size to keep SQL strings — and the server-side materialised
/// row group — bounded. Override with `OXY_CAMERAS_AIRHOUSE_INSERT_CHUNK_ROWS`.
const DEFAULT_INSERT_CHUNK_ROWS: usize = 500;

/// Default consecutive reconnect failures (per disconnect episode) a persistent
/// tenant connection tolerates before its background driver gives up, evicts
/// the tenant from the registry, and exits — so a deprovisioned / long-dead
/// tenant doesn't keep a reconnect task and a held server-side DuckDB session
/// alive forever. The next request re-establishes lazily. At the 30s backoff
/// cap, 20 attempts is ~10 minutes of continuous failure. Override with
/// `OXY_CAMERAS_AIRHOUSE_MAX_RECONNECT_ATTEMPTS`; `0` means retry forever.
const DEFAULT_MAX_RECONNECT_ATTEMPTS: u32 = 20;

/// Credential lifetime for the ingest / DDL path. Plenty of headroom for an
/// ingest burst; well under the airhouse max (`SYSTEM_MAX_TTL_SECS = 86400`).
/// Note this bounds the *credential*, not the connection — a persistent
/// connection is reused across TTLs and only re-mints on reconnect.
pub fn ingest_ttl() -> Duration {
    env_duration_secs(
        "OXY_CAMERAS_AIRHOUSE_INGEST_TTL_SECS",
        DEFAULT_INGEST_TTL_SECS,
    )
}

/// Credential lifetime for the read path.
pub fn read_ttl() -> Duration {
    env_duration_secs("OXY_CAMERAS_AIRHOUSE_READ_TTL_SECS", DEFAULT_READ_TTL_SECS)
}

/// Max rows per INSERT statement for the ingest writers.
pub fn insert_chunk_rows() -> usize {
    env_usize(
        "OXY_CAMERAS_AIRHOUSE_INSERT_CHUNK_ROWS",
        DEFAULT_INSERT_CHUNK_ROWS,
    )
}

/// Consecutive reconnect failures a persistent tenant connection tolerates
/// before giving up + evicting (see [`DEFAULT_MAX_RECONNECT_ATTEMPTS`]). `0`
/// (explicitly set) means retry forever; garbage / unset → default.
pub fn max_reconnect_attempts() -> u32 {
    std::env::var("OXY_CAMERAS_AIRHOUSE_MAX_RECONNECT_ATTEMPTS")
        .ok()
        .and_then(|v| v.trim().parse::<u32>().ok())
        .unwrap_or(DEFAULT_MAX_RECONNECT_ATTEMPTS)
}

/// Parse a positive-integer seconds env var into a `Duration`, falling back
/// to `default_secs` when unset, empty, unparseable, or zero.
fn env_duration_secs(var: &str, default_secs: u64) -> Duration {
    Duration::from_secs(env_u64(var, default_secs))
}

fn env_u64(var: &str, default: u64) -> u64 {
    std::env::var(var)
        .ok()
        .and_then(|v| v.trim().parse::<u64>().ok())
        .filter(|&n| n > 0)
        .unwrap_or(default)
}

fn env_usize(var: &str, default: usize) -> usize {
    std::env::var(var)
        .ok()
        .and_then(|v| v.trim().parse::<usize>().ok())
        .filter(|&n| n > 0)
        .unwrap_or(default)
}

/// In-process record of which workspace tenants have had the
/// camera-fleet DDL applied this process lifetime. Keyed by
/// `workspace_id`. Eviction happens only on process restart; that's
/// fine because the DDL is idempotent (`IF NOT EXISTS`).
fn ensured() -> &'static RwLock<HashSet<Uuid>> {
    static C: OnceLock<RwLock<HashSet<Uuid>>> = OnceLock::new();
    C.get_or_init(|| RwLock::new(HashSet::new()))
}

/// Ensure the camera-fleet DDL has been applied to this workspace's tenant at
/// least once this process, then return the **reused** Writer ingest client.
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
///   1. Tenants provisioned before the hook was registered (one-time —
///      solved automatically the first time ingest runs against any such
///      tenant).
///   2. A post-provision hook failure (logged but non-fatal — see
///      `airhouse::post_provision::invoke_all`).
///
/// **Role split**: DDL runs with **Admin** (Writer cannot `CREATE TABLE` in
/// DuckLake — fails with `42501 Permission denied`). INSERTs run with
/// **Writer** (least privilege for ingest). The two roles are separate
/// persistent connections (and separate broker cache keys), so the Writer
/// reuse doesn't churn the Admin path.
pub async fn connect_and_ensure(workspace_id: Uuid) -> Result<Arc<TenantClient>, AirhouseError> {
    // Fast path: schema already ensured for this workspace this process.
    let already_ensured = ensured().read().await.contains(&workspace_id);
    if !already_ensured {
        ensure_schema(workspace_id).await?;
        ensured().write().await.insert(workspace_id);
    }
    client::tenant_client(
        workspace_id,
        UserRole::Writer,
        SystemPurpose::EdgeIngest,
        ingest_ttl(),
    )
    .await
}

/// Read-side companion to [`connect_and_ensure`]. Used by service-layer
/// SELECTs (e.g. the Compliance tab pulling reports for a camera).
///
/// Differences from the write path:
///   - Audited as [`SystemPurpose::ComplianceReportsRead`] so the audit log
///     can separate UI traffic from bulk edge ingest.
///   - Mints with [`UserRole::Reader`] — read-only, can SELECT but not
///     INSERT / DDL.
///   - Shorter credential TTL ([`read_ttl`]).
///   - Does NOT run schema DDL. If the table doesn't exist (no edge box ever
///     wrote to it), the SELECT just returns an empty set or errors as
///     `undefined_table`, both of which are accurate.
///
/// Like the write path, the underlying connection is **persistent and reused**
/// per tenant rather than opened per request.
pub async fn connect_for_reads(workspace_id: Uuid) -> Result<Arc<TenantClient>, AirhouseError> {
    client::tenant_client(
        workspace_id,
        UserRole::Reader,
        SystemPurpose::ComplianceReportsRead,
        read_ttl(),
    )
    .await
}

/// One-shot connection for the log-retention sweep.
///
/// Retention runs infrequently (hourly+, `OXY_CAMERA_LOG_SWEEP_INTERVAL_HOURS`)
/// and issues large `DELETE`s. Deliberately **not** routed through the
/// persistent `(Writer, EdgeIngest)` ingest connection: the server-side DuckDB
/// session executes serially, so a slow retention `DELETE` sharing that
/// connection could head-of-line-block live edge ingest for the tenant. A
/// dedicated one-shot connection (audited under [`SystemPurpose::Scheduler`])
/// isolates it; being one-shot is also cheaper than holding a second persistent
/// session per tenant for an operation that runs a couple times a day. No DDL —
/// a missing table is handled by the caller as "nothing to retain".
pub async fn connect_for_retention(workspace_id: Uuid) -> Result<Client, AirhouseError> {
    connect(
        workspace_id,
        UserRole::Writer,
        SystemPurpose::Scheduler,
        ingest_ttl(),
    )
    .await
}

/// One-shot Admin-credentialled DDL run. The Admin client is created, used
/// for the `CREATE TABLE IF NOT EXISTS` statements, and dropped immediately —
/// no long-lived Admin handle in process memory. DDL is rare (gated by
/// `ensured()`), so this path intentionally does **not** join the persistent
/// registry.
pub async fn ensure_schema(workspace_id: Uuid) -> Result<(), AirhouseError> {
    let admin_client = connect(
        workspace_id,
        UserRole::Admin,
        SystemPurpose::EdgeIngest,
        ingest_ttl(),
    )
    .await?;
    schema::ensure(&admin_client).await?;
    drop(admin_client);
    Ok(())
}

/// Open a **fresh, one-shot** tokio-postgres client to the workspace's
/// Airhouse tenant at the requested role. The caller drops the client when
/// done; the detached connection task exits with it.
///
/// This is only for the short-lived Admin DDL path. High-frequency ingest /
/// read paths must use [`connect_and_ensure`] / [`connect_for_reads`], which
/// return a reused persistent connection instead.
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

    let pg = client::make_pg_config(
        &cfg.wire_host,
        cfg.wire_port,
        &cred.username,
        &cred.password,
        &cred.tenant,
    );
    let (client, conn_fut) = client::try_connect(&pg, client::insecure_from_env())
        .await
        .map_err(|e| AirhouseError::Connect(e.to_string()))?;

    // Drive the pgwire connection on a detached task. When the Client drops,
    // this future completes and the task exits.
    tokio::spawn(async move {
        if let Err(e) = conn_fut.await {
            tracing::warn!("cameras airhouse one-shot connection ended: {e}");
        }
    });

    Ok(client)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Env-var parsing must serialize: these touch the process-wide env.
    static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    fn ttls_and_chunk_fall_back_to_defaults_when_unset() {
        let _g = ENV_LOCK.lock().unwrap();
        // SAFETY: serialized via ENV_LOCK.
        unsafe {
            std::env::remove_var("OXY_CAMERAS_AIRHOUSE_INGEST_TTL_SECS");
            std::env::remove_var("OXY_CAMERAS_AIRHOUSE_READ_TTL_SECS");
            std::env::remove_var("OXY_CAMERAS_AIRHOUSE_INSERT_CHUNK_ROWS");
        }
        assert_eq!(ingest_ttl(), Duration::from_secs(DEFAULT_INGEST_TTL_SECS));
        assert_eq!(read_ttl(), Duration::from_secs(DEFAULT_READ_TTL_SECS));
        assert_eq!(insert_chunk_rows(), DEFAULT_INSERT_CHUNK_ROWS);
    }

    #[test]
    fn env_overrides_are_honored() {
        let _g = ENV_LOCK.lock().unwrap();
        // SAFETY: serialized via ENV_LOCK.
        unsafe {
            std::env::set_var("OXY_CAMERAS_AIRHOUSE_INGEST_TTL_SECS", "120");
            std::env::set_var("OXY_CAMERAS_AIRHOUSE_READ_TTL_SECS", "30");
            std::env::set_var("OXY_CAMERAS_AIRHOUSE_INSERT_CHUNK_ROWS", "1000");
        }
        assert_eq!(ingest_ttl(), Duration::from_secs(120));
        assert_eq!(read_ttl(), Duration::from_secs(30));
        assert_eq!(insert_chunk_rows(), 1000);
        // SAFETY: serialized via ENV_LOCK.
        unsafe {
            std::env::remove_var("OXY_CAMERAS_AIRHOUSE_INGEST_TTL_SECS");
            std::env::remove_var("OXY_CAMERAS_AIRHOUSE_READ_TTL_SECS");
            std::env::remove_var("OXY_CAMERAS_AIRHOUSE_INSERT_CHUNK_ROWS");
        }
    }

    #[test]
    fn max_reconnect_attempts_parsing() {
        let _g = ENV_LOCK.lock().unwrap();
        let var = "OXY_CAMERAS_AIRHOUSE_MAX_RECONNECT_ATTEMPTS";
        // SAFETY: serialized via ENV_LOCK.
        unsafe { std::env::remove_var(var) };
        assert_eq!(max_reconnect_attempts(), DEFAULT_MAX_RECONNECT_ATTEMPTS);
        // SAFETY: serialized via ENV_LOCK.
        unsafe { std::env::set_var(var, "5") };
        assert_eq!(max_reconnect_attempts(), 5);
        // Unlike the other knobs, 0 is meaningful here ("retry forever") and
        // must NOT fall back to the default.
        // SAFETY: serialized via ENV_LOCK.
        unsafe { std::env::set_var(var, "0") };
        assert_eq!(max_reconnect_attempts(), 0);
        // SAFETY: serialized via ENV_LOCK.
        unsafe { std::env::set_var(var, "garbage") };
        assert_eq!(max_reconnect_attempts(), DEFAULT_MAX_RECONNECT_ATTEMPTS);
        // SAFETY: serialized via ENV_LOCK.
        unsafe { std::env::remove_var(var) };
    }

    #[test]
    fn zero_and_garbage_values_fall_back_to_defaults() {
        let _g = ENV_LOCK.lock().unwrap();
        // SAFETY: serialized via ENV_LOCK.
        unsafe {
            std::env::set_var("OXY_CAMERAS_AIRHOUSE_INGEST_TTL_SECS", "0");
            std::env::set_var("OXY_CAMERAS_AIRHOUSE_INSERT_CHUNK_ROWS", "not-a-number");
        }
        assert_eq!(ingest_ttl(), Duration::from_secs(DEFAULT_INGEST_TTL_SECS));
        assert_eq!(insert_chunk_rows(), DEFAULT_INSERT_CHUNK_ROWS);
        // SAFETY: serialized via ENV_LOCK.
        unsafe {
            std::env::remove_var("OXY_CAMERAS_AIRHOUSE_INGEST_TTL_SECS");
            std::env::remove_var("OXY_CAMERAS_AIRHOUSE_INSERT_CHUNK_ROWS");
        }
    }
}
