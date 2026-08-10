//! Host-side data plane for Oxy Functions — the Rust implementation of the
//! `ctx.query` / `ctx.fetch` calls the isolate makes over the broker channel.
//!
//! See `internal-docs/customer-apps-functions.md` §11.5
//! (query cap), §11.9 (fetch size cap), §11.3 (warehouse write scope —
//! per-function fail-closed `destinations` allowlist).

use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;

use agentic_connector::DatabaseConnector;
use agentic_pipeline::airway_run::StartAirwayRequest;
use agentic_semantic::compile::{CompiledQuery, resolve_and_compile};
use agentic_semantic::config::SemanticQueryConfig;
use sea_orm::DatabaseConnection;
use uuid::Uuid;

use oxy::service::secret_manager::SecretManagerService;

use super::runtime::{
    FUNCTION_MAX_ROWS, FUNCTION_STREAM_MAX_ROWS, FunctionHost, FunctionProjectContext,
    FunctionQueryExecutor,
};

/// Outbound fetch response size cap (design doc §11.9).
const FETCH_MAX_BYTES: u64 = 10 * 1024 * 1024;

/// Total per-request wall-clock cap for `ctx.fetch` (connect + transfer). A
/// backstop so a slow/never-responding upstream can't keep the isolate parked
/// in a host call past the function's own timeout (design doc §11.9).
const FETCH_TOTAL_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(60);

/// Concrete `FunctionHost` backed by the invocation's resolved project
/// context (for SQL connectors) and a shared HTTP client (for `ctx.fetch`).
pub struct ProjectFunctionHost {
    /// The invocation's project context (connectors + workspace config +
    /// Airway seed), behind a trait so the runtime doesn't depend on
    /// `agentic_wiring`. Injected at construction.
    proj_ctx: Arc<dyn FunctionProjectContext>,
    /// Runs `ctx.query` / `ctx.queryStream` SQL. Injected at the composition
    /// root so the runtime depends on the trait, not on `projects::query`.
    query_exec: Arc<dyn FunctionQueryExecutor>,
    db: DatabaseConnection,
    http: reqwest::Client,
    /// §11.3 allowlist — database names `ctx.warehouse.*` may write to. Empty →
    /// the function may not write to any database (fail-closed).
    write_destinations: Vec<String>,
    /// Project the app belongs to — scopes the SecretManager for `ctx.secrets.set`.
    project_id: Uuid,
    /// App id — `ctx.secrets.set` writes land under `apps/<app_id>/`.
    app_id: Uuid,
    /// Actor stamped as created_by/updated_by on secret writes (the invoking
    /// user for a route call; the app owner for a scheduled call). Non-null FK.
    actor: Uuid,
    /// §11.x fail-closed: `ctx.secrets.set` is rejected unless the function
    /// declares the `secrets.write` capability in its manifest.
    secrets_write: bool,
    /// App display name, used as the `From` friendly name on `ctx.email.send`.
    app_name: String,
    /// Fail-closed capability gate for `ctx.email.send`.
    email_send: bool,
    /// Fail-closed capability gate for `ctx.storage` reads (getDownloadUrl / list / get).
    storage_read: bool,
    /// Fail-closed capability gate for `ctx.storage` writes (getUploadUrl / put).
    storage_write: bool,
    /// Per-invocation `ctx.email.send` counter (this host lives for exactly one
    /// invocation), bounding email fan-out from a single run.
    email_send_count: std::sync::atomic::AtomicUsize,
}

impl ProjectFunctionHost {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        proj_ctx: Arc<dyn FunctionProjectContext>,
        query_exec: Arc<dyn FunctionQueryExecutor>,
        db: DatabaseConnection,
        write_destinations: Vec<String>,
        project_id: Uuid,
        app_id: Uuid,
        actor: Uuid,
        secrets_write: bool,
        app_name: String,
        email_send: bool,
        storage_read: bool,
        storage_write: bool,
    ) -> Self {
        Self {
            proj_ctx,
            query_exec,
            db,
            write_destinations,
            project_id,
            app_id,
            actor,
            secrets_write,
            app_name,
            email_send,
            storage_read,
            storage_write,
            email_send_count: std::sync::atomic::AtomicUsize::new(0),
            // `ctx.fetch` is defended in two layers:
            //  1. `is_safe_outbound` rejects the request URL up front (scheme,
            //     literal private IPs, internal suffixes).
            //  2. `PublicOnlyDnsResolver` validates every *resolved* IP at
            //     connect time, closing the DNS-rebinding hole the URL string
            //     check can't see (a public hostname whose A record points at
            //     169.254.169.254 / 10.x / 127.x).
            // Redirects are disabled so a 302 to an internal address can't
            // launder past either layer on a later hop (reqwest gives no
            // per-hop validation hook), and a total timeout bounds the call.
            http: reqwest::Client::builder()
                .redirect(reqwest::redirect::Policy::none())
                .connect_timeout(std::time::Duration::from_secs(10))
                .dns_resolver(Arc::new(PublicOnlyDnsResolver))
                .build()
                .expect("failed to build ctx.fetch HTTP client"),
        }
    }

    /// Resolve the database `ctx.query` runs against: the project's
    /// configured default, else the first listed database. Mirrors the
    /// fallback in `projects::query::run_query`.
    fn default_database(&self) -> Result<String, String> {
        let cm = &self.proj_ctx.workspace_manager().config_manager;
        if let Some(default) = cm.default_database_ref() {
            Ok(default.clone())
        } else if let Some(first) = cm.list_databases().first() {
            Ok(first.name.clone())
        } else {
            Err("this project has no databases configured".to_string())
        }
    }

    /// Build a connector for `db_name`, bounded by the host DB-op timeout so a
    /// slow TLS/auth handshake during connector construction can't keep a
    /// detached isolate thread alive past the backstop.
    async fn connect(&self, db_name: &str) -> Result<Arc<dyn DatabaseConnector>, String> {
        with_db_timeout("connect", async {
            self.proj_ctx
                .build_connector_for(db_name)
                .await
                .map_err(|e| format!("failed to connect to database '{db_name}': {e}"))
        })
        .await
    }
}

#[async_trait::async_trait]
impl FunctionHost for ProjectFunctionHost {
    async fn query(&self, sql: String) -> Result<serde_json::Value, String> {
        let db_name = self.default_database()?;
        let connector = self.connect(&db_name).await?;
        let (rows, truncated) =
            query_with_truncation(&self.query_exec, connector, &sql, FUNCTION_MAX_ROWS).await?;
        let result = serde_json::json!({ "rows": rows, "truncated": truncated });
        enforce_result_byte_cap(&result)?;
        Ok(result)
    }

    async fn query_stream(&self, sql: String) -> Result<serde_json::Value, String> {
        let db_name = self.default_database()?;
        let connector = self.connect(&db_name).await?;
        let rows = with_db_timeout(
            "query_stream",
            self.query_exec
                .execute(connector, &sql, FUNCTION_STREAM_MAX_ROWS),
        )
        .await?;
        Ok(serde_json::Value::Array(rows))
    }

    async fn fetch(
        &self,
        url: String,
        init: serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        let parsed = reqwest::Url::parse(&url).map_err(|e| format!("invalid url: {e}"))?;
        if !is_safe_outbound(&parsed) {
            return Err(format!("fetch to '{url}' blocked by SSRF allowlist"));
        }

        let method = init
            .get("method")
            .and_then(|m| m.as_str())
            .unwrap_or("GET")
            .to_uppercase();
        let mut req = self
            .http
            .request(
                reqwest::Method::from_bytes(method.as_bytes())
                    .map_err(|_| format!("invalid method '{method}'"))?,
                parsed,
            )
            .timeout(FETCH_TOTAL_TIMEOUT);
        if let Some(headers) = init.get("headers").and_then(|h| h.as_object()) {
            for (k, v) in headers {
                if let Some(vs) = v.as_str() {
                    req = req.header(k, vs);
                }
            }
        }
        if let Some(body) = init.get("body").and_then(|b| b.as_str()) {
            req = req.body(body.to_string());
        }

        let resp = req.send().await.map_err(|e| format!("fetch failed: {e}"))?;
        let status = resp.status().as_u16();

        // §11.9 — reject oversized responses up front via Content-Length…
        if let Some(len) = resp.content_length()
            && len > FETCH_MAX_BYTES
        {
            return Err(format!(
                "response too large ({len} bytes > {FETCH_MAX_BYTES} cap)"
            ));
        }
        // …and bound the actual read for chunked responses without a length.
        let bytes = resp
            .bytes()
            .await
            .map_err(|e| format!("read failed: {e}"))?;
        if bytes.len() as u64 > FETCH_MAX_BYTES {
            return Err(format!(
                "response too large ({} bytes > {FETCH_MAX_BYTES} cap)",
                bytes.len()
            ));
        }
        // UTF-8 stays the default for back-compat, but it is lossy: every
        // non-UTF-8 byte becomes U+FFFD, so fetching a PDF/PNG to attach to an
        // email silently yields a corrupt file. `encoding: "base64"` is the
        // only way binary survives the crossing — same contract as
        // `ctx.storage.get`.
        let encoding = encoding_or(init.get("encoding").and_then(|e| e.as_str()), "utf8");
        let body = match encoding {
            "utf8" => String::from_utf8_lossy(&bytes).into_owned(),
            "base64" => encode_base64(&bytes),
            other => {
                return Err(format!(
                    "ctx.fetch: unknown encoding '{other}' (expected 'utf8' or 'base64')"
                ));
            }
        };
        Ok(serde_json::json!({ "status": status, "body": body, "encoding": encoding }))
    }

    async fn semantic_query(&self, spec: serde_json::Value) -> Result<serde_json::Value, String> {
        let query: SemanticQueryConfig = serde_json::from_value(spec)
            .map_err(|e| format!("invalid semantic query spec: {e}"))?;

        let cm = &self.proj_ctx.workspace_manager().config_manager;
        let scan_path = cm.semantics_scan_path();
        let databases: Vec<airlayer::DatabaseConfig> = cm
            .list_databases()
            .iter()
            .map(|db| airlayer::DatabaseConfig {
                name: db.name.clone(),
                db_type: db.database_type.to_string(),
            })
            .collect();

        let compiled = tokio::task::spawn_blocking(move || {
            resolve_and_compile(&scan_path, &databases, &query, None, 0, None)
        })
        .await
        .map_err(|e| format!("semantic compile task panicked: {e}"))?
        .map_err(|e| format!("semantic compile failed: {e}"))?;

        let (sql, database_name) = match compiled {
            CompiledQuery::Warehouse { sql, database_name } => (sql, database_name),
            CompiledQuery::Preaggregation {
                warehouse_sql,
                warehouse_database,
                ..
            } => (warehouse_sql, warehouse_database),
        };

        let connector = self.connect(&database_name).await?;
        let (rows, truncated) =
            query_with_truncation(&self.query_exec, connector, &sql, FUNCTION_MAX_ROWS).await?;
        let result = serde_json::json!({ "rows": rows, "truncated": truncated });
        enforce_result_byte_cap(&result)?;
        Ok(result)
    }

    async fn airway_run(
        &self,
        pipeline_ref: String,
        variables: serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        // Seed the run on the global queue and return the run id. We don't
        // drive a co-located coordinator (TaskScope::Global) because ELT runs
        // routinely outlast a function's timeout ceiling — the worker fleet
        // picks it up and drives it to completion asynchronously. The caller
        // polls the Orchestrator/runs API for status.
        let request = StartAirwayRequest {
            pipeline_ref,
            variables: if variables.is_null() {
                None
            } else {
                Some(variables)
            },
            thread_id: None,
            resources: Vec::new(),
            schedule_id: None,
            trigger: Some("manual".to_string()),
            logical_date: None,
            retry_of: None,
            backfill_from: None,
            backfill_to: None,
        };
        let run_id = self.proj_ctx.start_airway_seed(&self.db, request).await?;
        Ok(serde_json::json!({ "runId": run_id }))
    }

    async fn warehouse_write(
        &self,
        op: String,
        payload: serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        let database = payload
            .get("database")
            .and_then(|v| v.as_str())
            .ok_or_else(|| "warehouse write: `database` is required".to_string())?;

        // §11.3 — restrict writes to the function's declared `destinations`
        // allowlist (fail-closed: an empty allowlist denies all writes). This
        // is validated *before* any connector — and therefore any ephemeral
        // credential — is built, so a function can never `exec` DDL/DML against
        // the project's source warehouse just because it's configured. The
        // target must ALSO be a real configured database.
        if !self.write_destinations.iter().any(|d| d == database) {
            return Err(format!(
                "warehouse write: database '{database}' is not in this function's \
                 `destinations` allowlist (declare it in oxy-app.json to permit writes)"
            ));
        }
        let cm = &self.proj_ctx.workspace_manager().config_manager;
        if !cm.list_databases().iter().any(|db| db.name == database) {
            return Err(format!(
                "warehouse write: database '{database}' is not configured for this project"
            ));
        }

        let sql = match op.as_str() {
            "exec" => payload
                .get("sql")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "warehouse.exec: `sql` is required".to_string())?
                .to_string(),
            "insert" => build_insert_sql(&payload, false)?,
            "upsert" => build_insert_sql(&payload, true)?,
            other => return Err(format!("warehouse write: unknown op '{other}'")),
        };

        let connector = self.connect(database).await?;
        let label = format!("warehouse {op}");
        with_db_timeout(&label, async {
            connector
                .execute_statement(&sql)
                .await
                .map_err(|e| format!("warehouse {op} failed: {e}"))
        })
        .await?;
        Ok(serde_json::json!({ "ok": true }))
    }

    async fn secrets_set(&self, key: String, value: String) -> Result<serde_json::Value, String> {
        // §11.x fail-closed: the function must declare the `secrets.write`
        // capability. Scoped to the app's own `apps/<app_id>/` namespace, so it
        // can never touch another app's or the project's secrets.
        if !self.secrets_write {
            return Err(
                "ctx.secrets.set: this function has not declared the `secrets.write` \
                 capability (add it to oxy-app.json to permit writes)"
                    .to_string(),
            );
        }
        SecretManagerService::new(self.project_id)
            .set_app_secret(&self.db, self.app_id, &key, &value, self.actor)
            .await
            .map_err(|e| format!("ctx.secrets.set failed: {e}"))?;
        Ok(serde_json::json!({ "ok": true }))
    }

    async fn send_email(&self, input: serde_json::Value) -> Result<serde_json::Value, String> {
        // Fail-closed: the function must declare the `email.send` capability.
        if !self.email_send {
            return Err(
                "EmailCapabilityMissing: this function has not declared the `email.send` \
                 capability (add \"email\": { \"send\": true } to its oxy-app.json entry)"
                    .to_string(),
            );
        }
        // Per-invocation fan-out cap (this host serves exactly one invocation).
        let n = self
            .email_send_count
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            + 1;
        if n > MAX_EMAILS_PER_INVOCATION {
            return Err(format!(
                "RateLimitExceeded: this invocation exceeded the \
                 {MAX_EMAILS_PER_INVOCATION}-email limit"
            ));
        }
        let parsed =
            serde_json::from_value(input).map_err(|e| format!("InvalidEmailPayload: {e}"))?;
        crate::emails::app_emailer::AppEmailer::from_env(self.app_name.clone())
            .send(parsed)
            .await
    }

    async fn storage(
        &self,
        op: String,
        payload: serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        use crate::server::api::custom_apps_storage as st;

        // Fail-closed capability gate: uploads/put need `storage.write`; the
        // read paths need `storage.read`.
        let needs_write = matches!(op.as_str(), "getUploadUrl" | "put");
        if needs_write && !self.storage_write {
            return Err(
                "StorageCapabilityMissing: this function has not declared the `storage.write` \
                 capability (add \"storage\": { \"write\": true } to its oxy-app.json entry)"
                    .to_string(),
            );
        }
        if !needs_write && !self.storage_read {
            return Err(
                "StorageCapabilityMissing: this function has not declared the `storage.read` \
                 capability (add \"storage\": { \"read\": true } to its oxy-app.json entry)"
                    .to_string(),
            );
        }

        let str_field = |key: &str| payload.get(key).and_then(|v| v.as_str());
        let bool_field = |key: &str| payload.get(key).and_then(|v| v.as_bool()).unwrap_or(false);
        let u64_field = |key: &str| payload.get(key).and_then(|v| v.as_u64());
        let required = |field: &str| format!("ctx.storage.{op}: `{field}` is required");

        match op.as_str() {
            "getUploadUrl" => {
                // `pathname` is the general form; `filename` is the ergonomic
                // shorthand for "a human picked a file", which lands under
                // `uploads/`. Generated assets pass `pathname` directly.
                let pathname = match (str_field("pathname"), str_field("filename")) {
                    (Some(p), _) => p.to_string(),
                    (None, Some(f)) => format!("uploads/{f}"),
                    (None, None) => return Err(required("pathname")),
                };
                let content_length =
                    u64_field("contentLength").ok_or_else(|| required("contentLength"))?;
                let out = st::get_upload_url(
                    self.app_id,
                    &pathname,
                    str_field("contentType").unwrap_or(""),
                    content_length,
                    u64_field("expiresInSeconds"),
                )
                .await
                .map_err(|e| e.to_string())?;
                serde_json::to_value(out).map_err(|e| e.to_string())
            }
            "getDownloadUrl" => {
                let key = str_field("key").ok_or_else(|| required("key"))?;
                let out = st::get_download_url(
                    self.app_id,
                    key,
                    u64_field("expiresInSeconds"),
                    bool_field("download"),
                )
                .await
                .map_err(|e| e.to_string())?;
                serde_json::to_value(out).map_err(|e| e.to_string())
            }
            "put" => {
                let pathname = str_field("pathname")
                    .or_else(|| str_field("key"))
                    .ok_or_else(|| required("pathname"))?;
                let body = str_field("body").ok_or_else(|| required("body"))?;
                // base64 is how a BINARY generated asset (PDF, PNG, Parquet)
                // crosses the isolate's JSON boundary; utf8 is the default for
                // text a function built itself.
                let bytes = match encoding_or(str_field("encoding"), "utf8") {
                    "base64" => decode_base64(body)
                        .map_err(|e| format!("ctx.storage.put: `body` is not valid base64: {e}"))?,
                    "utf8" => body.as_bytes().to_vec(),
                    other => {
                        return Err(format!(
                            "ctx.storage.put: unknown encoding '{other}' (expected 'utf8' or \
                             'base64')"
                        ));
                    }
                };
                let out = st::put(
                    self.app_id,
                    pathname,
                    bytes,
                    st::PutOptions {
                        content_type: str_field("contentType").map(str::to_string),
                        add_random_suffix: bool_field("addRandomSuffix"),
                        allow_overwrite: bool_field("allowOverwrite"),
                        cache_control_max_age: u64_field("cacheControlMaxAge"),
                    },
                )
                .await
                .map_err(|e| e.to_string())?;
                serde_json::to_value(out).map_err(|e| e.to_string())
            }
            "get" => {
                let key = str_field("key").ok_or_else(|| required("key"))?;
                let encoding = encoding_or(str_field("encoding"), "utf8").to_string();
                if !matches!(encoding.as_str(), "utf8" | "base64") {
                    return Err(format!(
                        "ctx.storage.get: unknown encoding '{encoding}' (expected 'utf8' or \
                         'base64')"
                    ));
                }
                match st::get(self.app_id, key).await.map_err(|e| e.to_string())? {
                    Some((bytes, content_type)) => {
                        let size = bytes.len();
                        let body = if encoding == "base64" {
                            encode_base64(&bytes)
                        } else {
                            String::from_utf8_lossy(&bytes).into_owned()
                        };
                        Ok(serde_json::json!({
                            "body": body,
                            "contentType": content_type,
                            "size": size,
                            "encoding": encoding,
                        }))
                    }
                    None => Ok(serde_json::Value::Null),
                }
            }
            "head" => {
                let key = str_field("key").ok_or_else(|| required("key"))?;
                match st::head(self.app_id, key)
                    .await
                    .map_err(|e| e.to_string())?
                {
                    Some(meta) => serde_json::to_value(meta).map_err(|e| e.to_string()),
                    None => Ok(serde_json::Value::Null),
                }
            }
            "list" => {
                let out = st::list(
                    self.app_id,
                    str_field("prefix"),
                    u64_field("limit").map(|v| v as usize),
                    str_field("cursor").map(str::to_string),
                )
                .await
                .map_err(|e| e.to_string())?;
                serde_json::to_value(out).map_err(|e| e.to_string())
            }
            "delete" => {
                // One key or a batch, mirroring the object-store idiom.
                let keys: Vec<String> = match payload.get("keys") {
                    Some(serde_json::Value::Array(items)) => items
                        .iter()
                        .filter_map(|v| v.as_str().map(str::to_string))
                        .collect(),
                    _ => vec![str_field("key").ok_or_else(|| required("key"))?.to_string()],
                };
                let deleted = st::delete(self.app_id, &keys)
                    .await
                    .map_err(|e| e.to_string())?;
                Ok(serde_json::json!({ "deleted": deleted }))
            }
            "copy" => {
                let from = str_field("fromKey").ok_or_else(|| required("fromKey"))?;
                let to = str_field("toPathname").ok_or_else(|| required("toPathname"))?;
                let out = st::copy(self.app_id, from, to, bool_field("allowOverwrite"))
                    .await
                    .map_err(|e| e.to_string())?;
                serde_json::to_value(out).map_err(|e| e.to_string())
            }
            other => Err(format!("ctx.storage: unknown op '{other}'")),
        }
    }
}

/// Normalize an author-supplied `encoding` field. Absent, empty, and
/// whitespace-only all mean "unspecified" → `default`, so `ctx.fetch`,
/// `ctx.storage`, and `ctx.email.send` attachments all read it the same way:
/// a stray space shouldn't behave differently per API, and `?? ""` in JS
/// shouldn't be a distinct error from omitting the field.
fn encoding_or<'a>(value: Option<&'a str>, default: &'a str) -> &'a str {
    value
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(default)
}

/// Decode a base64 `ctx.storage` payload, tolerating whitespace/newlines a caller
/// may have wrapped it in. base64 is the only way binary can cross the isolate's
/// JSON op boundary.
fn decode_base64(s: &str) -> Result<Vec<u8>, base64::DecodeError> {
    use base64::Engine as _;
    let compact: String = s.chars().filter(|c| !c.is_whitespace()).collect();
    base64::engine::general_purpose::STANDARD
        .decode(&compact)
        .or_else(|_| base64::engine::general_purpose::STANDARD_NO_PAD.decode(&compact))
}

fn encode_base64(bytes: &[u8]) -> String {
    use base64::Engine as _;
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

/// Hard ceiling for a host DB op. The runtime's per-function timeout (≤300s) is
/// the primary bound, but on the timeout-grace path the isolate thread is
/// *detached* on the premise that each host op is individually bounded — only
/// true if the underlying call can't hang forever. This wraps the DB paths most
/// likely to hang: connector construction (`connect`, i.e. TLS/auth handshake),
/// query execution, and warehouse `execute_statement`. NOT yet wrapped: the
/// semantic compile (`resolve_and_compile`, a `spawn_blocking`) and the Airway
/// run seed — a hang in either can still outlive the backstop (follow-up). Set
/// above the max function timeout so it only fires as a backstop, never trips a
/// legitimate long `airwayStep`.
const HOST_DB_OP_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(330);

/// Max `ctx.email.send` calls per function invocation — bounds email fan-out
/// from a single run. The per-send recipient cap lives in `AppEmailer`.
const MAX_EMAILS_PER_INVOCATION: usize = 20;

async fn with_db_timeout<T>(
    label: &str,
    fut: impl std::future::Future<Output = Result<T, String>>,
) -> Result<T, String> {
    match tokio::time::timeout(HOST_DB_OP_TIMEOUT, fut).await {
        Ok(r) => r,
        Err(_) => Err(format!(
            "{label} exceeded the {}s host DB-op ceiling",
            HOST_DB_OP_TIMEOUT.as_secs()
        )),
    }
}

/// The connector's `ResultCap` already bounds a query's *raw* result in-pod
/// (≈256 MB), but that payload is then re-encoded to JSON, shipped over the
/// broker channel, and parsed into the V8 isolate heap (~3× the bytes) — and
/// for `ctx.query` it ultimately lands in a browser. Reject a result whose JSON
/// exceeds this budget with a clear, actionable error rather than spiking
/// isolate memory. (`ctx.queryStream` is the path for genuinely large scans; it
/// stays bounded by the connector `ResultCap` since ETL data must flow through.)
const FUNCTION_QUERY_MAX_BYTES: usize = 64 * 1024 * 1024;

/// Cheap estimate of a value's JSON byte size — walks the tree once with no
/// allocation, so the byte cap doesn't need a throwaway full serialization (the
/// runtime serializes the value for real afterward; measuring with `to_vec`
/// would double-encode every result).
fn approx_json_bytes(v: &serde_json::Value) -> usize {
    match v {
        serde_json::Value::Null => 4,
        serde_json::Value::Bool(_) => 5,
        serde_json::Value::Number(_) => 20,
        serde_json::Value::String(s) => s.len() + 2,
        serde_json::Value::Array(a) => 2 + a.len() + a.iter().map(approx_json_bytes).sum::<usize>(),
        serde_json::Value::Object(o) => {
            2 + o.len()
                + o.iter()
                    .map(|(k, val)| k.len() + 4 + approx_json_bytes(val))
                    .sum::<usize>()
        }
    }
}

fn enforce_result_byte_cap(result: &serde_json::Value) -> Result<(), String> {
    let bytes = approx_json_bytes(result);
    if bytes > FUNCTION_QUERY_MAX_BYTES {
        return Err(format!(
            "query result too large (~{bytes} bytes > {FUNCTION_QUERY_MAX_BYTES} byte cap); \
             narrow the query (fewer columns / a tighter filter), aggregate in SQL, or use \
             ctx.queryStream"
        ));
    }
    Ok(())
}

/// Run the injected query executor with a one-row overfetch so `truncated` is
/// reported correctly: a result of exactly `max_rows` rows is NOT truncated,
/// but `exec.execute(.., max_rows)` alone can't distinguish "exactly
/// `max_rows` rows exist" from "there were more and the LIMIT cut it off".
/// Fetching `max_rows + 1` and trimming the extra row resolves the ambiguity.
async fn query_with_truncation(
    exec: &Arc<dyn FunctionQueryExecutor>,
    connector: Arc<dyn DatabaseConnector>,
    sql: &str,
    max_rows: usize,
) -> Result<(Vec<serde_json::Value>, bool), String> {
    let mut rows = with_db_timeout("query", exec.execute(connector, sql, max_rows + 1)).await?;
    let truncated = rows.len() > max_rows;
    rows.truncate(max_rows);
    Ok((rows, truncated))
}

/// Build an `INSERT INTO table (cols) VALUES (...), (...)` statement from
/// `{ table, rows: [{col: value, ...}, ...], conflictColumns?: [...] }`.
/// `upsert` additionally appends `ON CONFLICT (conflictColumns) DO UPDATE SET
/// col = EXCLUDED.col` for every non-key column — the Postgres/DuckDB
/// upsert syntax, which covers the destinations this is scoped to (§11.3).
fn build_insert_sql(payload: &serde_json::Value, upsert: bool) -> Result<String, String> {
    let table = payload
        .get("table")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "warehouse write: `table` is required".to_string())?;
    let rows = payload
        .get("rows")
        .and_then(|v| v.as_array())
        .filter(|rows| !rows.is_empty())
        .ok_or_else(|| "warehouse write: `rows` must be a non-empty array".to_string())?;

    let first = rows[0]
        .as_object()
        .ok_or_else(|| "warehouse write: each row must be an object".to_string())?;
    let columns: Vec<String> = first.keys().cloned().collect();

    let mut values_sql = Vec::with_capacity(rows.len());
    for row in rows {
        let obj = row
            .as_object()
            .ok_or_else(|| "warehouse write: each row must be an object".to_string())?;
        let mut literals = Vec::with_capacity(columns.len());
        for col in &columns {
            let value = obj
                .get(col)
                .ok_or_else(|| format!("warehouse write: row missing column '{col}'"))?;
            literals.push(json_value_to_sql_literal(value));
        }
        values_sql.push(format!("({})", literals.join(", ")));
    }

    let quoted_columns: Vec<String> = columns.iter().map(|c| quote_ident(c)).collect();
    let mut sql = format!(
        "INSERT INTO {} ({}) VALUES {}",
        quote_ident(table),
        quoted_columns.join(", "),
        values_sql.join(", ")
    );

    if upsert {
        let conflict_columns: Vec<String> = payload
            .get("conflictColumns")
            .and_then(|v| v.as_array())
            .map(|cols| {
                cols.iter()
                    .filter_map(|c| c.as_str().map(quote_ident))
                    .collect()
            })
            .unwrap_or_default();
        if conflict_columns.is_empty() {
            return Err("warehouse.upsert: `conflictColumns` must be a non-empty array".into());
        }
        let conflict_set: std::collections::HashSet<&str> = payload["conflictColumns"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|c| c.as_str())
            .collect();
        let update_assignments: Vec<String> = columns
            .iter()
            .filter(|c| !conflict_set.contains(c.as_str()))
            .map(|c| {
                let q = quote_ident(c);
                format!("{q} = EXCLUDED.{q}")
            })
            .collect();
        sql.push_str(&format!(" ON CONFLICT ({}) ", conflict_columns.join(", ")));
        if update_assignments.is_empty() {
            sql.push_str("DO NOTHING");
        } else {
            sql.push_str(&format!("DO UPDATE SET {}", update_assignments.join(", ")));
        }
    }

    Ok(sql)
}

/// Double-quote an identifier, escaping embedded `"` (Postgres/DuckDB/
/// ANSI-style quoting — the destinations `ctx.warehouse` targets).
fn quote_ident(ident: &str) -> String {
    format!("\"{}\"", ident.replace('"', "\"\""))
}

/// Render a JSON value as a SQL literal. Strings are single-quote escaped;
/// objects/arrays are serialized to a JSON string literal (for `jsonb`-typed
/// columns).
fn json_value_to_sql_literal(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::Null => "NULL".to_string(),
        serde_json::Value::Bool(b) => b.to_string(),
        serde_json::Value::Number(n) => n.to_string(),
        serde_json::Value::String(s) => format!("'{}'", s.replace('\'', "''")),
        serde_json::Value::Array(_) | serde_json::Value::Object(_) => {
            format!("'{}'", value.to_string().replace('\'', "''"))
        }
    }
}

pub fn into_arc(host: ProjectFunctionHost) -> Arc<dyn FunctionHost> {
    Arc::new(host)
}

/// First-layer SSRF check for `ctx.fetch` — rejects non-HTTPS, loopback,
/// private, link-local, and internal-suffix hosts *by inspecting the URL
/// string only*. A non-literal hostname that resolves to a private/internal
/// address (DNS rebinding) passes this check; that case is caught at connect
/// time by [`PublicOnlyDnsResolver`]. Mirrors `custom_apps_proxy`'s
/// `is_safe_upstream` (kept as a local copy to avoid widening that module's
/// API surface; the two should stay in sync).
fn is_safe_outbound(url: &reqwest::Url) -> bool {
    if url.scheme() != "https" {
        return false;
    }
    let Some(host) = url.host_str() else {
        return false;
    };
    let host_for_ip = host
        .strip_prefix('[')
        .and_then(|s| s.strip_suffix(']'))
        .unwrap_or(host);
    if let Ok(ip) = host_for_ip.parse::<IpAddr>() {
        return is_public_ip(&ip);
    }
    let lower = host.to_ascii_lowercase();
    if matches!(
        lower.as_str(),
        "localhost" | "ip6-localhost" | "ip6-loopback"
    ) {
        return false;
    }
    const INTERNAL_SUFFIXES: &[&str] = &[
        ".internal",
        ".local",
        ".localdomain",
        ".svc",
        ".cluster.local",
    ];
    !INTERNAL_SUFFIXES.iter().any(|s| lower.ends_with(s))
}

fn is_public_ip(ip: &IpAddr) -> bool {
    // Fold an IPv4-mapped IPv6 address (`::ffff:a.b.c.d`) back to IPv4 before
    // classifying. Otherwise `::ffff:169.254.169.254` (cloud metadata),
    // `::ffff:10.x`, `::ffff:127.0.0.1` have `segments()[0] == 0` and slip
    // through the v6 arm below as "public" — and Linux routes a v4-mapped
    // address on a v6 socket straight to the underlying internal IPv4.
    // `to_canonical()` (stable since 1.75) makes the v4 checks below apply.
    let ip = ip.to_canonical();
    if ip.is_loopback() || ip.is_unspecified() {
        return false;
    }
    match ip {
        IpAddr::V4(v4) => {
            !v4.is_private() && !v4.is_link_local() && !v4.is_broadcast() && !v4.is_documentation()
        }
        IpAddr::V6(v6) => {
            // NAT64 (`64:ff9b::/96`) and IPv4-compatible (`::/96`) addresses
            // embed a v4 address in the low 32 bits. `to_canonical()` folds
            // v4-*mapped* (`::ffff:a.b.c.d`) but NOT these, so an id like
            // `64:ff9b::169.254.169.254` (or `::10.0.0.5`) would otherwise pass
            // the v6 arm below as "public" while pointing at metadata / RFC1918
            // — reachable on any host with a NAT64 path. Re-run the v4 checks on
            // the embedded address instead of trusting the v6 prefix.
            if let Some(v4) = embedded_ipv4(&v6) {
                return is_public_ip(&IpAddr::V4(v4));
            }
            let seg = v6.segments()[0];
            let link_local = (seg & 0xffc0) == 0xfe80;
            let unique_local = (seg & 0xfe00) == 0xfc00;
            !link_local && !unique_local
        }
    }
}

/// Extract the IPv4 address embedded in an IPv4-compatible (`::/96`) or NAT64
/// (`64:ff9b::/96`) IPv6 address. Returns `None` for every other v6 address.
///
/// v4-*mapped* (`::ffff:0:0/96`) is deliberately excluded: [`is_public_ip`]
/// calls `to_canonical()` first, which already folds it to a real `IpAddr::V4`,
/// so it never reaches here as a v6 value. Both prefixes handled here are
/// non-global by design (IPv4-compatible is deprecated; NAT64 is a translation
/// prefix), so treating anything in them as an embedded v4 cannot mis-block a
/// legitimately-routable global v6 address.
fn embedded_ipv4(v6: &std::net::Ipv6Addr) -> Option<std::net::Ipv4Addr> {
    let seg = v6.segments();
    let is_v4_compatible = seg[0..6] == [0, 0, 0, 0, 0, 0];
    let is_nat64 = seg[0] == 0x0064 && seg[1] == 0xff9b && seg[2..6] == [0, 0, 0, 0];
    if is_v4_compatible || is_nat64 {
        Some(std::net::Ipv4Addr::from(
            ((seg[6] as u32) << 16) | seg[7] as u32,
        ))
    } else {
        None
    }
}

/// Partition resolved socket addresses into the ones safe to connect to,
/// dropping any that point at a non-public IP. Factored out of the resolver so
/// the rebinding decision is unit-testable without real DNS. Returns `Err` with
/// a diagnostic when *every* resolved address is filtered out (the rebinding
/// case), so the caller surfaces a clear message instead of an opaque DNS miss.
fn keep_public_addrs(host: &str, resolved: Vec<SocketAddr>) -> Result<Vec<SocketAddr>, String> {
    let safe: Vec<SocketAddr> = resolved
        .into_iter()
        .filter(|addr| is_public_ip(&addr.ip()))
        .collect();
    if safe.is_empty() {
        return Err(format!("'{host}' resolves only to non-public addresses"));
    }
    Ok(safe)
}

/// Second-layer SSRF defense for `ctx.fetch`: a custom reqwest DNS resolver
/// that drops every resolved address pointing at a non-public IP. This closes
/// the DNS-rebinding vector [`is_safe_outbound`] can't see — a public hostname
/// whose A record resolves to `169.254.169.254` / `10.x` / `127.x`. Validation
/// happens at resolution (i.e. connect) time, so there is no resolve-then-
/// reconnect TOCTOU window, and reqwest connects only to an address we vetted.
#[derive(Debug)]
struct PublicOnlyDnsResolver;

impl reqwest::dns::Resolve for PublicOnlyDnsResolver {
    fn resolve(&self, name: reqwest::dns::Name) -> reqwest::dns::Resolving {
        Box::pin(async move {
            let host = name.as_str().to_owned();
            // Port 0: reqwest overrides it with the URL's port; we only care
            // about the resolved IPs here.
            let resolved: Vec<SocketAddr> = tokio::net::lookup_host((host.as_str(), 0u16))
                .await
                .map_err(|e| -> Box<dyn std::error::Error + Send + Sync> { Box::new(e) })?
                .collect();
            let safe = keep_public_addrs(&host, resolved)
                .map_err(|e| -> Box<dyn std::error::Error + Send + Sync> { e.into() })?;
            let addrs: reqwest::dns::Addrs = Box::new(safe.into_iter());
            Ok(addrs)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // ── build_insert_sql / quote_ident / json_value_to_sql_literal ─────────

    #[test]
    fn quote_ident_escapes_embedded_quotes() {
        assert_eq!(quote_ident("orders"), "\"orders\"");
        assert_eq!(quote_ident(r#"weird"col"#), "\"weird\"\"col\"");
    }

    #[test]
    fn json_value_to_sql_literal_covers_all_variants() {
        assert_eq!(json_value_to_sql_literal(&json!(null)), "NULL");
        assert_eq!(json_value_to_sql_literal(&json!(true)), "true");
        assert_eq!(json_value_to_sql_literal(&json!(42)), "42");
        assert_eq!(json_value_to_sql_literal(&json!(4.5)), "4.5");
        assert_eq!(json_value_to_sql_literal(&json!("plain")), "'plain'");
        assert_eq!(json_value_to_sql_literal(&json!("it's")), "'it''s'");
        // Arrays/objects round-trip through JSON with single quotes escaped.
        assert_eq!(
            json_value_to_sql_literal(&json!({"a": "b's"})),
            "'{\"a\":\"b''s\"}'"
        );
    }

    #[test]
    fn build_insert_sql_basic_insert() {
        let payload = json!({
            "table": "daily_rollup",
            "rows": [
                { "day": "2026-06-13", "store_id": 12, "total": 4821.5 },
            ],
        });
        let sql = build_insert_sql(&payload, false).unwrap();
        assert_eq!(
            sql,
            r#"INSERT INTO "daily_rollup" ("day", "store_id", "total") VALUES ('2026-06-13', 12, 4821.5)"#
        );
    }

    #[test]
    fn build_insert_sql_multi_row() {
        let payload = json!({
            "table": "t",
            "rows": [
                { "a": 1, "b": "x" },
                { "a": 2, "b": "y" },
            ],
        });
        let sql = build_insert_sql(&payload, false).unwrap();
        assert_eq!(
            sql,
            r#"INSERT INTO "t" ("a", "b") VALUES (1, 'x'), (2, 'y')"#
        );
    }

    #[test]
    fn build_insert_sql_requires_table() {
        let payload = json!({ "rows": [{ "a": 1 }] });
        let err = build_insert_sql(&payload, false).unwrap_err();
        assert!(err.contains("`table`"));
    }

    #[test]
    fn build_insert_sql_requires_non_empty_rows() {
        let payload = json!({ "table": "t", "rows": [] });
        let err = build_insert_sql(&payload, false).unwrap_err();
        assert!(err.contains("`rows`"));
    }

    #[test]
    fn build_insert_sql_requires_consistent_columns() {
        let payload = json!({
            "table": "t",
            "rows": [
                { "a": 1, "b": 2 },
                { "a": 1 },
            ],
        });
        let err = build_insert_sql(&payload, false).unwrap_err();
        assert!(err.contains("missing column 'b'"));
    }

    #[test]
    fn build_insert_sql_upsert_appends_on_conflict() {
        let payload = json!({
            "table": "daily_rollup",
            "rows": [{ "day": "2026-06-13", "store_id": 12, "total": 4821.5 }],
            "conflictColumns": ["day", "store_id"],
        });
        let sql = build_insert_sql(&payload, true).unwrap();
        assert_eq!(
            sql,
            r#"INSERT INTO "daily_rollup" ("day", "store_id", "total") VALUES ('2026-06-13', 12, 4821.5) ON CONFLICT ("day", "store_id") DO UPDATE SET "total" = EXCLUDED."total""#
        );
    }

    #[test]
    fn build_insert_sql_upsert_all_columns_in_conflict_does_nothing() {
        let payload = json!({
            "table": "t",
            "rows": [{ "a": 1, "b": 2 }],
            "conflictColumns": ["a", "b"],
        });
        let sql = build_insert_sql(&payload, true).unwrap();
        assert!(sql.ends_with("DO NOTHING"));
    }

    #[test]
    fn build_insert_sql_upsert_requires_conflict_columns() {
        let payload = json!({
            "table": "t",
            "rows": [{ "a": 1 }],
        });
        let err = build_insert_sql(&payload, true).unwrap_err();
        assert!(err.contains("conflictColumns"));
    }

    // ── is_safe_outbound / is_public_ip (ctx.fetch SSRF allowlist) ──────────

    fn url(s: &str) -> reqwest::Url {
        reqwest::Url::parse(s).unwrap()
    }

    #[test]
    fn rejects_non_https() {
        assert!(!is_safe_outbound(&url("http://example.com")));
    }

    #[test]
    fn allows_public_https_host() {
        assert!(is_safe_outbound(&url("https://api.example.com/v1")));
    }

    #[test]
    fn rejects_loopback_and_private_hosts() {
        assert!(!is_safe_outbound(&url("https://localhost/")));
        assert!(!is_safe_outbound(&url("https://127.0.0.1/")));
        assert!(!is_safe_outbound(&url("https://10.0.0.5/")));
        assert!(!is_safe_outbound(&url("https://192.168.1.1/")));
        assert!(!is_safe_outbound(&url("https://[::1]/")));
    }

    #[test]
    fn rejects_internal_suffix_hosts() {
        assert!(!is_safe_outbound(&url("https://service.internal/")));
        assert!(!is_safe_outbound(&url("https://db.svc/")));
        assert!(!is_safe_outbound(&url("https://app.cluster.local/")));
        assert!(!is_safe_outbound(&url("https://box.localdomain/")));
    }

    #[test]
    fn allows_public_ip_literal() {
        assert!(is_safe_outbound(&url("https://93.184.216.34/")));
    }

    #[test]
    fn rejects_link_local_ip() {
        assert!(!is_safe_outbound(&url("https://169.254.1.1/")));
    }

    #[test]
    fn rejects_ipv4_mapped_ipv6_internal_literals() {
        // ::ffff:a.b.c.d must be canonicalized to IPv4 before classification —
        // otherwise the metadata endpoint (and RFC1918 / loopback) reach the
        // internal target via a v6-socket-routed v4-mapped address.
        assert!(!is_safe_outbound(&url(
            "https://[::ffff:169.254.169.254]/latest/meta-data/"
        )));
        assert!(!is_safe_outbound(&url("https://[::ffff:10.0.0.5]/")));
        assert!(!is_safe_outbound(&url("https://[::ffff:127.0.0.1]/")));
        // The primitive directly: mapped-internal is rejected, mapped-public
        // still allowed.
        assert!(!is_public_ip(
            &"::ffff:169.254.169.254".parse::<IpAddr>().unwrap()
        ));
        assert!(is_public_ip(
            &"::ffff:93.184.216.34".parse::<IpAddr>().unwrap()
        ));
    }

    #[test]
    fn nat64_and_v4_compatible_embedded_internal_ips_are_blocked() {
        // NAT64 (`64:ff9b::/96`) and IPv4-compatible (`::/96`) addresses embed a
        // v4 in the low 32 bits and are NOT folded by `to_canonical()`, so the
        // embedded v4 must be re-checked. An embedded metadata / RFC1918 / loopback
        // address must be rejected; an embedded public one still allowed.
        for internal in [
            "64:ff9b::169.254.169.254", // NAT64 → cloud metadata
            "64:ff9b::10.0.0.5",        // NAT64 → RFC1918
            "64:ff9b::127.0.0.1",       // NAT64 → loopback
            "::169.254.169.254",        // IPv4-compatible → cloud metadata
            "::10.0.0.5",               // IPv4-compatible → RFC1918
        ] {
            let ip = internal.parse::<IpAddr>().unwrap();
            assert!(!is_public_ip(&ip), "{internal} must be classed non-public");
            assert!(
                !is_safe_outbound(&url(&format!("https://[{internal}]/latest/meta-data/"))),
                "{internal} must be blocked by is_safe_outbound"
            );
        }
        // An embedded PUBLIC v4 behind NAT64 is still reachable.
        assert!(is_public_ip(
            &"64:ff9b::93.184.216.34".parse::<IpAddr>().unwrap()
        ));
    }

    // ── keep_public_addrs (DNS-rebinding defense) ───────────────────────────

    fn sa(s: &str) -> SocketAddr {
        s.parse().unwrap()
    }

    #[test]
    fn keep_public_addrs_filters_private_and_metadata() {
        // A "public" hostname that resolves to a mix keeps only the public IP.
        let resolved = vec![sa("169.254.169.254:0"), sa("93.184.216.34:0")];
        let kept = keep_public_addrs("evil.example.com", resolved).unwrap();
        assert_eq!(kept, vec![sa("93.184.216.34:0")]);
    }

    #[test]
    fn keep_public_addrs_rejects_when_all_private() {
        // Pure rebinding: every resolved address is internal → hard error.
        let resolved = vec![sa("169.254.169.254:0"), sa("10.0.0.5:0"), sa("127.0.0.1:0")];
        let err = keep_public_addrs("evil.example.com", resolved).unwrap_err();
        assert!(err.contains("non-public"), "unexpected error: {err}");
    }

    #[test]
    fn keep_public_addrs_passes_all_public() {
        let resolved = vec![sa("93.184.216.34:0"), sa("1.1.1.1:0")];
        let kept = keep_public_addrs("api.example.com", resolved.clone()).unwrap();
        assert_eq!(kept, resolved);
    }
}
