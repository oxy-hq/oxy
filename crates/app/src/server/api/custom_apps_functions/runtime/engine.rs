//! The seam between the Oxy Functions runtime and the JS engine that runs a
//! function.
//!
//! [`run`](super::run) owns everything engine-neutral — admission, the
//! isolate thread, the broker loop, timeouts and cancellation, the bootstrap
//! script and the invoke script — and drives one isolate through
//! [`IsolateEngine`]. An engine owns only what is genuinely its own: creating
//! an isolate under a heap ceiling (started from a snapshot of the bootstrap
//! script where the engine has one), binding the `ctx.*` ops (whose logic is in
//! [`super::ops`]), compiling and evaluating the tenant's ES module, running a
//! script to a JSON value, and stopping itself from another thread.
//!
//! `deno_core` ([`deno`]) is the only implementation. The trait exists so a
//! second engine can be added beside it without touching the runtime around
//! it.
//!
//! ## What an engine must preserve
//!
//! - **Op names and the `Deno.core.ops` surface.** `BOOTSTRAP_JS` and the
//!   invoke script call `Deno.core.ops.op_*`; another engine must expose the
//!   same names under the same global, or the bootstrap must change with it.
//! - **Error shapes.** Each method documents the [`RuntimeError`] it returns.
//!   The messages are part of what an app and its logs see, so they are the
//!   engine's contract, not its choice.
//! - **Heap-limit reporting.** When the engine terminates an isolate for
//!   nearing its ceiling it sets the flag [`IsolateEngine::oom_flag`] returns
//!   *before* terminating, so the caller can tell that kill from a timeout or
//!   a cancel.
//! - **Per-invocation state is bound after creation.** Everything one
//!   invocation owns (the broker channel, the cancellation flag, the log
//!   buffer, the parked request body) reaches the ops through the isolate's
//!   op state at call time ([`IsolateEngine::bind_invocation`]), never through
//!   a value captured when the isolate was created. That is what lets the
//!   state be replaced, and cleared, without rebuilding the isolate.

use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use super::RuntimeError;
use super::ops::OpContext;

pub(super) mod deno;

/// The engine every isolate runs on.
pub(super) type ActiveEngine = deno::DenoEngine;

/// Stops a running isolate from another thread. Sent to the broker loop once
/// the isolate exists, so a timeout or cancel can interrupt even a
/// compute-only loop (design doc §11.4).
///
/// Terminating interrupts JS execution only; a host call already dispatched
/// still runs to completion, which is why every op also checks
/// [`OpContext::check_cancelled`].
pub(super) trait TerminateHandle: Send + 'static {
    fn terminate(&self);
}

/// How to create one isolate.
pub(super) struct IsolateConfig {
    /// Heap ceiling in bytes; `None` disables it (`heap_limit_bytes`).
    pub heap_limit: Option<usize>,
    /// The script that defines `Response`, `console` and `__buildCtx` in the
    /// global scope (`BOOTSTRAP_JS`). The same text for every isolate in the
    /// process, which is what lets an engine run it once and start every
    /// isolate from the result.
    pub bootstrap: &'static str,
}

/// One JS isolate, driven through a single invocation. Lives and dies on the
/// isolate thread, so it need not be `Send`.
pub(super) trait IsolateEngine: Sized {
    type Terminate: TerminateHandle;
    /// A compiled-but-not-evaluated module.
    type Module;

    /// Create the isolate under `config.heap_limit` with the `ctx.*` ops
    /// registered and `config.bootstrap` already in effect, whether the
    /// engine ran it now or restored it from a snapshot of the same text.
    /// `Err` carries the script's error, unprefixed. No invocation is bound
    /// yet: an op called before [`Self::bind_invocation`] finds no
    /// [`OpContext`] and refuses.
    fn create(config: IsolateConfig) -> Result<Self, String>;

    /// Set when the engine terminates the isolate for nearing its heap
    /// ceiling — *before* terminating, so the error path cannot observe the
    /// termination without the reason. `None`-limit isolates never set it.
    fn oom_flag(&self) -> Arc<AtomicBool>;

    /// Park `ops` where the `ctx.*` ops read them at call time, replacing
    /// whatever invocation was bound before, and clear the previous
    /// invocation's parked request body.
    fn bind_invocation(&mut self, ops: OpContext);

    /// The handle the broker loop terminates this isolate through.
    fn terminate_handle(&mut self) -> Self::Terminate;

    /// Compile `code` as a side ES module under `specifier`, without
    /// evaluating it. `Internal("bad module specifier: …")` when `specifier`
    /// is not a valid URL; `Js("module load failed: …")` when the code does
    /// not compile.
    async fn load_module(
        &mut self,
        specifier: &'static str,
        code: String,
    ) -> Result<Self::Module, RuntimeError>;

    /// Evaluate the module's top-level statements, driving the event loop
    /// until they settle. `Js(<event-loop error>)` when the loop fails;
    /// `Js("module evaluation failed: …")` when evaluation does.
    async fn evaluate_module(&mut self, module: Self::Module) -> Result<(), RuntimeError>;

    /// Park the request body for `op_req_take_body` to hand over, once.
    fn park_request_body(&mut self, body: String);

    /// Run `source` (whose completion value is a promise), drive the event
    /// loop until the promise settles, and return its value as JSON.
    /// `Js("invoke failed: …")` when the script will not run;
    /// `Js(<error>)` when the promise rejects or the loop fails;
    /// `Internal("result deserialize failed: …")` when the value is not JSON.
    async fn invoke(
        &mut self,
        name: &'static str,
        source: String,
    ) -> Result<serde_json::Value, RuntimeError>;
}
