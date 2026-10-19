use serde_json::json;

use super::*;

const APP: Uuid = Uuid::from_u128(0x5c4ed);

fn specs() -> Vec<(String, serde_json::Value)> {
    vec![
        (
            "refresh".into(),
            json!({ "schedule": "0 * * * *", "timezone": "Europe/Paris" }),
        ),
        ("route-only".into(), json!({ "route": true })),
        (
            "nightly".into(),
            json!({ "schedule": "0 3 * * *", "retries": { "maxAttempts": 3 } }),
        ),
    ]
}

/// Production keeps the identity it always had: `<app_id>/<function>`,
/// named `fn:<app_id>/<function>`, UTC unless the manifest says otherwise.
#[test]
fn production_rows_keep_the_two_segment_identity() {
    let rows = wanted(APP, Scope::Production, &specs());
    let refs: Vec<(String, String, String)> = rows
        .iter()
        .map(|r| (r.target_ref.clone(), r.name.clone(), r.timezone.clone()))
        .collect();
    let row = |function: &str, timezone: &str| {
        let target_ref = format!("{APP}/{function}");
        (
            target_ref.clone(),
            format!("fn:{target_ref}"),
            timezone.to_string(),
        )
    };
    assert_eq!(
        refs,
        vec![row("refresh", "Europe/Paris"), row("nightly", "UTC")]
    );
    assert!(
        rows.iter()
            .all(|r| r.target_kind == "function" && r.enabled)
    );
}

/// An environment's rows name it in the middle segment, and only functions
/// that declare a schedule get one.
#[test]
fn an_environments_rows_name_it() {
    let rows = wanted(APP, Scope::Environment("dev-a1"), &specs());
    let refs: Vec<String> = rows.iter().map(|r| r.target_ref.clone()).collect();
    assert_eq!(
        refs,
        vec![
            format!("{APP}/dev-a1/refresh"),
            format!("{APP}/dev-a1/nightly")
        ]
    );
    assert_eq!(rows[0].name, format!("fn:{APP}/dev-a1/refresh"));
}

/// The rule that keeps production's promote off every environment's rows,
/// and each environment off the others'.
#[test]
fn each_scope_owns_its_own_rows_and_no_others() {
    let app = APP.to_string();
    let other = Uuid::from_u128(9).to_string();
    let production = format!("{app}/refresh");
    let staging = format!("{app}/staging/refresh");
    let sandbox = format!("{app}/dev-a1/refresh");
    let elsewhere = format!("{other}/refresh");
    let owns = |scope: Scope<'_>, raw: &str| {
        let target = FunctionScheduleRef::parse(raw).expect("a ref");
        scope.owns(&app, &target)
    };

    assert!(owns(Scope::Production, &production));
    assert!(
        !owns(Scope::Production, &staging),
        "a promote leaves staging's row"
    );
    assert!(
        !owns(Scope::Production, &sandbox),
        "a promote leaves a sandbox's row"
    );
    assert!(!owns(Scope::Production, &elsewhere));

    assert!(owns(Scope::Environment("dev-a1"), &sandbox));
    assert!(!owns(Scope::Environment("dev-a1"), &staging));
    assert!(!owns(Scope::Environment("dev-a1"), &production));
    assert!(owns(Scope::Environment("staging"), &staging));
}

/// The retry policy rides the row, as it did on production's.
#[test]
fn the_retry_policy_rides_the_row() {
    let rows = wanted(APP, Scope::Environment("staging"), &specs());
    let nightly = rows
        .iter()
        .find(|r| r.target_ref.ends_with("/nightly"))
        .expect("nightly");
    let refresh = rows
        .iter()
        .find(|r| r.target_ref.ends_with("/refresh"))
        .expect("refresh");
    assert_eq!(
        nightly
            .variables
            .as_ref()
            .map(|v| v.get("task_policy").is_some()),
        Some(true)
    );
    assert_eq!(refresh.variables, None, "no retry declared, no policy");
}
