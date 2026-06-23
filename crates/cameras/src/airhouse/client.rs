//! Persistent, **per-tenant** Airhouse clients for the camera fleet.
//!
//! # Why this exists
//!
//! Airhouse speaks the Postgres wire protocol but backs **every** accepted
//! connection with its own server-side DuckDB session — a fresh in-memory
//! DuckDB instance, the DuckLake / h3 / spatial extensions, and an `ATTACH`
//! of the tenant's catalog (see `airhouse-server/src/session.rs`). That
//! per-connection session is not cheap and is not reused server-side.
//!
//! The previous edge-ingest path opened a brand-new `tokio_postgres::Client`
//! on every `write_*` call and dropped it immediately. A fleet of edge boxes
//! POSTing health every few seconds therefore churned ~hundreds of fresh
//! DuckDB sessions per minute against a single tenant, which OOM'd Airhouse.
//!
//! # What this does
//!
//! Keep **one** long-lived `tokio_postgres::Client` per
//! `(workspace_id, role, purpose)` — i.e. per tenant + access pattern —
//! behind a background reconnect driver, mirroring
//! `observability::backends::airhouse`. The first write per tenant opens the
//! connection; every subsequent write reuses it, so session churn collapses
//! from N-per-minute to ~1-per-tenant. A dropped connection is re-established
//! in the background with exponential backoff, **re-minting** the ephemeral
//! credential through the SA broker on each attempt (pgwire auth happens once
//! at connect, so a live connection outlives its minting credential's TTL).

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use tokio::sync::{Mutex, OnceCell, RwLock};
use tokio_postgres::{Client, NoTls, SimpleQueryMessage};
use tokio_postgres_rustls::MakeRustlsConnect;
use uuid::Uuid;

use airhouse::{AirhouseConfig, SystemPurpose, UserRole};

use super::AirhouseError;

/// A boxed connection-driver future (the TLS stream type is erased so the
/// same `loop` can drive a `NoTls` or rustls connection across reconnects).
type BoxConn = Pin<Box<dyn Future<Output = Result<(), tokio_postgres::Error>> + Send + 'static>>;

/// Async callback returning a fresh `(user, password, database)` triple.
/// Invoked once at construction and again on every reconnect so the
/// SA-broker-minted ephemeral credential is transparently refreshed.
type CredFn = Arc<
    dyn Fn()
            -> Pin<Box<dyn Future<Output = Result<(String, String, String), AirhouseError>> + Send>>
        + Send
        + Sync,
>;

/// A persistent pgwire client to one Airhouse tenant, transparently
/// reconnected on drop. Cloneable handle (`Arc`) shared by all callers for
/// the same `(workspace, role, purpose)`.
pub struct TenantClient {
    /// Swapped by the reconnect driver; read-locked only to clone the inner
    /// `Arc<Client>`, so concurrent queries on the same tenant do not
    /// serialize on this lock.
    client: Arc<RwLock<Arc<Client>>>,
}

impl std::fmt::Debug for TenantClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TenantClient").finish_non_exhaustive()
    }
}

impl TenantClient {
    /// Run a simple-query against the live connection.
    ///
    /// Signature matches [`tokio_postgres::Client::simple_query`] so existing
    /// call sites (`client.simple_query(&sql).await`) compile unchanged. If
    /// the connection is mid-reconnect the call surfaces the driver error;
    /// the caller retries on its next tick and the background driver will
    /// have re-established the connection by then.
    pub async fn simple_query(
        &self,
        sql: &str,
    ) -> Result<Vec<SimpleQueryMessage>, tokio_postgres::Error> {
        let client = Arc::clone(&*self.client.read().await);
        client.simple_query(sql).await
    }
}

// ── Per-tenant registry ─────────────────────────────────────────────────────

/// Key: one persistent connection per tenant **and** access pattern. Reader
/// vs Writer vs Admin connect with different minted credentials, and the
/// `purpose` segments the airhouse audit log, so they must not share a
/// connection.
type Key = (Uuid, UserRole, &'static str);

/// Per-key lazily-initialised client cell. The `OnceCell` single-flights the
/// first connect per key without holding the outer lock across the network
/// round-trip.
type Cell = Arc<OnceCell<Arc<TenantClient>>>;

/// Map of key → client cell. Guarantees we never spawn two perpetual
/// reconnect drivers for the same tenant (a duplicate driver would leak a
/// connection for the whole process lifetime).
fn registry() -> &'static Mutex<HashMap<Key, Cell>> {
    static R: OnceLock<Mutex<HashMap<Key, Cell>>> = OnceLock::new();
    R.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Get (or lazily open) the persistent client for a tenant + access pattern.
///
/// `ttl` is the lifetime of each minted credential; reuse of the *connection*
/// is independent of it (auth is checked once at connect), so a short TTL no
/// longer forces a reconnect.
pub async fn tenant_client(
    workspace_id: Uuid,
    role: UserRole,
    purpose: SystemPurpose,
    ttl: Duration,
) -> Result<Arc<TenantClient>, AirhouseError> {
    let key = (workspace_id, role, purpose.as_str());
    let cell = {
        let mut guard = registry().lock().await;
        guard
            .entry(key)
            .or_insert_with(|| Arc::new(OnceCell::new()))
            .clone()
    };
    // On error the cell stays uninitialised, so the next caller retries the
    // connect rather than caching a permanent failure.
    cell.get_or_try_init(|| async {
        TenantClient::connect(workspace_id, role, purpose, ttl)
            .await
            .map(Arc::new)
    })
    .await
    .cloned()
}

impl TenantClient {
    async fn connect(
        workspace_id: Uuid,
        role: UserRole,
        purpose: SystemPurpose,
        ttl: Duration,
    ) -> Result<Self, AirhouseError> {
        let cfg = match AirhouseConfig::from_env() {
            AirhouseConfig::Enabled(c) => c,
            _ => return Err(AirhouseError::Disabled),
        };
        let host = cfg.wire_host.clone();
        let port = cfg.wire_port;
        let insecure = insecure_from_env();
        let key = (workspace_id, role, purpose.as_str());
        let max_reconnect_attempts = super::max_reconnect_attempts();
        let get_credentials = make_cred_fn(workspace_id, role, purpose, ttl);

        let (user, password, database) = get_credentials().await?;
        let config = make_pg_config(&host, port, &user, &password, &database);
        let (client, conn) = try_connect(&config, insecure)
            .await
            .map_err(|e| AirhouseError::Connect(e.to_string()))?;

        let client_ref = Arc::new(RwLock::new(Arc::new(client)));
        spawn_driver(DriverCtx {
            key,
            conn,
            client_ref: Arc::clone(&client_ref),
            host,
            port,
            insecure,
            get_credentials,
            max_reconnect_attempts,
        });
        Ok(Self { client: client_ref })
    }
}

// ── Credential refresh ──────────────────────────────────────────────────────

/// Build a [`CredFn`] that mints a fresh ephemeral credential via the SA
/// broker for this `(workspace, role, purpose)`. The broker caches mints by
/// the same tuple, so steady-state calls are cheap; a reconnect after a long
/// idle period re-mints transparently.
fn make_cred_fn(
    workspace_id: Uuid,
    role: UserRole,
    purpose: SystemPurpose,
    ttl: Duration,
) -> CredFn {
    Arc::new(move || {
        Box::pin(async move {
            let broker = airhouse::token_broker().ok_or(AirhouseError::Disabled)?;
            let cred = broker
                .mint_for_system(workspace_id, purpose, role, ttl)
                .await
                .map_err(|e| AirhouseError::Mint(e.to_string()))?;
            Ok((cred.username, cred.password, cred.tenant))
        })
    })
}

// ── Reconnect driver ────────────────────────────────────────────────────────

/// Everything the background driver needs. Grouped into a struct so the spawn
/// call stays under clippy's argument-count limit.
struct DriverCtx {
    key: Key,
    conn: BoxConn,
    client_ref: Arc<RwLock<Arc<Client>>>,
    host: String,
    port: u16,
    insecure: bool,
    get_credentials: CredFn,
    /// Consecutive reconnect attempts (per disconnect episode) before the
    /// driver gives up and evicts the tenant; `0` means retry forever.
    max_reconnect_attempts: u32,
}

/// Drive the pgwire connection future and reconnect with exponential backoff
/// when it ends. A single spawned task owns the whole lifecycle; the inner
/// loop handles every reconnect attempt so no task is spawned per reconnect.
/// Mirrors `observability::backends::airhouse::spawn_driver`, plus a give-up
/// bound: a deprovisioned / long-dead tenant must not keep a reconnect task
/// (and its log spam) alive forever, so after `max_reconnect_attempts`
/// consecutive failures the driver evicts the registry entry and exits. The
/// next request for that tenant re-establishes lazily.
fn spawn_driver(ctx: DriverCtx) {
    let DriverCtx {
        key,
        mut conn,
        client_ref,
        host,
        port,
        insecure,
        get_credentials,
        max_reconnect_attempts,
    } = ctx;
    tokio::spawn(async move {
        loop {
            match conn.await {
                Err(e) => tracing::warn!("cameras airhouse connection dropped: {e}"),
                Ok(()) => {
                    tracing::info!("cameras airhouse connection closed cleanly; reconnecting")
                }
            }
            let mut delay = Duration::from_millis(200);
            let mut attempts: u32 = 0;
            loop {
                tokio::time::sleep(delay).await;
                attempts += 1;

                // On success, reassign `conn` and `break` adjacently so the
                // borrow checker can prove `conn` is live at the outer loop's
                // next `conn.await`.
                match get_credentials().await {
                    Err(e) => {
                        tracing::warn!(
                            "cameras airhouse credential refresh failed: {e}, retrying in {delay:?}"
                        );
                    }
                    Ok((user, password, database)) => {
                        let config = make_pg_config(&host, port, &user, &password, &database);
                        match try_connect(&config, insecure).await {
                            Ok((new_client, new_conn)) => {
                                *client_ref.write().await = Arc::new(new_client);
                                tracing::info!("cameras airhouse reconnected");
                                conn = new_conn;
                                break; // drive the new connection in the outer loop
                            }
                            Err(e) => {
                                tracing::warn!(
                                    "cameras airhouse reconnect failed: {e}, retrying in {delay:?}"
                                );
                            }
                        }
                    }
                }

                // Reached only on a failed attempt (success paths `break` above).
                if max_reconnect_attempts > 0 && attempts >= max_reconnect_attempts {
                    tracing::warn!(
                        "cameras airhouse giving up after {attempts} reconnect attempts; \
                         evicting tenant connection (re-established on next use)"
                    );
                    // Best-effort: drop the registry entry so a later request
                    // builds a fresh connection instead of finding this dead one.
                    registry().lock().await.remove(&key);
                    return;
                }
                delay = (delay * 2).min(Duration::from_secs(30));
            }
        }
    });
}

// ── Low-level connection helpers ────────────────────────────────────────────

pub(super) fn make_pg_config(
    host: &str,
    port: u16,
    user: &str,
    password: &str,
    database: &str,
) -> tokio_postgres::Config {
    let mut cfg = tokio_postgres::Config::new();
    cfg.host(host);
    cfg.port(port);
    cfg.user(user);
    cfg.password(password);
    cfg.dbname(database);
    cfg
}

pub(super) async fn try_connect(
    config: &tokio_postgres::Config,
    insecure: bool,
) -> Result<(Client, BoxConn), tokio_postgres::Error> {
    if insecure {
        let (c, conn) = config.connect(NoTls).await?;
        Ok((c, Box::pin(conn)))
    } else {
        let (c, conn) = config.connect(tls_connector()).await?;
        Ok((c, Box::pin(conn)))
    }
}

/// TLS on by default; opt out with `OXY_AIRHOUSE_OBS_INSECURE` for
/// localhost / trusted-network deployments. Shares the env var with the
/// observability backend so operators flip one switch.
pub(super) fn insecure_from_env() -> bool {
    std::env::var("OXY_AIRHOUSE_OBS_INSECURE")
        .map(|v| v == "true" || v == "1")
        .unwrap_or(false)
}

pub(super) fn tls_connector() -> MakeRustlsConnect {
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
