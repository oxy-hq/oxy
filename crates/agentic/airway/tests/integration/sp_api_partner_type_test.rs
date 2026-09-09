//! `partner_type` in a pipeline's config must decide which SP-API reports the
//! pipeline can reach.
//!
//! Seller Central and Vendor Central are separate Amazon accounts with separate
//! credentials. `resources()` publishes only the reports the configured account
//! can pull, so this is not a filter over one roster — it selects which half a
//! pipeline sees, and a pipeline naming no subset runs exactly that half.
//!
//! ## Why this is an integration test and not a unit test
//!
//! It has to call `build_source_connector`, which constructs an HTTP client,
//! which reads airway's process-global deployment config — a `OnceCell` that is
//! set once per PROCESS and cannot be reset. `deployment_config_tests` in this
//! same crate installs a `tls_ca_cert` of `/etc/pki/ca.pem`, a path that does
//! not exist, so every `build_source_connector` call in the lib test binary
//! after it fails with a TLS error unrelated to what it was testing. Six tests
//! in `source_factory` are red on `main` for exactly that reason, two of them
//! sp_api's own.
//!
//! The integration GROUP runs in its own process, so it gets a fresh
//! `OnceCell`. A `mod` here rather than a new `tests/*.rs`, per this crate's
//! grouping rule: a test FILE costs a link, and the isolation this needs comes
//! from the process, which the group already provides.
//!
//! A sibling module DOES install that global, and it is worth knowing which
//! way: `worker_integration` calls `Worker::execute`, which reaches
//! `deployment_config::install_once`, which loads the `airway_deployment_config`
//! row and installs it. On a throwaway testcontainer there is no configured
//! row, so `unwrap_or_default()` installs defaults and these tests pass.
//!
//! Point `OXY_DATABASE_URL` at a database carrying a real row with a
//! `tls_ca_cert` — which the sibling modules document as a supported way to run
//! them — and these would inherit it and fail on exactly the TLS error the move
//! escaped. The race runs the other way too: reach the global from here first
//! and `install_once` logs that the worker's values did not take effect.
//!
//! So the guarantee is "defaults, on a clean database", not "nothing else
//! touches it". The first draft of this comment claimed the second, having
//! looked at the module list rather than at what the modules do.
//!
//! The alternative was to add two more tests to the broken set in the lib
//! binary and call the wiring covered, which it would not have been.

use agentic_airway::config::SourceConfig;
use agentic_airway::source_factory::build_source_connector;
use airway::connector::Environment;
use serde_json::{Value, json};

/// The config an sp_api pipeline carries, minus whatever the test varies.
fn sp_api_config(partner_type: Option<&str>) -> SourceConfig {
    let mut obj = json!({
        "client_id": "amzn1.application-oa2-client.x",
        "client_secret": "secret",
        "refresh_token": "Atzr|refresh",
        "marketplace_id": "A2EUQ1WTGCTBG2",
        "default_start": "2026-01-01",
    })
    .as_object()
    .expect("object")
    .clone();
    if let Some(pt) = partner_type {
        obj.insert("partner_type".to_string(), Value::String(pt.to_string()));
    }
    SourceConfig {
        kind: "sp_api".to_string(),
        config: Value::Object(obj),
    }
}

fn resource_names(partner_type: Option<&str>) -> Vec<String> {
    build_source_connector(&sp_api_config(partner_type), None, Environment::Production)
        .expect("sp_api config builds")
        .resources()
        .into_iter()
        .map(|r| r.name)
        .collect()
}

/// An absent `partner_type` is the seller roster, unchanged.
///
/// The compatibility claim, and the one that matters most: every sp_api
/// pipeline that exists today names no partner, and must keep pulling exactly
/// what it pulled before vendor reports existed.
#[test]
fn an_absent_partner_type_pulls_the_seller_reports() {
    let names = resource_names(None);
    assert!(names.iter().any(|n| n == "shipments"), "{names:?}");
    assert!(
        !names.iter().any(|n| n.starts_with("vendor_")),
        "a seller pipeline must not be offered vendor reports it can only 403 on: {names:?}"
    );
}

/// A vendor credential reaches the vendor reports, and only those.
#[test]
fn a_vendor_partner_type_pulls_the_vendor_reports() {
    let names = resource_names(Some("vendor"));
    for expected in ["vendor_forecasting", "vendor_sales", "vendor_inventory"] {
        assert!(names.iter().any(|n| n == expected), "{expected}: {names:?}");
    }
    assert!(
        !names.iter().any(|n| n == "shipments"),
        "vendor credentials cannot pull a seller report: {names:?}"
    );
}

/// Naming it `seller` explicitly is the same as leaving it out.
///
/// Pinned because the default lives in two crates — the serde `default` here
/// and `SpApiSource`'s builder default in airway — and `build_sp_api` passes
/// the value unconditionally precisely so those two cannot drift apart
/// unnoticed. If they ever do, this is what fails.
#[test]
fn naming_the_seller_account_matches_leaving_it_absent() {
    assert_eq!(resource_names(Some("seller")), resource_names(None));
}
