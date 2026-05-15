//! Shared Jinja rendering + workspace path containment helpers.
//!
//! Used by [`step_executor`](crate::step_executor) (for `sql_file` paths
//! and inline SQL) and [`export`](crate::export) (for `export.path`).
//! Lifted into one module so a future divergence — e.g. one renderer
//! sandboxing templates differently — doesn't sneak in unnoticed.
//!
//! ## Path containment
//!
//! Workflow authors are trusted, but the data flowing through Jinja
//! isn't. A workflow that does `sql_file: "data/{{ loop.value }}.sql"`
//! over a list whose values come from a SQL query will substitute
//! whatever the query returned — and if any row contains `../etc/passwd`
//! we'd happily read it (or, in the export case, write to it). The
//! same path-traversal concern that closed
//! [`OxyProjectContext::resolve_workflow_yaml`][rwy] applies here.
//!
//! Containment is enforced syntactically (no `canonicalize`): we
//! reject empty paths, absolute paths, and any path containing a `..`
//! component. The export writer also targets paths that don't exist
//! yet, so the canonicalisation-based check `validate_path_within_project`
//! uses for read-only paths isn't applicable here.
//!
//! [rwy]: agentic_wiring::OxyProjectContext::resolve_workflow_yaml

use std::path::{Path, PathBuf};

use serde_json::Value;

/// Build a chainable-undefined minijinja [`Environment`] preloaded with
/// the workflow filter set.
///
/// Used for step bodies, formatters, and any user-authored template
/// where a typo'd path should expand to empty rather than fail — this
/// matches the legacy `oxy_core::execute::renderer::setup_jinja_environment`
/// behaviour so templates ported from the previous workflow engine
/// keep rendering. The filters registered are the same as
/// [`workflow_env_strict`]:
///
/// - `now(utc?, fmt?)` — current datetime, RFC3339 by default.
/// - `tojson` — serialize to a compact JSON string.
/// - `sqlquote` — escape as a SQL string literal (`O'Brien` →
///   `'O''Brien'`). Templates should NOT add their own surrounding
///   quotes when using this filter.
pub(crate) fn workflow_env() -> minijinja::Environment<'static> {
    let mut env = minijinja::Environment::new();
    env.set_undefined_behavior(minijinja::UndefinedBehavior::Chainable);
    add_global_helpers(&mut env);
    env
}

/// Build a strict-undefined minijinja [`Environment`] with the same
/// filter set as [`workflow_env`].
///
/// Used for runtime expressions where silent typos are unacceptable —
/// e.g. `loop_sequential.values: "{{ intervals.list }}"`, where an
/// empty resolution silently turns the loop into a no-op. Errors on
/// undefined access; callers translate the error into a clear "did
/// not resolve" / "available keys" message.
pub(crate) fn workflow_env_strict() -> minijinja::Environment<'static> {
    let mut env = minijinja::Environment::new();
    add_global_helpers(&mut env);
    env
}

fn add_global_helpers(env: &mut minijinja::Environment<'static>) {
    use chrono::{DateTime, Local, Utc};

    env.add_function(
        "now",
        |kwargs: minijinja::value::Kwargs| -> Result<String, minijinja::Error> {
            let utc = kwargs.get::<Option<bool>>("utc")?.unwrap_or(false);
            let fmt = kwargs.get::<Option<String>>("fmt")?;
            let out = if utc {
                let now: DateTime<Utc> = Utc::now();
                match fmt {
                    Some(f) => now.format(&f).to_string(),
                    None => now.to_rfc3339(),
                }
            } else {
                let now: DateTime<Local> = Local::now();
                match fmt {
                    Some(f) => now.format(&f).to_string(),
                    None => now.to_rfc3339(),
                }
            };
            Ok(out)
        },
    );

    env.add_filter(
        "tojson",
        |value: minijinja::Value| -> Result<String, minijinja::Error> {
            serde_json::to_string(&value).map_err(|e| {
                minijinja::Error::new(
                    minijinja::ErrorKind::InvalidOperation,
                    format!("Failed to convert to JSON: {e}"),
                )
            })
        },
    );

    // SQL-string-literal escape. Wraps in single quotes; doubles any
    // embedded single quotes per ANSI SQL.
    //   {{ controls.store | sqlquote }}  →  'O''Brien'
    // Templates must NOT add surrounding quotes themselves.
    env.add_filter(
        "sqlquote",
        |value: minijinja::Value| -> Result<String, minijinja::Error> {
            let escaped = value.to_string().replace('\'', "''");
            Ok(format!("'{escaped}'"))
        },
    );
}

/// Render a Jinja template string against the given context.
///
/// See [`workflow_env`] for the filters / functions available to
/// templates. Returns `Err` with a parse or render error on failure;
/// missing keys are forgiven (chainable undefined).
pub(crate) fn render_jinja_string(template: &str, context: &Value) -> Result<String, String> {
    let mut env = workflow_env();
    let tmpl = env
        .template_from_str(template)
        .map_err(|e| format!("template parse error: {e}"))?;
    let ctx = crate::step_orchestrator::build_minijinja_context(context);
    tmpl.render(&ctx).map_err(|e| format!("render error: {e}"))
}

/// Resolve a workspace-relative path, rejecting traversal attempts.
///
/// Returns `workspace.join(relative)` when:
/// - `relative` is non-empty,
/// - `relative` is not absolute,
/// - `relative` contains no `..` components.
///
/// Otherwise returns `Err` with a user-readable message.
///
/// Containment is purely syntactic — see the module docs for the
/// rationale and threat model.
pub(crate) fn validate_workspace_relative_path(
    workspace: &Path,
    relative: &str,
) -> Result<PathBuf, String> {
    if relative.is_empty() {
        return Err("path is empty".into());
    }
    let candidate = Path::new(relative);
    if candidate.is_absolute() {
        return Err(format!(
            "path {relative:?} must be relative to the workspace"
        ));
    }
    if candidate
        .components()
        .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        return Err(format!("path {relative:?} must not contain `..` segments"));
    }
    Ok(workspace.join(candidate))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn jinja_renders_simple_substitution() {
        let ctx = json!({"x": 42, "name": "world"});
        assert_eq!(render_jinja_string("v={{ x }}", &ctx).unwrap(), "v=42");
        assert_eq!(
            render_jinja_string("hello {{ name }}", &ctx).unwrap(),
            "hello world"
        );
    }

    #[test]
    fn jinja_renders_missing_keys_as_empty() {
        // Chainable: `foo.bar` when `foo` is undefined → empty.
        let ctx = json!({});
        assert_eq!(render_jinja_string("x={{ foo.bar }}", &ctx).unwrap(), "x=");
    }

    #[test]
    fn validate_accepts_simple_relative() {
        let p = validate_workspace_relative_path(Path::new("/ws"), "data/x.sql").unwrap();
        assert_eq!(p, Path::new("/ws/data/x.sql"));
    }

    #[test]
    fn validate_rejects_empty() {
        let err = validate_workspace_relative_path(Path::new("/ws"), "").unwrap_err();
        assert!(err.contains("empty"));
    }

    #[test]
    fn validate_rejects_absolute() {
        for p in ["/etc/passwd", "/tmp/escape.csv"] {
            let err = validate_workspace_relative_path(Path::new("/ws"), p).unwrap_err();
            assert!(err.contains("relative"), "for {p:?}: {err}");
        }
    }

    #[test]
    fn validate_rejects_parent_dir_traversal() {
        for p in ["../etc/passwd", "data/../../etc", "..", "a/../b", "./../x"] {
            let err = validate_workspace_relative_path(Path::new("/ws"), p);
            assert!(err.is_err(), "should reject {p:?}, got {err:?}");
        }
    }

    /// `sqlquote` wraps in single quotes and doubles embedded ones —
    /// matches the legacy `oxy_core::execute::renderer` semantics, so
    /// templates ported from the previous engine render the same SQL.
    #[test]
    fn sqlquote_filter_escapes_single_quotes() {
        let ctx = json!({"name": "O'Brien", "plain": "hello"});
        assert_eq!(
            render_jinja_string("{{ name | sqlquote }}", &ctx).unwrap(),
            "'O''Brien'"
        );
        assert_eq!(
            render_jinja_string("{{ plain | sqlquote }}", &ctx).unwrap(),
            "'hello'"
        );
    }

    /// `tojson` filter round-trips a value to its JSON encoding.
    #[test]
    fn tojson_filter_serializes() {
        let ctx = json!({"x": [1, 2, 3], "y": "abc"});
        assert_eq!(
            render_jinja_string("{{ x | tojson }}", &ctx).unwrap(),
            "[1,2,3]"
        );
        assert_eq!(
            render_jinja_string("{{ y | tojson }}", &ctx).unwrap(),
            "\"abc\""
        );
    }

    /// `now()` is callable. Don't pin a value (it changes every call) —
    /// just check the format-string variant produces the expected width.
    #[test]
    fn now_function_with_format_renders_fixed_width() {
        let ctx = json!({});
        let out = render_jinja_string("{{ now(fmt='%Y-%m-%d') }}", &ctx).unwrap();
        assert_eq!(out.len(), 10, "expected YYYY-MM-DD, got {out:?}");
        assert_eq!(out.chars().filter(|c| *c == '-').count(), 2);
    }

    #[test]
    fn validate_allows_current_dir_prefix() {
        // `./data/x.sql` has only `CurDir` + `Normal` components, no
        // `ParentDir` — allowed.
        let p = validate_workspace_relative_path(Path::new("/ws"), "./data/x.sql").unwrap();
        assert!(p.starts_with("/ws"));
    }
}
