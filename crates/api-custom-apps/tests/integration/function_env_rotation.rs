//! A rotated app secret is what the next call's `ctx.env` reads — from any
//! writer, on any replica.
//!
//! `ctx.env` is served from a per-replica cache (`custom_apps_functions::
//! env_cache`), validated on every hit by one stamp read over the app's
//! secret rows. The writers that matter are the ones that do NOT run this
//! replica's invalidation hook: the staff secrets surface on *another*
//! replica, and a function's own `ctx.secrets.set` (the host, not the
//! surface). Both reduce to a `SecretManagerService` write with no hook,
//! which is exactly what this test performs between two calls through the
//! real serve route.

use oxy::service::secret_manager::SecretManagerService;
use serde_json::json;

use crate::common::demo_workspace_id;
use crate::custom_app_functions_fixture::{
    FunctionSpec, call_function, publish_app, seeded_tenant,
};
use crate::staging_functions::data;

const ENV_JS: &str = "export default async (req, ctx) => Response.json({ env: ctx.env });";

/// 32 zero bytes, base64 — `get_encryption_key` otherwise generates one and
/// writes it into the state dir.
fn use_a_fixed_encryption_key() {
    // SAFETY: nextest runs each test in its own process, and this happens before
    // any secret is encrypted.
    unsafe {
        std::env::set_var(
            "OXY_ENCRYPTION_KEY",
            "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=",
        );
    }
}

#[tokio::test]
async fn a_rotation_by_another_writer_is_read_by_the_next_call() {
    use_a_fixed_encryption_key();
    let t = seeded_tenant().await;
    let app = "env-rotate";
    let published = publish_app(
        &t,
        app,
        demo_workspace_id(),
        &[FunctionSpec {
            name: "env",
            manifest: json!({ "route": true }),
            js: ENV_JS,
        }],
    )
    .await;
    let id = published.app_id;
    let secrets = SecretManagerService::new(demo_workspace_id());
    secrets
        .set_app_secret(&t.db, id, "API_KEY", "v1", t.guest_id)
        .await
        .expect("seed secret");

    let first = call_function(app, "env", json!({})).await;
    assert_eq!(
        data(&first)["env"],
        json!({ "API_KEY": "v1" }),
        "{}",
        first.raw
    );
    // The second read of an unchanged store is the cache's to answer.
    let again = call_function(app, "env", json!({})).await;
    assert_eq!(
        data(&again)["env"],
        json!({ "API_KEY": "v1" }),
        "{}",
        again.raw
    );

    // Rotated with no invalidation hook — what another replica's secrets
    // surface, or a function's `ctx.secrets.set`, amounts to on this one.
    secrets
        .set_app_secret(&t.db, id, "API_KEY", "v2", t.guest_id)
        .await
        .expect("rotate");
    let rotated = call_function(app, "env", json!({})).await;
    assert_eq!(
        data(&rotated)["env"],
        json!({ "API_KEY": "v2" }),
        "the rotation is read by the very next call, well inside the cache's TTL: {}",
        rotated.raw
    );

    // A delete deactivates the row in place: no fewer rows, but a newer one.
    secrets
        .delete_secret(&t.db, &format!("apps/{id}/API_KEY"))
        .await
        .expect("delete");
    let deleted = call_function(app, "env", json!({})).await;
    assert_eq!(
        data(&deleted)["env"],
        json!({}),
        "a delete is read by the next call too: {}",
        deleted.raw
    );
}
