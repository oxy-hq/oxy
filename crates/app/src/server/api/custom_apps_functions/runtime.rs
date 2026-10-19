//! V8-isolate execution for Oxy Functions.
//!
//! See `internal-docs/customer-apps-functions.md` §4 and
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
//! are all wired to real backends.
//!
//! `ctx.tx` adds multi-statement atomicity over that same allowlist: the
//! isolate holds only a handle id, the pinned connection lives in the host's
//! `tx::TxRegistry`, and the bootstrap wrapper owns the commit/rollback
//! bracket so an author cannot leave one open. `ctx.queryStream` (§11.5) fetches up to
//! `FUNCTION_STREAM_MAX_ROWS` rows in one host call and yields them to the
//! function as an async generator in client-side batches — a pragmatic MVP
//! pending a true warehouse-cursor implementation.
//!
//! ## Layout
//!
//! This file is the engine-neutral runtime: the types, the bootstrap script,
//! admission, the isolate thread and the broker loop. The JS engine sits
//! behind [`engine::IsolateEngine`] (`deno_core` is its only implementation,
//! in `engine/deno`); what each `ctx.*` op does is in [`ops`], and the
//! broker's side of a host call in [`host_call`].

use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use sentry::SentryFutureExt;
use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc, oneshot};
use tracing::Instrument;

mod engine;
mod host_call;
mod host_call_span;
mod ops;
#[cfg(test)]
mod tests;

use super::LogLine;
use super::env_policy::{EnvPolicy, HeldTarget, HostOp};
use super::host::FunctionCapabilities;
use crate::server::api::custom_apps_telemetry::InvocationMeters;
use engine::{ActiveEngine, IsolateConfig, IsolateEngine, TerminateHandle};
use host_call::{
    HostCall, dispatch_host_call, host_call_span, host_call_target, record_host_call_outcome,
};
use ops::{FunctionLogs, OpContext};

/// Per-invocation context handed to the isolate. Built fresh for every
/// invocation from the resolved identity (design doc §11.7) — never cached
/// across invocations.
#[derive(Debug, Clone, Serialize)]
pub struct InvocationCtx {
    pub user: CtxUser,
    pub env: BTreeMap<String, String>,
    /// `ctx.airhouse.schema` — the app's own Airhouse schema, present only when
    /// the function declared the `airhouse` capability.
    #[serde(rename = "airhouseSchema", skip_serializing_if = "Option::is_none")]
    pub airhouse_schema: Option<String>,
    /// `ctx.channel` (and its alias `ctx.environment`, the environments
    /// design's name): the app environment this invocation runs in —
    /// `"production"`, or `"staging"`, where the host holds every write
    /// (`custom_apps_functions::env_policy`).
    pub channel: String,
}

/// One org team the caller belongs to, as surfaced through `ctx.user.teams`.
///
/// **Scoped to the app's own org.** A team the caller holds in some *other* org
/// is never reported here — the same user in two tenants must not learn one
/// tenant's team names from the other's app.
#[derive(Debug, Clone, Serialize)]
pub struct CtxTeam {
    pub id: String,
    pub name: String,
}

/// Where this invocation's identity came from.
///
/// The distinction is **"is there a caller to attribute this to"**, not "did a
/// human cause it". Every background path runs under the org **owner's**
/// `user_id` (the invocation row needs a non-null FK, and `ctx.secrets` needs a
/// `created_by`) and carries no caller — including an operator's manual
/// **Run now**, which `trigger_function_job` deliberately routes down the same
/// system path under the owner identity, with no caller context beyond whatever
/// `input` the trigger was given. So a person may well have clicked; the platform
/// simply did not carry who through the task queue.
///
/// A function that branches on identity — "email the person who clicked", "show
/// the admin view" — has to be able to tell the two apart, and an email-sniffing
/// check (`endsWith("@system.oxy")`) is exactly the kind of heuristic that
/// silently stops working.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum CtxIdentityKind {
    /// A signed-in human called the function over its HTTP route.
    User,
    /// No caller to attribute it to: a schedule tick, an Airway transform step,
    /// or an operator's manual **Run now** (which the job trigger routes down
    /// this same path). Every *caller* field (`name`, `picture`, `orgRole`,
    /// `teams`) is absent, and `email` is the synthetic
    /// `schedule+<fn>@system.oxy`.
    ///
    /// A manual run therefore cannot reach the operator who triggered it —
    /// `run_function_job` discards the authenticated user, and the task payload
    /// has nowhere to put it. Threading it through is a real follow-up, and a
    /// behaviour change: the run would then execute under that operator's
    /// authority rather than the owner's.
    System,
}

/// The identity of whoever (or whatever) invoked this function.
///
/// Assembled server-side from the authenticated session on every invocation and
/// never cached across them, so **nothing here is client-supplied** — that is
/// the whole point of reading identity from `ctx` rather than from the request
/// body. See `internal-docs/custom-apps-user-identity.md` for the full contract
/// and for what the *client* side (`useShellContext`) can and cannot be trusted
/// for.
#[derive(Debug, Clone, Serialize)]
pub struct CtxUser {
    /// `users.id`. On a system invocation this is the org owner's id, not a
    /// caller — check `kind` before attributing anything to it.
    pub id: String,
    /// `users.email`, or the synthetic `schedule+<fn>@system.oxy` when
    /// `kind == "system"`.
    ///
    /// `None` — serialized as `ctx.user.email === null` — for a frontline
    /// worker enrolled without a mailbox. Deliberately NOT flattened to `""`:
    /// a function branching on "can I email this person" must be able to tell
    /// "no address" from "an address that happens to be empty", and an app that
    /// passes `""` to `ctx.email.send` gets an SES rejection instead of an
    /// obvious `null` check. See `internal-docs/frontline-identity.md`.
    pub email: Option<String>,
    /// The org that owns this app — the tenant boundary for any query the
    /// function runs.
    ///
    /// Serialized as `orgId`. Before 2026-08-21 this went out as `org_id`,
    /// which meant the documented `ctx.user.orgId` was `undefined` — a silent
    /// footgun for any SQL filtering on it. `__buildCtx` still mirrors the old
    /// `org_id` key so functions written against the shipped behaviour keep
    /// working.
    #[serde(rename = "orgId")]
    pub org_id: String,
    /// `users.name` — display identity, absent on a system invocation.
    ///
    /// Free text the user controls. Fine for a greeting or an audit row; never
    /// a key, and never interpolated into SQL or HTML without escaping.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// `users.picture` — an avatar URL, absent when unset or on a system
    /// invocation.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub picture: Option<String>,
    /// The caller's role **within this app** — `"admin"`, `"member"`, or absent.
    /// Server-derived: `"admin"` is `Ring::AppAdmin` in `oxy-authz` (an app grant,
    /// org owner/admin, or staff break-glass), and `"member"` is any grant the
    /// caller holds on the app — **direct (`app_members`) or through a team they
    /// belong to (`app_team_grants` × `org_team_members`)**. An app gates its
    /// privileged surface on this rather than on a client-side flag or a
    /// hard-coded email allowlist.
    ///
    /// A **system** invocation runs under the org owner, so this reads `"admin"`
    /// there — a schedule has owner authority by construction. Gate on `kind`
    /// too if a surface must be human-only.
    #[serde(rename = "appRole", skip_serializing_if = "Option::is_none")]
    pub app_role: Option<String>,
    /// The caller's role in the owning **org** — `"owner"`, `"admin"`, or
    /// `"member"`; absent when they reach the app without an org membership
    /// (Oxy staff on break-glass) or on a system invocation.
    ///
    /// A *fact* read straight off `org_members.role`, not an authorization
    /// verdict: org standing and app standing are different rings, and an app
    /// admin need not be an org Admin. Gate on [`Self::app_role`]; use this to
    /// explain, label, or route — "your org admin can change this".
    #[serde(rename = "orgRole", skip_serializing_if = "Option::is_none")]
    pub org_role: Option<String>,
    /// The org teams the caller belongs to, name-sorted, scoped to this app's
    /// org. Empty when they belong to none, and always present so a function can
    /// `.some(...)` without a null check.
    ///
    /// Teams are how an org grants an app to a group it already recognises, so
    /// they are useful for *shaping* a view (default the Finance team to the
    /// finance tab). They are not a permission: a team only means something on
    /// an app through `app_team_grants`, which is already folded into
    /// [`Self::app_role`].
    pub teams: Vec<CtxTeam>,
    /// Whether a human or the platform invoked this function.
    pub kind: CtxIdentityKind,
    /// Where the caller may act — the operating graph's answer, decided from
    /// their assignments before the function runs and applied by the function
    /// through `@oxy-hq/sdk/ops`. A system invocation reaches everywhere; a
    /// lookup failure reaches nowhere, so a blip cannot widen anything.
    /// `internal-docs/operating-graph.md` §3.3.
    pub reach: crate::server::api::operating_graph::reach::Reach,
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
    /// The isolate breached its heap ceiling and was terminated.
    ///
    /// Before the ceiling existed this case did not surface as an error at all:
    /// V8's own default limit is far above the pod's cgroup limit, so the
    /// kernel OOM killer reached the process first and took down every other
    /// app it was serving. This variant is that failure, made survivable and
    /// attributable to one invocation.
    #[error("function exceeded its memory limit")]
    OutOfMemory,
    /// No concurrency permit came free within the queue budget.
    ///
    /// The platform declining to start the work, not the function failing. The
    /// caller maps it to the `shed` invocation status, which
    /// [`super::failure_signal::Failure::of`] treats like a cancellation: no
    /// page, no `error.type`, and no dent in the app's availability. The count
    /// lives in `oxy_custom_app_admission_shed_total`.
    ///
    /// **Not a 503.** The route path answers over SSE
    /// (`mod.rs::sse_response`), so every outcome — success or failure — is a
    /// 200 carrying an event. A shed surfaces as `{"error": "shed", …}` in
    /// that stream; giving it a real status code would mean changing the
    /// response contract for every invocation, not just this one.
    #[error("too many functions are running; try again shortly")]
    Overloaded,
    #[error("internal runtime error: {0}")]
    Internal(String),
}

/// Function-scoped row cap for `ctx.query` (design doc §11.5): 10x the UI
/// `MAX_ROWS` cap, since ETL functions legitimately need more rows than a
/// rendered table. Genuinely large scans use `ctx.queryStream` instead.
pub const FUNCTION_MAX_ROWS: usize = 100_000;

/// Row cap for `ctx.queryStream` (design doc §11.5): a higher ceiling than
/// `FUNCTION_MAX_ROWS` for genuinely large scans. The MVP implementation
/// fetches up to this many rows in one shot and yields them to the isolate
/// in client-side batches; a true warehouse-cursor implementation is future
/// work.
pub const FUNCTION_STREAM_MAX_ROWS: usize = 1_000_000;

/// Host-side data plane the isolate calls back into. Implemented in
/// `mod.rs` against the resolved project context (connectors) + an HTTP
/// client. `Send + Sync` so the broker loop can `Arc`-clone it per call.
#[async_trait::async_trait]
pub trait FunctionHost: Send + Sync {
    /// `ctx.query(sql)` — read-only SQL, function-scoped row cap. Returns
    /// the rows as a JSON array value.
    async fn query(&self, sql: String) -> Result<serde_json::Value, String>;
    /// Called once after the isolate has finished (or been cancelled), on the
    /// caller's side of the channel: the place for work that must happen once
    /// per invocation rather than once per host call.
    async fn end_of_invocation(&self) {}

    /// What this function's manifest grants. The broker reads it before every
    /// op (`host_call::dispatch_host_call`): an op whose registry row names a
    /// capability the manifest lacks is refused with the host's own words
    /// (`FunctionCapabilities::refuse`) and never dispatched, so a method
    /// below runs only for an op its manifest allows. Fail-closed by default:
    /// a host that does not say grants nothing.
    fn capabilities(&self) -> &FunctionCapabilities {
        static NOTHING: std::sync::LazyLock<FunctionCapabilities> =
            std::sync::LazyLock::new(FunctionCapabilities::default);
        &NOTHING
    }
    /// What each op may do in the environment this run was admitted to. The
    /// broker decides every op that holds by not running (`HoldBy::Broker`)
    /// before it is dispatched; an op that holds by a mechanism of its own
    /// asks this policy inside its method. Production by default — what
    /// every run was before environments — so a test double is never held.
    fn env_policy(&self) -> &EnvPolicy {
        static PRODUCTION: std::sync::LazyLock<EnvPolicy> =
            std::sync::LazyLock::new(EnvPolicy::production);
        &PRODUCTION
    }
    /// Called by the broker when the environment held or refused an op
    /// before dispatch, with the identifiers the held-write log records it
    /// under (never values). The real host writes the `app.staging.held`
    /// row from these.
    async fn record_held(&self, _op: HostOp, _target: HeldTarget<'_>) {}

    /// Whether this host's environment policy decides — outside production,
    /// or a production run refused for reading a branch. Only then is a
    /// `HeldInStaging` / `EnvironmentRefused` label on its errors the
    /// policy's own (`host_call_attrs::classify_host_error_for`).
    fn decides_by_environment(&self) -> bool {
        false
    }
    /// Called by the broker when a host call failed in a way that pages
    /// (`host_call_attrs::counts_toward_paging`), before the isolate sees the
    /// rejection — so a handler that catches it and answers 2xx still leaves a
    /// failure behind. `op` is a fixed op name, never what the call carried;
    /// `message` is the host's error text, which the host normalizes before it
    /// keeps anything (`failure_signal::HostCallFailure::noted`), so the
    /// fingerprint can tell a new break on an op apart from the routine
    /// failure a handler already catches there; `target` is where the call
    /// went, for the ops that name one (`host_call_target`), normalized the
    /// same way before it is kept.
    fn note_host_call_failure(
        &self,
        _op: &'static str,
        _kind: &'static str,
        _message: &str,
        _target: Option<&str>,
    ) {
    }
    /// Called by the broker when a host call succeeded, so a failure noted for
    /// the same `op` against the same `target` earlier in the run is dropped:
    /// the run recovered from it, and the fingerprint should name one it did
    /// not. A success against another target — a `fetch` to Slack after a
    /// failed `fetch` to a vendor — clears nothing. The rule:
    /// `failure_signal::HostCallFailure::recovered`.
    fn note_host_call_success(&self, _op: &'static str, _target: Option<&str>) {}
    /// The failure noted for this invocation — the first not recovered from —
    /// read once the isolate has finished.
    fn host_call_failure(&self) -> Option<super::failure_signal::HostCallFailure> {
        None
    }
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

    /// `ctx.org.people()` — the org's people directory, read-only.
    ///
    /// Takes no arguments on purpose: the org is the host's, so a function
    /// cannot name one and a manifest cannot point this at another tenant.
    ///
    /// Defaulted rather than required, because every other implementor of this
    /// trait is a test double that has no directory to answer with — and the
    /// default is the same fail-closed refusal the capability gate gives, so a
    /// double that forgets to override it denies rather than inventing people.
    async fn org_people(&self) -> Result<serde_json::Value, String> {
        Err("ctx.org.people is not available in this host".to_string())
    }
    /// `ctx.org.places()` — the org's locations. Same shape and same
    /// fail-closed default as `org_people`.
    async fn org_places(&self) -> Result<serde_json::Value, String> {
        Err("ctx.org.places is not available in this host".to_string())
    }
    /// `ctx.org.assignments()` — who holds which position where.
    async fn org_assignments(&self) -> Result<serde_json::Value, String> {
        Err("ctx.org.assignments is not available in this host".to_string())
    }
    /// `ctx.platform.facts(opts)` — ids and counts about every org, for an
    /// internal app. `opts` carries `weeks` only; the org is the host's. The
    /// same fail-closed default as `org_people`.
    async fn platform_facts(&self, _opts: serde_json::Value) -> Result<serde_json::Value, String> {
        Err("ctx.platform.facts is not available in this host".to_string())
    }
    /// `ctx.airway.run(pipelineRef, variables)` — seed an Airway ELT run.
    /// Returns `{ runId }`; the run is driven asynchronously by the worker
    /// fleet (it does not block on completion — ELT runs routinely exceed
    /// the function timeout ceiling).
    async fn airway_run(
        &self,
        pipeline_ref: String,
        variables: serde_json::Value,
    ) -> Result<serde_json::Value, String>;
    /// `ctx.warehouse.query(database, sql)` — a read against a named database.
    async fn warehouse_query(
        &self,
        payload: serde_json::Value,
    ) -> Result<serde_json::Value, String>;
    /// `ctx.warehouse.{insert,exec,upsert}` — write to one of the app's
    /// configured destination databases. `op` is `"insert"`, `"exec"`, or
    /// `"upsert"` — a registry sub-op, the broker having refused one off the
    /// list (`env_policy::HostOp`); `payload` carries
    /// `{ database, table?, rows?, sql? }` depending on `op`. Validated
    /// against the project's configured databases (§11.3) before execution.
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
    /// `ctx.notify(input)` — tell people in the org something, by user, role
    /// at a location, or team. Gated by the fail-closed `notify` capability;
    /// the platform resolves recipients and channels and never hands the
    /// function an address. Returns `{ notificationId, recipients, skipped }`.
    async fn notify(&self, input: serde_json::Value) -> Result<serde_json::Value, String>;
    /// `ctx.storage.{getUploadUrl,getDownloadUrl,put,get,head,list,delete,copy}`
    /// — presigned S3 file storage scoped to the app's silo. `op` selects the
    /// operation (a `storage.*` registry sub-op); `payload` carries its args.
    /// Gated by the fail-closed `storage.{read,write}` manifest capabilities,
    /// per op in the registry. Mirrors `warehouse_write`'s
    /// single-op-dispatcher shape.
    async fn storage(
        &self,
        op: String,
        payload: serde_json::Value,
    ) -> Result<serde_json::Value, String>;
    /// `ctx.tx(database, fn)` and `ctx.oltp.tx(fn)` — a multi-statement
    /// transaction on a pinned connection. `op` is one of `begin` / `begin_oltp`
    /// / `query` / `exec` / `commit` / `rollback`; `payload` carries its args. Same single-op-dispatcher shape
    /// as `warehouse_write`, and gated by the same fail-closed `destinations`
    /// allowlist — a transaction is a write, so it may not reach a database the
    /// function did not declare.
    ///
    /// The op split exists because the transaction has to stay open across
    /// `await`s **in the author's JavaScript**: the isolate holds a handle id
    /// and the pinned connection lives here.
    async fn tx(&self, op: String, payload: serde_json::Value)
    -> Result<serde_json::Value, String>;
    /// `ctx.oltp.{query,exec}` — read/write the app's OWN per-org OLTP schema
    /// (`app_<writer>`) on the managed Postgres tenant. `op` is `query` or
    /// `exec`; `payload` carries `{ sql, params? }`. Gated by the fail-closed
    /// `oltp` manifest capability, and scoped to the app's own writer role — so unlike `ctx.warehouse` (read-only analyst on a
    /// managed database) it can write, and cannot see another app's data.
    async fn oltp(
        &self,
        op: String,
        payload: serde_json::Value,
    ) -> Result<serde_json::Value, String>;
    /// `ctx.airhouse.{query,exec,append}` — the app's own facts in its
    /// workspace's Airhouse, in the schema `app_<writer>` derived from its slug.
    /// Written as the app whoever invoked; every statement is checked against
    /// that schema first. Gated by the fail-closed `airhouse` manifest capability.
    async fn airhouse(
        &self,
        op: String,
        payload: serde_json::Value,
    ) -> Result<serde_json::Value, String>;
}

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

// Base64. This isolate is bare deno_core — no `deno_web`, so none of the Web
// binary helpers exist, and V8 here predates `Uint8Array.prototype.toBase64`.
// Without these an author literally cannot produce the base64 that
// `ctx.email.send` attachments and `ctx.storage.put({encoding:"base64"})`
// require: `btoa` was simply `undefined`.
//
// These follow WHATWG semantics so that a helper unit-tested under Node behaves
// identically here. For BYTES use `bytesToBase64` from `@oxy-hq/sdk`, which is
// plain bundled JS and therefore the same function everywhere.
const __B64 = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
// Reverse lookup. `__B64.indexOf(ch)` is a 64-char scan per input character;
// over the ~13.3 MB of base64 a 10 MiB attachment carries that is millions of
// scans inside a wall-clock-capped invocation.
const __B64R = new Uint8Array(256).fill(255);
for (let __i = 0; __i < 64; __i++) __B64R[__B64.charCodeAt(__i)] = __i;
// Accumulate into array segments rather than `out += c` per byte, which would
// allocate one rope node per byte at exactly the sizes this feature targets.
const __B64_CHUNK = 8192;

globalThis.btoa = (input) => {
  if (input instanceof ArrayBuffer || ArrayBuffer.isView(input)) {
    // The spec would ToString this to "37,80,68,70" and cheerfully encode the
    // wrong bytes. Refuse: a loud error beats a silently corrupt file, and the
    // named helper does the right thing.
    throw new TypeError(
      "btoa: expected a string. For bytes use bytesToBase64() from @oxy-hq/sdk"
    );
  }
  const s = String(input);
  const parts = [];
  let buf = "";
  for (let i = 0; i < s.length; i += 3) {
    const c0 = s.charCodeAt(i);
    const c1 = i + 1 < s.length ? s.charCodeAt(i + 1) : 0;
    const c2 = i + 2 < s.length ? s.charCodeAt(i + 2) : 0;
    if (c0 > 0xff || c1 > 0xff || c2 > 0xff) {
      // Same failure as a browser: btoa cannot carry UTF-8. Point at the way
      // out rather than emitting mojibake.
      throw new TypeError(
        "btoa: input contains characters outside the Latin1 range; for text " +
        "pass it directly with { encoding: \"utf8\" }"
      );
    }
    const n = (c0 << 16) | (c1 << 8) | c2;
    buf += __B64[(n >> 18) & 63] + __B64[(n >> 12) & 63]
      + (i + 1 < s.length ? __B64[(n >> 6) & 63] : "=")
      + (i + 2 < s.length ? __B64[n & 63] : "=");
    if (buf.length >= __B64_CHUNK) { parts.push(buf); buf = ""; }
  }
  parts.push(buf);
  return parts.join("");
};

globalThis.atob = (input) => {
  let s = String(input).replace(/[ \t\n\f\r]/g, "");
  // Strip padding BEFORE validating, and only when the length is a multiple of
  // 4 — that is what the spec does. Breaking out of the decode loop on the
  // first "=" instead would silently TRUNCATE: atob(chunkA + chunkB) where
  // chunkA ends in padding would return a short buffer and report success.
  if (s.length % 4 === 0) {
    let pad = 0;
    while (pad < 2 && s.charCodeAt(s.length - 1) === 61 /* = */) {
      s = s.slice(0, -1);
      pad++;
    }
  }
  if (s.indexOf("=") >= 0) {
    throw new TypeError("atob: '=' may only appear as trailing padding");
  }
  if (s.length % 4 === 1) throw new TypeError("atob: invalid base64 length");
  const parts = [];
  let chunk = [];
  let buf = 0;
  let bits = 0;
  for (let i = 0; i < s.length; i++) {
    const code = s.charCodeAt(i);
    const v = code < 256 ? __B64R[code] : 255;
    if (v === 255) throw new TypeError("atob: invalid base64 character '" + s[i] + "'");
    buf = (buf << 6) | v;
    bits += 6;
    if (bits >= 8) {
      bits -= 8;
      chunk.push((buf >> bits) & 0xff);
      if (chunk.length >= __B64_CHUNK) {
        parts.push(String.fromCharCode.apply(null, chunk));
        chunk = [];
      }
    }
  }
  if (chunk.length) parts.push(String.fromCharCode.apply(null, chunk));
  return parts.join("");
};

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

// The commit/rollback bracket behind ctx.tx and ctx.oltp.tx. It lives HERE
// rather than in the author's code on purpose: an author who forgets a rollback
// in a catch block leaves a transaction open holding locks, and the only
// reliable moment to close it is the one the runtime owns. Statements take bound
// parameters ($1, $2, …) — never build SQL by concatenating user input.
const __runTx = async (signature, label, beginOp, beginPayload, fn) => {
  if (typeof fn !== "function") {
    throw new TypeError(`${signature}: fn must be a function`);
  }
  const call = __wrapOp("op_ctx_tx");
  const { id } = await call(beginOp, beginPayload);
  let closed = false;
  // Every method re-checks `closed` so a handle that escapes the callback
  // (stashed on a global, captured by a stray promise) fails loudly instead of
  // addressing whatever transaction now holds that id.
  const live = (what) => {
    if (closed) {
      throw new Error(
        `${label}: this transaction is already finished — ${what} was called after the callback returned`,
      );
    }
  };
  const handle = {
    query: async (sql, params) => {
      live("query");
      // `??` not `||`: both map a real omission to [], but `||` also swallows 0
      // and "" — handing a wrong-typed argument to the host as "no parameters"
      // and producing a misleading arity error. Anything else is forwarded as-is
      // for the host to reject by name.
      const r = await call("query", { id, sql: String(sql), params: params ?? [] });
      return r.rows;
    },
    exec: async (sql, params) => {
      live("exec");
      const r = await call("exec", { id, sql: String(sql), params: params ?? [] });
      return r.rowCount;
    },
  };
  let result;
  try {
    result = await fn(handle);
  } catch (err) {
    closed = true;
    // Swallow a rollback failure: the connection drops either way, which the
    // server treats as a rollback, and surfacing it would replace the error the
    // author actually needs to see.
    try {
      await call("rollback", { id });
    } catch (_) {}
    throw err;
  }
  closed = true;
  await call("commit", { id });
  return result;
};

globalThis.__buildCtx = (ctxData) => ({
  // `org_id` is a back-compat mirror of `orgId`. The host used to serialize the
  // field snake_cased, so `ctx.user.orgId` — the name the SDK types and the docs
  // have always used — read `undefined`, and a tenant filter written against it
  // silently compared against nothing. The host now emits `orgId`; this keeps
  // any function written against the shipped `org_id` working.
  //
  // Removal is NOT gated on an SDK version. What reads this key is the function
  // source inside an already-published bundle, and a bundle keeps running until
  // someone republishes it — an SDK floor would never come due. The measurable
  // condition is the artifacts themselves: we hold every live build's
  // `functions/*.js` in the build store, so this is removable once no build a
  // live app points at contains `.org_id`.
  user: { ...ctxData.user, org_id: ctxData.user.orgId },
  env: ctxData.env,
  // The app environment this run is in. `environment` is the design's name;
  // `channel` the hold layer's. Same value: "production" or "staging".
  channel: ctxData.channel,
  environment: ctxData.channel,
  log: (...args) => Deno.core.ops.op_ctx_log("info", args.map(String).join(" ")),
  // Synchronous — pure CPU, so these skip the host-call channel entirely.
  //
  // verifyHmac is the one that matters: with req.headers carrying a signature,
  // an author would otherwise hand-roll HMAC in JS and compare with `===`,
  // which leaks the digest a byte at a time. Strip the provider's prefix
  // ("sha256=", "v0=") before calling — those formats are per-provider.
  crypto: {
    // `key` comes from configuration, never from the request, so an absent one
    // is an author error and throws. Coercing it would sign with the literal
    // "undefined" — a key anyone can guess.
    hmac: ({ algorithm, key, data, encoding }) => {
      if (key == null) throw new TypeError("ctx.crypto.hmac: `key` is required — is the secret set?");
      if (data == null) throw new TypeError("ctx.crypto.hmac: `data` is required");
      return Deno.core.ops.op_ctx_hmac(
        String(algorithm || "sha256"), String(key), String(data), String(encoding || "hex"));
    },
    verifyHmac: ({ algorithm, key, data, signature, encoding }) => {
      if (key == null) throw new TypeError("ctx.crypto.verifyHmac: `key` is required — is the secret set?");
      if (data == null) throw new TypeError("ctx.crypto.verifyHmac: `data` is required");
      // `signature` stays lenient on purpose: it is attacker-controlled, so an
      // absent or malformed one must reject rather than throw.
      return Deno.core.ops.op_ctx_verify_hmac(
        String(algorithm || "sha256"), String(key), String(data),
        String(signature ?? ""), String(encoding || "hex"));
    },
    // Both sides are symmetric here and either may be attacker-controlled (an
    // omitted header) or author-controlled (an unset secret) — we cannot tell
    // which. So an absent side is always `false`, never a throw: false fails
    // closed, and throwing would turn an omitted header into a 500.
    timingSafeEqual: (a, b) =>
      Deno.core.ops.op_ctx_timing_safe_equal(a == null ? "" : String(a), b == null ? "" : String(b)),
  },
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
    // query(database, sql) — a READ against a named database. `ctx.query` only
    // ever reaches the project default, so this is the one way an app reads its
    // own OLTP store. Not gated by `destinations`: that allowlist is about
    // writes, and postgres_managed resolves the read-only analyst regardless.
    query: (database, sql) =>
      __wrapOp("op_ctx_warehouse")("query", { database, sql }),
  },
  // tx(database, fn) — run `fn` inside one transaction on a pinned connection
  // to a declared destination. Commits when `fn` resolves, rolls back when it
  // throws, and rethrows the original error either way (see __runTx).
  tx: (database, fn) =>
    __runTx("ctx.tx(database, fn)", "ctx.tx", "begin", { database: String(database) }, fn),
  // oltp.query(sql, params?) / oltp.exec(sql, params?) — read/write the app's
  // OWN per-org OLTP schema (app_<writer>), and nothing else. This is the write
  // half ctx.warehouse cannot give an app on a managed database (that resolves
  // the read-only analyst, which also sees the org's raw_* extracts). The writer
  // is derived host-side from the app's own slug — oxy-app.json's
  // `oltp: { enabled }` only gates access, it never names the target — so no
  // database name crosses the boundary. Each call auto-commits (a failed
  // statement rolls back). Statements take bound parameters ($1, $2, …) — never
  // build SQL by concatenating user input.
  oltp: {
    // oltp.tx(fn) — several statements on the app's own schema as one
    // transaction: commits when `fn` resolves, rolls back when it throws. The
    // same bracket and handle as ctx.tx; the writer is derived host-side.
    tx: (fn) => __runTx("ctx.oltp.tx(fn)", "ctx.oltp.tx", "begin_oltp", {}, fn),
    query: async (sql, params) => {
      const r = await __wrapOp("op_ctx_oltp")("query", {
        sql: String(sql),
        params: params ?? [],
      });
      return r.rows;
    },
    exec: async (sql, params) => {
      const r = await __wrapOp("op_ctx_oltp")("exec", {
        sql: String(sql),
        params: params ?? [],
      });
      return r.rowCount;
    },
  },
  // airhouse.* — the app's own FACTS in its workspace's Airhouse: append-only
  // history (an order happened, a checklist was completed) in the schema
  // `app_<writer>`, derived host-side from the app's slug. Written as the app,
  // whoever invoked the function, so a schedule writes like a click. Reads may
  // name any schema; writes must name `${ctx.airhouse.schema}.<table>`, and
  // tables come from airhouseMigrations, not exec. DuckLake has no keys or
  // UNIQUE, so give each fact its source's id and keep one row per id on read.
  airhouse: {
    schema: ctxData.airhouseSchema ?? null,
    query: (sql) => __wrapOp("op_ctx_airhouse")("query", { sql: String(sql) }),
    exec: async (sql) => {
      await __wrapOp("op_ctx_airhouse")("exec", { sql: String(sql) });
    },
    append: async (table, rows) => {
      const r = await __wrapOp("op_ctx_airhouse")("append", { table: String(table), rows });
      return r.rowCount;
    },
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
  // notify(input) — tell people in this org something: { to, kind, title,
  // body?, link?, email?, slack?, idempotencyKey? }. `to` names people (a user
  // id, a role at a location, a team), never an address; the platform picks
  // inbox, push, email and Slack per person.
  notify: (input) => __wrapOp("op_ctx_notify")(input),
  org: {
    // people() — who is in this org, by name. Read-only, and NOT a contact
    // list: a display name and a role, never an email or a phone. No location —
    // the platform holds none for a member. Includes frontline workers granted
    // to THIS app, tagged `kind: "frontline"`; see the host handler for why the
    // scope is per-app.
    // Naming a colleague is a different need from being able to message them,
    // and only the first one was blocking anything.
    people: () => __wrapOp("op_ctx_org_people")(),
    // places() — the org's locations: hierarchy, status, timezone, and what
    // each integration calls the place. The whole registry; reach is applied
    // by the app, on top.
    places: () => __wrapOp("op_ctx_org_places")(),
    // assignments() — who holds which position where, for the people who can
    // reach this app. The roster, read.
    assignments: () => __wrapOp("op_ctx_org_assignments")(),
  },
  platform: {
    // facts({ weeks }) — ids and counts about every org on the deployment,
    // for an internal app. Two gates, both fail closed: the manifest's
    // `platform.facts`, and this app's org one of the deployment's internal
    // orgs (OXY_INTERNAL_ORGS). Never an email, a message or a row of customer
    // data.
    facts: (opts) => __wrapOp("op_ctx_platform_facts")(opts || {}),
  },
  storage: {
    // The app's asset store — uploaded files AND generated ones, one silo.
    // getUploadUrl/getDownloadUrl mint presigned URLs the BROWSER talks to
    // directly (bytes never cross this boundary); put/get/head/list/delete/copy
    // are server-side. `op` stays a bare string and the rest is packed into a
    // payload object that __wrapOp JSON-stringifies (matching ctx.warehouse).
    getUploadUrl: (opts) => __wrapOp("op_ctx_storage")("getUploadUrl", opts || {}),
    getDownloadUrl: (key, opts) =>
      __wrapOp("op_ctx_storage")("getDownloadUrl", Object.assign({ key: String(key) }, opts || {})),
    // put(pathname, body, opts) — body is a string; pass
    // { encoding: "base64" } for binary assets (PDF/PNG/Parquet).
    put: (pathname, body, opts) =>
      __wrapOp("op_ctx_storage")(
        "put",
        Object.assign({ pathname: String(pathname), body: String(body) }, opts || {})
      ),
    get: (key, opts) =>
      __wrapOp("op_ctx_storage")("get", Object.assign({ key: String(key) }, opts || {})),
    head: (key) => __wrapOp("op_ctx_storage")("head", { key: String(key) }),
    list: (opts) => __wrapOp("op_ctx_storage")("list", opts || {}),
    // delete(key) or delete([key, ...])
    delete: (keyOrKeys) =>
      __wrapOp("op_ctx_storage")(
        "delete",
        Array.isArray(keyOrKeys)
          ? { keys: keyOrKeys.map(String) }
          : { key: String(keyOrKeys) }
      ),
    copy: (fromKey, toPathname, opts) =>
      __wrapOp("op_ctx_storage")(
        "copy",
        Object.assign({ fromKey: String(fromKey), toPathname: String(toPathname) }, opts || {})
      ),
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

/// Threads abandoned so far on this process. Healthy is zero.
///
/// The detach is deliberate (see the grace arm in [`run`]) and it is also an
/// unbounded resource leak. A rising value is the earliest available signal
/// that a tenant's function is wedged in a host call that will not return, and
/// it is a fact about **our process** rather than about the app — so it belongs
/// in platform telemetry, never in the tenant-facing store.
///
/// The count itself lives in `oxy_telemetry::metrics::sources`, not here.
/// It used to be a private static in this module that only `worker_metrics`
/// read, and `worker_metrics` is mounted only on `oxy worker`'s health port.
/// The worker does claim `app_function` tasks (scheduled and job-mode runs), so
/// it creates isolates and the counter was not inert there — but **route-mode**
/// invocations, which are the bulk, run on `oxy serve`, which published
/// nothing. Moving the storage into the telemetry crate is what lets serve
/// export it too.
///
/// Scraped as `oxy_abandoned_isolates_total` (hand-rolled, `worker_metrics`)
/// and as `oxy_custom_app_isolates_abandoned_total` (OTel, every role).
/// Deliberately not on the fleet-health API: that endpoint is `FleetOk`, so a
/// load-balanced read would report whichever replica answered.
pub fn abandoned_isolates() -> u64 {
    oxy_telemetry::metrics::sources::abandoned_isolates()
}

/// Per-isolate heap ceiling, in bytes. Override with
/// [`HEAP_LIMIT_MB_ENV`]; `0` disables the ceiling.
///
/// 128 MiB is Cloudflare Workers' per-isolate limit. Matching a published
/// number is itself the argument: an app author who hits it can look up what it
/// means and what to do, which is not true of a number we invented.
///
/// **Why any ceiling is strictly safer than none.** `create_params` was unset,
/// so V8 used its own default — far above the pod's 2 GiB cgroup limit. The
/// kernel OOM killer therefore always won the race, and it kills the *process*,
/// which is serving 44 apps, every org subdomain and every `FleetOk` product
/// route. With a ceiling, the same runaway allocation terminates one isolate
/// and returns an error to one app. The only traffic this newly breaks is a
/// function that legitimately holds >128 MiB of live JS objects — and that
/// function was already one spike away from taking the fleet down.
/// Per-isolate heap ceiling.
///
/// **Deliberately not loosened alongside the admission ceilings.** Those are
/// *rejecting* limits whose failure mode when set too tight is refused work;
/// this one is the *containment* limit, and it is what actually keeps one
/// tenant's runaway allocation from taking the pod — and with it every other
/// tenant on that replica — down. `limits::DEFAULT_MAX_CONCURRENCY × this` is
/// already well past the cgroup limit, which is the proof that the concurrency
/// cap is not the memory protection: this is.
///
/// Its own too-tight failure mode is real but bounded and visible: one
/// invocation is terminated, `oxy_custom_app_isolates_heap_terminations_total`
/// records it against the org, and nothing else on the replica is affected.
/// That is a strictly better trade than a pod OOM, so this number moves only on
/// evidence of legitimate functions hitting it — not as part of the
/// loosen-then-tighten pass on the admission ceilings.
const DEFAULT_HEAP_LIMIT_BYTES: usize = 128 * 1024 * 1024;

/// Override for [`DEFAULT_HEAP_LIMIT_BYTES`], in megabytes. `0` disables.
pub const HEAP_LIMIT_MB_ENV: &str = "OXY_FUNCTION_HEAP_LIMIT_MB";

/// The ceiling in force for this process. `None` disables it.
pub fn heap_limit_bytes() -> Option<usize> {
    static VALUE: std::sync::OnceLock<Option<usize>> = std::sync::OnceLock::new();
    *VALUE.get_or_init(|| {
        match std::env::var(HEAP_LIMIT_MB_ENV)
            .ok()
            .and_then(|v| v.trim().parse::<usize>().ok())
        {
            Some(0) => None,
            Some(mb) => Some(mb * 1024 * 1024),
            None => Some(DEFAULT_HEAP_LIMIT_BYTES),
        }
    })
}
/// How long to wait for the isolate thread to actually exit after a wall-clock
/// timeout (or cancel) has terminated execution, before giving up and returning
/// `Timeout` regardless. Bounds the worst case where the isolate is parked in a
/// not-yet-returned host call that `terminate_execution` can't interrupt.
const TIMEOUT_GRACE: std::time::Duration = std::time::Duration::from_secs(5);

/// The triggering HTTP request, as the function's `req` argument sees it.
///
/// Bundled rather than passed as three positional parameters: `run` is already
/// at the arity `internal-docs/backend-architecture.md` caps out at.
///
/// `headers` has already been filtered by the caller — the runtime does not
/// re-check it, so anything placed here reaches app code verbatim.
pub struct FnRequest {
    pub method: String,
    pub headers: std::collections::BTreeMap<String, String>,
    pub body: Vec<u8>,
}

impl FnRequest {
    /// A request carrying only a body — no real HTTP request behind it. This is
    /// what the schedule / Airway / manual-run paths synthesise, and what a test
    /// exercising handler behaviour rather than request plumbing wants.
    pub fn from_body(body: Vec<u8>) -> Self {
        Self {
            method: "POST".to_string(),
            headers: std::collections::BTreeMap::new(),
            body,
        }
    }
}

/// Run `export default async (req, ctx) => Response` from a bundled ESM
/// artifact to completion, bridging `ctx.*` calls to `host`.
///
/// `cancel` resolving (the client disconnected, or the dashboard cancel
/// flag was observed) terminates the isolate promptly via
/// `terminate_execution`. `timeout` is enforced here (not by the caller
/// wrapping this future in `tokio::time::timeout`): on elapse the isolate is
/// terminated the same way as a cancel, and we wait up to `TIMEOUT_GRACE`
/// for the isolate thread to actually exit before returning — so in the common
/// case the OS thread + V8 isolate never outlive this call. If the isolate is
/// wedged in a not-yet-returned host call (which `terminate_execution` cannot
/// interrupt), we return `Timeout` after the grace period and let that thread
/// unwind on its own once the (individually bounded) host op completes.
#[allow(clippy::too_many_arguments)]
pub async fn run(
    artifact_js: String,
    // Whose invocation this is, for the per-org concurrency ceiling. Not part
    // of `InvocationCtx` on purpose: that struct is serialized into the isolate
    // as `ctx`, and the org id is a platform fact the tenant has no business
    // reading.
    org_id: uuid::Uuid,
    ctx: InvocationCtx,
    req: FnRequest,
    host: std::sync::Arc<dyn FunctionHost>,
    mut cancel: oneshot::Receiver<()>,
    timeout: std::time::Duration,
    // Shared with the caller: the isolate appends `console.*`/`ctx.log` here and
    // the caller drains it after (surfaced back to the app, not just tracing).
    logs: Arc<std::sync::Mutex<Vec<LogLine>>>,
    // Shared with the caller for the same reason as `logs`: the isolate thread
    // can outlive this call, so what it measures cannot be a return value.
    meters: InvocationMeters,
    // The span the isolate thread runs inside. Must be PARENTLESS — see the
    // comment at the spawn below for why a clone of the caller's span is the
    // wrong thing here.
    isolate_span: tracing::Span,
) -> Result<FnResponse, RuntimeError> {
    // Admission first, before anything is allocated. The cap exists to stop the
    // OS thread and the V8 heap from being created at all, so it has to gate
    // the spawn rather than the work after it — a permit taken later would
    // limit concurrency while still paying for every thread.
    let queued_at = std::time::Instant::now();
    let org_label = org_id.to_string();
    let _admission = match super::limits::admit(org_id, &mut cancel).await {
        Ok(permit) => {
            oxy_telemetry::metrics::record::custom_app_admission_wait(
                &org_label,
                super::limits::elapsed_since(queued_at),
            );
            permit
        }
        // The caller left while queued. Nothing was refused and nobody is
        // waiting for an answer, so this is a cancellation like any other —
        // counting it as a shed would make the shed rate track how impatient
        // users are rather than how loaded the fleet is.
        Err(reason) if !reason.is_shed() => {
            tracing::debug!(
                target: "oxy.custom_app.admission",
                queued_ms = queued_at.elapsed().as_millis() as u64,
                "caller went away while queued for a concurrency permit"
            );
            return Err(RuntimeError::Cancelled);
        }
        Err(reason) => {
            oxy_telemetry::metrics::record::custom_app_admission_shed(&org_label, reason.as_str());
            tracing::warn!(
                target: "oxy.custom_app.admission",
                reason = reason.as_str(),
                queued_ms = queued_at.elapsed().as_millis() as u64,
                "shed a function invocation: no concurrency permit came free"
            );
            return Err(RuntimeError::Overloaded);
        }
    };

    let (call_tx, mut call_rx) = mpsc::unbounded_channel::<HostCall>();
    let (done_tx, done_rx) = oneshot::channel::<Result<FnResponse, RuntimeError>>();
    let (handle_tx, handle_rx) = oneshot::channel::<<ActiveEngine as IsolateEngine>::Terminate>();
    let cancelled = Arc::new(AtomicBool::new(false));

    // Isolate runs on its own thread with a current-thread runtime.
    //
    // `tracing`'s notion of "the current span" is THREAD-LOCAL, so a span
    // opened by the caller does not follow execution across this `spawn`.
    // Every `ctx.log()` / `console.*` line is emitted from the isolate thread,
    // so without a span entered *on that thread* they all fire with no current
    // span — and the observability layer drops an event that has no span.
    //
    // **`isolate_span` is deliberately PARENTLESS, not a clone of the caller's.**
    // A clone bumps the registry refcount and `on_close` fires only at zero, and
    // this thread outlives `run` on two paths: `Cancelled`, and the grace-expiry
    // branch that lets a detached thread unwind on its own. A clone would
    // therefore pin the caller's invocation span open — exporting it late, or
    // never if the thread parks in a host op forever, and inflating its
    // `duration_ns` (measured at close) to the leaked thread's whole lifetime.
    // The invocations that time out are exactly the ones worth having.
    //
    // The cost is that logs land on a sibling span rather than a child: they
    // are correlated by the `invocation_id` field both spans carry, not by the
    // trace tree. That is the right trade — a late-but-correct sibling beats a
    // parent whose duration is a lie.
    // Sentry hubs are per thread: carry this invocation's hub onto the isolate
    // thread, so a custom-app surface tag (`middlewares::sentry_surface`) still
    // covers what is captured there, panics included.
    let sentry_hub = sentry::Hub::current();
    let thread = std::thread::Builder::new()
        .name("oxy-function".into())
        .spawn({
            let cancelled = cancelled.clone();
            let meters = meters.clone();
            move || {
                // Counted from inside the closure, so a thread that failed to
                // spawn is never counted as live. The guard covers every exit
                // path below — the early `return` when the runtime fails to
                // build, a panic in tenant code, and the normal end — because a
                // missed decrement drifts this gauge upward forever.
                let _isolate_guard = oxy_telemetry::metrics::sources::IsolateGuard::enter();
                sentry::Hub::run(sentry_hub, || {
                    // Sync closure, so holding the guard for the whole thread body
                    // is correct (the "never hold across an await" rule is about
                    // async fns; everything below runs inside `block_on`).
                    let _span_guard = isolate_span.enter();
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
                        let run = IsolateRun {
                            artifact_js,
                            ctx,
                            req,
                            call_tx,
                            cancelled,
                            logs,
                            meters,
                        };
                        let result = execute_isolate::<ActiveEngine>(run, handle_tx).await;
                        let _ = done_tx.send(result);
                    });
                })
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
                    h.terminate();
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
                    h.terminate();
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
                // Deliberate, and worth counting: see `abandoned_isolates`.
                let abandoned_total = oxy_telemetry::metrics::sources::isolate_abandoned();
                tracing::warn!(
                    abandoned_total,
                    grace_secs = TIMEOUT_GRACE.as_secs(),
                    "isolate thread did not exit after termination; detaching it"
                );
                return Err(RuntimeError::Timeout);
            }
            // Service a host call from the isolate. Spawn so concurrent
            // ctx.* awaits inside one function don't serialize. Each call runs
            // under its own span, a child of the invocation span `run` is
            // being polled in — `tokio::spawn` carries no span by itself, so
            // without `.instrument` every warehouse query and outbound fetch
            // was a root trace of its own.
            maybe_call = call_rx.recv() => {
                match maybe_call {
                    Some(call) => {
                        // Counted here rather than at the op sites: this is the
                        // one place every `ctx.*` call funnels through, so the
                        // count cannot drift as ops are added.
                        meters.host_calls.fetch_add(1, Ordering::Relaxed);
                        let host = host.clone();
                        let (span, kind, op) = host_call_span(&call);
                        // Read before dispatch consumes the call: the note
                        // keys a later success to the same target.
                        let target = host_call_target(&call);
                        tokio::spawn(
                            async move {
                                // `dispatch_host_call` consumes its host.
                                let host_note = host.clone();
                                let (reply, result) = dispatch_host_call(call, host).await;
                                let labels = host_note.decides_by_environment();
                                record_host_call_outcome(kind, &result, labels);
                                // Before the reply: once the handler can catch
                                // the error, the failure is already noted.
                                match &result {
                                    Err(message) => {
                                        let error_kind =
                                            super::host_call_attrs::classify_host_error_for(
                                                message, labels,
                                            );
                                        if super::host_call_attrs::counts_toward_paging(error_kind)
                                        {
                                            host_note.note_host_call_failure(
                                                op,
                                                error_kind,
                                                message,
                                                target.as_deref(),
                                            );
                                        }
                                    }
                                    Ok(_) => host_note.note_host_call_success(op, target.as_deref()),
                                }
                                let _ = reply.send(result);
                            }
                            .instrument(span)
                            // The invocation's Sentry hub, for the same reason as
                            // the isolate thread above.
                            .bind_hub(sentry::Hub::current()),
                        );
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

/// What one isolate needs from [`run`], moved onto the isolate thread.
struct IsolateRun {
    artifact_js: String,
    ctx: InvocationCtx,
    req: FnRequest,
    call_tx: mpsc::UnboundedSender<HostCall>,
    cancelled: Arc<AtomicBool>,
    logs: Arc<std::sync::Mutex<Vec<LogLine>>>,
    meters: InvocationMeters,
}

/// Body that runs on the isolate thread.
///
/// Wraps [`execute_isolate_inner`] only to translate a heap-limit kill. A
/// terminated isolate surfaces as a generic "execution terminated" JS error,
/// byte-identical to the one a cancel or a timeout produces — the flag the
/// near-limit callback sets is the only thing that knows which it was.
async fn execute_isolate<E: IsolateEngine>(
    run: IsolateRun,
    handle_tx: oneshot::Sender<E::Terminate>,
) -> Result<FnResponse, RuntimeError> {
    let oom = Arc::new(AtomicBool::new(false));
    let result = execute_isolate_inner::<E>(run, handle_tx, Arc::clone(&oom)).await;

    // Checked on the error path only: a function that allocated hard, tripped
    // the callback, and *still* returned a response has not failed, and
    // rewriting its success into an error would be a lie about what the app saw.
    if result.is_err() && oom.load(Ordering::Relaxed) {
        return Err(RuntimeError::OutOfMemory);
    }
    result
}

/// The module every function artifact is loaded as; the invoke script imports
/// it by this name.
const FUNCTION_MODULE: &str = "oxy:function";

/// Loads + evaluates the module, calls the default export, and returns the
/// parsed `FnResponse`.
async fn execute_isolate_inner<E: IsolateEngine>(
    run: IsolateRun,
    handle_tx: oneshot::Sender<E::Terminate>,
    oom: Arc<AtomicBool>,
) -> Result<FnResponse, RuntimeError> {
    let IsolateRun {
        artifact_js,
        ctx,
        req,
        call_tx,
        cancelled,
        logs,
        meters,
    } = run;
    let config = IsolateConfig {
        heap_limit: heap_limit_bytes(),
        oom,
    };
    let ops = OpContext {
        call_tx,
        cancelled,
        logs: FunctionLogs(logs),
    };
    let mut engine = E::create(config, ops);
    let _ = handle_tx.send(engine.terminate_handle());

    engine
        .execute_script("oxy:bootstrap", BOOTSTRAP_JS)
        .map_err(|e| RuntimeError::Internal(format!("bootstrap failed: {e}")))?;

    let ctx_json = serde_json::to_string(&ctx)
        .map_err(|e| RuntimeError::Internal(format!("ctx serialize failed: {e}")))?;
    let FnRequest {
        method,
        headers,
        body,
    } = req;
    // `req` without its body, which the script collects from `RequestBody`.
    let req_json = serde_json::json!({ "method": method, "headers": headers }).to_string();
    // Valid UTF-8 is moved, not copied. Anything else is decoded lossily, as it
    // always was: `req.body` is text.
    let body = String::from_utf8(body)
        .unwrap_or_else(|e| String::from_utf8_lossy(e.as_bytes()).into_owned());

    let module = engine.load_module(FUNCTION_MODULE, artifact_js).await?;

    // The setup/tenant boundary, and it sits here rather than after
    // `evaluate_module` on purpose.
    //
    // Everything above is ours: OS thread spawn, isolate creation, the
    // bootstrap script, and compiling the tenant's module. `evaluate_module`
    // below runs the module's **top-level statements**, which are tenant code —
    // an app doing heavy work at module scope is the app's own cost, and folding
    // it into `init_ms` would let a slow function look like a slow platform.
    //
    // So `init_ms` is what we are responsible for, and `duration_ms - init_ms`
    // is what the app is.
    meters.mark_tenant_code_entered();

    engine.evaluate_module(module).await?;

    // Parked only now, after the module's top-level statements have run, and
    // collected by the script's first statements — ahead of `__buildCtx`, which
    // is a global the artifact could have replaced with code of its own.
    // `body` is assigned after the literal, which keeps `req`'s keys in the
    // order they have always had.
    engine.park_request_body(body);

    let value = engine
        .invoke("oxy:invoke", invoke_script(&req_json, &ctx_json))
        .await?;
    serde_json::from_value(value)
        .map_err(|e| RuntimeError::Internal(format!("result shape invalid: {e}")))
}

/// The script that calls the function's default export. Its completion value
/// is the promise of `{ status, body }`.
fn invoke_script(req_json: &str, ctx_json: &str) -> String {
    format!(
        r#"
        (async () => {{
            const req = {req_json};
            req.body = Deno.core.ops.op_req_take_body();
            const ctx = globalThis.__buildCtx({ctx_json});
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
    )
}
