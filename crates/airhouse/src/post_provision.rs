//! Process-wide post-provision hook registry.
//!
//! Domain crates (cameras, future others) register an async callback at
//! app startup; `TenantProvisioner::provision()` invokes every hook
//! after a successful provision, in registration order.
//!
//! ## Why a callback registry and not a trait
//!
//! `airhouse` is an infrastructure crate; it must not import any domain
//! crate (the hard rule in `internal-docs/backend-architecture.md`).
//! The hook pattern inverts the dependency: domains depend on
//! airhouse, register with it at startup, and airhouse calls back
//! without knowing what's behind the callback.
//!
//! ## Failure posture
//!
//! Hook failures are **logged, not fatal**. Reasoning:
//! - The hook's contract is "make per-tenant setup more eager"; the
//!   actual functionality already has a lazy fallback (see
//!   `oxy_cameras::airhouse::connect_and_ensure`). If the hook fails,
//!   first ingest will re-attempt the DDL.
//! - Failing the provision because a downstream DDL hiccupped would
//!   leave the user staring at "couldn't connect Airhouse" when the
//!   tenant *was* in fact provisioned successfully.
//! - Hooks should themselves be idempotent (CREATE TABLE IF NOT
//!   EXISTS, ON CONFLICT DO NOTHING, etc.) so retry on next provision
//!   is safe.

use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, OnceLock, RwLock};

use uuid::Uuid;

/// Async callback fired after a successful `TenantProvisioner::provision`
/// for a given workspace. Implementors do **idempotent** per-tenant
/// setup work (DDL, default-row inserts, schema-ensure).
pub type PostProvisionHook =
    Arc<dyn Fn(Uuid) -> Pin<Box<dyn Future<Output = Result<(), HookError>> + Send>> + Send + Sync>;

/// Error type for post-provision hooks. Boxed so the registry stays
/// dyn-compatible without bleeding domain error types into airhouse.
pub type HookError = Box<dyn std::error::Error + Send + Sync>;

fn registry() -> &'static RwLock<Vec<(&'static str, PostProvisionHook)>> {
    static R: OnceLock<RwLock<Vec<(&'static str, PostProvisionHook)>>> = OnceLock::new();
    R.get_or_init(|| RwLock::new(Vec::new()))
}

/// Register a post-provision hook. Call at app startup before the HTTP
/// server starts accepting `/airhouse/me/provision` requests. `name`
/// appears in logs to identify which hook failed when one does.
pub fn register_post_provision_hook(name: &'static str, hook: PostProvisionHook) {
    registry()
        .write()
        .expect("post-provision registry poisoned")
        .push((name, hook));
    tracing::info!(hook = name, "registered post-provision hook");
}

/// Invoke every registered hook for `workspace_id`. Used internally by
/// the provisioner; tests can also call this to exercise the hook path
/// without going through a full provision.
pub async fn invoke_all(workspace_id: Uuid) {
    // Clone the Arcs out under the lock so the await points below don't
    // hold the lock (a hook could call back into `register_post_provision_hook`
    // — unusual but cheap to guard against).
    let hooks: Vec<(&'static str, PostProvisionHook)> = registry()
        .read()
        .expect("post-provision registry poisoned")
        .clone();
    for (name, hook) in hooks {
        match hook(workspace_id).await {
            Ok(()) => {
                tracing::debug!(hook = name, workspace_id = %workspace_id, "post-provision hook ok");
            }
            Err(e) => {
                tracing::warn!(
                    hook = name,
                    workspace_id = %workspace_id,
                    error = %e,
                    "post-provision hook failed (will be retried lazily on first use)"
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[tokio::test]
    async fn hook_invoked_with_workspace_id() {
        // Counter increments on every call; assert hook ran once per
        // workspace_id provided.
        static CALLS: AtomicUsize = AtomicUsize::new(0);
        register_post_provision_hook(
            "test-counter",
            Arc::new(|_wid| {
                Box::pin(async move {
                    CALLS.fetch_add(1, Ordering::SeqCst);
                    Ok(())
                })
            }),
        );
        let before = CALLS.load(Ordering::SeqCst);
        invoke_all(Uuid::new_v4()).await;
        invoke_all(Uuid::new_v4()).await;
        assert_eq!(CALLS.load(Ordering::SeqCst), before + 2);
    }

    #[tokio::test]
    async fn failing_hook_does_not_panic_other_hooks() {
        static FOLLOWUP_CALLED: AtomicUsize = AtomicUsize::new(0);
        register_post_provision_hook(
            "test-failing",
            Arc::new(|_wid| Box::pin(async move { Err("intentional".into()) })),
        );
        register_post_provision_hook(
            "test-followup",
            Arc::new(|_wid| {
                Box::pin(async move {
                    FOLLOWUP_CALLED.fetch_add(1, Ordering::SeqCst);
                    Ok(())
                })
            }),
        );
        let before = FOLLOWUP_CALLED.load(Ordering::SeqCst);
        invoke_all(Uuid::new_v4()).await;
        assert!(FOLLOWUP_CALLED.load(Ordering::SeqCst) > before);
    }
}
