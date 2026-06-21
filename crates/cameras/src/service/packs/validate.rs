//! PUT-time validation that a pack's prompts render cleanly.
//!
//! The worker uses python-jinja2 with `StrictUndefined`; we mirror
//! that behavior on the backend with minijinja. Rendering against
//! a fixed dummy context catches typos (a missing `}}`, a renamed
//! variable, an undefined filter) before the bad template ever
//! reaches a worker.

use super::body::{PackBody, PpeYoloConfig};
use crate::service::ServiceError;
use minijinja::{Environment, UndefinedBehavior, Value};
use serde_json::json;

/// Validate every role's prompt by rendering it against a fixed
/// dummy context, AND validate the optional `ppe_yolo` block shape.
/// Returns `Ok(())` if everything is well-formed; otherwise returns
/// `ServiceError::InvalidInput` with the first failure.
///
/// The dummy context is built from the pack's own `variables` so a
/// prompt that legitimately references e.g. `variables.brand_voice`
/// validates correctly after the operator adds that key. Each
/// variable is a JSON value mirroring whatever the pack persists.
///
/// Name kept as `check_pack_renders` for callsite stability — the
/// function does slightly more than its name suggests now. Resist
/// the rename until there's a structural reason; the call sites are
/// stable and grepping `check_pack_renders` lands you here cleanly.
pub fn check_pack_renders(body: &PackBody) -> Result<(), ServiceError> {
    let env = strict_env();
    let context = dummy_context(body);
    for role in &body.roles {
        // Compile + render each prompt. `add_template_owned` clones
        // the source — fine for validation; the worker keeps a
        // long-lived Environment, we don't.
        let tmpl = env
            .template_from_str(&role.prompt)
            .map_err(|e| invalid(&role.role_key, e.to_string()))?;
        tmpl.render(&context)
            .map_err(|e| invalid(&role.role_key, e.to_string()))?;
    }
    if let Some(cfg) = &body.ppe_yolo {
        check_ppe_yolo(cfg)?;
    }
    Ok(())
}

/// Validate a [`PpeYoloConfig`]. Rejects malformed bodies at the
/// PUT boundary so the worker never sees a config it can't act on.
///
/// What we check:
///   - `model_uri` is non-empty and looks like one of the accepted
///     shapes (https URL, hf:// id, or absolute filesystem path).
///   - `model_sha256` (when set) is 64 lowercase hex chars.
///   - `confidence_threshold` is in (0.0, 1.0].
///   - `classes` (when set) is non-empty, contains no duplicates, and
///     no empty strings.
///
/// `suppress_for_roles` is intentionally not validated — the role
/// taxonomy can evolve faster than this validator, and an unknown
/// role name in the suppress list is a harmless no-op on the worker.
pub fn check_ppe_yolo(cfg: &PpeYoloConfig) -> Result<(), ServiceError> {
    let bad = |msg: String| ServiceError::InvalidInput(format!("ppe_yolo: {msg}"));

    let uri = cfg.model_uri.trim();
    if uri.is_empty() {
        return Err(bad("model_uri is required".into()));
    }
    let looks_like_uri = uri.starts_with("https://")
        || uri.starts_with("http://")
        || uri.starts_with("hf://")
        || uri.starts_with('/');
    if !looks_like_uri {
        return Err(bad(format!(
            "model_uri must start with https://, hf://, or /, got {uri:?}"
        )));
    }

    if let Some(hash) = &cfg.model_sha256
        && (hash.len() != 64 || !hash.chars().all(|c| c.is_ascii_hexdigit()))
    {
        return Err(bad(format!(
            "model_sha256 must be 64 hex chars, got {} chars",
            hash.len()
        )));
    }

    if !(cfg.confidence_threshold > 0.0 && cfg.confidence_threshold <= 1.0) {
        return Err(bad(format!(
            "confidence_threshold must be in (0, 1], got {}",
            cfg.confidence_threshold
        )));
    }

    if let Some(classes) = &cfg.classes {
        if classes.is_empty() {
            return Err(bad("classes (when set) must be non-empty".into()));
        }
        let mut seen = std::collections::HashSet::new();
        for c in classes {
            if c.trim().is_empty() {
                return Err(bad("classes contains an empty string".into()));
            }
            if !seen.insert(c) {
                return Err(bad(format!("classes contains duplicate {c:?}")));
            }
        }
    }

    Ok(())
}

fn strict_env<'a>() -> Environment<'a> {
    let mut env = Environment::new();
    // Match python-jinja2's StrictUndefined: an undefined name in
    // the template body raises, instead of silently rendering to
    // empty string.
    env.set_undefined_behavior(UndefinedBehavior::Strict);
    env
}

/// Build a dummy context that exercises every prompt's referenced
/// variables. `track_id` and `dwell_seconds` are always provided.
/// Pack-wide `variables` flow through as-is so a prompt that
/// references `{{ variables.required_attire | join(", ") }}` sees
/// a real list.
fn dummy_context(body: &PackBody) -> Value {
    let vars: serde_json::Map<String, serde_json::Value> = body
        .variables
        .iter()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    Value::from_serialize(json!({
        "track_id": 0,
        "dwell_seconds": 0.0_f64,
        "variables": vars,
    }))
}

fn invalid(role_key: &str, msg: String) -> ServiceError {
    ServiceError::InvalidInput(format!("role `{role_key}` prompt failed to render: {msg}"))
}

#[cfg(test)]
mod tests {
    use super::super::body::{PackBody, RolePrompt};
    use super::*;
    use std::collections::BTreeMap;

    fn body_with_prompt(prompt: &str) -> PackBody {
        let mut vars = BTreeMap::new();
        vars.insert("required_attire".into(), json!(["hat"]));
        PackBody {
            schema_version: 1,
            output_schema: json!({}),
            variables: vars,
            roles: vec![RolePrompt {
                role_key: "test".into(),
                label: "Test".into(),
                hint: "test".into(),
                prompt: prompt.into(),
            }],
            ppe_yolo: None,
        }
    }

    #[test]
    fn valid_template_renders() {
        let body = body_with_prompt(
            "Track {{ track_id }} dwell {{ dwell_seconds }} attire {{ variables.required_attire | join(\",\") }}",
        );
        check_pack_renders(&body).expect("valid template should render");
    }

    #[test]
    fn unknown_variable_fails() {
        let body = body_with_prompt("Bad {{ variables.nonexistent }}");
        let err = check_pack_renders(&body).unwrap_err();
        match err {
            ServiceError::InvalidInput(msg) => assert!(msg.contains("test"), "got: {msg}"),
            _ => panic!("expected InvalidInput, got {err:?}"),
        }
    }

    #[test]
    fn syntax_error_fails() {
        let body = body_with_prompt("Bad {{ unclosed");
        let err = check_pack_renders(&body).unwrap_err();
        assert!(matches!(err, ServiceError::InvalidInput(_)));
    }

    #[test]
    fn restaurant_starter_renders() {
        let body = crate::service::packs::starter::body_for("restaurant").unwrap();
        check_pack_renders(&body).expect("shipped starter must render");
    }

    #[test]
    fn all_shipped_starters_render() {
        for s in crate::service::packs::starter::list() {
            let body = crate::service::packs::starter::body_for(s.key)
                .unwrap_or_else(|| panic!("starter `{}` listed but body_for is None", s.key));
            check_pack_renders(&body)
                .unwrap_or_else(|e| panic!("starter `{}` failed render: {e:?}", s.key));
        }
    }

    // ── ppe_yolo validation ─────────────────────────────────────────

    use super::super::body::PpeYoloConfig;

    fn valid_cfg() -> PpeYoloConfig {
        PpeYoloConfig {
            model_uri: "/app/models/food_ppe-trained.pt".into(),
            model_sha256: None,
            confidence_threshold: 0.25,
            classes: Some(vec!["person".into(), "hat".into()]),
            suppress_for_roles: vec!["dining".into()],
        }
    }

    #[test]
    fn ppe_yolo_baseline_passes() {
        check_ppe_yolo(&valid_cfg()).expect("happy path");
    }

    #[test]
    fn ppe_yolo_rejects_empty_uri() {
        let mut cfg = valid_cfg();
        cfg.model_uri = "   ".into();
        let err = check_ppe_yolo(&cfg).unwrap_err();
        match err {
            ServiceError::InvalidInput(m) => assert!(m.contains("model_uri"), "got: {m}"),
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn ppe_yolo_rejects_unknown_scheme() {
        let mut cfg = valid_cfg();
        cfg.model_uri = "ftp://example.com/model.pt".into();
        let err = check_ppe_yolo(&cfg).unwrap_err();
        assert!(
            matches!(err, ServiceError::InvalidInput(m) if m.contains("must start with")),
            "should reject unknown URI scheme"
        );
    }

    #[test]
    fn ppe_yolo_accepts_known_schemes() {
        for uri in [
            "https://example.com/m.pt",
            "http://example.com/m.pt",
            "hf://owner/repo",
            "/app/models/m.pt",
        ] {
            let mut cfg = valid_cfg();
            cfg.model_uri = uri.into();
            check_ppe_yolo(&cfg).unwrap_or_else(|e| panic!("{uri} should pass: {e:?}"));
        }
    }

    #[test]
    fn ppe_yolo_rejects_short_sha256() {
        let mut cfg = valid_cfg();
        cfg.model_sha256 = Some("deadbeef".into());
        let err = check_ppe_yolo(&cfg).unwrap_err();
        assert!(matches!(err, ServiceError::InvalidInput(m) if m.contains("64 hex")));
    }

    #[test]
    fn ppe_yolo_accepts_valid_sha256() {
        let mut cfg = valid_cfg();
        cfg.model_sha256 = Some("a".repeat(64));
        check_ppe_yolo(&cfg).expect("64 lowercase hex chars should pass");
    }

    #[test]
    fn ppe_yolo_rejects_threshold_out_of_range() {
        for bad in [-0.1f32, 0.0, 1.1, f32::INFINITY, f32::NAN] {
            let mut cfg = valid_cfg();
            cfg.confidence_threshold = bad;
            let err = check_ppe_yolo(&cfg).unwrap_err();
            assert!(
                matches!(err, ServiceError::InvalidInput(m) if m.contains("confidence_threshold")),
                "should reject threshold={bad}"
            );
        }
    }

    #[test]
    fn ppe_yolo_rejects_duplicate_classes() {
        let mut cfg = valid_cfg();
        cfg.classes = Some(vec!["hat".into(), "hat".into()]);
        let err = check_ppe_yolo(&cfg).unwrap_err();
        assert!(matches!(err, ServiceError::InvalidInput(m) if m.contains("duplicate")));
    }

    #[test]
    fn ppe_yolo_rejects_empty_class_list() {
        let mut cfg = valid_cfg();
        cfg.classes = Some(vec![]);
        let err = check_ppe_yolo(&cfg).unwrap_err();
        assert!(matches!(err, ServiceError::InvalidInput(m) if m.contains("non-empty")));
    }

    #[test]
    fn restaurant_starter_passes_full_check() {
        // The shipped Restaurant starter now includes a ppe_yolo
        // block — `check_pack_renders` exercises both render and
        // ppe_yolo validation, so this test guards against either
        // half regressing.
        let body = crate::service::packs::starter::body_for("restaurant").unwrap();
        assert!(
            body.ppe_yolo.is_some(),
            "Restaurant starter must carry ppe_yolo after Phase A"
        );
        check_pack_renders(&body).expect("full validation must pass");
    }
}
