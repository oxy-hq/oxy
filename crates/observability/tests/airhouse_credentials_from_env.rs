//! End-to-end smoke test for the env-var-backed observability credential path
//! against a local `docker-compose.airhouse.yml` stack.
//!
//! Marked `#[ignore]` so CI never runs it; run locally with:
//!
//! ```bash
//! docker compose -f docker-compose.airhouse.yml up -d
//! cargo nextest run -p oxy-observability --features airhouse \
//!   --run-ignored ignored-only airhouse_obs_connects_with_env_credentials
//! ```
//!
//! The compose bootstrap step pre-creates a `demo` tenant with an
//! `alice/alice` admin user, so the three OBS env vars below match
//! `docker-compose.airhouse.yml` defaults verbatim.

#![cfg(feature = "airhouse")]

use oxy_observability::backends::airhouse::{AirhouseObservabilityStorage, credentials_from_env};

/// Reproduces the post-#2356 wiring: caller supplies a host/port + a
/// CredentialFn, then constructs the storage. Before this fix the standard
/// wiring used the SA-backed broker with `Uuid::nil()` and failed with
/// `TenantNotFound` because no airhouse tenant exists at the nil workspace.
/// This test exercises the env-var path that replaces it.
#[tokio::test]
#[ignore = "requires docker compose -f docker-compose.airhouse.yml up -d"]
async fn airhouse_obs_connects_with_env_credentials() {
    // SAFETY: this integration test owns its own process; no other code reads
    // these vars concurrently.
    unsafe {
        std::env::set_var("OXY_AIRHOUSE_OBS_USER", "alice");
        std::env::set_var("OXY_AIRHOUSE_OBS_PASSWORD", "alice");
        std::env::set_var("OXY_AIRHOUSE_OBS_DATABASE", "demo");
    }

    let get_credentials =
        credentials_from_env().expect("env vars set above must produce a CredentialFn");

    // Successful connect exercises the full surface area of this fix:
    // credentials_from_env resolves the three env vars, the CredentialFn is
    // invoked, pgwire SCRAM auth succeeds against the compose stack, and the
    // reconnect driver spawns. We deliberately do NOT call ensure_schema():
    // the DuckLake backend the compose stack ships with does not support
    // CREATE INDEX, and that failure is unrelated to the credential path
    // this test guards.
    let _storage = AirhouseObservabilityStorage::connect(
        "localhost",
        5445,
        true, // insecure: local compose stack does not terminate TLS
        get_credentials,
    )
    .await
    .expect("connect must succeed against the compose stack");
}
