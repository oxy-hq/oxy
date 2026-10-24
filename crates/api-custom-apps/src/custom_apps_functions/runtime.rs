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

use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc, oneshot};

mod call_label;
#[cfg(test)]
mod call_label_tests;
mod engine;
mod host_call;
mod host_call_span;
mod host_call_task;
mod ops;
#[cfg(test)]
mod snapshot_tests;
#[cfg(test)]
mod test_calls;
#[cfg(test)]
mod tests;
mod warm_pool;
#[cfg(test)]
mod warm_pool_tests;

use super::LogLine;
use super::env_policy::{EnvPolicy, HeldTarget, HostOp};
use super::host::FunctionCapabilities;
use super::host_error::HostError;
use crate::custom_apps_telemetry::InvocationMeters;
use engine::{ActiveEngine, IsolateConfig, IsolateEngine, TerminateHandle};
use host_call::HostCall;
#[cfg(test)]
use host_call::{dispatch_host_call, host_call_span, host_call_target};
use ops::{FunctionLogs, OpContext};
use warm_pool::Job;

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
    pub reach: oxy_app_core::operating_graph::reach::Reach,
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
    async fn query(&self, sql: String) -> Result<serde_json::Value, HostError>;
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
    async fn query_stream(&self, sql: String) -> Result<serde_json::Value, HostError>;
    /// `ctx.fetch(url, init)` — SSRF-allowlisted outbound HTTP with a
    /// response size cap. Returns `{ status, body }`.
    async fn fetch(
        &self,
        url: String,
        init: serde_json::Value,
    ) -> Result<serde_json::Value, HostError>;
    /// `ctx.semantic.query(spec)` — airlayer-compiled semantic query.
    /// `spec` is the JSON-encoded `agentic_semantic::config::SemanticQueryConfig`.
    async fn semantic_query(&self, spec: serde_json::Value)
    -> Result<serde_json::Value, HostError>;

    /// `ctx.org.people()` — the org's people directory, read-only.
    ///
    /// Takes no arguments on purpose: the org is the host's, so a function
    /// cannot name one and a manifest cannot point this at another tenant.
    ///
    /// Defaulted rather than required, because every other implementor of this
    /// trait is a test double that has no directory to answer with — and the
    /// default is the same fail-closed refusal the capability gate gives, so a
    /// double that forgets to override it denies rather than inventing people.
    async fn org_people(&self) -> Result<serde_json::Value, HostError> {
        Err(HostError::backend(
            "ctx.org.people is not available in this host",
        ))
    }
    /// `ctx.org.places()` — the org's locations. Same shape and same
    /// fail-closed default as `org_people`.
    async fn org_places(&self) -> Result<serde_json::Value, HostError> {
        Err(HostError::backend(
            "ctx.org.places is not available in this host",
        ))
    }
    /// `ctx.org.assignments()` — who holds which position where.
    async fn org_assignments(&self) -> Result<serde_json::Value, HostError> {
        Err(HostError::backend(
            "ctx.org.assignments is not available in this host",
        ))
    }
    /// `ctx.platform.facts(opts)` — ids and counts about every org, for an
    /// internal app. `opts` carries `weeks` only; the org is the host's. The
    /// same fail-closed default as `org_people`.
    async fn platform_facts(
        &self,
        _opts: serde_json::Value,
    ) -> Result<serde_json::Value, HostError> {
        Err(HostError::backend(
            "ctx.platform.facts is not available in this host",
        ))
    }
    /// `ctx.airway.run(pipelineRef, variables)` — seed an Airway ELT run.
    /// Returns `{ runId }`; the run is driven asynchronously by the worker
    /// fleet (it does not block on completion — ELT runs routinely exceed
    /// the function timeout ceiling).
    async fn airway_run(
        &self,
        pipeline_ref: String,
        variables: serde_json::Value,
    ) -> Result<serde_json::Value, HostError>;
    /// `ctx.warehouse.query(database, sql)` — a read against a named database.
    async fn warehouse_query(
        &self,
        payload: serde_json::Value,
    ) -> Result<serde_json::Value, HostError>;
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
    ) -> Result<serde_json::Value, HostError>;
    /// `ctx.secrets.set(key, value)` — upsert an app-scoped secret
    /// (`apps/<app_id>/<key>`) into the same namespace `ctx.env` reads. Gated
    /// by the fail-closed `secrets.write` manifest capability.
    async fn secrets_set(&self, key: String, value: String)
    -> Result<serde_json::Value, HostError>;
    /// `ctx.email.send(input)` — send email on behalf of the app. Gated by the
    /// fail-closed `email.send` manifest capability; the platform controls the
    /// `from` address (the author may set `replyTo` only). `input` is the JS
    /// payload object; returns `{ messageId }`.
    async fn send_email(&self, input: serde_json::Value) -> Result<serde_json::Value, HostError>;
    /// `ctx.notify(input)` — tell people in the org something, by user, role
    /// at a location, or team. Gated by the fail-closed `notify` capability;
    /// the platform resolves recipients and channels and never hands the
    /// function an address. Returns `{ notificationId, recipients, skipped }`.
    async fn notify(&self, input: serde_json::Value) -> Result<serde_json::Value, HostError>;
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
    ) -> Result<serde_json::Value, HostError>;
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
    async fn tx(
        &self,
        op: String,
        payload: serde_json::Value,
    ) -> Result<serde_json::Value, HostError>;
    /// `ctx.oltp.{query,exec}` — read/write the app's OWN per-org OLTP schema
    /// (`app_<writer>`) on the managed Postgres tenant. `op` is `query` or
    /// `exec`; `payload` carries `{ sql, params? }`. Gated by the fail-closed
    /// `oltp` manifest capability, and scoped to the app's own writer role — so unlike `ctx.warehouse` (read-only analyst on a
    /// managed database) it can write, and cannot see another app's data.
    async fn oltp(
        &self,
        op: String,
        payload: serde_json::Value,
    ) -> Result<serde_json::Value, HostError>;
    /// `ctx.airhouse.{query,exec,append}` — the app's own facts in its
    /// workspace's Airhouse, in the schema `app_<writer>` derived from its slug.
    /// Written as the app whoever invoked; every statement is checked against
    /// that schema first. Gated by the fail-closed `airhouse` manifest capability.
    async fn airhouse(
        &self,
        op: String,
        payload: serde_json::Value,
    ) -> Result<serde_json::Value, HostError>;
}

/// Bootstrap script: polyfills the minimal `Response` the function author's
/// code expects, and assembles `globalThis.__buildCtx` from the host ops.
///
/// Kept in its own file because the engine builds a startup snapshot from it
/// (`engine::IsolateConfig::bootstrap`): every isolate starts with these
/// definitions already in its global scope instead of parsing and running the
/// script again.
const BOOTSTRAP_JS: &str = include_str!("runtime/bootstrap.js");

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
    org_id: uuid::Uuid,
    ctx: InvocationCtx,
    req: FnRequest,
    host: std::sync::Arc<dyn FunctionHost>,
    cancel: oneshot::Receiver<()>,
    timeout: std::time::Duration,
    logs: Arc<std::sync::Mutex<Vec<LogLine>>>,
    meters: InvocationMeters,
    isolate_span: tracing::Span,
) -> Result<FnResponse, RuntimeError> {
    run_in(
        warm_pool::global(),
        artifact_js,
        org_id,
        ctx,
        req,
        host,
        cancel,
        timeout,
        logs,
        meters,
        isolate_span,
    )
    .await
}

/// [`run`] on an explicit warm pool, so a test can pick the pool's size.
#[allow(clippy::too_many_arguments)]
async fn run_in(
    pool: &Arc<warm_pool::WarmPool>,
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

    // Sentry hubs are per thread: the job carries this invocation's hub onto
    // the isolate thread (see `spawn_cold`). The thread comes from the warm
    // pool when one is ready, and is built here otherwise.
    warm_pool::start_isolate(
        pool,
        Job {
            run: IsolateRun {
                artifact_js,
                ctx,
                req,
                call_tx,
                cancelled: cancelled.clone(),
                logs,
                meters: meters.clone(),
            },
            handle_tx,
            done_tx,
            hub: sentry::Hub::current(),
            span: isolate_span,
        },
    )?;

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
            // Service a host call from the isolate on a task of its own, so
            // concurrent ctx.* awaits inside one function don't serialize
            // (`host_call_task`: its span, its failure note, its trace record).
            maybe_call = call_rx.recv() => {
                match maybe_call {
                    Some(call) => host_call_task::spawn_host_call(call, host.clone(), &meters),
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

/// Build the isolate thread on the request path — the thread, its Tokio
/// runtime and the isolate — and run `job` on it. What every invocation did
/// before the warm pool, and what one still does when the pool has nothing
/// ready or is off (`OXY_FUNCTION_WARM_ISOLATES=0`).
fn spawn_cold(job: Job) -> std::io::Result<()> {
    let Job {
        run,
        handle_tx,
        done_tx,
        hub: sentry_hub,
        span: isolate_span,
    } = job;
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
    std::thread::Builder::new()
        .name("oxy-function".into())
        .spawn(move || {
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
                    let result = execute_isolate::<ActiveEngine>(run, handle_tx).await;
                    let _ = done_tx.send(result);
                });
            })
        })
        .map(|_| ())
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
    let engine = E::create(IsolateConfig {
        heap_limit: heap_limit_bytes(),
        bootstrap: BOOTSTRAP_JS,
    })
    .map_err(|e| RuntimeError::Internal(format!("bootstrap failed: {e}")))?;
    execute_on_engine(engine, run, handle_tx).await
}

/// [`execute_isolate`] from the point an isolate exists — whether it was
/// created just now or built ahead of demand by the warm pool.
async fn execute_on_engine<E: IsolateEngine>(
    mut engine: E,
    run: IsolateRun,
    handle_tx: oneshot::Sender<E::Terminate>,
) -> Result<FnResponse, RuntimeError> {
    let oom = engine.oom_flag();
    let _ = handle_tx.send(engine.terminate_handle());
    let result = execute_isolate_inner(&mut engine, run).await;

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
    engine: &mut E,
    run: IsolateRun,
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
    // The ops read this at call time, not at creation: the isolate does not
    // know whose invocation it serves until here.
    engine.bind_invocation(OpContext {
        call_tx,
        cancelled,
        logs: FunctionLogs(logs),
    });

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
