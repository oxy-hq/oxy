//! The publish check for an app's `notifications` block.
//!
//! Strict where the runtime read (`parse`) is lenient: a bundle is refused
//! with the first problem, named, before anything is stored. Unknown keys are
//! refused too — a misspelt `function` would otherwise turn a button that runs
//! something into one that only opens. `oxyc publish`, `oxyc validate` and
//! the vite plugin apply the same rules.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::{Map, Value};

use super::model::{
    GENERAL, Level, MAX_ACTION_LABEL_CHARS, MAX_ACTIONS, MAX_DEPTH, MAX_DESCRIPTION_CHARS,
    MAX_LABEL_CHARS, ancestors, depth, valid_segment, well_formed_path,
};
use super::parse::{declared_kinds, function_names};
use crate::notifications::input::{RESERVED_KIND_PREFIX, is_reserved_kind, valid_kind};

/// Why a catalogue is refused: a check id, what is wrong, one fix.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Problem {
    pub check: &'static str,
    pub message: String,
    pub remediation: String,
}

fn problem(check: &'static str, message: String, remediation: String) -> Problem {
    Problem {
        check,
        message,
        remediation,
    }
}

fn shape(message: String, remediation: String) -> Problem {
    problem("notify_catalogue_invalid", message, remediation)
}

/// Check `manifest`'s `notifications` block. `Ok` carries the warnings: a
/// kind no function may send, and a level a function's `critical: true`
/// overrides.
pub fn check(manifest: Option<&Value>) -> Result<Vec<String>, Problem> {
    let Some(block) = manifest.and_then(|m| m.get("notifications")) else {
        return Ok(Vec::new());
    };
    let obj = block.as_object().ok_or_else(|| {
        shape(
            "`notifications` must be an object with `categories` and `kinds`.".into(),
            "Write it as \"notifications\": { \"categories\": {…}, \"kinds\": {…} }.".into(),
        )
    })?;
    only_keys(obj, "notifications", &["categories", "kinds"])?;
    let paths = categories(obj.get("categories"))?;
    let ctx = Context {
        paths,
        functions: function_names(manifest),
        declared: declared_kinds(manifest),
    };
    let mut warnings = Vec::new();
    if let Some(kinds) = obj.get("kinds") {
        let kinds = kinds.as_object().ok_or_else(|| {
            shape(
                "`notifications.kinds` must be an object keyed by kind.".into(),
                "Write it as \"kinds\": { \"<kind>\": { … } }.".into(),
            )
        })?;
        for (kind, def) in kinds {
            check_kind(&ctx, kind, def, &mut warnings)?;
        }
    }
    Ok(warnings)
}

struct Context {
    /// Declared paths and every parent they imply.
    paths: BTreeSet<String>,
    functions: BTreeSet<String>,
    declared: BTreeMap<String, bool>,
}

fn only_keys(obj: &Map<String, Value>, at: &str, allowed: &[&str]) -> Result<(), Problem> {
    match obj.keys().find(|k| !allowed.contains(&k.as_str())) {
        None => Ok(()),
        Some(key) => Err(shape(
            format!(
                "`{at}.{key}` is not a field; {at} takes {}.",
                allowed.join(", ")
            ),
            format!("Remove or rename `{at}.{key}`."),
        )),
    }
}

fn bounded_text(v: Option<&Value>, at: &str, max: usize, required: bool) -> Result<(), Problem> {
    let ok = match v {
        None => !required,
        Some(Value::String(s)) => !s.trim().is_empty() && s.chars().count() <= max,
        Some(_) => false,
    };
    match ok {
        true => Ok(()),
        false => Err(shape(
            format!("`{at}` must be a non-empty string of at most {max} characters."),
            format!("Fix `{at}`."),
        )),
    }
}

fn categories(v: Option<&Value>) -> Result<BTreeSet<String>, Problem> {
    let Some(v) = v else {
        return Ok(BTreeSet::new());
    };
    let obj = v.as_object().ok_or_else(|| {
        shape(
            "`notifications.categories` must be an object keyed by category path.".into(),
            "Write it as \"categories\": { \"operations/food-safety\": { \"label\": \"Food safety\" } }."
                .into(),
        )
    })?;
    let mut paths = BTreeSet::new();
    for (path, def) in obj {
        check_path(path, &format!("notifications.categories.{path}"))?;
        let at = format!("notifications.categories.{path}");
        let def = def.as_object().ok_or_else(|| {
            shape(
                format!("`{at}` must be an object."),
                format!("Write it as \"{path}\": {{ \"label\": \"…\" }}."),
            )
        })?;
        only_keys(def, &at, &["label", "description"])?;
        bounded_text(
            def.get("label"),
            &format!("{at}.label"),
            MAX_LABEL_CHARS,
            false,
        )?;
        bounded_text(
            def.get("description"),
            &format!("{at}.description"),
            MAX_DESCRIPTION_CHARS,
            false,
        )?;
        paths.extend(ancestors(path));
    }
    Ok(paths)
}

fn check_path(path: &str, at: &str) -> Result<(), Problem> {
    if !well_formed_path(path) {
        return Err(problem(
            "notify_category_path_invalid",
            format!(
                "Category {path:?} is not a valid path: each segment is 1–32 lowercase letters, \
                 digits, \"_\" or \"-\", starting with a letter or digit, joined by \"/\"."
            ),
            format!("Rename it in `{at}` (for example \"operations/food-safety\")."),
        ));
    }
    if depth(path) > MAX_DEPTH {
        return Err(problem(
            "notify_category_too_deep",
            format!(
                "Category {path:?} is {} levels deep; at most {MAX_DEPTH} are allowed.",
                depth(path)
            ),
            format!("Shorten `{at}` to {MAX_DEPTH} segments or fewer."),
        ));
    }
    Ok(())
}

fn check_kind(
    ctx: &Context,
    kind: &str,
    def: &Value,
    warnings: &mut Vec<String>,
) -> Result<(), Problem> {
    let at = format!("notifications.kinds.{kind}");
    if is_reserved_kind(kind) {
        return Err(problem(
            "notify_kind_reserved",
            format!(
                "Kind {kind:?} starts with {RESERVED_KIND_PREFIX:?}, which is reserved for Oxygen's \
                 own notices."
            ),
            format!("Rename `{at}` and the ctx.notify calls that send it."),
        ));
    }
    if !valid_kind(kind) {
        return Err(shape(
            format!("Kind {kind:?} must be 1–64 lowercase letters, digits, \"_\", \"-\" or \".\"."),
            format!("Rename `{at}`."),
        ));
    }
    let obj = def.as_object().ok_or_else(|| {
        shape(
            format!("`{at}` must be an object."),
            format!("Write it as \"{kind}\": {{ \"label\": \"…\" }}."),
        )
    })?;
    only_keys(
        obj,
        &at,
        &["category", "label", "description", "level", "actions"],
    )?;
    bounded_text(
        obj.get("label"),
        &format!("{at}.label"),
        MAX_LABEL_CHARS,
        false,
    )?;
    bounded_text(
        obj.get("description"),
        &format!("{at}.description"),
        MAX_DESCRIPTION_CHARS,
        false,
    )?;
    check_category(ctx, kind, obj.get("category"), &at)?;
    let level = check_level(kind, obj.get("level"), &at)?;
    check_actions(ctx, kind, obj.get("actions"), &at)?;
    warn_kind(ctx, kind, level, warnings);
    Ok(())
}

fn check_category(ctx: &Context, kind: &str, v: Option<&Value>, at: &str) -> Result<(), Problem> {
    let Some(v) = v else { return Ok(()) };
    let Some(category) = v.as_str() else {
        return Err(shape(
            format!("`{at}.category` must be a category path."),
            format!("Fix `{at}.category`."),
        ));
    };
    check_path(category, &format!("{at}.category"))?;
    if category == GENERAL || ctx.paths.contains(category) {
        return Ok(());
    }
    Err(problem(
        "notify_category_unknown",
        format!(
            "Kind {kind:?} names the category {category:?}, which notifications.categories does not \
             declare."
        ),
        format!(
            "Declare \"{category}\" under notifications.categories, or change `{at}.category`."
        ),
    ))
}

fn check_level(kind: &str, v: Option<&Value>, at: &str) -> Result<Option<Level>, Problem> {
    let Some(v) = v else { return Ok(None) };
    match v.as_str().and_then(Level::parse) {
        Some(level) => Ok(Some(level)),
        None => Err(problem(
            "notify_level_invalid",
            format!(
                "Kind {kind:?} has level {v}; expected one of passive, active, time_sensitive, \
                 critical."
            ),
            format!("Fix `{at}.level`."),
        )),
    }
}

fn check_actions(ctx: &Context, kind: &str, v: Option<&Value>, at: &str) -> Result<(), Problem> {
    let Some(v) = v else { return Ok(()) };
    let Some(actions) = v.as_array() else {
        return Err(shape(
            format!("`{at}.actions` must be a list."),
            format!("Write it as \"actions\": [{{ \"id\": \"open\", \"label\": \"Open\" }}]."),
        ));
    };
    if actions.len() > MAX_ACTIONS {
        return Err(problem(
            "notify_actions_too_many",
            format!(
                "Kind {kind:?} declares {} actions; at most {MAX_ACTIONS} are allowed.",
                actions.len()
            ),
            format!("Keep {MAX_ACTIONS} or fewer in `{at}.actions`."),
        ));
    }
    let mut ids = BTreeSet::new();
    for (i, action) in actions.iter().enumerate() {
        let id = check_action(ctx, kind, action, &format!("{at}.actions[{i}]"))?;
        if !ids.insert(id.clone()) {
            return Err(problem(
                "notify_action_invalid",
                format!("Kind {kind:?} declares the action {id:?} twice."),
                format!("Give each entry of `{at}.actions` its own id."),
            ));
        }
    }
    Ok(())
}

fn check_action(ctx: &Context, kind: &str, v: &Value, at: &str) -> Result<String, Problem> {
    let invalid = |why: String| {
        problem(
            "notify_action_invalid",
            format!("Action `{at}` of kind {kind:?} {why}."),
            format!("Fix `{at}`."),
        )
    };
    let obj = v.as_object().ok_or_else(|| {
        invalid("must be an object { id, label, function?, destructive? }".into())
    })?;
    only_keys(obj, at, &["id", "label", "function", "destructive"])?;
    let id = obj
        .get("id")
        .and_then(Value::as_str)
        .filter(|i| valid_segment(i))
        .ok_or_else(|| {
            invalid("needs an `id` of 1–32 lowercase letters, digits, \"_\" or \"-\"".into())
        })?;
    if bounded_text(obj.get("label"), "label", MAX_ACTION_LABEL_CHARS, true).is_err() {
        return Err(invalid(format!(
            "needs a `label` of 1–{MAX_ACTION_LABEL_CHARS} characters"
        )));
    }
    if obj.get("destructive").is_some_and(|d| !d.is_boolean()) {
        return Err(invalid(
            "has a `destructive` that is not true or false".into(),
        ));
    }
    match obj.get("function") {
        None => Ok(id.to_string()),
        Some(Value::String(f)) if ctx.functions.contains(f) => Ok(id.to_string()),
        Some(f) => Err(problem(
            "notify_action_function_undeclared",
            format!(
                "Action {id:?} of kind {kind:?} calls the function {f}, which this manifest does \
                 not declare under functions."
            ),
            format!("Declare the function under functions, or remove `{at}.function`."),
        )),
    }
}

fn warn_kind(ctx: &Context, kind: &str, level: Option<Level>, warnings: &mut Vec<String>) {
    match ctx.declared.get(kind) {
        None => warnings.push(format!(
            "notifications.kinds.{kind}: no function declares it under notify.kinds, so nothing \
             can send it yet"
        )),
        Some(true) if level.is_some_and(|l| l != Level::Critical) => warnings.push(format!(
            "notifications.kinds.{kind}: its level is {} but a function marks it critical: true; \
             it is delivered as critical",
            level.map(Level::as_str).unwrap_or_default()
        )),
        Some(_) => {}
    }
}

#[cfg(test)]
#[path = "validate_tests.rs"]
mod tests;
