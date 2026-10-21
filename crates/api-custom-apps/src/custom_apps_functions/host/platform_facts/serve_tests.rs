//! The host op's gates and what it hands the isolate, without an isolate:
//! `serve` (`platform_facts_gate`, the span, the replies). The cache has its
//! own tests (`cache.rs`); the build has `tests.rs` and the integration suite.

use oxy_app_core::internal_orgs::INTERNAL_ORGS_ENV;

use super::*;

const APP_ORG: &str = "3b0d6c2a-8f41-4e7a-9d15-6a2c0e8b4f11";
const STAFF_ORG: &str = "9e4f1a7b-2c35-4d86-a0b9-1c7d3e5f6a22";

fn uuid(s: &str) -> Uuid {
    Uuid::parse_str(s).unwrap()
}

#[test]
fn without_the_capability_the_call_is_refused_even_for_an_internal_org() {
    let org = uuid(APP_ORG);
    let err = platform_facts_gate(false, org, &[org]).unwrap_err();
    assert!(
        err.starts_with("PlatformCapabilityMissing: "),
        "the capability is asked first: {err}"
    );
    assert!(
        err.contains(r#"add "platform": { "facts": true } to its oxy-app.json entry"#),
        "the refusal names the fix: {err}"
    );
}

#[test]
fn with_the_capability_an_org_that_is_not_internal_is_refused() {
    let org = uuid(APP_ORG);
    for (why, internal) in [
        ("no internal orgs", vec![]),
        ("another org internal", vec![uuid(STAFF_ORG)]),
    ] {
        let err = platform_facts_gate(true, org, &internal).unwrap_err();
        assert!(err.starts_with("PlatformFactsNotEnabled: "), "{why}: {err}");
        assert!(
            err.contains("is not an internal org") && err.contains(INTERNAL_ORGS_ENV),
            "{why}: says why, and names the list: {err}"
        );
        assert!(
            !err.contains(STAFF_ORG),
            "{why}: the refusal never names who is on the list: {err}"
        );
    }
}

#[test]
fn with_the_capability_an_internal_org_passes() {
    let org = uuid(APP_ORG);
    assert_eq!(
        platform_facts_gate(true, org, &[uuid(STAFF_ORG), org]),
        Ok(())
    );
}

#[test]
fn weeks_default_and_clamp() {
    let weeks = |opts: serde_json::Value| requested_weeks(&opts);
    assert_eq!(weeks(serde_json::json!({})), Ok(DEFAULT_WEEKS));
    assert_eq!(
        weeks(serde_json::json!({ "weeks": null })),
        Ok(DEFAULT_WEEKS)
    );
    assert_eq!(weeks(serde_json::Value::Null), Ok(DEFAULT_WEEKS));
    assert_eq!(weeks(serde_json::json!({ "weeks": 4 })), Ok(4));
    assert_eq!(weeks(serde_json::json!({ "weeks": 4.9 })), Ok(4));
    assert_eq!(weeks(serde_json::json!({ "weeks": 0 })), Ok(1));
    assert_eq!(weeks(serde_json::json!({ "weeks": -3 })), Ok(1));
    assert_eq!(weeks(serde_json::json!({ "weeks": 99 })), Ok(MAX_WEEKS));
    for bad in [
        serde_json::json!("8"),
        serde_json::json!(true),
        serde_json::json!([4]),
    ] {
        assert_eq!(
            weeks(serde_json::json!({ "weeks": bad })),
            Err(BAD_WEEKS.to_string()),
            "{bad}"
        );
    }
    assert!(
        BAD_WEEKS.starts_with("PlatformFactsBadRequest: "),
        "the refusal carries its label: {BAD_WEEKS}"
    );
}

/// A `weeks` that is not a number is the app's own argument error: the
/// broker files it `bad_request`, which never pages, wherever it runs.
#[test]
fn a_bad_weeks_is_the_apps_own_error_and_never_pages() {
    use crate::custom_apps_functions::host_call_attrs::{
        classify_host_error_for, counts_toward_paging,
    };
    let message = requested_weeks(&serde_json::json!({ "weeks": "8" })).unwrap_err();
    for environment_labels in [false, true] {
        let kind = classify_host_error_for(&message, environment_labels);
        assert_eq!(kind, "bad_request", "{message}");
        assert!(!counts_toward_paging(kind), "{message} must not page");
    }
}

/// A failed build reaches the isolate as the fixed label and nothing else:
/// the database's text stays in the log. Both doors, the cache's included.
#[tokio::test]
async fn a_database_failure_reaches_the_function_as_the_fixed_message() {
    let db = sea_orm::DatabaseConnection::default();
    let internal = [uuid(STAFF_ORG)];
    let loaded = load_reply(&db, &internal, 2, Utc::now()).await;
    assert_eq!(loaded, Err(UNAVAILABLE.to_string()));
    let cached = cached_reply(&db, &internal, 2).await;
    assert_eq!(cached, Err(UNAVAILABLE.to_string()));
}

/// What the broker files each failure under, from the host's own messages:
/// both gates are `not_allowed`, the failed build `host_call_failed`, and all
/// three page, in production and where the environment policy decides.
#[test]
fn the_real_messages_classify_as_paging_failures() {
    use crate::custom_apps_functions::host_call_attrs::{
        classify_host_error_for, counts_toward_paging,
    };
    let org = uuid(APP_ORG);
    let cases = [
        (
            platform_facts_gate(false, org, &[org]).unwrap_err(),
            "not_allowed",
        ),
        (
            platform_facts_gate(true, org, &[]).unwrap_err(),
            "not_allowed",
        ),
        (UNAVAILABLE.to_string(), "host_call_failed"),
    ];
    for (message, expected) in &cases {
        for environment_labels in [false, true] {
            let kind = classify_host_error_for(message, environment_labels);
            assert_eq!(kind, *expected, "{message}");
            assert!(counts_toward_paging(kind), "{message} must page");
        }
    }
}
