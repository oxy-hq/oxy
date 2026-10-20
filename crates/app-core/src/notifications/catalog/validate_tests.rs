use serde_json::{Value, json};

use super::*;

/// The brief's worked example: valid, and silent.
fn manifest() -> Value {
    json!({
        "functions": {
            "lists": { "notify": { "kinds": { "list_overdue": {}, "temp_alarm": {} } } },
            "ack-alarm": { "route": true }
        },
        "notifications": {
            "categories": {
                "operations": { "label": "Store operations" },
                "operations/food-safety": { "label": "Food safety", "description": "Temperature logs and checks" }
            },
            "kinds": {
                "list_overdue": { "category": "operations", "label": "Overdue lists", "level": "time_sensitive",
                                  "actions": [{ "id": "open", "label": "Open list" }] },
                "temp_alarm": { "category": "operations/food-safety", "label": "Temperature alarm", "level": "critical",
                                "actions": [{ "id": "ack", "label": "Acknowledge", "function": "ack-alarm" }] }
            }
        }
    })
}

/// `manifest()` with `key` set to `value` in the object at `parent`.
fn with(parent: &str, key: &str, value: Value) -> Value {
    let mut m = manifest();
    m.pointer_mut(parent)
        .and_then(Value::as_object_mut)
        .unwrap()
        .insert(key.to_string(), value);
    m
}

fn refused(m: &Value) -> Problem {
    check(Some(m)).expect_err("refused")
}

#[test]
fn the_worked_example_passes_without_warnings() {
    assert_eq!(check(Some(&manifest())), Ok(vec![]));
    assert_eq!(check(Some(&json!({ "functions": {} }))), Ok(vec![]));
    assert_eq!(check(None), Ok(vec![]));
}

#[test]
fn a_malformed_path_is_refused() {
    let m = with("/notifications/categories", "Store Ops", json!({}));
    assert_eq!(refused(&m).check, "notify_category_path_invalid");
    let m = with(
        "/notifications/kinds/list_overdue",
        "category",
        json!("ops//x"),
    );
    assert_eq!(refused(&m).check, "notify_category_path_invalid");
}

#[test]
fn a_path_deeper_than_three_is_refused() {
    let m = with("/notifications/categories", "a/b/c/d", json!({}));
    let p = refused(&m);
    assert_eq!(p.check, "notify_category_too_deep");
    assert!(p.message.contains("4 levels deep"), "{}", p.message);
}

#[test]
fn a_kinds_unknown_category_is_refused_and_implied_parents_and_general_are_not() {
    let m = with(
        "/notifications/kinds/list_overdue",
        "category",
        json!("finance"),
    );
    let p = refused(&m);
    assert_eq!(p.check, "notify_category_unknown");
    assert!(p.message.contains("\"finance\""), "{}", p.message);
    // `a` is implied by declaring `a/b`; `general` always exists.
    let mut m = with("/notifications/categories", "a/b", json!({}));
    m["notifications"]["kinds"]["list_overdue"]["category"] = json!("a");
    m["notifications"]["kinds"]["temp_alarm"]["category"] = json!("general");
    assert!(check(Some(&m)).is_ok());
}

#[test]
fn a_bad_level_is_refused() {
    let m = with(
        "/notifications/kinds/list_overdue",
        "level",
        json!("urgent"),
    );
    assert_eq!(refused(&m).check, "notify_level_invalid");
}

#[test]
fn more_than_three_actions_are_refused() {
    let four: Vec<Value> = (0..4)
        .map(|i| json!({ "id": format!("a{i}"), "label": "Go" }))
        .collect();
    let m = with("/notifications/kinds/list_overdue", "actions", json!(four));
    assert_eq!(refused(&m).check, "notify_actions_too_many");
}

#[test]
fn an_action_naming_an_undeclared_function_is_refused() {
    let m = with(
        "/notifications/kinds/temp_alarm",
        "actions",
        json!([{ "id": "ack", "label": "Acknowledge", "function": "ack-it" }]),
    );
    let p = refused(&m);
    assert_eq!(p.check, "notify_action_function_undeclared");
    assert!(p.message.contains("ack-it"), "{}", p.message);
}

#[test]
fn a_platform_kind_is_refused() {
    let m = with("/notifications/kinds", "oxy.anomaly", json!({}));
    assert_eq!(refused(&m).check, "notify_kind_reserved");
}

#[test]
fn malformed_actions_and_misspelt_keys_are_refused() {
    for actions in [
        json!([{ "id": "Ack", "label": "Acknowledge" }]),
        json!([{ "id": "ack", "label": "" }]),
        json!([{ "id": "ack", "label": "x".repeat(25) }]),
        json!([{ "id": "ack", "label": "A" }, { "id": "ack", "label": "B" }]),
        json!([{ "id": "ack", "label": "A", "destructive": "yes" }]),
    ] {
        let m = with(
            "/notifications/kinds/temp_alarm",
            "actions",
            actions.clone(),
        );
        assert_eq!(refused(&m).check, "notify_action_invalid", "{actions}");
    }
    // A typo'd `function` would quietly make the button only open.
    let m = with(
        "/notifications/kinds/temp_alarm",
        "actions",
        json!([{ "id": "ack", "label": "Ack", "fucntion": "ack-alarm" }]),
    );
    let p = refused(&m);
    assert_eq!(p.check, "notify_catalogue_invalid");
    assert!(p.message.contains("fucntion"), "{}", p.message);
    let m = with("/notifications/kinds/list_overdue", "lable", json!("x"));
    assert_eq!(refused(&m).check, "notify_catalogue_invalid");
    let m = with("/notifications/kinds", "List Overdue", json!({}));
    assert_eq!(refused(&m).check, "notify_catalogue_invalid");
}

#[test]
fn a_kind_no_function_sends_and_an_overridden_level_are_warnings() {
    let m = with(
        "/notifications/kinds",
        "stock_low",
        json!({ "level": "passive" }),
    );
    let warnings = check(Some(&m)).expect("a warning, not a refusal");
    assert_eq!(warnings.len(), 1);
    assert!(warnings[0].contains("stock_low"), "{warnings:?}");

    let mut m = manifest();
    m["functions"]["lists"]["notify"]["kinds"]["list_overdue"] = json!({ "critical": true });
    let warnings = check(Some(&m)).expect("a warning");
    assert!(
        warnings[0].contains("delivered as critical"),
        "{warnings:?}"
    );
}

#[test]
fn the_oltp_bookings_example_declares_a_valid_catalogue() {
    let manifest: Value = serde_json::from_str(include_str!(
        "../../../../../customer-apps/examples/oltp-bookings/oxy-app.json"
    ))
    .expect("the example manifest is JSON");
    assert!(
        manifest.get("notifications").is_some(),
        "the worked example"
    );
    assert_eq!(check(Some(&manifest)), Ok(vec![]));
}
