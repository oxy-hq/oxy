//! A build's catalogue, read leniently.
//!
//! Publish refuses a malformed `notifications` block (`validate`), so a build
//! that went live holds a well-formed one — or none, if it predates them. This
//! reads whatever is well formed and skips the rest rather than failing a
//! person's inbox over an app's manifest.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::Value;

use super::model::{
    Action, Catalogue, CategoryDef, KindDef, Level, MAX_ACTIONS, valid_path, valid_segment,
};
use crate::server::api::notifications::input::{is_reserved_kind, valid_kind};

/// The app-level `notifications` block of `manifest`.
pub fn catalogue(manifest: Option<&Value>) -> Catalogue {
    let Some(block) = manifest
        .and_then(|m| m.get("notifications"))
        .and_then(Value::as_object)
    else {
        return Catalogue::default();
    };
    Catalogue {
        categories: categories(block.get("categories")),
        kinds: kinds(block.get("kinds")),
    }
}

/// Every kind a function in `manifest` may send (`functions.*.notify.kinds`),
/// each mapped to whether any of them marks it `critical: true`.
pub fn declared_kinds(manifest: Option<&Value>) -> BTreeMap<String, bool> {
    let mut out: BTreeMap<String, bool> = BTreeMap::new();
    let Some(functions) = manifest
        .and_then(|m| m.get("functions"))
        .and_then(Value::as_object)
    else {
        return out;
    };
    for spec in functions.values() {
        let Some(kinds) = spec.pointer("/notify/kinds").and_then(Value::as_object) else {
            continue;
        };
        for (kind, k) in kinds {
            let critical = k.get("critical").and_then(Value::as_bool).unwrap_or(false);
            *out.entry(kind.clone()).or_default() |= critical;
        }
    }
    out
}

/// The function names `manifest` declares.
pub fn function_names(manifest: Option<&Value>) -> BTreeSet<String> {
    manifest
        .and_then(|m| m.get("functions"))
        .and_then(Value::as_object)
        .map(|f| f.keys().cloned().collect())
        .unwrap_or_default()
}

fn text(v: Option<&Value>) -> Option<String> {
    v.and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

fn categories(v: Option<&Value>) -> BTreeMap<String, CategoryDef> {
    let Some(obj) = v.and_then(Value::as_object) else {
        return BTreeMap::new();
    };
    obj.iter()
        .filter(|(path, _)| valid_path(path))
        .map(|(path, def)| {
            (
                path.clone(),
                CategoryDef {
                    label: text(def.get("label")),
                    description: text(def.get("description")),
                },
            )
        })
        .collect()
}

fn kinds(v: Option<&Value>) -> BTreeMap<String, KindDef> {
    let Some(obj) = v.and_then(Value::as_object) else {
        return BTreeMap::new();
    };
    obj.iter()
        .filter(|(kind, _)| valid_kind(kind) && !is_reserved_kind(kind))
        .map(|(kind, def)| (kind.clone(), kind_def(def)))
        .collect()
}

fn kind_def(def: &Value) -> KindDef {
    KindDef {
        category: text(def.get("category")).filter(|c| valid_path(c)),
        label: text(def.get("label")),
        description: text(def.get("description")),
        level: def
            .get("level")
            .and_then(Value::as_str)
            .and_then(Level::parse),
        actions: def
            .get("actions")
            .and_then(Value::as_array)
            .map(|a| a.iter().filter_map(action).take(MAX_ACTIONS).collect())
            .unwrap_or_default(),
    }
}

fn action(v: &Value) -> Option<Action> {
    let id = v
        .get("id")
        .and_then(Value::as_str)
        .filter(|i| valid_segment(i))?;
    Some(Action {
        id: id.to_string(),
        label: text(v.get("label"))?,
        function: text(v.get("function")),
        destructive: v
            .get("destructive")
            .and_then(Value::as_bool)
            .unwrap_or(false),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn manifest() -> Value {
        json!({
            "functions": {
                "lists": { "notify": { "kinds": { "list_overdue": {}, "temp_alarm": {} } } },
                "ack-alarm": { "notify": { "kinds": { "temp_alarm": { "critical": true } } } }
            },
            "notifications": {
                "categories": {
                    "operations": { "label": "Store operations" },
                    "operations/food-safety": { "label": "Food safety", "description": "Temps" },
                    "Bad Path": {}
                },
                "kinds": {
                    "list_overdue": { "category": "operations", "level": "time_sensitive",
                                      "actions": [{ "id": "open", "label": "Open list" }] },
                    "temp_alarm": { "category": "operations/food-safety", "level": "critical",
                                    "actions": [{ "id": "ack", "label": "Acknowledge", "function": "ack-alarm" },
                                                { "id": "Bad", "label": "x" }] },
                    "oxy.anomaly": {}
                }
            }
        })
    }

    #[test]
    fn the_well_formed_parts_are_read_and_the_rest_skipped() {
        let c = catalogue(Some(&manifest()));
        assert_eq!(
            c.categories.keys().collect::<Vec<_>>(),
            vec!["operations", "operations/food-safety"]
        );
        assert_eq!(c.kinds.len(), 2, "the reserved kind is skipped");
        let alarm = &c.kinds["temp_alarm"];
        assert_eq!(alarm.level, Some(Level::Critical));
        assert_eq!(alarm.actions.len(), 1, "the malformed action is skipped");
        assert_eq!(alarm.actions[0].function.as_deref(), Some("ack-alarm"));
        assert!(catalogue(Some(&json!({}))).is_empty());
        assert!(catalogue(None).is_empty());
    }

    #[test]
    fn a_kind_is_critical_when_any_function_says_so() {
        let declared = declared_kinds(Some(&manifest()));
        assert_eq!(declared.get("temp_alarm"), Some(&true));
        assert_eq!(declared.get("list_overdue"), Some(&false));
        assert_eq!(
            function_names(Some(&manifest()))
                .into_iter()
                .collect::<Vec<_>>(),
            vec!["ack-alarm", "lists"]
        );
    }
}
