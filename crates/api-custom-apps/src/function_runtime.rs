//! The function runtime as the object `oxy-app` calls it through, and the boot
//! step that publishes its limits.

use std::sync::Arc;

use oxy_app_core::function_seam::{
    FunctionQueryExecutor, FunctionRequest, FunctionResponse, FunctionRuntime,
};

/// Runs a `/fn/<name>` request through the runtime with `executor` as the
/// data-plane query executor.
///
/// Handed to `oxy-app` as `SurfaceSeams::function_runtime`, for the handlers
/// there that run a function on a person's behalf (a notification's function
/// action); this crate's own serve route calls the runtime directly.
pub struct Runtime {
    executor: Arc<dyn FunctionQueryExecutor>,
}

impl Runtime {
    pub fn new(executor: Arc<dyn FunctionQueryExecutor>) -> Self {
        Self { executor }
    }
}

impl FunctionRuntime for Runtime {
    fn handle<'a>(&'a self, request: FunctionRequest<'a>) -> FunctionResponse<'a> {
        crate::custom_apps_functions::handle_function_request(
            request.org_slug,
            request.app_slug,
            request.function_name,
            request.method,
            request.uri,
            request.headers,
            request.body,
            self.executor.clone(),
            request.preagg,
        )
    }
}

/// Resolve the function concurrency ceilings and the isolate heap limit at boot.
///
/// Each of these is a `OnceLock` that also PUBLISHES a gauge as a side effect of
/// being resolved. Left to be resolved by first use, each gauge reads 0 until
/// then — and 0 is also what "this limit is disabled" looks like, so over the
/// whole boot-to-first-request window the two are indistinguishable and the
/// saturation ratios divide by zero.
///
/// THE CALLS MUST BE `let` BINDINGS, NOT FIELD EXPRESSIONS. `tracing` evaluates a
/// field's value expression only when the callsite is enabled — the macro wraps
/// the value set in `if enabled { … }`. The default `OXY_LOG_LEVEL` is `warn`, the
/// OTel layer takes events only at <= WARN, and the OTLP log bridge is opt-in — so
/// on a default serve an `info!` is disabled everywhere and anything called inside
/// its field list NEVER RUNS. Prod happened to be covered only because it runs at
/// `info`.
#[cfg(feature = "custom-app-functions")]
pub fn resolve_limits_at_boot() {
    use crate::custom_apps_functions::{limits, runtime};
    let max_concurrency = limits::max_concurrency();
    let max_org_concurrency = limits::max_org_concurrency();
    let max_queued = limits::max_queued();
    let queue_budget_secs = limits::queue_budget().as_secs();
    let heap_limit_bytes = runtime::heap_limit_bytes();
    tracing::info!(
        target: "oxy.custom_app.admission",
        max_concurrency,
        max_org_concurrency,
        max_queued,
        queue_budget_secs,
        heap_limit_bytes = ?heap_limit_bytes,
        "custom-app function limits"
    );
}

/// Without the V8 runtime the ceilings mean nothing.
#[cfg(not(feature = "custom-app-functions"))]
pub fn resolve_limits_at_boot() {}
