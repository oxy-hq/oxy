//! `oxy-app.json` parsing for the self-serve publish flow.
//!
//! The manifest is identity-first; `build` and `environments` are optional
//! and fall back to convention-over-configuration defaults (Vercel-style),
//! so the common-case manifest stays identity-only. `oxy publish` and
//! `oxy login` both read target resolution from here.

use std::collections::HashMap;
use std::path::Path;

use serde::Deserialize;

const MANIFEST_FILE: &str = "oxy-app.json";

/// Parsed `oxy-app.json`. Unknown fields are ignored (forward-compat).
#[derive(Debug, Default, Deserialize)]
pub struct OxyAppManifest {
    pub slug: Option<String>,
    #[serde(rename = "orgSlug")]
    pub org_slug: Option<String>,
    pub name: Option<String>,
    pub build: Option<BuildSpec>,
    pub environments: Option<HashMap<String, EnvSpec>>,
    /// Optional Oxy Functions shipped in the bundle's `functions/` dir,
    /// keyed by function name. See
    /// `internal-docs/2026-06-12-customer-apps-functions-design.md`.
    pub functions: Option<HashMap<String, FunctionSpec>>,
}

/// Per-function manifest entry. Mirrors `OxyAppFunctionManifest` in the
/// TypeScript SDK; unknown fields ignored for forward-compat.
#[derive(Debug, Default, Deserialize, Clone)]
pub struct FunctionSpec {
    pub entry: Option<String>,
    pub schedule: Option<String>,
    pub timezone: Option<String>,
    pub route: Option<bool>,
    #[serde(rename = "airwayStep")]
    pub airway_step: Option<AirwayStepSpec>,
    #[serde(rename = "timeoutSeconds")]
    pub timeout_seconds: Option<u32>,
}

#[derive(Debug, Default, Deserialize, Clone)]
pub struct AirwayStepSpec {
    pub pipeline: String,
    pub resource: String,
}

impl FunctionSpec {
    /// Source entry path relative to the app dir. Default
    /// `functions/<name>.ts`.
    pub fn entry_for(&self, name: &str) -> String {
        self.entry
            .clone()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| format!("functions/{name}.ts"))
    }
}

#[derive(Debug, Default, Deserialize)]
pub struct BuildSpec {
    pub install: Option<String>,
    pub command: Option<String>,
    #[serde(rename = "outDir")]
    pub out_dir: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct EnvSpec {
    pub target: String,
}

impl OxyAppManifest {
    /// Load `<dir>/oxy-app.json`, or `None` if absent / unparsable. Callers
    /// treat absence as "fall back to flags + defaults".
    pub fn load_from_dir(dir: &Path) -> Option<Self> {
        let raw = std::fs::read_to_string(dir.join(MANIFEST_FILE)).ok()?;
        serde_json::from_str(&raw).ok()
    }

    /// Install command, default `pnpm install`.
    pub fn build_install(&self) -> String {
        self.build
            .as_ref()
            .and_then(|b| b.install.clone())
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| "pnpm install".to_string())
    }

    /// Build command, default `pnpm build`.
    pub fn build_command(&self) -> String {
        self.build
            .as_ref()
            .and_then(|b| b.command.clone())
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| "pnpm build".to_string())
    }

    /// Output directory, default `out` (matches the vite-plugin's forced
    /// `outDir`).
    pub fn build_out_dir(&self) -> String {
        self.build
            .as_ref()
            .and_then(|b| b.out_dir.clone())
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| "out".to_string())
    }
}

/// Built-in target for a well-known environment name. Used when the manifest
/// has no `environments.<env>` entry — keeps the common case zero-config.
/// Overridden by `environments` in the manifest and by `--target`.
pub fn default_target(env: &str) -> Option<&'static str> {
    match env {
        // The web-app Vite dev server (port 5173), NOT oxy's own port (3000):
        // `oxy login` opens `<target>/cli-auth`, a route that only exists in
        // the live web-app — and oxy serves a *pre-built embedded* bundle that
        // may predate it. Vite serves the current route and proxies `/api/*`
        // to oxy on :3000, so build-config / publish / whoami flow through too.
        "local" => Some("http://localhost:5173"),
        "dev" | "development" => Some("https://app-dev.oxygen-hq.com"),
        "staging" => Some("https://app-staging.oxygen-hq.com"),
        "production" | "prod" => Some("https://app.oxygen-hq.com"),
        _ => None,
    }
}

/// Resolve the oxy URL to publish/authenticate against. Precedence:
/// `--target` flag → manifest `environments.<env>.target` → built-in
/// default for `<env>`. Returns `None` if nothing resolves.
pub fn resolve_target(
    manifest: Option<&OxyAppManifest>,
    env: Option<&str>,
    target_flag: Option<&str>,
) -> Option<String> {
    if let Some(t) = target_flag.filter(|s| !s.trim().is_empty()) {
        return Some(t.trim_end_matches('/').to_string());
    }
    let env = env?;
    if let Some(spec) = manifest
        .and_then(|m| m.environments.as_ref())
        .and_then(|envs| envs.get(env))
    {
        return Some(spec.target.trim_end_matches('/').to_string());
    }
    default_target(env).map(|s| s.trim_end_matches('/').to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_defaults_apply_when_absent() {
        let m = OxyAppManifest::default();
        assert_eq!(m.build_install(), "pnpm install");
        assert_eq!(m.build_command(), "pnpm build");
        assert_eq!(m.build_out_dir(), "out");
    }

    #[test]
    fn build_overrides_win() {
        let m: OxyAppManifest = serde_json::from_value(serde_json::json!({
            "slug": "x",
            "build": { "install": "bun install", "command": "bun run build", "outDir": "dist" }
        }))
        .unwrap();
        assert_eq!(m.build_install(), "bun install");
        assert_eq!(m.build_command(), "bun run build");
        assert_eq!(m.build_out_dir(), "dist");
    }

    #[test]
    fn target_precedence_flag_over_manifest_over_default() {
        let m: OxyAppManifest = serde_json::from_value(serde_json::json!({
            "slug": "x",
            "environments": { "dev": { "target": "https://custom.example.com/" } }
        }))
        .unwrap();
        // flag wins
        assert_eq!(
            resolve_target(Some(&m), Some("dev"), Some("https://flag.example.com")).as_deref(),
            Some("https://flag.example.com")
        );
        // manifest wins over built-in default (and trailing slash trimmed)
        assert_eq!(
            resolve_target(Some(&m), Some("dev"), None).as_deref(),
            Some("https://custom.example.com")
        );
        // built-in default when manifest has no entry
        assert_eq!(
            resolve_target(Some(&m), Some("local"), None).as_deref(),
            Some("http://localhost:5173")
        );
        // unknown env, no manifest, no flag → None
        assert_eq!(resolve_target(None, Some("bogus"), None), None);
    }

    #[test]
    fn builtin_targets_for_known_environments() {
        assert_eq!(default_target("local"), Some("http://localhost:5173"));
        assert_eq!(default_target("dev"), Some("https://app-dev.oxygen-hq.com"));
        assert_eq!(
            default_target("staging"),
            Some("https://app-staging.oxygen-hq.com")
        );
        assert_eq!(
            default_target("production"),
            Some("https://app.oxygen-hq.com")
        );
        assert_eq!(default_target("prod"), Some("https://app.oxygen-hq.com"));
        assert_eq!(default_target("bogus"), None);
    }
}
