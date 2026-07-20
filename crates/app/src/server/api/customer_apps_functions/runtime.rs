//! V8-isolate execution for Oxy Functions.
//!
//! See `internal-docs/2026-06-12-customer-apps-functions-design.md` §4 and
//! §11. Given a bundled function artifact (esbuild ESM output) and a `ctx`
//! payload, run `export default async (req, ctx) => Response` to completion
//! and return the resulting status/body.
//!
//! ## Why a dedicated thread + channel broker
//!
//! `deno_core::JsRuntime` owns a V8 isolate and is `!Send` — it cannot be
//! held across `.await` in the `Send` axum handler future, nor moved between
//! tokio workers. So the isolate runs on its **own OS thread** with a
//! current-thread tokio runtime, and every `ctx.*` host call is bridged to
//! the async host side (DB connectors, outbound fetch) over an mpsc channel.
//! The handler future only ever holds channel endpoints + a join handle, all
//! of which are `Send`.
//!
//! ```text
//!   handler (Send)            isolate thread (!Send)
//!   ─────────────             ──────────────────────
//!   broker loop  <── HostCall ── op_ctx_query / op_ctx_fetch
//!        │  host.query(sql).await
//!        └────────── oneshot reply ─────────►  resolves the JS promise
//! ```
//!
//! `ctx.semantic.query` (airlayer), `ctx.airway.run` (Airway runner), and
//! `ctx.warehouse.{insert,exec,upsert}` (project-database allowlist, §11.3)
//! are all wired to real backends. `ctx.queryStream` (§11.5) fetches up to
//! `FUNCTION_STREAM_MAX_ROWS` rows in one host call and yields them to the
//! function as an async generator in client-side batches — a pragmatic MVP
//! pending a true warehouse-cursor implementation.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use deno_core::{JsRuntime, OpState, RuntimeOptions, op2};
use deno_error::JsErrorBox;
use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc, oneshot};

/// Per-invocation context handed to the isolate. Built fresh for every
/// invocation from the resolved identity (design doc §11.7) — never cached
/// across invocations.
#[derive(Debug, Clone, Serialize)]
pub struct InvocationCtx {
    pub user: CtxUser,
    pub env: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CtxUser {
    pub id: String,
    pub email: String,
    pub org_id: String,
}

/// Result of running a function to completion.
#[derive(Debug, Deserialize)]
pub struct FnResponse {
    #[serde(default = "default_status")]
    pub status: u16,
    #[serde(default)]
    pub body: String,
}

fn default_status() -> u16 {
    200
}

/// Errors surfaced to the route handler as SSE `event: error` frames.
#[derive(Debug, thiserror::Error)]
pub enum RuntimeError {
    #[error("function threw: {0}")]
    Js(String),
    #[error("function was cancelled")]
    Cancelled,
    #[error("function execution timed out")]
    Timeout,
    #[error("internal runtime error: {0}")]
    Internal(String),
}

/// Host-side data plane the isolate calls back into. Implemented in
/// `mod.rs` against the resolved project context (connectors) + an HTTP
/// client. `Send + Sync` so the broker loop can `Arc`-clone it per call.
#[async_trait::async_trait]
pub trait FunctionHost: Send + Sync {
    /// `ctx.query(sql)` — read-only SQL, function-scoped row cap. Returns
    /// the rows as a JSON array value.
    async fn query(&self, sql: String) -> Result<serde_json::Value, String>;
    /// `ctx.queryStream(sql)` — read-only SQL with a higher row cap
    /// (`FUNCTION_STREAM_MAX_ROWS`) for large scans. Returns the rows as a
    /// JSON array value; the isolate yields them to the function in batches.
    async fn query_stream(&self, sql: String) -> Result<serde_json::Value, String>;
    /// `ctx.fetch(url, init)` — SSRF-allowlisted outbound HTTP with a
    /// response size cap. Returns `{ status, body }`.
    async fn fetch(
        &self,
        url: String,
        init: serde_json::Value,
    ) -> Result<serde_json::Value, String>;
    /// `ctx.semantic.query(spec)` — airlayer-compiled semantic query.
    /// `spec` is the JSON-encoded `agentic_semantic::config::SemanticQueryConfig`.
    async fn semantic_query(&self, spec: serde_json::Value) -> Result<serde_json::Value, String>;
    /// `ctx.airway.run(pipelineRef, variables)` — seed an Airway ELT run.
    /// Returns `{ runId }`; the run is driven asynchronously by the worker
    /// fleet (it does not block on completion — ELT runs routinely exceed
    /// the function timeout ceiling).
    async fn airway_run(
        &self,
        pipeline_ref: String,
        variables: serde_json::Value,
    ) -> Result<serde_json::Value, String>;
    /// `ctx.warehouse.{insert,exec,upsert}` — write to one of the app's
    /// configured destination databases. `op` is `"insert"`, `"exec"`, or
    /// `"upsert"`; `payload` carries `{ database, table?, rows?, sql? }`
    /// depending on `op`. Validated against the project's configured
    /// databases (§11.3) before execution.
    async fn warehouse_write(
        &self,
        op: String,
        payload: serde_json::Value,
    ) -> Result<serde_json::Value, String>;
    /// `ctx.secrets.set(key, value)` — upsert an app-scoped secret
    /// (`apps/<app_id>/<key>`) into the same namespace `ctx.env` reads. Gated
    /// by the fail-closed `secrets.write` manifest capability.
    async fn secrets_set(&self, key: String, value: String) -> Result<serde_json::Value, String>;
    /// `ctx.email.send(input)` — send email on behalf of the app. Gated by the
    /// fail-closed `email.send` manifest capability; the platform controls the
    /// `from` address (the author may set `replyTo` only). `input` is the JS
    /// payload object; returns `{ messageId }`.
    async fn send_email(&self, input: serde_json::Value) -> Result<serde_json::Value, String>;
}

/// A request the isolate sends to the broker loop.
enum HostCall {
    Query {
        sql: String,
        reply: oneshot::Sender<Result<serde_json::Value, String>>,
    },
    QueryStream {
        sql: String,
        reply: oneshot::Sender<Result<serde_json::Value, String>>,
    },
    Fetch {
        url: String,
        init: serde_json::Value,
        reply: oneshot::Sender<Result<serde_json::Value, String>>,
    },
    SemanticQuery {
        spec: serde_json::Value,
        reply: oneshot::Sender<Result<serde_json::Value, String>>,
    },
    AirwayRun {
        pipeline_ref: String,
        variables: serde_json::Value,
        reply: oneshot::Sender<Result<serde_json::Value, String>>,
    },
    WarehouseWrite {
        op: String,
        payload: serde_json::Value,
        reply: oneshot::Sender<Result<serde_json::Value, String>>,
    },
    SecretsSet {
        key: String,
        value: String,
        reply: oneshot::Sender<Result<serde_json::Value, String>>,
    },
    SendEmail {
        input: serde_json::Value,
        reply: oneshot::Sender<Result<serde_json::Value, String>>,
    },
}

// ── ctx ops ──────────────────────────────────────────────────────────────

use super::LogLine;

/// Per-invocation log buffer, shared between the isolate thread (appends via
/// `op_ctx_log`) and the caller (drains it after the run). Newtype so it's
/// uniquely addressable in `OpState`; capped so a runaway loop can't OOM.
pub(super) struct FunctionLogs(pub Arc<std::sync::Mutex<Vec<LogLine>>>);
const MAX_CAPTURED_LOGS: usize = 500;

// Still `(fast)`: the JS-visible args are primitives (no return), so deno_core
// 0.331 requires `(fast)` — and a fast op may take `&mut OpState` as a leading
// special arg to reach the shared log buffer.
#[op2(fast)]
fn op_ctx_log(state: &mut OpState, #[string] level: &str, #[string] message: &str) {
    match level {
        "warn" => tracing::warn!(target: "customer_app_function", "{message}"),
        "error" => tracing::error!(target: "customer_app_function", "{message}"),
        _ => tracing::info!(target: "customer_app_function", "{message}"),
    }
    if let Some(FunctionLogs(buf)) = state.try_borrow::<FunctionLogs>()
        && let Ok(mut v) = buf.lock()
        && v.len() < MAX_CAPTURED_LOGS
    {
        v.push(LogLine {
            level: level.to_string(),
            message: message.to_string(),
        });
    }
}

#[op2(async)]
#[string]
async fn op_ctx_query(
    state: Rc<RefCell<OpState>>,
    #[string] sql: String,
) -> Result<String, JsErrorBox> {
    check_cancelled(&state)?;
    let tx = state
        .borrow()
        .borrow::<mpsc::UnboundedSender<HostCall>>()
        .clone();
    let (reply, rx) = oneshot::channel();
    tx.send(HostCall::Query { sql, reply })
        .map_err(|_| JsErrorBox::generic("function host unavailable"))?;
    let result = rx
        .await
        .map_err(|_| JsErrorBox::generic("function host dropped the request"))?;
    Ok(reply_json("ctx.query", result))
}

#[op2(async)]
#[string]
async fn op_ctx_query_stream(
    state: Rc<RefCell<OpState>>,
    #[string] sql: String,
) -> Result<String, JsErrorBox> {
    check_cancelled(&state)?;
    let tx = state
        .borrow()
        .borrow::<mpsc::UnboundedSender<HostCall>>()
        .clone();
    let (reply, rx) = oneshot::channel();
    tx.send(HostCall::QueryStream { sql, reply })
        .map_err(|_| JsErrorBox::generic("function host unavailable"))?;
    let result = rx
        .await
        .map_err(|_| JsErrorBox::generic("function host dropped the request"))?;
    Ok(reply_json("ctx.queryStream", result))
}

#[op2(async)]
#[string]
async fn op_ctx_fetch(
    state: Rc<RefCell<OpState>>,
    #[string] url: String,
    #[string] init_json: String,
) -> Result<String, JsErrorBox> {
    check_cancelled(&state)?;
    let tx = state
        .borrow()
        .borrow::<mpsc::UnboundedSender<HostCall>>()
        .clone();
    let init: serde_json::Value =
        serde_json::from_str(&init_json).unwrap_or(serde_json::Value::Null);
    let (reply, rx) = oneshot::channel();
    tx.send(HostCall::Fetch { url, init, reply })
        .map_err(|_| JsErrorBox::generic("function host unavailable"))?;
    let result = rx
        .await
        .map_err(|_| JsErrorBox::generic("function host dropped the request"))?;
    Ok(reply_json("ctx.fetch", result))
}

#[op2(async)]
#[string]
async fn op_ctx_warehouse(
    state: Rc<RefCell<OpState>>,
    #[string] op: String,
    #[string] payload_json: String,
) -> Result<String, JsErrorBox> {
    check_cancelled(&state)?;
    let tx = state
        .borrow()
        .borrow::<mpsc::UnboundedSender<HostCall>>()
        .clone();
    let payload: serde_json::Value =
        serde_json::from_str(&payload_json).unwrap_or(serde_json::Value::Null);
    let (reply, rx) = oneshot::channel();
    tx.send(HostCall::WarehouseWrite { op, payload, reply })
        .map_err(|_| JsErrorBox::generic("function host unavailable"))?;
    let result = rx
        .await
        .map_err(|_| JsErrorBox::generic("function host dropped the request"))?;
    Ok(reply_json("ctx.warehouse", result))
}

/// `ctx.secrets.set(key, value)` — bridge to `FunctionHost::secrets_set`.
#[op2(async)]
#[string]
async fn op_ctx_secrets_set(
    state: Rc<RefCell<OpState>>,
    #[string] key: String,
    #[string] value: String,
) -> Result<String, JsErrorBox> {
    check_cancelled(&state)?;
    let tx = state
        .borrow()
        .borrow::<mpsc::UnboundedSender<HostCall>>()
        .clone();
    let (reply, rx) = oneshot::channel();
    tx.send(HostCall::SecretsSet { key, value, reply })
        .map_err(|_| JsErrorBox::generic("function host unavailable"))?;
    let result = rx
        .await
        .map_err(|_| JsErrorBox::generic("function host dropped the request"))?;
    Ok(reply_json("ctx.secrets.set", result))
}

/// `ctx.email.send(input)` — bridge to `FunctionHost::send_email`. `input` is
/// the JS payload object, JSON-stringified by the bootstrap `__wrapOp`.
#[op2(async)]
#[string]
async fn op_ctx_email_send(
    state: Rc<RefCell<OpState>>,
    #[string] input_json: String,
) -> Result<String, JsErrorBox> {
    check_cancelled(&state)?;
    let tx = state
        .borrow()
        .borrow::<mpsc::UnboundedSender<HostCall>>()
        .clone();
    let input: serde_json::Value =
        serde_json::from_str(&input_json).unwrap_or(serde_json::Value::Null);
    let (reply, rx) = oneshot::channel();
    tx.send(HostCall::SendEmail { input, reply })
        .map_err(|_| JsErrorBox::generic("function host unavailable"))?;
    let result = rx
        .await
        .map_err(|_| JsErrorBox::generic("function host dropped the request"))?;
    Ok(reply_json("ctx.email.send", result))
}

#[op2(async)]
#[string]
async fn op_ctx_semantic_query(
    state: Rc<RefCell<OpState>>,
    #[string] spec_json: String,
) -> Result<String, JsErrorBox> {
    check_cancelled(&state)?;
    let tx = state
        .borrow()
        .borrow::<mpsc::UnboundedSender<HostCall>>()
        .clone();
    let spec: serde_json::Value = serde_json::from_str(&spec_json)
        .map_err(|e| JsErrorBox::generic(format!("invalid semantic query spec: {e}")))?;
    let (reply, rx) = oneshot::channel();
    tx.send(HostCall::SemanticQuery { spec, reply })
        .map_err(|_| JsErrorBox::generic("function host unavailable"))?;
    let result = rx
        .await
        .map_err(|_| JsErrorBox::generic("function host dropped the request"))?;
    Ok(reply_json("ctx.semantic.query", result))
}

#[op2(async)]
#[string]
async fn op_ctx_airway_run(
    state: Rc<RefCell<OpState>>,
    #[string] pipeline_ref: String,
    #[string] vars_json: String,
) -> Result<String, JsErrorBox> {
    check_cancelled(&state)?;
    let tx = state
        .borrow()
        .borrow::<mpsc::UnboundedSender<HostCall>>()
        .clone();
    let variables: serde_json::Value =
        serde_json::from_str(&vars_json).unwrap_or(serde_json::Value::Null);
    let (reply, rx) = oneshot::channel();
    tx.send(HostCall::AirwayRun {
        pipeline_ref,
        variables,
        reply,
    })
    .map_err(|_| JsErrorBox::generic("function host unavailable"))?;
    let result = rx
        .await
        .map_err(|_| JsErrorBox::generic("function host dropped the request"))?;
    Ok(reply_json("ctx.airway.run", result))
}

/// Second cancellation layer (design doc §11.4): checked at the entry of
/// every `ctx.*` op so an in-flight or about-to-start host call fails fast
/// once cancellation has been observed, rather than only relying on
/// `terminate_execution` (which interrupts JS execution but not an
/// already-dispatched host call).
fn check_cancelled(state: &Rc<RefCell<OpState>>) -> Result<(), JsErrorBox> {
    if state
        .borrow()
        .borrow::<Arc<AtomicBool>>()
        .load(Ordering::Relaxed)
    {
        return Err(JsErrorBox::generic("function was cancelled"));
    }
    Ok(())
}

/// Encode a host reply as the JSON envelope the bootstrap `__wrapOp` reads:
/// either the value itself, or `{ __oxyError, message }` on failure.
fn reply_json(what: &str, result: Result<serde_json::Value, String>) -> String {
    match result {
        Ok(value) => value.to_string(),
        Err(message) => serde_json::json!({
            "__oxyError": "HostError",
            "message": format!("{what}: {message}"),
        })
        .to_string(),
    }
}

deno_core::extension!(
    oxy_functions_ext,
    ops = [
        op_ctx_log,
        op_ctx_query,
        op_ctx_query_stream,
        op_ctx_fetch,
        op_ctx_warehouse,
        op_ctx_secrets_set,
        op_ctx_semantic_query,
        op_ctx_airway_run,
        op_ctx_email_send,
    ],
);

/// Bootstrap script: polyfills the minimal `Response` the function author's
/// code expects, and assembles `globalThis.__buildCtx` from the host ops.
const BOOTSTRAP_JS: &str = r#"
class OxyResponse {
  constructor(body, init) {
    this.body = body ?? "";
    this.status = (init && init.status) || 200;
    this.headers = (init && init.headers) || {};
  }
  static json(value, init) {
    return new OxyResponse(JSON.stringify(value), {
      status: (init && init.status) || 200,
      headers: Object.assign({ "content-type": "application/json" }, init && init.headers),
    });
  }
}
globalThis.Response = OxyResponse;

// Wire the console developers reach for reflexively into the same host log
// sink as ctx.log — captured per-invocation and sent back with the response.
const __fmt = (a) => {
  if (typeof a === "string") return a;
  try {
    return JSON.stringify(a ?? null);
  } catch {
    // Circular refs / BigInt / etc. — native console.log tolerates these, so a
    // format failure must never fail the whole invocation.
    return String(a);
  }
};
const __log = (level) => (...args) => Deno.core.ops.op_ctx_log(level, args.map(__fmt).join(" "));
const __noop = () => {};
globalThis.console = {
  log: __log("info"),
  info: __log("info"),
  debug: __log("info"),
  warn: __log("warn"),
  error: __log("error"),
  trace: __log("info"),
  dir: __log("info"),
  assert: (cond, ...args) => {
    if (!cond) __log("error")("Assertion failed:", ...args);
  },
  // Extras stubbed to no-ops so an author reaching for them can't throw.
  table: __noop,
  group: __noop,
  groupCollapsed: __noop,
  groupEnd: __noop,
  count: __noop,
  countReset: __noop,
  time: __noop,
  timeEnd: __noop,
  timeLog: __noop,
};

function __wrapOp(opName) {
  return async (...args) => {
    const raw = await Deno.core.ops[opName](...args.map((a) =>
      typeof a === "string" ? a : JSON.stringify(a ?? null)
    ));
    const parsed = JSON.parse(raw);
    if (parsed && parsed.__oxyError) {
      const err = new Error(parsed.message);
      err.name = parsed.__oxyError;
      throw err;
    }
    return parsed;
  };
}

globalThis.__buildCtx = (ctxData) => ({
  user: ctxData.user,
  env: ctxData.env,
  log: (...args) => Deno.core.ops.op_ctx_log("info", args.map(String).join(" ")),
  query: __wrapOp("op_ctx_query"),
  // queryStream(sql, opts?) — fetches up to FUNCTION_STREAM_MAX_ROWS rows in
  // one host call, then yields them to the caller in `opts.batchSize`-sized
  // arrays via an async generator. Not a true warehouse cursor (yet); see
  // design doc §11.5.
  queryStream: async function* (sql, opts) {
    const batchSize = (opts && opts.batchSize) || 1000;
    const rows = await __wrapOp("op_ctx_query_stream")(sql);
    for (let i = 0; i < rows.length; i += batchSize) {
      yield rows.slice(i, i + batchSize);
    }
  },
  fetch: __wrapOp("op_ctx_fetch"),
  warehouse: {
    // insert(database, table, rows) / exec(database, sql) /
    // upsert(database, table, rows, conflictColumns) — `op` stays a bare
    // string, the rest of the call is packed into a payload object that
    // __wrapOp JSON-stringifies.
    insert: (database, table, rows) =>
      __wrapOp("op_ctx_warehouse")("insert", { database, table, rows }),
    exec: (database, sql) =>
      __wrapOp("op_ctx_warehouse")("exec", { database, sql }),
    upsert: (database, table, rows, conflictColumns) =>
      __wrapOp("op_ctx_warehouse")("upsert", { database, table, rows, conflictColumns }),
  },
  secrets: {
    // set(key, value) — both stay bare strings so they arrive as the op's
    // `#[string]` args (matching op_ctx_airway_run's bare-string pipelineRef).
    set: (key, value) => __wrapOp("op_ctx_secrets_set")(String(key), String(value)),
  },
  email: {
    // send(input) — input is an object; __wrapOp JSON-stringifies it. The host
    // controls `from` (author sets replyTo only). Render templates to `html`
    // with `render` from @oxy-hq/sdk/email before calling this.
    send: (input) => __wrapOp("op_ctx_email_send")(input),
  },
  semantic: { query: __wrapOp("op_ctx_semantic_query") }, // spec is JSON-stringified by __wrapOp
  airway: {
    // run(pipelineRef, variables?) — pipelineRef stays a bare string,
    // variables is JSON-stringified by __wrapOp.
    run: (pipelineRef, variables) =>
      __wrapOp("op_ctx_airway_run")(String(pipelineRef), variables ?? null),
  },
});
"#;

/// How long to wait for the isolate thread to actually exit after a wall-clock
/// timeout (or cancel) has terminated execution, before giving up and returning
/// `Timeout` regardless. Bounds the worst case where the isolate is parked in a
/// not-yet-returned host call that `terminate_execution` can't interrupt.
const TIMEOUT_GRACE: std::time::Duration = std::time::Duration::from_secs(5);

/// Run `export default async (req, ctx) => Response` from a bundled ESM
/// artifact to completion, bridging `ctx.*` calls to `host`.
///
/// `cancel` resolving (the client disconnected, or the dashboard cancel
/// flag was observed) terminates the isolate promptly via
/// `terminate_execution`. `timeout` is enforced here (not by the caller
/// wrapping this future in `tokio::time::timeout`): on elapse the isolate is
/// terminated the same way as a cancel, and we wait up to [`TIMEOUT_GRACE`]
/// for the isolate thread to actually exit before returning — so in the common
/// case the OS thread + V8 isolate never outlive this call. If the isolate is
/// wedged in a not-yet-returned host call (which `terminate_execution` cannot
/// interrupt), we return `Timeout` after the grace period and let that thread
/// unwind on its own once the (individually bounded) host op completes.
pub async fn run(
    artifact_js: String,
    ctx: InvocationCtx,
    req_body: Vec<u8>,
    host: std::sync::Arc<dyn FunctionHost>,
    mut cancel: oneshot::Receiver<()>,
    timeout: std::time::Duration,
    // Shared with the caller: the isolate appends `console.*`/`ctx.log` here and
    // the caller drains it after (surfaced back to the app, not just tracing).
    logs: Arc<std::sync::Mutex<Vec<LogLine>>>,
) -> Result<FnResponse, RuntimeError> {
    let (call_tx, mut call_rx) = mpsc::unbounded_channel::<HostCall>();
    let (done_tx, done_rx) = oneshot::channel::<Result<FnResponse, RuntimeError>>();
    let (handle_tx, handle_rx) = oneshot::channel::<deno_core::v8::IsolateHandle>();
    let cancelled = Arc::new(AtomicBool::new(false));

    // Isolate runs on its own thread with a current-thread runtime.
    let thread = std::thread::Builder::new()
        .name("oxy-function".into())
        .spawn({
            let cancelled = cancelled.clone();
            move || {
                let rt = match tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                {
                    Ok(rt) => rt,
                    Err(e) => {
                        let _ = done_tx.send(Err(RuntimeError::Internal(format!(
                            "failed to build isolate runtime: {e}"
                        ))));
                        return;
                    }
                };
                let local = tokio::task::LocalSet::new();
                local.block_on(&rt, async move {
                    let result = execute_isolate(
                        artifact_js,
                        ctx,
                        req_body,
                        call_tx,
                        handle_tx,
                        cancelled,
                        logs,
                    )
                    .await;
                    let _ = done_tx.send(result);
                });
            }
        });
    if let Err(e) = thread {
        return Err(RuntimeError::Internal(format!(
            "failed to spawn isolate thread: {e}"
        )));
    }

    // Grab the isolate handle so cancellation can terminate execution even
    // when the isolate is stuck in a compute-only loop (design doc §11.4).
    let isolate_handle = handle_rx.await.ok();

    tokio::pin!(done_rx);
    let sleep = tokio::time::sleep(timeout);
    tokio::pin!(sleep);
    // Grace timer, armed only after the wall-clock timeout fires. Starts
    // already-elapsed but is gated off by `if timed_out` until then.
    let grace = tokio::time::sleep(std::time::Duration::ZERO);
    tokio::pin!(grace);
    let mut timed_out = false;
    loop {
        tokio::select! {
            // Cancellation: terminate the isolate; the thread's done_tx will
            // then deliver an error/partial which we map to Cancelled.
            _ = &mut cancel => {
                cancelled.store(true, Ordering::Relaxed);
                if let Some(h) = &isolate_handle {
                    h.terminate_execution();
                }
                return Err(RuntimeError::Cancelled);
            }
            // Wall-clock timeout: terminate the isolate the same way as
            // cancel, then keep looping until the isolate thread actually
            // exits (`done_rx` below) so the OS thread + V8 isolate don't
            // outlive this call — but only up to `TIMEOUT_GRACE` (armed here),
            // after which we give up waiting (see the grace branch).
            _ = &mut sleep, if !timed_out => {
                timed_out = true;
                cancelled.store(true, Ordering::Relaxed);
                if let Some(h) = &isolate_handle {
                    h.terminate_execution();
                }
                grace
                    .as_mut()
                    .reset(tokio::time::Instant::now() + TIMEOUT_GRACE);
            }
            // Grace expired: the isolate did not exit within `TIMEOUT_GRACE`
            // of termination. This only happens if it's parked in a host call
            // that hasn't returned (terminate_execution can't interrupt a
            // pending host await — there's no running JS to throw into). Host
            // ops are individually bounded (ctx.fetch carries connect+total
            // timeouts; connectors carry their own), so rather than block this
            // request indefinitely we return Timeout and let the detached
            // thread unwind on its own once the host op completes — its
            // `done_tx`/`call_tx` sends then no-op against our dropped ends.
            _ = &mut grace, if timed_out => {
                return Err(RuntimeError::Timeout);
            }
            // Service a host call from the isolate. Spawn so concurrent
            // ctx.* awaits inside one function don't serialize.
            maybe_call = call_rx.recv() => {
                match maybe_call {
                    Some(call) => {
                        let host = host.clone();
                        tokio::spawn(async move {
                            match call {
                                HostCall::Query { sql, reply } => {
                                    let _ = reply.send(host.query(sql).await);
                                }
                                HostCall::QueryStream { sql, reply } => {
                                    let _ = reply.send(host.query_stream(sql).await);
                                }
                                HostCall::Fetch { url, init, reply } => {
                                    let _ = reply.send(host.fetch(url, init).await);
                                }
                                HostCall::SemanticQuery { spec, reply } => {
                                    let _ = reply.send(host.semantic_query(spec).await);
                                }
                                HostCall::AirwayRun { pipeline_ref, variables, reply } => {
                                    let _ = reply.send(host.airway_run(pipeline_ref, variables).await);
                                }
                                HostCall::WarehouseWrite { op, payload, reply } => {
                                    let _ = reply.send(host.warehouse_write(op, payload).await);
                                }
                                HostCall::SecretsSet { key, value, reply } => {
                                    let _ = reply.send(host.secrets_set(key, value).await);
                                }
                                HostCall::SendEmail { input, reply } => {
                                    let _ = reply.send(host.send_email(input).await);
                                }
                            }
                        });
                    }
                    None => { /* sender dropped; isolate is finishing */ }
                }
            }
            done = &mut done_rx => {
                if timed_out {
                    return Err(RuntimeError::Timeout);
                }
                return done.unwrap_or_else(|_| {
                    Err(RuntimeError::Internal("isolate thread vanished".into()))
                });
            }
        }
    }
}

/// Body that runs on the isolate thread. Loads + evaluates the module, calls
/// the default export, and returns the parsed `FnResponse`.
async fn execute_isolate(
    artifact_js: String,
    ctx: InvocationCtx,
    req_body: Vec<u8>,
    call_tx: mpsc::UnboundedSender<HostCall>,
    handle_tx: oneshot::Sender<deno_core::v8::IsolateHandle>,
    cancelled: Arc<AtomicBool>,
    logs: Arc<std::sync::Mutex<Vec<LogLine>>>,
) -> Result<FnResponse, RuntimeError> {
    let mut runtime = JsRuntime::new(RuntimeOptions {
        extensions: vec![oxy_functions_ext::init_ops()],
        ..Default::default()
    });
    let _ = handle_tx.send(runtime.v8_isolate().thread_safe_handle());
    runtime.op_state().borrow_mut().put(call_tx);
    runtime.op_state().borrow_mut().put(cancelled);
    runtime.op_state().borrow_mut().put(FunctionLogs(logs));

    runtime
        .execute_script("oxy:bootstrap", BOOTSTRAP_JS)
        .map_err(|e| RuntimeError::Internal(format!("bootstrap failed: {e}")))?;

    let ctx_json = serde_json::to_string(&ctx)
        .map_err(|e| RuntimeError::Internal(format!("ctx serialize failed: {e}")))?;
    let req_body_str = String::from_utf8_lossy(&req_body);
    let req_json = serde_json::json!({ "body": req_body_str }).to_string();

    let specifier = deno_core::resolve_url("oxy:function")
        .map_err(|e| RuntimeError::Internal(format!("bad module specifier: {e}")))?;
    let mod_id = runtime
        .load_side_es_module_from_code(&specifier, artifact_js)
        .await
        .map_err(|e| RuntimeError::Js(format!("module load failed: {e}")))?;
    let eval = runtime.mod_evaluate(mod_id);
    runtime
        .run_event_loop(Default::default())
        .await
        .map_err(|e| RuntimeError::Js(e.to_string()))?;
    eval.await
        .map_err(|e| RuntimeError::Js(format!("module evaluation failed: {e}")))?;

    let invoke_script = format!(
        r#"
        (async () => {{
            const ctx = globalThis.__buildCtx({ctx_json});
            const req = {req_json};
            const mod = await import("oxy:function");
            const handler = mod.default;
            if (typeof handler !== "function") {{
                throw new Error("function module has no default export");
            }}
            const res = await handler(req, ctx);
            return {{
                status: (res && res.status) || 200,
                body: res && res.body !== undefined ? String(res.body) : "",
            }};
        }})()
        "#,
    );

    let promise = runtime
        .execute_script("oxy:invoke", invoke_script)
        .map_err(|e| RuntimeError::Js(format!("invoke failed: {e}")))?;
    // `resolve()` alone does NOT pump the event loop — it just returns a future
    // that settles when the promise does. A handler whose promise awaits an
    // async host op (ctx.query/fetch/…) would then hang forever: the op's reply
    // arrives on the broker, but nothing re-polls the isolate to deliver it back
    // into JS, so the promise never resolves (→ wall-clock timeout). Drive the
    // event loop while awaiting, exactly like the module-eval path above does.
    let resolve_fut = Box::pin(runtime.resolve(promise));
    let result = runtime
        .with_event_loop_promise(resolve_fut, deno_core::PollEventLoopOptions::default())
        .await
        .map_err(|e| RuntimeError::Js(e.to_string()))?;

    let scope = &mut runtime.handle_scope();
    let local = deno_core::v8::Local::new(scope, result);
    let value: serde_json::Value = deno_core::serde_v8::from_v8(scope, local)
        .map_err(|e| RuntimeError::Internal(format!("result deserialize failed: {e}")))?;
    serde_json::from_value(value)
        .map_err(|e| RuntimeError::Internal(format!("result shape invalid: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reply_json_ok_passes_value_through() {
        let out = reply_json("ctx.query", Ok(serde_json::json!({ "rows": [] })));
        assert_eq!(out, r#"{"rows":[]}"#);
    }

    #[test]
    fn reply_json_err_wraps_as_oxy_error() {
        let out = reply_json("ctx.warehouse", Err("boom".to_string()));
        let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(parsed["__oxyError"], "HostError");
        assert_eq!(parsed["message"], "ctx.warehouse: boom");
    }

    #[test]
    fn cancelled_flag_defaults_false() {
        let flag = Arc::new(AtomicBool::new(false));
        assert!(!flag.load(Ordering::Relaxed));
        flag.store(true, Ordering::Relaxed);
        assert!(flag.load(Ordering::Relaxed));
    }

    /// Regression: a handler that awaits an async host op (`ctx.query`) must
    /// RESUME and return once the op replies — it must not hang until the
    /// wall-clock timeout. This reproduces the `resolve()`-doesn't-pump-the-
    /// event-loop bug: with the buggy invoke path this returns `Err(Timeout)`;
    /// with `with_event_loop_promise` it returns the handler's `Response` fast.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn handler_resumes_after_async_host_op() {
        struct MockHost;
        #[async_trait::async_trait]
        impl FunctionHost for MockHost {
            async fn query(&self, _sql: String) -> Result<serde_json::Value, String> {
                Ok(serde_json::json!({ "rows": [{ "x": 1 }], "truncated": false }))
            }
            async fn query_stream(&self, _sql: String) -> Result<serde_json::Value, String> {
                Err("unused".into())
            }
            async fn fetch(
                &self,
                _url: String,
                _init: serde_json::Value,
            ) -> Result<serde_json::Value, String> {
                Err("unused".into())
            }
            async fn semantic_query(
                &self,
                _spec: serde_json::Value,
            ) -> Result<serde_json::Value, String> {
                Err("unused".into())
            }
            async fn airway_run(
                &self,
                _pipeline_ref: String,
                _variables: serde_json::Value,
            ) -> Result<serde_json::Value, String> {
                Err("unused".into())
            }
            async fn warehouse_write(
                &self,
                _op: String,
                _payload: serde_json::Value,
            ) -> Result<serde_json::Value, String> {
                Err("unused".into())
            }
            async fn secrets_set(
                &self,
                _key: String,
                _value: String,
            ) -> Result<serde_json::Value, String> {
                Err("unused".into())
            }
            async fn send_email(
                &self,
                _input: serde_json::Value,
            ) -> Result<serde_json::Value, String> {
                Err("unused".into())
            }
        }

        let artifact = r#"
            export default async (req, ctx) => {
                const r = await ctx.query("select 1");
                return Response.json({ rows: r.rows.length });
            };
        "#;
        let ctx = InvocationCtx {
            user: CtxUser {
                id: "u".into(),
                email: "e@example.com".into(),
                org_id: "o".into(),
            },
            env: Default::default(),
        };
        let (_cancel_tx, cancel_rx) = tokio::sync::oneshot::channel::<()>();
        let result = run(
            artifact.to_string(),
            ctx,
            b"{}".to_vec(),
            std::sync::Arc::new(MockHost),
            cancel_rx,
            std::time::Duration::from_secs(10),
            Arc::new(std::sync::Mutex::new(Vec::new())),
        )
        .await;
        let resp = result.expect("handler must resume after the async host op, not time out");
        assert!(
            resp.body.contains("\"rows\":1"),
            "unexpected handler body: {}",
            resp.body
        );
    }
}
