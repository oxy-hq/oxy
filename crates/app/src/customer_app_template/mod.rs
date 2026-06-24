//! Bundled templates for customer apps, plus the placeholder renderer
//! both the CLI scaffold and the admin-triggered GitHub scaffold use.
//!
//! Sharing the renderer guarantees `oxy apps init` (writes to disk)
//! and the admin-bootstrap GitHub PR (POSTs files through the
//! Contents API) produce identical bundles — no template drift
//! between paths.
//!
//! Template files live at `sdk/create-oxy-app/templates/<id>/` —
//! one canonical set, consumed by both the server-side scaffold here
//! (baked in at compile time via `include_dir!`) and the
//! `create-oxy-app` CLI (bundled into its npm package). Adding a new
//! template means dropping a directory there with a `template.json` +
//! the file tree; both code paths pick it up. The registry is in
//! `registry.rs`; this module owns rendering.

pub mod registry;

use include_dir::Dir;

pub use registry::{Template, TemplateMeta, get_template, templates};

/// Placeholder substitution applied to UTF-8 template file bodies.
/// Binary files pass through unchanged (none today; the API leaves
/// room).
///
/// `app_base_path` is the served URL prefix — `/customer-apps/<org>/<slug>/`.
/// Now retained only for backwards-compat with templates that still
/// reference `{{OXY_APP_BASE_PATH}}`. The kit-native templates do
/// not — the `@oxy-hq/vite-plugin` derives `base` from
/// `oxy-app.json` at build time, so a baked fallback is unnecessary
/// and the server-time rescue handles bundles built with default
/// base. Field stays so external callers (and any future template
/// that wants the value for some other purpose) don't break.
///
/// `org_slug` and `project_id` are intentionally absent: `oxy-app.json`
/// carries only identity-level fields (`schemaVersion`, `slug`, `name`).
/// The server injects org/project context at serve time, so the same
/// bundle can be linked under any customer without a manifest edit.
pub struct Substitutions<'a> {
    pub app_slug: &'a str,
    pub app_display_name: &'a str,
    /// `/customer-apps/<org_slug>/<app_slug>/`. Trailing slash required;
    /// Vite normalises `base` against it and the rewrite in
    /// `customer_apps_serve` looks for the full slashed prefix.
    pub app_base_path: &'a str,
}

impl Substitutions<'_> {
    pub fn apply(&self, s: &str) -> String {
        s.replace("{{APP_SLUG}}", self.app_slug)
            .replace("{{APP_DISPLAY_NAME}}", self.app_display_name)
            .replace("{{OXY_APP_BASE_PATH}}", self.app_base_path)
    }
}

/// Walk the bundled template identified by `template_id` and emit
/// (relative-path, contents) pairs with placeholders substituted.
/// Used by the admin-triggered GitHub scaffold to POST files through
/// the Contents API.
///
/// Filters out `.example` workflow files — the customer-apps repo
/// has a shared CI workflow at the root, per-app workflows would
/// conflict. Also filters `template.json` (registry metadata) and
/// `screenshot.png` (gallery image) from the emitted bundle.
pub fn render_template_files(
    template_id: &str,
    sub: &Substitutions<'_>,
) -> Result<Vec<(String, String)>, String> {
    let template = registry::get_template(template_id)
        .ok_or_else(|| format!("unknown template_id: {template_id}"))?;
    let mut out = Vec::new();
    collect_template_files(template.dir, sub, "", &mut out);
    // template.json is metadata — never rendered into the bundle.
    out.retain(|(path, _)| path != "template.json");
    // Screenshot is gallery-only — never rendered into the bundle.
    out.retain(|(path, _)| !path.ends_with("screenshot.png"));
    Ok(out)
}

fn collect_template_files(
    dir: &Dir<'_>,
    sub: &Substitutions<'_>,
    rel_prefix: &str,
    out: &mut Vec<(String, String)>,
) {
    for entry in dir.entries() {
        let name = entry
            .path()
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("");
        let rel = if rel_prefix.is_empty() {
            name.to_string()
        } else {
            format!("{rel_prefix}/{name}")
        };
        match entry {
            include_dir::DirEntry::Dir(sub_dir) => {
                collect_template_files(sub_dir, sub, &rel, out);
            }
            include_dir::DirEntry::File(f) => {
                if rel.ends_with(".yml.example") || rel.ends_with(".yaml.example") {
                    continue;
                }
                if let Ok(text) = std::str::from_utf8(f.contents()) {
                    out.push((rel, sub.apply(text)));
                }
                // Binary files (none today): skip rather than corrupt.
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn substitutions_replace_placeholders() {
        let sub = Substitutions {
            app_slug: "my-app",
            app_display_name: "My App",
            app_base_path: "/customer-apps/acme/my-app/",
        };
        assert_eq!(
            sub.apply(
                "name: {{APP_SLUG}}\ntitle: {{APP_DISPLAY_NAME}}\nbase: {{OXY_APP_BASE_PATH}}"
            ),
            "name: my-app\ntitle: My App\nbase: /customer-apps/acme/my-app/",
        );
    }

    #[test]
    fn render_template_files_emits_core_files() {
        let sub = Substitutions {
            app_slug: "test-app",
            app_display_name: "Test App",
            app_base_path: "/customer-apps/acme/test-app/",
        };
        let files = render_template_files("vite", &sub).expect("vite template present");
        let paths: Vec<&str> = files.iter().map(|(p, _)| p.as_str()).collect();
        assert!(paths.contains(&"package.json"), "missing package.json");
        assert!(paths.contains(&"vite.config.ts"), "missing vite.config.ts");
        assert!(paths.contains(&"src/App.tsx"), "missing src/App.tsx");
        assert!(paths.contains(&"oxy-app.json"), "missing oxy-app.json",);
        // template.json must not leak into the bundle output.
        assert!(
            !paths.contains(&"template.json"),
            "template.json must be filtered from bundle output",
        );
    }

    #[test]
    fn render_template_files_substitutes_placeholders() {
        let sub = Substitutions {
            app_slug: "test-app",
            app_display_name: "Test App",
            app_base_path: "/customer-apps/acme/test-app/",
        };
        let files = render_template_files("vite", &sub).expect("vite template present");
        let pkg = files
            .iter()
            .find(|(p, _)| p == "package.json")
            .expect("package.json present");
        assert!(pkg.1.contains("\"name\": \"test-app\""), "got: {}", pkg.1);

        // oxy-app.json is identity-only: schemaVersion + slug + name.
        // org_slug and project_id are injected by the server at serve time.
        let manifest = files
            .iter()
            .find(|(p, _)| p == "oxy-app.json")
            .expect("oxy-app.json present");
        assert!(
            manifest.1.contains("\"slug\": \"test-app\""),
            "slug missing or not substituted: {}",
            manifest.1,
        );
        assert!(
            manifest.1.contains("\"name\": \"Test App\""),
            "name missing or not substituted: {}",
            manifest.1,
        );
        assert!(
            !manifest.1.contains("orgSlug") && !manifest.1.contains("projectId"),
            "org/project identity must not appear in scaffolded manifest: {}",
            manifest.1,
        );
    }

    #[test]
    fn render_template_files_filters_example_automations() {
        let sub = Substitutions {
            app_slug: "x",
            app_display_name: "X",
            app_base_path: "/customer-apps/acme/x/",
        };
        let files = render_template_files("vite", &sub).expect("vite template present");
        for (p, _) in &files {
            assert!(
                !p.ends_with(".yml.example") && !p.ends_with(".yaml.example"),
                "example workflow leaked into scaffold output: {p}",
            );
        }
    }

    #[test]
    fn render_template_files_unknown_id_errors() {
        let sub = Substitutions {
            app_slug: "x",
            app_display_name: "X",
            app_base_path: "/customer-apps/acme/x/",
        };
        assert!(
            render_template_files("does-not-exist", &sub).is_err(),
            "unknown template_id must return Err",
        );
    }

    #[test]
    fn render_dashboard_template_emits_app_tsx_with_use_query() {
        let sub = Substitutions {
            app_slug: "demo",
            app_display_name: "Demo App",
            app_base_path: "/customer-apps/test/demo/",
        };
        let files = render_template_files("dashboard", &sub).expect("render");
        let app_tsx = files
            .iter()
            .find(|(p, _)| p == "src/App.tsx")
            .expect("App.tsx");
        assert!(
            app_tsx.1.contains("useQuery"),
            "dashboard template must call useQuery",
        );
        assert!(
            !app_tsx.1.contains("oxymart"),
            "dashboard template must not reference oxymart",
        );
    }

    #[test]
    fn render_single_store_template_emits_app_tsx_with_use_query() {
        let sub = Substitutions {
            app_slug: "demo",
            app_display_name: "Demo App",
            app_base_path: "/customer-apps/test/demo/",
        };
        let files = render_template_files("single-store", &sub).expect("render");
        let app_tsx = files
            .iter()
            .find(|(p, _)| p == "src/App.tsx")
            .expect("App.tsx");
        assert!(app_tsx.1.contains("useQuery"));
    }
}
